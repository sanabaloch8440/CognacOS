//! Strict loader for RdotOS's fixed-base PE32+ native kernel image.
//! The image is not an EFI application: it is a static AMD64 native-subsystem
//! PE image with no imports or base relocations. The loader allocates physical
//! section backing below 4 GiB and maps each section at ImageBase + RVA.
use uefi::boot::{self, AllocateType, MemoryType, PAGE_SIZE};

const IMAGE_BASE: u64 = 0xffff_ffff_7ff0_0000;
const PE_HEADER_LIMIT: usize = 4096;
const MAX_SECTIONS: usize = 128;
const IMAGE_FILE_MACHINE_AMD64: u16 = 0x8664;
const PE32_PLUS_MAGIC: u16 = 0x20b;
const IMAGE_SUBSYSTEM_NATIVE: u16 = 1;
const IMAGE_SCN_MEM_EXECUTE: u32 = 0x2000_0000;
const IMAGE_SCN_MEM_READ: u32 = 0x4000_0000;
const IMAGE_SCN_MEM_WRITE: u32 = 0x8000_0000;
const IMAGE_SCN_MEM_DISCARDABLE: u32 = 0x0200_0000;
const FOUR_GIB: u64 = 0x1_0000_0000;

#[derive(Debug)]
pub enum PeError {
    Truncated,
    BadMagic,
    UnsupportedImage,
    BadHeader,
    BadSection,
    AllocationFailed,
}

#[derive(Clone, Copy)]
pub struct LoadSegment {
    pub virtual_start: u64,
    pub physical_start: u64,
    pub memory_size: u64,
    /// ELF-compatible permission bits: execute=1, write=2, read=4.
    pub flags: u32,
}
impl LoadSegment {
    const EMPTY: Self = Self { virtual_start: 0, physical_start: 0, memory_size: 0, flags: 0 };
}

pub struct LoadedKernel {
    pub entry: u64,
    pub segments: [LoadSegment; MAX_SECTIONS],
    pub segment_count: usize,
}

fn u16le(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(o..o.checked_add(2)?)?.try_into().ok()?))
}
fn u32le(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o.checked_add(4)?)?.try_into().ok()?))
}
fn u64le(b: &[u8], o: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(o..o.checked_add(8)?)?.try_into().ok()?))
}
fn range_ok(offset: u64, len: u64, total: usize) -> bool {
    offset.checked_add(len).is_some_and(|end| end <= total as u64)
}
fn canonical(address: u64) -> bool {
    let high = address >> 48;
    let sign = (address >> 47) & 1;
    (sign == 0 && high == 0) || (sign == 1 && high == 0xffff)
}

/// Validate a fixed-base PE32+ image and load each section into low physical
/// pages. Relocations/imports are deliberately rejected rather than ignored.
pub unsafe fn load(image: &[u8]) -> Result<LoadedKernel, PeError> {
    if image.len() < 0x40 { return Err(PeError::Truncated); }
    if image.get(0..2) != Some(b"MZ") { return Err(PeError::BadMagic); }
    let peoff = u32le(image, 0x3c).ok_or(PeError::BadHeader)? as usize;
    if peoff > image.len().saturating_sub(24) || image.get(peoff..peoff + 4) != Some(b"PE\0\0") {
        return Err(PeError::BadHeader);
    }
    let coff = peoff + 4;
    let machine = u16le(image, coff).ok_or(PeError::BadHeader)?;
    let section_count = u16le(image, coff + 2).ok_or(PeError::BadHeader)? as usize;
    let optional_size = u16le(image, coff + 16).ok_or(PeError::BadHeader)? as usize;
    let characteristics = u16le(image, coff + 18).ok_or(PeError::BadHeader)?;
    if machine != IMAGE_FILE_MACHINE_AMD64 || section_count == 0 || section_count > MAX_SECTIONS
        || optional_size < 240 || characteristics & 0x0002 == 0
    { return Err(PeError::UnsupportedImage); }
    let optional = coff.checked_add(20).ok_or(PeError::BadHeader)?;
    let section_table = optional.checked_add(optional_size).ok_or(PeError::BadHeader)?;
    let section_table_end = section_table.checked_add(section_count.checked_mul(40).ok_or(PeError::BadHeader)?)
        .ok_or(PeError::BadHeader)?;
    if section_table_end > image.len() || optional_size < 240 { return Err(PeError::Truncated); }
    if u16le(image, optional) != Some(PE32_PLUS_MAGIC) { return Err(PeError::UnsupportedImage); }

    let entry_rva = u32le(image, optional + 16).ok_or(PeError::BadHeader)? as u64;
    let image_base = u64le(image, optional + 24).ok_or(PeError::BadHeader)?;
    let section_alignment = u32le(image, optional + 32).ok_or(PeError::BadHeader)? as u64;
    let file_alignment = u32le(image, optional + 36).ok_or(PeError::BadHeader)? as u64;
    let size_image = u32le(image, optional + 56).ok_or(PeError::BadHeader)? as u64;
    let size_headers = u32le(image, optional + 60).ok_or(PeError::BadHeader)? as u64;
    let subsystem = u16le(image, optional + 68).ok_or(PeError::BadHeader)?;
    let dll_characteristics = u16le(image, optional + 70).ok_or(PeError::BadHeader)?;
    let directory_count = u32le(image, optional + 108).ok_or(PeError::BadHeader)? as usize;
    if image_base != IMAGE_BASE || !canonical(image_base)
        || section_alignment != 4096 || file_alignment != 512
        || size_image == 0 || size_image > 0x4000_0000
        || size_headers < section_table_end as u64 || size_headers > image.len() as u64
        || subsystem != IMAGE_SUBSYSTEM_NATIVE || dll_characteristics & 0x0100 == 0
        || directory_count > 16
    { return Err(PeError::UnsupportedImage); }

    // Fixed-base kernel policy: imports and relocations must be absent.
    if directory_count > 1 {
        let import_rva = u32le(image, optional + 112 + 8).ok_or(PeError::BadHeader)?;
        let import_size = u32le(image, optional + 112 + 12).ok_or(PeError::BadHeader)?;
        if import_rva != 0 || import_size != 0 { return Err(PeError::UnsupportedImage); }
    }
    if directory_count > 5 {
        let reloc_rva = u32le(image, optional + 112 + 5 * 8).ok_or(PeError::BadHeader)?;
        let reloc_size = u32le(image, optional + 112 + 5 * 8 + 4).ok_or(PeError::BadHeader)?;
        if reloc_rva != 0 || reloc_size != 0 { return Err(PeError::UnsupportedImage); }
    }

    #[derive(Clone, Copy)]
    struct Section {
        rva: u64, virtual_size: u64, raw_offset: u64, raw_size: u64, flags: u32,
    }
    const EMPTY: Section = Section { rva: 0, virtual_size: 0, raw_offset: 0, raw_size: 0, flags: 0 };
    let mut sections = [EMPTY; MAX_SECTIONS];
    let mut entry_is_executable = false;
    for i in 0..section_count {
        let off = section_table + i * 40;
        let virtual_size = u32le(image, off + 8).ok_or(PeError::BadSection)? as u64;
        let rva = u32le(image, off + 12).ok_or(PeError::BadSection)? as u64;
        let raw_size = u32le(image, off + 16).ok_or(PeError::BadSection)? as u64;
        let raw_offset = u32le(image, off + 20).ok_or(PeError::BadSection)? as u64;
        let flags = u32le(image, off + 36).ok_or(PeError::BadSection)?;
        if virtual_size == 0 || rva == 0 || rva % section_alignment != 0
            || rva.checked_add(virtual_size).is_none_or(|end| end > size_image)
            || raw_size > 0 && (raw_offset < size_headers || raw_offset % file_alignment != 0
                || raw_size % file_alignment != 0 || !range_ok(raw_offset, raw_size, image.len()))
            || flags & IMAGE_SCN_MEM_READ == 0
            || flags & IMAGE_SCN_MEM_DISCARDABLE != 0
            || flags & (IMAGE_SCN_MEM_WRITE | IMAGE_SCN_MEM_EXECUTE)
                == (IMAGE_SCN_MEM_WRITE | IMAGE_SCN_MEM_EXECUTE)
        { return Err(PeError::BadSection); }
        if flags & IMAGE_SCN_MEM_EXECUTE != 0 && entry_rva >= rva && entry_rva < rva + virtual_size {
            entry_is_executable = true;
        }
        for old in sections[..i].iter() {
            let old_end = old.rva.checked_add(old.virtual_size).ok_or(PeError::BadSection)?;
            if rva < old_end && old.rva < rva + virtual_size { return Err(PeError::BadSection); }
        }
        sections[i] = Section { rva, virtual_size, raw_offset, raw_size, flags };
    }
    if !entry_is_executable { return Err(PeError::BadSection); }
    let entry = image_base.checked_add(entry_rva).ok_or(PeError::BadHeader)?;
    if !canonical(entry) { return Err(PeError::BadHeader); }

    let mut loaded = [LoadSegment::EMPTY; MAX_SECTIONS];
    for (i, section) in sections[..section_count].iter().copied().enumerate() {
        let virtual_start = image_base.checked_add(section.rva).ok_or(PeError::BadSection)?;
        let virtual_end = virtual_start.checked_add(section.virtual_size).ok_or(PeError::BadSection)?;
        if !canonical(virtual_start) || !canonical(virtual_end - 1) {
            return Err(PeError::BadSection);
        }
        let pages = section.virtual_size.checked_add(PAGE_SIZE as u64 - 1)
            .ok_or(PeError::BadSection)? / PAGE_SIZE as u64;
        let allocation = unsafe {
            boot::allocate_pages(
                AllocateType::MaxAddress(0xffff_ffff),
                MemoryType::LOADER_DATA,
                pages as usize,
            )
        }.map_err(|_| PeError::AllocationFailed)?;
        let physical_start = allocation.as_ptr() as u64;
        let allocated_bytes = pages.checked_mul(PAGE_SIZE as u64).ok_or(PeError::BadSection)?;
        if physical_start == 0 || physical_start.checked_add(allocated_bytes).is_none_or(|end| end > FOUR_GIB) {
            return Err(PeError::AllocationFailed);
        }
        unsafe { core::ptr::write_bytes(physical_start as *mut u8, 0, allocated_bytes as usize); }
        let copy_len = core::cmp::min(section.raw_size, section.virtual_size) as usize;
        if copy_len != 0 {
            unsafe {
                core::ptr::copy_nonoverlapping(
                    image.as_ptr().add(section.raw_offset as usize),
                    physical_start as *mut u8,
                    copy_len,
                );
            }
        }
        let flags = (if section.flags & IMAGE_SCN_MEM_EXECUTE != 0 { 1 } else { 0 })
            | (if section.flags & IMAGE_SCN_MEM_WRITE != 0 { 2 } else { 0 })
            | (if section.flags & IMAGE_SCN_MEM_READ != 0 { 4 } else { 0 });
        loaded[i] = LoadSegment { virtual_start, physical_start, memory_size: section.virtual_size, flags };
    }
    Ok(LoadedKernel { entry, segments: loaded, segment_count: section_count })
}
