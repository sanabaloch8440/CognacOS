//! Bounded PE32+ x86-64 user-image loader.
//! Supports mapped headers/sections, DIR64 base relocations and import binding
//! through a kernel-provided resolver. TLS callbacks, delay imports, bound imports,
//! load-config/CFG, unwind registration and API-set resolution are not yet supported.
use crate::vm::{AddressSpace, VmError, is_user_range};

const PAGE_SIZE: u64 = 4096;
const MAX_IMAGE_SIZE: usize = 8 * 1024 * 1024;
const MAX_IMAGE_PAGES: usize = MAX_IMAGE_SIZE / PAGE_SIZE as usize;
const MAX_SECTIONS: usize = 96;
const IMAGE_REL_BASED_ABSOLUTE: u16 = 0;
const IMAGE_REL_BASED_DIR64: u16 = 10;
const IMAGE_ORDINAL_FLAG64: u64 = 1 << 63;
const IMAGE_SCN_MEM_WRITE: u32 = 0x8000_0000;
const IMAGE_SCN_MEM_EXECUTE: u32 = 0x2000_0000;
const USER_IMAGE_FALLBACK: u64 = 0x0000_2000_0000_0000;
const USER_END: u64 = 0x0000_8000_0000_0000;

#[derive(Clone, Copy, Debug)]
pub enum PeError {
    Truncated, BadDosHeader, BadSignature, UnsupportedMachine, UnsupportedOptionalHeader,
    InvalidHeader, InvalidSection, ImageTooLarge, AddressUnavailable, OutOfMemory,
    InvalidRelocations, RelocationsRequired, InvalidImports, ImportResolverMissing,
    UnresolvedImport, WxPageRejected, Vm(VmError),
}
impl From<VmError> for PeError { fn from(e: VmError) -> Self { Self::Vm(e) } }

#[derive(Clone, Copy)]
struct Section { virtual_size: u32, virtual_address: u32, raw_size: u32, raw_offset: u32, characteristics: u32 }
#[derive(Clone, Copy)]
pub enum ImportSymbol<'a> { Name(&'a [u8]), Ordinal(u16) }
pub type ImportResolver = for<'a> fn(dll: &[u8], symbol: ImportSymbol<'a>) -> Option<u64>;

#[derive(Clone, Copy, Debug)]
pub struct LoadedImage { pub base: u64, pub entry: u64, pub image_size: u32 }

fn u16at(b: &[u8], o: usize) -> Result<u16, PeError> {
    Ok(u16::from_le_bytes(b.get(o..o.checked_add(2).ok_or(PeError::InvalidHeader)?)
        .ok_or(PeError::Truncated)?.try_into().map_err(|_| PeError::Truncated)?))
}
fn u32at(b: &[u8], o: usize) -> Result<u32, PeError> {
    Ok(u32::from_le_bytes(b.get(o..o.checked_add(4).ok_or(PeError::InvalidHeader)?)
        .ok_or(PeError::Truncated)?.try_into().map_err(|_| PeError::Truncated)?))
}
fn u64at(b: &[u8], o: usize) -> Result<u64, PeError> {
    Ok(u64::from_le_bytes(b.get(o..o.checked_add(8).ok_or(PeError::InvalidHeader)?)
        .ok_or(PeError::Truncated)?.try_into().map_err(|_| PeError::Truncated)?))
}
fn file_range(bytes: &[u8], offset: u32, size: u32) -> Result<&[u8], PeError> {
    let start = offset as usize;
    let end = start.checked_add(size as usize).ok_or(PeError::InvalidHeader)?;
    bytes.get(start..end).ok_or(PeError::Truncated)
}
fn align_up(v: u64, a: u64) -> Option<u64> { v.checked_add(a - 1).map(|n| n & !(a - 1)) }

fn read_image(space: &AddressSpace, base: u64, rva: u32, out: &mut [u8]) -> Result<(), PeError> {
    let address = base.checked_add(rva as u64).ok_or(PeError::InvalidHeader)?;
    unsafe { space.copy_from_user(address, out)?; }
    Ok(())
}
fn write_image(space: &AddressSpace, base: u64, rva: u32, data: &[u8]) -> Result<(), PeError> {
    let address = base.checked_add(rva as u64).ok_or(PeError::InvalidHeader)?;
    unsafe { space.copy_to_user(address, data)?; }
    Ok(())
}
fn read_image_u16(space: &AddressSpace, base: u64, rva: u32) -> Result<u16, PeError> {
    let mut b = [0; 2]; read_image(space, base, rva, &mut b)?; Ok(u16::from_le_bytes(b))
}
fn read_image_u32(space: &AddressSpace, base: u64, rva: u32) -> Result<u32, PeError> {
    let mut b = [0; 4]; read_image(space, base, rva, &mut b)?; Ok(u32::from_le_bytes(b))
}
fn read_image_u64(space: &AddressSpace, base: u64, rva: u32) -> Result<u64, PeError> {
    let mut b = [0; 8]; read_image(space, base, rva, &mut b)?; Ok(u64::from_le_bytes(b))
}
fn write_image_u64(space: &AddressSpace, base: u64, rva: u32, value: u64) -> Result<(), PeError> {
    write_image(space, base, rva, &value.to_le_bytes())
}
fn read_c_string(space: &AddressSpace, base: u64, rva: u32) -> Result<([u8; 260], usize), PeError> {
    let mut out = [0u8; 260];
    for i in 0..out.len() {
        let at = rva.checked_add(i as u32).ok_or(PeError::InvalidImports)?;
        read_image(space, base, at, &mut out[i..i + 1])?;
        if out[i] == 0 { return Ok((out, i)); }
        if !out[i].is_ascii() { return Err(PeError::InvalidImports); }
    }
    Err(PeError::InvalidImports)
}
fn available_base(space: &AddressSpace, base: u64, pages: usize) -> bool {
    for i in 0..pages {
        let Some(va) = base.checked_add(i as u64 * PAGE_SIZE) else { return false; };
        if space.physical_for_user(va).is_ok() { return false; }
    }
    true
}

/// Load a Windows PE32+ AMD64 image into an isolated address space.
///
/// Image pages are first mapped RW/NX, populated, relocated, and import-bound;
/// final per-page permissions are then applied with W^X enforced. The caller
/// must supply a resolver for every imported symbol. No user code is executed here.
pub unsafe fn load(
    space: &mut AddressSpace,
    file: &[u8],
    resolver: Option<ImportResolver>,
) -> Result<LoadedImage, PeError> {
    if file.len() < 0x40 { return Err(PeError::Truncated); }
    if u16at(file, 0)? != 0x5a4d { return Err(PeError::BadDosHeader); }
    let nt = u32at(file, 0x3c)? as usize;
    if file.get(nt..nt.checked_add(4).ok_or(PeError::InvalidHeader)?)
        .ok_or(PeError::Truncated)? != b"PE\0\0" { return Err(PeError::BadSignature); }
    let coff = nt.checked_add(4).ok_or(PeError::InvalidHeader)?;
    if u16at(file, coff)? != 0x8664 { return Err(PeError::UnsupportedMachine); }
    let section_count = u16at(file, coff + 2)? as usize;
    let optional_size = u16at(file, coff + 16)? as usize;
    if section_count == 0 || section_count > MAX_SECTIONS || optional_size < 240 {
        return Err(PeError::InvalidHeader);
    }
    let opt = coff.checked_add(20).ok_or(PeError::InvalidHeader)?;
    if u16at(file, opt)? != 0x20b { return Err(PeError::UnsupportedOptionalHeader); }
    let entry_rva = u32at(file, opt + 16)?;
    let preferred_base = u64at(file, opt + 24)?;
    let section_alignment = u32at(file, opt + 32)? as u64;
    let file_alignment = u32at(file, opt + 36)? as u64;
    let image_size = u32at(file, opt + 56)?;
    let headers_size = u32at(file, opt + 60)?;
    let directory_count = core::cmp::min(u32at(file, opt + 108)? as usize, 16);
    if section_alignment < PAGE_SIZE || !section_alignment.is_power_of_two()
        || file_alignment == 0 || !file_alignment.is_power_of_two()
        || image_size == 0 || image_size as usize > MAX_IMAGE_SIZE
        || headers_size == 0 || headers_size > image_size
        || headers_size as usize > file.len()
        || entry_rva >= image_size
    { return Err(PeError::InvalidHeader); }
    let page_count = (align_up(image_size as u64, PAGE_SIZE).ok_or(PeError::InvalidHeader)? / PAGE_SIZE) as usize;
    if page_count == 0 || page_count > MAX_IMAGE_PAGES { return Err(PeError::ImageTooLarge); }

    let section_table = opt.checked_add(optional_size).ok_or(PeError::InvalidHeader)?;
    let section_bytes = section_count.checked_mul(40).ok_or(PeError::InvalidHeader)?;
    if section_table.checked_add(section_bytes).filter(|&end| end <= file.len()).is_none() {
        return Err(PeError::Truncated);
    }
    let mut sections = [Section { virtual_size: 0, virtual_address: 0, raw_size: 0, raw_offset: 0, characteristics: 0 }; MAX_SECTIONS];
    let mut entry_is_executable = false;
    for i in 0..section_count {
        let o = section_table + i * 40;
        let s = Section { virtual_size: u32at(file, o + 8)?, virtual_address: u32at(file, o + 12)?,
            raw_size: u32at(file, o + 16)?, raw_offset: u32at(file, o + 20)?,
            characteristics: u32at(file, o + 36)? };
        let span = core::cmp::max(s.virtual_size, s.raw_size);
        let end = s.virtual_address.checked_add(span).ok_or(PeError::InvalidSection)?;
        if span != 0 && (s.virtual_address == 0 || end > image_size
            || s.virtual_address as u64 % section_alignment != 0) {
            return Err(PeError::InvalidSection);
        }
        if s.raw_size != 0 {
            if s.raw_offset as u64 % file_alignment != 0 || s.raw_size as usize > file.len() {
                return Err(PeError::InvalidSection);
            }
            file_range(file, s.raw_offset, s.raw_size)?;
        }
        if (s.characteristics & IMAGE_SCN_MEM_EXECUTE) != 0
            && entry_rva >= s.virtual_address && (entry_rva as u64) < s.virtual_address as u64 + span as u64 {
            entry_is_executable = true;
        }
        for old in sections[..i].iter() {
            let old_span = core::cmp::max(old.virtual_size, old.raw_size);
            if span != 0 && old_span != 0 {
                let a0 = s.virtual_address as u64; let a1 = end as u64;
                let b0 = old.virtual_address as u64; let b1 = b0 + old_span as u64;
                if a0 < b1 && b0 < a1 { return Err(PeError::InvalidSection); }
            }
        }
        sections[i] = s;
    }
    if !entry_is_executable { return Err(PeError::InvalidHeader); }

    let image_base = if preferred_base >= 0x0000_0080_0000_0000
        && preferred_base.checked_add(page_count as u64 * PAGE_SIZE).is_some_and(|e| e <= USER_END)
        && available_base(space, preferred_base, page_count) {
        preferred_base
    } else if available_base(space, USER_IMAGE_FALLBACK, page_count) {
        USER_IMAGE_FALLBACK
    } else { return Err(PeError::AddressUnavailable); };
    if !is_user_range(image_base, page_count as u64 * PAGE_SIZE) { return Err(PeError::AddressUnavailable); }

    for i in 0..page_count {
        let va = image_base + i as u64 * PAGE_SIZE;
        let physical = unsafe { crate::memory::allocate_page() }.map_err(|_| PeError::OutOfMemory)?;
        unsafe { space.map_user_page(va, physical, true, false)?; }
    }
    write_image(space, image_base, 0, &file[..headers_size as usize])?;
    for s in sections[..section_count].iter() {
        if s.raw_size != 0 {
            write_image(space, image_base, s.virtual_address, file_range(file, s.raw_offset, s.raw_size)?)?;
        }
    }

    let dir = opt + 112;
    let (reloc_rva, reloc_size) = if directory_count > 5 {
        (u32at(file, dir + 5 * 8)?, u32at(file, dir + 5 * 8 + 4)?)
    } else { (0, 0) };
    let delta = image_base.wrapping_sub(preferred_base);
    if delta != 0 && (reloc_rva == 0 || reloc_size == 0) { return Err(PeError::RelocationsRequired); }
    if reloc_rva != 0 && reloc_size != 0 {
        let end = reloc_rva.checked_add(reloc_size).filter(|&e| e <= image_size).ok_or(PeError::InvalidRelocations)?;
        let mut cursor = reloc_rva;
        while cursor < end {
            if end - cursor < 8 { return Err(PeError::InvalidRelocations); }
            let page_rva = read_image_u32(space, image_base, cursor)?;
            let block_size = read_image_u32(space, image_base, cursor + 4)?;
            if block_size < 8 || block_size > end - cursor || block_size & 1 != 0 {
                return Err(PeError::InvalidRelocations);
            }
            let entries = (block_size - 8) / 2;
            for j in 0..entries {
                let item = read_image_u16(space, image_base, cursor + 8 + j * 2)?;
                let kind = item >> 12; let offset = (item & 0x0fff) as u32;
                if kind == IMAGE_REL_BASED_ABSOLUTE { continue; }
                if kind != IMAGE_REL_BASED_DIR64 { return Err(PeError::InvalidRelocations); }
                let target = page_rva.checked_add(offset).ok_or(PeError::InvalidRelocations)?;
                if target.checked_add(8).filter(|&e| e <= image_size).is_none() { return Err(PeError::InvalidRelocations); }
                let old = read_image_u64(space, image_base, target)?;
                write_image_u64(space, image_base, target, old.wrapping_add(delta))?;
            }
            cursor = cursor.checked_add(block_size).ok_or(PeError::InvalidRelocations)?;
        }
    }

    let (import_rva, import_size) = if directory_count > 1 {
        (u32at(file, dir + 8)?, u32at(file, dir + 12)?)
    } else { (0, 0) };
    if import_rva != 0 && import_size != 0 {
        let import_end = import_rva.checked_add(import_size).filter(|&e| e <= image_size).ok_or(PeError::InvalidImports)?;
        let mut descriptor = import_rva;
        let mut terminated = false;
        while descriptor.checked_add(20).is_some_and(|e| e <= import_end) {
            let original_thunk = read_image_u32(space, image_base, descriptor)?;
            let name_rva = read_image_u32(space, image_base, descriptor + 12)?;
            let first_thunk = read_image_u32(space, image_base, descriptor + 16)?;
            if original_thunk == 0 && name_rva == 0 && first_thunk == 0 { terminated = true; break; }
            if name_rva == 0 || first_thunk == 0 { return Err(PeError::InvalidImports); }
            let (dll, dll_len) = read_c_string(space, image_base, name_rva)?;
            let lookup = if original_thunk != 0 { original_thunk } else { first_thunk };
            let mut index = 0u32;
            loop {
                let thunk_rva = lookup.checked_add(index.checked_mul(8).ok_or(PeError::InvalidImports)?).ok_or(PeError::InvalidImports)?;
                if thunk_rva.checked_add(8).filter(|&e| e <= image_size).is_none() { return Err(PeError::InvalidImports); }
                let thunk = read_image_u64(space, image_base, thunk_rva)?;
                if thunk == 0 { break; }
                let symbol = if thunk & IMAGE_ORDINAL_FLAG64 != 0 {
                    ImportSymbol::Ordinal((thunk & 0xffff) as u16)
                } else {
                    if thunk > u32::MAX as u64 { return Err(PeError::InvalidImports); }
                    let name_rva = (thunk as u32).checked_add(2).ok_or(PeError::InvalidImports)?;
                    let (name, name_len) = read_c_string(space, image_base, name_rva)?;
                    // The resolver only sees the name bytes, not the trailing NUL.
                    let resolved = resolver.ok_or(PeError::ImportResolverMissing)?(&dll[..dll_len], ImportSymbol::Name(&name[..name_len])).ok_or(PeError::UnresolvedImport)?;
                    let iat = first_thunk.checked_add(index * 8).ok_or(PeError::InvalidImports)?;
                    write_image_u64(space, image_base, iat, resolved).map_err(|_| PeError::InvalidImports)?;
                    index += 1;
                    continue;
                };
                let resolved = resolver.ok_or(PeError::ImportResolverMissing)?(&dll[..dll_len], symbol).ok_or(PeError::UnresolvedImport)?;
                let iat = first_thunk.checked_add(index.checked_mul(8).ok_or(PeError::InvalidImports)?).ok_or(PeError::InvalidImports)?;
                write_image_u64(space, image_base, iat, resolved)?;
                index += 1;
            }
            descriptor = descriptor.checked_add(20).ok_or(PeError::InvalidImports)?;
        }
        if !terminated { return Err(PeError::InvalidImports); }
    }

    // Headers default RO/NX. Section page permissions are combined; a shared
    // page requested as both writable and executable is rejected.
    let mut page_flags = [0u8; MAX_IMAGE_PAGES];
    for s in sections[..section_count].iter() {
        let span = core::cmp::max(s.virtual_size, s.raw_size) as u64;
        if span == 0 { continue; }
        let start = s.virtual_address as u64 & !(PAGE_SIZE - 1);
        let end = align_up(s.virtual_address as u64 + span, PAGE_SIZE).ok_or(PeError::InvalidSection)?;
        let mut flags = 0u8;
        if s.characteristics & IMAGE_SCN_MEM_WRITE != 0 { flags |= 1; }
        if s.characteristics & IMAGE_SCN_MEM_EXECUTE != 0 { flags |= 2; }
        let mut va = start;
        while va < end {
            let index = (va / PAGE_SIZE) as usize;
            if index >= page_count { return Err(PeError::InvalidSection); }
            page_flags[index] |= flags;
            if page_flags[index] & 3 == 3 { return Err(PeError::WxPageRejected); }
            va += PAGE_SIZE;
        }
    }
    for i in 0..page_count {
        let flags = page_flags[i];
        unsafe { space.protect_user_page(image_base + i as u64 * PAGE_SIZE, flags & 1 != 0, flags & 2 != 0)?; }
    }
    Ok(LoadedImage { base: image_base, entry: image_base + entry_rva as u64, image_size })
}

/// Build and load a tiny synthetic AMD64 PE32+ image to exercise header parsing,
/// section mapping, entry validation and final RX/RO-NX permissions during boot.
pub unsafe fn self_test(space: &mut AddressSpace) -> Result<LoadedImage, PeError> {
    fn put16(b: &mut [u8], o: usize, v: u16) { b[o..o + 2].copy_from_slice(&v.to_le_bytes()); }
    fn put32(b: &mut [u8], o: usize, v: u32) { b[o..o + 4].copy_from_slice(&v.to_le_bytes()); }
    fn put64(b: &mut [u8], o: usize, v: u64) { b[o..o + 8].copy_from_slice(&v.to_le_bytes()); }

    let mut image = [0u8; 0x600];
    put16(&mut image, 0, 0x5a4d);
    put32(&mut image, 0x3c, 0x80);
    image[0x80..0x84].copy_from_slice(b"PE\0\0");
    let coff = 0x84;
    put16(&mut image, coff, 0x8664);
    put16(&mut image, coff + 2, 2);
    put16(&mut image, coff + 16, 240);
    put16(&mut image, coff + 18, 0x0022);
    let opt = coff + 20;
    put16(&mut image, opt, 0x20b);
    put32(&mut image, opt + 16, 0x1000);
    put64(&mut image, opt + 24, 0x0000_0001_4000_0000);
    put32(&mut image, opt + 32, 0x1000);
    put32(&mut image, opt + 36, 0x200);
    put32(&mut image, opt + 56, 0x3000);
    put32(&mut image, opt + 60, 0x200);
    put32(&mut image, opt + 108, 16);
    // IMAGE_DIRECTORY_ENTRY_BASERELOC: RVA 0x2000, one DIR64 relocation.
    put32(&mut image, opt + 112 + 5 * 8, 0x2000);
    put32(&mut image, opt + 112 + 5 * 8 + 4, 12);

    let section = opt + 240;
    image[section..section + 8].copy_from_slice(b".text\0\0\0");
    put32(&mut image, section + 8, 0x100);
    put32(&mut image, section + 12, 0x1000);
    put32(&mut image, section + 16, 0x200);
    put32(&mut image, section + 20, 0x200);
    put32(&mut image, section + 36, 0x6000_0020);

    let reloc = section + 40;
    image[reloc..reloc + 8].copy_from_slice(b".reloc\0\0");
    put32(&mut image, reloc + 8, 12);
    put32(&mut image, reloc + 12, 0x2000);
    put32(&mut image, reloc + 16, 0x200);
    put32(&mut image, reloc + 20, 0x400);
    put32(&mut image, reloc + 36, 0x4200_0040);

    image[0x200] = 0xc3; // RET; mapped RX but deliberately not executed in this test.
    put64(&mut image, 0x210, 0x0000_0001_4000_1234);
    put32(&mut image, 0x400, 0x1000); // relocation block page RVA
    put32(&mut image, 0x404, 12);     // block size
    put16(&mut image, 0x408, 0xa010); // DIR64 at RVA 0x1010
    put16(&mut image, 0x40a, 0);      // ABSOLUTE padding

    let loaded = unsafe { load(space, &image, None)? };
    if loaded.base != USER_IMAGE_FALLBACK || loaded.entry != loaded.base + 0x1000 {
        return Err(PeError::InvalidHeader);
    }
    let relocated = read_image_u64(space, loaded.base, 0x1010)?;
    if relocated != loaded.base + 0x1234 { return Err(PeError::InvalidRelocations); }
    Ok(loaded)
}
