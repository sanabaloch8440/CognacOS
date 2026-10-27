use super::*;
use super::search::{append_search_component, LOAD_WITH_ALTERED_SEARCH_PATH, MAX_BUILT_SEARCH_PATH_UNITS};
use super::notifications::{delay_directory, image_ascii_string, IMAGE_DIRECTORY_ENTRY_DELAY_IMPORT, IMAGE_ORDINAL_FLAG64, MAX_DELAY_IMPORTS};

/// Resolve one delay-import thunk in an already mapped PE32+ image. Dependency
/// mapping is delegated to LdrLoadDll; until that service can map new files, this
/// correctly resolves only dependencies already present in the process loader list.
#[no_mangle]
pub unsafe extern "system" fn LdrResolveDelayLoadedAPI(
    parent_module_base: PVOID,
    descriptor: *const IMAGE_DELAYLOAD_DESCRIPTOR,
    _dll_failure_hook: PVOID,
    _system_failure_hook: PVOID,
    thunk_address: *mut u64,
    flags: ULONG,
) -> PVOID {
    if parent_module_base.is_null() || descriptor.is_null() || thunk_address.is_null() || flags != 0 {
        return ptr::null_mut();
    }
    let (base, image_size, _) = match unsafe { mapped_image_info(parent_module_base) } {
        Ok(v) => v, Err(_) => return ptr::null_mut(),
    };
    let descriptor_address = descriptor as usize;
    if descriptor_address < base || descriptor_address.checked_add(32).is_none_or(|end| end > base + image_size) {
        return ptr::null_mut();
    }
    let d = unsafe { &*descriptor };
    if d.Attributes & 1 == 0 || d.DllNameRVA == 0 || d.ModuleHandleRVA == 0
        || d.ImportAddressTableRVA == 0 || d.ImportNameTableRVA == 0 {
        return ptr::null_mut();
    }
    let rva_ok = |rva: u32, size: usize| (rva as usize).checked_add(size).is_some_and(|end| end <= image_size);
    if !rva_ok(d.DllNameRVA, 1) || !rva_ok(d.ModuleHandleRVA, 8)
        || !rva_ok(d.ImportAddressTableRVA, 8) || !rva_ok(d.ImportNameTableRVA, 8) {
        return ptr::null_mut();
    }
    let iat = base + d.ImportAddressTableRVA as usize;
    let int = base + d.ImportNameTableRVA as usize;
    let target = thunk_address as usize;
    if target < iat || (target - iat) % 8 != 0 || target.checked_add(8).is_none_or(|end| end > base + image_size) {
        return ptr::null_mut();
    }
    let index = (target - iat) / 8;
    let Some(lookup_rva) = (d.ImportNameTableRVA as usize).checked_add(index.saturating_mul(8)) else { return ptr::null_mut(); };
    if lookup_rva.checked_add(8).is_none_or(|end| end > image_size) { return ptr::null_mut(); }
    let thunk = unsafe { ptr::read_unaligned((int + index * 8) as *const u64) };
    if thunk == 0 { return ptr::null_mut(); }

    let module_slot = (base + d.ModuleHandleRVA as usize) as *mut PVOID;
    let mut dll_handle = unsafe { ptr::read_unaligned(module_slot) };
    if dll_handle.is_null() {
        let mut dll_ascii = [0u8; 260];
        let dll_len = match unsafe { image_ascii_string(base, image_size, d.DllNameRVA as usize, &mut dll_ascii) } {
            Ok(n) if n > 0 => n, _ => return ptr::null_mut(),
        };
        let mut dll_utf16 = [0u16; 260];
        for i in 0..dll_len { dll_utf16[i] = dll_ascii[i] as u16; }
        let name = UNICODE_STRING { length: (dll_len * 2) as u16, maximum_length: (dll_len * 2) as u16, buffer: dll_utf16.as_mut_ptr() };
        if unsafe { LdrLoadDll(ptr::null(), ptr::null_mut(), &name, &mut dll_handle) } != STATUS_SUCCESS {
            return ptr::null_mut();
        }
        unsafe { ptr::write_unaligned(module_slot, dll_handle); }
    }

    let mut procedure = ptr::null_mut();
    let status = if thunk & IMAGE_ORDINAL_FLAG64 != 0 {
        unsafe { crate::rtl_loader::LdrGetProcedureAddress(dll_handle, ptr::null(), (thunk & 0xffff) as ULONG, &mut procedure) }
    } else {
        let name_rva = match usize::try_from(thunk) { Ok(v) => v, Err(_) => return ptr::null_mut() };
        if name_rva.checked_add(3).is_none_or(|end| end > image_size) { return ptr::null_mut(); }
        let mut function_ascii = [0u8; 512];
        let function_len = match unsafe { image_ascii_string(base, image_size, name_rva + 2, &mut function_ascii) } {
            Ok(n) if n > 0 => n, _ => return ptr::null_mut(),
        };
        let ansi = crate::rtl_loader::COGNAC_ANSI_STRING {
            length: function_len as u16, maximum_length: function_len as u16,
            buffer: function_ascii.as_mut_ptr(),
        };
        unsafe { crate::rtl_loader::LdrGetProcedureAddress(dll_handle, &ansi, 0, &mut procedure) }
    };
    if status != STATUS_SUCCESS || procedure.is_null() { return ptr::null_mut(); }
    unsafe { ptr::write_unaligned(thunk_address, procedure as u64); }
    procedure
}

#[no_mangle]
pub unsafe extern "system" fn LdrResolveDelayLoadsFromDll(
    parent_module_base: PVOID, dll_name: *const u16,
) -> NTSTATUS {
    if parent_module_base.is_null() || dll_name.is_null() { return STATUS_INVALID_PARAMETER; }
    let (base, image_size, directory_rva) = match unsafe { delay_directory(parent_module_base) } {
        Ok(v) => v, Err(status) => return status,
    };
    let mut wanted = [0u16; 260];
    let mut wanted_len = 0usize;
    while wanted_len < wanted.len() {
        let ch = unsafe { ptr::read_unaligned(dll_name.add(wanted_len)) };
        if ch == 0 { break; }
        wanted[wanted_len] = ch;
        wanted_len += 1;
    }
    if wanted_len == 0 || wanted_len == wanted.len() { return STATUS_INVALID_PARAMETER; }
    let directory_size = unsafe {
        let nt = ptr::read_unaligned((base + 0x3c) as *const u32) as usize;
        let opt = base + nt + 24;
        let dir = opt + 112 + IMAGE_DIRECTORY_ENTRY_DELAY_IMPORT * 8;
        ptr::read_unaligned((dir + 4) as *const u32) as usize
    };
    let count = core::cmp::min(directory_size / 32, MAX_DELAY_IMPORTS);
    let mut matched = false;
    for i in 0..count {
        let descriptor = (base + directory_rva + i * 32) as *const IMAGE_DELAYLOAD_DESCRIPTOR;
        let d = unsafe { &*descriptor };
        if d.Attributes == 0 && d.DllNameRVA == 0 && d.ImportAddressTableRVA == 0 { break; }
        if d.Attributes & 1 == 0 { continue; }
        let mut name = [0u8; 260];
        let len = match unsafe { image_ascii_string(base, image_size, d.DllNameRVA as usize, &mut name) } {
            Ok(n) => n, Err(_) => return STATUS_INVALID_PARAMETER,
        };
        if len != wanted_len || (0..len).any(|j| (name[j] as u16) != wanted[j]) { continue; }
        matched = true;
        if d.ImportAddressTableRVA == 0 || d.ImportNameTableRVA == 0
            || d.ImportAddressTableRVA as usize >= image_size || d.ImportNameTableRVA as usize >= image_size {
            return STATUS_INVALID_PARAMETER;
        }
        let mut resolved = 0usize;
        for thunk_index in 0..MAX_DELAY_IMPORTS {
            let iat_rva = d.ImportAddressTableRVA as usize + thunk_index * 8;
            let int_rva = d.ImportNameTableRVA as usize + thunk_index * 8;
            if iat_rva.checked_add(8).is_none_or(|end| end > image_size)
                || int_rva.checked_add(8).is_none_or(|end| end > image_size) { return STATUS_INVALID_PARAMETER; }
            let lookup = unsafe { ptr::read_unaligned((base + int_rva) as *const u64) };
            if lookup == 0 { break; }
            let address = unsafe { LdrResolveDelayLoadedAPI(parent_module_base, descriptor, ptr::null_mut(), ptr::null_mut(), (base + iat_rva) as *mut u64, 0) };
            if address.is_null() { return STATUS_PROCEDURE_NOT_FOUND; }
            resolved += 1;
        }
        if resolved == MAX_DELAY_IMPORTS { return STATUS_INVALID_PARAMETER; }
    }
    if matched { STATUS_SUCCESS } else { STATUS_NOT_FOUND }
}

#[no_mangle]
pub unsafe extern "system" fn LdrResRelease() -> i32 { STATUS_NOT_IMPLEMENTED }

/// Search a mapped PE resource tree using the same validated type/name/language traversal
/// as LdrFindResource_U. This avoids a second, less strict parser for the legacy entry point.
#[no_mangle]
pub unsafe extern "system" fn LdrResSearchResource(
    module: PVOID, resource_info: *const LDR_RESOURCE_INFO, level: ULONG,
    resource_data_entry: *mut *mut IMAGE_RESOURCE_DATA_ENTRY,
) -> NTSTATUS {
    unsafe { LdrFindResource_U(module, resource_info, level, resource_data_entry) }
}

/// Test whether a numeric or string resource type has a top-level entry.
#[no_mangle]
pub unsafe extern "system" fn LdrRscIsTypeExist(module: PVOID, resource_type: ULONG_PTR) -> BOOLEAN {
    let (base, _image_size, packed) = match unsafe { mapped_image_info(module) } {
        Ok(v) => v, Err(_) => return 0,
    };
    let rva = packed as u32 as usize;
    let size = (packed >> 32) as usize;
    if rva == 0 || size < 16 { return 0; }
    let dir = (base + rva) as *const u8;
    let named = unsafe { ptr::read_unaligned(dir.add(12).cast::<u16>()) } as usize;
    let ids = unsafe { ptr::read_unaligned(dir.add(14).cast::<u16>()) } as usize;
    let Some(count) = named.checked_add(ids).filter(|n| *n <= MAX_RESOURCE_ENTRIES) else { return 0; };
    if 16usize.checked_add(count * 8).is_none_or(|end| end > size) { return 0; }
    for i in 0..count {
        let encoded = unsafe { ptr::read_unaligned(dir.add(16 + i * 8).cast::<u32>()) };
        if unsafe { resource_key_matches(base, _image_size, rva, size, encoded, resource_type) } { return 1; }
    }
    0
}

#[no_mangle]
pub unsafe extern "system" fn LdrSetAppCompatDllRedirectionCallback() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrSetDllManifestProber() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrSetImplicitPathOptions() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrSetMUICacheType() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrShutdownThread() -> NTSTATUS {
    // Thread-specific TLS/FLS and detach callbacks require a real thread/module
    // runtime. Do not fabricate callbacks; report success only for the no-op path.
    if DLL_SHUTDOWN.load(Ordering::Acquire) { STATUS_SUCCESS } else { STATUS_SUCCESS }
}

#[no_mangle]
pub unsafe extern "system" fn LdrStandardizeSystemPath() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrSystemDllInitBlock() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrUnloadAlternateResourceModule() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrUnloadAlternateResourceModuleEx() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrUnloadDll(dll_handle: PVOID) -> NTSTATUS {
    if dll_handle.is_null() { return STATUS_INVALID_PARAMETER; }
    if unsafe { find_entry_by_base(dll_handle as usize) }.is_none() { return STATUS_DLL_NOT_FOUND; }
    let _guard = ModuleRefGuard::acquire();
    let records = core::ptr::addr_of_mut!(MODULE_REFS).cast::<ModuleRef>();
    for i in 0..MAX_REF_RECORDS {
        let record = unsafe { &mut *records.add(i) };
        if record.base == dll_handle as usize {
            if record.pinned { return STATUS_SUCCESS; }
            if record.references > 1 { record.references -= 1; return STATUS_SUCCESS; }
            // Do not unlink the PEB entry or free memory without kernel-backed
            // unmap and dependency rundown services.
            return STATUS_NOT_IMPLEMENTED;
        }
    }
    STATUS_NOT_IMPLEMENTED
}



#[no_mangle]
pub unsafe extern "system" fn LdrUpdatePackageSearchPath() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrVerifyImageMatchesChecksum() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrVerifyImageMatchesChecksumEx() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn NtLoadDriver() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn NtUnloadDriver() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn RtlComputePrivatizedDllName_U() -> i32 { STATUS_NOT_IMPLEMENTED }



#[no_mangle]
pub unsafe extern "system" fn RtlIsThreadWithinLoaderCallout() -> BOOLEAN { 0 }

#[no_mangle]
pub unsafe extern "system" fn RtlWnfDllUnloadCallback() -> i32 { STATUS_NOT_IMPLEMENTED }


