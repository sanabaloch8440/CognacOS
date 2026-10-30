use super::*;
use super::notifications::{image_ascii_string, IMAGE_ORDINAL_FLAG64};

#[repr(C)] struct LoaderLink { flink: *mut LoaderLink, blink: *mut LoaderLink }
#[repr(C)] struct LdrEntry {
    load: LoaderLink, memory: LoaderLink, init: LoaderLink,
    base: PVOID, entry: PVOID, size: ULONG, _pad: ULONG,
    full_name: UNICODE_STRING, base_name: UNICODE_STRING,
}
#[repr(C)] struct LoadDllResponse {
    base: u64, entry: u64, slot: u32, generation: u32, already_loaded: u32,
    path_len: u32, path: [u8; 260],
}
unsafe fn list_insert_tail(head: *mut LoaderLink, item: *mut LoaderLink) -> bool {
    if head.is_null() || item.is_null() { return false; }
    let tail = unsafe { (*head).blink };
    if tail.is_null() { return false; }
    unsafe { (*item).flink=head; (*item).blink=tail; (*tail).flink=item; (*head).blink=item; }
    true
}
unsafe fn publish_peb_entry(response: &LoadDllResponse, name: &[u8]) -> *mut LdrEntry {
    let Some(head0) = (unsafe { current_peb_ldr_head() }) else { return ptr::null_mut(); };
    let ldr = unsafe { head0.sub(0x10) };
    let head0 = head0.cast::<LoaderLink>() as *mut LoaderLink;
    let head1 = unsafe { ldr.add(0x20) }.cast::<LoaderLink>() as *mut LoaderLink;
    let head2 = unsafe { ldr.add(0x30) }.cast::<LoaderLink>() as *mut LoaderLink;
    let full_len = response.path_len as usize;
    if full_len > response.path.len() || name.is_empty() { return ptr::null_mut(); }
    let bytes = match core::mem::size_of::<LdrEntry>().checked_add((full_len + name.len()) * 2) {
        Some(n) => n, None => return ptr::null_mut(),
    };
    let heap = unsafe { crate::rtl_heap::RtlGetProcessHeap() };
    if heap.is_null() { return ptr::null_mut(); }
    let allocation = unsafe { crate::rtl_heap::RtlAllocateHeap(heap, 8, bytes) } as *mut u8;
    if allocation.is_null() { return ptr::null_mut(); }
    let entry = allocation.cast::<LdrEntry>();
    let full = unsafe { allocation.add(core::mem::size_of::<LdrEntry>()) }.cast::<u16>();
    let base = unsafe { full.add(full_len) };
    for i in 0..full_len {
        if !response.path[i].is_ascii() {
            let _ = unsafe { crate::rtl_heap::RtlFreeHeap(heap, 0, allocation.cast()) };
            return ptr::null_mut();
        }
        unsafe { full.add(i).write(response.path[i] as u16); }
    }
    for (i, ch) in name.iter().copied().enumerate() { unsafe { base.add(i).write(ch as u16); } }
    unsafe {
        ptr::write(entry, LdrEntry {
            load: LoaderLink { flink: ptr::null_mut(), blink: ptr::null_mut() },
            memory: LoaderLink { flink: ptr::null_mut(), blink: ptr::null_mut() },
            init: LoaderLink { flink: ptr::null_mut(), blink: ptr::null_mut() },
            base: response.base as PVOID, entry: response.entry as PVOID,
            size: 0, _pad: 0,
            full_name: UNICODE_STRING { length: (full_len * 2) as u16, maximum_length: (full_len * 2) as u16, buffer: full },
            base_name: UNICODE_STRING { length: (name.len() * 2) as u16, maximum_length: (name.len() * 2) as u16, buffer: base },
        });
        let image = response.base as *const u8;
        let nt = ptr::read_unaligned(image.add(0x3c).cast::<u32>()) as usize;
        (*entry).size = ptr::read_unaligned(image.add(nt + 80).cast::<u32>());
        if !list_insert_tail(head0, ptr::addr_of_mut!((*entry).load))
            || !list_insert_tail(head1, ptr::addr_of_mut!((*entry).memory))
            || !list_insert_tail(head2, ptr::addr_of_mut!((*entry).init)) {
            let _ = crate::rtl_heap::RtlFreeHeap(heap, 0, allocation.cast());
            return ptr::null_mut();
        }
    }
    entry
}
unsafe fn remove_peb_entry(base: PVOID) {
    let Some(entry) = (unsafe { find_entry_by_base(base as usize) }) else { return; };
    let heap = unsafe { crate::rtl_heap::RtlGetProcessHeap() };
    unsafe {
        for offset in [0usize, 0x10, 0x20] {
            let link = entry.add(offset).cast::<LoaderLink>();
            let forward = (*link).flink;
            let backward = (*link).blink;
            if !forward.is_null() && !backward.is_null() {
                (*backward).flink = forward;
                (*forward).blink = backward;
            }
        }
        if !heap.is_null() { let _ = crate::rtl_heap::RtlFreeHeap(heap, 0, entry.cast()); }
    }
}

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
    output.length = source_length;
    output.maximum_length = source_maximum;
    output.buffer = source_buffer;
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
    let descriptor = unsafe { &*dll_name };
    if descriptor.length == 0 || descriptor.length & 1 != 0
        || descriptor.length > descriptor.maximum_length || descriptor.buffer.is_null()
        || descriptor.length as usize / 2 >= MAX_PATH_UNITS { return STATUS_INVALID_PARAMETER; }
    let units = unsafe { core::slice::from_raw_parts(descriptor.buffer, descriptor.length as usize / 2) };
    let mut name = [0u8; 96];
    let mut name_len = 0usize;
    for unit in units {
        if *unit == 0 || *unit > 0x7f { return STATUS_INVALID_PARAMETER; }
        let byte = *unit as u8;
        if byte == b'\\' || byte == b'/' { name_len = 0; continue; }
        if name_len == name.len() { return STATUS_INVALID_PARAMETER; }
        name[name_len] = byte; name_len += 1;
    }
    if name_len == 0 { return STATUS_INVALID_PARAMETER; }
    if !name[..name_len].contains(&b'.') {
        if name_len + 4 >= name.len() { return STATUS_INVALID_PARAMETER; }
        name[name_len..name_len + 4].copy_from_slice(b".dll"); name_len += 4;
    }
    let module_name = &name[..name_len];
    let existing = unsafe { crate::rtl_loader::LdrGetDllHandle(dll_path as PVOID, dll_characteristics, dll_name.cast(), dll_handle) };
    if existing == STATUS_SUCCESS { return unsafe { track_module_reference(*dll_handle as usize, false) }; }

    #[cfg(not(target_os = "windows"))]
    { let _ = dll_path; return STATUS_NOT_IMPLEMENTED; }

    #[cfg(target_os = "windows")]
    {
    let mut path = [0u8; 260];
    let mut path_len = 0usize;
    if let Some((directory, length)) = unsafe { super::search::process_image_directory() } {
        if length + name_len < path.len() {
            for i in 0..length {
                if directory[i] > 0x7f { return STATUS_INVALID_PARAMETER; }
                path[i] = directory[i] as u8;
            }
            path_len = length;
            if path_len != 0 && path[path_len - 1] != b'\\' && path[path_len - 1] != b'/' {
                path[path_len] = b'\\'; path_len += 1;
            }
            path[path_len..path_len + name_len].copy_from_slice(module_name);
            path_len += name_len; path[path_len] = 0;
        }
    }
    let _ = dll_path;
    let mut response = LoadDllResponse { base:0, entry:0, slot:0, generation:0, already_loaded:0, path_len:0, path:[0;260] };
    let status = unsafe {
        crate::native::syscall_stubs::cognacos_nt_load_dll(
            module_name.as_ptr(), if path_len == 0 { ptr::null() } else { path.as_ptr() },
            (&mut response as *mut LoadDllResponse).cast(),
        )
    };
    if status < 0 { return status; }
    if response.base == 0 || response.path_len as usize > response.path.len() { return STATUS_INVALID_PARAMETER; }
    if unsafe { publish_peb_entry(&response, module_name) }.is_null() {
        if response.already_loaded == 0 {
            let _ = unsafe { crate::native::syscall_stubs::cognacos_nt_complete_dll_init(
                module_name.as_ptr(), response.slot, response.generation, 0,
            ) };
        }
        return 0xC000_0017u32 as i32;
    }
    if response.already_loaded == 0 {
        if response.entry != 0 {
            type DllEntry = unsafe extern "system" fn(PVOID, u32, PVOID) -> BOOLEAN;
            let entry: DllEntry = unsafe { core::mem::transmute(response.entry as usize) };
            if unsafe { entry(response.base as PVOID, 1, ptr::null_mut()) } == 0 {
                let _ = unsafe { crate::native::syscall_stubs::cognacos_nt_complete_dll_init(module_name.as_ptr(), response.slot, response.generation, 0) };
                unsafe { remove_peb_entry(response.base as PVOID); }
                return STATUS_DLL_INIT_FAILED;
            }
        }
        let complete = unsafe { crate::native::syscall_stubs::cognacos_nt_complete_dll_init(module_name.as_ptr(), response.slot, response.generation, 1) };
        if complete < 0 { unsafe { remove_peb_entry(response.base as PVOID); } return complete; }
    }
    unsafe { dll_handle.write(response.base as PVOID); }
    STATUS_SUCCESS
    }
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


