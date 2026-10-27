use super::*;

#[no_mangle]
pub unsafe extern "system" fn LdrAddDllDirectory(directory: *const UNICODE_STRING, cookie_out: *mut PVOID) -> NTSTATUS {
    if cookie_out.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe { cookie_out.write(ptr::null_mut()); }
    let mut candidate = PathSlot::EMPTY;
    let status = unsafe { copy_absolute_path(directory, &mut candidate) };
    if status != STATUS_SUCCESS { return status; }
    let _guard = StateGuard::acquire();
    let mut i = 0;
    while i < MAX_DLL_DIRECTORIES {
        if !unsafe { USER_DIRECTORIES[i].active } {
            let id = NEXT_COOKIE.fetch_add(1, Ordering::Relaxed).max(1) & (usize::MAX >> 8);
            candidate.generation = id;
            unsafe { USER_DIRECTORIES[i] = candidate; }
            unsafe { cookie_out.write((id << 8 | (i + 1)) as PVOID); }
            return STATUS_SUCCESS;
        }
        i += 1;
    }
    STATUS_RESOURCE_IN_USE
}

#[no_mangle]
pub unsafe extern "system" fn LdrRemoveDllDirectory(cookie: PVOID) -> NTSTATUS {
    let raw = cookie as usize;
    let slot = raw & 0xff;
    if slot == 0 || slot > MAX_DLL_DIRECTORIES { return STATUS_INVALID_HANDLE; }
    let _guard = StateGuard::acquire();
    let entry = unsafe { &USER_DIRECTORIES[slot - 1] };
    if !entry.active || entry.generation != (raw >> 8) { return STATUS_INVALID_HANDLE; }
    unsafe { USER_DIRECTORIES[slot - 1] = PathSlot::EMPTY; }
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn LdrSetDefaultDllDirectories(flags: ULONG) -> NTSTATUS {
    if flags == 0 || flags & !VALID_SEARCH_FLAGS != 0 { return STATUS_INVALID_PARAMETER; }
    let _guard = StateGuard::acquire();
    unsafe { DEFAULT_SEARCH_FLAGS = flags; }
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn LdrSetDllDirectory(directory: *const UNICODE_STRING) -> NTSTATUS {
    let mut candidate = PathSlot::EMPTY;
    if !directory.is_null() {
        let status = unsafe { copy_absolute_path(directory, &mut candidate) };
        if status != STATUS_SUCCESS { return status; }
    }
    let _guard = StateGuard::acquire();
    unsafe { DLL_DIRECTORY = candidate; }
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn LdrGetDllDirectory(path_name: *mut UNICODE_STRING) -> NTSTATUS {
    if path_name.is_null() { return STATUS_INVALID_PARAMETER; }
    let _guard = StateGuard::acquire();
    let path = unsafe { &DLL_DIRECTORY };
    let bytes = path.len.saturating_mul(2);
    if bytes > u16::MAX as usize { return STATUS_BUFFER_TOO_SMALL; }
    let descriptor = unsafe { &mut *path_name };
    // This native API accepts a UNICODE_STRING descriptor, not a separate
    // caller buffer/length pair. Return a borrowed pointer to process-local
    // loader storage, matching the one-argument ABI.
    descriptor.length = bytes as u16;
    descriptor.maximum_length = bytes as u16;
    descriptor.buffer = if path.len == 0 { ptr::null_mut() } else { path.data.as_ptr() as *mut u16 };
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn LdrLockLoaderLock(flags: ULONG, disposition: *mut ULONG, cookie: *mut ULONG_PTR) -> NTSTATUS {
    if flags & !0x3 != 0 || disposition.is_null() || cookie.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe { disposition.write(0); cookie.write(0); }
    if flags & 0x2 != 0 {
        if LOADER_LOCK.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok() {
            let token = NEXT_COOKIE.fetch_add(1, Ordering::Relaxed).max(1);
            ACTIVE_LOADER_COOKIE.store(token, Ordering::Release);
            unsafe { disposition.write(1); cookie.write(token); }
        } else { unsafe { disposition.write(2); } }
        return STATUS_SUCCESS;
    }
    while LOADER_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
    let token = NEXT_COOKIE.fetch_add(1, Ordering::Relaxed).max(1);
    ACTIVE_LOADER_COOKIE.store(token, Ordering::Release);
    unsafe { disposition.write(1); cookie.write(token); }
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn LdrUnlockLoaderLock(flags: ULONG, cookie: ULONG_PTR) -> NTSTATUS {
    if flags != 0 || cookie == 0 || ACTIVE_LOADER_COOKIE.load(Ordering::Acquire) != cookie { return STATUS_INVALID_PARAMETER; }
    if LOADER_LOCK.compare_exchange(true, false, Ordering::Release, Ordering::Relaxed).is_err() {
        return STATUS_INVALID_PARAMETER;
    }
    ACTIVE_LOADER_COOKIE.store(0, Ordering::Release);
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn LdrDisableThreadCalloutsForDll(module_base: PVOID) -> NTSTATUS {
    if module_base.is_null() { return STATUS_INVALID_PARAMETER; }
    let _guard = StateGuard::acquire();
    let address = module_base as usize;
    let mut empty = None;
    let mut i = 0;
    while i < MAX_DISABLED_CALLOUT_MODULES {
        let value = unsafe { DISABLED_CALLOUT_MODULES[i] };
        if value == address { return STATUS_SUCCESS; }
        if value == 0 && empty.is_none() { empty = Some(i); }
        i += 1;
    }
    if let Some(index) = empty {
        unsafe { DISABLED_CALLOUT_MODULES[index] = address; }
        STATUS_SUCCESS
    } else { STATUS_RESOURCE_IN_USE }
}

pub(super) const LOAD_WITH_ALTERED_SEARCH_PATH: ULONG = 0x0000_0008;
pub(super) const MAX_BUILT_SEARCH_PATH_UNITS: usize = 2048;

pub(super) fn append_search_component(out: &mut [u16; MAX_BUILT_SEARCH_PATH_UNITS], used: &mut usize, component: &[u16]) -> bool {
    if component.is_empty() { return true; }
    let separator = usize::from(*used != 0);
    let Some(end) = used.checked_add(separator).and_then(|n| n.checked_add(component.len())) else { return false; };
    if end >= out.len() { return false; }
    if separator != 0 { out[*used] = b';' as u16; *used += 1; }
    out[*used..*used + component.len()].copy_from_slice(component);
    *used += component.len();
    true
}

pub(super) unsafe fn process_image_directory() -> Option<([u16; MAX_PATH_UNITS], usize)> {
    let peb: *const u8;
    unsafe { core::arch::asm!("mov {}, gs:[0x60]", out(reg) peb, options(nostack, preserves_flags, readonly)); }
    if peb.is_null() { return None; }
    let params = unsafe { ptr::read_unaligned(peb.add(0x20).cast::<usize>()) } as *const u8;
    if params.is_null() { return None; }
    // RTL_USER_PROCESS_PARAMETERS.ImagePathName on x64 is at +0x60.
    let descriptor = unsafe { params.add(0x60) };
    let bytes = unsafe { ptr::read_unaligned(descriptor.cast::<u16>()) } as usize;
    let maximum = unsafe { ptr::read_unaligned(descriptor.add(2).cast::<u16>()) } as usize;
    let buffer = unsafe { ptr::read_unaligned(descriptor.add(8).cast::<usize>()) } as *const u16;
    if bytes == 0 || bytes & 1 != 0 || bytes > maximum || bytes / 2 >= MAX_PATH_UNITS || buffer.is_null() { return None; }
    let count = bytes / 2;
    let mut path = [0u16; MAX_PATH_UNITS];
    for i in 0..count { path[i] = unsafe { ptr::read_unaligned(buffer.add(i)) }; }
    let mut end = count;
    while end > 0 && path[end - 1] != b'\\' as u16 && path[end - 1] != b'/' as u16 { end -= 1; }
    if end == 0 { return None; }
    Some((path, end))
}

pub(super) unsafe fn dll_load_directory(dll_name: *const u16) -> Option<([u16; MAX_PATH_UNITS], usize)> {
    if dll_name.is_null() { return None; }
    let mut path = [0u16; MAX_PATH_UNITS];
    let mut count = 0usize;
    while count < MAX_PATH_UNITS {
        let ch = unsafe { ptr::read_unaligned(dll_name.add(count)) };
        if ch == 0 { break; }
        path[count] = ch;
        count += 1;
    }
    if count == 0 || count == MAX_PATH_UNITS { return None; }
    let mut end = count;
    while end > 0 && path[end - 1] != b'\\' as u16 && path[end - 1] != b'/' as u16 { end -= 1; }
    if end == 0 { return None; }
    Some((path, end))
}

#[no_mangle]
pub unsafe extern "system" fn LdrGetDllPath(
    dll_name: *const u16, flags: ULONG, dll_path: *mut *mut u16, unknown: *mut *mut u16,
) -> NTSTATUS {
    if dll_name.is_null() || dll_path.is_null() || unknown.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe { dll_path.write(ptr::null_mut()); unknown.write(ptr::null_mut()); }
    let search_mask = VALID_SEARCH_FLAGS | LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR;
    if flags & !(search_mask | LOAD_WITH_ALTERED_SEARCH_PATH) != 0
        || (flags & LOAD_WITH_ALTERED_SEARCH_PATH != 0 && flags & search_mask != 0) {
        return STATUS_INVALID_PARAMETER;
    }
    let mut effective = flags;
    if effective & LOAD_WITH_ALTERED_SEARCH_PATH != 0 {
        let _guard = StateGuard::acquire();
        effective |= unsafe { DEFAULT_SEARCH_FLAGS } | LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR;
    } else if effective & search_mask == 0 {
        let _guard = StateGuard::acquire();
        effective |= unsafe { DEFAULT_SEARCH_FLAGS };
    }
    let default_dirs = effective & LOAD_LIBRARY_SEARCH_DEFAULT_DIRS != 0;
    let mut result = [0u16; MAX_BUILT_SEARCH_PATH_UNITS];
    let mut used = 0usize;

    if effective & LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR != 0 {
        if let Some((path, len)) = unsafe { dll_load_directory(dll_name) } {
            if !append_search_component(&mut result, &mut used, &path[..len]) { return STATUS_BUFFER_TOO_SMALL; }
        }
    }
    if effective & LOAD_LIBRARY_SEARCH_APPLICATION_DIR != 0 || default_dirs {
        if let Some((path, len)) = unsafe { process_image_directory() } {
            if !append_search_component(&mut result, &mut used, &path[..len]) { return STATUS_BUFFER_TOO_SMALL; }
        }
    }
    if effective & LOAD_LIBRARY_SEARCH_USER_DIRS != 0 || default_dirs {
        let (dll_dir, user_dirs) = {
            let _guard = StateGuard::acquire();
            (unsafe { DLL_DIRECTORY }, unsafe { USER_DIRECTORIES })
        };
        if dll_dir.active && !append_search_component(&mut result, &mut used, &dll_dir.data[..dll_dir.len]) {
            return STATUS_BUFFER_TOO_SMALL;
        }
        for directory in user_dirs.iter().filter(|entry| entry.active) {
            if !append_search_component(&mut result, &mut used, &directory.data[..directory.len]) {
                return STATUS_BUFFER_TOO_SMALL;
            }
        }
    }
    if effective & LOAD_LIBRARY_SEARCH_SYSTEM32 != 0 || default_dirs {
        const SYSTEM32: [u16; 22] = [
            b'\\' as u16,b'S' as u16,b'y' as u16,b's' as u16,b't' as u16,b'e' as u16,b'm' as u16,b'R' as u16,b'o' as u16,b'o' as u16,b't' as u16,
            b'\\' as u16,b'S' as u16,b'y' as u16,b's' as u16,b't' as u16,b'e' as u16,b'm' as u16,b'3' as u16,b'2' as u16,0,0
        ];
        if !append_search_component(&mut result, &mut used, &SYSTEM32[..20]) { return STATUS_BUFFER_TOO_SMALL; }
    }
    let heap = unsafe { crate::rtl_heap::RtlGetProcessHeap() };
    if heap.is_null() { return 0xC000_0017u32 as i32; }
    let bytes = match used.checked_add(1).and_then(|n| n.checked_mul(2)) { Some(n) => n, None => return STATUS_BUFFER_TOO_SMALL };
    let allocation = unsafe { crate::rtl_heap::RtlAllocateHeap(heap, 0, bytes) } as *mut u16;
    if allocation.is_null() { return 0xC000_0017u32 as i32; }
    unsafe {
        ptr::copy_nonoverlapping(result.as_ptr(), allocation, used);
        allocation.add(used).write(0);
        dll_path.write(allocation);
    }
    STATUS_SUCCESS
}



