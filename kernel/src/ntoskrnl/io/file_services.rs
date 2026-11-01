//! Read-only VFS provider routed through WDM FILE_OBJECT and IRP dispatch.
//!
//! Registered immutable boot assets are exposed by an internal read-only device.
//! Open/read/cleanup/close are represented by IRP_MJ_CREATE, IRP_MJ_READ,
//! IRP_MJ_CLEANUP and IRP_MJ_CLOSE. Object Manager owns the process handles;
//! the local slot array supplies only stable FILE_OBJECT backing storage.

use crate::hal::IrqSpinLock;
use crate::wdm::{self, DriverObject, DeviceObject, Irp, IRP_MJ_CREATE, IRP_MJ_READ, IRP_MJ_CLEANUP, IRP_MJ_CLOSE};
use crate::nt_abi::io::FILE_OBJECT;
use crate::ntoskrnl::ob::runtime as ob;
use crate::nt_abi::object::UNICODE_STRING;
use core::ffi::c_void;
use core::ptr;
use core::sync::atomic::{AtomicUsize, Ordering};

pub const MAX_FILES: usize = 32;
pub const MAX_FILE_OBJECTS: usize = 64;
pub const MAX_PATH_BYTES: usize = 260;
pub const STATUS_SUCCESS: i32 = 0;
pub const STATUS_OBJECT_NAME_NOT_FOUND: i32 = 0xC000_0034u32 as i32;
pub const STATUS_INVALID_HANDLE: i32 = 0xC000_0008u32 as i32;
pub const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
pub const STATUS_END_OF_FILE: i32 = 0xC000_0011u32 as i32;
pub const STATUS_INSUFFICIENT_RESOURCES: i32 = 0xC000_009Au32 as i32;
pub const STATUS_DEVICE_BUSY: i32 = 0x8000_0011u32 as i32;
const STATUS_UNRECOGNIZED_VOLUME:i32=0xC000_014F_u32 as i32;
const FILE_DEVICE_DISK_FILE_SYSTEM:u32=8;
const IRP_MN_MOUNT_VOLUME:u8=1;
const IRP_MN_VERIFY_VOLUME:u8=0;
const IRP_MN_USER_FS_REQUEST:u8=0;
const STATUS_INVALID_DEVICE_REQUEST: i32 = 0xC000_0010u32 as i32;
const IO_TYPE_FILE: i16 = 5;
const FILE_DEVICE_DISK: u32 = 7;
const FILE_USE_FILE_POINTER_POSITION: i64 = -2;

#[derive(Clone, Copy)]
struct FileRecord {
    path: [u8; MAX_PATH_BYTES],
    path_len: usize,
    address: usize,
    length: usize,
    generation: usize,
    source: u8,
    used: bool,
}
impl FileRecord {
    const EMPTY: Self = Self { path: [0; MAX_PATH_BYTES], path_len: 0, address: 0, length: 0, generation: 0, source: 0, used: false };
}

const fn empty_file_object() -> FILE_OBJECT {
    FILE_OBJECT {
        type_: IO_TYPE_FILE,
        size: core::mem::size_of::<FILE_OBJECT>() as i16,
        device_object: ptr::null_mut(),
        vpb: ptr::null_mut(),
        fs_context: ptr::null_mut(),
        fs_context2: ptr::null_mut(),
        section_object_pointer: ptr::null_mut(),
        private_cache_map: ptr::null_mut(),
        final_status: 0,
        related_file_object: ptr::null_mut(),
        lock_operation: 0,
        delete_pending: 0,
        read_access: 1,
        write_access: 0,
        delete_access: 0,
        shared_read: 1,
        shared_write: 0,
        shared_delete: 0,
        flags: crate::nt_abi::io::FO_SYNCHRONOUS_IO,
        file_name: UNICODE_STRING { length: 0, maximum_length: 0, buffer: ptr::null_mut() },
        current_byte_offset: 0,
        waiters: 0,
        busy: 0,
        last_lock: ptr::null_mut(),
        lock: [0; 0x18],
        event: [0; 0x18],
        completion_context: ptr::null_mut(),
        irp_list_lock: 0,
        irp_list: [0; 2],
        file_object_extension: ptr::null_mut(),
    }
}

#[derive(Clone, Copy)]
struct FileObjectRecord {
    active_io: u32,
    closing: bool,
    used: bool,
    file_object: *mut FILE_OBJECT,
}
impl FileObjectRecord {
    const EMPTY: Self = Self {
        active_io: 0, closing: false, used: false,
        file_object: ptr::null_mut(),
    };
}

struct State {
    files: [FileRecord; MAX_FILES],
    file_objects: [FileObjectRecord; MAX_FILE_OBJECTS],
    next_generation: usize,
}
impl State {
    const fn new() -> Self {
        Self { files: [FileRecord::EMPTY; MAX_FILES], file_objects: [FileObjectRecord::EMPTY; MAX_FILE_OBJECTS], next_generation: 1 }
    }
}
unsafe impl Send for State {}
static STATE: IrqSpinLock<State> = IrqSpinLock::new(State::new());
static BOOT_INIT: AtomicUsize = AtomicUsize::new(0);
static DEVICE_INIT: AtomicUsize = AtomicUsize::new(0);
static FILE_DEVICE: AtomicUsize = AtomicUsize::new(0);
static FILE_SYSTEM_DEVICE:AtomicUsize=AtomicUsize::new(0);
static FILE_SYSTEM_INIT:AtomicUsize=AtomicUsize::new(0);
static FILE_OBJECT_TYPE: AtomicUsize = AtomicUsize::new(0);
static VOLUME_DEVICE:AtomicUsize=AtomicUsize::new(0);

unsafe extern "win64" fn file_dispatch(device: *mut DeviceObject, irp: *mut Irp) -> i32 {
    if device.is_null() || irp.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    let stack = unsafe { wdm::io_get_current_irp_stack_location(irp) };
    if stack.is_null() {
        unsafe { wdm::io_complete_irp_status(irp, STATUS_INVALID_PARAMETER, 0, 0); }
        return STATUS_INVALID_PARAMETER;
    }
    let file_object = unsafe { (*stack).file_object.cast::<FILE_OBJECT>() };
    let major = unsafe { (*stack).major_function };
    let mut status = STATUS_SUCCESS;
    let mut information = 0usize;

    match major {
        IRP_MJ_CREATE => {
            if file_object.is_null() {
                status = STATUS_INVALID_PARAMETER;
            } else {
                let index = unsafe { (*file_object).fs_context as usize };
                let state = STATE.lock();
                if index == 0 || index > MAX_FILES || !state.files[index - 1].used {
                    status = STATUS_OBJECT_NAME_NOT_FOUND;
                } else if unsafe { (*file_object).device_object } != device.cast() {
                    status = STATUS_INVALID_PARAMETER;
                }
            }
        }
        IRP_MJ_READ => {
            if file_object.is_null() {
                status = STATUS_INVALID_PARAMETER;
            } else {
                                let length = unsafe { wdm::io_read_stack_u32(stack, 0) as usize };
                let requested_offset = unsafe { wdm::io_read_stack_i64(stack, 8) };
                let output = unsafe { wdm::io_get_irp_user_buffer(irp).cast::<u8>() };
                let index = unsafe { (*file_object).fs_context as usize };
                if index == 0 || index > MAX_FILES || (length != 0 && output.is_null()) {
                    status = STATUS_INVALID_PARAMETER;
                } else {
                    let file={let state=STATE.lock();if !state.files[index-1].used{None}else{Some(state.files[index-1])}};
                    if let Some(file)=file{
                     let offset=if requested_offset==FILE_USE_FILE_POINTER_POSITION{unsafe{(*file_object).current_byte_offset}}else{requested_offset};
                     if offset<0{status=STATUS_INVALID_PARAMETER;}
                     else if offset as usize>=file.length{status=STATUS_END_OF_FILE;}
                     else{
                      let count=core::cmp::min(length,file.length-offset as usize);
                      if count!=0{
                       if file.source==0{unsafe{ptr::copy_nonoverlapping((file.address+offset as usize) as *const u8,output,count);}information=count;}
                       else if !output.is_null(){
                        let out=unsafe{core::slice::from_raw_parts_mut(output,count)};
                        match crate::ata_disk::read_ext4_root_file_range(&file.path[..file.path_len],offset as u64,out){
                         Ok((size,read)) if size==file.length as u64=>information=read,
                         _=>status=STATUS_OBJECT_NAME_NOT_FOUND,
                        }
                       }else{status=STATUS_INVALID_PARAMETER;}
                      }
                      if status>=0&&requested_offset==FILE_USE_FILE_POINTER_POSITION{unsafe{(*file_object).current_byte_offset=offset+information as i64;}}
                     }
                    }else{status=STATUS_INVALID_HANDLE;}
                }
            }
        }
        IRP_MJ_CLEANUP => {
            if file_object.is_null() {
                status = STATUS_INVALID_PARAMETER;
            } else {
                unsafe { (*file_object).flags |= crate::nt_abi::io::FO_CLEANUP_COMPLETE; }
            }
        }
        IRP_MJ_CLOSE => {
            if file_object.is_null() {
                status = STATUS_INVALID_PARAMETER;
            } else {
                unsafe {
                    (*file_object).fs_context = ptr::null_mut();
                    (*file_object).device_object = ptr::null_mut();
                }
            }
        }
        _ => status = STATUS_INVALID_DEVICE_REQUEST,
    }

    if !file_object.is_null() { unsafe { (*file_object).final_status = status; } }
    unsafe { wdm::io_complete_irp_status(irp, status, information, 0); }
    status
}

unsafe extern "win64" fn file_system_control_dispatch(_device:*mut DeviceObject,irp:*mut Irp)->i32{
 if irp.is_null(){return STATUS_INVALID_PARAMETER}
 let stack=unsafe{wdm::io_get_current_irp_stack_location(irp)};
 if stack.is_null(){unsafe{wdm::io_complete_irp_status(irp,STATUS_INVALID_PARAMETER,0,0);}return STATUS_INVALID_PARAMETER}
 let minor=unsafe{(*stack).minor_function};
 let mut status=STATUS_INVALID_DEVICE_REQUEST;
 if minor==IRP_MN_MOUNT_VOLUME{
  let real=unsafe{wdm::io_read_stack_pointer::<DeviceObject>(stack,4)};
  let vpb=unsafe{wdm::io_read_stack_pointer::<wdm::Vpb>(stack,12)};
  if real.is_null()||vpb.is_null()||!crate::ata_disk::is_ext4_partition(real){status=STATUS_UNRECOGNIZED_VOLUME;}
  else{
   let existing=VOLUME_DEVICE.load(Ordering::Acquire);
   if existing!=0{
    let volume=existing as *mut DeviceObject;
    if unsafe{(*vpb).device_object}.is_null(){unsafe{(*vpb).device_object=volume.cast();(*volume).vpb=vpb.cast();}status=STATUS_SUCCESS;}
    else if unsafe{(*vpb).device_object}==volume.cast(){status=STATUS_SUCCESS}else{status=STATUS_DEVICE_BUSY}
   }else{
    let mut volume=ptr::null_mut();
    let create=unsafe{wdm::io_create_device(ptr::addr_of_mut!(FILE_DRIVER),0,ptr::null_mut(),FILE_DEVICE_DISK_FILE_SYSTEM,0,0,&mut volume)};
    if create>=0&&!volume.is_null(){
     unsafe{(*volume).vpb=vpb.cast();(*vpb).device_object=volume.cast();}
     VOLUME_DEVICE.store(volume as usize,Ordering::Release);status=STATUS_SUCCESS;
    }else{status=create}
   }
  }
 }else if minor==IRP_MN_USER_FS_REQUEST{
  let control=unsafe{wdm::io_read_stack_u32(stack,8)};
  if control==0x0009_0020{status=STATUS_SUCCESS;}
 }
 unsafe{wdm::io_complete_irp_status(irp,status,0,0);}status
}

const fn make_file_driver() -> DriverObject {
    let mut major: [Option<wdm::DriverDispatch>; 28] = [None; 28];
    major[IRP_MJ_CREATE as usize] = Some(file_dispatch);
    major[IRP_MJ_READ as usize] = Some(file_dispatch);
    major[IRP_MJ_CLEANUP as usize] = Some(file_dispatch);
    major[IRP_MJ_CLOSE as usize] = Some(file_dispatch);
    major[wdm::IRP_MJ_FILE_SYSTEM_CONTROL as usize] = Some(file_system_control_dispatch);
    DriverObject {
        object_type: 4,
        size: core::mem::size_of::<DriverObject>() as i16,
        device_object: ptr::null_mut(),
        flags: 0,
        driver_start: ptr::null_mut(),
        driver_size: 0,
        driver_section: ptr::null_mut(),
        driver_extension: ptr::null_mut(),
        driver_name: UNICODE_STRING { length: 0, maximum_length: 0, buffer: ptr::null_mut() },
        hardware_database: ptr::null_mut(),
        fast_io_dispatch: ptr::null_mut(),
        driver_init: ptr::null_mut(),
        driver_start_io: ptr::null_mut(),
        driver_unload: None,
        major_function: major,
    }
}
static mut FILE_DRIVER: DriverObject = make_file_driver();

fn ensure_file_device() -> Result<*mut DeviceObject, i32> {
    let existing = FILE_DEVICE.load(Ordering::Acquire);
    if existing != 0 {
        return Ok(existing as *mut DeviceObject);
    }
    match DEVICE_INIT.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => {
            let mut device = ptr::null_mut();
            let status = unsafe {
                wdm::io_create_device(
                    ptr::addr_of_mut!(FILE_DRIVER),
                    0,
                    ptr::null_mut(),
                    FILE_DEVICE_DISK,
                    0,
                    0,
                    &mut device,
                )
            };
            if status >= 0 && !device.is_null() {
                FILE_DEVICE.store(device as usize, Ordering::Release);
                DEVICE_INIT.store(2, Ordering::Release);
                Ok(device)
            } else {
                DEVICE_INIT.store(0, Ordering::Release);
                Err(status)
            }
        }
        Err(1) => {
            while DEVICE_INIT.load(Ordering::Acquire) == 1 { core::hint::spin_loop(); }
            let device = FILE_DEVICE.load(Ordering::Acquire);
            if device != 0 { Ok(device as *mut DeviceObject) } else { Err(STATUS_INSUFFICIENT_RESOURCES) }
        }
        Err(_) => {
            let device = FILE_DEVICE.load(Ordering::Acquire);
            if device != 0 { Ok(device as *mut DeviceObject) } else { Err(STATUS_INSUFFICIENT_RESOURCES) }
        }
    }
}

fn ensure_filesystem_device()->Result<*mut DeviceObject,i32>{
 let existing=FILE_SYSTEM_DEVICE.load(Ordering::Acquire);if existing!=0{return Ok(existing as *mut DeviceObject)}
 match FILE_SYSTEM_INIT.compare_exchange(0,1,Ordering::AcqRel,Ordering::Acquire){
  Ok(_)=>{
   let mut device=ptr::null_mut();
   let status=unsafe{wdm::io_create_device(ptr::addr_of_mut!(FILE_DRIVER),0,ptr::null_mut(),FILE_DEVICE_DISK_FILE_SYSTEM,0,0,&mut device)};
   if status>=0&&!device.is_null(){
    unsafe{wdm::io_register_file_system(device);}
    if wdm::is_file_system_registered(device){FILE_SYSTEM_DEVICE.store(device as usize,Ordering::Release);FILE_SYSTEM_INIT.store(2,Ordering::Release);Ok(device)}
    else{unsafe{wdm::io_delete_device(device);}FILE_SYSTEM_INIT.store(0,Ordering::Release);Err(STATUS_INSUFFICIENT_RESOURCES)}
   }else{FILE_SYSTEM_INIT.store(0,Ordering::Release);Err(status)}
  }
  Err(1)=>{while FILE_SYSTEM_INIT.load(Ordering::Acquire)==1{core::hint::spin_loop();}let d=FILE_SYSTEM_DEVICE.load(Ordering::Acquire);if d!=0{Ok(d as *mut DeviceObject)}else{Err(STATUS_INSUFFICIENT_RESOURCES)}}
  Err(_)=>{let d=FILE_SYSTEM_DEVICE.load(Ordering::Acquire);if d!=0{Ok(d as *mut DeviceObject)}else{Err(STATUS_INSUFFICIENT_RESOURCES)}}
 }
}
pub fn mount_ext4_volume(real_device:*mut DeviceObject)->i32{
 if real_device.is_null(){return STATUS_INVALID_PARAMETER}
 if let Err(status)=ensure_filesystem_device(){return status}
 let status=unsafe{wdm::io_verify_volume(real_device,0)};
 if status>=0&&VOLUME_DEVICE.load(Ordering::Acquire)!=0{STATUS_SUCCESS}
 else if status>=0{STATUS_UNRECOGNIZED_VOLUME}else{status}
}

fn dispatch_file_irp(file_object: *mut FILE_OBJECT, major: u8, offset: i64, buffer: *mut u8, length: usize) -> Result<usize, i32> {
    if file_object.is_null() || (major == IRP_MJ_READ && length != 0 && buffer.is_null()) || length > u32::MAX as usize {
        return Err(STATUS_INVALID_PARAMETER);
    }
    let device = unsafe { (*file_object).device_object.cast::<DeviceObject>() };
    if device.is_null(){return Err(STATUS_INVALID_HANDLE)}
    let stack_size = unsafe { (*device).stack_size };
    let irp = unsafe { wdm::io_allocate_irp(stack_size, 0) };
    if irp.is_null() { return Err(STATUS_INSUFFICIENT_RESOURCES); }
    let stack = unsafe { wdm::io_get_next_irp_stack_location(irp) };
    if stack.is_null() {
        unsafe { wdm::io_free_irp(irp); }
        return Err(STATUS_INSUFFICIENT_RESOURCES);
    }
    unsafe {
        (*stack).major_function = major;
        (*stack).file_object = file_object.cast::<c_void>();
        if major == IRP_MJ_READ {
            let _ = wdm::io_write_stack_u32(stack, 0, length as u32);
            let _ = wdm::io_write_stack_i64(stack, 8, offset);
            let _ = wdm::io_set_irp_user_buffer(irp, buffer.cast());
        }
    }
    let status = unsafe { wdm::io_call_driver(device, irp) };
    let information = unsafe { wdm::io_get_irp_information(irp) };
    unsafe { wdm::io_free_irp(irp); }
    if major == IRP_MJ_READ && status == STATUS_END_OF_FILE { return Ok(0); }
    if status < 0 { Err(status) } else { Ok(information) }
}

fn fold(b: u8) -> u8 { if (b'A'..=b'Z').contains(&b) { b + (b'a' - b'A') } else { b } }
fn path_equal(a: &[u8], b: &[u8]) -> bool { a.len() == b.len() && a.iter().zip(b).all(|(x, y)| fold(*x) == fold(*y)) }

/// Register immutable bytes under an absolute NT-style path.
pub fn register_static(path: &[u8], bytes: &'static [u8]) -> Result<(), i32> {
    if path.is_empty() || path.len() > MAX_PATH_BYTES || !path.starts_with(b"\\") {
        return Err(STATUS_INVALID_PARAMETER);
    }
    let mut state = STATE.lock();
    if state.files.iter().any(|f| f.used && path_equal(&f.path[..f.path_len], path)) {
        return Err(STATUS_INVALID_PARAMETER);
    }
    let slot = state.files.iter().position(|f| !f.used).ok_or(STATUS_INSUFFICIENT_RESOURCES)?;
    let generation = state.next_generation;
    state.next_generation = state.next_generation.wrapping_add(1).max(1);
    let mut file = FileRecord::EMPTY;
    file.path[..path.len()].copy_from_slice(path);
    file.path_len = path.len();
    file.address = bytes.as_ptr() as usize;
    file.length = bytes.len();
    file.generation = generation;
    file.source = 0;
    file.used = true;
    state.files[slot] = file;
    Ok(())
}

/// Register the boot-staged system DLLs in the immutable provider namespace.
pub fn mount_boot_system_files() {
    match BOOT_INIT.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => {
            if let Some(bytes) = crate::boot_ntdll_bytes() {
                let _ = register_static(b"\\SystemRoot\\System32\\ntdll.dll", bytes);
                let _ = register_static(b"\\Windows\\System32\\ntdll.dll", bytes);
            }
            for (name, systemroot_path, windows_path) in [
                ("kernel32.dll", b"\\SystemRoot\\System32\\kernel32.dll" as &[u8], b"\\Windows\\System32\\kernel32.dll" as &[u8]),
                ("kernelbase.dll", b"\\SystemRoot\\System32\\kernelbase.dll" as &[u8], b"\\Windows\\System32\\kernelbase.dll" as &[u8]),
                ("shell32.dll", b"\\SystemRoot\\System32\\shell32.dll" as &[u8], b"\\Windows\\System32\\shell32.dll" as &[u8]),
                ("msvcrt.dll", b"\\SystemRoot\\System32\\msvcrt.dll" as &[u8], b"\\Windows\\System32\\msvcrt.dll" as &[u8]),
            ] {
                if let Some(bytes) = crate::boot_system_dll_bytes(name) {
                    let _ = register_static(systemroot_path, bytes);
                    let _ = register_static(windows_path, bytes);
                }
            }
            BOOT_INIT.store(2, Ordering::Release);
        }
        Err(1) => {
            while BOOT_INIT.load(Ordering::Acquire) == 1 { core::hint::spin_loop(); }
        }
        Err(_) => {}
    }
}

const FILE_OBJECT_TYPE_NAME: [u16; 4] = [b'F' as u16, b'i' as u16, b'l' as u16, b'e' as u16];

unsafe extern "win64" fn file_object_finalizer(body: *mut c_void) {
    if body.is_null() { return; }
    let mut state = STATE.lock();
    if let Some(slot) = state.file_objects.iter_mut().find(|slot| slot.file_object == body.cast::<FILE_OBJECT>()) {
        *slot = FileObjectRecord::EMPTY;
    }
}

/// Resolve a process-owned Object Manager handle and pin the FILE_OBJECT for I/O.
fn pin_handle(owner: u64, handle: usize) -> Result<*mut FILE_OBJECT, i32> {
    if handle == 0 { return Err(STATUS_INVALID_HANDLE); }
    let body = ob::reference_by_handle(handle as *mut c_void, 1, FILE_OBJECT_TYPE.load(Ordering::Acquire) as u16, owner as usize, false)?;
    let mut state = STATE.lock();
    let Some(slot) = state.file_objects.iter_mut().find(|slot| slot.file_object == body.cast::<FILE_OBJECT>() && slot.used && !slot.closing) else {
        drop(state);
        let _ = ob::dereference_body(body);
        return Err(STATUS_INVALID_HANDLE);
    };
    slot.active_io = match slot.active_io.checked_add(1) {
        Some(value) => value,
        None => { drop(state); let _ = ob::dereference_body(body); return Err(STATUS_DEVICE_BUSY); }
    };
    Ok(body.cast::<FILE_OBJECT>())
}
fn unpin_handle(file_object: *mut FILE_OBJECT) {
    if file_object.is_null() { return; }
    {
        let mut state = STATE.lock();
        if let Some(slot) = state.file_objects.iter_mut().find(|slot| slot.file_object == file_object && slot.used) {
            slot.active_io = slot.active_io.saturating_sub(1);
        }
    }
    let _ = ob::dereference_body(file_object.cast());
}

/// Open a registered path and issue a real IRP_MJ_CREATE against the provider device.
pub fn open(path: &[u8]) -> Result<usize, i32> { open_for_process(ob::current_owner() as u64, path) }
pub fn open_for_process(owner: u64, path: &[u8]) -> Result<usize, i32> {
    if path.is_empty() || path.len() > MAX_PATH_BYTES { return Err(STATUS_INVALID_PARAMETER); }
    mount_boot_system_files();
    let registered={let state=STATE.lock();state.files.iter().position(|f|f.used&&path_equal(&f.path[..f.path_len],path))};
    let file_index=if let Some(index)=registered{index}else{
     let size=crate::ata_disk::ext4_root_file_size(path).map_err(|_|STATUS_OBJECT_NAME_NOT_FOUND)?;
     let mut state=STATE.lock();
     if let Some(index)=state.files.iter().position(|f|f.used&&path_equal(&f.path[..f.path_len],path)){index}
     else{
      let index=state.files.iter().position(|f|!f.used).ok_or(STATUS_INSUFFICIENT_RESOURCES)?;
      let generation=state.next_generation;state.next_generation=state.next_generation.wrapping_add(1).max(1);
      let mut file=FileRecord::EMPTY;file.path[..path.len()].copy_from_slice(path);file.path_len=path.len();
      file.length=usize::try_from(size).map_err(|_|STATUS_INVALID_PARAMETER)?;file.generation=generation;file.source=1;file.used=true;
      state.files[index]=file;index
     }
    };
    let file_source=STATE.lock().files[file_index].source;
    let device=if file_source==1{let d=VOLUME_DEVICE.load(Ordering::Acquire);if d==0{return Err(STATUS_UNRECOGNIZED_VOLUME)}d as *mut DeviceObject}else{ensure_file_device()?};
    let slot = {
        let mut state = STATE.lock();
        let slot = state.file_objects.iter().position(|h| !h.used).ok_or(STATUS_INSUFFICIENT_RESOURCES)?;
        state.file_objects[slot] = FileObjectRecord { active_io: 0, closing: false, used: true, file_object: ptr::null_mut() };
        slot
    };
    if FILE_OBJECT_TYPE.load(Ordering::Acquire) == 0 {
        match ob::ensure_type(&FILE_OBJECT_TYPE_NAME, 1, 2, 4, 7, u32::MAX) {
            Ok(id) => { FILE_OBJECT_TYPE.store(id as usize, Ordering::Release); }
            Err(status) => { STATE.lock().file_objects[slot] = FileObjectRecord::EMPTY; return Err(status); }
        }
    }
    let file_object = match ob::create_body_object(
        FILE_OBJECT_TYPE.load(Ordering::Acquire) as u16,
        core::mem::size_of::<FILE_OBJECT>(),
        Some(file_object_finalizer),
    ) {
        Ok(body) => body.cast::<FILE_OBJECT>(),
        Err(status) => { STATE.lock().file_objects[slot] = FileObjectRecord::EMPTY; return Err(status); }
    };
    let mut initial_file_object = empty_file_object();
    initial_file_object.device_object = device.cast();
    initial_file_object.vpb = unsafe { (*device).vpb.cast() };
    initial_file_object.fs_context = (file_index + 1) as *mut c_void;
    unsafe { ptr::write(file_object, initial_file_object); }
    STATE.lock().file_objects[slot].file_object = file_object;
    let object_handle = match ob::open_by_pointer(file_object.cast(), 1, owner as usize, true, 0) {
        Ok(handle) => handle,
        Err(status) => { ob::unregister_object(file_object.cast()); return Err(status); }
    };
    if let Err(status) = dispatch_file_irp(file_object, IRP_MJ_CREATE, 0, ptr::null_mut(), 0) {
        let _ = ob::close_handle(object_handle, owner as usize);
        ob::unregister_object(file_object.cast());
        return Err(status);
    }
    Ok(object_handle as usize)
}

/// Read from an explicit byte offset using IRP_MJ_READ.
pub fn read_at(handle: usize, offset: usize, out: &mut [u8]) -> Result<usize, i32> {
    read_at_for_process(ob::current_owner() as u64, handle, offset, out)
}
pub fn read_at_for_process(owner: u64, handle: usize, offset: usize, out: &mut [u8]) -> Result<usize, i32> {
    let byte_offset: i64 = offset.try_into().map_err(|_| STATUS_INVALID_PARAMETER)?;
    let file_object = pin_handle(owner, handle)?;
    let result = dispatch_file_irp(file_object, IRP_MJ_READ, byte_offset, out.as_mut_ptr(), out.len());
    unpin_handle(file_object);
    result
}

/// Read from and advance FILE_OBJECT.CurrentByteOffset using the WDM file-pointer sentinel.
pub fn read_current(handle: usize, out: &mut [u8]) -> Result<usize, i32> {
    read_current_for_process(ob::current_owner() as u64, handle, out)
}
pub fn read_current_for_process(owner: u64, handle: usize, out: &mut [u8]) -> Result<usize, i32> {
    let file_object = pin_handle(owner, handle)?;
    let result = dispatch_file_irp(file_object, IRP_MJ_READ, FILE_USE_FILE_POINTER_POSITION, out.as_mut_ptr(), out.len());
    unpin_handle(file_object);
    result
}

pub fn current_offset_for_process(owner: u64, handle: usize) -> Result<usize, i32> {
    let file_object = pin_handle(owner, handle)?;
    let offset = unsafe { (*file_object).current_byte_offset };
    unpin_handle(file_object);
    usize::try_from(offset).map_err(|_| STATUS_INVALID_PARAMETER)
}
pub fn advance_current_for_process(owner: u64, handle: usize, expected: usize, amount: usize) -> Result<(), i32> {
    let end = expected.checked_add(amount).ok_or(STATUS_INVALID_PARAMETER)?;
    let file_object = pin_handle(owner, handle)?;
    let result = {
        let current = unsafe { (*file_object).current_byte_offset };
        if current < 0 || current as usize != expected { Err(STATUS_INVALID_HANDLE) }
        else {
            unsafe { (*file_object).current_byte_offset = end as i64; }
            Ok(())
        }
    };
    unpin_handle(file_object);
    result
}

/// Issue IRP_MJ_CLEANUP followed by IRP_MJ_CLOSE before releasing the FILE_OBJECT.
pub fn close(handle: usize) -> Result<(), i32> { close_for_process(ob::current_owner() as u64, handle) }
pub fn close_for_process(owner: u64, handle: usize) -> Result<(), i32> {
    if handle == 0 { return Err(STATUS_INVALID_HANDLE); }
    let body = ob::reference_by_handle(handle as *mut c_void, 0, FILE_OBJECT_TYPE.load(Ordering::Acquire) as u16, owner as usize, false)?;
    let file_object = body.cast::<FILE_OBJECT>();
    {
        let mut state = STATE.lock();
        let Some(slot) = state.file_objects.iter_mut().find(|slot| slot.file_object == file_object && slot.used && !slot.closing) else {
            drop(state);
            let _ = ob::dereference_body(body);
            return Err(STATUS_INVALID_HANDLE);
        };
        if slot.active_io != 0 {
            drop(state);
            let _ = ob::dereference_body(body);
            return Err(STATUS_DEVICE_BUSY);
        }
        slot.closing = true;
    }
    if let Err(status) = dispatch_file_irp(file_object, IRP_MJ_CLEANUP, 0, ptr::null_mut(), 0) {
        if let Some(slot) = STATE.lock().file_objects.iter_mut().find(|slot| slot.file_object == file_object) { slot.closing = false; }
        let _ = ob::dereference_body(body);
        return Err(status);
    }
    if let Err(status) = dispatch_file_irp(file_object, IRP_MJ_CLOSE, 0, ptr::null_mut(), 0) {
        if let Some(slot) = STATE.lock().file_objects.iter_mut().find(|slot| slot.file_object == file_object) { slot.closing = false; }
        let _ = ob::dereference_body(body);
        return Err(status);
    }
    ob::close_handle(handle as *mut c_void, owner as usize)?;
    if !ob::unregister_object(body) {
        let _ = ob::dereference_body(body);
        return Err(STATUS_INVALID_HANDLE);
    }
    let _ = ob::dereference_body(body);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    static TEST_BYTES: &[u8] = b"MZ-test-image";

    #[test]
    fn open_read_close_and_stale_handle() {
        let path = b"\\SystemRoot\\Tests\\fixture-wdm.dll";
        let _ = register_static(path, TEST_BYTES);
        let handle = open(path).unwrap();
        let mut buf = [0u8; 32];
        let n = read_at(handle, 0, &mut buf).unwrap();
        assert_eq!(&buf[..n], TEST_BYTES);
        assert_eq!(read_at(handle, TEST_BYTES.len(), &mut buf).unwrap(), 0);
        close(handle).unwrap();
        assert_eq!(read_at(handle, 0, &mut buf), Err(STATUS_INVALID_HANDLE));
    }

    #[test]
    fn missing_file_fails_closed() {
        assert_eq!(open(b"\\SystemRoot\\missing-wdm.dll"), Err(STATUS_OBJECT_NAME_NOT_FOUND));
    }

    #[test]
    fn handles_are_process_scoped() {
        let path = b"\\SystemRoot\\Tests\\owner-wdm.dll";
        let _ = register_static(path, TEST_BYTES);
        let handle = open_for_process(41, path).unwrap();
        let mut out = [0u8; 4];
        assert_eq!(read_at_for_process(42, handle, 0, &mut out), Err(STATUS_INVALID_HANDLE));
        assert_eq!(read_at_for_process(41, handle, 0, &mut out), Ok(4));
        assert_eq!(close_for_process(42, handle), Err(STATUS_INVALID_HANDLE));
        close_for_process(41, handle).unwrap();
    }

    #[test]
    fn sequential_reads_advance_file_object_position() {
        let path = b"\\SystemRoot\\Tests\\sequential-wdm.dll";
        let _ = register_static(path, TEST_BYTES);
        let handle = open(path).unwrap();
        let mut first = [0u8; 2];
        let mut second = [0u8; 3];
        assert_eq!(read_current(handle, &mut first), Ok(2));
        assert_eq!(read_current(handle, &mut second), Ok(3));
        assert_eq!(&first, b"MZ");
        assert_eq!(&second, b"-te");
        assert_eq!(current_offset_for_process(0, handle), Ok(5));
        close(handle).unwrap();
    }
}
