//! Native ACPI bus-driver entry point and WDM/PnP integration.
//!
//! This binary owns the ACPI bus FDO, child PDO bookkeeping, firmware-table
//! lifetime, and dispatch glue. The library crate performs bounded parsing and
//! AML operations; kernel PnP owns devnode/resource arbitration, PCI owns
//! configuration space, and HAL owns interrupt-controller programming.
//! Keep this layer as an adapter: do not duplicate parsers or touch APIC/PCI
//! hardware directly. Every unsafe block must document the kernel contract
//! that makes its pointers, IRQL, object references, and lifetime valid.
/*
 * FIXME(ACPI-PRODUCTION) — ACPI WDM bus driver and PnP integration
 * FIXME(ACPI-PRODUCTION): This entry point wires table loading, AML identity evaluation, bus FDO/PDO publication, resource queries, and firmware-table interface callbacks together, but several lifecycle paths intentionally fail closed or remain incomplete. Intended completion: transactional DriverEntry rollback, complete IRP/PnP state machine and completion ownership, reference-counted PDO/relations lifetime, correct _PRS/_SRS/_CRS resource programming and verification, dependency/hotplug notifications, interface reference/dereference, unload synchronization, and robust status mapping. Do not claim production readiness until all allocations/registrations unwind on failure and QEMU plus hardware lifecycle tests pass.
 */

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
struct AcpiPdoExtension { signature:u64, node:u16, devnode_index:u32, table:Option<acpi::aml_tables::TableHandle>, aml_bytes:*const u8, aml_length:usize, identity:acpi::acpi_device::AcpiDeviceIdentity }
const ACPI_PDO_SIGNATURE:u64=0x4143504950444f31;
#[repr(C)]
struct AcpiDeviceRelations { count:u32, objects:[*mut cognacos_nt_abi::io::DEVICE_OBJECT;MAX_ACPI_PDOS] }
unsafe impl Sync for AcpiDeviceRelations {}
static PDO_RELATIONS_LOCK:AtomicBool=AtomicBool::new(false);
static mut ACPI_BUS_DEVICE:*mut cognacos_nt_abi::io::DEVICE_OBJECT=core::ptr::null_mut();
static mut ACPI_PDOS:[*mut cognacos_nt_abi::io::DEVICE_OBJECT;MAX_ACPI_PDOS]=[core::ptr::null_mut();MAX_ACPI_PDOS];
static mut ACPI_PDO_COUNT:usize=0;
static mut ACPI_BUS_RELATIONS:AcpiDeviceRelations=AcpiDeviceRelations{count:0,objects:[core::ptr::null_mut();MAX_ACPI_PDOS]};
// Windows x64 KMUTEX is an alias of KMUTANT: a 24-byte dispatcher header,
// two-pointer LIST_ENTRY, owner-thread pointer, mutant flags, and APC-disable
// byte. The complete 56-byte object must be reserved even though CognacOS
// currently keeps owner/list bookkeeping in the kernel dispatcher side table.
const MAX_ACPI_METHOD_MUTEXES:usize=acpi::aml::MAX_NAMESPACE_NODES;
const _:()=assert!(core::mem::size_of::<[u64;7]>()==56);
#[repr(C,align(8))]
struct KernelAmlMutex { initialized:core::sync::atomic::AtomicU8, header:core::cell::UnsafeCell<[u64;7]> }
const _:()=assert!(core::mem::offset_of!(KernelAmlMutex,header)==8);
const _:()=assert!(core::mem::size_of::<KernelAmlMutex>()==64);
impl KernelAmlMutex {
 const fn new()->Self{Self{initialized:core::sync::atomic::AtomicU8::new(0),header:core::cell::UnsafeCell::new([0;3])}}
 fn object(&self)->*mut core::ffi::c_void{self.header.get().cast()}
}
unsafe impl Sync for KernelAmlMutex {}
struct KernelAmlMethodSynchronizer;
static KERNEL_AML_METHOD_MUTEXES:[KernelAmlMutex;MAX_ACPI_METHOD_MUTEXES]=
 [const{KernelAmlMutex::new()};MAX_ACPI_METHOD_MUTEXES];
static KERNEL_AML_METHOD_SYNCHRONIZER:KernelAmlMethodSynchronizer=KernelAmlMethodSynchronizer;
unsafe extern "win64" {
 fn KeInitializeMutex(mutex:*mut core::ffi::c_void,level:u32);
 fn KeWaitForSingleObject(object:*mut core::ffi::c_void,wait_reason:i32,wait_mode:i32,alertable:u8,timeout:*const i64)->i32;
 fn KeReleaseMutex(mutex:*mut core::ffi::c_void,wait:u8)->i32;
}
impl acpi::aml_exec::AmlMethodSynchronizer for KernelAmlMethodSynchronizer {
 fn acquire(&self,method_index:u16,_sync_level:u8)->Result<(),acpi::aml_exec::ExecError>{
  let mutex=KERNEL_AML_METHOD_MUTEXES.get(method_index as usize)
   .ok_or(acpi::aml_exec::ExecError::SynchronizationUnavailable)?;
  loop {
   match mutex.initialized.load(Ordering::Acquire) {
    2=>break,
    0=>if mutex.initialized.compare_exchange(0,1,Ordering::AcqRel,Ordering::Acquire).is_ok(){
     // KeInitializeMutex initializes the kernel dispatcher header and registers
     // this address with the kernel's mutant owner/recursion table.
     unsafe{KeInitializeMutex(mutex.object(),0);}
     mutex.initialized.store(2,Ordering::Release);
     break;
    },
    _=>core::hint::spin_loop(),
   }
  }
  // Executive/KernelMode, non-alertable, infinite wait. The kernel performs
  // owner transfer and recursive acquisition; a non-success status fails closed.
  let status=unsafe{KeWaitForSingleObject(mutex.object(),0,0,0,core::ptr::null())};
  if status==STATUS_SUCCESS||status==0x0000_0080{Ok(())}
  else{Err(acpi::aml_exec::ExecError::ExecutionBusy)}
 }
 fn release(&self,method_index:u16){
  if let Some(mutex)=KERNEL_AML_METHOD_MUTEXES.get(method_index as usize){
   if mutex.initialized.load(Ordering::Acquire)==2{
    unsafe{let _=KeReleaseMutex(mutex.object(),0);}
   }
  }
 }
}

unsafe extern "win64" {
 fn IoCreateDevice(driver:*mut DRIVER_OBJECT,extension_size:u32,device_name:*mut cognacos_nt_abi::object::UNICODE_STRING,device_type:u32,characteristics:u32,exclusive:u8,output:*mut *mut cognacos_nt_abi::io::DEVICE_OBJECT)->i32;
 fn IoMarkBusEnumeratedPdo(device:*mut cognacos_nt_abi::io::DEVICE_OBJECT)->i32;
 fn IoPnPRegisterBusDevice(enumerator_id:u32,identity_hash:u64,device:*mut cognacos_nt_abi::io::DEVICE_OBJECT,output_index:*mut u32)->i32;
 fn IoPnPEnumerateBus(bus:*mut cognacos_nt_abi::io::DEVICE_OBJECT)->i32;
 fn ExAllocatePoolWithTag(pool_type:u32,size:usize,tag:u32)->*mut u8;
 fn ExFreePoolWithTag(buffer:*mut u8,tag:u32);
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
.global IoPnPRegisterBusDevice
.type IoPnPRegisterBusDevice,@function
IoPnPRegisterBusDevice:
 jmp QWORD PTR [rip + __imp_io_IoPnPRegisterBusDevice]
.size IoPnPRegisterBusDevice, .-IoPnPRegisterBusDevice
.global IoPnPEnumerateBus
.type IoPnPEnumerateBus,@function
IoPnPEnumerateBus:
 jmp QWORD PTR [rip + __imp_io_IoPnPEnumerateBus]
.size IoPnPEnumerateBus, .-IoPnPEnumerateBus
.global ExAllocatePoolWithTag
.type ExAllocatePoolWithTag,@function
ExAllocatePoolWithTag:
 jmp QWORD PTR [rip + __imp_ex_ExAllocatePoolWithTag]
.size ExAllocatePoolWithTag, .-ExAllocatePoolWithTag
.global ExFreePoolWithTag
.type ExFreePoolWithTag,@function
ExFreePoolWithTag:
 jmp QWORD PTR [rip + __imp_ex_ExFreePoolWithTag]
.size ExFreePoolWithTag, .-ExFreePoolWithTag
.section .data.imports.ke,"aw",@progbits
.p2align 3
.global __imp_ke_KeInitializeMutex
__imp_ke_KeInitializeMutex: .quad 0
.global __imp_ke_KeWaitForSingleObject
__imp_ke_KeWaitForSingleObject: .quad 0
.global __imp_ke_KeReleaseMutex
__imp_ke_KeReleaseMutex: .quad 0
.section .text.import_thunks,"ax",@progbits
.global KeInitializeMutex
.type KeInitializeMutex,@function
KeInitializeMutex:
 jmp QWORD PTR [rip + __imp_ke_KeInitializeMutex]
.size KeInitializeMutex, .-KeInitializeMutex
.global KeWaitForSingleObject
.type KeWaitForSingleObject,@function
KeWaitForSingleObject:
 jmp QWORD PTR [rip + __imp_ke_KeWaitForSingleObject]
.size KeWaitForSingleObject, .-KeWaitForSingleObject
.global KeReleaseMutex
.type KeReleaseMutex,@function
KeReleaseMutex:
 jmp QWORD PTR [rip + __imp_ke_KeReleaseMutex]
.size KeReleaseMutex, .-KeReleaseMutex

.section .data.imports.io,"aw",@progbits
.p2align 3
.global __imp_io_IoCreateDevice
__imp_io_IoCreateDevice: .quad 0
.global __imp_io_IoMarkBusEnumeratedPdo
__imp_io_IoMarkBusEnumeratedPdo: .quad 0
.global __imp_io_IoPnPRegisterBusDevice
__imp_io_IoPnPRegisterBusDevice: .quad 0
.global __imp_io_IoPnPEnumerateBus
__imp_io_IoPnPEnumerateBus: .quad 0
.section .data.imports.ex,"aw",@progbits
.p2align 3
.global __imp_ex_ExAllocatePoolWithTag
__imp_ex_ExAllocatePoolWithTag: .quad 0
.global __imp_ex_ExFreePoolWithTag
__imp_ex_ExFreePoolWithTag: .quad 0
"#);

#[derive(Clone,Copy)]
struct AcpiDeviceRecord { table:Option<acpi::aml_tables::TableHandle>,node:u16,aml_bytes:&'static [u8],identity:acpi::acpi_device::AcpiDeviceIdentity }
impl AcpiDeviceRecord {
    const EMPTY:Self=Self{table:None,node:0,aml_bytes:&[],identity:acpi::acpi_device::AcpiDeviceIdentity::EMPTY};
}
struct AcpiState {
    ready: AtomicBool,
    unloading: AtomicBool,
    active_operations: core::sync::atomic::AtomicUsize,
    interface_references: core::sync::atomic::AtomicUsize,
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
    unloading: AtomicBool::new(false),
    active_operations: core::sync::atomic::AtomicUsize::new(0),
    interface_references: core::sync::atomic::AtomicUsize::new(0),
    madt: UnsafeCell::new(MaybeUninit::uninit()),
    mcfg: UnsafeCell::new(MaybeUninit::uninit()),
    mcfg_count: core::sync::atomic::AtomicUsize::new(0),
    dmar_present: AtomicBool::new(false),
    aml_manager: UnsafeCell::new(MaybeUninit::new(acpi::aml_tables::AmlTableManager::new())),
    aml_barrier: UnsafeCell::new(acpi::aml_tables::AmlExecutionBarrier::new(100_000)),
    device_records: UnsafeCell::new(MaybeUninit::new([AcpiDeviceRecord::EMPTY;MAX_ACPI_DEVICES])),
    device_count: core::sync::atomic::AtomicUsize::new(0),
};

/// Pins driver code and AML namespace state while a dispatch callback is
/// executing. DriverUnload closes admission first, then drains this count
/// before the image or its static dispatcher headers can be reclaimed.
struct AcpiOperationGuard;
impl AcpiOperationGuard {
    fn enter()->Option<Self>{
        if STATE.unloading.load(Ordering::Acquire){return None}
        STATE.active_operations.fetch_add(1,Ordering::AcqRel);
        if STATE.unloading.load(Ordering::Acquire){
            STATE.active_operations.fetch_sub(1,Ordering::AcqRel);
            return None
        }
        Some(Self)
    }
}
impl Drop for AcpiOperationGuard {
    fn drop(&mut self){STATE.active_operations.fetch_sub(1,Ordering::AcqRel);}
}


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
const IRP_MN_QUERY_CAPABILITIES: u8 = 0x09;
const IRP_MN_QUERY_RESOURCES: u8 = 0x0a;
const IRP_MN_QUERY_RESOURCE_REQUIREMENTS: u8 = 0x0b;
const IRP_MN_QUERY_ID: u8 = 0x13;
const BUS_QUERY_DEVICE_ID: u32 = 0;
const BUS_QUERY_INSTANCE_ID: u32 = 1;
const BUS_QUERY_HARDWARE_IDS: u32 = 2;
const BUS_QUERY_COMPATIBLE_IDS: u32 = 3;
const PAGED_POOL: u32 = 1;
const ACPI_POOL_TAG: u32 = u32::from_le_bytes(*b"ACPI");


/// Build a WDM-owned UTF-16 ID buffer. PnP owns and frees this paged-pool
/// allocation after consuming IoStatus.Information.
unsafe fn allocate_id_string(id:&acpi::acpi_device::AcpiId)->Result<usize,i32>{
    if id.length==0||id.as_bytes().iter().any(|b|*b<=0x20||*b>0x7f||*b==b',') {
        return Err(STATUS_INVALID_PARAMETER);
    }
    let units=(id.length as usize+1).checked_mul(2).ok_or(STATUS_INSUFFICIENT_RESOURCES)?;
    let buffer=unsafe{ExAllocatePoolWithTag(PAGED_POOL,units,ACPI_POOL_TAG)};
    if buffer.is_null(){return Err(STATUS_INSUFFICIENT_RESOURCES);}
    let output=buffer.cast::<u16>();
    for (index,byte) in id.as_bytes().iter().enumerate(){unsafe{output.add(index).write(*byte as u16);}}
    unsafe{output.add(id.length as usize).write(0);}
    Ok(buffer as usize)
}

unsafe fn allocate_prefixed_id_list(ids:&[acpi::acpi_device::AcpiId])->Result<usize,i32>{
    if ids.is_empty(){return Err(STATUS_NOT_SUPPORTED);}
    // ACPI enumerator IDs use the ACPI\\ namespace prefix; the wildcard
    // form is also emitted to match the WDK/ReactOS ACPI bus ID convention.
    let mut units=1usize;
    for id in ids {
        if id.length==0||id.as_bytes().iter().any(|b|*b<=0x20||*b>0x7f||*b==b','||*b==b'\\') {
            return Err(STATUS_INVALID_PARAMETER);
        }
        units=units.checked_add(5+id.length as usize+1).ok_or(STATUS_INSUFFICIENT_RESOURCES)?;
        units=units.checked_add(1+id.length as usize+1).ok_or(STATUS_INSUFFICIENT_RESOURCES)?;
    }
    let bytes=units.checked_mul(core::mem::size_of::<u16>()).ok_or(STATUS_INSUFFICIENT_RESOURCES)?;
    let buffer=unsafe{ExAllocatePoolWithTag(PAGED_POOL,bytes,ACPI_POOL_TAG)};
    if buffer.is_null(){return Err(STATUS_INSUFFICIENT_RESOURCES);}
    let output=buffer.cast::<u16>();
    let mut cursor=0usize;
    for id in ids {
        for byte in b"ACPI\\".iter().chain(id.as_bytes().iter()) {
            unsafe{output.add(cursor).write(*byte as u16);}
            cursor+=1;
        }
        unsafe{output.add(cursor).write(0);}
        cursor+=1;
        unsafe{output.add(cursor).write(b'*' as u16);}
        cursor+=1;
        for byte in id.as_bytes() {
            unsafe{output.add(cursor).write(*byte as u16);}
            cursor+=1;
        }
        unsafe{output.add(cursor).write(0);}
        cursor+=1;
    }
    unsafe{output.add(cursor).write(0);}
    Ok(buffer as usize)
}

unsafe fn allocate_acpi_device_id(id:&acpi::acpi_device::AcpiId)->Result<usize,i32>{
    if id.length==0||id.as_bytes().iter().any(|b|*b<=0x20||*b>0x7f||*b==b','||*b==b'\\') {
        return Err(STATUS_INVALID_PARAMETER);
    }
    let units=5usize.checked_add(id.length as usize+1).ok_or(STATUS_INSUFFICIENT_RESOURCES)?;
    let buffer=unsafe{ExAllocatePoolWithTag(PAGED_POOL,units*2,ACPI_POOL_TAG)};
    if buffer.is_null(){return Err(STATUS_INSUFFICIENT_RESOURCES);}
    let output=buffer.cast::<u16>();
    for (index,byte) in b"ACPI\\".iter().chain(id.as_bytes().iter()).enumerate(){
        unsafe{output.add(index).write(*byte as u16);}
    }
    unsafe{output.add(5+id.length as usize).write(0);}
    Ok(buffer as usize)
}

fn decimal_uid(value:u64)->acpi::acpi_device::AcpiId{
    let mut digits=[0u8;20];
    let mut value=value;
    let mut cursor=digits.len();
    loop {
        cursor-=1;
        digits[cursor]=b'0'+(value%10) as u8;
        value/=10;
        if value==0{break;}
    }
    let mut id=acpi::acpi_device::AcpiId{bytes:[0;acpi::acpi_device::MAX_ID_STRING],length:0};
    let count=digits.len()-cursor;
    id.bytes[..count].copy_from_slice(&digits[cursor..]);
    id.length=count as u8;
    id
}

#[repr(C)]
struct AcpiDeviceCapabilities {
    size:u16,
    version:u16,
    flags:u32,
    address:u32,
    ui_number:u32,
    device_state:[u32;7],
    system_wake:u32,
    device_wake:u32,
    d1_latency:u32,
    d2_latency:u32,
    d3_latency:u32,
}
const DEVICE_CAPABILITIES_SIZE:usize=core::mem::size_of::<AcpiDeviceCapabilities>();
const _:()=assert!(DEVICE_CAPABILITIES_SIZE==0x40);
const _:()=assert!(core::mem::offset_of!(AcpiDeviceCapabilities,address)==0x08);
const _:()=assert!(core::mem::offset_of!(AcpiDeviceCapabilities,ui_number)==0x0c);
const _:()=assert!(core::mem::offset_of!(AcpiDeviceCapabilities,device_state)==0x10);
const _:()=assert!(core::mem::offset_of!(AcpiDeviceCapabilities,system_wake)==0x2c);
const _:()=assert!(core::mem::offset_of!(AcpiDeviceCapabilities,d3_latency)==0x3c);

unsafe fn query_pdo_capabilities(device:*mut cognacos_nt_abi::io::DEVICE_OBJECT,stack:*mut cognacos_nt_abi::io::IO_STACK_LOCATION)->i32{
    if device.is_null()||stack.is_null(){return STATUS_INVALID_PARAMETER;}
    let extension=unsafe{(*device).device_extension.cast::<AcpiPdoExtension>()};
    if extension.is_null()||unsafe{(*extension).signature}!=ACPI_PDO_SIGNATURE{return STATUS_INVALID_PARAMETER;}
    let capabilities=unsafe{core::ptr::read_unaligned((*stack).parameters.as_ptr().cast::<*mut AcpiDeviceCapabilities>())};
    if capabilities.is_null(){return STATUS_INVALID_PARAMETER;}
    let size=unsafe{core::ptr::read_unaligned(core::ptr::addr_of!((*capabilities).size))} as usize;
    let version=unsafe{core::ptr::read_unaligned(core::ptr::addr_of!((*capabilities).version))};
    if version!=1||size<DEVICE_CAPABILITIES_SIZE{return STATUS_INVALID_PARAMETER;}
    // Conservative ACPI defaults: the firmware runtime has not yet connected
    // _SxD/_SxW/_PRW, so do not advertise unsupported wake or D1/D2 behavior.
    let mut value=AcpiDeviceCapabilities{
        size:size as u16,
        version,
        flags:0, // UniqueID remains FALSE: the instance ID is parent-bus scoped.
        address:unsafe{(*extension).identity.adr.and_then(|value|u32::try_from(value).ok()).unwrap_or(u32::MAX)},
        ui_number:u32::MAX,
        device_state:[0,1,4,4,4,4,4], // Unspecified; Working=D0; sleep/hibernate/shutdown=D3.
        system_wake:0,
        device_wake:0,
        d1_latency:0,
        d2_latency:0,
        d3_latency:0,
    };
    if unsafe{(*extension).identity.sta}&acpi::acpi_device::DEVICE_UI==0 {
        value.flags|=1<<17; // NoDisplayInUI
    }
    unsafe{core::ptr::copy_nonoverlapping(&value as *const AcpiDeviceCapabilities as *const u8,capabilities as *mut u8,DEVICE_CAPABILITIES_SIZE);}
    STATUS_SUCCESS
}

unsafe fn query_pdo_id(device:*mut cognacos_nt_abi::io::DEVICE_OBJECT,irp:*mut cognacos_nt_abi::io::IRP,stack:*mut cognacos_nt_abi::io::IO_STACK_LOCATION)->Result<usize,i32>{
    if device.is_null()||irp.is_null()||stack.is_null(){return Err(STATUS_INVALID_PARAMETER);}
    let extension=unsafe{(*device).device_extension.cast::<AcpiPdoExtension>()};
    if extension.is_null()||unsafe{(*extension).signature}!=ACPI_PDO_SIGNATURE{return Err(STATUS_INVALID_PARAMETER);}
    let id_type=unsafe{core::ptr::read_unaligned((*stack).parameters.as_ptr().cast::<u32>())};
    let identity=unsafe{&(*extension).identity};
    match id_type {
        BUS_QUERY_DEVICE_ID => {
            if !identity.has_hid{return Err(STATUS_NOT_SUPPORTED);}
            unsafe{allocate_acpi_device_id(&identity.hid)}
        }
        BUS_QUERY_HARDWARE_IDS => {
            if !identity.has_hid{return Err(STATUS_NOT_SUPPORTED);}
            let mut ids=[acpi::acpi_device::AcpiId{bytes:[0;acpi::acpi_device::MAX_ID_STRING],length:0};acpi::acpi_device::MAX_COMPATIBLE_IDS+1];
            ids[0]=identity.hid;
            let count=identity.compatible_id_count as usize;
            if count>acpi::acpi_device::MAX_COMPATIBLE_IDS{return Err(STATUS_INVALID_PARAMETER);}
            ids[1..1+count].copy_from_slice(&identity.compatible_ids[..count]);
            unsafe{allocate_prefixed_id_list(&ids[..count+1])}
        }
        BUS_QUERY_COMPATIBLE_IDS => {
            let count=identity.compatible_id_count as usize;
            if count==0{return Err(STATUS_NOT_SUPPORTED);}
            if count>acpi::acpi_device::MAX_COMPATIBLE_IDS{return Err(STATUS_INVALID_PARAMETER);}
            unsafe{allocate_prefixed_id_list(&identity.compatible_ids[..count])}
        }
        BUS_QUERY_INSTANCE_ID => {
            let uid=identity.uid_string.unwrap_or_else(||decimal_uid(identity.uid.unwrap_or(unsafe{(*extension).node} as u64)));
            unsafe{allocate_id_string(&uid)}
        }
        _=>Err(STATUS_NOT_SUPPORTED),
    }
}


fn read_pdo_resource_template(device:*mut cognacos_nt_abi::io::DEVICE_OBJECT,name:[u8;4])->Result<acpi::acpi_resources::ResourceList,i32>{
 if device.is_null(){return Err(STATUS_INVALID_PARAMETER)}
 let ext=unsafe{(*device).device_extension.cast::<AcpiPdoExtension>()};
 if ext.is_null()||unsafe{(*ext).signature}!=ACPI_PDO_SIGNATURE{return Err(STATUS_INVALID_PARAMETER)}
 let ext=unsafe{&*ext};let handle=ext.table.ok_or(STATUS_INVALID_PARAMETER)?;
 if ext.aml_bytes.is_null()||ext.aml_length<36{return Err(STATUS_INVALID_PARAMETER)}
 let aml=unsafe{core::slice::from_raw_parts(ext.aml_bytes,ext.aml_length)};
 let manager=unsafe{(*STATE.aml_manager.get()).assume_init_ref()};let gate=unsafe{&*STATE.aml_barrier.get()};
 let resource_node=manager.namespace().find_child(ext.node,name).ok_or(STATUS_NOT_SUPPORTED)?;
 let scratch=unsafe{ExAllocatePoolWithTag(PAGED_POOL,65536,ACPI_POOL_TAG)};
 if scratch.is_null(){return Err(STATUS_INSUFFICIENT_RESOURCES)}
 let length=match manager.read_named_buffer(handle,aml,resource_node,gate,unsafe{core::slice::from_raw_parts_mut(scratch,65536)}){
  Ok(n)=>n,Err(_)=>{unsafe{ExFreePoolWithTag(scratch,ACPI_POOL_TAG)};return Err(STATUS_NOT_SUPPORTED)}
 };
 let parsed=acpi::acpi_resources::parse_resource_template(unsafe{core::slice::from_raw_parts(scratch,length)});
 unsafe{ExFreePoolWithTag(scratch,ACPI_POOL_TAG)};
 parsed.map_err(|_|STATUS_NOT_SUPPORTED)
}
unsafe fn query_pdo_resources(device:*mut cognacos_nt_abi::io::DEVICE_OBJECT)->Result<usize,i32>{
 let resources=read_pdo_resource_template(device,*b"_CRS")?;
 let size=acpi::acpi_wdm_resources::required_size(&resources).map_err(|_|STATUS_INSUFFICIENT_RESOURCES)?;
 if acpi::acpi_wdm_resources::descriptor_count(&resources).map_err(|_|STATUS_NOT_SUPPORTED)?==0{return Err(STATUS_NOT_SUPPORTED)}
 let output=unsafe{ExAllocatePoolWithTag(PAGED_POOL,size,ACPI_POOL_TAG)};
 if output.is_null(){return Err(STATUS_INSUFFICIENT_RESOURCES)}
 match unsafe{acpi::acpi_wdm_resources::serialize(&resources,output,size)}{
  Ok(_)=>Ok(output as usize),Err(_)=>{unsafe{ExFreePoolWithTag(output,ACPI_POOL_TAG)};Err(STATUS_INVALID_PARAMETER)}
 }
}
fn read_pdo_resource_alternatives(device:*mut cognacos_nt_abi::io::DEVICE_OBJECT)->Result<acpi::acpi_resources::ResourceAlternatives,i32>{
 if device.is_null(){return Err(STATUS_INVALID_PARAMETER)}
 let ext=unsafe{(*device).device_extension.cast::<AcpiPdoExtension>()};
 if ext.is_null()||unsafe{(*ext).signature}!=ACPI_PDO_SIGNATURE{return Err(STATUS_INVALID_PARAMETER)}
 let ext=unsafe{&*ext};let handle=ext.table.ok_or(STATUS_INVALID_PARAMETER)?;
 if ext.aml_bytes.is_null()||ext.aml_length<36{return Err(STATUS_INVALID_PARAMETER)}
 let aml=unsafe{core::slice::from_raw_parts(ext.aml_bytes,ext.aml_length)};
 let manager=unsafe{(*STATE.aml_manager.get()).assume_init_ref()};let gate=unsafe{&*STATE.aml_barrier.get()};
 let resource_node=manager.namespace().find_child(ext.node,*b"_PRS").ok_or(STATUS_NOT_SUPPORTED)?;
 let scratch=unsafe{ExAllocatePoolWithTag(PAGED_POOL,65536,ACPI_POOL_TAG)};
 if scratch.is_null(){return Err(STATUS_INSUFFICIENT_RESOURCES)}
 let length=match manager.read_named_buffer(handle,aml,resource_node,gate,unsafe{core::slice::from_raw_parts_mut(scratch,65536)}){
  Ok(n)=>n,Err(_)=>{unsafe{ExFreePoolWithTag(scratch,ACPI_POOL_TAG)};return Err(STATUS_NOT_SUPPORTED)}
 };
 let parsed=acpi::acpi_resources::parse_resource_requirements_template(unsafe{core::slice::from_raw_parts(scratch,length)});
 unsafe{ExFreePoolWithTag(scratch,ACPI_POOL_TAG)};
 parsed.map_err(|_|STATUS_NOT_SUPPORTED)
}
unsafe fn query_pdo_resource_requirements(device:*mut cognacos_nt_abi::io::DEVICE_OBJECT)->Result<usize,i32>{
 let resources=read_pdo_resource_alternatives(device)?;
 let size=acpi::acpi_wdm_requirements::required_size_alternatives(&resources).map_err(|_|STATUS_INSUFFICIENT_RESOURCES)?;
 let output=unsafe{ExAllocatePoolWithTag(PAGED_POOL,size,ACPI_POOL_TAG)};
 if output.is_null(){return Err(STATUS_INSUFFICIENT_RESOURCES)}
 match unsafe{acpi::acpi_wdm_requirements::serialize_alternatives(&resources,output,size)}{
  Ok(_)=>Ok(output as usize),Err(_)=>{unsafe{ExFreePoolWithTag(output,ACPI_POOL_TAG)};Err(STATUS_INVALID_PARAMETER)}
 }
}

unsafe extern "win64" fn pnp_dispatch(device: *mut cognacos_nt_abi::io::DEVICE_OBJECT, irp: *mut cognacos_nt_abi::io::IRP) -> i32 {
    let Some(_operation)=AcpiOperationGuard::enter() else{return STATUS_DEVICE_NOT_READY;};
    if irp.is_null() { return STATUS_INVALID_PARAMETER; }
    let stack = unsafe { (*irp).tail[8] as *mut cognacos_nt_abi::io::IO_STACK_LOCATION };
    if stack.is_null() { return STATUS_INVALID_PARAMETER; }
    // IRP_MN_QUERY_DEVICE_RELATIONS returns a caller-consumed
    // DEVICE_RELATIONS pointer in IoStatus.Information. Keep it local until
    // the common completion write below; writing Information in the match
    // arm and then clearing it at the end silently publishes zero children.
    let mut information = 0usize;
    let status = match unsafe { (*stack).minor_function } {
        // The ACPI bus FDO may start once its firmware namespace is ready.
        // A child PDO must not be reported started until its selected _PRS
        // configuration has been programmed through _SRS and the resulting
        // _CRS has been verified. The typed-buffer AML path is not ready yet.
        IRP_MN_START_DEVICE if device==unsafe{ACPI_BUS_DEVICE}
            && STATE.ready.load(Ordering::Acquire) => STATUS_SUCCESS,
        // Fail closed rather than falsely publishing an unprogrammed PDO.
        IRP_MN_START_DEVICE if device!=unsafe{ACPI_BUS_DEVICE} => STATUS_NOT_SUPPORTED,
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
                    information=relations as usize;
                }
                PDO_RELATIONS_LOCK.store(false,Ordering::Release);
                STATUS_SUCCESS
            }
        },
        IRP_MN_QUERY_CAPABILITIES if device!=unsafe{ACPI_BUS_DEVICE} => {
            unsafe{query_pdo_capabilities(device,stack)}
        },
        IRP_MN_QUERY_RESOURCES if device!=unsafe{ACPI_BUS_DEVICE} => {
            match unsafe{query_pdo_resources(device)} { Ok(pointer)=>{information=pointer;STATUS_SUCCESS}, Err(status)=>status }
        },
        IRP_MN_QUERY_RESOURCE_REQUIREMENTS if device!=unsafe{ACPI_BUS_DEVICE} => {
            match unsafe{query_pdo_resource_requirements(device)} { Ok(pointer)=>{information=pointer;STATUS_SUCCESS}, Err(status)=>status }
        },
        IRP_MN_QUERY_ID if device!=unsafe{ACPI_BUS_DEVICE} => {
            match unsafe{query_pdo_id(device,irp,stack)} {
                Ok(pointer)=>{information=pointer;STATUS_SUCCESS}
                Err(status)=>status,
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
    unsafe {
        (*irp).io_status.status.status = status;
        (*irp).io_status.information = if status == STATUS_SUCCESS { information } else { 0 };
    }
    status
}
unsafe extern "win64" fn driver_unload(_driver: *mut DRIVER_OBJECT) {
    // Stop new ACPI PnP callbacks before waiting for callbacks already inside
    // AML evaluation. The kernel's mutant records are retired on final release;
    // draining here guarantees no owner/waiter still references driver storage.
    STATE.unloading.store(true, Ordering::Release);
    STATE.ready.store(false, Ordering::Release);
    while STATE.active_operations.load(Ordering::Acquire)!=0
        || STATE.interface_references.load(Ordering::Acquire)!=0 {
        core::hint::spin_loop();
    }
}

fn collect_device_identities(
    manager:&acpi::aml_tables::AmlTableManager,
    handle:acpi::aml_tables::TableHandle,
    table_sources:&[&[u8]],
    source_bytes:&'static [u8],
    gate:&acpi::aml_tables::AmlExecutionBarrier,
    records:&mut [AcpiDeviceRecord;MAX_ACPI_DEVICES],
    count:&mut usize,
)->Result<(),i32>{
    let info=manager.table_info(handle).map_err(|_|STATUS_INVALID_PARAMETER)?;
    let namespace=manager.namespace();
    for index in info.namespace_start..info.namespace_end{
        let node=&namespace.nodes[index as usize];
        if !matches!(node.object_type,acpi::aml::AmlObjectType::Device|acpi::aml::AmlObjectType::Processor|acpi::aml::AmlObjectType::PowerResource|acpi::aml::AmlObjectType::ThermalZone){continue}
        let mut methods=acpi::acpi_device::SynchronizedMethodInvoker{synchronizer:&KERNEL_AML_METHOD_SYNCHRONIZER};
        match manager.evaluate_device_identity(handle,table_sources,index,gate,&mut methods){
            Ok(identity)=>{
                if *count>=records.len(){return Err(STATUS_INSUFFICIENT_RESOURCES)}
                records[*count]=AcpiDeviceRecord{table:Some(handle),node:index,aml_bytes:source_bytes,identity};
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

/// DriverEntry validates firmware, loads DSDT/SSDT namespace definitions,
/// builds the ACPI device catalogue, and publishes bus-enumerated PDOs. PDOs
/// answer the WDM QueryId contract using PnP-owned paged-pool buffers.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn DriverEntry(driver: PVOID, _registry_path: PVOID) -> i32 {
    if driver.is_null() { return STATUS_INVALID_PARAMETER; }
    STATE.ready.store(false,Ordering::Release);
    STATE.unloading.store(false,Ordering::Release);
    STATE.active_operations.store(0,Ordering::Release);
    STATE.interface_references.store(0,Ordering::Release);
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
    if let Err(status)=collect_device_identities(manager,loaded.dsdt,&table_sources[..ssdt_count+1],dsdt,gate,records,&mut identity_count){return status;}
    for index in 0..loaded.ssdt_count{
        let handle=match loaded.ssdts[index]{Some(handle)=>handle,None=>return STATUS_INVALID_PARAMETER};
        if let Err(status)=collect_device_identities(manager,handle,&table_sources[..ssdt_count+1],ssdt_tables[index],gate,records,&mut identity_count){return status;}
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
            extension.write(AcpiPdoExtension{signature:ACPI_PDO_SIGNATURE,node:record.node,devnode_index:u32::MAX,table:record.table,aml_bytes:record.aml_bytes.as_ptr(),aml_length:record.aml_bytes.len(),identity:record.identity});
        }
        let marked=unsafe{IoMarkBusEnumeratedPdo(pdo)};
        if marked!=STATUS_SUCCESS { return marked; }
        // Publish the ACPI namespace identity into the kernel's generic PnP
        // devnode table. The PDO relation database remains the WDM authority;
        // this registration supplies the kernel arbiter with a stable owner.
        let mut devnode_index=0u32;
        let registered=unsafe{IoPnPRegisterBusDevice(
            u32::from_le_bytes(*b"ACPI"),
            (record.node as u64)+1,
            pdo,
            &mut devnode_index,
        )};
        if registered!=STATUS_SUCCESS{return registered;}
        unsafe { (*(*pdo).device_extension.cast::<AcpiPdoExtension>()).devnode_index=devnode_index; }
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

/// A referenced firmware interface pins ACPI.sys until the consumer calls
/// InterfaceDereference. The reference callback itself is guarded as an active
/// operation so unload cannot race the count increment.
unsafe extern "win64" fn acpi_interface_reference(context:PVOID){
    if context!=core::ptr::addr_of!(STATE).cast_mut().cast(){return}
    let Some(_operation)=AcpiOperationGuard::enter() else{return};
    STATE.interface_references.fetch_add(1,Ordering::AcqRel);
}
unsafe extern "win64" fn acpi_interface_dereference(context:PVOID){
    if context!=core::ptr::addr_of!(STATE).cast_mut().cast(){return}
    // A held interface reference keeps the image resident while this callback
    // runs. Count the callback itself until its final instruction as well.
    STATE.active_operations.fetch_add(1,Ordering::AcqRel);
    let _=STATE.interface_references.fetch_update(Ordering::AcqRel,Ordering::Acquire,|count|Some(count.saturating_sub(1)));
    STATE.active_operations.fetch_sub(1,Ordering::AcqRel);
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
    let Some(_operation)=AcpiOperationGuard::enter() else{return STATUS_DEVICE_NOT_READY;};
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
                interface_reference: Some(acpi_interface_reference),
                interface_dereference: Some(acpi_interface_dereference),
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
