#[no_mangle]
pub unsafe extern "system" fn LdrFindResource_U(
    module: PVOID, resource_info: *const LDR_RESOURCE_INFO, level: ULONG,
    resource_data_entry: *mut *mut IMAGE_RESOURCE_DATA_ENTRY,
) -> NTSTATUS {
    if resource_data_entry.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe { resource_data_entry.write(ptr::null_mut()); }
    match unsafe { find_resource_entry(module, resource_info, level, false) } {
        Ok(entry) => { unsafe { resource_data_entry.write(entry); } STATUS_SUCCESS },
        Err(status) => status,
    }
}

#[no_mangle]
pub unsafe extern "system" fn LdrFindResourceDirectory_U(
    module: PVOID, resource_info: *const LDR_RESOURCE_INFO, level: ULONG,
    resource_directory: *mut *mut c_void,
) -> NTSTATUS {
    if resource_directory.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe { resource_directory.write(ptr::null_mut()); }
    match unsafe { find_resource_entry(module, resource_info, level, true) } {
        Ok(entry) => { unsafe { resource_directory.write(entry.cast()); } STATUS_SUCCESS },
        Err(status) => status,
    }
}

#[no_mangle]
pub unsafe extern "system" fn LdrFindResourceEx_U(
    flags: ULONG, module: PVOID, resource_info: *const LDR_RESOURCE_INFO, level: ULONG,
    resource_data_entry: *mut *mut IMAGE_RESOURCE_DATA_ENTRY,
) -> NTSTATUS {
    if flags != 0 { return STATUS_INVALID_PARAMETER; }
    unsafe { LdrFindResource_U(module, resource_info, level, resource_data_entry) }
}

#[no_mangle]
pub unsafe extern "system" fn LdrAccessResource(
    module: PVOID, resource_data_entry: *const IMAGE_RESOURCE_DATA_ENTRY,
    resource_address: *mut *mut c_void, resource_size: *mut ULONG,
) -> NTSTATUS {
    if module.is_null() || resource_data_entry.is_null() || resource_address.is_null() || resource_size.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    unsafe { resource_address.write(ptr::null_mut()); resource_size.write(0); }
    let (base, image_size, packed) = match unsafe { mapped_image_info(module) } { Ok(v) => v, Err(e) => return e };
    let resource_rva = packed as u32 as usize;
    let resource_directory_size = (packed >> 32) as usize;
    let entry_address = resource_data_entry as usize;
    let image_base = base;
    let expected_start = image_base.saturating_add(resource_rva);
    let expected_end = expected_start.checked_add(resource_directory_size);
    if resource_rva == 0 || expected_end.map_or(true, |end| entry_address < expected_start || entry_address.checked_add(16).map_or(true, |entry_end| entry_end > end)) {
        return STATUS_INVALID_PARAMETER;
    }
    let rva = unsafe { ptr::read_unaligned(ptr::addr_of!((*resource_data_entry).OffsetToData)) } as usize;
    let resource_bytes = unsafe { ptr::read_unaligned(ptr::addr_of!((*resource_data_entry).Size)) } as usize;
    if rva.checked_add(resource_bytes).filter(|end| *end <= image_size).is_none() { return 0xC000_007Bu32 as i32; }
    unsafe { resource_address.write((base + rva) as *mut c_void); resource_size.write(resource_bytes as ULONG); }
    STATUS_SUCCESS
}

const MAX_DLL_NOTIFICATIONS: usize = 32;
#[derive(Clone, Copy)]
struct Notification { cookie: usize, callback: usize, context: usize }
impl Notification { const EMPTY: Self = Self { cookie: 0, callback: 0, context: 0 }; }
static mut DLL_NOTIFICATIONS: [Notification; MAX_DLL_NOTIFICATIONS] = [Notification::EMPTY; MAX_DLL_NOTIFICATIONS];

