use super::*;
use super::notifications::{image_ascii_string, IMAGE_ORDINAL_FLAG64};

#[no_mangle]
pub unsafe extern "system" fn LdrAddLoadAsDataTable() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrAddRefDll(flags: ULONG, dll_handle: PVOID) -> NTSTATUS {
    if flags & !1 != 0 || dll_handle.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe { track_module_reference(dll_handle as usize, flags & 1 != 0) }
}

#[no_mangle]
pub unsafe extern "system" fn LdrAppxHandleIntegrityFailure() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrCallEnclave() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrControlFlowGuardEnforced() -> BOOLEAN { 0 }

#[no_mangle]
pub unsafe extern "system" fn LdrCreateEnclave() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrDeleteEnclave() -> i32 { STATUS_NOT_IMPLEMENTED }

#[repr(C)]
pub struct LDR_DATA_TABLE_ENTRY_PREFIX {
    pub InLoadOrderLinks: [usize; 2], pub InMemoryOrderLinks: [usize; 2],
    pub InInitializationOrderLinks: [usize; 2], pub DllBase: PVOID, pub EntryPoint: PVOID,
    pub SizeOfImage: ULONG, pub _padding: ULONG,
}
pub type PLDR_ENUM_CALLBACK = Option<unsafe extern "system" fn(*const LDR_DATA_TABLE_ENTRY_PREFIX, PVOID, *mut BOOLEAN)>;
#[no_mangle]
pub unsafe extern "system" fn LdrEnumerateLoadedModules(flags: ULONG, callback: PLDR_ENUM_CALLBACK, context: PVOID) -> NTSTATUS {
    if flags != 0 || callback.is_none() { return STATUS_INVALID_PARAMETER; }
    let Some(head) = (unsafe { current_peb_ldr_head() }) else { return STATUS_DLL_NOT_FOUND; };
    let head_addr = head as usize;
    let mut link = unsafe { ptr::read_unaligned(head.cast::<usize>()) };
    let mut count = 0usize;
    let callback = callback.unwrap();
    while link != 0 && link != head_addr && count < MAX_LOADER_ENTRIES {
        let entry = link as *const LDR_DATA_TABLE_ENTRY_PREFIX;
        let mut stop: BOOLEAN = 0;
        unsafe { callback(entry, context, &mut stop); }
        if stop != 0 { return STATUS_SUCCESS; }
        link = unsafe { ptr::read_unaligned(link as *const usize) };
        count += 1;
    }
    if count == MAX_LOADER_ENTRIES { STATUS_INVALID_PARAMETER } else { STATUS_SUCCESS }
}

#[no_mangle]
pub unsafe extern "system" fn LdrEnumResources() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrFastFailInLoaderCallout() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrFindEntryForAddress(address: PVOID, entry_out: *mut PVOID) -> NTSTATUS {
    if address.is_null() || entry_out.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe { entry_out.write(ptr::null_mut()); }
    let Some(head) = (unsafe { current_peb_ldr_head() }) else { return STATUS_DLL_NOT_FOUND; };
    let head_addr = head as usize;
    let mut link = unsafe { ptr::read_unaligned(head.cast::<usize>()) };
    let target = address as usize;
    let mut count = 0usize;
    while link != 0 && link != head_addr && count < MAX_LOADER_ENTRIES {
        let entry = link as *mut u8;
        let base = unsafe { ptr::read_unaligned(entry.add(0x30).cast::<usize>()) };
        let size = unsafe { ptr::read_unaligned(entry.add(0x40).cast::<u32>()) } as usize;
        if size != 0 && target >= base && target.checked_sub(base).is_some_and(|offset| offset < size) {
            unsafe { entry_out.write(entry.cast()); }
            return STATUS_SUCCESS;
        }
        link = unsafe { ptr::read_unaligned(entry.cast::<usize>()) };
        count += 1;
    }
    STATUS_NOT_FOUND
}







#[no_mangle]
pub unsafe extern "system" fn LdrFlushAlternateResourceModules() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrGetDllHandleByMapping(mapping_base: PVOID, entry_out: *mut PVOID) -> NTSTATUS {
    if mapping_base.is_null() || entry_out.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe { entry_out.write(ptr::null_mut()); }
    match unsafe { find_entry_by_base(mapping_base as usize) } {
        Some(entry) => { let base = unsafe { ptr::read_unaligned(entry.add(0x30).cast::<usize>()) }; unsafe { entry_out.write(base as PVOID); } STATUS_SUCCESS },
        None => STATUS_DLL_NOT_FOUND,
    }
}

#[no_mangle]
pub unsafe extern "system" fn LdrGetDllFullName(dll_handle: PVOID, full_dll_name: *mut UNICODE_STRING) -> NTSTATUS {
    if dll_handle.is_null() || full_dll_name.is_null() { return STATUS_INVALID_PARAMETER; }
    let Some(entry) = (unsafe { find_entry_by_base(dll_handle as usize) }) else { return STATUS_DLL_NOT_FOUND; };
    let source_length = unsafe { ptr::read_unaligned(entry.add(0x48).cast::<u16>()) };
    let source_maximum = unsafe { ptr::read_unaligned(entry.add(0x4a).cast::<u16>()) };
    let source_buffer = unsafe { ptr::read_unaligned(entry.add(0x50).cast::<usize>()) } as *mut u16;
    if source_length & 1 != 0 || source_length > source_maximum
        || (source_length != 0 && source_buffer.is_null()) { return STATUS_INVALID_PARAMETER; }
    let output = unsafe { &mut *full_dll_name };
    // The returned descriptor borrows the loader-owned name; it does not copy
    // into a caller-owned buffer or transfer ownership of the string.
    output.Length = source_length;
    output.MaximumLength = source_maximum;
    output.Buffer = source_buffer;
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn LdrGetDllHandleByName(
    base_name: *const UNICODE_STRING, full_name: *const UNICODE_STRING, dll_handle: *mut PVOID,
) -> NTSTATUS {
    if (base_name.is_null() && full_name.is_null()) || dll_handle.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe { dll_handle.write(ptr::null_mut()); }
    match unsafe { find_entry_by_name(base_name, full_name) } {
        Some(entry) => {
            let base = unsafe { ptr::read_unaligned(entry.add(0x30).cast::<usize>()) };
            unsafe { dll_handle.write(base as PVOID); }
            STATUS_SUCCESS
        },
        None => STATUS_DLL_NOT_FOUND,
    }
}

#[no_mangle]
pub unsafe extern "system" fn LdrGetDllHandleEx(
    flags: ULONG,
    dll_path: *const u16,
    dll_characteristics: *mut ULONG,
    dll_name: *const UNICODE_STRING,
    dll_handle: *mut PVOID,
) -> NTSTATUS {
    if flags & !0x3 != 0 || dll_name.is_null() || dll_handle.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let status = unsafe {
        crate::rtl_loader::LdrGetDllHandle(
            dll_path as PVOID, dll_characteristics, dll_name.cast(), dll_handle,
        )
    };
    if status != STATUS_SUCCESS || (flags & 1 != 0 && flags & 2 == 0) { return status; }
    unsafe { track_module_reference(*dll_handle as usize, flags & 2 != 0) }
}

#[no_mangle]
pub unsafe extern "system" fn LdrGetFailureData() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrGetFileNameFromLoadAsDataTable() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrGetKnownDllSectionHandle() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrGetProcedureAddressEx(
    dll_handle: PVOID,
    procedure_name: *const crate::rtl_loader::COGNAC_ANSI_STRING,
    procedure_number: ULONG,
    procedure_address: *mut PVOID,
    flags: ULONG,
) -> NTSTATUS {
    if flags != 0 { return STATUS_INVALID_PARAMETER; }
    unsafe {
        crate::rtl_loader::LdrGetProcedureAddress(
            dll_handle, procedure_name, procedure_number, procedure_address,
        )
    }
}

#[no_mangle]
pub unsafe extern "system" fn LdrGetProcedureAddressForCaller(
    dll_handle: PVOID,
    procedure_name: *const crate::rtl_loader::COGNAC_ANSI_STRING,
    procedure_number: ULONG,
    procedure_address: *mut PVOID,
    flags: ULONG,
    _caller_address: PVOID,
) -> NTSTATUS {
    if flags & !1 != 0 { return STATUS_INVALID_PARAMETER; }
    unsafe { crate::rtl_loader::LdrGetProcedureAddress(dll_handle, procedure_name, procedure_number, procedure_address) }
}

#[no_mangle]
pub unsafe extern "system" fn LdrHotPatchNotify() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrInitializeEnclave() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrInitializeThunk() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrInitShimEngineDynamic() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrIsModuleSxsRedirected(_module_name: *const UNICODE_STRING) -> BOOLEAN { 0 }

#[no_mangle]
pub unsafe extern "system" fn LdrLoadAlternateResourceModule() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrLoadAlternateResourceModuleEx() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrLoadDll(
    dll_path: *const u16, dll_characteristics: *mut ULONG,
    dll_name: *const UNICODE_STRING, dll_handle: *mut PVOID,
) -> NTSTATUS {
    let _ = super::super::exception::RtlRegisterExceptionDispatcher();
    if dll_name.is_null() || dll_handle.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe { dll_handle.write(ptr::null_mut()); }
    // Reuse an existing mapping without pretending that a missing module was
    // loaded. New-image mapping requires the not-yet-implemented native section
    // mapping/loader transaction bridge; fail closed until that exists.
    let status = unsafe { crate::rtl_loader::LdrGetDllHandle(dll_path as PVOID, dll_characteristics, dll_name.cast(), dll_handle) };
    if status == STATUS_SUCCESS { return unsafe { track_module_reference(*dll_handle as usize, false) }; }
    STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrLoadEnclaveModule() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrOpenImageFileOptionsKey() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrpResGetMappingSize() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrpResGetResourceDirectory() -> i32 { STATUS_NOT_IMPLEMENTED }

pub(super) static PROCESS_INITIALIZATION_COMPLETE: AtomicBool = AtomicBool::new(false);

#[no_mangle]
pub unsafe extern "system" fn LdrProcessInitializationComplete() -> NTSTATUS {
    PROCESS_INITIALIZATION_COMPLETE.store(true, Ordering::Release);
    STATUS_SUCCESS
}


