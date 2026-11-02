//! Early boot device and driver orchestration.
//! This module owns firmware-to-driver handoff and boot-start device staging;
//! device-specific execution remains in the separately built driver images.
use crate::*;
use core::arch::asm;

/// PCI.sys is the authoritative PCI bus enumerator. It is staged by the
/// driver build and embedded separately from ntoskrnl for registry-selected
/// initialization through the native PE driver loader.
const ACPI_PE_IMAGE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/acpi.sys"));
const PCI_PE_IMAGE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/pci.sys"));
const USBXHCI_PE_IMAGE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/usbxhci.sys"));
const UCX01020_PE_IMAGE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ucx01020.sys"));
const USBHUB3_PE_IMAGE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/usbhub3.sys"));

mod staged_driver_images {
    include!(concat!(env!("OUT_DIR"), "/driver_images.rs"));
}


/// Resolve a registry ImagePath against the native driver images staged by the
/// kernel build. The PE loader remains independent of the image source; this
/// catalog is only the current boot media provider. Unknown paths are refused.
fn driver_image_path_eq(path: &[u16], ascii: &[u8]) -> bool {
    path.len() == ascii.len() && path.iter().zip(ascii).all(|(unit, byte)| {
        let unit = if (b'A' as u16..=b'Z' as u16).contains(unit) { *unit + 32 } else { *unit };
        let byte = if (b'A'..=b'Z').contains(byte) { *byte + 32 } else { *byte };
        unit == byte as u16
    })
}

fn resolve_builtin_driver_image(path: &[u16], output: &mut [u8]) -> Option<usize> {
    const PREFIX: &[u16] = &[92, 83, 121, 115, 116, 101, 109, 82, 111, 111, 116, 92, 83, 121, 115, 116, 101, 109, 51, 50, 92, 100, 114, 105, 118, 101, 114, 115, 92];
    if path.len() <= PREFIX.len()
        || !path[..PREFIX.len()].iter().zip(PREFIX).all(|(a, b)| {
            let a = if (b'A' as u16..=b'Z' as u16).contains(a) { *a + 32 } else { *a };
            let b = if (b'A' as u16..=b'Z' as u16).contains(b) { *b + 32 } else { *b };
            a == b
        })
    {
        return None;
    }
    let name = &path[PREFIX.len()..];
    if name.iter().any(|unit| *unit == 0 || *unit == b'\\' as u16 || *unit == b'/' as u16) {
        return None;
    }
    let image = staged_driver_images::DRIVER_IMAGES.iter()
        .find(|(filename, _)| driver_image_path_eq(name, filename.as_bytes()))
        .map(|(_, bytes)| *bytes)?;
    if image.is_empty() || image.len() > output.len() {
        return None;
    }
    output[..image.len()].copy_from_slice(image);
    Some(image.len())
}

#[cfg(test)]
mod driver_image_source_tests {
    use super::*;

    fn path(name: &str) -> ([u16; 96], usize) {
        let mut result = [0u16; 96];
        let mut count = 0;
        for unit in "\\SystemRoot\\System32\\drivers\\".encode_utf16().chain(name.encode_utf16()) {
            result[count] = unit;
            count += 1;
        }
        (result, count)
    }

    fn resolve(name: &str, output: &mut [u8]) -> Option<usize> {
        let (units, length) = path(name);
        resolve_builtin_driver_image(&units[..length], output)
    }

    #[test]
    fn image_path_matching_is_case_insensitive_and_rejects_unknown_or_nested_paths() {
        let (canonical, length) = path("ACPI.SYS");
        let prefix_length = "\\SystemRoot\\System32\\drivers\\".encode_utf16().count();
        assert!(driver_image_path_eq(&canonical[prefix_length..length], b"acpi.sys"));
        let mut output = [0u8; 16];
        assert_eq!(resolve("..\\acpi.sys", &mut output), None);
        assert_eq!(resolve("unknown.sys", &mut output), None);
    }

    #[test]
    fn image_path_resolution_refuses_insufficient_output() {
        let mut output = [];
        assert_eq!(resolve("acpi.sys", &mut output), None);
    }
}

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
pub(crate) fn prepare_ps2_irq_route(madt:&acpi::MadtInfo,bsp_apic_id:u32)->bool {
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
pub(crate) fn poll_ps2_keyboard_input(){
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
pub(crate) fn initialize_acpi_driver(rsdp_address:u64)->bool{
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
   |image_path, output| resolve_builtin_driver_image(image_path, output),ntoskrnl::resolve
  ).is_ok()
 }
}

static PCI_DRIVER_BINDING_STARTED:core::sync::atomic::AtomicBool=core::sync::atomic::AtomicBool::new(false);
static PCI_DRIVER_READY:core::sync::atomic::AtomicBool=core::sync::atomic::AtomicBool::new(false);
static mut PCI_DRIVER_OBJECT_STORAGE:core::mem::MaybeUninit<ntoskrnl::driver_loader::DriverObject>=core::mem::MaybeUninit::uninit();
static mut PCI_HANDOFF_STORAGE:core::mem::MaybeUninit<nt_abi::bus::PCI_DRIVER_HANDOFF>=core::mem::MaybeUninit::uninit();
static mut PCI_REGISTRY_IMAGE_BUFFER:[u8;PCI_PE_IMAGE.len()]=[0;PCI_PE_IMAGE.len()];
static mut PCI_BUS_FDO:*mut io::wdm::DeviceObject=core::ptr::null_mut();

/// Prepare ECAM resources and invoke pci.sys DriverEntry, without enumerating.
fn initialize_pci_driver(rsdp_address:u64)->bool {
 use core::sync::atomic::Ordering;
 use nt_abi::bus::{PCI_DRIVER_HANDOFF,PCI_ECAM_WINDOW,PCI_DRIVER_HANDOFF_MAGIC,PCI_DRIVER_HANDOFF_VERSION,PCI_DRIVER_HANDOFF_MAX_WINDOWS};
 if PCI_PE_IMAGE.is_empty()||rsdp_address==0{return false;}
 if PCI_DRIVER_BINDING_STARTED.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).is_err(){return PCI_DRIVER_READY.load(Ordering::Acquire);}
 let result=(|| -> bool {
  let table=match unsafe{acpi::find_table(rsdp_address,*b"MCFG",0)}{Ok(t)=>t,Err(_)=>return false};
  let mcfg=match acpi::parse_mcfg(table){Ok(v)=>v,Err(_)=>return false};
  if mcfg.count==0||mcfg.count>PCI_DRIVER_HANDOFF_MAX_WINDOWS{return false;}
  let mut handoff=PCI_DRIVER_HANDOFF{magic:PCI_DRIVER_HANDOFF_MAGIC,size:core::mem::size_of::<PCI_DRIVER_HANDOFF>() as u32,
   version:PCI_DRIVER_HANDOFF_VERSION,window_count:mcfg.count as u32,reserved:0,
   windows:[PCI_ECAM_WINDOW::default();PCI_DRIVER_HANDOFF_MAX_WINDOWS]};
  for (i,a) in mcfg.allocations.iter().take(mcfg.count).enumerate(){
   if mcfg.allocations.iter().take(i).any(|p|p.segment==a.segment&&p.start_bus<=a.end_bus&&a.start_bus<=p.end_bus){return false;}
  }
  let mut mapped=0usize;
  for a in mcfg.allocations.iter().take(mcfg.count){
   let Some(length)=((a.end_bus as u64-a.start_bus as u64)+1).checked_mul(1<<20) else{return false};
   let virtual_base=match kernel_vm::map_io_space(a.base_address,length){Ok(v)=>v,Err(_)=>return false};
   handoff.windows[mapped]=PCI_ECAM_WINDOW{virtual_base,physical_base:a.base_address,segment:a.segment,bus_start:a.start_bus,bus_end:a.end_bus};mapped+=1;
  }
  for w in handoff.windows.iter().take(mapped){
   if hal::register_pci_ecam_window(w.virtual_base,w.physical_base,w.segment,w.bus_start,w.bus_end).is_err(){return false;}
  }
  unsafe {
   let handoff_ptr=core::ptr::addr_of_mut!(PCI_HANDOFF_STORAGE).cast::<PCI_DRIVER_HANDOFF>();handoff_ptr.write(handoff);
   let driver=core::ptr::addr_of_mut!(PCI_DRIVER_OBJECT_STORAGE).cast::<ntoskrnl::driver_loader::DriverObject>();
   driver.write(core::mem::zeroed());(*driver).object_type=4;(*driver).size=core::mem::size_of::<ntoskrnl::driver_loader::DriverObject>() as i16;(*driver).driver_extension=handoff_ptr.cast();
   let Some(bytes)=boot_hive_bytes("SYSTEM") else{return false};
   let hive=match ntoskrnl::cm::hive::Hive::open(bytes){Ok(h)=>h,Err(_)=>return false};
   const SERVICE:&[u16]=&[80,67,73];let mut path=[0u16;1024];
   let image=core::slice::from_raw_parts_mut(core::ptr::addr_of_mut!(PCI_REGISTRY_IMAGE_BUFFER).cast::<u8>(),PCI_PE_IMAGE.len());
   if ntoskrnl::driver_loader::load_registered_service_with_resolver(&hive,SERVICE,&mut path,image,driver,core::ptr::null_mut(),
    |p,o|resolve_builtin_driver_image(p,o),ntoskrnl::resolve).is_err(){return false;}
  }
  true
 })();
 PCI_DRIVER_READY.store(result,Ordering::Release);
 result
}

/// PCI enumeration is a separate phase and is forbidden until DriverEntry passed.
fn enumerate_pci_bus_devices()->usize {
 use core::sync::atomic::Ordering;
 if !PCI_DRIVER_READY.load(Ordering::Acquire){return 0;}
 unsafe {
  let Some(bytes)=boot_hive_bytes("SYSTEM") else{return 0};
  let hive=match ntoskrnl::cm::hive::Hive::open(bytes){Ok(h)=>h,Err(_)=>return 0};
  let driver=core::ptr::addr_of_mut!(PCI_DRIVER_OBJECT_STORAGE).cast::<ntoskrnl::driver_loader::DriverObject>();
  let mut bus=core::ptr::null_mut();
  if io::wdm::io_create_device(driver.cast(),0,core::ptr::null_mut(),0x22,0,0,&mut bus)!=nt_abi::ntstatus::STATUS_SUCCESS||bus.is_null(){return 0;}
  let root=match ntoskrnl::pnp::register_root_device(){Ok(i)=>i,Err(_)=>{let _=io::wdm::io_delete_device(bus);return 0;}};
  if ntoskrnl::pnp::bind_pnp_device_object(root,bus).is_err(){let _=io::wdm::io_delete_device(bus);return 0;}
  PCI_BUS_FDO=bus;
  let status=io::wdm::IoPnPEnumerateBus(bus);
  if status!=nt_abi::ntstatus::STATUS_SUCCESS{let _=io::wdm::io_delete_device(bus);PCI_BUS_FDO=core::ptr::null_mut();return 0;}
  let mut count=0usize;
  for child_index in 0..ntoskrnl::pnp::MAX_PNP_DEVICES{
   let mut child=core::ptr::null_mut();
   if io::wdm::IoPnPQueryBusChild(bus,child_index as u32,&mut child)!=nt_abi::ntstatus::STATUS_SUCCESS{break;}
   if child.is_null(){continue;}
   let ext=(*child).device_extension.cast::<io::wdm::PciPdoExtension>();
   if ext.is_null()||(*ext).magic!=io::wdm::PCI_PDO_EXTENSION_MAGIC{continue;}
   if let Ok(index)=ntoskrnl::pnp::register_pci_information(&(*ext).information,Some(root)){
    if ntoskrnl::pnp::bind_pnp_device_object(index,child).is_ok(){count+=1;}
    else{let _=ntoskrnl::pnp::transition_pnp_device(index,ntoskrnl::pnp::PNP_DEVICE_REMOVED,nt_abi::ntstatus::STATUS_INVALID_DEVICE_REQUEST);}
   }
  }
  let _=ntoskrnl::pnp::match_registry_services(&hive);count
 }
}

pub(crate) fn initialize_pci_driver_for_boot(rsdp_address:u64)->bool{initialize_pci_driver(rsdp_address)}
pub(crate) fn enumerate_acpi_pci_devices(_rsdp_address:u64)->usize{enumerate_pci_bus_devices()}

/// Query PCI.sys' versioned inventory, resolve the endpoint's full bridge
/// path, size BAR0 with decode and bus mastering disabled, then publish the
/// kernel-owned PnP resource record. PCI.sys remains the sole enumerator.
pub(crate) fn probe_xhci_pci_function(madt:&acpi::MadtInfo,bsp_apic_id:u32)->bool {
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
   |image_path, output| resolve_builtin_driver_image(image_path, output),ntoskrnl::resolve
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
   |image_path, output| resolve_builtin_driver_image(image_path, output),ntoskrnl::resolve
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
            |image_path, output| resolve_builtin_driver_image(image_path, output),ntoskrnl::resolve);
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

