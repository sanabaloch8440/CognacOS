//! FAT32 WDM filesystem driver.
//!
//! This is the filesystem-facing WDM boundary: it registers a disk-file-system
//! driver, recognizes FAT32 volumes during IRP_MN_MOUNT_VOLUME, publishes a
//! volume DEVICE_OBJECT through the VPB, and services FILE_OBJECT requests.
//! The filesystem engine is isolated in cognacos-fat32; all sectors travel
//! through the partition DEVICE_OBJECT and the storage driver's READ/WRITE IRPs.
//!
//! The initial implementation serializes mount/operation/unmount transactions.
//! This is deliberately conservative while the kernel has no filesystem
//! resource/worker-thread implementation; it prevents concurrent FatFs
//! metadata mutations from racing. It is not a substitute for ERESOURCE,
//! FCB/CCB, oplock, or cache-manager integration.

use crate::hal::IrqSpinLock;
use crate::nt_abi::io::{FILE_OBJECT, VPB};
use crate::nt_abi::object::UNICODE_STRING;
use crate::wdm::{self, DeviceObject, DriverObject, Irp, IRP_MJ_CREATE, IRP_MJ_READ,
    IRP_MJ_WRITE, IRP_MJ_CLEANUP, IRP_MJ_CLOSE};
use cognacos_fat32::{FatFilesystem, DiskStream, MountError};
use cognacos_fat32::fatfs::{Read, Seek, SeekFrom, Write};
use core::ffi::c_void;
use core::ptr;
use core::sync::atomic::{AtomicUsize, Ordering};

const STATUS_SUCCESS: i32 = 0;
const STATUS_INVALID_PARAMETER: i32 = 0xC000_000D_u32 as i32;
const STATUS_INVALID_DEVICE_REQUEST: i32 = 0xC000_0010_u32 as i32;
const STATUS_OBJECT_NAME_NOT_FOUND: i32 = 0xC000_0034_u32 as i32;
const STATUS_END_OF_FILE: i32 = 0xC000_0011_u32 as i32;
const STATUS_UNRECOGNIZED_VOLUME: i32 = 0xC000_014F_u32 as i32;
const STATUS_IO_DEVICE_ERROR: i32 = 0xC000_0185_u32 as i32;
const STATUS_ACCESS_DENIED: i32 = 0xC000_0022_u32 as i32;
const STATUS_INSUFFICIENT_RESOURCES: i32 = 0xC000_009A_u32 as i32;
const STATUS_DEVICE_BUSY: i32 = 0x8000_0011_u32 as i32;
const FILE_DEVICE_DISK_FILE_SYSTEM: u32 = 8;
const IRP_MN_MOUNT_VOLUME: u8 = 1;
const IRP_MN_VERIFY_VOLUME: u8 = 0;
const IRP_MN_USER_FS_REQUEST: u8 = 0;
const FILE_USE_FILE_POINTER_POSITION: i64 = -2;
const IRP_MJ_FLUSH_BUFFERS: usize = 9;

type MountedFat = FatFilesystem<crate::io::storage::IoManagerBlockDevice>;

static DRIVER_DEVICE: AtomicUsize = AtomicUsize::new(0);
static DRIVER_INIT: AtomicUsize = AtomicUsize::new(0);
static VOLUME_DEVICE: AtomicUsize = AtomicUsize::new(0);
// FatFs uses interior mutability. All access is serialized by this lock and
// each operation owns the mounted instance for its complete lifetime.
static VOLUME_RESOURCE: IrqSpinLock<()> = IrqSpinLock::new(());

const fn make_driver() -> DriverObject {
    let mut major: [Option<wdm::DriverDispatch>; 28] = [None; 28];
    major[IRP_MJ_CREATE as usize] = Some(filesystem_dispatch);
    major[IRP_MJ_READ as usize] = Some(filesystem_dispatch);
    major[IRP_MJ_WRITE as usize] = Some(filesystem_dispatch);
    major[IRP_MJ_CLEANUP as usize] = Some(filesystem_dispatch);
    major[IRP_MJ_CLOSE as usize] = Some(filesystem_dispatch);
    major[IRP_MJ_FLUSH_BUFFERS] = Some(filesystem_dispatch);
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
static mut FAT32_DRIVER: DriverObject = make_driver();

fn map_mount_error(_: MountError) -> i32 { STATUS_UNRECOGNIZED_VOLUME }

fn with_mounted<T>(
    writable: bool,
    operation: impl FnOnce(&cognacos_fat32::fatfs::FileSystem<DiskStream<crate::io::storage::IoManagerBlockDevice>>) -> Result<T, i32>,
) -> Result<T, i32> {
    let _resource = VOLUME_RESOURCE.lock();
    let disk = crate::io::storage::fat32_block_device().map_err(|_| STATUS_IO_DEVICE_ERROR)?;
    let sectors = disk.sector_count();
    let mounted = if writable {
        FatFilesystem::mount_read_write(disk, 0, sectors)
    } else {
        FatFilesystem::mount_read_only(disk, 0, sectors)
    }.map_err(map_mount_error)?;
    let result = operation(mounted.filesystem());
    let unmount = mounted.unmount().map_err(map_mount_error);
    match result {
        Err(status) => Err(status),
        Ok(value) => { unmount?; Ok(value) }
    }
}

fn normalize_path(bytes: &[u8]) -> Result<([u8; 260], usize), i32> {
    if bytes.is_empty() || bytes.len() > 260 || !bytes.starts_with(b"\\") {
        return Err(STATUS_INVALID_PARAMETER);
    }
    let mut path = [0u8; 260];
    let mut length = 0usize;
    for byte in bytes.iter().copied().skip(1) {
        if byte == b'\\' { path[length] = b'/'; }
        else { path[length] = byte; }
        length += 1;
    }
    if length == 0 { return Err(STATUS_ACCESS_DENIED); }
    core::str::from_utf8(&path[..length]).map_err(|_| STATUS_INVALID_PARAMETER)?;
    Ok((path, length))
}

fn file_size(path: &[u8]) -> Result<u64, i32> {
    let (normalized, length) = normalize_path(path)?;
    let name = core::str::from_utf8(&normalized[..length]).map_err(|_| STATUS_INVALID_PARAMETER)?;
    with_mounted(false, |fs| {
        let root = fs.root_dir();
        let mut file = root.open_file(name).map_err(|_| STATUS_OBJECT_NAME_NOT_FOUND)?;
        file.seek(SeekFrom::End(0)).map_err(|_| STATUS_IO_DEVICE_ERROR)
    })
}

fn read_file(path: &[u8], offset: u64, output: &mut [u8]) -> Result<usize, i32> {
    let (normalized, length) = normalize_path(path)?;
    let name = core::str::from_utf8(&normalized[..length]).map_err(|_| STATUS_INVALID_PARAMETER)?;
    with_mounted(false, |fs| {
        let root = fs.root_dir();
        let mut file = root.open_file(name).map_err(|_| STATUS_OBJECT_NAME_NOT_FOUND)?;
        let size = file.seek(SeekFrom::End(0)).map_err(|_| STATUS_IO_DEVICE_ERROR)?;
        if offset >= size { return Ok(0); }
        file.seek(SeekFrom::Start(offset)).map_err(|_| STATUS_INVALID_PARAMETER)?;
        file.read(output).map_err(|_| STATUS_IO_DEVICE_ERROR)
    })
}

fn write_file(path: &[u8], offset: u64, input: &[u8]) -> Result<usize, i32> {
    let (normalized, length) = normalize_path(path)?;
    let name = core::str::from_utf8(&normalized[..length]).map_err(|_| STATUS_INVALID_PARAMETER)?;
    with_mounted(true, |fs| {
        let root = fs.root_dir();
        let mut file = root.open_file(name).map_err(|_| STATUS_OBJECT_NAME_NOT_FOUND)?;
        file.seek(SeekFrom::Start(offset)).map_err(|_| STATUS_INVALID_PARAMETER)?;
        let written = file.write(input).map_err(|_| STATUS_IO_DEVICE_ERROR)?;
        file.flush().map_err(|_| STATUS_IO_DEVICE_ERROR)?;
        Ok(written)
    })
}

pub fn volume_device() -> *mut DeviceObject {
    VOLUME_DEVICE.load(Ordering::Acquire) as *mut DeviceObject
}

fn ensure_driver_device() -> Result<*mut DeviceObject, i32> {
    let existing = DRIVER_DEVICE.load(Ordering::Acquire);
    if existing != 0 { return Ok(existing as *mut DeviceObject); }
    match DRIVER_INIT.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => {
            let mut device = ptr::null_mut();
            let status = unsafe {
                wdm::io_create_device(ptr::addr_of_mut!(FAT32_DRIVER), 0, ptr::null_mut(),
                    FILE_DEVICE_DISK_FILE_SYSTEM, 0, 0, &mut device)
            };
            if status >= 0 && !device.is_null() {
                unsafe { wdm::io_register_file_system(device); }
                if wdm::is_file_system_registered(device) {
                    DRIVER_DEVICE.store(device as usize, Ordering::Release);
                    DRIVER_INIT.store(2, Ordering::Release);
                    Ok(device)
                } else {
                    unsafe { wdm::io_delete_device(device); }
                    DRIVER_INIT.store(0, Ordering::Release);
                    Err(STATUS_INSUFFICIENT_RESOURCES)
                }
            } else {
                DRIVER_INIT.store(0, Ordering::Release);
                Err(status)
            }
        }
        Err(1) => {
            while DRIVER_INIT.load(Ordering::Acquire) == 1 { core::hint::spin_loop(); }
            let device = DRIVER_DEVICE.load(Ordering::Acquire);
            if device != 0 { Ok(device as *mut DeviceObject) } else { Err(STATUS_INSUFFICIENT_RESOURCES) }
        }
        Err(_) => {
            let device = DRIVER_DEVICE.load(Ordering::Acquire);
            if device != 0 { Ok(device as *mut DeviceObject) } else { Err(STATUS_INSUFFICIENT_RESOURCES) }
        }
    }
}

/// Ask the I/O Manager to verify and mount the partition through registered
/// filesystem drivers. The FAT32 mount IRP publishes the resulting volume device.
pub fn mount_volume(real_device: *mut DeviceObject) -> i32 {
    if real_device.is_null() { return STATUS_INVALID_PARAMETER; }
    if let Err(status) = ensure_driver_device() { return status; }
    let status = unsafe { wdm::io_verify_volume(real_device, 0) };
    if status >= 0 && !volume_device().is_null() { STATUS_SUCCESS }
    else if status >= 0 { STATUS_UNRECOGNIZED_VOLUME }
    else { status }
}

unsafe extern "win64" fn file_system_control_dispatch(_device: *mut DeviceObject, irp: *mut Irp) -> i32 {
    if irp.is_null() { return STATUS_INVALID_PARAMETER; }
    let stack = unsafe { wdm::io_get_current_irp_stack_location(irp) };
    if stack.is_null() {
        unsafe { wdm::io_complete_irp_status(irp, STATUS_INVALID_PARAMETER, 0, 0); }
        return STATUS_INVALID_PARAMETER;
    }
    let minor = unsafe { (*stack).minor_function };
    let mut status = STATUS_INVALID_DEVICE_REQUEST;
    if minor == IRP_MN_MOUNT_VOLUME {
        let real = unsafe { wdm::io_read_stack_pointer::<DeviceObject>(stack, 4) };
        let vpb = unsafe { wdm::io_read_stack_pointer::<VPB>(stack, 12) };
        if real.is_null() || vpb.is_null() {
            status = STATUS_INVALID_PARAMETER;
        } else if with_mounted(false, |_| Ok(())).is_err() {
            status = STATUS_UNRECOGNIZED_VOLUME;
        } else {
            let existing = volume_device();
            if !existing.is_null() {
                if unsafe { (*vpb).device_object }.is_null() {
                    unsafe {
                        (*vpb).device_object = existing.cast();
                        (*existing).vpb = vpb.cast();
                        (*vpb).real_device = real.cast();
                        (*vpb).flags |= crate::nt_abi::io::VPB_MOUNTED;
                    }
                    status = STATUS_SUCCESS;
                } else if unsafe { (*vpb).device_object } == existing.cast() {
                    status = STATUS_SUCCESS;
                } else {
                    status = STATUS_DEVICE_BUSY;
                }
            } else {
                let mut volume = ptr::null_mut();
                let create = unsafe {
                    wdm::io_create_device(ptr::addr_of_mut!(FAT32_DRIVER), 0, ptr::null_mut(),
                        FILE_DEVICE_DISK_FILE_SYSTEM, 0, 0, &mut volume)
                };
                if create >= 0 && !volume.is_null() {
                    unsafe {
                        (*volume).vpb = vpb.cast();
                        (*vpb).device_object = volume.cast();
                        (*vpb).real_device = real.cast();
                        (*vpb).flags |= crate::nt_abi::io::VPB_MOUNTED;
                    }
                    VOLUME_DEVICE.store(volume as usize, Ordering::Release);
                    status = STATUS_SUCCESS;
                } else { status = create; }
            }
        }
    } else if minor == IRP_MN_VERIFY_VOLUME {
        status = if with_mounted(false, |_| Ok(())).is_ok() { STATUS_SUCCESS } else { STATUS_UNRECOGNIZED_VOLUME };
    } else if minor == IRP_MN_USER_FS_REQUEST {
        let control = unsafe { wdm::io_read_stack_u32(stack, 8) };
        // FSCTL_IS_VOLUME_MOUNTED
        if control == 0x0009_0020 {
            status = if !volume_device().is_null() { STATUS_SUCCESS } else { STATUS_UNRECOGNIZED_VOLUME };
        }
    }
    unsafe { wdm::io_complete_irp_status(irp, status, 0, 0); }
    status
}

unsafe extern "win64" fn filesystem_dispatch(device: *mut DeviceObject, irp: *mut Irp) -> i32 {
    if device.is_null() || irp.is_null() { return STATUS_INVALID_PARAMETER; }
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
            if file_object.is_null() || unsafe { (*file_object).device_object } != device.cast() {
                status = STATUS_INVALID_PARAMETER;
            } else if let Some((path, length)) = crate::file_services::path_for_file_object(file_object) {
                match file_size(&path[..length]) {
                    Ok(size) => {
                        unsafe { (*file_object).final_status = STATUS_SUCCESS; }
                        let _ = size;
                    }
                    Err(error) => status = error,
                }
            } else { status = STATUS_OBJECT_NAME_NOT_FOUND; }
        }
        IRP_MJ_READ | IRP_MJ_WRITE => {
            if file_object.is_null() {
                status = STATUS_INVALID_PARAMETER;
            } else {
                let write = major == IRP_MJ_WRITE;
                let length = unsafe { wdm::io_read_stack_u32(stack, 0) as usize };
                let requested = unsafe { wdm::io_read_stack_i64(stack, 8) };
                let mdl = unsafe { wdm::io_get_irp_mdl_address(irp) };
                let buffer = if mdl.is_null() {
                    unsafe { wdm::io_get_irp_user_buffer(irp).cast::<u8>() }
                } else {
                    unsafe { crate::ntoskrnl::mm::runtime::mm_get_system_address_for_mdl_safe(mdl, 0x20).cast::<u8>() }
                };
                let offset = if requested == FILE_USE_FILE_POINTER_POSITION {
                    unsafe { (*file_object).current_byte_offset }
                } else { requested };
                if (length != 0 && buffer.is_null()) || offset < 0 || length > 1024 * 1024 {
                    status = STATUS_INVALID_PARAMETER;
                } else if write && unsafe { (*file_object).write_access } == 0 {
                    status = STATUS_ACCESS_DENIED;
                } else if let Some((path, path_length)) = crate::file_services::path_for_file_object(file_object) {
                    if length != 0 {
                        let result = if write {
                            let input = unsafe { core::slice::from_raw_parts(buffer.cast_const(), length) };
                            write_file(&path[..path_length], offset as u64, input)
                        } else {
                            let output = unsafe { core::slice::from_raw_parts_mut(buffer, length) };
                            read_file(&path[..path_length], offset as u64, output)
                        };
                        match result {
                            Ok(count) => information = count,
                            Err(error) => status = error,
                        }
                    }
                    if status >= 0 && requested == FILE_USE_FILE_POINTER_POSITION {
                        unsafe { (*file_object).current_byte_offset = offset + information as i64; }
                    }
                } else { status = STATUS_INVALID_PARAMETER; }
            }
        }
        IRP_MJ_CLEANUP => {
            if file_object.is_null() { status = STATUS_INVALID_PARAMETER; }
            else {
                let _ = with_mounted(true, |_| Ok(()));
                unsafe { (*file_object).flags |= crate::nt_abi::io::FO_CLEANUP_COMPLETE; }
            }
        }
        IRP_MJ_CLOSE => {
            if file_object.is_null() { status = STATUS_INVALID_PARAMETER; }
            else {
                unsafe {
                    (*file_object).fs_context = ptr::null_mut();
                    (*file_object).device_object = ptr::null_mut();
                }
            }
        }
        IRP_MJ_FLUSH_BUFFERS => {
            status = with_mounted(true, |_| Ok(())).map(|_| STATUS_SUCCESS).unwrap_or(STATUS_IO_DEVICE_ERROR);
        }
        _ => status = STATUS_INVALID_DEVICE_REQUEST,
    }

    if !file_object.is_null() { unsafe { (*file_object).final_status = status; } }
    unsafe { wdm::io_complete_irp_status(irp, status, information, 0); }
    status
}

/// Flush the mounted FAT32 volume through its partition-backed block provider.
pub fn flush_volume() -> i32 {
    with_mounted(true, |_| Ok(())).map(|_| STATUS_SUCCESS).unwrap_or(STATUS_IO_DEVICE_ERROR)
}
