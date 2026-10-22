//! Minimal, defensive ELF64 executable loader for the early x86-64 boot path.
//! Supports little-endian ET_EXEC images with identity-mapped PT_LOAD segments.
//! Deliberately rejects dynamic relocations, interpreters, and high-half images.
use uefi::boot::{self, AllocateType, MemoryType, PAGE_SIZE};

const ELF_HEADER_SIZE: usize = 64;
const PROGRAM_HEADER_SIZE: usize = 56;
const PT_LOAD: u32 = 1;
const ET_EXEC: u16 = 2;
const EM_X86_64: u16 = 62;
const MAX_KERNEL_END: u64 = 0x4000_0000; // 1 GiB identity-mapped bootstrap ceiling.

#[derive(Debug)]
pub enum ElfError {
    Truncated,
    BadMagic,
    UnsupportedImage,
    BadHeader,
    BadSegment,
    AllocationFailed,
}

#[derive(Clone, Copy)]
struct ProgramHeader {
    kind: u32,
    flags: u32,
    offset: u64,
    vaddr: u64,
    paddr: u64,
    filesz: u64,
    memsz: u64,
    align: u64,
}

fn u16le(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(o..o + 2)?.try_into().ok()?))
}
fn u32le(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}
fn u64le(b: &[u8], o: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(o..o + 8)?.try_into().ok()?))
}
fn range_ok(offset: u64, len: u64, total: usize) -> bool {
    offset.checked_add(len).is_some_and(|end| end <= total as u64)
}

/// Validate and load PT_LOAD segments. Returns the executable entry address.
/// All destinations are allocated at their ELF physical addresses, and the
/// early kernel is linked for identity mapping. No relocation is performed.
pub unsafe fn load(image: &[u8]) -> Result<u64, ElfError> {
    if image.len() < ELF_HEADER_SIZE { return Err(ElfError::Truncated); }
    if image.get(0..4) != Some(b"\x7fELF") { return Err(ElfError::BadMagic); }
    if image[4] != 2 || image[5] != 1 || image[6] != 1 {
        return Err(ElfError::UnsupportedImage);
    }
    if u16le(image, 16) != Some(ET_EXEC)
        || u16le(image, 18) != Some(EM_X86_64)
        || u32le(image, 20) != Some(1)
    { return Err(ElfError::UnsupportedImage); }

    let entry = u64le(image, 24).ok_or(ElfError::BadHeader)?;
    let phoff = u64le(image, 32).ok_or(ElfError::BadHeader)?;
    let ehsize = u16le(image, 52).ok_or(ElfError::BadHeader)? as usize;
    let phentsize = u16le(image, 54).ok_or(ElfError::BadHeader)? as usize;
    let phnum = u16le(image, 56).ok_or(ElfError::BadHeader)? as usize;
    if ehsize != ELF_HEADER_SIZE || phentsize != PROGRAM_HEADER_SIZE || phnum == 0 || phnum > 128 {
        return Err(ElfError::BadHeader);
    }
    let table_size = phentsize.checked_mul(phnum).ok_or(ElfError::BadHeader)? as u64;
    if !range_ok(phoff, table_size, image.len()) { return Err(ElfError::Truncated); }

    let mut entry_is_loaded = false;
    let mut loaded_any = false;
    for i in 0..phnum {
        let o = (phoff as usize).checked_add(i * phentsize).ok_or(ElfError::BadHeader)?;
        let p = ProgramHeader {
            kind: u32le(image, o).ok_or(ElfError::Truncated)?,
            flags: u32le(image, o + 4).ok_or(ElfError::Truncated)?,
            offset: u64le(image, o + 8).ok_or(ElfError::Truncated)?,
            vaddr: u64le(image, o + 16).ok_or(ElfError::Truncated)?,
            paddr: u64le(image, o + 24).ok_or(ElfError::Truncated)?,
            filesz: u64le(image, o + 32).ok_or(ElfError::Truncated)?,
            memsz: u64le(image, o + 40).ok_or(ElfError::Truncated)?,
            align: u64le(image, o + 48).ok_or(ElfError::Truncated)?,
        };
        if p.kind != PT_LOAD { continue; }
        if p.memsz == 0 || p.filesz > p.memsz || !range_ok(p.offset, p.filesz, image.len()) {
            return Err(ElfError::BadSegment);
        }
        let dest = if p.paddr != 0 { p.paddr } else { p.vaddr };
        let end = dest.checked_add(p.memsz).ok_or(ElfError::BadSegment)?;
        if dest < 0x10_0000 || end > MAX_KERNEL_END || end <= dest || (p.align > 1 && !p.align.is_power_of_two()) {
            return Err(ElfError::BadSegment);
        }
        if p.align > 1 && (p.vaddr.wrapping_sub(p.offset) & (p.align - 1)) != 0 {
            return Err(ElfError::BadSegment);
        }
        let first_page = dest & !((PAGE_SIZE as u64) - 1);
        let last_page = end.checked_add(PAGE_SIZE as u64 - 1).ok_or(ElfError::BadSegment)?
            & !((PAGE_SIZE as u64) - 1);
        let pages = ((last_page - first_page) / PAGE_SIZE as u64) as usize;
        // SAFETY: destination range was checked and is requested as loader-owned memory.
        let allocated = unsafe {
            boot::allocate_pages(AllocateType::Address(first_page), MemoryType::LOADER_DATA, pages)
        }.map_err(|_| ElfError::AllocationFailed)?;
        if allocated.as_ptr() as u64 != first_page {
            return Err(ElfError::AllocationFailed);
        }
        // SAFETY: the allocated pages cover [first_page, last_page), and source
        // bounds were checked above. Clear BSS and copy only validated file bytes.
        unsafe {
            core::ptr::write_bytes(first_page as *mut u8, 0, (last_page - first_page) as usize);
            core::ptr::copy_nonoverlapping(
                image.as_ptr().add(p.offset as usize),
                dest as *mut u8,
                p.filesz as usize,
            );
        }
        loaded_any = true;
        if (p.flags & 0x1) != 0 && entry >= dest && entry < end { entry_is_loaded = true; }
    }
    if !loaded_any || !entry_is_loaded { return Err(ElfError::BadSegment); }
    Ok(entry)
}
