#![no_std]
#![no_main]

// Canonical Windows NT-style ABI types are shared with NTDLL and driver crates.
pub(crate) use cognacos_nt_abi as nt_abi;

pub(crate) use ntoskrnl::mm::{kernel_space as kernel_vm, pagefile, physical as memory};
pub(crate) use ntoskrnl::se::runtime as security;
mod user;
mod pe;
mod process;
mod syscall;
mod winabi;
mod pe_runtime;
mod pe_modules;
mod pe_loader;
mod pe_dependencies;
mod usermode;
use cognacos_acpi as acpi;
use cognacos_pci as pci;
mod dma;
mod hal;
mod smp;
mod platform_drivers;
mod boot_orchestration;
static BOOT_COORDINATOR: boot_orchestration::BootCoordinator = boot_orchestration::BootCoordinator::new();
pub(crate) use ntoskrnl::ke::{dispatcher, scheduler, thread};
pub(crate) use ntoskrnl::ob::object_table as object;
#[path = "ntoskrnl/io/wdm.rs"]
pub(crate) mod wdm;
/// Compatibility namespace for existing kernel call sites. The implementation
/// is the single WDM manager; no parallel legacy I/O runtime is retained.
pub(crate) mod io {
    pub use crate::wdm::*;
    pub use crate::wdm::Irp as WdmIrp;
    pub mod wdm {
        pub use crate::wdm::*;
    }
}
#[path = "ntoskrnl/io/file_services.rs"]
pub(crate) mod file_services;
#[path = "ntoskrnl/io/ata_disk.rs"]
mod ata_disk;
use cognacos_ext4 as extfs;
pub(crate) use ntoskrnl::rtl_runtime::nls;
mod ntoskrnl;
#[path = "ntoskrnl/debug_console.rs"]
mod debug_console;

use core::arch::asm;
use core::panic::PanicInfo;

/// PCI.sys is the authoritative PCI bus enumerator. It is staged by the
/// driver build and embedded separately from ntoskrnl for registry-selected
/// initialization through the native PE driver loader.
const ACPI_PE_IMAGE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/acpi.sys"));
const PCI_PE_IMAGE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/pci.sys"));
const USBXHCI_PE_IMAGE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/usbxhci.sys"));
const UCX01020_PE_IMAGE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ucx01020.sys"));
const USBHUB3_PE_IMAGE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/usbhub3.sys"));

use cognacos_kbdclass::{KeyboardInputData,KeyboardInputStack,KEY_BREAK};
use cognacos_i8042prt::{Ps2KeyboardPort,PortIo,PowerAwarePortIo,PortError,controller_command,wait_output_full,wait_input_empty,CMD_READ_CONFIG,CMD_WRITE_CONFIG,CONFIG_KEYBOARD_IRQ,STATUS_OUTPUT_FULL,STATUS_PORT,DATA_PORT};
use kbdus::{Modifiers,translate_packet,VK_SHIFT,VK_CAPITAL,VK_CONTROL,VK_MENU};

static mut KBD_STACK:KeyboardInputStack=KeyboardInputStack::new();
static mut KBD_PORT:Ps2KeyboardPort=Ps2KeyboardPort::new();
static mut KBD_MODIFIERS:Modifiers=Modifiers{shift:false,caps_lock:false,control:false,alt:false};
static KBD_STACK_READY:core::sync::atomic::AtomicBool=core::sync::atomic::AtomicBool::new(false);
static KBD_INPUT_COUNT:core::sync::atomic::AtomicU64=core::sync::atomic::AtomicU64::new(0);
static PS2_IRQ_ACTIVE:core::sync::atomic::AtomicBool=core::sync::atomic::AtomicBool::new(false);
static PS2_IRQ_ROUTE_READY:core::sync::atomic::AtomicBool=core::sync::atomic::AtomicBool::new(false);
static PS2_IRQ_VECTOR:core::sync::atomic::AtomicU8=core::sync::atomic::AtomicU8::new(0);
static PS2_IRQ_COUNT:core::sync::atomic::AtomicU64=core::sync::atomic::AtomicU64::new(0);
#[cfg(debug_assertions)]
static PS2_IRQ_DIAGNOSTIC_COUNT:core::sync::atomic::AtomicU64=core::sync::atomic::AtomicU64::new(0);
#[cfg(debug_assertions)]
static KBD_INPUT_DIAGNOSTIC_COUNT:core::sync::atomic::AtomicU64=core::sync::atomic::AtomicU64::new(0);
static PS2_IRQ_IO_BASE:core::sync::atomic::AtomicU64=core::sync::atomic::AtomicU64::new(0);
static PS2_IRQ_GSI_BASE:core::sync::atomic::AtomicU32=core::sync::atomic::AtomicU32::new(0);
static PS2_IRQ_GSI:core::sync::atomic::AtomicU32=core::sync::atomic::AtomicU32::new(0);

#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub struct KeyboardEvent { pub virtual_key:u16,pub scan_code:u16,pub unicode:u16,pub key_up:bool,pub extended:bool }
const KBD_EVENT_CAPACITY:usize=128;
static mut KBD_EVENTS:[KeyboardEvent;KBD_EVENT_CAPACITY]=[KeyboardEvent{virtual_key:0,scan_code:0,unicode:0,key_up:false,extended:false};KBD_EVENT_CAPACITY];
static KBD_EVENT_HEAD:core::sync::atomic::AtomicUsize=core::sync::atomic::AtomicUsize::new(0);
static KBD_EVENT_TAIL:core::sync::atomic::AtomicUsize=core::sync::atomic::AtomicUsize::new(0);
static KBD_EVENT_DROPS:core::sync::atomic::AtomicU64=core::sync::atomic::AtomicU64::new(0);
fn publish_keyboard_event(event:KeyboardEvent){
 use core::sync::atomic::Ordering;
 let head=KBD_EVENT_HEAD.load(Ordering::Relaxed);
 let next=(head+1)%KBD_EVENT_CAPACITY;
 if next==KBD_EVENT_TAIL.load(Ordering::Acquire){KBD_EVENT_DROPS.fetch_add(1,Ordering::Relaxed);return}
 unsafe{core::ptr::write(core::ptr::addr_of_mut!(KBD_EVENTS).cast::<KeyboardEvent>().add(head),event);}
 KBD_EVENT_HEAD.store(next,Ordering::Release);
}
/// Dequeue one translated key event for the future Win32 input-queue adapter.
/// This is a single-consumer kernel API, not yet a user-mode NtUserGetMessage syscall.
pub fn dequeue_keyboard_event()->Option<KeyboardEvent>{
 use core::sync::atomic::Ordering;
 let tail=KBD_EVENT_TAIL.load(Ordering::Relaxed);
 if tail==KBD_EVENT_HEAD.load(Ordering::Acquire){return None}
 let event=unsafe{core::ptr::read(core::ptr::addr_of!(KBD_EVENTS).cast::<KeyboardEvent>().add(tail))};
 KBD_EVENT_TAIL.store((tail+1)%KBD_EVENT_CAPACITY,Ordering::Release);
 Some(event)
}
pub fn keyboard_event_drop_count()->u64{KBD_EVENT_DROPS.load(core::sync::atomic::Ordering::Relaxed)}



struct KernelPs2PortIo;
impl PortIo for KernelPs2PortIo {
 fn read8(&mut self,port:u16)->u8{let v:u8;unsafe{asm!("in al, dx",in("dx") port,out("al") v,options(nomem,nostack,preserves_flags));}v}
 fn write8(&mut self,port:u16,value:u8){unsafe{asm!("out dx, al",in("dx") port,in("al") value,options(nomem,nostack,preserves_flags));}}
}
impl PowerAwarePortIo for KernelPs2PortIo {
 fn set_keyboard_irq_enabled(&mut self,enabled:bool)->Result<(),PortError>{
  controller_command(self,CMD_READ_CONFIG,100_000)?;
  let mut config=wait_output_full(self,100_000)?;
  if enabled{config|=CONFIG_KEYBOARD_IRQ}else{config&=!CONFIG_KEYBOARD_IRQ}
  controller_command(self,CMD_WRITE_CONFIG,100_000)?;
  wait_input_empty(self,100_000)?;
  self.write8(DATA_PORT,config);Ok(())
 }
 fn synchronize_keyboard_irq(&mut self)->Result<(),PortError>{
  // CognacOS currently keeps legacy IRQ routing masked and polls the output buffer.
  // This is safe until the IDT thunk and interrupt rundown path are installed.
  Ok(())
 }
}

/// Prepare IRQ1 from the firmware MADT. The I/O-APIC route stays masked until
/// the 8042 and keyboard stack are initialized and the ISR is fully registered.
fn prepare_ps2_irq_route(madt:&acpi::MadtInfo,bsp_apic_id:u32)->bool {
 use core::sync::atomic::Ordering;
 if PS2_IRQ_ROUTE_READY.load(Ordering::Acquire) { return true; }
 let mut gsi=1u32;
 let mut active_low=false;
 let mut level_triggered=false;
 let mut found_override=false;
 for ov in madt.irq_overrides.iter().take(madt.irq_override_count) {
  if ov.bus==0 && ov.source_irq==1 {
   if found_override { serial_write(b"PS/2 IRQ: duplicate MADT override; refusing to enable.\\r\\n"); return false; }
   gsi=ov.gsi; active_low=ov.active_low; level_triggered=ov.level_triggered; found_override=true;
  }
 }
 let io=match madt.io_apics.iter().take(madt.io_apic_count).find(|io| gsi>=io.gsi_base && gsi.saturating_sub(io.gsi_base)<120) {
  Some(io)=>io, None=>{serial_write(b"PS/2 IRQ: no I/O APIC owns IRQ1 GSI; polling retained.\\r\\n");return false}
 };
 let vector=match hal::allocate_device_interrupt_vector() {
  Ok(v)=>v,Err(_)=>{serial_write(b"PS/2 IRQ: vector allocation failed.\\r\\n");return false}
 };
 if !ntoskrnl::ke::arch::device_interrupt_vector_ready(vector) {
  let _=hal::release_device_interrupt_vector(vector);
  serial_write(b"PS/2 IRQ: IDT gate validation failed; route remains masked.\\r\\n");return false
 }
 if hal::register_device_interrupt_handler(vector,ps2_keyboard_interrupt,core::ptr::null_mut()).is_err() {
  let _=hal::release_device_interrupt_vector(vector);
  serial_write(b"PS/2 IRQ: ISR registration failed; route remains masked.\\r\\n");return false
 }
 if unsafe {hal::configure_device_interrupt_route_masked(io.address as u64,io.gsi_base,gsi,vector,bsp_apic_id as u8,active_low,level_triggered)}.is_err() {
  let _=hal::release_device_interrupt_vector(vector);
  serial_write(b"PS/2 IRQ: I/O APIC route setup failed; route remains masked.\\r\\n");return false
 }
 PS2_IRQ_VECTOR.store(vector,Ordering::Release);
 PS2_IRQ_IO_BASE.store(io.address as u64,Ordering::Release);
 PS2_IRQ_GSI_BASE.store(io.gsi_base,Ordering::Release);
 PS2_IRQ_GSI.store(gsi,Ordering::Release);
 PS2_IRQ_ROUTE_READY.store(true,Ordering::Release);
 serial_write(b"PS/2 IRQ: MADT IRQ1 resolved to GSI=");serial_write_decimal(gsi as usize);
 serial_write(b" vector=");serial_write_hex(vector as u64);
 serial_write(b"; route configured masked.\\r\\n");
 true
}

/// Bounded IRQ-context drain. The generic IDT stub dispatches through HAL, which
/// performs exactly one local-APIC EOI after this callback returns.
unsafe extern "win64" fn ps2_keyboard_interrupt(_vector:u8,_frame:*mut core::ffi::c_void,_context:*mut core::ffi::c_void)->bool {
 use core::sync::atomic::Ordering;
 if !KBD_STACK_READY.load(Ordering::Acquire) { return false; }
 PS2_IRQ_COUNT.fetch_add(1,Ordering::Relaxed);
 let mut handled=false;
 for _ in 0..32 {
  let status:u8;unsafe{asm!("in al, dx",in("dx") STATUS_PORT,out("al") status,options(nomem,nostack,preserves_flags));}
  if status&STATUS_OUTPUT_FULL==0 { break; }
  let byte:u8;unsafe{asm!("in al, dx",in("dx") DATA_PORT,out("al") byte,options(nomem,nostack,preserves_flags));}
  handled=true;
  if status&0x20!=0 { continue; } // auxiliary/mouse byte; not a keyboard scan code
  unsafe {
   let packet=(&mut *core::ptr::addr_of_mut!(KBD_PORT)).feed_scancode(byte);
   let Some(packet)=packet else{continue};
   let stack=&mut *core::ptr::addr_of_mut!(KBD_STACK);
   if stack.submit_from_port(packet).is_err(){continue}
   let mut out=[KeyboardInputData::default();1];
   if stack.read_packets(&mut out).ok()!=Some(1){continue}
   let p=out[0];
   let key=translate_packet(p,*core::ptr::addr_of!(KBD_MODIFIERS));
   let mods=&mut *core::ptr::addr_of_mut!(KBD_MODIFIERS);
   let down=p.flags&KEY_BREAK==0;
   match key.map(|e|e.virtual_key).unwrap_or(0) {
    VK_SHIFT=>mods.shift=down, VK_CONTROL=>mods.control=down, VK_MENU=>mods.alt=down,
    VK_CAPITAL if down=>mods.caps_lock=!mods.caps_lock, _=>{}
   }
   if let Some(e)=key {
    publish_keyboard_event(KeyboardEvent{virtual_key:e.virtual_key,scan_code:e.scan_code,unicode:e.unicode,key_up:e.key_up,extended:e.extended});
    KBD_INPUT_COUNT.fetch_add(1,Ordering::Relaxed);
   }
  }
 }
 handled
}

fn enable_ps2_irq_route()->bool {
 use core::sync::atomic::Ordering;
 if !PS2_IRQ_ROUTE_READY.load(Ordering::Acquire) { return false; }
 if PS2_IRQ_ACTIVE.load(Ordering::Acquire) { return true; }
 let io=PS2_IRQ_IO_BASE.load(Ordering::Acquire);
 let gsi_base=PS2_IRQ_GSI_BASE.load(Ordering::Acquire);
 let gsi=PS2_IRQ_GSI.load(Ordering::Acquire);
 PS2_IRQ_ACTIVE.store(true,Ordering::Release);
 if unsafe{hal::set_device_interrupt_route_enabled(io,gsi_base,gsi,true)}.is_err() {
  PS2_IRQ_ACTIVE.store(false,Ordering::Release);
  serial_write(b"PS/2 IRQ: route enable failed; keeping polling fallback.\\r\\n");return false
 }
 serial_write(b"PS/2 IRQ: route enabled after controller initialization; bounded ISR active.\\r\\n");
 true
}

/// Poll the legacy 8042 output buffer while the legacy IRQ route is still masked.
/// This provides a safe bring-up path while IDT/IRQ registration is unfinished.
fn poll_ps2_keyboard_input(){
 use core::sync::atomic::Ordering;
 if PS2_IRQ_ACTIVE.load(Ordering::Acquire) {
  #[cfg(debug_assertions)]
  {
   let irq_count=PS2_IRQ_COUNT.load(Ordering::Acquire);
   if irq_count!=0 && PS2_IRQ_DIAGNOSTIC_COUNT.compare_exchange(0,irq_count,Ordering::AcqRel,Ordering::Relaxed).is_ok() {
    serial_write(b"PS/2 IRQ delivered: count=");serial_write_decimal(irq_count as usize);
    serial_write(b"; keyboard bytes drained by ISR; APIC EOI handled by HAL dispatcher.\\r\\n");
   }
   let input_count=KBD_INPUT_COUNT.load(Ordering::Acquire);
   if input_count!=0 && KBD_INPUT_DIAGNOSTIC_COUNT.compare_exchange(0,input_count,Ordering::AcqRel,Ordering::Relaxed).is_ok() {
    serial_write(b"QEMU KEYBOARD INPUT: source=IRQ1/KbdClass count=");serial_write_hex(input_count);
    serial_write(b"\\r\\n");
   }
  }
  return;
 }
 if !KBD_STACK_READY.load(Ordering::Acquire) {
  unsafe {
   let stack=&mut *core::ptr::addr_of_mut!(KBD_STACK);
   if stack.attach_port().is_err()||stack.open().is_err(){return}
   let mut io=KernelPs2PortIo;
   if (&mut *core::ptr::addr_of_mut!(KBD_PORT)).start(&mut io,100_000).is_err(){
    serial_write(b"PS/2 keyboard controller initialization failed; polling input disabled.\r\n");return
   }
  }
  KBD_STACK_READY.store(true,Ordering::Release);
  let _ = platform_drivers::mark_ready(platform_drivers::Stage::KeyboardStack);
  unsafe{serial_write(b"KEYBOARD STACK: PS/2 controller + KbdClass queue initialized; preparing real IRQ1 route.\\r\\n");
  if PS2_IRQ_ROUTE_READY.load(Ordering::Acquire) { let _=enable_ps2_irq_route(); }}
 }
 for _ in 0..16 {
  let status:u8;unsafe{asm!("in al, dx",in("dx") STATUS_PORT,out("al") status,options(nomem,nostack,preserves_flags));}
  if status&STATUS_OUTPUT_FULL==0{break}
  let byte:u8;unsafe{asm!("in al, dx",in("dx") DATA_PORT,out("al") byte,options(nomem,nostack,preserves_flags));}
  if status&0x20!=0{continue}
  let packet=unsafe{(&mut *core::ptr::addr_of_mut!(KBD_PORT)).feed_scancode(byte)};
  let Some(packet)=packet else{continue};
  unsafe{
   let stack=&mut *core::ptr::addr_of_mut!(KBD_STACK);
   if stack.submit_from_port(packet).is_err(){continue}
   let mut out=[KeyboardInputData::default();1];
   if stack.read_packets(&mut out).ok()!=Some(1){continue}
   let p=out[0];
   let key=translate_packet(p,*core::ptr::addr_of!(KBD_MODIFIERS));
   let mods=&mut *core::ptr::addr_of_mut!(KBD_MODIFIERS);
   let down=p.flags&KEY_BREAK==0;
   match key.map(|e|e.virtual_key).unwrap_or(0){
    VK_SHIFT=>mods.shift=down,
    VK_CONTROL=>mods.control=down,
    VK_MENU=>mods.alt=down,
    VK_CAPITAL if down=>mods.caps_lock=!mods.caps_lock,
    _=>{}
   }
   if let Some(e)=key{
    let event=KeyboardEvent{virtual_key:e.virtual_key,scan_code:e.scan_code,unicode:e.unicode,key_up:e.key_up,extended:e.extended};
    publish_keyboard_event(event);
    let count=KBD_INPUT_COUNT.fetch_add(1,Ordering::Relaxed)+1;
    #[cfg(debug_assertions)]
    {
     serial_write(b"QEMU KEYBOARD INPUT: count=");serial_write_hex(count);
     serial_write(b" VK=");serial_write_hex(e.virtual_key as u64);
     serial_write(b" scan=");serial_write_hex(e.scan_code as u64);
     serial_write(b" up=");serial_write_hex(e.key_up as u64);
     if e.unicode!=0{serial_write(b" UTF16=");serial_write_hex(e.unicode as u64);}
     serial_write(b"\\r\\n");
    }
   }
  }
 }
}



// PCI configuration cycles are routed through HAL's validated MCFG/ECAM
// windows. NTOSKRNL does not use legacy CF8/CFC port cycles.
unsafe fn pci_config_read32(bus:u8,device:u8,function:u8,offset:u8)->u32 {
    unsafe { hal::pci_config_read(0,bus,device,function,offset as u16,4) }.unwrap_or(u32::MAX)
}
unsafe fn pci_config_write16(bus:u8,device:u8,function:u8,offset:u8,value:u16) {
    let _=unsafe { hal::pci_config_write(0,bus,device,function,offset as u16,2,value as u32) };
}


unsafe extern "C" fn vtd_alloc_table(_ctx:*mut core::ffi::c_void,pa_out:*mut u64,va_out:*mut *mut u64)->i32{
 if pa_out.is_null()||va_out.is_null(){return dma::iommu::vtd::STATUS_INVALID_PARAMETER}
 let pa=match unsafe{memory::allocate_contiguous_pages(1)}{Ok(v)=>v,Err(_)=>return dma::iommu::vtd::STATUS_INSUFFICIENT_RESOURCES};
 let Some(va)=memory::physical_access_pointer(pa)else{let _=unsafe{memory::free_page(pa)};return dma::iommu::vtd::STATUS_INSUFFICIENT_RESOURCES};
 unsafe{pa_out.write(pa);va_out.write(va.cast::<u64>());}
 dma::iommu::vtd::STATUS_SUCCESS
}
unsafe extern "C" fn vtd_free_table(_ctx:*mut core::ffi::c_void,pa:u64,_va:*mut u64)->i32{
 if pa==0||pa&0xfff!=0{return dma::iommu::vtd::STATUS_INVALID_PARAMETER}
 match unsafe{memory::free_page(pa)}{Ok(())=>dma::iommu::vtd::STATUS_SUCCESS,Err(_)=>dma::iommu::vtd::STATUS_INSUFFICIENT_RESOURCES}
}
unsafe extern "C" fn vtd_map_mmio(_ctx:*mut core::ffi::c_void,pa:u64,length:u64,out:*mut u64)->i32{
 if out.is_null(){return dma::iommu::vtd::STATUS_INVALID_PARAMETER}
 match kernel_vm::map_io_space(pa,length){Ok(va)=>{unsafe{out.write(va)};dma::iommu::vtd::STATUS_SUCCESS},Err(_)=>dma::iommu::vtd::STATUS_NOT_SUPPORTED}
}
unsafe extern "C" fn vtd_unmap_mmio(_ctx:*mut core::ffi::c_void,va:u64,length:u64)->i32{
 match kernel_vm::unmap_io_space(va,length){Ok(())=>dma::iommu::vtd::STATUS_SUCCESS,Err(_)=>dma::iommu::vtd::STATUS_NOT_SUPPORTED}
}
unsafe extern "C" fn vtd_clean_cache(_ctx:*mut core::ffi::c_void,_pa:u64,_length:u64)->i32{
 unsafe{core::arch::asm!("mfence",options(nostack,preserves_flags));}
 dma::iommu::vtd::STATUS_SUCCESS
}
/// Enable the firmware-described segment-0 VT-d unit before xHCI discovery.
/// If unavailable, DMA stays fail-closed and PCI bus mastering remains disabled.
extern "C" fn xhci_set_bus_master(device_object:u64,enabled:bool)->bool{
    let Some(r)=ntoskrnl::pnp::xhci_resources() else{return false};
    if device_object==0||device_object!=r.device_object||r.segment!=0{return false}
    if enabled{
        let owner_key=((r.segment as u32)<<16)|((r.bus as u32)<<8)
            |((r.device as u32)<<3)|r.function as u32;
        if VT_D_REQUESTER_OWNER.load(core::sync::atomic::Ordering::Acquire)!=owner_key{return false}
        if r.state<ntoskrnl::pnp::STATE_DMA_READY||r.interrupt_vector==0
            ||!hal::device_interrupt_handler_registered(r.interrupt_vector)
            ||!dma::iommu::available()
            ||!dma::iommu::vtd::is_enabled()
            ||dma::iommu::common_allocation_count(device_object as usize)<4
            ||!dma::iommu::device_busy(device_object as usize){return false}
        let Some(id)=dma::iommu::vtd::RequesterId::new_segment(0,r.bus,r.device,r.function) else{return false};
        if !dma::iommu::vtd::requester_domain_usable(id){return false}
    }
    let Ok(command_value)= (unsafe{hal::pci_config_read(r.segment,r.bus,r.device,r.function,0x04,2)}) else{return false};
    let command=command_value as u16;
    let next=if enabled{command|0x4}else{command&!0x4};
    if unsafe{hal::pci_config_write(r.segment,r.bus,r.device,r.function,0x04,2,next as u32)}.is_err(){return false}
    let Ok(verify_value)= (unsafe{hal::pci_config_read(r.segment,r.bus,r.device,r.function,0x04,2)}) else{return false};
    let verify=verify_value as u16;
    let ok=(verify&0x4!=0)==enabled;
    if ok {
        if enabled { unsafe{serial_write(b"xHCI PCI bus mastering enabled after VT-d domain, DMA mappings, and IRQ registration.\r\n");} }
        else { unsafe{serial_write(b"xHCI PCI bus mastering disabled.\r\n");} }
    }
    ok
}

unsafe extern "C" fn vtd_quarantine_requester(
 _context:*mut core::ffi::c_void,segment:u16,bus:u8,device:u8,function:u8
)->i32{
 if segment!=0||device>=32||function>=8{return -1}
 let Some(resources)=ntoskrnl::pnp::xhci_resources() else{return -1};
 if resources.bus!=bus||resources.device!=device||resources.function!=function{return -1}
 let command=unsafe{pci_config_read32(bus,device,function,0x04)} as u16;
 unsafe{pci_config_write16(bus,device,function,0x04,command&!0x4);}
 let verify=unsafe{pci_config_read32(bus,device,function,0x04)} as u16;
 if verify&0x4==0{0}else{-1}
}

static VT_D_REQUESTER_OWNER:core::sync::atomic::AtomicU32=core::sync::atomic::AtomicU32::new(u32::MAX);
fn initialize_vtd_from_acpi(path:&pci::PciRequesterPath)->bool{
 if !platform_drivers::is_ready(platform_drivers::Stage::Acpi)
    || !platform_drivers::is_ready(platform_drivers::Stage::PciXhciResources) { return false; }
 let requester_key=((path.segment as u32)<<16)|((path.endpoint_bus as u32)<<8)
    |((path.endpoint_device as u32)<<3)|path.endpoint_function as u32;
 if dma::iommu::vtd::segment_is_enabled(path.segment){
  let ready=dma::iommu::available()&&VT_D_REQUESTER_OWNER.load(core::sync::atomic::Ordering::Acquire)==requester_key;
  if ready { let _ = platform_drivers::mark_ready(platform_drivers::Stage::VtdRequester); }
  return ready;
 }
 let rsdp=acpi::rsdp_address();
 if rsdp==0{unsafe{serial_write(b"VT-d: RSDP unavailable; DMA remains fail-closed.\r\n");}return false}
 let table=match unsafe{acpi::find_table(rsdp,*b"DMAR",0)}{
  Ok(t)=>t,Err(_)=>{unsafe{serial_write(b"VT-d: ACPI DMAR table not found; DMA remains fail-closed.\r\n");}return false}
 };
 let info=match acpi::parse_dmar(table){Ok(v)=>v,Err(_)=>{unsafe{serial_write(b"VT-d: malformed DMAR table; DMA remains fail-closed.\r\n");}return false}};
 let count=path.path_count as usize;
 if count==0||count>8{return false}
 let mut dmar_path=[acpi::DmarPath{device:0,function:0};8];
 for i in 0..count {
  dmar_path[i]=acpi::DmarPath{device:path.path[i].device,function:path.path[i].function};
 }
 let Some(unit)=acpi::select_dmar_unit(&info,path.segment,path.root_bus,&dmar_path[..count]).ok() else{
  unsafe{serial_write(b"VT-d: no DRHD scope owns the complete xHCI PCI path; DMA remains fail-closed.\r\n");}
  return false
 };
 let ops=dma::iommu::vtd::PlatformOps{
  size:core::mem::size_of::<dma::iommu::vtd::PlatformOps>() as u32,
  context:core::ptr::null_mut(),alloc_table:Some(vtd_alloc_table),free_table:Some(vtd_free_table),
  map_mmio:Some(vtd_map_mmio),unmap_mmio:Some(vtd_unmap_mmio),clean_cache:Some(vtd_clean_cache),
  quarantine_requester:Some(vtd_quarantine_requester),
 };
 let status=unsafe{dma::iommu::vtd::initialize_for_pci_path(&ops,&info,path,0x1000)};
 if status!=dma::iommu::vtd::STATUS_SUCCESS||!dma::iommu::available(){
  unsafe{serial_write(b"VT-d: initialization or generic DMA backend registration failed; DMA remains fail-closed status=");serial_write_hex(status as u32 as u64);serial_write(b"\r\n");}
  return false
 }
 let Some(resources)=ntoskrnl::pnp::xhci_resources() else{return false};
 let Some(requester)=dma::iommu::vtd::RequesterId::new_segment(path.segment,path.endpoint_bus,path.endpoint_device,path.endpoint_function) else{return false};
 // Only firmware-described RAM that the early physical allocator has already
 // excluded may be identity-mapped. MMIO, unusable pages and kernel-image pages fail closed.
 for region in info.rmrr.regions.iter().take(info.rmrr.count) {
  if region.segment!=path.segment {continue}
  let scoped=region.scopes.iter().take(region.scope_count as usize)
    .any(|scope|acpi::dmar_scope_matches_path(scope,path.root_bus,&dmar_path[..count]));
  if !scoped {continue}
  let Some(length)=region.limit.checked_sub(region.base).and_then(|n|n.checked_add(1)) else{return false};
  let Some(region_end)=region.base.checked_add(length) else{return false};
  // DMAR's DmarInfo describes firmware tables, not the loaded kernel image.
  // Use linker-provided physical section boundaries so a firmware RMRR can
  // never be identity-mapped over live kernel text/data/BSS.
  let kernel_sections = unsafe {
   [
    (core::ptr::addr_of!(__text_phys_start) as u64, core::ptr::addr_of!(__text_phys_end) as u64),
    (core::ptr::addr_of!(__rodata_phys_start) as u64, core::ptr::addr_of!(__rodata_phys_end) as u64),
    (core::ptr::addr_of!(__data_phys_start) as u64, core::ptr::addr_of!(__data_phys_end) as u64),
    (core::ptr::addr_of!(__bss_phys_start) as u64, core::ptr::addr_of!(__bss_phys_end) as u64),
   ]
  };
  if kernel_sections.iter().any(|(start,end)| *start < *end && region.base < *end && *start < region_end) {
   return false;
  }
  if !memory::is_firmware_reserved_ram_range(region.base,length) {
   unsafe{serial_write(b"VT-d: RMRR is not safely reserved RAM; refusing unsafe identity mapping.\r\n");}
   return false
  }
  let status=unsafe{dma::iommu::vtd::map_rmrr_identity(resources.device_object as usize,requester,region.base,length)};
  if status!=dma::iommu::vtd::STATUS_SUCCESS {
   unsafe{serial_write(b"VT-d: failed to publish scoped RMRR identity mapping; requester remains bus-master-disabled status=");serial_write_hex(status as u32 as u64);serial_write(b"\r\n");}
   return false
  }
 }
 VT_D_REQUESTER_OWNER.store(requester_key,core::sync::atomic::Ordering::Release);
 let _ = platform_drivers::mark_ready(platform_drivers::Stage::VtdRequester);
 unsafe{serial_write(b"VT-d: exact DRHD and scoped RMRR mappings installed for the full PCI bridge path.\r\n");}
 true
}


static ACPI_DRIVER_BINDING_STARTED:core::sync::atomic::AtomicBool=core::sync::atomic::AtomicBool::new(false);
static mut ACPI_DRIVER_OBJECT_STORAGE:core::mem::MaybeUninit<ntoskrnl::driver_loader::DriverObject>=core::mem::MaybeUninit::uninit();
static mut ACPI_REGISTRY_IMAGE_BUFFER:[u8;ACPI_PE_IMAGE.len()]=[0;ACPI_PE_IMAGE.len()];

/// Load ACPI.sys as an independent boot-start native driver. Firmware table
/// discovery is delegated to the Windows-compatible HalAcpiGetTableEx import;
/// no private physical-address handoff is exposed to the driver.
fn initialize_acpi_driver(rsdp_address:u64)->bool{
 use core::sync::atomic::Ordering;
 if ACPI_PE_IMAGE.is_empty()||rsdp_address==0{return false}
 if ACPI_DRIVER_BINDING_STARTED.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).is_err(){return false}
 unsafe{
  let driver=core::ptr::addr_of_mut!(ACPI_DRIVER_OBJECT_STORAGE).cast::<ntoskrnl::driver_loader::DriverObject>();
  driver.write(core::mem::zeroed());
  (*driver).object_type=4;(*driver).size=core::mem::size_of::<ntoskrnl::driver_loader::DriverObject>() as i16;
  (*driver).driver_extension=core::ptr::null_mut();
  let Some(system_bytes)=boot_hive_bytes("SYSTEM") else{return false};
  let system_hive=match ntoskrnl::cm::hive::Hive::open(system_bytes){Ok(h)=>h,Err(_)=>return false};
  const SERVICE:&[u16]=&[65,67,80,73];
  const IMAGE_PATH:&[u16]=&[92,83,121,115,116,101,109,82,111,111,116,92,83,121,115,116,101,109,51,50,92,100,114,105,118,101,114,115,92,97,99,112,105,46,115,121,115];
  let mut registry_path=[0u16;1024];
  let image_buffer=core::slice::from_raw_parts_mut(core::ptr::addr_of_mut!(ACPI_REGISTRY_IMAGE_BUFFER).cast::<u8>(),ACPI_PE_IMAGE.len());
  ntoskrnl::driver_loader::load_registered_service_with_resolver(
   &system_hive,SERVICE,&mut registry_path,image_buffer,driver,core::ptr::null_mut(),
   |path,out|{
    let equal=path.len()==IMAGE_PATH.len()&&path.iter().zip(IMAGE_PATH).all(|(a,b)|{
     let a=if (b'A' as u16..=b'Z' as u16).contains(a){*a+32}else{*a};
     let b=if (b'A' as u16..=b'Z' as u16).contains(b){*b+32}else{*b};a==b
    });
    if !equal||out.len()<ACPI_PE_IMAGE.len(){return None}
    out[..ACPI_PE_IMAGE.len()].copy_from_slice(ACPI_PE_IMAGE);Some(ACPI_PE_IMAGE.len())
   },ntoskrnl::resolve
  ).is_ok()
 }
}

static PCI_DRIVER_BINDING_STARTED:core::sync::atomic::AtomicBool=core::sync::atomic::AtomicBool::new(false);
static mut PCI_DRIVER_OBJECT_STORAGE:core::mem::MaybeUninit<ntoskrnl::driver_loader::DriverObject>=core::mem::MaybeUninit::uninit();
static mut PCI_HANDOFF_STORAGE:core::mem::MaybeUninit<nt_abi::bus::PCI_DRIVER_HANDOFF>=core::mem::MaybeUninit::uninit();
static mut PCI_REGISTRY_IMAGE_BUFFER:[u8;PCI_PE_IMAGE.len()]=[0;PCI_PE_IMAGE.len()];
static mut PCI_BUS_FDO:*mut io::wdm::DeviceObject=core::ptr::null_mut();

/// Load pci.sys from its registry service entry, pass it firmware-derived ECAM
/// mappings, and let it create PDOs through the documented NTOSKRNL imports.
/// Firmware discovery and MMIO mapping remain bootstrap responsibilities;
/// PCI configuration-space enumeration is exclusively performed by PCI.sys.
fn initialize_pci_bus_driver(rsdp_address:u64)->usize {
 use core::sync::atomic::Ordering;
 use nt_abi::bus::{PCI_DRIVER_HANDOFF,PCI_ECAM_WINDOW,PCI_DRIVER_HANDOFF_MAGIC,PCI_DRIVER_HANDOFF_VERSION,PCI_DRIVER_HANDOFF_MAX_WINDOWS};
 if PCI_PE_IMAGE.is_empty()||rsdp_address==0{return 0;}
 if PCI_DRIVER_BINDING_STARTED.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).is_err(){return 0;}
 let table=match unsafe{acpi::find_table(rsdp_address,*b"MCFG",0)}{Ok(t)=>t,Err(_)=>return 0};
 let mcfg=match acpi::parse_mcfg(table){Ok(v)=>v,Err(_)=>return 0};
 if mcfg.count==0||mcfg.count>PCI_DRIVER_HANDOFF_MAX_WINDOWS{return 0;}
 let mut handoff=PCI_DRIVER_HANDOFF{
  magic:PCI_DRIVER_HANDOFF_MAGIC,size:core::mem::size_of::<PCI_DRIVER_HANDOFF>() as u32,
  version:PCI_DRIVER_HANDOFF_VERSION,window_count:mcfg.count as u32,reserved:0,
  windows:[PCI_ECAM_WINDOW::default();PCI_DRIVER_HANDOFF_MAX_WINDOWS],
 };
 // Reject overlapping ECAM allocations before publishing any aperture to HAL.
 for (i,allocation) in mcfg.allocations.iter().take(mcfg.count).enumerate() {
  if mcfg.allocations.iter().take(i).any(|prior|prior.segment==allocation.segment
   &&prior.start_bus<=allocation.end_bus&&allocation.start_bus<=prior.end_bus){return 0;}
 }
 let mut mapped=0usize;
 for allocation in mcfg.allocations.iter().take(mcfg.count){
  let span=((allocation.end_bus as u64-allocation.start_bus as u64)+1).checked_mul(1<<20);
  let Some(length)=span else{return 0};
  let virtual_base=match kernel_vm::map_io_space(allocation.base_address,length){
   Ok(v)=>v,Err(_)=>{
    for prior in handoff.windows.iter().take(mapped){let len=((prior.bus_end as u64-prior.bus_start as u64)+1)<<20;let _=kernel_vm::unmap_io_space(prior.virtual_base,len);}
    return 0
   }
  };
  handoff.windows[mapped]=PCI_ECAM_WINDOW{
   virtual_base,physical_base:allocation.base_address,segment:allocation.segment,
   bus_start:allocation.start_bus,bus_end:allocation.end_bus,
  };
  mapped+=1;
 }
 // Publish validated, already mapped apertures to HAL only after the complete
 // MCFG set has been mapped and checked for overlap.
 for window in handoff.windows.iter().take(mapped) {
  if hal::register_pci_ecam_window(window.virtual_base,window.physical_base,window.segment,window.bus_start,window.bus_end).is_err(){return 0;}
 }
 unsafe{
  let handoff_ptr=core::ptr::addr_of_mut!(PCI_HANDOFF_STORAGE).cast::<PCI_DRIVER_HANDOFF>();
  handoff_ptr.write(handoff);
  let driver=core::ptr::addr_of_mut!(PCI_DRIVER_OBJECT_STORAGE).cast::<ntoskrnl::driver_loader::DriverObject>();
  driver.write(core::mem::zeroed());
  (*driver).object_type=4;(*driver).size=core::mem::size_of::<ntoskrnl::driver_loader::DriverObject>() as i16;
  (*driver).driver_extension=handoff_ptr.cast();
  let Some(system_bytes)=boot_hive_bytes("SYSTEM") else{return 0};
  let system_hive=match ntoskrnl::cm::hive::Hive::open(system_bytes){Ok(h)=>h,Err(_)=>return 0};
  const SERVICE:&[u16]=&[80,67,73];
  const IMAGE_PATH:&[u16]=&[92,83,121,115,116,101,109,82,111,111,116,92,83,121,115,116,101,109,51,50,92,100,114,105,118,101,114,115,92,112,99,105,46,115,121,115];
  let mut registry_path=[0u16;1024];
  let image_buffer=core::slice::from_raw_parts_mut(core::ptr::addr_of_mut!(PCI_REGISTRY_IMAGE_BUFFER).cast::<u8>(),PCI_PE_IMAGE.len());
  let loaded=ntoskrnl::driver_loader::load_registered_service_with_resolver(
   &system_hive,SERVICE,&mut registry_path,image_buffer,driver,core::ptr::null_mut(),
   |path,out|{
    let equal=path.len()==IMAGE_PATH.len()&&path.iter().zip(IMAGE_PATH).all(|(a,b)|{
     let a=if (b'A' as u16..=b'Z' as u16).contains(a){*a+32}else{*a};
     let b=if (b'A' as u16..=b'Z' as u16).contains(b){*b+32}else{*b};a==b
    });
    if !equal||out.len()<PCI_PE_IMAGE.len(){return None;}
    out[..PCI_PE_IMAGE.len()].copy_from_slice(PCI_PE_IMAGE);Some(PCI_PE_IMAGE.len())
   },ntoskrnl::resolve);
  if loaded.is_err(){return 0;}
  let mut bus=core::ptr::null_mut();
  let status=io::wdm::io_create_device(driver.cast(),0,core::ptr::null_mut(),0x22,0,0,&mut bus);
  if status!=nt_abi::ntstatus::STATUS_SUCCESS||bus.is_null(){return 0;}
  let root_index=match ntoskrnl::pnp::register_root_device(){Ok(index)=>index,Err(_)=>{
   let _=io::wdm::io_delete_device(bus);return 0
  }};
  if ntoskrnl::pnp::bind_pnp_device_object(root_index,bus).is_err(){
   let _=ntoskrnl::pnp::transition_pnp_device(root_index,ntoskrnl::pnp::PNP_DEVICE_REMOVED,nt_abi::ntstatus::STATUS_INVALID_DEVICE_REQUEST);
   let _=io::wdm::io_delete_device(bus);return 0
  }
  PCI_BUS_FDO=bus;
  let status=io::wdm::IoPnPEnumerateBus(bus);
  if status!=nt_abi::ntstatus::STATUS_SUCCESS {
   let _=ntoskrnl::pnp::transition_pnp_device(root_index,ntoskrnl::pnp::PNP_DEVICE_REMOVED,status);
   let _=io::wdm::io_delete_device(bus);PCI_BUS_FDO=core::ptr::null_mut();return 0;
  }
  // Transfer the validated PDO identities from the WDM BusRelations table
  // into the kernel's registry-facing devnode database. No PCI config access
  // occurs here; the data originates from pci.sys-created PDO extensions.
  let mut count=0usize;
  for child_index in 0..ntoskrnl::pnp::MAX_PNP_DEVICES {
   let mut child=core::ptr::null_mut();
   let query=io::wdm::IoPnPQueryBusChild(bus,child_index as u32,&mut child);
   if query!=nt_abi::ntstatus::STATUS_SUCCESS {break;}
   if child.is_null(){continue;}
   let extension=(*child).device_extension.cast::<io::wdm::PciPdoExtension>();
   if extension.is_null()||(*extension).magic!=io::wdm::PCI_PDO_EXTENSION_MAGIC{continue;}
   if let Ok(index)=ntoskrnl::pnp::register_pci_information(&(*extension).information,Some(root_index)){
    if ntoskrnl::pnp::bind_pnp_device_object(index,child).is_ok(){count+=1;}
    else {let _=ntoskrnl::pnp::transition_pnp_device(index,ntoskrnl::pnp::PNP_DEVICE_REMOVED,nt_abi::ntstatus::STATUS_INVALID_DEVICE_REQUEST);}
   }
  }
  let _=ntoskrnl::pnp::match_registry_services(&system_hive);
  count
 }
}

/// Compatibility call site retained while resource assignment and xHCI start
/// are completed by the PnP/resource-arbitration stages.
fn enumerate_acpi_pci_devices(rsdp_address:u64)->usize {
 initialize_pci_bus_driver(rsdp_address)
}

/// Query PCI.sys' versioned inventory, resolve the endpoint's full bridge
/// path, size BAR0 with decode and bus mastering disabled, then publish the
/// kernel-owned PnP resource record. PCI.sys remains the sole enumerator.
fn probe_xhci_pci_function(madt:&acpi::MadtInfo,bsp_apic_id:u32)->bool {
    use core::mem::MaybeUninit;
    use nt_abi::bus::{PCI_BUS_INTERFACE,PCI_XHCI_INFORMATION,PCI_BRIDGE_INFORMATION,PCI_BUS_INTERFACE_VERSION,GUID_PCI_BUS_INTERFACE};
    let bus=unsafe{PCI_BUS_FDO};
    if bus.is_null(){return false}
    let mut interface_storage=MaybeUninit::<PCI_BUS_INTERFACE>::uninit();
    let status=unsafe{io::wdm::IoPnPQueryBusInterface(
        bus,&GUID_PCI_BUS_INTERFACE,core::mem::size_of::<PCI_BUS_INTERFACE>() as u16,
        PCI_BUS_INTERFACE_VERSION as u16,interface_storage.as_mut_ptr().cast(),
    )};
    if status!=nt_abi::ntstatus::STATUS_SUCCESS{
        
        return false
    }
    let interface=unsafe{interface_storage.assume_init()};
    if (interface.header.size as usize)<core::mem::size_of::<PCI_BUS_INTERFACE>()
        ||interface.header.version as u32!=PCI_BUS_INTERFACE_VERSION {
        return false
    }
    let (Some(query_counts),Some(query_bridge),Some(query_xhci),Some(read_config),Some(write_config))=(
        interface.query_counts,interface.query_bridge,interface.query_xhci,
        interface.read_config,interface.write_config,
    ) else {return false};
    let mut function_count=0u32;let mut bridge_count=0u32;let mut xhci_count=0u32;
    let status=unsafe{query_counts(interface.header.context,&mut function_count,&mut bridge_count,&mut xhci_count)};
    if status!=nt_abi::ntstatus::STATUS_SUCCESS||function_count==0||xhci_count==0 {
        return false
    }
    let mut selected=None;
    for index in 0..xhci_count {
        let mut candidate=PCI_XHCI_INFORMATION::default();
        if unsafe{query_xhci(interface.header.context,index,&mut candidate)}!=nt_abi::ntstatus::STATUS_SUCCESS{continue}
        // The current HAL bus-master hook is segment-0 only. Reject other
        // segments instead of accidentally programming legacy CF8/CFC space.
        if candidate.segment==0 {selected=Some(candidate);break}
    }
    let Some(function)=selected else{
        return false
    };
    if function.device>=32||function.function>=8||function.bar_physical==0
        ||function.interrupt_pin==0||function.interrupt_line==0xff {
        return false
    }
    let mut bridges=[pci::PciBridgeRange::default();pci::MAX_PCI_BRIDGES];
    let mut copied=0usize;
    for index in 0..bridge_count.min(pci::MAX_PCI_BRIDGES as u32) {
        let mut item=PCI_BRIDGE_INFORMATION::default();
        if unsafe{query_bridge(interface.header.context,index,&mut item)}!=nt_abi::ntstatus::STATUS_SUCCESS{continue}
        bridges[copied]=pci::PciBridgeRange{
            segment:item.segment,primary_bus:item.primary_bus,secondary_bus:item.secondary_bus,
            subordinate_bus:item.subordinate_bus,device:item.device,function:item.function,
        };
        copied+=1;
    }
    let path=match pci::resolve_requester_path(function.segment,function.root_bus,function.bus,
        function.device,function.function,&bridges[..copied]) {
        Ok(path)=>path,
        Err(_)=>{return false}
    };
    let context=interface.header.context;
    let read=|offset:u16,width:u8|->Option<u32>{
        let mut value=0u32;
        let result=unsafe{read_config(context,function.segment,function.bus,function.device,function.function,offset,width,&mut value)};
        if result==nt_abi::ntstatus::STATUS_SUCCESS{Some(value)}else{None}
    };
    let write=|offset:u16,width:u8,value:u32|->bool{
        (unsafe{write_config(context,function.segment,function.bus,function.device,function.function,offset,width,value)})
            ==nt_abi::ntstatus::STATUS_SUCCESS
    };
    // BAR sizing is a controlled configuration transaction: disable I/O,
    // memory decode and BME; probe the complete 32/64-bit BAR; restore its
    // original value and decode policy while leaving BME disabled.
    let original_command=read(0x04,2).unwrap_or(function.command as u32) as u16;
    if !write(0x04,2,(original_command&!0x7) as u32) {
        return false
    }
    let original_low=read(0x10,4);
    let original_high=if function.bar_is_64bit!=0{read(0x14,4)}else{Some(0)};
    let mut mask_low=None;let mut mask_high=Some(0u32);
    if let (Some(low),Some(high))=(original_low,original_high) {
        let original_base=((high as u64)<<32)|(low as u64&!0xf);
        if original_base==function.bar_physical
            &&write(0x10,4,u32::MAX)
            &&(function.bar_is_64bit==0||write(0x14,4,u32::MAX)) {
            mask_low=read(0x10,4);
            if function.bar_is_64bit!=0{mask_high=read(0x14,4);}
        }
    }
    let restore_high=if function.bar_is_64bit!=0 {
        original_high.is_some_and(|v|write(0x14,4,v))
    }else{true};
    let restore_low=original_low.is_some_and(|v|write(0x10,4,v));
    let restore_command=write(0x04,2,(original_command&!0x4) as u32);
    if !restore_high||!restore_low||!restore_command||mask_low.is_none()||mask_high.is_none(){
        return false
    }
    let size=if function.bar_is_64bit!=0 {
        let mask=((mask_high.unwrap() as u64)<<32)|(mask_low.unwrap() as u64&!0xf);
        (!mask).wrapping_add(1)
    }else{
        let mask=(mask_low.unwrap()&!0xf) as u32;
        (!mask).wrapping_add(1) as u64
    };
    if size<0x1000||size>0x10000||!size.is_power_of_two()||function.bar_physical&(size-1)!=0 {
        return false
    }
    // Keep the BAR physical in the PnP resource record. usbxhci.sys maps it
    // through the documented MmMapIoSpace export after DriverEntry.
    let Some(pnp_index)=ntoskrnl::pnp::find_pnp_pci_function(function.segment,function.bus,function.device,function.function) else{return false};
    let Some(pnp_snapshot)=ntoskrnl::pnp::query_pnp_device(pnp_index) else{return false};
    if pnp_snapshot.device_object==0{return false}
    if ntoskrnl::pnp::publish_xhci(&path,function.vendor_id,function.device_id,
        function.interrupt_line,function.interrupt_pin,pnp_snapshot.device_object as *mut core::ffi::c_void,
        function.bar_physical,size,0).is_err(){return false}
    if platform_drivers::mark_ready(platform_drivers::Stage::PciXhciResources).is_err(){return false}
    
    if !initialize_vtd_from_acpi(&path){
        return false
    }
    // Resolve the PCI INTx pin through MADT overrides. The route is configured
    // masked; usbxhci.sys connects its ISR through IoConnectInterrupt and enables it through HAL exports.
    let irq=function.interrupt_line;
    let mut gsi=irq as u32;let mut active_low=true;let mut level_triggered=true;let mut override_seen=false;
    for ov in madt.irq_overrides.iter().take(madt.irq_override_count){
        if ov.bus==0&&ov.source_irq==irq{
            if override_seen{return false}
            gsi=ov.gsi;active_low=ov.active_low;level_triggered=ov.level_triggered;override_seen=true;
        }
    }
    let Some(io_apic)=madt.io_apics.iter().take(madt.io_apic_count)
        .find(|io|gsi>=io.gsi_base&&gsi.saturating_sub(io.gsi_base)<120) else {
        return false
    };
    if ntoskrnl::pnp::prepare_xhci_interrupt_route(io_apic.address as u64,io_apic.gsi_base,gsi,
        bsp_apic_id as u8,active_low,level_triggered).is_err(){
        return false
    }
    
    bind_xhci_pe_driver()
}

#[repr(C)]
struct XhciDriverExtensionHeader {
 magic:u64,size:u32,version:u32,mmio_physical:u64,mmio_length:u64,device_object:u64,
 bus:u32,slot:u32,interrupt_vector:u32,max_slots:u32,interrupt_irql:u8,_padding:[u8;7],
 fdo_out:*mut u64,
 ready:core::sync::atomic::AtomicBool,
 interrupt_requested:core::sync::atomic::AtomicBool,
 event_count:core::sync::atomic::AtomicU64,
 port_change_count:core::sync::atomic::AtomicU64,
 command_completion_count:core::sync::atomic::AtomicU64,
 transfer_completion_count:core::sync::atomic::AtomicU64,
 runtime_lock:core::sync::atomic::AtomicBool,
 event_sink:core::sync::atomic::AtomicU64,
 event_context:core::sync::atomic::AtomicU64,
 operational_offset:core::sync::atomic::AtomicU32,
 port_count:core::sync::atomic::AtomicU32,
 mmio_virtual:core::sync::atomic::AtomicU64,
 dma_adapter:core::sync::atomic::AtomicU64,
 interrupt_object:core::sync::atomic::AtomicU64,
}
#[repr(C)]
struct XhciDriverExtensionBacking {
 header:XhciDriverExtensionHeader,
 // Four 24-byte DMA allocations followed by the event-ring indices and
 // lifecycle booleans in the driver's private XhciRuntimeResources value.
 runtime:[u8;104],
}
static mut XHCI_HAL_SERVICES:core::mem::MaybeUninit<cognacos_hal_contract::HalServiceTable>=core::mem::MaybeUninit::uninit();
static mut XHCI_DEVICE_SERVICES:core::mem::MaybeUninit<cognacos_hal_contract::DeviceServiceTable>=core::mem::MaybeUninit::uninit();
static mut XHCI_KERNEL_IO_SERVICES:core::mem::MaybeUninit<cognacos_hal_contract::KernelIoServiceTable>=core::mem::MaybeUninit::uninit();
static mut XHCI_FDO_DEVICE:u64=0;
static mut XHCI_DRIVER_EXTENSION:core::mem::MaybeUninit<XhciDriverExtensionBacking>=core::mem::MaybeUninit::uninit();
static mut XHCI_DRIVER_OBJECT:core::mem::MaybeUninit<ntoskrnl::driver_loader::DriverObject>=core::mem::MaybeUninit::uninit();

#[repr(C)]
struct UcxDriverExtensionHeader{
 magic:u64,size:u32,version:u32,parent_bus_device:u64,
 kernel_io:*const cognacos_hal_contract::KernelIoServiceTable,
 device_services:*const cognacos_hal_contract::DeviceServiceTable,dma_device_object:u64,
 hal:*const cognacos_hal_contract::HalServiceTable,
 event_sink:*mut core::sync::atomic::AtomicU64,
 event_context:*mut core::sync::atomic::AtomicU64,
 mmio_base:u64,operational_offset:u32,port_count:u32,
 hub_event_sink:*mut core::sync::atomic::AtomicU64,
 hub_event_context:*mut core::sync::atomic::AtomicU64,
 root_hub_device_out:*mut u64,
}
static mut UCX_DRIVER_EXTENSION:core::mem::MaybeUninit<UcxDriverExtensionHeader>=core::mem::MaybeUninit::uninit();
static mut UCX_DRIVER_OBJECT:core::mem::MaybeUninit<ntoskrnl::driver_loader::DriverObject>=core::mem::MaybeUninit::uninit();
static mut UCX_ROOT_HUB_DEVICE:u64=0;
static UCX_HUB_EVENT_SINK:core::sync::atomic::AtomicU64=core::sync::atomic::AtomicU64::new(0);
static UCX_HUB_EVENT_CONTEXT:core::sync::atomic::AtomicU64=core::sync::atomic::AtomicU64::new(0);
static mut UCX_REGISTRY_IMAGE_BUFFER:[u8;UCX01020_PE_IMAGE.len()]=[0;UCX01020_PE_IMAGE.len()];
#[repr(C)]
struct Usbhub3DriverExtensionHeader{
 magic:u64,size:u32,version:u32,
 kernel_io:*const cognacos_hal_contract::KernelIoServiceTable,
 root_hub_device:u64,parent_bus_device:u64,port_count:u32,
 event_sink:*mut core::sync::atomic::AtomicU64,
 event_context:*mut core::sync::atomic::AtomicU64,
 hub_device_out:*mut u64,child_factory_out:*mut u64,
}
static mut USBHUB3_DRIVER_EXTENSION:core::mem::MaybeUninit<Usbhub3DriverExtensionHeader>=core::mem::MaybeUninit::uninit();
static mut USBHUB3_DRIVER_OBJECT:core::mem::MaybeUninit<ntoskrnl::driver_loader::DriverObject>=core::mem::MaybeUninit::uninit();
static mut USBHUB3_REGISTRY_IMAGE_BUFFER:[u8;USBHUB3_PE_IMAGE.len()]=[0;USBHUB3_PE_IMAGE.len()];
static mut USBHUB3_DEVICE:u64=0;
static mut USBHUB3_CHILD_FACTORY:u64=0;
static USBHUB3_DRIVER_BINDING_STARTED:core::sync::atomic::AtomicBool=core::sync::atomic::AtomicBool::new(false);
fn bind_usbhub3_pe_driver()->bool{
 use core::sync::atomic::Ordering;
 if USBHUB3_PE_IMAGE.is_empty()||unsafe{UCX_ROOT_HUB_DEVICE}==0{return false}
 if USBHUB3_DRIVER_BINDING_STARTED.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).is_err(){return false}
 unsafe{
  let xhci=core::ptr::addr_of_mut!(XHCI_DRIVER_EXTENSION).cast::<XhciDriverExtensionHeader>();
  let port_count=(*xhci).port_count.load(Ordering::Acquire);
  if port_count==0||port_count>255{return false}
  let io_ptr=core::ptr::addr_of_mut!(XHCI_KERNEL_IO_SERVICES).cast::<cognacos_hal_contract::KernelIoServiceTable>();
  let ext=core::ptr::addr_of_mut!(USBHUB3_DRIVER_EXTENSION).cast::<Usbhub3DriverExtensionHeader>();
  ext.write(Usbhub3DriverExtensionHeader{
   magic:0x434f_474e_4143_4833,size:core::mem::size_of::<Usbhub3DriverExtensionHeader>() as u32,version:1,
   kernel_io:io_ptr,root_hub_device:UCX_ROOT_HUB_DEVICE,parent_bus_device:XHCI_FDO_DEVICE,port_count,
   event_sink:core::ptr::addr_of!(UCX_HUB_EVENT_SINK).cast_mut(),
   event_context:core::ptr::addr_of!(UCX_HUB_EVENT_CONTEXT).cast_mut(),
   hub_device_out:core::ptr::addr_of_mut!(USBHUB3_DEVICE),
   child_factory_out:core::ptr::addr_of_mut!(USBHUB3_CHILD_FACTORY),
  });
  let driver=core::ptr::addr_of_mut!(USBHUB3_DRIVER_OBJECT).cast::<ntoskrnl::driver_loader::DriverObject>();
  driver.write(core::mem::MaybeUninit::<ntoskrnl::driver_loader::DriverObject>::zeroed().assume_init());
  (*driver).size=core::mem::size_of::<ntoskrnl::driver_loader::DriverObject>() as i16;
  (*driver).driver_extension=ext.cast::<core::ffi::c_void>();
  let Some(system_bytes)=boot_hive_bytes("SYSTEM") else{return false};
  let hive=match ntoskrnl::cm::hive::Hive::open(system_bytes){Ok(h)=>h,Err(_)=>return false};
  const SERVICE:&[u16]=&[85,83,66,72,85,66,51];
  const IMAGE_PATH:&[u16]=&[92,83,121,115,116,101,109,82,111,111,116,92,83,121,115,116,101,109,51,50,92,100,114,105,118,101,114,115,92,117,115,98,104,117,98,51,46,115,121,115];
  let mut registry_path=[0u16;1024];
  let image_buffer=core::slice::from_raw_parts_mut(core::ptr::addr_of_mut!(USBHUB3_REGISTRY_IMAGE_BUFFER).cast::<u8>(),USBHUB3_PE_IMAGE.len());
  let result=ntoskrnl::driver_loader::load_registered_service_with_resolver(
   &hive,SERVICE,&mut registry_path,image_buffer,driver,core::ptr::null_mut(),
   |image_path,out|{
    let equal=image_path.len()==IMAGE_PATH.len()&&image_path.iter().zip(IMAGE_PATH).all(|(a,b)|{
     let aa=if (b'A' as u16..=b'Z' as u16).contains(a){*a+32}else{*a};
     let bb=if (b'A' as u16..=b'Z' as u16).contains(b){*b+32}else{*b};aa==bb
    });
    if !equal||out.len()<USBHUB3_PE_IMAGE.len(){return None}
    out[..USBHUB3_PE_IMAGE.len()].copy_from_slice(USBHUB3_PE_IMAGE);Some(USBHUB3_PE_IMAGE.len())
   },ntoskrnl::resolve
  );
  match result{
   Ok(image) if USBHUB3_DEVICE!=0&&USBHUB3_CHILD_FACTORY!=0=>{
    serial_write(b"USBHUB3 bind: hub FDO attached to UCX root hub; child-PDO factory=");serial_write_hex(USBHUB3_CHILD_FACTORY);
    serial_write(b" image=");serial_write_hex(image.image_base);serial_write(b"\\r\\n");true
   },
   _=>{serial_write(b"USBHUB3 bind: native hub driver or PDO callback registration failed.\\r\\n");false}
  }
 }
}

static UCX_DRIVER_BINDING_STARTED:core::sync::atomic::AtomicBool=core::sync::atomic::AtomicBool::new(false);

fn bind_ucx_pe_driver()->bool{
 use core::sync::atomic::Ordering;
 if UCX01020_PE_IMAGE.is_empty(){serial_write(b"UCX bind: ucx01020.sys was not embedded; root hub remains unavailable.\\r\\n");return false}
 if UCX_DRIVER_BINDING_STARTED.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).is_err(){return false}
 let xhci=core::ptr::addr_of_mut!(XHCI_DRIVER_EXTENSION).cast::<XhciDriverExtensionHeader>();
 unsafe{
  let port_count=(*xhci).port_count.load(Ordering::Acquire);
  let operational_offset=(*xhci).operational_offset.load(Ordering::Acquire);
  if port_count==0||port_count>255||operational_offset<0x20||(*xhci).event_sink.load(Ordering::Acquire)!=0{
   serial_write(b"UCX bind: xHCI event ring or root-hub geometry is unavailable.\\r\\n");return false
  }
  let io_ptr=core::ptr::addr_of_mut!(XHCI_KERNEL_IO_SERVICES).cast::<cognacos_hal_contract::KernelIoServiceTable>();
  let hal_ptr=core::ptr::addr_of_mut!(XHCI_HAL_SERVICES).cast::<cognacos_hal_contract::HalServiceTable>();
  let ext=core::ptr::addr_of_mut!(UCX_DRIVER_EXTENSION).cast::<UcxDriverExtensionHeader>();
  ext.write(UcxDriverExtensionHeader{
   magic:0x434f_474e_4143_5543,size:core::mem::size_of::<UcxDriverExtensionHeader>() as u32,version:1,parent_bus_device:XHCI_FDO_DEVICE,
   kernel_io:io_ptr,device_services:core::ptr::addr_of_mut!(XHCI_DEVICE_SERVICES).cast::<cognacos_hal_contract::DeviceServiceTable>(),dma_device_object:(*xhci).device_object,hal:hal_ptr,
   event_sink:core::ptr::addr_of_mut!((*xhci).event_sink),
   event_context:core::ptr::addr_of_mut!((*xhci).event_context),
   mmio_base:(*xhci).mmio_virtual.load(Ordering::Acquire),operational_offset,port_count,
   hub_event_sink:core::ptr::addr_of!(UCX_HUB_EVENT_SINK).cast_mut(),
   hub_event_context:core::ptr::addr_of!(UCX_HUB_EVENT_CONTEXT).cast_mut(),
   root_hub_device_out:core::ptr::addr_of_mut!(UCX_ROOT_HUB_DEVICE),
  });
  let driver=core::ptr::addr_of_mut!(UCX_DRIVER_OBJECT).cast::<ntoskrnl::driver_loader::DriverObject>();
  driver.write(core::mem::MaybeUninit::<ntoskrnl::driver_loader::DriverObject>::zeroed().assume_init());
  (*driver).size=core::mem::size_of::<ntoskrnl::driver_loader::DriverObject>() as i16;
  (*driver).driver_extension=ext.cast::<core::ffi::c_void>();
  let Some(system_bytes)=boot_hive_bytes("SYSTEM") else{return false};
  let system_hive=match ntoskrnl::cm::hive::Hive::open(system_bytes){Ok(h)=>h,Err(_)=>return false};
  const SERVICE:&[u16]=&[85,67,88,48,49,48,50,48];
  const IMAGE_PATH:&[u16]=&[92,83,121,115,116,101,109,82,111,111,116,92,83,121,115,116,101,109,51,50,92,100,114,105,118,101,114,115,92,117,99,120,48,49,48,50,48,46,115,121,115];
  let mut registry_path=[0u16;1024];
  let image_buffer=core::slice::from_raw_parts_mut(core::ptr::addr_of_mut!(UCX_REGISTRY_IMAGE_BUFFER).cast::<u8>(),UCX01020_PE_IMAGE.len());
  let loaded=ntoskrnl::driver_loader::load_registered_service_with_resolver(
   &system_hive,SERVICE,&mut registry_path,image_buffer,driver,core::ptr::null_mut(),
   |image_path,out|{
    let equal=image_path.len()==IMAGE_PATH.len()&&image_path.iter().zip(IMAGE_PATH).all(|(a,b)|{
     let aa=if (b'A' as u16..=b'Z' as u16).contains(a){*a+32}else{*a};
     let bb=if (b'A' as u16..=b'Z' as u16).contains(b){*b+32}else{*b};aa==bb
    });
    if !equal||out.len()<UCX01020_PE_IMAGE.len(){return None}
    out[..UCX01020_PE_IMAGE.len()].copy_from_slice(UCX01020_PE_IMAGE);Some(UCX01020_PE_IMAGE.len())
   },ntoskrnl::resolve
  );
  match loaded{
   Ok(image) if UCX_ROOT_HUB_DEVICE!=0=>{
    serial_write(b"UCX bind: DriverEntry succeeded root_hub=");serial_write_hex(UCX_ROOT_HUB_DEVICE);
    serial_write(b" image=");serial_write_hex(image.image_base);serial_write(b"\\r\\n");true
   },
   Ok(_)=>{serial_write(b"UCX bind: DriverEntry returned without publishing a root-hub DEVICE_OBJECT.\\r\\n");false},
   Err(_)=>{serial_write(b"UCX bind: registry-selected ucx01020.sys load failed.\\r\\n");false}
  }
 }
}

static XHCI_DRIVER_BINDING_STARTED:core::sync::atomic::AtomicBool=core::sync::atomic::AtomicBool::new(false);
static mut XHCI_REGISTRY_IMAGE_BUFFER: [u8; USBXHCI_PE_IMAGE.len()] = [0; USBXHCI_PE_IMAGE.len()];


fn bind_xhci_pe_driver()->bool {
    use core::sync::atomic::Ordering;
    if USBXHCI_PE_IMAGE.is_empty() {
        serial_write(b"xHCI bind: usbxhci.sys was not embedded; driver remains unbound.\r\n");
        return false;
    }
    if XHCI_DRIVER_BINDING_STARTED.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).is_err() { return false; }
    let Some(resources)=ntoskrnl::pnp::xhci_resources() else {
        serial_write(b"xHCI bind: PnP resource record unavailable.\r\n"); return false
    };
    if !platform_drivers::is_ready(platform_drivers::Stage::PciXhciResources)
        || !platform_drivers::is_ready(platform_drivers::Stage::VtdRequester)
        {
        serial_write(b"xHCI bind: platform dependency graph is not ready; DMA remains disabled.\r\n");
        return false;
    }
    let requester_key=((resources.segment as u32)<<16)|((resources.bus as u32)<<8)
        |((resources.device as u32)<<3)|resources.function as u32;
    if resources.interrupt_vector==0 || resources.bar_physical==0 || resources.bar_length<0x1000
        || !dma::iommu::available()
        || VT_D_REQUESTER_OWNER.load(Ordering::Acquire)!=requester_key {
        serial_write(b"xHCI bind: safe IOMMU DMA or masked interrupt route unavailable; bus mastering stays disabled.\r\n");
        return false;
    }
    let hal_table=hal::driver_service_table();
    let device_table=hal::driver_device_service_table();
    let kernel_io_table=io::driver_kernel_io_service_table();
    if !hal_table.compatible() || !device_table.compatible() || !device_table.has_dma() || !device_table.has_interrupts() || !device_table.has_bus_master() || !kernel_io_table.compatible() {
        serial_write(b"xHCI bind: required HAL/device services are incomplete.\r\n"); return false;
    }
    unsafe {
        let hal_ptr=core::ptr::addr_of_mut!(XHCI_HAL_SERVICES).cast::<cognacos_hal_contract::HalServiceTable>();
        hal_ptr.write(hal_table);
        let device_ptr=core::ptr::addr_of_mut!(XHCI_DEVICE_SERVICES).cast::<cognacos_hal_contract::DeviceServiceTable>();
        device_ptr.write(device_table);
        let kernel_io_ptr=core::ptr::addr_of_mut!(XHCI_KERNEL_IO_SERVICES).cast::<cognacos_hal_contract::KernelIoServiceTable>();
        kernel_io_ptr.write(kernel_io_table);
        let extension=core::ptr::addr_of_mut!(XHCI_DRIVER_EXTENSION).cast::<XhciDriverExtensionHeader>();
        extension.write(XhciDriverExtensionHeader {
            magic:0x434f_474e_4143_5848, size:core::mem::size_of::<XhciDriverExtensionBacking>() as u32,
            version:1, mmio_physical:resources.bar_physical, mmio_length:resources.bar_length,
            device_object:resources.device_object, bus:resources.bus as u32,
            slot:((resources.device as u32)&0x1f)|(((resources.function as u32)&7)<<5),
            interrupt_vector:resources.interrupt_vector as u32, max_slots:64, interrupt_irql:0x0d, _padding:[0;7],
            fdo_out:core::ptr::addr_of_mut!(XHCI_FDO_DEVICE),
            ready:core::sync::atomic::AtomicBool::new(false),
            interrupt_requested:core::sync::atomic::AtomicBool::new(false),
            event_count:core::sync::atomic::AtomicU64::new(0),
            port_change_count:core::sync::atomic::AtomicU64::new(0),
            command_completion_count:core::sync::atomic::AtomicU64::new(0),
            transfer_completion_count:core::sync::atomic::AtomicU64::new(0),
            runtime_lock:core::sync::atomic::AtomicBool::new(false),
            event_sink:core::sync::atomic::AtomicU64::new(0),
            event_context:core::sync::atomic::AtomicU64::new(0),
            operational_offset:core::sync::atomic::AtomicU32::new(0),
            port_count:core::sync::atomic::AtomicU32::new(0),
            mmio_virtual:core::sync::atomic::AtomicU64::new(0),
            dma_adapter:core::sync::atomic::AtomicU64::new(0),
            interrupt_object:core::sync::atomic::AtomicU64::new(0),
        });
        let driver=core::ptr::addr_of_mut!(XHCI_DRIVER_OBJECT).cast::<ntoskrnl::driver_loader::DriverObject>();
        driver.write(core::mem::MaybeUninit::<ntoskrnl::driver_loader::DriverObject>::zeroed().assume_init());
        (*driver).size=core::mem::size_of::<ntoskrnl::driver_loader::DriverObject>() as i16;
        (*driver).driver_extension=extension.cast::<core::ffi::c_void>();
        serial_write(b"xHCI bind: loading PE32+ usbxhci.sys bytes=");
        serial_write_decimal(USBXHCI_PE_IMAGE.len());
        serial_write(b" vector="); serial_write_hex(resources.interrupt_vector as u64); serial_write(b"\r\n");
        let Some(system_bytes)=boot_hive_bytes("SYSTEM") else {
            serial_write(b"xHCI bind: generated SYSTEM hive is not mounted; registry-selected load refused.\\r\\n");
            return false;
        };
        let system_hive=match ntoskrnl::cm::hive::Hive::open(system_bytes) {
            Ok(hive)=>hive,
            Err(_)=>{
                serial_write(b"xHCI bind: SYSTEM hive failed validation; registry-selected load refused.\\r\\n");
                return false;
            }
        };
        const XHCI_SERVICE_NAME:&[u16]=&[85,83,66,88,72,67,73];
        const XHCI_IMAGE_PATH:&[u16]=&[92,83,121,115,116,101,109,82,111,111,116,92,83,121,115,116,101,109,51,50,92,100,114,105,118,101,114,115,92,117,115,98,120,104,99,105,46,115,121,115];
        let mut registry_path=[0u16;1024];
        let image_buffer=core::slice::from_raw_parts_mut(
            core::ptr::addr_of_mut!(XHCI_REGISTRY_IMAGE_BUFFER).cast::<u8>(),USBXHCI_PE_IMAGE.len());
        let load_result=ntoskrnl::driver_loader::load_registered_service_with_resolver(
            &system_hive,XHCI_SERVICE_NAME,&mut registry_path,image_buffer,driver,
            core::ptr::null_mut(),
            |image_path,output|{
                let equal=image_path.len()==XHCI_IMAGE_PATH.len()
                    &&image_path.iter().zip(XHCI_IMAGE_PATH).all(|(a,b)|{
                        let a=if (b'A' as u16..=b'Z' as u16).contains(a){*a+32}else{*a};
                        let b=if (b'A' as u16..=b'Z' as u16).contains(b){*b+32}else{*b};
                        a==b
                    });
                if !equal||output.len()<USBXHCI_PE_IMAGE.len(){return None;}
                output[..USBXHCI_PE_IMAGE.len()].copy_from_slice(USBXHCI_PE_IMAGE);
                Some(USBXHCI_PE_IMAGE.len())
            },ntoskrnl::resolve);
        match load_result {
            Ok(image) => {
                serial_write(b"xHCI bind: DriverEntry succeeded image="); serial_write_hex(image.image_base);
                serial_write(b" size="); serial_write_hex(image.image_size as u64); serial_write(b"\r\n");
                // DriverEntry has successfully mapped the BAR with MmMapIoSpace
                // and initialized the per-PDO DMA adapter/common buffers.
                if ntoskrnl::pnp::advance_xhci_state(ntoskrnl::pnp::STATE_BAR_ASSIGNED,ntoskrnl::pnp::STATE_MMIO_MAPPED).is_err()
                    ||ntoskrnl::pnp::advance_xhci_state(ntoskrnl::pnp::STATE_MMIO_MAPPED,ntoskrnl::pnp::STATE_DMA_READY).is_err(){
                    serial_write(b"xHCI bind: PnP rejected post-DriverEntry MMIO/DMA state.\r\n");return false;
                }
                if !hal::device_interrupt_handler_registered(resources.interrupt_vector)
                    || platform_drivers::mark_ready(platform_drivers::Stage::XhciInterrupt).is_err() {
                    if let Some(set)=device_table.set_bus_master { let _=set(resources.device_object,false); }
                    return false;
                }
                if ntoskrnl::pnp::advance_xhci_state(ntoskrnl::pnp::STATE_DMA_READY,ntoskrnl::pnp::STATE_INTERRUPT_READY).is_err() {
                    serial_write(b"xHCI bind: DriverEntry succeeded but interrupt-ready PnP transition failed; revoking DMA before halting.\\r\\n");
                    let _=unsafe{hal::set_device_interrupt_vector_enabled(resources.interrupt_vector,false)};
                    if let Some(set)=device_table.set_bus_master { if set(resources.device_object,false).code!=0 { loop { core::arch::asm!("cli; hlt",options(nomem,nostack)); } } }
                    loop { core::arch::asm!("cli; hlt",options(nomem,nostack)); }
                }
                if ntoskrnl::pnp::advance_xhci_state(ntoskrnl::pnp::STATE_INTERRUPT_READY,ntoskrnl::pnp::STATE_DRIVER_BOUND).is_err() {
                    let _=unsafe{hal::set_device_interrupt_vector_enabled(resources.interrupt_vector,false)};
                    if let Some(set)=device_table.set_bus_master { if set(resources.device_object,false).code!=0 { loop { core::arch::asm!("cli; hlt",options(nomem,nostack)); } } }
                    loop { core::arch::asm!("cli; hlt",options(nomem,nostack)); }
                }
                let _ = platform_drivers::mark_ready(platform_drivers::Stage::XhciController);
                if !bind_ucx_pe_driver(){serial_write(b"USB stack: UCX root hub not bound; external USB enumeration remains disabled.\\r\\n");}
                else if !bind_usbhub3_pe_driver(){serial_write(b"USB stack: usbhub3 not bound; child PDO enumeration remains unavailable.\\r\\n");}
                let ext=&*extension;
                serial_write(b"xHCI runtime: events="); serial_write_hex(ext.event_count.load(Ordering::Acquire));
                serial_write(b" port_changes="); serial_write_hex(ext.port_change_count.load(Ordering::Acquire)); serial_write(b"\r\n");
                true
            }
            Err(ntoskrnl::driver_loader::RegistryDriverLoadError::Load(ntoskrnl::driver_loader::DriverLoadError::EntryFailed(status))) => {
                serial_write(b"xHCI bind: DriverEntry returned NTSTATUS=");
                serial_write_hex(status as u32 as u64);
                serial_write(b"; controller remains unbound and bus mastering stays disabled.\r\n");
                false
            }
            Err(ntoskrnl::driver_loader::RegistryDriverLoadError::Load(error)) => {
                use ntoskrnl::driver_loader::DriverLoadError as E;
                serial_write(b"xHCI bind: PE loader rejected usbxhci.sys: ");
                serial_write(match error {
                    E::Truncated=>b"truncated image",E::BadDosHeader=>b"bad DOS header",
                    E::BadPeSignature=>b"bad PE signature",E::WrongMachine=>b"wrong machine",
                    E::WrongSubsystem=>b"wrong subsystem",E::NotDll=>b"not a driver DLL",
                    E::InvalidHeader=>b"invalid PE header",E::InvalidSection=>b"invalid section",
                    E::ImageTooLarge=>b"image too large",E::WrongBase=>b"wrong image base",
                    E::ImportsUnsupported=>b"imports unsupported",E::ImportNotFound=>b"unresolved import",
                    E::InvalidImport=>b"invalid import",E::RelocationsUnsupported=>b"relocations unsupported",
                    E::RelocationUnsupported=>b"unsupported relocation",E::WxSection=>b"W+X section",
                    E::EntryNotExecutable=>b"entry not executable",E::MappingFailed=>b"mapping failed",
                    E::NoMemory=>b"out of memory",E::EntryFailed(_)=>b"DriverEntry returned failure",
                });
                serial_write(b"; controller remains unbound and bus mastering stays disabled.\r\n");
                false
            }
            Err(_registry_error) => {
                serial_write(b"xHCI bind: registry service metadata, start type, or ImagePath rejected; driver remains unbound.\r\n");
                false
            }
        }
    }
}

const BOOT_INFO_MAGIC: u64 = 0x5244_4F54_4F53_4249;
#[repr(C)]
#[derive(Clone,Copy)]
struct BootFile { address:u64, length:u64 }
impl BootFile { const EMPTY:Self=Self{address:0,length:0}; }
static BOOT_HIVE_ADDRESS:[core::sync::atomic::AtomicU64;7]=[const{core::sync::atomic::AtomicU64::new(0)};7];
static BOOT_HIVE_LENGTH:[core::sync::atomic::AtomicU64;7]=[const{core::sync::atomic::AtomicU64::new(0)};7];
static BOOT_NLS_ADDRESS:[core::sync::atomic::AtomicU64;8]=[const{core::sync::atomic::AtomicU64::new(0)};8];
static BOOT_NLS_LENGTH:[core::sync::atomic::AtomicU64;8]=[const{core::sync::atomic::AtomicU64::new(0)};8];
static BOOT_NTDLL_ADDRESS:core::sync::atomic::AtomicU64=core::sync::atomic::AtomicU64::new(0);
static BOOT_NTDLL_LENGTH:core::sync::atomic::AtomicU64=core::sync::atomic::AtomicU64::new(0);
static BOOT_SYSTEM_DLL_ADDRESS:[core::sync::atomic::AtomicU64;4]=[const{core::sync::atomic::AtomicU64::new(0)};4];
static BOOT_SYSTEM_DLL_LENGTH:[core::sync::atomic::AtomicU64;4]=[const{core::sync::atomic::AtomicU64::new(0)};4];
const BOOT_SYSTEM_DLL_NAMES:[&str;4]=["kernel32.dll","kernelbase.dll","shell32.dll","msvcrt.dll"];
static BOOT_BEEP_ADDRESS:core::sync::atomic::AtomicU64=core::sync::atomic::AtomicU64::new(0);
static BOOT_BEEP_LENGTH:core::sync::atomic::AtomicU64=core::sync::atomic::AtomicU64::new(0);
static BOOT_SECURITY_FLAGS:core::sync::atomic::AtomicU64=core::sync::atomic::AtomicU64::new(0);
static BOOT_KERNEL_DIGEST:[core::sync::atomic::AtomicU8;32]=[const{core::sync::atomic::AtomicU8::new(0)};32];

/// Security properties attested by the bootloader before transferring control.
pub fn boot_security_flags()->u64{BOOT_SECURITY_FLAGS.load(core::sync::atomic::Ordering::Acquire)}
/// SHA-256 of the exact kernel PE32+ file verified by the bootloader.
pub fn boot_kernel_digest()->[u8;32]{
    let mut digest=[0u8;32];
    for (index,byte) in digest.iter_mut().enumerate(){
        *byte=BOOT_KERNEL_DIGEST[index].load(core::sync::atomic::Ordering::Acquire);
    }
    digest
}
pub fn boot_kernel_signature_verified()->bool{boot_security_flags()&(1<<6)!=0}
pub fn boot_secure_boot_enabled()->Option<bool>{
    let flags=boot_security_flags();
    if flags&(1<<0)==0{None}else{Some(flags&(1<<1)!=0)}
}
pub fn boot_tpm_kernel_measurement_recorded()->bool{boot_security_flags()&(1<<5)!=0}
static mut THREAD_SMOKE_STACK: [u8; 16384] = [0; 16384];
static mut THREAD_BLOCK_STACK: [u8; 16384] = [0; 16384];
static mut THREAD_WAKE_STACK: [u8; 16384] = [0; 16384];
static THREAD_SMOKE_RAN: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
static THREAD_BLOCK_STAGE: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);

extern "C" fn thread_smoke_entry(_argument: *mut core::ffi::c_void) {
    THREAD_SMOKE_RAN.store(true, core::sync::atomic::Ordering::Release);
}
extern "C" fn thread_block_smoke_entry(_argument: *mut core::ffi::c_void) {
    THREAD_BLOCK_STAGE.store(1, core::sync::atomic::Ordering::Release);
    if thread::block_current().is_err() {
        THREAD_BLOCK_STAGE.store(0xff, core::sync::atomic::Ordering::Release);
        return;
    }
    THREAD_BLOCK_STAGE.store(3, core::sync::atomic::Ordering::Release);
}
extern "C" fn thread_wake_smoke_entry(argument: *mut core::ffi::c_void) {
    if thread::wake(argument as u64).is_ok() {
        THREAD_BLOCK_STAGE.store(2, core::sync::atomic::Ordering::Release);
    } else {
        THREAD_BLOCK_STAGE.store(0xfe, core::sync::atomic::Ordering::Release);
    }
}

unsafe extern "C" {
    static __text_start: u8;
    static __text_end: u8;
    static __rodata_start: u8;
    static __rodata_end: u8;
    static __data_start: u8;
    static __data_end: u8;
    static __bss_start: u8;
    static __bss_end: u8;
    static __text_phys_start: u8;
    static __text_phys_end: u8;
    static __rodata_phys_start: u8;
    static __rodata_phys_end: u8;
    static __data_phys_start: u8;
    static __data_phys_end: u8;
    static __bss_phys_start: u8;
    static __bss_phys_end: u8;
}

#[repr(C)]
struct BootInfo {
    magic: u64,
    version: u32,
    struct_size: u32,
    pml4_physical: u64,
    memory_map: *const memory::MemoryDescriptor,
    memory_map_entries: usize,
    memory_descriptor_size: usize,
    memory_descriptor_version: u32,
    kernel_stack_guard_page: u64,
    kernel_stack_pages: u64,
    kernel_stack_virtual_guard: u64,
    rsdp_address: u64,
    ap_trampoline_physical: u64,
    kernel_physical_sections: [(u64, u64); 4],
    kernel_physical_section_count: usize,
    registry_hives: [BootFile;7],
    nls_files: [BootFile;8],
    ntdll_file: BootFile,
    system_dlls: [BootFile;4],
    beep_file: BootFile,
    security_flags: u64,
    kernel_digest: [u8; 32],
}


pub fn boot_hive_bytes(name:&str)->Option<&'static [u8]>{
 let kind=ntoskrnl::cm::hive::hive_kind(name).ok()? as usize;
 let address=BOOT_HIVE_ADDRESS.get(kind)?.load(core::sync::atomic::Ordering::Acquire);
 let length=BOOT_HIVE_LENGTH.get(kind)?.load(core::sync::atomic::Ordering::Acquire) as usize;
 if address==0||length==0||length>256*1024*1024{return None;}
 Some(unsafe{core::slice::from_raw_parts(address as *const u8,length)})
}
pub fn boot_ntdll_bytes()->Option<&'static [u8]>{
 let address=BOOT_NTDLL_ADDRESS.load(core::sync::atomic::Ordering::Acquire);
 let length=BOOT_NTDLL_LENGTH.load(core::sync::atomic::Ordering::Acquire) as usize;
 if address==0||length==0||length>16*1024*1024{return None;}
 Some(unsafe{core::slice::from_raw_parts(address as *const u8,length)})
}
pub fn boot_system_dll_bytes(name:&str)->Option<&'static [u8]>{
 let index=BOOT_SYSTEM_DLL_NAMES.iter().position(|candidate|candidate.eq_ignore_ascii_case(name))?;
 let address=BOOT_SYSTEM_DLL_ADDRESS[index].load(core::sync::atomic::Ordering::Acquire);
 let length=BOOT_SYSTEM_DLL_LENGTH[index].load(core::sync::atomic::Ordering::Acquire) as usize;
 if address==0||length==0||length>16*1024*1024{return None;}
 Some(unsafe{core::slice::from_raw_parts(address as *const u8,length)})
}
pub fn boot_beep_bytes()->Option<&'static [u8]>{
 let address=BOOT_BEEP_ADDRESS.load(core::sync::atomic::Ordering::Acquire);
 let length=BOOT_BEEP_LENGTH.load(core::sync::atomic::Ordering::Acquire) as usize;
 if address==0||length==0||length>4*1024*1024{return None;}
 Some(unsafe{core::slice::from_raw_parts(address as *const u8,length)})
}
pub fn boot_nls_bytes(name:&str)->Option<&'static [u8]>{
 let index=nls::FILES.iter().position(|f|f.name.eq_ignore_ascii_case(name))?;
 let address=BOOT_NLS_ADDRESS[index].load(core::sync::atomic::Ordering::Acquire);
 let length=BOOT_NLS_LENGTH[index].load(core::sync::atomic::Ordering::Acquire) as usize;
 if address==0||length==0||length>nls::MAX_NLS_FILE_SIZE{return None;}
 Some(unsafe{core::slice::from_raw_parts(address as *const u8,length)})
}
unsafe fn install_boot_assets(info:&BootInfo){
 let mut hive_count=0usize;let mut nls_count=0usize;
 for i in 0..info.registry_hives.len(){
  let file=info.registry_hives[i];if file.address==0&&file.length==0{continue;}
  if file.address==0||file.length==0||file.length>256*1024*1024||file.address.checked_add(file.length).is_none_or(|e|e>0x1_0000_0000){unsafe{serial_write(b"Rejected malformed boot registry asset descriptor.\r\n");}continue;}
  let bytes=unsafe{core::slice::from_raw_parts(file.address as *const u8,file.length as usize)};
  if ntoskrnl::cm::hive::Hive::open(bytes).is_err(){unsafe{serial_write(b"Rejected invalid or dirty native registry hive.\r\n");}continue;}
  if ntoskrnl::cm::runtime::mount_hive_bytes(ntoskrnl::cm::hive::HIVE_NAMES[i],bytes).is_err(){
   unsafe{serial_write(b"Configuration Manager rejected registry hive mount.\r\n");}
   continue;
  }
  BOOT_HIVE_ADDRESS[i].store(file.address,core::sync::atomic::Ordering::Release);
  BOOT_HIVE_LENGTH[i].store(file.length,core::sync::atomic::Ordering::Release);hive_count+=1;
 }
 let ntdll=info.ntdll_file;
 if ntdll.address!=0||ntdll.length!=0 {
  if ntdll.address==0||ntdll.length==0||ntdll.length>16*1024*1024||ntdll.address.checked_add(ntdll.length).is_none_or(|e|e>0x1_0000_0000){
   unsafe{serial_write(b"Rejected malformed boot ntdll.dll descriptor.\r\n");}
  }else{
   BOOT_NTDLL_ADDRESS.store(ntdll.address,core::sync::atomic::Ordering::Release);
   BOOT_NTDLL_LENGTH.store(ntdll.length,core::sync::atomic::Ordering::Release);
  }
 }
 for (index,file) in info.system_dlls.iter().copied().enumerate() {
  if file.address==0&&file.length==0 { continue; }
  if file.address==0||file.length==0||file.length>16*1024*1024||file.address.checked_add(file.length).is_none_or(|e|e>0x1_0000_0000) {
   unsafe{serial_write(b"Rejected malformed boot system DLL descriptor.\\r\\n");}
   continue;
  }
  let bytes=unsafe{core::slice::from_raw_parts(file.address as *const u8,file.length as usize)};
  if bytes.get(..2)==Some(b"MZ") {
   BOOT_SYSTEM_DLL_ADDRESS[index].store(file.address,core::sync::atomic::Ordering::Release);
   BOOT_SYSTEM_DLL_LENGTH[index].store(file.length,core::sync::atomic::Ordering::Release);
  } else {
   unsafe{serial_write(b"Rejected staged system DLL without MZ signature.\\r\\n");}
  }
 }
 let beep=info.beep_file;
 if beep.address!=0||beep.length!=0 {
  if beep.address==0||beep.length==0||beep.length>4*1024*1024||beep.address.checked_add(beep.length).is_none_or(|e|e>0x1_0000_0000){
   unsafe{serial_write(b"Rejected malformed boot beep.sys descriptor.\r\n");}
  }else{
   let bytes=unsafe{core::slice::from_raw_parts(beep.address as *const u8,beep.length as usize)};
   if bytes.get(..2)==Some(b"MZ") {
    BOOT_BEEP_ADDRESS.store(beep.address,core::sync::atomic::Ordering::Release);
    BOOT_BEEP_LENGTH.store(beep.length,core::sync::atomic::Ordering::Release);
    unsafe{serial_write(b"Native beep.sys driver staged for runtime loading.\r\n");}
   }else{unsafe{serial_write(b"Rejected boot beep.sys image without MZ signature.\r\n");}}
  }
 }
 for i in 0..info.nls_files.len(){
  let file=info.nls_files[i];if file.address==0&&file.length==0{continue;}
  if file.address==0||file.length==0||file.length>nls::MAX_NLS_FILE_SIZE as u64||file.address.checked_add(file.length).is_none_or(|e|e>0x1_0000_0000){unsafe{serial_write(b"Rejected malformed boot NLS asset descriptor.\r\n");}continue;}
  let bytes=unsafe{core::slice::from_raw_parts(file.address as *const u8,file.length as usize)};
  if nls::NlsFile::open(nls::FILES[i].name,bytes).is_err(){unsafe{serial_write(b"Rejected invalid native NLS file.\r\n");}continue;}
  BOOT_NLS_ADDRESS[i].store(file.address,core::sync::atomic::Ordering::Release);
  BOOT_NLS_LENGTH[i].store(file.length,core::sync::atomic::Ordering::Release);nls_count+=1;
 }
 unsafe{serial_write(b"Native Windows assets staged: registry hives=");serial_write_hex(hive_count as u64);serial_write(b", NLS tables=");serial_write_hex(nls_count as u64);serial_write(b"\r\n");}
}

#[unsafe(no_mangle)]
pub extern "C" fn _start(boot_info: *const BootInfo) -> ! {
    unsafe {
        serial_init();
        serial_write(b"CognacOS Rust kernel entered.\r\n");
        if !pe_modules::self_test() {
            serial_write(b"FATAL: PE module lifetime metadata self-test failed.\r\n");
            halt();
        }
        serial_write(b"PE module lifetime metadata self-test passed.\r\n");
        if boot_info.is_null() {
            serial_write(b"FATAL: null boot information pointer.\r\n");
            halt();
        }
        let info = &*boot_info;
        if info.magic == BOOT_INFO_MAGIC && info.version == 11
            && (info.struct_size as usize) >= core::mem::size_of::<BootInfo>() {
            BOOT_SECURITY_FLAGS.store(info.security_flags,core::sync::atomic::Ordering::Release);
            for (index,byte) in info.kernel_digest.iter().copied().enumerate(){
                BOOT_KERNEL_DIGEST[index].store(byte,core::sync::atomic::Ordering::Release);
            }
            install_boot_assets(info);
        }
        if info.magic != BOOT_INFO_MAGIC || info.version != 11
            || (info.struct_size as usize) < core::mem::size_of::<BootInfo>()
        {
            serial_write(b"FATAL: invalid boot information structure.\r\n");
            halt();
        }
        if info.memory_map_entries > 0 && info.memory_map.is_null() {
            serial_write(b"FATAL: missing memory map.\r\n");
            halt();
        }
        if info.memory_descriptor_size < core::mem::size_of::<memory::MemoryDescriptor>() {
            serial_write(b"FATAL: unsupported UEFI memory descriptor size.\r\n");
            halt();
        }
        if info.kernel_stack_pages == 0 || info.kernel_stack_pages > 1024
            || info.kernel_stack_guard_page & 0xfff != 0
            || info.kernel_stack_virtual_guard < 0xffff_8000_0000_0000
            || info.kernel_stack_virtual_guard & 0xfff != 0
        {
            serial_write(b"FATAL: invalid high-half kernel stack description.\r\n");
            halt();
        }
        let stack_allocation_bytes = match info.kernel_stack_pages.checked_add(1)
            .and_then(|pages| pages.checked_mul(4096)) {
            Some(bytes) => bytes,
            None => { serial_write(b"FATAL: kernel stack size overflow.\r\n"); halt(); }
        };

        if info.pml4_physical == 0 || (info.pml4_physical & 0xfff) != 0 || info.pml4_physical >= 0x1_0000_0000 {
            serial_write(b"FATAL: invalid bootstrap PML4 address.\r\n");
            halt();
        }
        serial_write(b"UEFI boot services exited successfully.\r\n");
        serial_write(b"Boot information version: 11\r\n");
        if info.security_flags & (1 << 6) == 0 {
            serial_write(b"FATAL: bootloader did not attest kernel code signature verification.\r\n");
            halt();
        }
        serial_write(b"Bootloader verified kernel SHA-256 and Ed25519 code signature.\r\n");
        if info.security_flags & (1 << 1) != 0 {
            serial_write(b"UEFI Secure Boot enabled.\r\n");
        } else if info.security_flags & (1 << 0) != 0 {
            serial_write(b"UEFI Secure Boot disabled; optional mode.\r\n");
        } else {
            serial_write(b"UEFI Secure Boot state unknown.\r\n");
        }
        if info.security_flags & (1 << 4) != 0 {
            if info.security_flags & (1 << 5) != 0 {
                serial_write(b"TPM 2.0 kernel measurement recorded.\r\n");
            } else {
                serial_write(b"TPM 2.0 present; measurement unavailable.\r\n");
            }
        } else {
            serial_write(b"No TPM 2.0 measured-boot device; optional feature skipped.\r\n");
        }
        if info.ap_trampoline_physical != 0
            && (info.ap_trampoline_physical >= 0x10_0000 || info.ap_trampoline_physical & 0xfff != 0)
        {
            serial_write(b"FATAL: invalid low-memory AP trampoline reservation.\r\n");
            halt();
        }
        if info.ap_trampoline_physical == 0 {
            serial_write(b"WARNING: AP trampoline page unavailable; SMP startup disabled.\r\n");
        } else {
            serial_write(b"AP trampoline storage reserved at: ");
            serial_write_hex(info.ap_trampoline_physical);
            serial_write(b" (not yet executable).\r\n");
        }
        serial_write(b"Memory map entries: ");
        serial_write_decimal(info.memory_map_entries);
        serial_write(b"\r\n");

        // Install a private descriptor table while the bootloader-established
        // high-half CR3 is active. The explicit reload below verifies ownership.
        ntoskrnl::ke::arch::initialize();
        // Enable the XSAVE state components managed by this kernel before
        // user threads or SIMD-using drivers can execute. Unsupported hardware
        // remains on the legacy path; APs must repeat this during AP startup.
        if ntoskrnl::ke::runtime::ki_initialize_xstate_current_cpu() {
            let cfg = ntoskrnl::ke::runtime::ki_query_xstate_configuration();
            serial_write(b"XSAVE enabled; XCR0 mask: ");
            serial_write_hex(cfg.enabled_features);
            serial_write(b", area bytes: ");
            serial_write_decimal(cfg.area_size as usize);
            serial_write(b"\r\n");
        } else {
            serial_write(b"XSAVE unavailable; extended processor state remains disabled.\r\n");
        }
        if !ntoskrnl::ke::arch::per_cpu_tss_self_test(0) {
            serial_write(b"FATAL: BSP TSS/IST descriptor self-test failed.\r\n");
            halt();
        }
        serial_write(b"Per-CPU TSS/IST setup self-test passed.\r\n");
        let bsp_apic_id = (core::arch::x86_64::__cpuid(1).ebx >> 24) & 0xff;
        if hal::initialize_bsp(bsp_apic_id).is_err()
            || unsafe { hal::install_cpu_local(0) }.is_err()
        {
            serial_write(b"FATAL: bootstrap CPU-local HAL setup failed.\r\n");
            halt();
        }
        // xHCI PCI resource probing is deferred until kernel VM is initialized.
        ntoskrnl::ke::runtime::ke_initialize_executive();
        let bsp_stack_top = match info.kernel_stack_virtual_guard.checked_add(stack_allocation_bytes) {
            Some(top) if top != 0 => top,
            _ => { serial_write(b"FATAL: bootstrap stack-top calculation overflow.\r\n"); halt(); }
        };
        if !thread::set_idle_stack_top(0, bsp_stack_top) {
            serial_write(b"FATAL: BSP idle stack/TSS RSP0 setup failed.\r\n");
            halt();
        }
        if !ntoskrnl::ke::runtime::ki_initialize_processor(0) {
            serial_write(b"FATAL: KiInitializeProcessor BSP validation failed.\r\n");
            halt();
        }
        let xstate = ntoskrnl::ke::runtime::ki_query_windows_xstate_configuration();
        if xstate.enabled_features & 3 == 3 {
            if xstate.enabled_features & !3 != 0 || xstate.enabled_supervisor_features != 0 {
                serial_write(b"FATAL: unsafe XSTATE feature mask was enabled.\r\n");
                halt();
            }
            serial_write(b"XSTATE processor policy initialized; enabled mask=");
            serial_write_hex(xstate.enabled_features);
            serial_write(b", area size=");
            serial_write_hex(xstate.size as u64);
            serial_write(b".\r\nXSTATE XSAVE/XRSTOR processor self-test passed.\r\n");
        } else {
            serial_write(b"XSTATE unavailable; extended features remain disabled.\r\n");
        }
        user::initialize();
        syscall::initialize();
        usermode::initialize();
        ntoskrnl::ke::arch::enable_kernel_paging(info.pml4_physical);
        serial_write(b"Kernel-owned CR3 activated successfully.\r\n");

        // Inspect the live page tables before allocator setup or permission
        // changes. These ranges are active bootstrap dependencies: the running
        // kernel, its current stack, BootInfo, final UEFI map, page-table root,
        // reserved stack allocation, and ACPI/AP handoff storage.
        let mut current_rsp: u64;
        let active_cr3: u64;
        asm!("mov {}, rsp", out(reg) current_rsp, options(nomem, nostack, preserves_flags));
        asm!("mov {}, cr3", out(reg) active_cr3, options(nomem, nostack, preserves_flags));
        if active_cr3 & !0xfff != info.pml4_physical {
            serial_write(b"FATAL: active CR3 does not match the validated bootstrap root.\r\n");
            halt();
        }
        let map_bytes = match (info.memory_map_entries as u64)
            .checked_mul(info.memory_descriptor_size as u64) {
            Some(bytes) => bytes,
            None => { serial_write(b"FATAL: memory-map byte length overflow.\r\n"); halt(); }
        };
        let bootstrap_ranges = [
            (info.pml4_physical, 4096u64),
            (boot_info as u64, core::mem::size_of::<BootInfo>() as u64),
            (info.memory_map as u64, map_bytes),
            (info.kernel_stack_guard_page, stack_allocation_bytes),
            (0xfec0_0000, 4096u64),
            (0xfee0_0000, 4096u64),
        ];
        let mut mapping_ok = true;
        for (start, length) in bootstrap_ranges {
            if start != 0 && unsafe {
                memory::validate_identity_range(info.pml4_physical, start, length)
            }.is_err() {
                mapping_ok = false;
                break;
            }
        }
        if info.rsdp_address != 0 && unsafe {
            memory::validate_identity_range(info.pml4_physical, info.rsdp_address, 36)
        }.is_err() {
            mapping_ok = false;
        }
        if info.ap_trampoline_physical != 0 && unsafe {
            memory::validate_identity_range(info.pml4_physical, info.ap_trampoline_physical, 4096)
        }.is_err() {
            mapping_ok = false;
        }
        if !mapping_ok {
            serial_write(b"FATAL: bootstrap page-table inspection found an unmapped or non-identity live dependency.\r\n");
            halt();
        }
        serial_write(b"Page-table inspection passed: low boot data, physical stack backing, APIC windows, and firmware handoff mappings are valid.\r\n");

        let stack_end_virtual = info.kernel_stack_virtual_guard.checked_add(stack_allocation_bytes);
        let expected_stack_physical = current_rsp.checked_sub(info.kernel_stack_virtual_guard)
            .and_then(|offset| info.kernel_stack_guard_page.checked_add(offset));
        let stack_mapping_ok = stack_end_virtual.is_some_and(|end|
            current_rsp >= info.kernel_stack_virtual_guard + 4096 && current_rsp < end)
            && expected_stack_physical.is_some_and(|expected| unsafe {
                memory::inspect_mapping(info.pml4_physical, current_rsp)
                    .is_ok_and(|mapping| mapping.physical_address == expected)
            });
        let section_mappings_ok = unsafe {
            let text_start = core::ptr::addr_of!(__text_start) as u64;
            let text_end = core::ptr::addr_of!(__text_end) as u64;
            let ro_start = core::ptr::addr_of!(__rodata_start) as u64;
            let ro_end = core::ptr::addr_of!(__rodata_end) as u64;
            let data_start = core::ptr::addr_of!(__data_start) as u64;
            let data_end = core::ptr::addr_of!(__data_end) as u64;
            let bss_start = core::ptr::addr_of!(__bss_start) as u64;
            let bss_end = core::ptr::addr_of!(__bss_end) as u64;
            let sections = &info.kernel_physical_sections;
            let count_ok = info.kernel_physical_section_count >= 3
                && info.kernel_physical_section_count <= sections.len();
            count_ok
                && sections[0].0 < sections[0].1
                && sections[1].0 < sections[1].1
                && sections[2].0 < sections[2].1
                && memory::validate_translation_range(info.pml4_physical, text_start,
                    sections[0].0, text_end - text_start).is_ok()
                && memory::validate_translation_range(info.pml4_physical, ro_start,
                    sections[1].0, ro_end - ro_start).is_ok()
                && memory::validate_translation_range(info.pml4_physical, data_start,
                    sections[2].0, data_end - data_start).is_ok()
                && memory::validate_translation_range(info.pml4_physical, bss_start,
                    sections[2].0.checked_add(bss_start - data_start).unwrap_or(u64::MAX),
                    bss_end - bss_start).is_ok()
        };
        let high_guard_unmapped = unsafe {
            memory::inspect_mapping(info.pml4_physical, info.kernel_stack_virtual_guard).is_err()
        };
        if !stack_mapping_ok || !section_mappings_ok || !high_guard_unmapped {
            serial_write(b"FATAL: high-half kernel/stack translations or guard validation failed.\r\n");
            halt();
        }
        serial_write(b"Page-table inspection passed: high-half kernel sections, stack alias, low boot data, and guard pages validated.\r\n");

        // Only after the new identity map is active may physical pages be touched
        // through their physical addresses. The allocator uses conventional RAM.
        // Parse DMAR before the physical allocator publishes any free pages.
        // Every RMRR is globally reserved here, independent of which requester
        // will later consume its identity mapping.
        let mut rmrr_ranges = [(0u64, 0u64); 8];
        let mut rmrr_count = 0usize;
        if info.rsdp_address != 0 {
            match unsafe { acpi::find_table(info.rsdp_address, *b"DMAR", 0) } {
                Ok(dmar_bytes) => {
                    let dmar = match acpi::parse_dmar(dmar_bytes) {
                        Ok(parsed) => parsed,
                        Err(_) => {
                            serial_write(b"FATAL: malformed ACPI DMAR table; refusing unsafe memory initialization.\\r\\n");
                            halt();
                        }
                    };
                    for region in dmar.rmrr.regions.iter().take(dmar.rmrr.count) {
                        let end = match region.limit.checked_add(1) {
                            Some(value) => value,
                            None => {
                                serial_write(b"FATAL: invalid ACPI RMRR extent.\\r\\n");
                                halt();
                            }
                        };
                        if rmrr_count == rmrr_ranges.len() || region.base == 0
                            || region.base >= end || region.base & 0xfff != 0 || end & 0xfff != 0 {
                            serial_write(b"FATAL: unsupported ACPI RMRR reservation set.\\r\\n");
                            halt();
                        }
                        rmrr_ranges[rmrr_count] = (region.base, end);
                        rmrr_count += 1;
                    }
                }
                Err(_) => {}
            }
        }
        let region_count = match unsafe {
            memory::initialize_with_reserved_ranges(
                info.memory_map.cast::<u8>(),
                info.memory_map_entries,
                info.memory_descriptor_size,
                &rmrr_ranges[..rmrr_count],
            )
        } {
            Ok(count) => count,
            Err(_) => {
                serial_write(b"FATAL: invalid physical memory map or unsafe RMRR reservation.\\r\\n");
                halt();
            }
        };
        // The final UEFI map can still describe PE image backing as
        // conventional RAM on some firmware/QEMU combinations. Keep every
        // kernel section out of the physical allocators before any subsystem
        // can obtain a PFN for page tables, DMA, or ordinary data.
        if info.kernel_physical_section_count == 0
            || info.kernel_physical_section_count > info.kernel_physical_sections.len()
            || info.kernel_physical_sections[..info.kernel_physical_section_count]
                .iter().any(|(start, end)| *start == 0 || *start >= *end) {
            serial_write(b"FATAL: invalid PE section physical-backing metadata.\\r\\n");
            halt();
        }
        for &(start, end) in info.kernel_physical_sections[..info.kernel_physical_section_count].iter() {
            if unsafe { memory::reserve_range(start, end) }.is_err() {
                serial_write(b"FATAL: cannot reserve kernel image backing from physical allocator.\\r\\n");
                halt();
            }
        }

        // Discover NUMA memory proximity domains from firmware before any
        // driver allocation can request node-local pages. ACPI table parsing is
        // bounded to the validated firmware mapping; absence of SRAT remains a
        // supported non-NUMA/unknown-topology case rather than guessing node 0.
        if info.rsdp_address != 0 {
            match unsafe { acpi::find_table(info.rsdp_address, *b"SRAT", 0) } {
                Ok(srat) => match ntoskrnl::mm::topology::parse_srat(srat) {
                    Ok(ranges) => {
                        serial_write(b"ACPI SRAT memory-affinity ranges accepted: ");
                        serial_write_decimal(ranges);
                        serial_write(b"\r\n");
                        match unsafe { acpi::find_table(info.rsdp_address, *b"SLIT", 0) } {
                            Ok(slit) => match ntoskrnl::mm::topology::parse_slit(slit) {
                                Ok(nodes) => {
                                    serial_write(b"ACPI SLIT NUMA distance matrix accepted; nodes: ");
                                    serial_write_decimal(nodes);
                                    serial_write(b"\r\n");
                                }
                                Err(_) => serial_write(b"WARNING: ACPI SLIT rejected; NUMA distances unavailable.\r\n"),
                            },
                            Err(_) => serial_write(b"WARNING: ACPI SLIT unavailable; NUMA distances unavailable.\r\n"),
                        }
                    }
                    Err(_) => serial_write(b"WARNING: ACPI SRAT rejected; node-local allocation remains disabled.\r\n"),
                },
                Err(_) => {
                    // Windows treats a genuinely non-NUMA machine as one node
                    // (node 0). Only use that fallback when both SRAT and SLIT
                    // are absent; a SLIT without SRAT is inconsistent firmware
                    // data and must not be guessed into a topology.
                    if unsafe { acpi::find_table(info.rsdp_address, *b"SLIT", 0) }.is_ok() {
                        serial_write(b"WARNING: ACPI SLIT exists but SRAT is unavailable; NUMA ownership remains unknown.\r\n");
                    } else {
                        let mut ranges = [(0u64, 0u64); ntoskrnl::mm::topology::MAX_RANGES];
                        let count = memory::physical_memory_ranges(&mut ranges);
                        match ntoskrnl::mm::topology::initialize_single_node(&ranges[..count]) {
                            Ok(accepted) => {
                                serial_write(b"ACPI SRAT/SLIT absent; Windows-compatible UMA node 0 initialized, ranges: ");
                                serial_write_decimal(accepted);
                                serial_write(b"\r\n");
                            }
                            Err(_) => serial_write(b"ACPI SRAT unavailable and UMA fallback failed; memory node ownership remains unknown.\r\n"),
                        }
                    }
                },
            }
        }
        // Publish direct-map W^X policy before any high-RAM probe can fault in
        // a leaf. An uninitialized policy intentionally defaults to read-only.
        memory::set_direct_map_readonly_ranges([
            info.kernel_physical_sections[0],
            info.kernel_physical_sections.get(1).copied().unwrap_or((0, 0)),
        ]);
        serial_write(b"Kernel GDT and exception IDT installed (vectors 0-31).\r\n");
        // Keep the entire physical stack allocation out of the allocator even
        // if firmware's final map incorrectly reports any page as conventional.
        if unsafe {
            memory::reserve_range(
                info.kernel_stack_guard_page,
                info.kernel_stack_guard_page + stack_allocation_bytes,
            )
        }.is_err() {
            serial_write(b"FATAL: cannot reserve bootstrap stack from physical allocator.\r\n");
            halt();
        }
        if pagefile::initialize() {
            serial_write(b"Pagefile storage initialized on dedicated ATA primary-slave disk; slots: ");
            serial_write_decimal(pagefile::capacity_slots() as usize);
            serial_write(b"\r\n");
            if !pagefile::self_test() {
                serial_write(b"FATAL: pagefile storage read/write self-test failed.\r\n");
                halt();
            }
            serial_write(b"Pagefile storage read/write self-test passed.\r\n");
        } else {
            serial_write(b"WARNING: no dedicated pagefile disk; pageable eviction disabled.\r\n");
        }
        if kernel_vm::initialize(info.pml4_physical).is_err() {
            serial_write(b"FATAL: kernel virtual-memory manager initialization failed.\r\n");
            halt();
        }
        if !kernel_vm::self_test() {
            serial_write(b"FATAL: kernel VM self-test failed.\r\n");
            #[cfg(debug_assertions)]
            serial_write_kernel_vm_diagnostics();
            halt();
        }
        serial_write(b"Kernel virtual-address arena and mapping self-test passed.\r\n");
        if unsafe { syscall::prepare_windows_syscall_stack(0) }.is_err() {
            serial_write(b"FATAL: could not allocate BSP Windows SYSCALL stack.\r\n");
            halt();
        }
        serial_write(b"Windows SYSCALL per-CPU stack prepared.\r\n");
        if unsafe { syscall::initialize_windows_syscall_abi(0) }.is_err() {
            serial_write(b"FATAL: could not initialize the BSP Windows x64 SYSCALL ABI.\r\n");
            halt();
        }
        serial_write(b"Windows x64 SYSCALL ABI initialized on BSP.\r\n");

        if memory::first_high_conventional_page().is_some() {
            serial_write(b"High-RAM allocator and direct-map self-test passed.\r\n");
        } else {
            serial_write(b"High RAM direct-map test skipped: firmware exposes no conventional RAM above 4 GiB.\r\n");
        }
        if pagefile::is_active() {
            serial_write(b"Pagefile-backed pageable eviction and fault-driven page-in self-test passed.\r\n");
        }
        if !ntoskrnl::memory_manager_self_test() {
            serial_write(b"FATAL: NT memory-manager translation/copy self-test failed.\r\n");
            halt();
        }
        serial_write(b"NT memory manager: physical translation, validity, and checked-copy self-test passed.\r\n");
        serial_write(b"Physical allocator initialized; conventional regions: ");
        serial_write_decimal(region_count);
        serial_write(b"\r\n");

        // Validate the separate HAL before using its synchronization primitives.
        if !hal::self_test() {
            serial_write(b"FATAL: HAL self-test failed.\r\n");
            halt();
        }
        serial_write(b"HAL CPU-local registry, MMIO bounds, and x86-64 primitives initialized.\r\n");

        if !smp::self_test() {
            serial_write(b"FATAL: SMP startup-target validation self-test failed.\r\n");
            halt();
        }
        serial_write(b"SMP startup target validation passed.\r\n");

        // Validate the bounded ACPI parser against a minimal synthetic MADT.
        let mut madt = [0u8; 44];
        madt[0..4].copy_from_slice(b"APIC");
        madt[4..8].copy_from_slice(&(44u32).to_le_bytes());
        madt[36..40].copy_from_slice(&(0xfee0_0000u32).to_le_bytes());
        madt[40..44].copy_from_slice(&(1u32).to_le_bytes());
        let sum = madt.iter().fold(0u8, |acc, byte| acc.wrapping_add(*byte));
        madt[9] = 0u8.wrapping_sub(sum);
        if !matches!(acpi::parse_madt(&madt), Ok(info) if info.local_apic_address == 0xfee0_0000 && info.flags == 1) {
            serial_write(b"FATAL: ACPI MADT parser self-test failed.\r\n");
            halt();
        }
        serial_write(b"ACPI MADT parser self-test passed; APIC discovery primitives available.\r\n");

        let mut local_timer_ready = false;
        if info.rsdp_address != 0 {
            // Publish the firmware RSDP to the HAL's table service before
            // ACPI.sys imports HalAcpiGetTableEx during DriverEntry.
            let firmware_madt = acpi::discover_madt(info.rsdp_address);
            // ACPI.sys validates firmware tables as a native boot-start
            // driver before PCI.sys receives the MCFG-derived ECAM windows.
            let acpi_driver_ready=initialize_acpi_driver(info.rsdp_address);
            let _pnp_enumerated=if acpi_driver_ready {
                // The SYSTEM hive has been validated by the registry-backed
                // ACPI boot-driver load. Publish milestones only after their
                // corresponding initialization operation has succeeded.
                if BOOT_COORDINATOR.advance(boot_orchestration::BootStage::Registry).is_err() {
                    halt();
                }
                let enumerated = enumerate_acpi_pci_devices(info.rsdp_address);
                if matches!(boot_orchestration::validate_device_tree(), Ok(count) if count != 0) {
                    if BOOT_COORDINATOR.advance(boot_orchestration::BootStage::BootDrivers).is_err()
                        || BOOT_COORDINATOR.advance(boot_orchestration::BootStage::DeviceTree).is_err() {
                        halt();
                    }
                }
                enumerated
            } else { 0 };
            match firmware_madt {
                Ok(firmware_madt) => {
                    let _ = platform_drivers::mark_ready(platform_drivers::Stage::Acpi);
                    let discovery = (|| -> Result<usize, hal::HalError> {
                        if hal::cpu_index_for_apic(bsp_apic_id).is_none() { hal::initialize_bsp(bsp_apic_id)?; }
                        let mut discovered = 0usize;
                        for entry in firmware_madt.cpus.iter().take(firmware_madt.cpu_count) {
                            if !entry.enabled || entry.apic_id == bsp_apic_id { continue; }
                            if hal::cpu_index_for_apic(entry.apic_id).is_some() { continue; }
                            hal::discover_cpu(entry.apic_id)?;
                            discovered += 1;
                        }
                        Ok(discovered)
                    })();
                    match discovery {
                        Ok(discovered) => {
                            serial_write(b"Firmware ACPI MADT validated; enabled APs discovered: ");
                            serial_write_decimal(discovered);
                            serial_write(b" (not started).\r\n");
                            serial_write(b"Firmware MADT Local APIC address: ");
                            serial_write_hex(firmware_madt.local_apic_address);
                            serial_write(b"\r\n");

                            // The bootloader identity-maps 0..4 GiB with 2 MiB pages.
                            // Keep legacy and I/O APIC routes masked until the local timer is ready.
                            match (if firmware_madt.local_apic_address == 0xfee0_0000
                                && firmware_madt.io_apics.iter().take(firmware_madt.io_apic_count)
                                    .all(|io| (io.address as u64) >= 0xfec0_0000 && (io.address as u64) < 0xfee0_0000)
                            {
                                // The bootloader maps these standard MMIO windows UC.
                                unsafe { hal::initialize_local_interrupt_controller(firmware_madt.local_apic_address) }
                            } else {
                                Err(hal::HalError::InvalidInterruptBase)
                            }) {
                                Ok(local_id) if local_id == bsp_apic_id => {
                                    let mut routes_masked = true;
                                    for io in firmware_madt.io_apics.iter().take(firmware_madt.io_apic_count) {
                                        if unsafe { hal::mask_interrupt_controller_routes(io.address as u64) }.is_err() {
                                            routes_masked = false;
                                            break;
                                        }
                                    }
                                    if routes_masked {
                                        unsafe { outb(0x21, 0xff); outb(0xa1, 0xff); }
                                        let _=prepare_ps2_irq_route(&firmware_madt,bsp_apic_id);
                                        hal::install_device_dpc_hooks(
                                            ntoskrnl::ke::runtime::hal_queue_device_dpc,
                                            ntoskrnl::ke::runtime::hal_remove_device_dpc,
                                            ntoskrnl::ke::runtime::hal_initialize_device_dpc,
                                        );
    hal::install_device_bus_master_hook(xhci_set_bus_master);
                                        let _xhci_pnp_resources_ready=probe_xhci_pci_function(&firmware_madt,bsp_apic_id);
                                        match unsafe { hal::calibrate_local_clock_event() } {
                                            Ok(count) => {
                                                local_timer_ready = true;
                                                serial_write(b"Local APIC timer calibrated (still masked); 1 ms count=");
                                                serial_write_decimal(count as usize);
                                                serial_write(b"; legacy and I/O APIC routes masked.\r\n");
                                            }
                                            Err(_) => serial_write(b"WARNING: APIC timer calibration failed; interrupts stay disabled.\r\n"),
                                        }
                                    } else {
                                        serial_write(b"WARNING: could not mask every I/O APIC route; interrupts stay disabled.\r\n");
                                    }
                                }
                                _ => serial_write(b"WARNING: local APIC initialization failed or APIC ID mismatch; interrupts stay disabled.\r\n"),
                            }
                        }
                        Err(_) => {
                            serial_write(b"WARNING: MADT parsed, but HAL CPU registration failed; SMP remains disabled.\r\n");
                        }
                    }
                }
                Err(_) => {
                    serial_write(b"WARNING: firmware ACPI RSDP/root/MADT validation failed; SMP remains disabled.\r\n");
                }
            }
        } else {
            serial_write(b"WARNING: no firmware ACPI RSDP handoff; SMP discovery skipped.\r\n");
        }

        // Exercise scheduler metadata/priority/affinity without switching stacks.
        scheduler::initialize();
        let low = match scheduler::create_task(1, 1, 0x7000, 0x1000) {
            Ok(id) => id, Err(_) => { serial_write(b"FATAL: scheduler task creation failed.\r\n"); halt(); }
        };
        let high = match scheduler::create_task(6, 1, 0x9000, 0x2000) {
            Ok(id) => id, Err(_) => { serial_write(b"FATAL: scheduler task creation failed.\r\n"); halt(); }
        };
        if !matches!(scheduler::choose_next(0), Ok(Some(task)) if task.priority == 6 && task.id == high)
            || scheduler::yield_current(0).is_err()
            || scheduler::set_affinity(low, 0).is_ok()
            || scheduler::exit_task(low).is_err()
            || scheduler::exit_task(high).is_err()
        {
            serial_write(b"FATAL: scheduler priority/affinity self-test failed.\r\n");
            halt();
        }
        // The metadata fixture must not leave synthetic READY tasks in the
        // production scheduler: their fake IDs have no ThreadRecord/context,
        // and a later dispatch would select them ahead of real kernel threads.
        serial_write(b"Scheduler priority, affinity, cooperative-yield, and fixture cleanup self-test passed.\r\n");

        // Exercise a real saved-stack transition, thread trampoline, exit, and
        // return to the bootstrap context before enabling timer interrupts.
        let smoke_stack_ptr = core::ptr::addr_of_mut!(THREAD_SMOKE_STACK).cast::<u8>();
        let smoke_stack: &'static mut [u8] = core::slice::from_raw_parts_mut(smoke_stack_ptr, 16384);
        let smoke_thread = match thread::spawn(smoke_stack, 1, thread_smoke_entry, core::ptr::null_mut()) {
            Ok(id) => id,
            Err(_) => { serial_write(b"FATAL: kernel thread context creation failed.\r\n"); halt(); }
        };
        let smoke_switch = thread::yield_current();
        let smoke_ran = THREAD_SMOKE_RAN.load(core::sync::atomic::Ordering::Acquire);
        let smoke_state = thread::state(smoke_thread);
        if smoke_switch.is_err() || !smoke_ran || smoke_state != Some(thread::ThreadState::Exited) {
            serial_write(b"FATAL: x86-64 context-switch/thread-exit self-test failed; switch=");
            serial_write_decimal(smoke_switch.is_ok() as usize);
            serial_write(b" error=");
            serial_write_decimal(match smoke_switch { Ok(()) => 0, Err(thread::ThreadError::InvalidStack) => 1, Err(thread::ThreadError::InvalidAffinity) => 2, Err(thread::ThreadError::Full) => 3, Err(thread::ThreadError::NoCurrentThread) => 4, Err(thread::ThreadError::InvalidState) => 5, Err(thread::ThreadError::InvalidCpu) => 6 });
            serial_write(b" ran=");
            serial_write_decimal(smoke_ran as usize);
            serial_write(b" state=");
            serial_write_decimal(match smoke_state { Some(thread::ThreadState::Empty) => 0, Some(thread::ThreadState::Ready) => 1, Some(thread::ThreadState::Running) => 2, Some(thread::ThreadState::Blocked) => 3, Some(thread::ThreadState::Exited) => 4, None => 5 });
            serial_write(b"\r\n");
            halt();
        }
        serial_write(b"Kernel thread stack switch, trampoline, exit, and bootstrap resume passed.\r\n");

        let block_stack_ptr = core::ptr::addr_of_mut!(THREAD_BLOCK_STACK).cast::<u8>();
        let wake_stack_ptr = core::ptr::addr_of_mut!(THREAD_WAKE_STACK).cast::<u8>();
        let block_stack: &'static mut [u8] = core::slice::from_raw_parts_mut(block_stack_ptr, 16384);
        let wake_stack: &'static mut [u8] = core::slice::from_raw_parts_mut(wake_stack_ptr, 16384);
        THREAD_BLOCK_STAGE.store(0, core::sync::atomic::Ordering::Release);
        let blocked_thread = match thread::spawn(block_stack, 1, thread_block_smoke_entry, core::ptr::null_mut()) {
            Ok(id) => id,
            Err(_) => { serial_write(b"FATAL: blocked-thread fixture creation failed.\r\n"); halt(); }
        };
        let wake_thread = match thread::spawn(wake_stack, 1, thread_wake_smoke_entry, blocked_thread as *mut core::ffi::c_void) {
            Ok(id) => id,
            Err(_) => { serial_write(b"FATAL: wake-thread fixture creation failed.\r\n"); halt(); }
        };
        if thread::yield_current().is_err()
            || THREAD_BLOCK_STAGE.load(core::sync::atomic::Ordering::Acquire) != 3
            || thread::state(blocked_thread) != Some(thread::ThreadState::Exited)
            || thread::state(wake_thread) != Some(thread::ThreadState::Exited)
        {
            serial_write(b"FATAL: kernel thread block/wake/resume self-test failed.\r\n");
            halt();
        }
        serial_write(b"Kernel thread block, cross-thread wake, resume, and exit passed.\r\n");
        if !dispatcher::self_test() {
            serial_write(b"FATAL: dispatcher FIFO, cancellation, wait-group, or timeout self-test failed.\r\n");
            halt();
        }
        serial_write(b"Dispatcher FIFO, cancellation, wait-any/wait-all, and timeout self-tests passed.\r\n");

        // Exercise typed handles and explicit access checks.
        let event = match object::create(object::ObjectType::Event, object::ACCESS_READ) {
            Ok(h) => h, Err(_) => { serial_write(b"FATAL: object handle creation failed.\r\n"); halt(); }
        };
        if !matches!(object::lookup(event.handle, object::ACCESS_READ), Ok(h) if h.object_id == event.object_id)
            || !matches!(object::lookup(event.handle, object::ACCESS_WRITE), Err(object::ObjectError::AccessDenied))
            || object::name_object(b"\\BaseNamedObjects\\SmokeEvent", event.object_id, object::ObjectType::Event, object::ACCESS_READ).is_err()
            || !matches!(object::open_named(b"\\BaseNamedObjects\\SmokeEvent", object::ACCESS_READ), Ok(h) if h.object_id == event.object_id)
            || object::close(event.handle).is_err()
        {
            serial_write(b"FATAL: object manager handle/access self-test failed.\r\n");
            halt();
        }
        serial_write(b"Object handles, named lookup, and access checks self-test passed.\r\n");

        // Exercise the canonical WDM IRP/device implementation.
        if !wdm::rundown_self_test() {
            serial_write(b"FATAL: WDM exactly-once completion or outstanding-IRP unload rundown self-test failed.\r\n");
            halt();
        }
        serial_write(b"WDM exactly-once completion and outstanding-IRP unload rundown self-test passed.\r\n");
        // Read-only native Rust ext filesystem parser self-test.
        if !extfs::self_test() {
            serial_write(b"FATAL: ext filesystem parser self-test failed.\r\n");
            halt();
        }
        serial_write(b"Ext filesystem parser self-test passed (read-only ext2/ext3/ext4 metadata; unsupported features rejected).\r\n");

        // Exercise the actual QEMU IDE boot disk, not a synthetic parser fixture.
        // Failure is fatal so CI cannot confuse host-side debugfs checks with a
        // successful guest storage/filesystem read.
        if !ata_disk::run_ext4_readme_test() {
            serial_write(b"FATAL: guest ATA/GPT/ext4 read/write end-to-end test failed.\r\n");
            halt();
        }
        let _ = platform_drivers::mark_ready(platform_drivers::Stage::Storage);
        let _ = platform_drivers::mark_ready(platform_drivers::Stage::ExtFilesystem);
        serial_write(b"Guest ATA/GPT/ext4 read/write end-to-end test passed.\r\n");

        // APIC registers are intentionally not enabled here: no firmware MADT
        // handoff, AP startup trampoline, timer calibration, or complete IRQ
        // routing is installed yet. Keep IF clear until those prerequisites exist.

        // Activate processor-enforced baseline mitigations only after the
        // kernel-owned page tables are live. NXE prepares the CPU to honor NX
        // PTE bits; strict per-page NX/W^X mappings are a subsequent step.
        let mitigations = ntoskrnl::ke::mitigations::initialize();
        serial_write(b"Security: CR0.WP ");
        serial_write(if mitigations.write_protect { b"enabled\r\n" } else { b"FAILED\r\n" });
        serial_write(b"Security: NX ");
        serial_write(if mitigations.nx_enabled { b"enabled; strict page policy follows\r\n" } else if mitigations.nx_supported { b"supported but enable failed\r\n" } else { b"unsupported\r\n" });
        serial_write(b"Security: SMEP ");
        serial_write(if mitigations.smep_enabled { b"enabled\r\n" } else if mitigations.smep_supported { b"supported but enable failed\r\n" } else { b"unsupported\r\n" });
        serial_write(b"Security: SMAP ");
        serial_write(if mitigations.smap_supported && !mitigations.smap_enabled { b"supported; explicitly disabled until guarded user-copy STAC/CLAC paths are complete\r\n" } else if mitigations.smap_enabled { b"FATAL: SMAP unexpectedly enabled before guarded user-copy audit\r\n" } else { b"unsupported and disabled\r\n" });
        if mitigations.smap_enabled { serial_write(b"FATAL: refusing to continue with unaudited SMAP state.\r\n"); halt(); }

        if !mitigations.nx_enabled {
            serial_write(b"FATAL: NX unavailable; refusing to enable strict page policy.\r\n");
            halt();
        }
        let text = (core::ptr::addr_of!(__text_start) as u64, core::ptr::addr_of!(__text_end) as u64);
        let rodata = (core::ptr::addr_of!(__rodata_start) as u64, core::ptr::addr_of!(__rodata_end) as u64);
        let data = (core::ptr::addr_of!(__data_start) as u64, core::ptr::addr_of!(__data_end) as u64);
        let bss = (core::ptr::addr_of!(__bss_start) as u64, core::ptr::addr_of!(__bss_end) as u64);
        let stack_virtual = (
            info.kernel_stack_virtual_guard + 4096,
            info.kernel_stack_virtual_guard + stack_allocation_bytes,
        );
        let physical_sections = info.kernel_physical_sections;
        if unsafe { memory::harden_page_tables(
            info.pml4_physical, text, rodata, data, bss,
            info.kernel_stack_guard_page, stack_virtual, info.kernel_stack_virtual_guard,
            physical_sections,
        ) }.is_err() {
            serial_write(b"FATAL: kernel page-permission hardening failed.\r\n");
            halt();
        }
        serial_write(b"Security: 4 KiB page policy active; W^X enforced for kernel sections.\r\n");
        serial_write(b"Security: kernel stack guard page unmapped.\r\n");
        if !ntoskrnl::ke::arch::permission_fault_self_test(_start as usize as u64) {
            serial_write(b"FATAL: write-protect or NX permission-fault self-test failed.\r\n");
            halt();
        }
        serial_write(b"Protection self-tests passed: read-only write trapped and NX execution trapped.\r\n");
        if !ntoskrnl::ke::arch::trap_frame_fixup_self_test() {
            serial_write(b"FATAL: invalid kernel probe was not recovered through its trap-frame fixup.\r\n");
            halt();
        }
        serial_write(b"Trap-frame fixup self-test passed: invalid kernel probe returned STATUS_ACCESS_VIOLATION.\r\n");
        // The NT executive owns processor-start sequencing; the platform layer
        // supplies firmware handoff data and reports the resulting status.
        let processor_startup = ntoskrnl::ke::runtime::ki_start_all_processors(
            info.pml4_physical,
            info.ap_trampoline_physical,
            info.rsdp_address,
            bsp_apic_id,
        );
        if processor_startup.startup_ok {
            serial_write(b"SMP AP startup complete; online application processors: ");
            serial_write_decimal(processor_startup.online_aps);
            serial_write(if processor_startup.ipi_enabled {
                b"; interrupt-driven TLB shootdowns enabled.\r\n"
            } else {
                b"; using polling TLB fallback (IPI setup unavailable).\r\n"
            });
            serial_write(b"SMP startup diagnostic error code: ");
            serial_write_decimal(smp::last_start_error_code());
            serial_write(b"; last target APIC ID: ");
            serial_write_decimal(smp::last_target_apic_id());
            serial_write(b"; AP entry functions reached: ");
            serial_write_decimal(smp::ap_entry_reached_count());
            serial_write(b"\r\n");
            if processor_startup.ipi_enabled {
                if !processor_startup.interrupt_stress_passed {
                    serial_write(b"FATAL: per-CPU IDT/TSS/LAPIC IPI stress test failed.\r\n");
                    halt();
                }
                serial_write(b"Per-CPU interrupt setup stress passed: 64 acknowledged IPIs per online AP.\r\n");
                if !ntoskrnl::ke::runtime::broadcast_rendezvous_self_test() {
                    serial_write(b"FATAL: KeIpiGenericCall/KeGenericCallDpc rendezvous or barrier self-test failed.\r\n");
                    halt();
                }
                serial_write(b"Ke broadcast rendezvous and DPC synchronization/completion barriers passed on every online CPU.\r\n");
            } else {
                serial_write(b"WARNING: per-CPU IPI stress skipped because interrupt-driven IPIs are unavailable.\r\n");
            }
            serial_write(if processor_startup.tsc_synced {
                b"TSC cross-core synchronization passed; max offset magnitude="
            } else {
                b"WARNING: TSC sync incomplete; global monotonic clamp remains active; max offset magnitude="
            });
            serial_write_decimal(processor_startup.worst_tsc_offset as usize);
            serial_write(b" ticks.\r\n");
        } else {
            serial_write(b"WARNING: executive processor startup unavailable; retaining BSP-only operation.\r\n");
        }
        if !processor_startup.scheduler_stress_passed {
            serial_write(b"FATAL: SMP scheduler, lock contention, or IRP/object lifecycle stress failed.\r\n");
            halt();
        }
        serial_write(b"SMP scheduler, cross-CPU wakeup, contended-lock, IRP/object teardown stress passed.\r\n");
        if !processor_startup.vm_stress_passed {
            serial_write(b"FATAL: SMP kernel VM/TLB shootdown self-test failed.\r\n");
            #[cfg(debug_assertions)]
            serial_write_kernel_vm_diagnostics();
            halt();
        }
        serial_write(b"Kernel VM extent stress and SMP TLB shootdown self-test passed.\r\n");
        if !ntoskrnl::ke::runtime::ki_initialize_kernel(&processor_startup) {
            serial_write(b"FATAL: KiInitializeKernel executive validation failed.\r\n");
            halt();
        }
        serial_write(b"KiInitializeKernel startup gate passed.\r\n");

        // Exercise an independent user address space without activating ring 3.
        let mut address_space = match unsafe { ntoskrnl::mm::address_space::AddressSpace::new(info.pml4_physical) } {
            Ok(space) => space,
            Err(_) => { serial_write(b"FATAL: address-space root allocation failed.\r\n"); halt(); }
        };
        let user_page = match unsafe { memory::allocate_page() } {
            Ok(page) => page,
            Err(_) => { serial_write(b"FATAL: user test page allocation failed.\r\n"); halt(); }
        };
        const TEST_USER_ADDRESS: u64 = 0x0000_0080_0000_0000;
        if unsafe { address_space.map_user_page(TEST_USER_ADDRESS, user_page, true, false) }.is_err() {
            serial_write(b"FATAL: user mapping setup failed.\r\n");
            halt();
        }
        let sample = *b"CognacOS user-copy check";
        if unsafe { address_space.copy_to_user(TEST_USER_ADDRESS, &sample) }.is_err() {
            serial_write(b"FATAL: checked copy-to-user failed.\r\n");
            halt();
        }
        let mut roundtrip = [0u8; 24];
        if unsafe { address_space.copy_from_user(TEST_USER_ADDRESS, &mut roundtrip) }.is_err()
            || roundtrip != sample
        {
            serial_write(b"FATAL: checked user-copy validation failed.\r\n");
            halt();
        }

        // Adversarial isolation test: two independently rooted process page
        // tables map the same user VA to different frames. Neither mapping may
        // overwrite or alias the other process, and user translation must never
        // expose the shared supervisor-only kernel image.
        let mut second_address_space = match unsafe { ntoskrnl::mm::address_space::AddressSpace::new(info.pml4_physical) } {
            Ok(space) => space,
            Err(_) => { serial_write(b"FATAL: second isolated address-space allocation failed.\r\n"); halt(); }
        };
        let second_user_page = match unsafe { memory::allocate_page() } {
            Ok(page) => page,
            Err(_) => { serial_write(b"FATAL: second process user page allocation failed.\r\n"); halt(); }
        };
        if unsafe { second_address_space.map_user_page(TEST_USER_ADDRESS, second_user_page, true, false) }.is_err() {
            serial_write(b"FATAL: second process same-VA mapping failed.\r\n"); halt();
        }
        let other_sample = *b"Separate process memory!";
        unsafe { core::ptr::copy_nonoverlapping(other_sample.as_ptr(), second_user_page as *mut u8, other_sample.len()); }
        let first_pa = match address_space.translate_user(TEST_USER_ADDRESS, false) {
            Ok(pa) => pa, Err(_) => { serial_write(b"FATAL: first process user translation failed.\r\n"); halt(); }
        };
        let second_pa = match second_address_space.translate_user(TEST_USER_ADDRESS, false) {
            Ok(pa) => pa, Err(_) => { serial_write(b"FATAL: second process user translation failed.\r\n"); halt(); }
        };
        if first_pa == second_pa {
            serial_write(b"FATAL: cross-process user VA unexpectedly aliases the same physical frame.\r\n"); halt();
        }
        let kernel_probe = ntoskrnl::ke::arch::initialize as usize as u64;
        {
            let _guard = ntoskrnl::mm::address_space::ADDRESS_SPACE_LIFETIME_LOCK.lock();
            if ntoskrnl::mm::address_space::translate_in_root_locked(second_address_space.root_physical(), kernel_probe, false, false).is_none()
                || ntoskrnl::mm::address_space::translate_in_root_locked(second_address_space.root_physical(), kernel_probe, false, true).is_some()
            {
                serial_write(b"FATAL: supervisor kernel mapping is not isolated from CPL3 translation.\r\n"); halt();
            }
        }
        let mut second_roundtrip = [0u8; 24];
        if unsafe { second_address_space.copy_from_user(TEST_USER_ADDRESS, &mut second_roundtrip) }.is_err()
            || second_roundtrip != other_sample
            || unsafe { address_space.copy_from_user(TEST_USER_ADDRESS, &mut roundtrip) }.is_err()
            || roundtrip != sample
        {
            serial_write(b"FATAL: cross-process mapping isolation regression.\r\n"); halt();
        }
        if unsafe { second_address_space.destroy() }.is_err() {
            serial_write(b"FATAL: second process address-space teardown failed.\r\n"); halt();
        }
        serial_write(b"Per-process page-table isolation, same-VA non-aliasing, and supervisor mapping protection passed.\r\n");
        serial_write(b"Process address-space and checked user-copy self-test passed.\r\n");
        if !syscall::native_nt_dispatch_self_test(&address_space, TEST_USER_ADDRESS) {
            serial_write(b"FATAL: native NT syscall dispatcher self-test failed.\r\n"); halt();
        }
        serial_write(b"Native NT syscall dispatcher QEMU self-test passed: five args, invalid buffers, unsupported IDs, and 17-arg stack copy.\r\n");
        // Keep the bootstrap execution context represented by a real
        // scheduler task before any user-mode image or driver smoke test runs.
        let bootstrap_thread_id = match thread::current_thread_id() {
            Some(id) => id,
            None => match thread::adopt_current_kernel_thread(bsp_stack_top) {
                Ok(id) => id,
                Err(_) => {
                    serial_write(b"FATAL: could not attach bootstrap execution to a scheduler-owned ETHREAD.\r\n");
                    halt();
                }
            },
        };
        serial_write(b"Scheduler-owned bootstrap ETHREAD ready id=");
        serial_write_hex(bootstrap_thread_id);
        serial_write(b"; user faults and driver requests now have a current thread.\r\n");
        if let Some(ntdll_bytes) = boot_ntdll_bytes() {
            serial_write(b"Attempting to map staged ntdll.dll in an isolated user address space.\r\n");
            let mut ntdll_space = match unsafe { ntoskrnl::mm::address_space::AddressSpace::new(info.pml4_physical) } {
                Ok(space) => space,
                Err(_) => { serial_write(b"FATAL: NTDLL test address-space allocation failed.\r\n"); halt(); }
            };
            let ntdll_image = match unsafe { pe::load_dll(&mut ntdll_space, ntdll_bytes, b"ntdll.dll", None) } {
                Ok(image) => image,
                Err(error) => {
                    let label: &[u8] = match error {
                        pe::PeError::Truncated => b"Truncated",
                        pe::PeError::BadDosHeader => b"BadDosHeader",
                        pe::PeError::BadSignature => b"BadSignature",
                        pe::PeError::UnsupportedMachine => b"UnsupportedMachine",
                        pe::PeError::UnsupportedOptionalHeader => b"UnsupportedOptionalHeader",
                        pe::PeError::InvalidHeader => b"InvalidHeader",
                        pe::PeError::InvalidSection => b"InvalidSection",
                        pe::PeError::ImageTooLarge => b"ImageTooLarge",
                        pe::PeError::AddressUnavailable => b"AddressUnavailable",
                        pe::PeError::OutOfMemory => b"OutOfMemory",
                        pe::PeError::InvalidRelocations => b"InvalidRelocations",
                        pe::PeError::RelocationsRequired => b"RelocationsRequired",
                        pe::PeError::InvalidImports => b"InvalidImports",
                        pe::PeError::ImportResolverMissing => b"ImportResolverMissing",
                        pe::PeError::UnresolvedImport => b"UnresolvedImport",
                        pe::PeError::WxPageRejected => b"WxPageRejected",
                        pe::PeError::Vm(_) => b"VmError",
                    };
                    serial_write(b"FATAL: NTDLL load failed: ");
                    serial_write(label);
                    serial_write(b"\r\n");
                    halt();
                }
            };
            serial_write(b"NTDLL PE32+ image mapped and exports registered.\r\n");
            if !syscall::configure_windows_service_table_from_image(ntdll_bytes) {
                serial_write(b"FATAL: Rust NTDLL is missing canonical Windows x64 SYSCALL exports.\r\n");
                halt();
            }
            match unsafe { usermode::run_native_ntdll_smoke(&mut ntdll_space, ntdll_image) } {
                Ok(()) => serial_write(b"\r\nCognacOS Rust NTDLL passed ring-3 tests through the unified Windows x64 SYSCALL ABI.\r\n"),
                Err(_) => {
                    // A user-mode exception is a process failure, not a kernel
                    // boot failure. Keep later scheduler, storage and driver
                    // smoke tests running after the faulting process is reaped.
                    serial_write(b"WARNING: CognacOS Rust NTDLL ring-3 smoke did not complete; any user fault was contained to its process. Continuing boot and downstream tests.\r\n");
                }
            }
            syscall::set_windows_service_table_enabled(false);
            // run_loaded_image's cleanup path already destroyed this address space.
        } else {
            serial_write(b"WARNING: no staged ntdll.dll; PE loader smoke remains fixture-only.\r\n");
        }
        serial_write(b"Security: ring-3 GDT/TSS RSP0 installed; IRETQ transition available.\r\n");
        serial_write(b"Security: ring-3 GDT/TSS RSP0 installed; IRETQ transition available.\r\n");
        let loaded_pe = match unsafe { pe::self_test(&mut address_space) } {
            Ok(image) => image,
            Err(_) => { serial_write(b"FATAL: PE32+ loader self-test failed.\r\n"); halt(); }
        };
        serial_write(b"PE32+ loader self-test passed; image base=");
        serial_write_hex(loaded_pe.base);
        serial_write(b" entry=");
        serial_write_hex(loaded_pe.entry);
        serial_write(b" (RX section and RO/NX headers).\r\n");
        let mut tls_callbacks = [0u64; 64];
        let tls_count = match pe_runtime::tls_callbacks(loaded_pe.base, &mut tls_callbacks) {
            Ok(count) => count,
            Err(_) => { serial_write(b"FATAL: PE TLS callback registration failed.\r\n"); halt(); }
        };
        if tls_count != 1 || pe_runtime::lookup_function(loaded_pe.base, loaded_pe.base + 0x1100).is_none() {
            serial_write(b"FATAL: PE TLS or unwind registration self-test failed.\r\n");
            halt();
        }
        if !matches!(winabi::api_set_host(b"api-ms-win-core-file-l1-1-0"), Some(host) if host == b"kernel32.dll") {
            serial_write(b"FATAL: API-set contract resolution self-test failed.\r\n");
            halt();
        }
        serial_write(b"Windows API-set resolution and PE TLS/unwind registration passed.\r\n");
        if unsafe { usermode::run_loaded_image(&mut address_space, loaded_pe) }.is_err() {
            serial_write(b"FATAL: ring-3 syscall/process lifecycle self-test failed.\r\n");
            halt();
        }
        serial_write(b"Ring-3 syscall dispatch, process exit, and PEB/TEB setup passed.\r\n");
        // beep.sys request testing is deferred until a scheduler-owned kernel thread exists.
        serial_write(b"Runtime beep.sys test deferred until scheduler thread context is available.\r\n");


        serial_write(b"Kernel-owned identity map active for 0..4 GiB.\r\n");
        serial_write(b"Physical page allocator online (single-core bootstrap mode).\r\n");
        if local_timer_ready {
            if !thread::initialize_threaded_dpc_workers() {
                serial_write(b"WARNING: threaded DPC worker initialization incomplete.\r\n");
            }
            if crate::ntoskrnl::ke::runtime::register_clock_interrupt_handler().is_err() {
                serial_write(b"WARNING: NT clock callback registration failed; local timer remains disabled.\r\n");
            } else {
            match unsafe { hal::start_local_clock_event() } {
                Ok(()) => {
                    // Keep timer interrupts enabled, but do not globally enable
                    // asynchronous context switching until the timer-preemption
                    // stress test has proved that its saved kernel contexts are
                    // resumable. Previously enabling preemption first allowed a
                    // bad saved RIP to turn this diagnostic into a supervisor
                    // instruction-fetch fault at address zero.
                    thread::set_preemption_enabled(false);
                    if thread::timer_preemption_self_test() {
                        thread::set_preemption_enabled(true);
                        serial_write(b"Timer preemption stress passed: non-yielding thread was preempted for a runnable peer.\r\n");
                    } else {
                        thread::set_preemption_enabled(false);
                        serial_write(b"Timer preemption stress FAILED; asynchronous preemption remains disabled for kernel safety.\r\n");
                    }
                    let beep_stack = unsafe { core::slice::from_raw_parts_mut(core::ptr::addr_of_mut!(BEEP_TEST_STACK).cast::<u8>(), 8192) };
                    match thread::spawn(beep_stack, 1, beep_test_worker, core::ptr::null_mut()) {
                        Ok(_) => serial_write(b"Scheduled beep.sys test worker.\r\n"),
                        Err(_) => serial_write(b"Failed to schedule beep.sys test worker.\r\n"),
                    }
                    serial_write(b"Local APIC timers enabled; BSP timer preemption validated; AP timer preemption remains disabled pending per-CPU self-tests.\r\n");
                    // Only writable mounts with a durable backing callback and reserved
                    // snapshot slots can be promoted; read-only boot assets are skipped.
                    let _ = unsafe { ntoskrnl::cm::runtime::mark_hives_boot_successful() };
                    serial_write(b"cognacos-kd> type help for debugger commands\r\n");
                    loop {
                        crate::debug_console::poll();
                        poll_ps2_keyboard_input();
                        unsafe { asm!("sti; hlt", options(nomem, nostack)); }
                        poll_ps2_keyboard_input();
                        ntoskrnl::ke::runtime::dispatch_threaded_dpcs();
                        if thread::has_ready_threads() { let _ = thread::yield_current(); }
                    }
                }
                Err(_) => serial_write(b"WARNING: calibrated APIC timer could not start; interrupts remain disabled.\r\n"),
            }
            }
        } else {
            serial_write(b"Interrupts remain disabled because APIC/timer setup was not fully validated.\r\n");
        }
    }
    unsafe { halt() }
}

unsafe fn serial_init() {
    outb(0x3f9, 0x00);
    outb(0x3fb, 0x80);
    outb(0x3f8, 0x03);
    outb(0x3f9, 0x00);
    outb(0x3fb, 0x03);
    outb(0x3fa, 0xC7);
    outb(0x3fc, 0x0B);
}

unsafe fn outb(port: u16, value: u8) {
    unsafe { asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags)); }
}


static mut BEEP_TEST_STACK: [u8; 8192] = [0; 8192];

extern "C" fn beep_test_worker(_argument: *mut core::ffi::c_void) {
    unsafe { serial_write(b"Runtime beep.sys test started on scheduler-owned kernel thread.\r\n"); }
    run_beep_runtime_test();
}

fn run_beep_runtime_test() {
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

pub fn serial_write(bytes: &[u8]) {
    for byte in bytes {
        unsafe {
            let mut spins = 0u32;
            while spins < 100_000 {
                let status: u8;
                asm!("in al, dx", in("dx") (0x3f8 + 5u16), out("al") status, options(nomem, nostack, preserves_flags));
                if status & 0x20 != 0 { break; }
                spins += 1;
            }
            outb(0x3f8, *byte);
        }
    }
}

pub fn serial_write_hex(value: u64) {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = [0u8; 18];
    out[0] = b'0';
    out[1] = b'x';
    for i in 0..16 {
        out[2 + i] = DIGITS[((value >> ((15 - i) * 4)) & 0xf) as usize];
    }
    serial_write(&out);
}

fn serial_write_decimal(mut value: usize) {
    let mut digits = [0u8; 20];
    let mut index = digits.len();
    if value == 0 {
        serial_write(b"0");
        return;
    }
    while value != 0 && index > 0 {
        index -= 1;
        digits[index] = b'0' + (value % 10) as u8;
        value /= 10;
    }
    serial_write(&digits[index..]);
}

#[cfg(debug_assertions)]
fn serial_write_kernel_vm_diagnostics() {
    let d = kernel_vm::self_test_diagnostics();
    serial_write(b"DEBUG VM: phase=");
    unsafe { serial_write_decimal(d[0] as usize); }
    serial_write(b" (1=direct-map,2=high-RAM,3=extent,4=protection,5=demand-zero,6=pagefile,7=final-reclaim) source-line=");
    unsafe { serial_write_decimal(d[1] as usize); }
    serial_write(b" TLB-IPI-before=");
    unsafe { serial_write_decimal(d[2] as usize); }
    serial_write(b" TLB-IPI-after=");
    unsafe { serial_write_decimal(d[3] as usize); }
    serial_write(b" owner-CR3=");
    unsafe { serial_write_hex(d[4]); }
    serial_write(b" active-CR3=");
    unsafe { serial_write_hex(d[5]); }
    serial_write(b" online-CPUs=");
    unsafe { serial_write_decimal(d[6] as usize); }
    serial_write(b" pagefile-active=");
    unsafe { serial_write_decimal(d[7] as usize); }
    serial_write(b" IPI-shootdowns-enabled=");
    unsafe { serial_write_decimal(d[8] as usize); }
    serial_write(b"\r\n");
    #[cfg(feature = "diagnostic-vm")]
    {
        let map = kernel_vm::self_test_mapping_failure();
        if map[3] != 0 {
            serial_write(b"DEBUG VM WALK: stage=");
            unsafe { serial_write_decimal(map[3] as usize); }
            serial_write(b" (1=input,2=PML4,3=PDPT,4=PD,5=PT) root=");
            unsafe { serial_write_hex(map[0]); }
            serial_write(b" expected-pa=");
            unsafe { serial_write_hex(map[1]); }
            serial_write(b" va=");
            unsafe { serial_write_hex(map[2]); }
            serial_write(b" PML4E=");
            unsafe { serial_write_hex(map[4]); }
            serial_write(b" PDPTE=");
            unsafe { serial_write_hex(map[5]); }
            serial_write(b" PDE=");
            unsafe { serial_write_hex(map[6]); }
            serial_write(b" PTE=");
            unsafe { serial_write_hex(map[7]); }
            serial_write(b"\r\n");
        }
    }
}

pub unsafe fn halt() -> ! {
    // Persist the most recent trace when the disk-backed debug partition is
    // available, then keep the polled COM1 debugger prompt available.
    let saved = crate::ntoskrnl::debug::persist_to_storage();
    unsafe {
        if saved { serial_write(b"\r\nNTOSKRNL trace snapshot persisted.\r\n"); }
        else { serial_write(b"\r\nNTOSKRNL trace persistence unavailable.\r\n"); }
        serial_write(b"cognacos-kd> type help for debugger commands\r\n");
    }
    loop {
        crate::debug_console::poll();
        unsafe { asm!("pause", options(nomem, nostack)); }
    }
}

struct PanicSerialWriter;

impl core::fmt::Write for PanicSerialWriter {
    fn write_str(&mut self, value: &str) -> core::fmt::Result {
        unsafe { serial_write(value.as_bytes()); }
        Ok(())
    }
}

#[panic_handler]
fn panic(info: &PanicInfo<'_>) -> ! {
    use core::fmt::Write as _;

    let panic_line = info.location().map(|location| location.line() as u64).unwrap_or(0);
    crate::ntoskrnl::ke::diagnostics::record(crate::ntoskrnl::ke::diagnostics::EVENT_PANIC, panic_line, 0, 0);
    crate::ntoskrnl::debug::record_unfiltered(
        crate::ntoskrnl::debug::COMPONENT_EXCEPTION,
        crate::ntoskrnl::debug::LEVEL_ERROR,
        crate::ntoskrnl::debug::EVENT_KERNEL_PANIC,
        panic_line, 0, 0,
    );
    unsafe { serial_write(b"\r\nCognacOS FATAL: Rust panic"); }
    if let Some(location) = info.location() {
        let mut writer = PanicSerialWriter;
        let _ = write!(&mut writer, " at {}:{}", location.file(), location.line());
    }
    unsafe { serial_write(b"\r\n  message: "); }
    let mut writer = PanicSerialWriter;
    let _ = write!(&mut writer, "{}\r\n", info.message());
    unsafe {
        serial_write(b"System halted.\r\n");
        crate::ntoskrnl::debug::dump_serial(32);
        crate::ntoskrnl::ke::diagnostics::dump_serial(16);
        halt()
    }
}
