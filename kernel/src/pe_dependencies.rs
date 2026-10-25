//! Bounded first phase of PE dependency loading.
//!
//! This parses import descriptors from the file image before any virtual mapping
//! occurs. It deliberately stores names inline: no heap allocation, unbounded
//! recursion, or file-backed references survive the discovery phase.
pub const MAX_DEPENDENCIES: usize = 32;
pub const MAX_MODULE_NAME: usize = 96;
pub const STATUS_INVALID_IMAGE_FORMAT: i32 = 0xC000_007Bu32 as i32;
pub const STATUS_BUFFER_TOO_SMALL: i32 = 0xC000_0023u32 as i32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dependency {
    pub name: [u8; MAX_MODULE_NAME],
    pub len: u8,
}
impl Dependency {
    const EMPTY: Self = Self { name: [0; MAX_MODULE_NAME], len: 0 };
    pub fn as_bytes(&self) -> &[u8] { &self.name[..self.len as usize] }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DependencyPlan {
    entries: [Dependency; MAX_DEPENDENCIES],
    len: usize,
}
impl DependencyPlan {
    pub const fn new() -> Self { Self { entries: [Dependency::EMPTY; MAX_DEPENDENCIES], len: 0 } }
    pub fn len(&self) -> usize { self.len }
    pub fn is_empty(&self) -> bool { self.len == 0 }
    pub fn get(&self, index: usize) -> Option<&Dependency> { self.entries.get(index).filter(|_| index < self.len) }
    fn push_unique(&mut self, name: &[u8]) -> Result<(), i32> {
        if name.is_empty() || name.len() >= MAX_MODULE_NAME
            || !name.iter().all(|b| b.is_ascii_alphanumeric() || matches!(*b, b'.' | b'_' | b'-' | b'+' )) {
            return Err(STATUS_INVALID_IMAGE_FORMAT);
        }
        if self.entries[..self.len].iter().any(|e| e.as_bytes().eq_ignore_ascii_case(name)) { return Ok(()); }
        if self.len == MAX_DEPENDENCIES { return Err(STATUS_BUFFER_TOO_SMALL); }
        let entry = &mut self.entries[self.len];
        entry.name[..name.len()].copy_from_slice(name);
        entry.len = name.len() as u8;
        self.len += 1;
        Ok(())
    }
}
impl Default for DependencyPlan { fn default() -> Self { Self::new() } }

fn u16at(b: &[u8], o: usize) -> Result<u16, i32> {
    Ok(u16::from_le_bytes(b.get(o..o.checked_add(2).ok_or(STATUS_INVALID_IMAGE_FORMAT)?)
        .ok_or(STATUS_INVALID_IMAGE_FORMAT)?.try_into().map_err(|_| STATUS_INVALID_IMAGE_FORMAT)?))
}
fn u32at(b: &[u8], o: usize) -> Result<u32, i32> {
    Ok(u32::from_le_bytes(b.get(o..o.checked_add(4).ok_or(STATUS_INVALID_IMAGE_FORMAT)?)
        .ok_or(STATUS_INVALID_IMAGE_FORMAT)?.try_into().map_err(|_| STATUS_INVALID_IMAGE_FORMAT)?))
}
#[derive(Clone, Copy)]
struct Section { va: u32, virtual_size: u32, raw: u32, raw_size: u32 }
fn rva_to_file(file: &[u8], rva: u32, size: usize, headers: u32, sections: &[Section]) -> Result<usize, i32> {
    let end = rva.checked_add(u32::try_from(size).map_err(|_| STATUS_INVALID_IMAGE_FORMAT)?)
        .ok_or(STATUS_INVALID_IMAGE_FORMAT)?;
    let offset = if rva < headers && end <= headers {
        rva as usize
    } else {
        let section = sections.iter().find(|s| {
            let span = core::cmp::max(s.virtual_size, s.raw_size);
            rva >= s.va && end <= s.va.saturating_add(span)
                && end <= s.va.saturating_add(s.raw_size)
        }).ok_or(STATUS_INVALID_IMAGE_FORMAT)?;
        section.raw.checked_add(rva - section.va).ok_or(STATUS_INVALID_IMAGE_FORMAT)? as usize
    };
    offset.checked_add(size).filter(|e| *e <= file.len()).ok_or(STATUS_INVALID_IMAGE_FORMAT)?;
    Ok(offset)
}
fn read_name(file: &[u8], rva: u32, headers: u32, sections: &[Section]) -> Result<([u8; MAX_MODULE_NAME], usize), i32> {
    let mut out = [0u8; MAX_MODULE_NAME];
    for i in 0..MAX_MODULE_NAME {
        let at = rva.checked_add(i as u32).ok_or(STATUS_INVALID_IMAGE_FORMAT)?;
        let offset = rva_to_file(file, at, 1, headers, sections)?;
        let b = file[offset];
        if b == 0 {
            if i == 0 { return Err(STATUS_INVALID_IMAGE_FORMAT); }
            return Ok((out, i));
        }
        if !b.is_ascii() { return Err(STATUS_INVALID_IMAGE_FORMAT); }
        out[i] = b;
    }
    Err(STATUS_INVALID_IMAGE_FORMAT)
}

/// Discover normal and RVA-based delay-import DLL names from a PE32+ file.
/// Output is a deduplicated fixed-capacity plan; the caller must load each
/// dependency before mapping the parent and roll back new mappings on failure.
pub fn discover(file: &[u8]) -> Result<DependencyPlan, i32> {
    if file.len() < 0x40 || u16at(file, 0)? != 0x5a4d { return Err(STATUS_INVALID_IMAGE_FORMAT); }
    let nt = u32at(file, 0x3c)? as usize;
    if file.get(nt..nt.checked_add(4).ok_or(STATUS_INVALID_IMAGE_FORMAT)?) != Some(b"PE\0\0") {
        return Err(STATUS_INVALID_IMAGE_FORMAT);
    }
    let coff = nt.checked_add(4).ok_or(STATUS_INVALID_IMAGE_FORMAT)?;
    if u16at(file, coff)? != 0x8664 { return Err(STATUS_INVALID_IMAGE_FORMAT); }
    let count = u16at(file, coff + 2)? as usize;
    let opt_size = u16at(file, coff + 16)? as usize;
    if count == 0 || count > 96 || opt_size < 240 || u16at(file, coff + 20)? != 0x20b {
        return Err(STATUS_INVALID_IMAGE_FORMAT);
    }
    let opt = coff + 20;
    let headers = u32at(file, opt + 60)?;
    let dir_count = core::cmp::min(u32at(file, opt + 108)? as usize, 16);
    let section_table = opt.checked_add(opt_size).ok_or(STATUS_INVALID_IMAGE_FORMAT)?;
    let mut sections = [Section { va: 0, virtual_size: 0, raw: 0, raw_size: 0 }; 96];
    for (i, s) in sections[..count].iter_mut().enumerate() {
        let o = section_table.checked_add(i.checked_mul(40).ok_or(STATUS_INVALID_IMAGE_FORMAT)?)
            .ok_or(STATUS_INVALID_IMAGE_FORMAT)?;
        if o.checked_add(40).filter(|e| *e <= file.len()).is_none() { return Err(STATUS_INVALID_IMAGE_FORMAT); }
        *s = Section { virtual_size: u32at(file, o + 8)?, va: u32at(file, o + 12)?,
            raw_size: u32at(file, o + 16)?, raw: u32at(file, o + 20)? };
    }
    let mut plan = DependencyPlan::new();
    if dir_count > 1 {
        let rva = u32at(file, opt + 112 + 8)?;
        let size = u32at(file, opt + 112 + 12)?;
        if rva != 0 || size != 0 {
            if rva == 0 || size < 20 { return Err(STATUS_INVALID_IMAGE_FORMAT); }
            let mut offset_rva = 0u32;
            let mut terminated = false;
            while offset_rva.checked_add(20).is_some_and(|e| e <= size) {
                let d = rva.checked_add(offset_rva).ok_or(STATUS_INVALID_IMAGE_FORMAT)?;
                let o = rva_to_file(file, d, 20, headers, &sections)?;
                let original = u32at(file, o)?;
                let name_rva = u32at(file, o + 12)?;
                let first = u32at(file, o + 16)?;
                if original == 0 && name_rva == 0 && first == 0 { terminated = true; break; }
                if name_rva == 0 || first == 0 { return Err(STATUS_INVALID_IMAGE_FORMAT); }
                let (name, len) = read_name(file, name_rva, headers, &sections)?;
                plan.push_unique(&name[..len])?;
                offset_rva = offset_rva.checked_add(20).ok_or(STATUS_INVALID_IMAGE_FORMAT)?;
            }
            if !terminated { return Err(STATUS_INVALID_IMAGE_FORMAT); }
        }
    }
    if dir_count > 13 {
        let rva = u32at(file, opt + 112 + 13 * 8)?;
        let size = u32at(file, opt + 112 + 13 * 8 + 4)?;
        if rva != 0 || size != 0 {
            if rva == 0 || size < 32 { return Err(STATUS_INVALID_IMAGE_FORMAT); }
            let mut delta = 0u32;
            let mut terminated = false;
            while delta.checked_add(32).is_some_and(|e| e <= size) {
                let o = rva_to_file(file, rva.checked_add(delta).ok_or(STATUS_INVALID_IMAGE_FORMAT)?, 32, headers, &sections)?;
                let attrs = u32at(file, o)?;
                let name_rva = u32at(file, o + 4)?;
                let iat = u32at(file, o + 12)?;
                let int = u32at(file, o + 16)?;
                if attrs == 0 && name_rva == 0 && iat == 0 && int == 0 { terminated = true; break; }
                if attrs & 1 == 0 || name_rva == 0 || iat == 0 { return Err(STATUS_INVALID_IMAGE_FORMAT); }
                let (name, len) = read_name(file, name_rva, headers, &sections)?;
                plan.push_unique(&name[..len])?;
                delta = delta.checked_add(32).ok_or(STATUS_INVALID_IMAGE_FORMAT)?;
            }
            if !terminated { return Err(STATUS_INVALID_IMAGE_FORMAT); }
        }
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dependency_plan_deduplicates_case_insensitively() {
        let mut plan = DependencyPlan::new();
        plan.push_unique(b"NTDLL.dll").unwrap();
        plan.push_unique(b"ntdll.DLL").unwrap();
        assert_eq!(plan.len(), 1);
        assert_eq!(plan.get(0).unwrap().as_bytes(), b"NTDLL.dll");
    }
    #[test]
    fn dependency_plan_is_bounded() {
        let mut plan = DependencyPlan::new();
        for i in 0..MAX_DEPENDENCIES {
            let name = [b'a' + i as u8, b'.', b'd', b'l', b'l'];
            plan.push_unique(&name).unwrap();
        }
        assert_eq!(plan.push_unique(b"overflow.dll"), Err(STATUS_BUFFER_TOO_SMALL));
        assert_eq!(plan.len(), MAX_DEPENDENCIES);
    }
}


pub mod transaction;
