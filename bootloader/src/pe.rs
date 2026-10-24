//! Minimal PE32+ image loader for the fixed-base RdotOS kernel image.
//! The bootloader allocates physical section backing and later maps each PE RVA
//! at its high-half virtual address. Dynamic relocations and imports are rejected.
use uefi::boot::{self, AllocateType, MemoryType, PAGE_SIZE};
use crate::elf::{LoadSegment, LoadedKernel};

const IMAGE_BASE: u64 = 0xffff_ffff_7ff0_0000;
const PE_HEADER_MIN: usize = 24;
const SECTION_HEADER_SIZE: usize = 40;
const MAX_SECTIONS: usize = 128;
const IMAGE_SCN_MEM_EXECUTE: u32 = 0x2000_0000;
const IMAGE_SCN_MEM_READ: u32 = 0x4000_0000;
const IMAGE_SCN_MEM_WRITE: u32 = 0x8000_0000;

#[derive(Debug)]
pub enum PeError { Truncated, BadMagic, UnsupportedImage, BadHeader, BadSection, AllocationFailed }

fn u16le(b: &[u8], o: usize) -> Option<u16> { Some(u16::from_le_bytes(b.get(o..o+2)?.try_into().ok()?)) }
fn u32le(b: &[u8], o: usize) -> Option<u32> { Some(u32::from_le_bytes(b.get(o..o+4)?.try_into().ok()?)) }
fn u64le(b: &[u8], o: usize) -> Option<u64> { Some(u64::from_le_bytes(b.get(o..o+8)?.try_into().ok()?)) }
fn range_ok(off: u32, len: u32, total: usize) -> bool {
    (off as usize).checked_add(len as usize).is_some_and(|end| end <= total)
}

/// Validate and load the PE32+ kernel sections into separate physical backing.
pub unsafe fn load(image: &[u8]) -> Result<LoadedKernel, PeError> {
    if image.len() < 0x40 { return Err(PeError::Truncated); }
    if image.get(0..2) != Some(b"MZ") { return Err(PeError::BadMagic); }
    let peoff = u32le(image, 0x3c).ok_or(PeError::BadHeader)? as usize;
    if peoff.checked_add(PE_HEADER_MIN).is_none_or(|end| end > image.len())
        || image.get(peoff..peoff+4) != Some(b"PE\0\0") {
        return Err(PeError::BadHeader);
    }
    let coff = peoff + 4;
    let machine = u16le(image, coff).ok_or(PeError::Truncated)?;
    let section_count = u16le(image, coff+2).ok_or(PeError::Truncated)? as usize;
    let optional_size = u16le(image, coff+16).ok_or(PeError::Truncated)? as usize;
    let characteristics = u16le(image, coff+18).ok_or(PeError::Truncated)?;
    if machine != 0x8664 || section_count == 0 || section_count > MAX_SECTIONS
        || optional_size < 240 || characteristics & 0x0002 == 0 {
        return Err(PeError::UnsupportedImage);
    }
    let opt = coff + 20;
    if opt.checked_add(optional_size).is_none_or(|end| end > image.len())
        || u16le(image, opt) != Some(0x20b)
        || u64le(image, opt+24) != Some(IMAGE_BASE)
        || u32le(image, opt+32) != Some(0x1000)
        || u32le(image, opt+36) != Some(0x200)
        || u16le(image, opt+68) != Some(1) {
        return Err(PeError::UnsupportedImage);
    }
    let entry_rva = u32le(image, opt+16).ok_or(PeError::Truncated)? as u64;
    let size_headers = u32le(image, opt+60).ok_or(PeError::Truncated)? as usize;
    if size_headers > image.len() || size_headers < opt + optional_size + section_count*SECTION_HEADER_SIZE {
        return Err(PeError::BadHeader);
    }
    // This fixed-base kernel deliberately has no relocation directory. Reject
    // imports/relocations/TLS rather than silently accepting unsupported images.
    let directory_count = u32le(image, opt+108).ok_or(PeError::Truncated)?.min(16) as usize;
    for index in [1usize, 5, 9] { // imports, base relocations, TLS
        if index < directory_count {
            let dir = opt + 112 + index*8;
            if u32le(image, dir).unwrap_or(0) != 0 || u32le(image, dir+4).unwrap_or(0) != 0 {
                return Err(PeError::UnsupportedImage);
            }
        }
    }

    let section_table = opt + optional_size;
    let mut segments = [LoadSegment { virtual_start: 0, physical_start: 0, memory_size: 0, flags: 0 }; MAX_SECTIONS];
    let mut count = 0usize;
    let mut entry_ok = false;
    for i in 0..section_count {
        let o = section_table + i*SECTION_HEADER_SIZE;
        let virtual_size = u32le(image, o+8).ok_or(PeError::Truncated)? as u64;
        let rva = u32le(image, o+12).ok_or(PeError::Truncated)? as u64;
        let raw_size = u32le(image, o+16).ok_or(PeError::Truncated)?;
        let raw_offset = u32le(image, o+20).ok_or(PeError::Truncated)?;
        let flags = u32le(image, o+36).ok_or(PeError::Truncated)?;
        if virtual_size == 0 || rva & 0xfff != 0 || rva.checked_add(virtual_size).is_none()
            || raw_size != 0 && (!range_ok(raw_offset, raw_size, image.len()) || raw_offset < size_headers as u32)
            || raw_size as u64 > virtual_size + 0x1ff
            || flags & IMAGE_SCN_MEM_READ == 0
            || flags & IMAGE_SCN_MEM_WRITE != 0 && flags & IMAGE_SCN_MEM_EXECUTE != 0 {
            return Err(PeError::BadSection);
        }
        let virtual_start = IMAGE_BASE.checked_add(rva).ok_or(PeError::BadSection)?;
        let end = virtual_start.checked_add(virtual_size).ok_or(PeError::BadSection)?;
        for previous in segments[..count].iter() {
            let prev_end = previous.virtual_start.checked_add(previous.memory_size).ok_or(PeError::BadSection)?;
            if virtual_start < prev_end && previous.virtual_start < end { return Err(PeError::BadSection); }
        }
        let pages = virtual_size.checked_add(PAGE_SIZE as u64 - 1).ok_or(PeError::BadSection)? / PAGE_SIZE as u64;
        let allocation = unsafe {
            boot::allocate_pages(AllocateType::MaxAddress(0xffff_ffff), MemoryType::LOADER_DATA, pages as usize)
        }.map_err(|_| PeError::AllocationFailed)?;
        let physical = allocation.as_ptr() as u64;
        if physical & (PAGE_SIZE as u64 - 1) != 0
            || physical.checked_add(pages*PAGE_SIZE as u64).is_none_or(|end| end > 0x1_0000_0000) {
            return Err(PeError::AllocationFailed);
        }
        unsafe { core::ptr::write_bytes(physical as *mut u8, 0, (pages*PAGE_SIZE as u64) as usize); }
        let copy_len = (raw_size as u64).min(virtual_size) as usize;
        if copy_len != 0 {
            unsafe { core::ptr::copy_nonoverlapping(image.as_ptr().add(raw_offset as usize), physical as *mut u8, copy_len); }
        }
        let mut elf_flags = 4; // PF_R
        if flags & IMAGE_SCN_MEM_WRITE != 0 { elf_flags |= 2; }
        if flags & IMAGE_SCN_MEM_EXECUTE != 0 { elf_flags |= 1; }
        if entry_rva >= rva && entry_rva < rva + virtual_size && elf_flags & 1 != 0 { entry_ok = true; }
        segments[count] = LoadSegment { virtual_start, physical_start: physical, memory_size: virtual_size, flags: elf_flags };
        count += 1;
    }
    if !entry_ok { return Err(PeError::BadSection); }
    let entry = IMAGE_BASE.checked_add(entry_rva).ok_or(PeError::BadHeader)?;
    Ok(LoadedKernel { entry, segments, segment_count: count })
}
