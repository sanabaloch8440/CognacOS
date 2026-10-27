use super::*;
use super::resources::{DLL_NOTIFICATIONS, MAX_DLL_NOTIFICATIONS, Notification};
use core::sync::atomic::{AtomicBool, AtomicUsize};

static DLL_SHUTDOWN_COMPLETE: AtomicBool = AtomicBool::new(false);
static DLL_SHUTDOWN_OWNER_TID: AtomicUsize = AtomicUsize::new(0);

struct ShutdownCompletion;
impl Drop for ShutdownCompletion {
    fn drop(&mut self) { DLL_SHUTDOWN_COMPLETE.store(true, Ordering::Release); }
}

fn current_loader_thread_id() -> usize {
    #[cfg(all(target_arch = "x86_64", any(target_os = "windows", target_os = "none")))]
    unsafe {
        let teb: *const u8;
        core::arch::asm!("mov {}, gs:[0x30]", out(reg) teb, options(nostack, preserves_flags, readonly));
        if teb.is_null() { return 0; }
        return ptr::read_unaligned(teb.add(0x48).cast::<usize>());
    }
    #[allow(unreachable_code)]
    0
}

#[no_mangle]
pub unsafe extern "system" fn LdrRegisterDllNotification(
    flags: ULONG, callback: Option<unsafe extern "system" fn(ULONG, *const c_void, PVOID)>,
    context: PVOID, cookie_out: *mut PVOID,
) -> NTSTATUS {
    if flags != 0 || callback.is_none() || cookie_out.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe { cookie_out.write(ptr::null_mut()); }
    let _guard = StateGuard::acquire();
    let entries = core::ptr::addr_of_mut!(DLL_NOTIFICATIONS).cast::<Notification>();
    for i in 0..MAX_DLL_NOTIFICATIONS {
        let entry = unsafe { &mut *entries.add(i) };
        if entry.cookie == 0 {
            let cookie = NEXT_COOKIE.fetch_add(1, Ordering::Relaxed).max(1);
            *entry = Notification { cookie, callback: callback.unwrap() as usize, context: context as usize };
            unsafe { cookie_out.write(cookie as PVOID); }
            return STATUS_SUCCESS;
        }
    }
    STATUS_RESOURCE_IN_USE
}

#[no_mangle]
pub unsafe extern "system" fn LdrUnregisterDllNotification(cookie: PVOID) -> NTSTATUS {
    let value = cookie as usize;
    if value == 0 { return STATUS_INVALID_PARAMETER; }
    let _guard = StateGuard::acquire();
    let entries = core::ptr::addr_of_mut!(DLL_NOTIFICATIONS).cast::<Notification>();
    for i in 0..MAX_DLL_NOTIFICATIONS {
        let entry = unsafe { &mut *entries.add(i) };
        if entry.cookie == value { *entry = Notification::EMPTY; return STATUS_SUCCESS; }
    }
    STATUS_INVALID_HANDLE
}

#[no_mangle]
pub unsafe extern "system" fn LdrShutdownProcess() -> NTSTATUS {
    if DLL_SHUTDOWN.swap(true, Ordering::AcqRel) {
        // Reentrant shutdown from a DllMain callback must not wait on itself.
        // Other threads wait for the single detach traversal to finish, so
        // process teardown cannot race a second traversal of the loader list.
        let tid = current_loader_thread_id();
        if tid == 0 || DLL_SHUTDOWN_OWNER_TID.load(Ordering::Acquire) == tid {
            return STATUS_SUCCESS;
        }
        while !DLL_SHUTDOWN_COMPLETE.load(Ordering::Acquire) { core::hint::spin_loop(); }
        return STATUS_SUCCESS;
    }
    DLL_SHUTDOWN_OWNER_TID.store(current_loader_thread_id(), Ordering::Release);
    let _completion = ShutdownCompletion;
    crate::native::etw::rundown_current_process();

    // LDR_DATA_TABLE_ENTRY_PREFIX is the documented x64 prefix used by the
    // loader's own enumeration path. The first load-order entry is the process
    // image; only DLL entry points receive DLL_PROCESS_DETACH.
    let Some(head) = (unsafe { current_peb_ldr_head() }) else { return STATUS_SUCCESS; };
    let head_addr=head as usize;
    let mut entries=[(0usize,0usize,0usize,0usize);MAX_LOADER_ENTRIES];
    let mut count=0usize;
    let mut link=unsafe{ptr::read_unaligned(head.cast::<usize>())};
    while link!=0 && link!=head_addr && count<MAX_LOADER_ENTRIES {
        let entry=link as *const super::LDR_DATA_TABLE_ENTRY_PREFIX;
        let (base,entry_point,size)=unsafe{
            ((*entry).DllBase as usize,(*entry).EntryPoint as usize,(*entry).SizeOfImage as usize)
        };
        if base!=0 && entry_point!=0 {
            if let Ok((image_base,image_size,_))=unsafe{mapped_image_info(base as PVOID)} {
                if entry_point>=image_base && entry_point<image_base.saturating_add(image_size)
                    && size!=0 && size<=image_size {
                    entries[count]=(base,entry_point,size,link);
                }
            }
        }
        count+=1;
        link=unsafe{ptr::read_unaligned(link as *const usize)};
    }
    if count>=MAX_LOADER_ENTRIES { return STATUS_INVALID_PARAMETER; }
    // Prefer the actual resolved IAT dependency graph over a guessed reverse
    // load-order traversal. If imports are unresolved or the graph is cyclic,
    // the bounded fallback preserves reverse load order for remaining entries.
    let order = loader_detach_order(&entries, count);
    for index in order.into_iter().take(count.saturating_sub(1)) {
        let (base,entry_point,_size,_ldr_entry)=entries[index];
        if index==0 || base==0 || entry_point==0 { continue; }
        let detach: unsafe extern "system" fn(PVOID,u32,PVOID)->i32 =
            unsafe{core::mem::transmute(entry_point)};
        // A nonzero DllMain result does not cancel process shutdown. True SEH
        // containment needs the user exception dispatcher; this call remains
        // guarded by image-range validation and the one-shot shutdown barrier.
        let _=unsafe{detach(base as PVOID,0,ptr::null_mut())};
    }
    STATUS_SUCCESS
}

/// Build a bounded detach order from the resolved PE import-address tables.
/// For A -> B (A imports an address inside B), A is detached before B.
fn loader_detach_order(
    entries: &[(usize, usize, usize, usize); MAX_LOADER_ENTRIES],
    count: usize,
) -> [usize; MAX_LOADER_ENTRIES] {
    let mut remaining=[false;MAX_LOADER_ENTRIES];
    let mut depends=[[false;MAX_LOADER_ENTRIES];MAX_LOADER_ENTRIES];
    for i in 1..count { remaining[i]=entries[i].0!=0; }
    for i in 1..count {
        let (base,_,size,_)=entries[i];
        if base==0 || size==0 { continue; }
        for j in 1..count {
            if i==j || entries[j].0==0 { continue; }
            depends[i][j]=module_imports_image(base,size,entries[j].0,entries[j].2);
        }
    }
    let mut result=[0usize;MAX_LOADER_ENTRIES];
    let mut written=0usize;
    while written<count.saturating_sub(1) {
        // A node with no remaining dependents is safe to detach now.
        let mut candidate=None;
        for i in (1..count).rev() {
            if !remaining[i] { continue; }
            let has_dependent=(1..count).any(|j|remaining[j]&&depends[j][i]);
            if !has_dependent { candidate=Some(i); break; }
        }
        let chosen=match candidate {
            Some(i)=>i,
            None=>match (1..count).rev().find(|i|remaining[*i]) {Some(i)=>i,None=>break},
        };
        result[written]=chosen;written+=1;remaining[chosen]=false;
    }
    if written<count.saturating_sub(1) {
        for i in (1..count).rev() { if remaining[i] { result[written]=i;written+=1; } }
    }
    result
}

fn module_imports_image(importer:usize, importer_size:usize, target:usize, target_size:usize)->bool {
    if importer==0||target==0||importer_size<0x100||target_size==0{return false;}
    let base=importer as *const u8;
    let read_u16=|offset:usize|->Option<u16>{
        if offset.checked_add(2)? > importer_size{return None;}
        Some(unsafe{ptr::read_unaligned(base.add(offset).cast::<u16>())})
    };
    let read_u32=|offset:usize|->Option<u32>{
        if offset.checked_add(4)? > importer_size{return None;}
        Some(unsafe{ptr::read_unaligned(base.add(offset).cast::<u32>())})
    };
    if read_u16(0)!=Some(0x5a4d){return false;}
    let nt=match read_u32(0x3c){Some(v)=>v as usize,None=>return false};
    if nt.checked_add(24).is_none_or(|end|end>importer_size)
        || read_u32(nt)!=Some(0x00004550){return false;}
    let optional=nt+24;
    if read_u16(optional)!=Some(0x20b){return false;}
    let optional_size=match read_u16(nt+20){Some(v)=>v as usize,None=>return false};
    if optional_size<128||optional.checked_add(optional_size).is_none_or(|end|end>importer_size){return false;}
    let directory_count=match read_u32(optional+108){Some(v)=>v as usize,None=>return false};
    if directory_count<=1{return false;}
    let import_rva=match read_u32(optional+120){Some(v)=>v as usize,None=>return false};
    let import_size=match read_u32(optional+124){Some(v)=>v as usize,None=>return false};
    if import_rva==0||import_size<20||import_rva.checked_add(import_size).is_none_or(|end|end>importer_size){return false;}
    let descriptor_count=core::cmp::min(import_size/20,1024);
    for d in 0..descriptor_count {
        let at=import_rva+d*20;
        let first_thunk=match read_u32(at+16){Some(v)=>v as usize,None=>return false};
        let name=match read_u32(at+12){Some(v)=>v,None=>return false};
        if first_thunk==0&&name==0{break;}
        if first_thunk==0||first_thunk>=importer_size{return false;}
        for t in 0..4096usize {
            let Some(offset)=first_thunk.checked_add(t*8) else{return false;};
            if offset.checked_add(8).is_none_or(|end|end>importer_size){return false;}
            let address=unsafe{ptr::read_unaligned(base.add(offset).cast::<usize>())};
            if address==0{break;}
            if address>=target&&address<target.saturating_add(target_size){return true;}
        }
    }
    false
}

#[no_mangle]
pub unsafe extern "system" fn RtlDllShutdownInProgress() -> BOOLEAN {
    DLL_SHUTDOWN.load(Ordering::Acquire) as BOOLEAN
}


/// Apply the AMD64 base-relocation entries for one image page.
#[no_mangle]
pub unsafe extern "system" fn LdrProcessRelocationBlock(
    va: ULONG_PTR, size_of_block: ULONG, next_offset: *mut u16, diff: isize,
) -> *mut u16 {
    if va == 0 || size_of_block & 1 != 0 || (size_of_block != 0 && next_offset.is_null()) {
        return ptr::null_mut();
    }
    if size_of_block == 0 { return next_offset; }
    let count = (size_of_block / 2) as usize;
    for index in 0..count {
        let item = unsafe { ptr::read_unaligned(next_offset.add(index)) };
        let kind = item >> 12;
        let offset = (item & 0x0fff) as usize;
        if kind == 0 { continue; } // IMAGE_REL_BASED_ABSOLUTE padding
        if kind != 10 { return ptr::null_mut(); } // IMAGE_REL_BASED_DIR64 only on AMD64
        let Some(address) = va.checked_add(offset) else { return ptr::null_mut(); };
        let location = address as *mut u64;
        let value = unsafe { ptr::read_unaligned(location) };
        unsafe { ptr::write_unaligned(location, value.wrapping_add(diff as u64)); }
    }
    unsafe { next_offset.add(count) }
}

#[no_mangle]
pub unsafe extern "system" fn LdrProcessRelocationBlockEx(
    machine: ULONG, va: ULONG_PTR, size_of_block: ULONG, next_offset: *mut u16, diff: isize,
) -> *mut u16 {
    if machine != 0x8664 { return ptr::null_mut(); } // IMAGE_FILE_MACHINE_AMD64
    unsafe { LdrProcessRelocationBlock(va, size_of_block, next_offset, diff) }
}



#[no_mangle]
pub unsafe extern "system" fn LdrQueryImageFileExecutionOptions() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrQueryImageFileExecutionOptionsEx() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrQueryImageFileKeyOption() -> i32 { STATUS_NOT_IMPLEMENTED }

#[no_mangle]
pub unsafe extern "system" fn LdrQueryModuleServiceTags() -> i32 { STATUS_NOT_IMPLEMENTED }

/// Probe whether an optional delay-loaded export is already available.
/// This query deliberately does not load a DLL: optional imports must not turn a
/// presence check into a loader-lock-sensitive recursive load.
#[no_mangle]
pub unsafe extern "system" fn LdrQueryOptionalDelayLoadedAPI(
    _parent_module_base: PVOID,
    dll_name: *const u8,
    procedure_name: *const u8,
    procedure_address: *mut PVOID,
) -> NTSTATUS {
    if dll_name.is_null() || procedure_name.is_null() || procedure_address.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    unsafe { procedure_address.write(ptr::null_mut()); }

    let mut dll_utf16 = [0u16; 260];
    let mut dll_len = 0usize;
    while dll_len < dll_utf16.len() - 1 {
        let ch = unsafe { ptr::read_unaligned(dll_name.add(dll_len)) };
        if ch == 0 { break; }
        if !ch.is_ascii() { return STATUS_INVALID_PARAMETER; }
        dll_utf16[dll_len] = ch as u16;
        dll_len += 1;
    }
    if dll_len == 0 || dll_len == dll_utf16.len() - 1 {
        return STATUS_INVALID_PARAMETER;
    }

    let mut procedure_ascii = [0u8; 512];
    let mut procedure_len = 0usize;
    while procedure_len < procedure_ascii.len() - 1 {
        let ch = unsafe { ptr::read_unaligned(procedure_name.add(procedure_len)) };
        if ch == 0 { break; }
        procedure_ascii[procedure_len] = ch;
        procedure_len += 1;
    }
    if procedure_len == 0 || procedure_len == procedure_ascii.len() - 1 {
        return STATUS_INVALID_PARAMETER;
    }

    let dll_descriptor = UNICODE_STRING {
        length: (dll_len * 2) as u16,
        maximum_length: (dll_len * 2) as u16,
        buffer: dll_utf16.as_mut_ptr(),
    };
    let mut module = ptr::null_mut();
    let status = unsafe {
        LdrGetDllHandleByName(&dll_descriptor, ptr::null(), &mut module)
    };
    if status != STATUS_SUCCESS { return STATUS_NOT_FOUND; }

    let procedure_descriptor = crate::rtl_loader::COGNAC_ANSI_STRING {
        Length: procedure_len as u16,
        MaximumLength: procedure_len as u16,
        Buffer: procedure_ascii.as_mut_ptr(),
    };
    let status = unsafe {
        crate::rtl_loader::LdrGetProcedureAddress(
            module, &procedure_descriptor, 0, procedure_address,
        )
    };
    if status == STATUS_SUCCESS && !unsafe { *procedure_address }.is_null() {
        STATUS_SUCCESS
    } else {
        unsafe { procedure_address.write(ptr::null_mut()); }
        STATUS_PROCEDURE_NOT_FOUND
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RTL_PROCESS_MODULE_INFORMATION {
    pub Section: PVOID,
    pub MappedBase: PVOID,
    pub ImageBase: PVOID,
    pub ImageSize: ULONG,
    pub Flags: ULONG,
    pub LoadOrderIndex: u16,
    pub InitOrderIndex: u16,
    pub LoadCount: u16,
    pub OffsetToFileName: u16,
    pub FullPathName: [u8; 256],
}
#[repr(C)]
pub struct RTL_PROCESS_MODULES {
    pub NumberOfModules: ULONG,
    pub Modules: [RTL_PROCESS_MODULE_INFORMATION; 1],
}
pub(super) const RTL_PROCESS_MODULE_INFORMATION_SIZE: usize = core::mem::size_of::<RTL_PROCESS_MODULE_INFORMATION>();

#[no_mangle]
pub unsafe extern "system" fn LdrQueryProcessModuleInformation(
    module_information: *mut RTL_PROCESS_MODULES,
    size: ULONG,
    returned_size: *mut ULONG,
) -> NTSTATUS {
    if returned_size.is_null() { return STATUS_INVALID_PARAMETER; }
    let Some(head) = (unsafe { current_peb_ldr_head() }) else {
        unsafe { returned_size.write(0); }
        return STATUS_DLL_NOT_FOUND;
    };
    let head_addr = head as usize;
    let mut link = unsafe { ptr::read_unaligned(head.cast::<usize>()) };
    let mut count = 0usize;
    while link != 0 && link != head_addr && count < MAX_LOADER_ENTRIES {
        count += 1;
        link = unsafe { ptr::read_unaligned((link as *const u8).cast::<usize>()) };
    }
    if count == MAX_LOADER_ENTRIES { return STATUS_INVALID_PARAMETER; }
    let module_array_offset = core::mem::offset_of!(RTL_PROCESS_MODULES, Modules);
    let required = module_array_offset
        .checked_add(count.checked_mul(RTL_PROCESS_MODULE_INFORMATION_SIZE).unwrap_or(usize::MAX))
        .filter(|n| *n <= u32::MAX as usize).ok_or(STATUS_INVALID_PARAMETER);
    let required = match required { Ok(n) => n, Err(e) => return e };
    unsafe { returned_size.write(required as ULONG); }
    if module_information.is_null() || (size as usize) < required { return STATUS_BUFFER_TOO_SMALL; }
    unsafe { ptr::write_unaligned(ptr::addr_of_mut!((*module_information).NumberOfModules), count as ULONG); }
    link = unsafe { ptr::read_unaligned(head.cast::<usize>()) };
    let output = unsafe { (module_information.cast::<u8>().add(module_array_offset)).cast::<RTL_PROCESS_MODULE_INFORMATION>() };
    let mut index = 0usize;
    while link != 0 && link != head_addr && index < count {
        let entry = link as *const u8;
        let base = unsafe { ptr::read_unaligned(entry.add(0x30).cast::<usize>()) };
        let image_size = unsafe { ptr::read_unaligned(entry.add(0x40).cast::<ULONG>()) };
        let name_length = unsafe { ptr::read_unaligned(entry.add(0x48).cast::<u16>()) } as usize;
        let name_buffer = unsafe { ptr::read_unaligned(entry.add(0x50).cast::<usize>()) } as *const u16;
        let mut info = RTL_PROCESS_MODULE_INFORMATION {
            Section: ptr::null_mut(), MappedBase: base as PVOID, ImageBase: base as PVOID,
            ImageSize: image_size, Flags: 0, LoadOrderIndex: index as u16, InitOrderIndex: 0,
            LoadCount: 1, OffsetToFileName: 0, FullPathName: [0; 256],
        };
        if name_length & 1 == 0 && name_length != 0 && !name_buffer.is_null() {
            let units = core::cmp::min(name_length / 2, 255);
            for i in 0..units {
                let ch = unsafe { ptr::read_unaligned(name_buffer.add(i)) };
                info.FullPathName[i] = if ch <= 0x7f { ch as u8 } else { b'?' };
                if ch == b'\\' as u16 || ch == b'/' as u16 { info.OffsetToFileName = (i + 1) as u16; }
            }
        }
        unsafe { ptr::write_unaligned(output.add(index), info); }
        link = unsafe { ptr::read_unaligned(entry.cast::<usize>()) };
        index += 1;
    }
    STATUS_SUCCESS
}



#[no_mangle]
pub unsafe extern "system" fn LdrRemoveLoadAsDataTable() -> i32 { STATUS_NOT_IMPLEMENTED }

/// Native resource lookup entry point. The resource tree is parsed from the mapped PE image;
/// malformed RVAs and missing type/name/language nodes are reported by find_resource_entry.
#[no_mangle]
pub unsafe extern "system" fn LdrResFindResource(
    module: PVOID, resource_info: *const LDR_RESOURCE_INFO, level: ULONG,
    resource_data_entry: *mut *mut IMAGE_RESOURCE_DATA_ENTRY,
) -> NTSTATUS {
    unsafe { LdrFindResource_U(module, resource_info, level, resource_data_entry) }
}

/// Find a resource directory node (rather than a leaf data entry) in a mapped PE image.
#[no_mangle]
pub unsafe extern "system" fn LdrResFindResourceDirectory(
    module: PVOID, resource_info: *const LDR_RESOURCE_INFO, level: ULONG,
    directory_out: *mut PVOID,
) -> NTSTATUS {
    if directory_out.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe { directory_out.write(ptr::null_mut()); }
    match unsafe { find_resource_entry(module, resource_info, level, true) } {
        Ok(directory) => { unsafe { directory_out.write(directory.cast()); } STATUS_SUCCESS },
        Err(status) => status,
    }
}

/// Return the resource-configuration directory only when it exists and is structurally valid.
/// RC configuration parsing is not guessed: this helper validates the resource root and reports
/// STATUS_NOT_FOUND for images without a resource directory.
#[no_mangle]
pub unsafe extern "system" fn LdrResGetRCConfig(module: PVOID, directory_out: *mut PVOID) -> NTSTATUS {
    if directory_out.is_null() || module.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe { directory_out.write(ptr::null_mut()); }
    match unsafe { mapped_image_info(module) } {
        Ok((base, _image_size, packed)) => {
            let rva = packed as u32 as usize;
            if rva == 0 { return STATUS_NOT_FOUND; }
            unsafe { directory_out.write((base + rva) as PVOID); }
            STATUS_SUCCESS
        }
        Err(status) => status,
    }
}


pub(super) const IMAGE_DIRECTORY_ENTRY_DELAY_IMPORT: usize = 13;
pub(super) const IMAGE_ORDINAL_FLAG64: u64 = 1u64 << 63;
pub(super) const MAX_DELAY_IMPORTS: usize = 4096;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct IMAGE_DELAYLOAD_DESCRIPTOR {
    pub Attributes: u32,
    pub DllNameRVA: u32,
    pub ModuleHandleRVA: u32,
    pub ImportAddressTableRVA: u32,
    pub ImportNameTableRVA: u32,
    pub BoundImportAddressTableRVA: u32,
    pub UnloadInformationTableRVA: u32,
    pub TimeDateStamp: u32,
}

pub(super) unsafe fn delay_directory(module: PVOID) -> Result<(usize, usize, usize), NTSTATUS> {
    let (base, image_size, _) = unsafe { mapped_image_info(module) }?;
    let raw = base as *const u8;
    let nt = unsafe { ptr::read_unaligned(raw.add(0x3c).cast::<u32>()) } as usize;
    let optional_size = unsafe { ptr::read_unaligned(raw.add(nt + 4 + 16).cast::<u16>()) } as usize;
    let optional = nt.checked_add(24).ok_or(STATUS_INVALID_PARAMETER)?;
    if optional_size < 112 + (IMAGE_DIRECTORY_ENTRY_DELAY_IMPORT + 1) * 8 {
        return Err(STATUS_NOT_FOUND);
    }
    let directory_count = unsafe { ptr::read_unaligned(raw.add(optional + 108).cast::<u32>()) } as usize;
    if directory_count <= IMAGE_DIRECTORY_ENTRY_DELAY_IMPORT { return Err(STATUS_NOT_FOUND); }
    let directory = optional + 112 + IMAGE_DIRECTORY_ENTRY_DELAY_IMPORT * 8;
    let rva = unsafe { ptr::read_unaligned(raw.add(directory).cast::<u32>()) } as usize;
    let size = unsafe { ptr::read_unaligned(raw.add(directory + 4).cast::<u32>()) } as usize;
    if rva == 0 || size < core::mem::size_of::<IMAGE_DELAYLOAD_DESCRIPTOR>()
        || rva.checked_add(size).is_none_or(|end| end > image_size) {
        return Err(STATUS_NOT_FOUND);
    }
    Ok((base, image_size, rva))
}

pub(super) unsafe fn image_ascii_string(base: usize, image_size: usize, rva: usize, out: &mut [u8]) -> Result<usize, NTSTATUS> {
    if rva >= image_size || out.is_empty() { return Err(STATUS_INVALID_PARAMETER); }
    for i in 0..core::cmp::min(out.len(), image_size - rva) {
        let ch = unsafe { ptr::read_unaligned((base + rva + i) as *const u8) };
        if ch == 0 { return Ok(i); }
        if !ch.is_ascii() { return Err(STATUS_INVALID_PARAMETER); }
        out[i] = ch;
    }
    Err(STATUS_INVALID_PARAMETER)
}

