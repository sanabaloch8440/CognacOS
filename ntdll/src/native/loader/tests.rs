#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dll_directory_round_trip_uses_native_unicode_string_abi() {
        let mut path: Vec<u16> = "C:\\System32".encode_utf16().collect();
        let input = UNICODE_STRING {
            length: (path.len() * 2) as u16,
            maximum_length: (path.len() * 2) as u16,
            buffer: path.as_mut_ptr(),
        };
        assert_eq!(unsafe { LdrSetDllDirectory(&input) }, STATUS_SUCCESS);
        let mut output = UNICODE_STRING { length: 0, maximum_length: 0, buffer: ptr::null_mut() };
        assert_eq!(unsafe { LdrGetDllDirectory(&mut output) }, STATUS_SUCCESS);
        assert_eq!(output.length, input.length);
        assert!(!output.buffer.is_null());
        assert_eq!(unsafe { core::slice::from_raw_parts(output.buffer, path.len()) }, path.as_slice());
        assert_eq!(unsafe { LdrSetDllDirectory(ptr::null()) }, STATUS_SUCCESS);
    }

    #[test]
    fn dll_directory_cookie_rejects_stale_handles() {
        let mut path: Vec<u16> = "C:\\Plugins".encode_utf16().collect();
        let input = UNICODE_STRING {
            length: (path.len() * 2) as u16,
            maximum_length: (path.len() * 2) as u16,
            buffer: path.as_mut_ptr(),
        };
        let mut cookie: PVOID = ptr::null_mut();
        assert_eq!(unsafe { LdrAddDllDirectory(&input, &mut cookie) }, STATUS_SUCCESS);
        assert!(!cookie.is_null());
        assert_eq!(unsafe { LdrRemoveDllDirectory(cookie) }, STATUS_SUCCESS);
        assert_eq!(unsafe { LdrRemoveDllDirectory(cookie) }, STATUS_INVALID_HANDLE);
    }

    #[test]
    fn default_search_flags_are_validated() {
        assert_eq!(unsafe { LdrSetDefaultDllDirectories(0) }, STATUS_INVALID_PARAMETER);
        assert_eq!(unsafe { LdrSetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_DEFAULT_DIRS) }, STATUS_SUCCESS);
    }

    #[test]
    fn dll_path_rejects_unknown_and_conflicting_flags_before_peb_access() {
        let name = [b'x' as u16, 0];
        let mut path = ptr::null_mut();
        let mut unknown = ptr::null_mut();
        assert_eq!(unsafe { LdrGetDllPath(name.as_ptr(), 0x8000_0000, &mut path, &mut unknown) }, STATUS_INVALID_PARAMETER);
        assert_eq!(unsafe { LdrGetDllPath(name.as_ptr(), LOAD_WITH_ALTERED_SEARCH_PATH | LOAD_LIBRARY_SEARCH_SYSTEM32, &mut path, &mut unknown) }, STATUS_INVALID_PARAMETER);
        assert!(path.is_null());
        assert!(unknown.is_null());
    }

    #[test]
    fn search_path_builder_is_bounded_and_separates_components() {
        let mut path = [0u16; MAX_BUILT_SEARCH_PATH_UNITS];
        let mut used = 0usize;
        assert!(append_search_component(&mut path, &mut used, &[b'X' as u16, b':' as u16, b'\\' as u16]));
        assert!(append_search_component(&mut path, &mut used, &[b'Y' as u16, b':' as u16, b'\\' as u16]));
        assert_eq!(&path[..used], &[b'X' as u16, b':' as u16, b'\\' as u16, b';' as u16, b'Y' as u16, b':' as u16, b'\\' as u16]);
    }

    #[test]
    fn mapped_pe_resource_lookup_and_access_are_bounded() {
        let mut image = [0u8; 0x3000];
        image[0..2].copy_from_slice(&0x5a4du16.to_le_bytes());
        image[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        image[0x80..0x84].copy_from_slice(b"PE\0\0");
        image[0x84..0x86].copy_from_slice(&0x8664u16.to_le_bytes());
        image[0x94..0x96].copy_from_slice(&240u16.to_le_bytes());
        let opt = 0x98;
        image[opt..opt+2].copy_from_slice(&0x20bu16.to_le_bytes());
        image[opt+56..opt+60].copy_from_slice(&0x3000u32.to_le_bytes());
        image[opt+108..opt+112].copy_from_slice(&16u32.to_le_bytes());
        image[opt+112+16..opt+112+20].copy_from_slice(&0x2000u32.to_le_bytes());
        image[opt+112+20..opt+112+24].copy_from_slice(&0x1000u32.to_le_bytes());
        // Three resource-directory levels: type 10, name 1, language 0x409.
        image[0x2000+14..0x2000+16].copy_from_slice(&1u16.to_le_bytes());
        image[0x2010..0x2014].copy_from_slice(&10u32.to_le_bytes());
        image[0x2014..0x2018].copy_from_slice(&0x8000_0020u32.to_le_bytes());
        image[0x2020+14..0x2020+16].copy_from_slice(&1u16.to_le_bytes());
        image[0x2030..0x2034].copy_from_slice(&1u32.to_le_bytes());
        image[0x2034..0x2038].copy_from_slice(&0x8000_0040u32.to_le_bytes());
        image[0x2040+14..0x2040+16].copy_from_slice(&1u16.to_le_bytes());
        image[0x2050..0x2054].copy_from_slice(&0x409u32.to_le_bytes());
        image[0x2054..0x2058].copy_from_slice(&0x60u32.to_le_bytes());
        image[0x2060..0x2064].copy_from_slice(&0x2100u32.to_le_bytes());
        image[0x2064..0x2068].copy_from_slice(&4u32.to_le_bytes());
        image[0x2100..0x2104].copy_from_slice(b"TEST");
        let info = LDR_RESOURCE_INFO { Type: 10, Name: 1, Language: 0x409 };
        let mut entry = ptr::null_mut();
        assert_eq!(unsafe { LdrFindResource_U(image.as_mut_ptr().cast(), &info, 3, &mut entry) }, STATUS_SUCCESS);
        assert!(!entry.is_null());
        let mut address = ptr::null_mut();
        let mut size = 0;
        assert_eq!(unsafe { LdrAccessResource(image.as_mut_ptr().cast(), entry, &mut address, &mut size) }, STATUS_SUCCESS);
        assert_eq!(size, 4);
        assert_eq!(unsafe { core::slice::from_raw_parts(address.cast::<u8>(), size as usize) }, b"TEST");
        assert_eq!(unsafe { LdrFindResource_U(image.as_mut_ptr().cast(), &info, 4, &mut entry) }, STATUS_INVALID_PARAMETER);
    }

    unsafe extern "system" fn notification_callback(_reason: ULONG, _data: *const c_void, _context: PVOID) {}

    #[test]
    fn dll_notification_cookie_lifetime_is_checked() {
        let mut cookie = ptr::null_mut();
        assert_eq!(unsafe { LdrRegisterDllNotification(0, Some(notification_callback), ptr::null_mut(), &mut cookie) }, STATUS_SUCCESS);
        assert!(!cookie.is_null());
        assert_eq!(unsafe { LdrUnregisterDllNotification(cookie) }, STATUS_SUCCESS);
        assert_eq!(unsafe { LdrUnregisterDllNotification(cookie) }, STATUS_INVALID_HANDLE);
    }

    #[test]
    fn optional_delay_import_query_validates_outputs_before_peb_access() {
        let mut address = 1usize as PVOID;
        assert_eq!(
            unsafe { LdrQueryOptionalDelayLoadedAPI(ptr::null_mut(), ptr::null(), ptr::null(), &mut address) },
            STATUS_INVALID_PARAMETER
        );
        assert!(address.is_null());
    }

    #[test]
    fn amd64_relocation_block_applies_dir64_and_rejects_unknown_types() {
        let mut image = [0u8; 64];
        image[0x20..0x28].copy_from_slice(&0x1234u64.to_le_bytes());
        let mut entries = [0xA020u16, 0x0000u16];
        let end = unsafe { LdrProcessRelocationBlock(image.as_mut_ptr() as usize, 4, entries.as_mut_ptr(), 0x1000) };
        assert_eq!(end, unsafe { entries.as_mut_ptr().add(2) });
        assert_eq!(u64::from_le_bytes(image[0x20..0x28].try_into().unwrap()), 0x2234);
        let mut unsupported = [0x3020u16];
        assert!(unsafe { LdrProcessRelocationBlock(image.as_mut_ptr() as usize, 2, unsupported.as_mut_ptr(), 1) }.is_null());
        assert!(unsafe { LdrProcessRelocationBlockEx(0x14c, image.as_mut_ptr() as usize, 2, unsupported.as_mut_ptr(), 1) }.is_null());
    }
}
