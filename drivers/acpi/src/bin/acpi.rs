#![no_std]
#![no_main]

use core::{cell::UnsafeCell, mem::MaybeUninit, sync::atomic::{AtomicBool, Ordering}};
use cognacos_acpi as acpi;
use cognacos_nt_abi::{bus::{ACPI_FIRMWARE_INTERFACE, INTERFACE_HEADER, ACPI_FIRMWARE_INTERFACE_VERSION}, io::DRIVER_OBJECT, ntstatus::*, types::PVOID};


const MAX_ACPI_DEVICES:usize=256;
const MAX_ACPI_PDOS:usize=64;
const FILE_DEVICE_BUS_EXTENDER:u32=0x0000002a;
const DEVICE_RELATIONS_BUS:u32=0;
#[repr(C)]
struct AcpiPdoExtension { signature:u64, node:u16, source_table:u16, identity:acpi::acpi_device::AcpiDeviceIdentity }
const ACPI_PDO_SIGNATURE:u64=0x4143504950444f31;
#[repr(C)]
struct AcpiDeviceRelations { count:u32, objects:[*mut cognacos_nt_abi::io::DEVICE_OBJECT;MAX_ACPI_PDOS] }
unsafe impl Sync for AcpiDeviceRelations {}
static PDO_RELATIONS_LOCK:AtomicBool=AtomicBool::new(false);
static mut ACPI_BUS_DEVICE:*mut cognacos_nt_abi::io::DEVICE_OBJECT=core::ptr::null_mut();
static mut ACPI_PDOS:[*mut cognacos_nt_abi::io::DEVICE_OBJECT;MAX_ACPI_PDOS]=[core::ptr::null_mut();MAX_ACPI_PDOS];
static mut ACPI_PDO_COUNT:usize=0;
static mut ACPI_BUS_RELATIONS:AcpiDeviceRelations=AcpiDeviceRelations{count:0,objects:[core::ptr::null_mut();MAX_ACPI_PDOS]};
unsafe extern "win64" {
 fn IoCreateDevice(driver:*mut DRIVER_OBJECT,extension_size:u32,device_name:*mut cognacos_nt_abi::object::UNICODE_STRING,device_type:u32,characteristics:u32,exclusive:u8,output:*mut *mut cognacos_nt_abi::io::DEVICE_OBJECT)->i32;
 fn IoMarkBusEnumeratedPdo(device:*mut cognacos_nt_abi::io::DEVICE_OBJECT)->i32;
 fn IoPnPEnumerateBus(bus:*mut cognacos_nt_abi::io::DEVICE_OBJECT)->i32;
}
core::arch::global_asm!(r#"
.intel_syntax noprefix
.section .text.import_thunks,"ax",@progbits
.global IoCreateDevice
.type IoCreateDevice,@function
IoCreateDevice:
 jmp QWORD PTR [rip + __imp_io_IoCreateDevice]
.size IoCreateDevice, .-IoCreateDevice
.global IoMarkBusEnumeratedPdo
.type IoMarkBusEnumeratedPdo,@function
IoMarkBusEnumeratedPdo:
 jmp QWORD PTR [rip + __imp_io_IoMarkBusEnumeratedPdo]
.size IoMarkBusEnumeratedPdo, .-IoMarkBusEnumeratedPdo
.global IoPnPEnumerateBus
.type IoPnPEnumerateBus,@function
IoPnPEnumerateBus:
 jmp QWORD PTR [rip + __imp_io_IoPnPEnumerateBus]
.size IoPnPEnumerateBus, .-IoPnPEnumerateBus
.section .data.imports.io,"aw",@progbits
.p2align 3
.global __imp_io_IoCreateDevice
__imp_io_IoCreateDevice: .quad 0
.global __imp_io_IoMarkBusEnumeratedPdo
__imp_io_IoMarkBusEnumeratedPdo: .quad 0
.global __imp_io_IoPnPEnumerateBus
__imp_io_IoPnPEnumerateBus: .quad 0
"#);

#[derive(Clone,Copy)]
struct AcpiDeviceRecord { table:Option<acpi::aml_tables::TableHandle>,node:u16,identity:acpi::acpi_device::AcpiDeviceIdentity }
impl AcpiDeviceRecord {
    const EMPTY:Self=Self{table:None,node:0,identity:acpi::acpi_device::AcpiDeviceIdentity::EMPTY};
}
struct AcpiState {
    ready: AtomicBool,
    madt: UnsafeCell<MaybeUninit<acpi::MadtInfo>>,
    mcfg: UnsafeCell<MaybeUninit<acpi::McfgInfo>>,
    mcfg_count: core::sync::atomic::AtomicUsize,
    dmar_present: AtomicBool,
    aml_manager: UnsafeCell<MaybeUninit<acpi::aml_tables::AmlTableManager>>,
    aml_barrier: UnsafeCell<acpi::aml_tables::AmlExecutionBarrier>,
    device_records: UnsafeCell<MaybeUninit<[AcpiDeviceRecord;MAX_ACPI_DEVICES]>>,
    device_count: core::sync::atomic::AtomicUsize,
}
unsafe impl Sync for AcpiState {}
static STATE: AcpiState = AcpiState {
    ready: AtomicBool::new(false),
    madt: UnsafeCell::new(MaybeUninit::uninit()),
    mcfg: UnsafeCell::new(MaybeUninit::uninit()),
    mcfg_count: core::sync::atomic::AtomicUsize::new(0),
    dmar_present: AtomicBool::new(false),
    aml_manager: UnsafeCell::new(MaybeUninit::new(acpi::aml_tables::AmlTableManager::new())),
    aml_barrier: UnsafeCell::new(acpi::aml_tables::AmlExecutionBarrier::new(100_000)),
    device_records: UnsafeCell::new(MaybeUninit::new([AcpiDeviceRecord::EMPTY;MAX_ACPI_DEVICES])),
    device_count: core::sync::atomic::AtomicUsize::new(0),
};


// ACPI.sys obtains firmware tables through the HAL's Windows-compatible
// HalAcpiGetTableEx export. It never dereferences a bootloader-supplied physical
// address or calls a Rust-private kernel/HAL function.
unsafe extern "win64" {
    fn HalAcpiGetTableEx(signature: u32, instance: u32, table: *mut *mut core::ffi::c_void, table_size: *mut u32) -> i32;
}
core::arch::global_asm!(r#"
.intel_syntax noprefix
.section .text.import_thunks,"ax",@progbits
.global HalAcpiGetTableEx
.type HalAcpiGetTableEx,@function
HalAcpiGetTableEx:
    jmp QWORD PTR [rip + __imp_hal_HalAcpiGetTableEx]
.size HalAcpiGetTableEx, .-HalAcpiGetTableEx

.section .data.imports.hal,"aw",@progbits
.p2align 3
.global __imp_hal_HalAcpiGetTableEx
.type __imp_hal_HalAcpiGetTableEx,@object
__imp_hal_HalAcpiGetTableEx: .quad 0
.size __imp_hal_HalAcpiGetTableEx,8
"#);

const MAX_ACPI_TABLE_LENGTH: u32 = 1024 * 1024;
unsafe fn query_hal_table(signature: [u8; 4], instance: u32) -> Result<&'static [u8], i32> {
    let mut table = core::ptr::null_mut();
    let mut length = 0u32;
    let status = unsafe { HalAcpiGetTableEx(u32::from_le_bytes(signature), instance, &mut table, &mut length) };
    if status != STATUS_SUCCESS { return Err(status); }
    if table.is_null() || length < 36 || length > MAX_ACPI_TABLE_LENGTH {
        return Err(STATUS_INVALID_PARAMETER);
    }
    Ok(unsafe { core::slice::from_raw_parts(table.cast::<u8>(), length as usize) })
}

const IRP_MJ_PNP: usize = 0x1b;
const IRP_MN_START_DEVICE: u8 = 0x00;
const IRP_MN_STOP_DEVICE: u8 = 0x04;
const IRP_MN_REMOVE_DEVICE: u8 = 0x02;
const IRP_MN_QUERY_DEVICE_RELATIONS: u8 = 0x07;
const IRP_MN_QUERY_INTERFACE: u8 = 0x08;

unsafe extern "win64" fn pnp_dispatch(device: *mut cognacos_nt_abi::io::DEVICE_OBJECT, irp: *mut cognacos_nt_abi::io::IRP) -> i32 {
    if irp.is_null() { return STATUS_INVALID_PARAMETER; }
    let stack = unsafe { (*irp).tail[8] as *mut cognacos_nt_abi::io::IO_STACK_LOCATION };
    if stack.is_null() { return STATUS_INVALID_PARAMETER; }
    let status = match unsafe { (*stack).minor_function } {
        IRP_MN_START_DEVICE if STATE.ready.load(Ordering::Acquire) => STATUS_SUCCESS,
        IRP_MN_STOP_DEVICE | IRP_MN_REMOVE_DEVICE => STATUS_SUCCESS,
        IRP_MN_QUERY_DEVICE_RELATIONS => {
            let relation_type=unsafe{core::ptr::read_unaligned((*stack).parameters.as_ptr().cast::<u32>())};
            if relation_type!=DEVICE_RELATIONS_BUS || device!=unsafe{ACPI_BUS_DEVICE} {
                STATUS_NOT_SUPPORTED
            } else {
                while PDO_RELATIONS_LOCK.compare_exchange(false,true,Ordering::Acquire,Ordering::Relaxed).is_err() {
                    core::hint::spin_loop();
                }
                let relations=core::ptr::addr_of_mut!(ACPI_BUS_RELATIONS);
                unsafe {
                    (*relations).count=ACPI_PDO_COUNT as u32;
                    for i in 0..MAX_ACPI_PDOS { (*relations).objects[i]=ACPI_PDOS[i]; }
                    (*irp).io_status.information=relations as usize;
                }
                PDO_RELATIONS_LOCK.store(false,Ordering::Release);
                STATUS_SUCCESS
            }
        },
        IRP_MN_QUERY_INTERFACE => {
            let parameters = unsafe { &(*stack).parameters };
            let guid = unsafe { core::ptr::read_unaligned(parameters.as_ptr().cast::<*const cognacos_nt_abi::bus::GUID>()) };
            let size = unsafe { core::ptr::read_unaligned(parameters.as_ptr().add(8).cast::<u16>()) };
            let version = unsafe { core::ptr::read_unaligned(parameters.as_ptr().add(10).cast::<u16>()) };
            let output = unsafe { core::ptr::read_unaligned(parameters.as_ptr().add(16).cast::<*mut ACPI_FIRMWARE_INTERFACE>()) };
            if guid.is_null() || output.is_null() {
                STATUS_INVALID_PARAMETER
            } else if unsafe { *guid } != cognacos_nt_abi::bus::GUID_ACPI_FIRMWARE_INTERFACE
                || version as u32 != ACPI_FIRMWARE_INTERFACE_VERSION
                || (size as usize) < core::mem::size_of::<ACPI_FIRMWARE_INTERFACE>() {
                STATUS_NOT_SUPPORTED
            } else {
                unsafe { cognac_acpi_get_firmware_interface(output) }
            }
        },
        IRP_MN_START_DEVICE => STATUS_DEVICE_NOT_READY,
        _ => STATUS_INVALID_DEVICE_REQUEST,
    };
    unsafe { (*irp).io_status.status.status = status; (*irp).io_status.information = 0; }
    status
}
unsafe extern "win64" fn driver_unload(_driver: *mut DRIVER_OBJECT) {
    STATE.ready.store(false, Ordering::Release);
}

fn collect_device_identities(
    manager:&acpi::aml_tables::AmlTableManager,
    handle:acpi::aml_tables::TableHandle,
    table_sources:&[&[u8]],
    gate:&acpi::aml_tables::AmlExecutionBarrier,
    records:&mut [AcpiDeviceRecord;MAX_ACPI_DEVICES],
    count:&mut usize,
)->Result<(),i32>{
    let info=manager.table_info(handle).map_err(|_|STATUS_INVALID_PARAMETER)?;
    let namespace=manager.namespace();
    for index in info.namespace_start..info.namespace_end{
        let node=&namespace.nodes[index as usize];
        if !matches!(node.object_type,acpi::aml::AmlObjectType::Device|acpi::aml::AmlObjectType::Processor|acpi::aml::AmlObjectType::PowerResource|acpi::aml::AmlObjectType::ThermalZone){continue}
        let mut methods=acpi::acpi_device::IntegerMethodInvoker;
        match manager.evaluate_device_identity(handle,table_sources,index,gate,&mut methods){
            Ok(identity)=>{
                if *count>=records.len(){return Err(STATUS_INSUFFICIENT_RESOURCES)}
                records[*count]=AcpiDeviceRecord{table:Some(handle),node:index,identity};
                *count+=1;
            }
            // An unsupported AML method/value suppresses this device's
            // identity record, not the entire ACPI bus driver. Later AML
            // runtime milestones can retry it with a richer evaluator.
            Err(_)=>{},
        }
    }
    Ok(())
}

/// DriverEntry validates firmware, loads DSDT/SSDT namespace definitions, and
/// builds the ACPI-1 device identity catalogue. PDO publication and BusRelations
/// remain ACPI-4 work.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn DriverEntry(driver: PVOID, _registry_path: PVOID) -> i32 {
    if driver.is_null() { return STATUS_INVALID_PARAMETER; }
    let object = unsafe { &mut *(driver as *mut DRIVER_OBJECT) };
    let madt_bytes = match unsafe { query_hal_table(*b"APIC", 0) } {
        Ok(bytes) => bytes,
        Err(status) => return status,
    };
    let madt = match acpi::parse_madt(madt_bytes) {
        Ok(info) => info,
        Err(error) => return acpi::error_to_ntstatus(error),
    };
    unsafe { (*STATE.madt.get()).write(madt); }

    // MCFG and DMAR are optional. HAL reports a missing table distinctly from
    // malformed firmware data; malformed tables fail driver initialization.
    match unsafe { query_hal_table(*b"MCFG", 0) } {
        Ok(bytes) => match acpi::parse_mcfg(bytes) {
            Ok(info) => {
                STATE.mcfg_count.store(info.count, Ordering::Release);
                unsafe { (*STATE.mcfg.get()).write(info); }
            }
            Err(error) => return acpi::error_to_ntstatus(error),
        },
        Err(STATUS_NOT_FOUND) => {}
        Err(status) => return status,
    }
    match unsafe { query_hal_table(*b"DMAR", 0) } {
        Ok(bytes) => match acpi::parse_dmar(bytes) {
            Ok(_) => STATE.dmar_present.store(true, Ordering::Release),
            Err(error) => return acpi::error_to_ntstatus(error),
        },
        Err(STATUS_NOT_FOUND) => {}
        Err(status) => return status,
    }
    // DSDT is obtained through FADT by the HAL; SSDTs are root-table entries.
    // Retain the firmware-owned slices for all future AML method dispatch.
    let dsdt=match unsafe{query_hal_table(*b"DSDT",0)}{Ok(bytes)=>bytes,Err(status)=>return status};
    let mut ssdt_tables:[&'static [u8];acpi::aml_tables::MAX_AML_TABLES-1]=[&[];acpi::aml_tables::MAX_AML_TABLES-1];
    let mut ssdt_count=0usize;
    for instance in 0..acpi::aml_tables::MAX_AML_TABLES{
        match unsafe{query_hal_table(*b"SSDT",instance as u32)}{
            Ok(bytes)=>{if ssdt_count==ssdt_tables.len(){return STATUS_INSUFFICIENT_RESOURCES;}ssdt_tables[ssdt_count]=bytes;ssdt_count+=1;},
            Err(STATUS_NOT_FOUND)=>break,
            Err(status)=>return status,
        }
    }
    let mut table_sources:[&[u8];acpi::aml_tables::MAX_AML_TABLES]=[&[];acpi::aml_tables::MAX_AML_TABLES];
    table_sources[0]=dsdt;
    for index in 0..ssdt_count{table_sources[index+1]=ssdt_tables[index];}
    let manager=unsafe{(*STATE.aml_manager.get()).assume_init_mut()};
    let barrier=unsafe{&mut *STATE.aml_barrier.get()};
    let loaded=match manager.load_firmware_tables(dsdt,&ssdt_tables[..ssdt_count],barrier){
        Ok(tables)=>tables,
        Err(acpi::aml_tables::TableError::NamespaceCapacity|acpi::aml_tables::TableError::TableLimit)=>return STATUS_INSUFFICIENT_RESOURCES,
        Err(_)=>return STATUS_INVALID_PARAMETER,
    };
    let gate=unsafe{&*STATE.aml_barrier.get()};
    let records=unsafe{(*STATE.device_records.get()).assume_init_mut()};
    let mut identity_count=0usize;
    if let Err(status)=collect_device_identities(manager,loaded.dsdt,&table_sources[..ssdt_count+1],gate,records,&mut identity_count){return status;}
    for index in 0..loaded.ssdt_count{
        let handle=match loaded.ssdts[index]{Some(handle)=>handle,None=>return STATUS_INVALID_PARAMETER};
        if let Err(status)=collect_device_identities(manager,handle,&table_sources[..ssdt_count+1],gate,records,&mut identity_count){return status;}
    }
    // Create an ACPI bus device and real WDM PDOs for firmware devices
    // reported present and enabled. Each PDO owns an immutable identity copy.
    if identity_count>MAX_ACPI_PDOS { return STATUS_INSUFFICIENT_RESOURCES; }
    let mut bus_device=core::ptr::null_mut();
    let create_bus=unsafe{IoCreateDevice(object,0,core::ptr::null_mut(),FILE_DEVICE_BUS_EXTENDER,0,0,&mut bus_device)};
    if create_bus!=STATUS_SUCCESS || bus_device.is_null() { return if create_bus==STATUS_SUCCESS {STATUS_INSUFFICIENT_RESOURCES} else {create_bus}; }
    unsafe { ACPI_BUS_DEVICE=bus_device; }
    let mut pdo_count=0usize;
    for record in records[..identity_count].iter() {
        let status=record.identity.sta;
        if status&(acpi::acpi_device::DEVICE_PRESENT|acpi::acpi_device::DEVICE_ENABLED)
            !=(acpi::acpi_device::DEVICE_PRESENT|acpi::acpi_device::DEVICE_ENABLED) { continue; }
        let mut pdo=core::ptr::null_mut();
        let create=unsafe{IoCreateDevice(object,core::mem::size_of::<AcpiPdoExtension>() as u32,
            core::ptr::null_mut(),FILE_DEVICE_BUS_EXTENDER,0,0,&mut pdo)};
        if create!=STATUS_SUCCESS || pdo.is_null() { return if create==STATUS_SUCCESS {STATUS_INSUFFICIENT_RESOURCES} else {create}; }
        unsafe {
            let extension=(*pdo).device_extension.cast::<AcpiPdoExtension>();
            if extension.is_null() { return STATUS_INSUFFICIENT_RESOURCES; }
            extension.write(AcpiPdoExtension{signature:ACPI_PDO_SIGNATURE,node:record.node,source_table:0,identity:record.identity});
        }
        let marked=unsafe{IoMarkBusEnumeratedPdo(pdo)};
        if marked!=STATUS_SUCCESS { return marked; }
        unsafe { ACPI_PDOS[pdo_count]=pdo; }
        pdo_count+=1;
    }
    unsafe {
        ACPI_PDO_COUNT=pdo_count;
        ACPI_BUS_RELATIONS.count=pdo_count as u32;
        for i in 0..MAX_ACPI_PDOS { ACPI_BUS_RELATIONS.objects[i]=ACPI_PDOS[i]; }
    }
    STATE.device_count.store(identity_count,Ordering::Release);
    object.major_function[IRP_MJ_PNP] = Some(pnp_dispatch);
    object.driver_unload = Some(driver_unload);
    STATE.ready.store(true, Ordering::Release);
    let enumerate=unsafe{IoPnPEnumerateBus(bus_device)};
    if enumerate!=STATUS_SUCCESS && enumerate!=STATUS_PENDING { return enumerate; }
    STATUS_SUCCESS
}

/// Copy a validated firmware table through the versioned ACPI bus interface.
/// The caller owns the destination buffer; no firmware physical pointer escapes.
unsafe extern "win64" fn query_firmware_table(
    context: PVOID,
    signature: *const u8,
    instance: u32,
    buffer: PVOID,
    buffer_length: u32,
    required_length: *mut u32,
) -> i32 {
    if context.is_null() || signature.is_null() || required_length.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    if !STATE.ready.load(Ordering::Acquire) {
        return STATUS_DEVICE_NOT_READY;
    }
    if context != core::ptr::addr_of!(STATE).cast_mut().cast() {
        return STATUS_INVALID_PARAMETER;
    }
    let signature = unsafe { core::slice::from_raw_parts(signature, 4) };
    let signature = [signature[0], signature[1], signature[2], signature[3]];
    let table = match unsafe { query_hal_table(signature, instance) } {
        Ok(table) => table,
        Err(status) => return status,
    };
    let Ok(length) = u32::try_from(table.len()) else {
        return STATUS_INSUFFICIENT_RESOURCES;
    };
    unsafe { required_length.write(length); }
    if buffer_length < length {
        return STATUS_BUFFER_TOO_SMALL;
    }
    if buffer.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    unsafe { core::ptr::copy_nonoverlapping(table.as_ptr(), buffer.cast::<u8>(), table.len()); }
    STATUS_SUCCESS
}

/// Return ACPI.sys' stable, versioned firmware-table interface to a consumer.
unsafe extern "win64" fn cognac_acpi_get_firmware_interface(
    output: *mut ACPI_FIRMWARE_INTERFACE,
) -> i32 {
    if output.is_null() { return STATUS_INVALID_PARAMETER; }
    if !STATE.ready.load(Ordering::Acquire) { return STATUS_DEVICE_NOT_READY; }
    unsafe {
        output.write(ACPI_FIRMWARE_INTERFACE {
            header: INTERFACE_HEADER {
                size: core::mem::size_of::<ACPI_FIRMWARE_INTERFACE>() as u16,
                version: ACPI_FIRMWARE_INTERFACE_VERSION as u16,
                context: core::ptr::addr_of!(STATE).cast_mut().cast::<core::ffi::c_void>(),
                interface_reference: None,
                interface_dereference: None,
            },
            query_table: Some(query_firmware_table),
        });
    }
    STATUS_SUCCESS
}

#[used]
#[unsafe(link_section = ".rodata")]
static DRIVER_SIGNATURE: [u8; 8] = *b"ACPI____";
#[used]
#[unsafe(link_section = ".data")]
static DRIVER_COOKIE: u64 = 0x5244_4F54_4F53_0001;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    loop { unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)); } }
}
