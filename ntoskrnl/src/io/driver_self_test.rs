//! I/O-manager and native driver-loader boot smoke tests.
//! These tests are kept separate from the production kernel entrypoint.
use crate::*;
use core::arch::asm;

static mut BEEP_TEST_STACK: [u8; 8192] = [0; 8192];

pub(crate) extern "C" fn beep_test_worker(_argument: *mut core::ffi::c_void) {
    unsafe { serial_write(b"Runtime beep.sys test started on scheduler-owned kernel thread.\r\n"); }
    run_beep_runtime_test();
}

pub(crate) fn run_beep_runtime_test() {
    // Enable verbose WDM tracing for this controlled test only. The same
    // filter can be toggled by DbgSetDebugFilterState once a live debugger
    // command channel is available.
    let _ = ntoskrnl::debug::set_filter(
        ntoskrnl::debug::COMPONENT_DRIVER,
        ntoskrnl::debug::LEVEL_INFO,
        true,
    );
    unsafe {
            serial_write(b"DRIVER DEBUG: verbose driver entry/dispatch tracing enabled for beep.sys test.\r\n");
            // Real runtime test: invoke the native driver loader and DriverEntry.
        if let Some(beep_bytes) = boot_beep_bytes() {
            use ntoskrnl::driver_loader::{DriverLoadError, DriverObject, load_allocated};
            const BEEP_NAME: [u16; 13] = [92, 68, 114, 105, 118, 101, 114, 92, 66, 101, 101, 112, 0];
            let driver_name = nt_abi::object::UNICODE_STRING {
                length: ((BEEP_NAME.len() - 1) * 2) as u16,
                maximum_length: (BEEP_NAME.len() * 2) as u16,
                buffer: BEEP_NAME.as_ptr() as *mut u16,
            };
            // DriverObject must remain valid for any device objects published by the driver.
            static mut BEEP_DRIVER_OBJECT: core::mem::MaybeUninit<DriverObject> = core::mem::MaybeUninit::uninit();
            let driver_object = core::ptr::addr_of_mut!(BEEP_DRIVER_OBJECT).cast::<DriverObject>();
            unsafe { driver_object.write(DriverObject {
                object_type: 4, size: core::mem::size_of::<DriverObject>() as i16,
                device_object: core::ptr::null_mut(), flags: 0,
                driver_start: core::ptr::null_mut(), driver_size: 0,
                driver_section: core::ptr::null_mut(), driver_extension: core::ptr::null_mut(),
                driver_name, hardware_database: core::ptr::null_mut(),
                fast_io_dispatch: core::ptr::null_mut(), driver_init: None,
                driver_start_io: None, driver_unload: None, major_function: [None; 28],
            }); }
            const BEEP_REGISTRY_PATH: [u16; 57] = [92, 82, 101, 103, 105, 115, 116, 114, 121, 92, 77, 97, 99, 104, 105, 110, 101, 92, 83, 121, 115, 116, 101, 109, 92, 67, 117, 114, 114, 101, 110, 116, 67, 111, 110, 116, 114, 111, 108, 83, 101, 116, 92, 83, 101, 114, 118, 105, 99, 101, 115, 92, 66, 101, 101, 112, 0];
            let mut registry_path = nt_abi::object::UNICODE_STRING {
                length: ((BEEP_REGISTRY_PATH.len() - 1) * 2) as u16,
                maximum_length: (BEEP_REGISTRY_PATH.len() * 2) as u16,
                buffer: BEEP_REGISTRY_PATH.as_ptr() as *mut u16,
            };
            serial_write(b"Runtime driver test: attempting native beep.sys load and DriverEntry.\r\n    ");
            match unsafe { load_allocated(beep_bytes, driver_object, (&mut registry_path as *mut nt_abi::object::UNICODE_STRING).cast()) } {
                Ok(loaded) => {
                    serial_write(b"beep.sys DriverEntry returned NTSTATUS success: image=");
                    serial_write_hex(loaded.image_base);
                    serial_write(b" size=");
                    unsafe { serial_write_decimal(loaded.image_size as usize); }
                    serial_write(b" entry=");
                    serial_write_hex(loaded.entry_point);
                    serial_write(b".\r\n    ");
                    let device_object = unsafe { (*driver_object).device_object };
                    let dispatch_count = unsafe { (*driver_object).major_function.iter().filter(|entry| entry.is_some()).count() };
                    serial_write(b"beep.sys device object=");
                    serial_write_hex(device_object as u64);
                    serial_write(b" dispatch-routines=");
                    unsafe { serial_write_decimal(dispatch_count); }
                    if !device_object.is_null() && dispatch_count != 0 {
                        serial_write(b"\r\n    beep.sys device and dispatch registration passed.\r\n    ");
                        // Route a real WDM IOCTL through beep.sys' registered dispatch.
                        // IOCTL_BEEP_SET = CTL_CODE(FILE_DEVICE_BEEP, 0, METHOD_BUFFERED, FILE_ANY_ACCESS).
                        match unsafe { submit_beep_ioctl(driver_object, device_object) } {
                            Some((create_status, create_io, dispatch_status, irp_status)) => {
                                serial_write(b"\r\n    BEEP CREATE: dispatch=");
                                serial_write_hex(create_status as u32 as u64);
                                serial_write(b" IoStatus=");
                                serial_write_hex(create_io as u32 as u64);
                                serial_write(b"\r\n    BEEP DEVICE CONTROL: dispatch=");
                                serial_write_hex(dispatch_status as u32 as u64);
                                serial_write(b" IoStatus=");
                                serial_write_hex(irp_status as u32 as u64);
                                if create_status >= 0 && create_io >= 0 &&
                                   (dispatch_status >= 0 || dispatch_status == nt_abi::ntstatus::STATUS_PENDING) &&
                                   (irp_status >= 0 || irp_status == nt_abi::ntstatus::STATUS_PENDING) {
                                    serial_write(b"\r\n    BEEP DISPATCH RESULT: PASS - create succeeded and IOCTL reached the registered device-control dispatch routine.\r\n    ");
                                } else {
                                    serial_write(b"\r\n    BEEP DISPATCH RESULT: FAIL - create or device-control request returned an unexpected failure status.\r\n    ");
                                }
                            }
                            None => serial_write(b"\r\n    BEEP DISPATCH RESULT: FAIL - required registered create/device-control dispatch routine was unavailable.\r\n    "),
                        }
                        // Separate hardware-path diagnostic; audio is supplemental evidence.
                        if unsafe { qemu_pc_speaker_diagnostic_tone() } {
                            serial_write(b"QEMU PC speaker diagnostic tone requested after beep.sys load.\r\n    ");
                        } else {
                            serial_write(b"QEMU PC speaker diagnostic tone could not be requested.\r\n    ");
                        }
                    } else {
                        serial_write(b"\r\n    beep.sys DriverEntry succeeded but device/dispatch registration is incomplete.\r\n    ");
                    }
                }
                Err(error) => {
                    serial_write(b"beep.sys runtime load failed before successful DriverEntry: ");
                    serial_write(match error {
                        DriverLoadError::Truncated => b"Truncated",
                        DriverLoadError::BadDosHeader => b"BadDosHeader",
                        DriverLoadError::BadPeSignature => b"BadPeSignature",
                        DriverLoadError::WrongMachine => b"WrongMachine",
                        DriverLoadError::WrongSubsystem => b"WrongSubsystem",
                        DriverLoadError::NotDll => b"NotDllOrExecutable",
                        DriverLoadError::InvalidHeader => b"InvalidHeader",
                        DriverLoadError::InvalidSection => b"InvalidSection",
                        DriverLoadError::ImageTooLarge => b"ImageTooLarge",
                        DriverLoadError::WrongBase => b"WrongBase",
                        DriverLoadError::ImportsUnsupported => b"ImportsUnsupported",
                        DriverLoadError::ImportNotFound => b"ImportNotFound",
                        DriverLoadError::InvalidImport => b"InvalidImport",
                        DriverLoadError::RelocationsUnsupported => b"RelocationsUnsupported",
                        DriverLoadError::RelocationUnsupported => b"RelocationUnsupported",
                        DriverLoadError::WxSection => b"WxSection",
                        DriverLoadError::EntryNotExecutable => b"EntryNotExecutable",
                        DriverLoadError::MappingFailed => b"MappingFailed",
                        DriverLoadError::NoMemory => b"NoMemory",
                        DriverLoadError::EntryFailed(_) => b"DriverEntryFailed",
                    });
                    serial_write(b"\r\n    ");
                }
            }
        } else {
            serial_write(b"Runtime driver test skipped: beep.sys was not staged by UEFI.\r\n");
        }
    }
}

/// Build and submit a minimal WDM request using the registered dispatch table.
/// The test models a device open before issuing IOCTL_BEEP_SET.
unsafe fn submit_beep_ioctl(
    driver_object: *mut ntoskrnl::driver_loader::DriverObject,
    device_object: *mut core::ffi::c_void,
) -> Option<(i32, i32, i32, i32)> {
    use nt_abi::io::{IO_STACK_LOCATION, IRP, IO_STATUS_BLOCK};
    const IRP_MJ_CREATE: usize = 0;
    const IOCTL_BEEP_SET: u32 = 0x0000_1000;
    if driver_object.is_null() || device_object.is_null() { return None; }
    let create_dispatch = unsafe { (*driver_object).major_function[IRP_MJ_CREATE] }?;
    if unsafe { (*driver_object).major_function[14] }.is_none() { return None; }

    // FILE_OBJECT x64 prefix: Type, Size, DeviceObject, FsContext, FsContext2.
    let mut file_storage = [0usize; 28];
    file_storage[0] = (0x00d8usize << 16) | 5usize;
    file_storage[1] = device_object as usize;
    let file_object = file_storage.as_mut_ptr().cast::<core::ffi::c_void>();

    // Create remains a direct callback until the I/O manager supports an
    // open-by-name/object-manager path for this test device.
    let mut create_stack = IO_STACK_LOCATION {
        major_function: IRP_MJ_CREATE as u8, minor_function: 0,
        flags: 0, control: 0, parameters: [0; 0x20],
        device_object: device_object.cast(), file_object,
        completion_routine: None, context: core::ptr::null_mut(),
    };
    let mut create_irp = make_beep_test_irp(&mut create_stack, core::ptr::null_mut());
    let create_status = unsafe {
        create_dispatch(device_object, (&mut create_irp as *mut IRP).cast::<core::ffi::c_void>())
    };
    let create_io_status = unsafe { create_irp.io_status.status.status };
    serial_write(b"beep.sys CREATE (direct callback): dispatch=");
    unsafe { serial_write_hex(create_status as u32 as u64); }
    serial_write(b" IoStatus=");
    unsafe { serial_write_hex(create_io_status as u32 as u64); }
    serial_write(b" file_object=");
    unsafe { serial_write_hex(file_object as u64); }
    serial_write(b" FsContext=");
    unsafe { serial_write_hex(file_storage[2] as u64); }
    serial_write(b"\r\n");
    if create_status < 0 || create_io_status < 0 {
        return Some((create_status, create_io_status, create_status, create_io_status));
    }

    // Submit METHOD_BUFFERED IOCTL_BEEP_SET through the kernel's WDM builder
    // and IoCallDriver path. The builder owns the IRP/system buffer lifetime.
    let mut parameters = [880u32, 200u32];
    let mut iosb: IO_STATUS_BLOCK = unsafe { core::mem::zeroed() };
    let irp = unsafe {
        io::io_build_device_io_control_request(
            IOCTL_BEEP_SET, device_object.cast(),
            parameters.as_mut_ptr().cast(), core::mem::size_of_val(&parameters) as u32,
            core::ptr::null_mut(), 0, 0,
            core::ptr::null_mut(), &mut iosb,
        )
    };
    if irp.is_null() {
        serial_write(b"BEEP IO MANAGER: IoBuildDeviceIoControlRequest returned NULL (thread/IRQL, device, or allocation precondition failed).\r\n");
        return Some((create_status, create_io_status, nt_abi::ntstatus::STATUS_INSUFFICIENT_RESOURCES as i32, nt_abi::ntstatus::STATUS_INSUFFICIENT_RESOURCES as i32));
    }
    let stack = unsafe { io::io_get_next_irp_stack_location(irp) };
    if stack.is_null() {
        unsafe { io::io_free_irp(irp); }
        return Some((create_status, create_io_status, nt_abi::ntstatus::STATUS_INVALID_PARAMETER as i32, nt_abi::ntstatus::STATUS_INVALID_PARAMETER as i32));
    }
    unsafe { (*stack).file_object = file_object; }

    serial_write(b"BEEP IOCTL PRE-DISPATCH: code=");
    unsafe { serial_write_hex(IOCTL_BEEP_SET as u64); }
    serial_write(b" input_length=");
    unsafe { serial_write_decimal(core::mem::size_of_val(&parameters)); }
    serial_write(b" output_length=0 system_buffer=");
    unsafe { serial_write_hex(core::ptr::read_unaligned((*irp).bytes.as_ptr().add(0x18).cast::<usize>()) as u64); }
    serial_write(b" file_object=");
    unsafe { serial_write_hex(file_object as u64); }
    serial_write(b" FsContext=");
    unsafe { serial_write_hex(file_storage[2] as u64); }
    serial_write(b" dispatch=IoCallDriver\r\n");

    // The synchronous builder requires a completion routine when no event is
    // supplied. It returns STATUS_SUCCESS to permit normal I/O-manager unwind.
    unsafe {
        io::io_set_completion_routine(
            irp, Some(beep_test_completion), core::ptr::null_mut(), 1, 1, 1,
        );
        // Report the callback actually stored in the ABI stack slot. Do not
        // patch the slot directly here: that would hide an IoSetCompletionRoutine
        // or stack-layout regression in the I/O manager.
        serial_write(b"BEEP COMPLETION ROUTINE: stack-slot callback=");
        serial_write_hex((*stack).completion_routine.map(|routine| routine as usize as u64).unwrap_or(0));
        serial_write(b" control=");
        serial_write_hex((*stack).control as u64);
        serial_write(b"\r\n");
    }
    let dispatch_status = unsafe { io::io_call_driver(device_object.cast(), irp) };
    let irp_status = unsafe { iosb.status.status };
    serial_write(b"BEEP IO MANAGER RETURN: IoCallDriver=");
    unsafe { serial_write_hex(dispatch_status as u32 as u64); }
    serial_write(b" IOSB.Status=");
    unsafe { serial_write_hex(irp_status as u32 as u64); }
    serial_write(b" IOSB.Information=");
    unsafe { serial_write_hex(iosb.information as u64); }
    serial_write(b"\r\n");
    Some((create_status, create_io_status, dispatch_status, irp_status))
}

unsafe extern "win64" fn beep_test_completion(
    _device: *mut io::DeviceObject,
    _irp: *mut io::WdmIrp,
    _context: *mut core::ffi::c_void,
) -> i32 {
    nt_abi::ntstatus::STATUS_SUCCESS as i32
}

fn make_beep_test_irp(
    stack: &mut nt_abi::io::IO_STACK_LOCATION,
    system_buffer: *mut core::ffi::c_void,
) -> nt_abi::io::IRP {
    use nt_abi::io::{IO_STATUS_BLOCK, IO_STATUS_BLOCK_STATUS, IRP};
    let mut irp = IRP {
        type_: 6, size: core::mem::size_of::<IRP>() as u16,
        mdl_address: core::ptr::null_mut(), flags: 0x10,
        associated_irp: [system_buffer as usize],
        thread_list_entry: [0; 2],
        io_status: IO_STATUS_BLOCK {
            status: IO_STATUS_BLOCK_STATUS { status: nt_abi::ntstatus::STATUS_PENDING },
            information: 0,
        },
        requestor_mode: 0, pending_returned: 0, stack_count: 1, current_location: 1,
        cancel: 0, cancel_irql: 0, apc_environment: 0, allocation_flags: 0,
        user_iosb: core::ptr::null_mut(), user_event: core::ptr::null_mut(),
        overlay: [0; 2], cancel_routine: core::ptr::null_mut(),
        user_buffer: core::ptr::null_mut(), tail: [0; 11],
    };
    irp.tail[8] = (stack as *mut nt_abi::io::IO_STACK_LOCATION) as usize;
    irp
}

/// Emit a short PIT channel-2 tone through the legacy PC speaker.
///
/// This is a QEMU audio-path diagnostic, not a substitute for issuing the
/// Windows Beep.sys device-control IRP. The workflow verifies captured samples.
unsafe fn qemu_pc_speaker_diagnostic_tone() -> bool {
    let mut speaker_port: u8;
    unsafe {
        asm!("in al, dx", in("dx") 0x61u16, out("al") speaker_port,
             options(nomem, nostack, preserves_flags));
    }
    // PIT channel 2, square-wave mode, approximately 880 Hz.
    let divisor: u16 = (1_193_182u32 / 880) as u16;
    unsafe {
        outb(0x43, 0xb6);
        outb(0x42, divisor as u8);
        outb(0x42, (divisor >> 8) as u8);
        outb(0x61, speaker_port | 0x03);
    }
    for _ in 0..20_000_000usize {
        core::hint::spin_loop();
    }
    unsafe { outb(0x61, speaker_port & !0x03); }
    true
}

