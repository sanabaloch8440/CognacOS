//! Bounded Rust port of the PE-resource lookup core from Wine's ntdll/resource.c.
//!
//! This module parses an already-mapped PE image represented as a byte slice.
//! It deliberately does not turn arbitrary HMODULE pointers into unchecked
//! slices, install exception handlers, or claim the Windows loader ABI exports.
//! A future native bridge can construct a validated image view from the loader's
//! image mapping and route LdrFindResource_U/LdrAccessResource/RtlFindMessage
//! through these routines.
//!
//! Upstream correspondence: root/wine/dlls/ntdll/resource.c (LGPL-2.1-or-later).

#![allow(non_camel_case_types, non_snake_case, dead_code)]

pub type ULONG = u32;
pub type USHORT = u16;
pub type LANGID = u16;
pub type NTSTATUS = i32;

pub const STATUS_SUCCESS: NTSTATUS = 0;
pub const STATUS_INVALID_PARAMETER: NTSTATUS = 0xC000_000D_u32 as i32;
pub const STATUS_RESOURCE_DATA_NOT_FOUND: NTSTATUS = 0xC000_0089_u32 as i32;
pub const STATUS_RESOURCE_TYPE_NOT_FOUND: NTSTATUS = 0xC000_008A_u32 as i32;
pub const STATUS_RESOURCE_NAME_NOT_FOUND: NTSTATUS = 0xC000_008B_u32 as i32;
pub const STATUS_RESOURCE_LANG_NOT_FOUND: NTSTATUS = 0xC000_008C_u32 as i32;
pub const STATUS_MESSAGE_NOT_FOUND: NTSTATUS = 0xC000_0109_u32 as i32;
pub const STATUS_INVALID_IMAGE_FORMAT: NTSTATUS = 0xC000_007B_u32 as i32;

const IMAGE_RESOURCE_DIRECTORY_SIZE: usize = 16;
const IMAGE_RESOURCE_DIRECTORY_ENTRY_SIZE: usize = 8;
const IMAGE_RESOURCE_DATA_ENTRY_SIZE: usize = 16;
const IMAGE_RESOURCE_DATA_IS_DIRECTORY: u32 = 0x8000_0000;
const IMAGE_RESOURCE_NAME_IS_STRING: u32 = 0x8000_0000;
const IMAGE_RESOURCE_OFFSET_MASK: u32 = 0x7fff_ffff;

/// A resource identifier is either an integer ID or a case-sensitive UTF-16 name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceKey {
    Id(u16),
    Name(Vec<u16>),
}

impl ResourceKey {
    pub fn from_int_resource(value: usize) -> Option<Self> {
        if value >> 16 == 0 { Some(Self::Id(value as u16)) } else { None }
    }
}

/// Resource lookup information corresponding to LDR_RESOURCE_INFO.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceInfo {
    pub resource_type: ResourceKey,
    pub name: ResourceKey,
    pub language: LANGID,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceDataEntry {
    pub data_rva: ULONG,
    pub size: ULONG,
    pub code_page: ULONG,
    pub reserved: ULONG,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceLocation {
    /// Offset of the IMAGE_RESOURCE_DIRECTORY in the mapped image.
    pub directory_offset: usize,
    pub directory_size: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceDataLocation {
    pub entry_offset: usize,
    pub data_rva: ULONG,
    pub size: ULONG,
    pub code_page: ULONG,
}

/// A checked view of an already-mapped PE image's resource directory.
/// All offsets in resource directory entries are relative to resource_root.
pub struct ResourceImage<'a> {
    image: &'a [u8],
    resource_root: usize,
    resource_size: usize,
}

impl<'a> ResourceImage<'a> {
    /// Construct from a mapped image and its validated resource-directory RVA/size.
    pub fn new(image: &'a [u8], resource_rva: ULONG, resource_size: ULONG) -> Result<Self, NTSTATUS> {
        let root = resource_rva as usize;
        let size = resource_size as usize;
        if size < IMAGE_RESOURCE_DIRECTORY_SIZE
            || root.checked_add(size).filter(|&end| end <= image.len()).is_none()
        {
            return Err(STATUS_RESOURCE_DATA_NOT_FOUND);
        }
        Ok(Self { image, resource_root: root, resource_size: size })
    }

    fn range(&self, offset: usize, length: usize) -> Result<&'a [u8], NTSTATUS> {
        let end = offset.checked_add(length).ok_or(STATUS_INVALID_IMAGE_FORMAT)?;
        if end > self.resource_size {
            return Err(STATUS_INVALID_IMAGE_FORMAT);
        }
        let absolute = self.resource_root.checked_add(offset).ok_or(STATUS_INVALID_IMAGE_FORMAT)?;
        self.image.get(absolute..absolute + length).ok_or(STATUS_INVALID_IMAGE_FORMAT)
    }

    fn u16_at(&self, offset: usize) -> Result<u16, NTSTATUS> {
        let b = self.range(offset, 2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32_at(&self, offset: usize) -> Result<u32, NTSTATUS> {
        let b = self.range(offset, 4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn directory_counts(&self, offset: usize) -> Result<(usize, usize), NTSTATUS> {
        let bytes = self.range(offset, IMAGE_RESOURCE_DIRECTORY_SIZE)?;
        let named = u16::from_le_bytes([bytes[12], bytes[13]]) as usize;
        let ids = u16::from_le_bytes([bytes[14], bytes[15]]) as usize;
        let count = named.checked_add(ids).ok_or(STATUS_INVALID_IMAGE_FORMAT)?;
        let entries_len = count.checked_mul(IMAGE_RESOURCE_DIRECTORY_ENTRY_SIZE)
            .ok_or(STATUS_INVALID_IMAGE_FORMAT)?;
        self.range(offset + IMAGE_RESOURCE_DIRECTORY_SIZE, entries_len)?;
        Ok((named, ids))
    }

    fn entry(&self, directory_offset: usize, index: usize) -> Result<(u32, u32), NTSTATUS> {
        let offset = directory_offset
            .checked_add(IMAGE_RESOURCE_DIRECTORY_SIZE)
            .and_then(|x| x.checked_add(index.checked_mul(IMAGE_RESOURCE_DIRECTORY_ENTRY_SIZE)?))
            .ok_or(STATUS_INVALID_IMAGE_FORMAT)?;
        Ok((self.u32_at(offset)?, self.u32_at(offset + 4)?))
    }

    fn name_at(&self, relative_offset: usize) -> Result<Vec<u16>, NTSTATUS> {
        let len = self.u16_at(relative_offset)? as usize;
        let bytes_len = len.checked_mul(2).ok_or(STATUS_INVALID_IMAGE_FORMAT)?;
        let bytes = self.range(relative_offset + 2, bytes_len)?;
        Ok(bytes.chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]])).collect())
    }

    fn key_matches(&self, encoded_name: u32, key: &ResourceKey) -> Result<bool, NTSTATUS> {
        match key {
            ResourceKey::Id(id) => Ok(encoded_name & IMAGE_RESOURCE_NAME_IS_STRING == 0
                && (encoded_name as u16) == *id),
            ResourceKey::Name(wanted) => {
                if encoded_name & IMAGE_RESOURCE_NAME_IS_STRING == 0 { return Ok(false); }
                let offset = (encoded_name & IMAGE_RESOURCE_OFFSET_MASK) as usize;
                Ok(self.name_at(offset)? == *wanted)
            }
        }
    }

    fn find_entry(&self, directory_offset: usize, key: &ResourceKey) -> Result<Option<(usize, u32)>, NTSTATUS> {
        let (named_count, id_count) = self.directory_counts(directory_offset)?;
        let count = named_count + id_count;
        // PE directories sort named entries before numeric IDs. Search each
        // region independently; malformed ordering cannot cause out-of-bounds reads.
        let (start, end) = match key {
            ResourceKey::Name(_) => (0, named_count),
            ResourceKey::Id(_) => (named_count, named_count + id_count),
        };
        for index in start..end.min(count) {
            let (name, child) = self.entry(directory_offset, index)?;
            if self.key_matches(name, key)? {
                return Ok(Some((index, child)));
            }
        }
        Ok(None)
    }

    fn first_child(&self, directory_offset: usize, want_directory: bool) -> Result<Option<usize>, NTSTATUS> {
        let (named, ids) = self.directory_counts(directory_offset)?;
        for index in 0..named + ids {
            let (_, child) = self.entry(directory_offset, index)?;
            if ((child & IMAGE_RESOURCE_DATA_IS_DIRECTORY) != 0) == want_directory {
                return Ok(Some((child & IMAGE_RESOURCE_OFFSET_MASK) as usize));
            }
        }
        Ok(None)
    }

    fn child_directory(&self, child: u32) -> Result<usize, NTSTATUS> {
        if child & IMAGE_RESOURCE_DATA_IS_DIRECTORY == 0 {
            return Err(STATUS_INVALID_IMAGE_FORMAT);
        }
        let offset = (child & IMAGE_RESOURCE_OFFSET_MASK) as usize;
        self.directory_counts(offset)?;
        Ok(offset)
    }

    fn find_language(&self, directory: usize, language: LANGID) -> Result<Option<usize>, NTSTATUS> {
        if let Some((_, child)) = self.find_entry(directory, &ResourceKey::Id(language))? {
            if child & IMAGE_RESOURCE_DATA_IS_DIRECTORY != 0 {
                return Err(STATUS_INVALID_IMAGE_FORMAT);
            }
            return Ok(Some((child & IMAGE_RESOURCE_OFFSET_MASK) as usize));
        }
        // Windows/Wine may try a richer preferred-language fallback chain.
        // This safe baseline follows the neutral-language fallback, then the
        // first available leaf when the caller requested a neutral language.
        if language != 0 {
            if let Some((_, child)) = self.find_entry(directory, &ResourceKey::Id(0))? {
                if child & IMAGE_RESOURCE_DATA_IS_DIRECTORY == 0 {
                    return Ok(Some((child & IMAGE_RESOURCE_OFFSET_MASK) as usize));
                }
            }
        } else if let Some(offset) = self.first_child(directory, false)? {
            return Ok(Some(offset));
        }
        Ok(None)
    }

    /// Resolve a resource path. Levels are 1 (type), 2 (type/name), or 3
    /// (type/name/language), matching the shape accepted by LdrFindResource*.
    pub fn find(&self, info: &ResourceInfo, level: ULONG, want_directory: bool) -> Result<ResourceLocation, NTSTATUS> {
        if !(1..=3).contains(&level) { return Err(STATUS_INVALID_PARAMETER); }
        let mut directory = 0usize;
        self.directory_counts(directory)?;

        if level == 1 {
            return Ok(ResourceLocation { directory_offset: 0, directory_size: self.resource_size });
        }
        let (_, type_child) = self.find_entry(directory, &info.resource_type)?
            .ok_or(STATUS_RESOURCE_TYPE_NOT_FOUND)?;
        directory = self.child_directory(type_child)?;
        let (_, name_child) = self.find_entry(directory, &info.name)?
            .ok_or(STATUS_RESOURCE_NAME_NOT_FOUND)?;
        if level == 2 {
            let offset = self.child_directory(name_child)?;
            return Ok(ResourceLocation { directory_offset: offset, directory_size: self.resource_size - offset });
        }
        directory = self.child_directory(name_child)?;
        if want_directory {
            if let Some(offset) = self.find_language(directory, info.language)? {
                self.directory_counts(offset)?;
                return Ok(ResourceLocation { directory_offset: offset, directory_size: self.resource_size - offset });
            }
            return Err(STATUS_RESOURCE_LANG_NOT_FOUND);
        }
        let offset = self.find_language(directory, info.language)?
            .ok_or(STATUS_RESOURCE_LANG_NOT_FOUND)?;
        self.range(offset, IMAGE_RESOURCE_DATA_ENTRY_SIZE)?;
        Ok(ResourceLocation { directory_offset: offset, directory_size: IMAGE_RESOURCE_DATA_ENTRY_SIZE })
    }

    /// Resolve the resource data entry and validate its payload range.
    pub fn find_data(&self, info: &ResourceInfo) -> Result<ResourceDataLocation, NTSTATUS> {
        let location = self.find(info, 3, false)?;
        let offset = location.directory_offset;
        let data_rva = self.u32_at(offset)?;
        let size = self.u32_at(offset + 4)?;
        let code_page = self.u32_at(offset + 8)?;
        let _reserved = self.u32_at(offset + 12)?;
        let data_start = data_rva as usize;
        let data_end = data_start.checked_add(size as usize).ok_or(STATUS_INVALID_IMAGE_FORMAT)?;
        if data_end > self.image.len() {
            return Err(STATUS_INVALID_IMAGE_FORMAT);
        }
        Ok(ResourceDataLocation { entry_offset: self.resource_root + offset, data_rva, size, code_page })
    }

    /// Returns bytes for a data entry's payload after validating the RVA and size.
    pub fn data<'b>(&'b self, location: &ResourceDataLocation) -> Result<&'a [u8], NTSTATUS> {
        let start = location.data_rva as usize;
        let end = start.checked_add(location.size as usize).ok_or(STATUS_INVALID_IMAGE_FORMAT)?;
        self.image.get(start..end).ok_or(STATUS_INVALID_IMAGE_FORMAT)
    }
}

/// Parse a MESSAGE_RESOURCE_DATA block array and return the entry for message_id.
/// Returned entry includes its 4-byte header (Length, Flags, then message bytes).
pub fn find_message_entry(data: &[u8], message_id: ULONG) -> Result<&[u8], NTSTATUS> {
    if data.len() < 4 { return Err(STATUS_INVALID_IMAGE_FORMAT); }
    let blocks_count = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
    let table_bytes = blocks_count.checked_mul(12).ok_or(STATUS_INVALID_IMAGE_FORMAT)?;
    let table_end = 4usize.checked_add(table_bytes).ok_or(STATUS_INVALID_IMAGE_FORMAT)?;
    if table_end > data.len() { return Err(STATUS_INVALID_IMAGE_FORMAT); }

    for index in 0..blocks_count {
        let off = 4 + index * 12;
        let low = u32::from_le_bytes(data[off..off + 4].try_into().unwrap());
        let high = u32::from_le_bytes(data[off + 4..off + 8].try_into().unwrap());
        let entries_offset = u32::from_le_bytes(data[off + 8..off + 12].try_into().unwrap()) as usize;
        if message_id < low || message_id > high { continue; }

        let mut cursor = entries_offset;
        for _ in low..message_id {
            let header_end = cursor.checked_add(4).ok_or(STATUS_INVALID_IMAGE_FORMAT)?;
            if header_end > data.len() { return Err(STATUS_INVALID_IMAGE_FORMAT); }
            let length = u16::from_le_bytes([data[cursor], data[cursor + 1]]) as usize;
            if length < 4 { return Err(STATUS_INVALID_IMAGE_FORMAT); }
            cursor = cursor.checked_add(length).ok_or(STATUS_INVALID_IMAGE_FORMAT)?;
            if cursor > data.len() { return Err(STATUS_INVALID_IMAGE_FORMAT); }
        }
        let header_end = cursor.checked_add(4).ok_or(STATUS_INVALID_IMAGE_FORMAT)?;
        if header_end > data.len() { return Err(STATUS_INVALID_IMAGE_FORMAT); }
        let length = u16::from_le_bytes([data[cursor], data[cursor + 1]]) as usize;
        if length < 4 { return Err(STATUS_INVALID_IMAGE_FORMAT); }
        let end = cursor.checked_add(length).ok_or(STATUS_INVALID_IMAGE_FORMAT)?;
        return data.get(cursor..end).ok_or(STATUS_INVALID_IMAGE_FORMAT);
    }
    Err(STATUS_MESSAGE_NOT_FOUND)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put16(b: &mut [u8], off: usize, value: u16) { b[off..off+2].copy_from_slice(&value.to_le_bytes()); }
    fn put32(b: &mut [u8], off: usize, value: u32) { b[off..off+4].copy_from_slice(&value.to_le_bytes()); }

    fn sample_resource_image() -> Vec<u8> {
        // Root: type 10 -> name 1 -> language 0x0409 -> data entry at +64.
        let mut image = vec![0u8; 256];
        let root = 32usize;
        // Root directory, one ID entry.
        put16(&mut image, root + 14, 1);
        put32(&mut image, root + 16, 10);
        put32(&mut image, root + 20, IMAGE_RESOURCE_DATA_IS_DIRECTORY | 24);
        // Type directory at root+24, name ID 1.
        put16(&mut image, root + 24 + 14, 1);
        put32(&mut image, root + 24 + 16, 1);
        put32(&mut image, root + 24 + 20, IMAGE_RESOURCE_DATA_IS_DIRECTORY | 48);
        // Name directory at root+48, language 0x0409 -> data entry at root+64.
        put16(&mut image, root + 48 + 14, 1);
        put32(&mut image, root + 48 + 16, 0x0409);
        put32(&mut image, root + 48 + 20, 80);
        put32(&mut image, root + 80, 200); // payload RVA
        put32(&mut image, root + 84, 4);
        put32(&mut image, root + 88, 1200);
        image[200..204].copy_from_slice(b"test");
        image
    }

    #[test]
    fn finds_numeric_resource_and_reads_payload() {
        let image = sample_resource_image();
        let view = ResourceImage::new(&image, 32, 128).unwrap();
        let info = ResourceInfo { resource_type: ResourceKey::Id(10), name: ResourceKey::Id(1), language: 0x0409 };
        let found = view.find_data(&info).unwrap();
        assert_eq!(found.size, 4);
        assert_eq!(found.code_page, 1200);
        assert_eq!(view.data(&found).unwrap(), b"test");
    }

    #[test]
    fn malformed_directory_offsets_are_rejected() {
        let image = vec![0u8; 48];
        let view = ResourceImage::new(&image, 16, 32).unwrap();
        assert_eq!(view.find_data(&ResourceInfo { resource_type: ResourceKey::Id(1), name: ResourceKey::Id(1), language: 0 }), Err(STATUS_RESOURCE_TYPE_NOT_FOUND));
    }

    #[test]
    fn message_lookup_walks_variable_length_entries() {
        let mut data = vec![0u8; 40];
        put32(&mut data, 0, 1);
        put32(&mut data, 4, 100);
        put32(&mut data, 8, 101);
        put32(&mut data, 12, 16);
        put16(&mut data, 16, 6);
        data[20..22].copy_from_slice(b"A!");
        put16(&mut data, 22, 7);
        data[26..29].copy_from_slice(b"XYZ");
        assert_eq!(find_message_entry(&data, 101).unwrap(), &data[22..29]);
        assert_eq!(find_message_entry(&data, 99), Err(STATUS_MESSAGE_NOT_FOUND));
    }

    #[test]
    fn rejects_truncated_message_block_table() {
        assert_eq!(find_message_entry(&[1, 0, 0, 0], 1), Err(STATUS_INVALID_IMAGE_FORMAT));
    }

    #[test]
    fn integer_resource_detection_matches_win32_encoding() {
        assert_eq!(ResourceKey::from_int_resource(0x0000_0010), Some(ResourceKey::Id(16)));
        assert_eq!(ResourceKey::from_int_resource(0x0001_0010), None);
    }
}
