#![no_std]
#![no_main]
use core::{cell::UnsafeCell,ffi::c_void,mem::MaybeUninit,sync::atomic::{AtomicBool,AtomicU64,Ordering}};
use cognacos_hal_contract::{HalServiceTable,KernelIoServiceTable};
use cognacos_ucx01020::root_hub::{self,PortStatus,RootHubNotifications,RootHubPortIo,SetupPacket};
use cognacos_ucx01020::wdm::{UrbHeader,URB_FUNCTION_BULK_OR_INTERRUPT_TRANSFER,URB_FUNCTION_CONTROL_TRANSFER};
const EXT_MAGIC:u64=0x434f_474e_4143_5543;
const STATUS_SUCCESS:i32=0;
const STATUS_PENDING:i32=0x00000103;
const STATUS_INVALID_PARAMETER:i32=0xC000000D_u32 as i32;
const STATUS_INVALID_DEVICE_REQUEST:i32=0xC0000010_u32 as i32;
const STATUS_INSUFFICIENT_RESOURCES:i32=0xC000009A_u32 as i32;
const STATUS_DELETE_PENDING:i32=0xC0000056_u32 as i32;
const MAX_PENDING:usize=32;
pub const ROOT_HUB_INTERRUPT_PIPE:u64=0xC0AC_0001;
const TRB_TYPE_PORT_STATUS_CHANGE:u8=34;
#[repr(C)]
#[derive(Clone,Copy)]
struct XhciTrb{parameter:u64,status:u32,control:u32}
#[repr(C)]
pub struct UcxDriverExtension{
 magic:u64,size:u32,version:u32,
 kernel_io:*const KernelIoServiceTable,hal:*const HalServiceTable,
 event_sink:*mut AtomicU64,event_context:*mut AtomicU64,
 mmio_base:u64,operational_offset:u32,port_count:u32,
 root_hub_device_out:*mut u64,
}
type DriverDispatch=unsafe extern "win64" fn(*mut c_void,*mut c_void)->i32;
#[repr(C)]
struct DriverObject{
 object_type:i16,size:i16,device_object:*mut c_void,flags:u32,
 driver_start:*mut c_void,driver_size:u32,driver_section:*mut c_void,
 driver_extension:*mut UcxDriverExtension,
 driver_name:cognacos_nt_abi::object::UNICODE_STRING,
 hardware_database:*mut cognacos_nt_abi::object::UNICODE_STRING,
 fast_io_dispatch:*mut c_void,driver_init:*mut c_void,driver_start_io:*mut c_void,
 driver_unload:Option<unsafe extern "win64" fn(*mut c_void)>,
 major_function:[Option<DriverDispatch>;28],
}
#[repr(C)]
struct UrbBulkInterrupt{
 header:UrbHeader,pipe_handle:u64,transfer_flags:u32,transfer_buffer_length:u32,
 transfer_buffer:*mut c_void,transfer_buffer_mdl:*mut c_void,urb_link:*mut c_void,hca:*mut c_void,
}
#[repr(C)]
struct UrbControlTransfer{
 header:UrbHeader,pipe_handle:u64,transfer_flags:u32,transfer_buffer_length:u32,
 transfer_buffer:*mut c_void,transfer_buffer_mdl:*mut c_void,urb_link:*mut c_void,hca:*mut c_void,
 setup_packet:[u8;8],
}
#[derive(Clone,Copy)]
struct Pending{used:bool,irp:u64,urb:u64,mdl:u64,mapped:u64,length:u32}
impl Pending{const EMPTY:Self=Self{used:false,irp:0,urb:0,mdl:0,mapped:0,length:0};}
struct Runtime{notifications:RootHubNotifications,pending:[Pending;MAX_PENDING],port_count:u8}
impl Runtime{const fn new(ports:u8)->Self{Self{notifications:RootHubNotifications::new(ports),pending:[Pending::EMPTY;MAX_PENDING],port_count:ports}}}
struct RuntimeCell(UnsafeCell<MaybeUninit<Runtime>>);
unsafe impl Sync for RuntimeCell{}
static RUNTIME:RuntimeCell=RuntimeCell(UnsafeCell::new(MaybeUninit::uninit()));
static RUNTIME_READY:AtomicBool=AtomicBool::new(false);
static RUNTIME_LOCK:AtomicBool=AtomicBool::new(false);
static ROOT_HUB_DEVICE:AtomicU64=AtomicU64::new(0);
static CONTROLLER_DEVICE:AtomicU64=AtomicU64::new(0);
static KERNEL_IO:AtomicU64=AtomicU64::new(0);
static EVENT_SINK_SLOT:AtomicU64=AtomicU64::new(0);
static EVENT_CONTEXT_SLOT:AtomicU64=AtomicU64::new(0);
static HAL_TABLE:AtomicU64=AtomicU64::new(0);
static MMIO_BASE:AtomicU64=AtomicU64::new(0);
static OPERATIONAL_OFFSET:AtomicU64=AtomicU64::new(0);
static PORT_COUNT:AtomicU64=AtomicU64::new(0);
fn lock_runtime(){while RUNTIME_LOCK.compare_exchange_weak(false,true,Ordering::Acquire,Ordering::Relaxed).is_err(){core::hint::spin_loop();}}
fn unlock_runtime(){RUNTIME_LOCK.store(false,Ordering::Release);}
unsafe fn runtime_mut()->&'static mut Runtime{unsafe{(*RUNTIME.0.get()).assume_init_mut()}}
fn io_table()->Option<&'static KernelIoServiceTable>{let p=KERNEL_IO.load(Ordering::Acquire) as *const KernelIoServiceTable;if p.is_null(){None}else{unsafe{p.as_ref()}}}
fn finish_pending(p:Pending,bytes:&[u8],status:i32,io:&KernelIoServiceTable){
 let n=if status==STATUS_SUCCESS{(p.length as usize).min(bytes.len())}else{0};
 if p.urb!=0{unsafe{
  let h=p.urb as *mut UrbHeader;(*h).status=if status==STATUS_SUCCESS{0}else{0xC0000001u32};
  core::ptr::write_unaligned((p.urb as *mut u8).add(core::mem::size_of::<UrbHeader>()+12).cast::<u32>(),n as u32);
 }}
 if n!=0&&p.mapped!=0{unsafe{core::ptr::copy_nonoverlapping(bytes.as_ptr(),p.mapped as *mut u8,n);}}
 if let Some(complete)=io.complete_irp_status{unsafe{complete(p.irp as *mut c_void,status,n,0);}}
 if let Some(unpin)=io.unpin_buffer{unsafe{unpin(p.mdl as *mut c_void,p.mapped as *mut c_void);}}
}

fn try_complete_pending(){
 let Some(io)=io_table() else{return};
 if !RUNTIME_READY.load(Ordering::Acquire){return}
 let mut payload=[0u8;32];
 let picked={
  lock_runtime();
  let rt=unsafe{runtime_mut()};
  if !rt.notifications.has_pending(){unlock_runtime();return}
  let Some(index)=rt.pending.iter().position(|p|p.used) else{unlock_runtime();return};
  let n=match rt.notifications.complete_interrupt(&mut payload){Ok(n)=>n,Err(_)=>{unlock_runtime();return}};
  let mut p=rt.pending[index];p.length=p.length.min(n as u32);rt.pending[index]=Pending::EMPTY;
  unlock_runtime();(p,n)
 };
 finish_pending(picked.0,&payload[..picked.1],STATUS_SUCCESS,io);
}
unsafe extern "win64" fn xhci_event_sink(context:u64,event:*const XhciTrb){
 if context==0||event.is_null()||!RUNTIME_READY.load(Ordering::Acquire){return}
 let e=unsafe{*event};
 let ty=((e.control>>10)&0x3f) as u8;
 if ty!=TRB_TYPE_PORT_STATUS_CHANGE{return}
 let port=(e.parameter>>24) as u8;
 let rt_ptr=context as *mut Runtime;
 lock_runtime();
 let result=unsafe{(*rt_ptr).notifications.notify_port(port)};
 unlock_runtime();
 if result.is_ok(){try_complete_pending();}
}
struct PortIo{hal:&'static HalServiceTable,base:u64,op:u32,ports:u8}
impl PortIo{
 fn address(&self,port:u8)->Result<u64,()>{if port==0||port>self.ports{return Err(())}self.base.checked_add(self.op as u64+0x400+(port as u64-1)*0x10).ok_or(())}
 fn read(&self,port:u8)->Result<u32,()>{let f=self.hal.read_mmio_u32.ok_or(())?;let mut v=0;let s=unsafe{f(self.address(port)?,&mut v)};if s.code==0{Ok(v)}else{Err(())}}
 fn write(&self,port:u8,set:u32,clear:u32,w1c:u32)->Result<(),()>{let f=self.hal.write_mmio_u32.ok_or(())?;let old=self.read(port)?;let rw=(1<<1)|(1<<4)|(0xf<<5)|(1<<9);let value=((old&rw)&!clear)|set|w1c;let s=unsafe{f(self.address(port)?,value)};if s.code==0{Ok(())}else{Err(())}}
}
impl RootHubPortIo for PortIo{
 fn read_port(&mut self,port:u8)->Result<PortStatus,()>{
  let v=self.read(port)?;
  let mut status=0u16;let mut change=0u16;
  if v&(1<<0)!=0{status|=1<<0}if v&(1<<1)!=0{status|=1<<1}if v&(1<<3)!=0{status|=1<<3}
  if v&(1<<4)!=0{status|=1<<4}if (v>>5)&0xf==3{status|=1<<2}if v&(1<<9)!=0{status|=1<<8}
  let speed=(v>>10)&0xf;if speed==2{status|=1<<9}else if speed==3{status|=1<<10}else if speed>=4{status|=1<<10}
  if v&(1<<17)!=0{change|=1<<0}if v&(1<<18)!=0{change|=1<<1}
  if v&(1<<20)!=0{change|=1<<3}if v&(1<<21)!=0{change|=1<<4}
  if v&(1<<22)!=0{change|=1<<2}
  Ok(PortStatus{status,change})
 }
 fn set_feature(&mut self,port:u8,feature:u16)->Result<(),()>{
  match feature{root_hub::USB_PORT_FEAT_POWER=>self.write(port,1<<9,0,0),
   root_hub::USB_PORT_FEAT_RESET=>self.write(port,1<<4,0,0),
   root_hub::USB_PORT_FEAT_SUSPEND=>self.write(port,(3<<5)|(1<<16),0,0),
   root_hub::USB_PORT_FEAT_ENABLE=>if self.read(port)?&(1<<1)!=0{Ok(())}else{Err(())},_=>Err(())}
 }
 fn clear_feature(&mut self,port:u8,feature:u16)->Result<(),()>{
  match feature{root_hub::USB_PORT_FEAT_ENABLE=>self.write(port,1<<1,0,0),
   root_hub::USB_PORT_FEAT_SUSPEND=>self.write(port,(1<<16),(0xf<<5),0),
   root_hub::USB_PORT_FEAT_POWER=>self.write(port,0,1<<9,0),
   root_hub::USB_PORT_FEAT_C_CONNECTION=>self.write(port,0,0,1<<17),
   root_hub::USB_PORT_FEAT_C_ENABLE=>self.write(port,0,0,1<<18),
   root_hub::USB_PORT_FEAT_C_SUSPEND=>self.write(port,0,0,1<<22),
   root_hub::USB_PORT_FEAT_C_OVER_CURRENT=>self.write(port,0,0,1<<20),
   root_hub::USB_PORT_FEAT_C_RESET=>self.write(port,0,0,1<<21),_=>Err(())}
 }
 fn get_descriptor(&mut self,kind:u8,out:&mut [u8])->Result<usize,()>{
  let bits=(self.ports as usize+1+7)/8;
  let n=if kind==0x29{7+2*bits}else if kind==0x2a{10+2*bits}else{return Err(())};
  if out.len()<n{return Err(())}
  out[..n].fill(0);out[0]=n as u8;out[1]=kind;out[2]=self.ports;out[3]=1;out[5]=10;
  if kind==0x29{out[7+bits..n].fill(0xff);}else{out[10+bits..n].fill(0xff);}
  Ok(n)
 }
}
fn complete_now(irp:*mut c_void,urb:*mut UrbHeader,mdl:*mut c_void,mapped:*mut c_void,length:u32,bytes:&[u8],status:i32)->i32{
 let p=Pending{used:true,irp:irp as u64,urb:urb as u64,mdl:mdl as u64,mapped:mapped as u64,length};
 if let Some(io)=io_table(){finish_pending(p,bytes,status,io);}
 status
}
fn submit_interrupt_urb(irp:*mut c_void,urb:*mut UrbBulkInterrupt)->(i32,bool){
 if urb.is_null()||urb as usize&7!=0{return (STATUS_INVALID_PARAMETER,false)}
 let u=unsafe{&mut *urb};
 if u.header.length as usize<core::mem::size_of::<UrbBulkInterrupt>()||u.pipe_handle!=ROOT_HUB_INTERRUPT_PIPE
  ||u.transfer_buffer.is_null()||u.transfer_buffer_length==0||u.transfer_buffer_length>32||!u.transfer_buffer_mdl.is_null(){return (STATUS_INVALID_PARAMETER,false)}
 let Some(io)=io_table() else{return (STATUS_INVALID_DEVICE_REQUEST,false)};
 let (Some(pin),Some(unpin))=(io.pin_buffer,io.unpin_buffer) else{return (STATUS_INVALID_DEVICE_REQUEST,false)};
 let mut mdl=core::ptr::null_mut();let mut mapped=core::ptr::null_mut();
 let st=unsafe{pin(u.transfer_buffer,u.transfer_buffer_length,1,&mut mdl,&mut mapped)};
 if st<0||mdl.is_null()||mapped.is_null(){return (if st<0{st}else{STATUS_INSUFFICIENT_RESOURCES},false)}
 let mut payload=[0u8;32];
 let immediate={
  lock_runtime();
  let rt=unsafe{runtime_mut()};
  if rt.notifications.has_pending(){
   let n=rt.notifications.complete_interrupt(&mut payload).unwrap_or(0);
   unlock_runtime();Some(n)
  }else if let Some(slot)=rt.pending.iter_mut().find(|p|!p.used){
   *slot=Pending{used:true,irp:irp as u64,urb:urb as u64,mdl:mdl as u64,mapped:mapped as u64,length:u.transfer_buffer_length};
   unlock_runtime();None
  }else{unlock_runtime();unsafe{unpin(mdl,mapped);}return (STATUS_INSUFFICIENT_RESOURCES,false)}
 };
 if let Some(n)=immediate{let p=Pending{used:true,irp:irp as u64,urb:urb as u64,mdl:mdl as u64,mapped:mapped as u64,length:u.transfer_buffer_length};finish_pending(p,&payload[..n],STATUS_SUCCESS,io);(STATUS_SUCCESS,true)}else{(STATUS_PENDING,false)}
}
fn control_urb(irp:*mut c_void,urb:*mut UrbControlTransfer)->(i32,usize){
 if urb.is_null()||urb as usize&7!=0{return (STATUS_INVALID_PARAMETER,0)}
 let u=unsafe{&mut *urb};
 if u.header.length as usize<core::mem::size_of::<UrbControlTransfer>()||u.transfer_buffer_length>4096{return (STATUS_INVALID_PARAMETER,0)}
 let setup=SetupPacket{request_type:u.setup_packet[0],request:u.setup_packet[1],
  value:u16::from_le_bytes([u.setup_packet[2],u.setup_packet[3]]),
  index:u16::from_le_bytes([u.setup_packet[4],u.setup_packet[5]]),
  length:u16::from_le_bytes([u.setup_packet[6],u.setup_packet[7]])};
 if setup.length as u32>u.transfer_buffer_length{return (STATUS_INVALID_PARAMETER,0)}
 let Some(io)=io_table() else{return (STATUS_INVALID_DEVICE_REQUEST,0)};
 let mut mdl=core::ptr::null_mut();let mut mapped=core::ptr::null_mut();
 let out_len=u.transfer_buffer_length;
 if out_len!=0{
  if u.transfer_buffer.is_null()||!u.transfer_buffer_mdl.is_null(){return (STATUS_INVALID_PARAMETER,0)}
  let Some(pin)=io.pin_buffer else{return (STATUS_INVALID_DEVICE_REQUEST,0)};
  let st=unsafe{pin(u.transfer_buffer,out_len,u8::from(setup.request_type&0x80!=0),&mut mdl,&mut mapped)};
  if st<0||mdl.is_null()||mapped.is_null(){return (if st<0{st}else{STATUS_INSUFFICIENT_RESOURCES},0)}
 }
 let ports=PORT_COUNT.load(Ordering::Acquire) as u8;
 let hal=HAL_TABLE.load(Ordering::Acquire) as *const HalServiceTable;
 if hal.is_null(){if out_len!=0{if let Some(unpin)=io.unpin_buffer{unsafe{unpin(mdl,mapped);}}}return (STATUS_INVALID_DEVICE_REQUEST,0)}
 let mut port_io=PortIo{hal:unsafe{&*hal},base:MMIO_BASE.load(Ordering::Acquire),op:OPERATIONAL_OFFSET.load(Ordering::Acquire) as u32,ports};
 let out=if out_len==0{&mut []}else{unsafe{core::slice::from_raw_parts_mut(mapped.cast::<u8>(),out_len as usize)}};
 let result=root_hub::handle_control(&mut port_io,setup,ports,out);
 let (status,n)=match result{Ok(n)=>(STATUS_SUCCESS,n),Err(_)=>(STATUS_INVALID_DEVICE_REQUEST,0)};
 u.header.status=if status==STATUS_SUCCESS{0}else{0xC0000001};
 u.transfer_buffer_length=n as u32;
 if out_len!=0{if let Some(unpin)=io.unpin_buffer{unsafe{unpin(mdl,mapped);}}}
 let _=irp;
 (status,n)
}
unsafe extern "win64" fn ucx_dispatch(device:*mut c_void,irp:*mut c_void)->i32{
 if irp.is_null(){return STATUS_INVALID_PARAMETER}
 let Some(io)=io_table() else{return STATUS_INVALID_DEVICE_REQUEST};
 let Some(get_stack)=io.current_stack_location else{return STATUS_INVALID_DEVICE_REQUEST};
 let stack=unsafe{get_stack(irp)};
 if stack.is_null(){return STATUS_INVALID_PARAMETER}
 let major=unsafe{core::ptr::read(stack.cast::<u8>())};
 let minor=unsafe{core::ptr::read(stack.cast::<u8>().add(1))};
 let mut information=0usize;let mut completed_inside=false;
 let status=match major{
  0x1b=>match minor{0x00|0x03|0x04|0x05|0x06|0x17|0x02=>STATUS_SUCCESS,_=>STATUS_INVALID_DEVICE_REQUEST},
  0x16=>STATUS_SUCCESS,
  IRP_MJ_INTERNAL_DEVICE_CONTROL=>{
   if device as u64==ROOT_HUB_DEVICE.load(Ordering::Acquire){
    let urb=unsafe{core::ptr::read_unaligned(stack.cast::<u8>().add(4).cast::<*mut c_void>())};
    if urb.is_null(){STATUS_INVALID_PARAMETER}else{
     let function=unsafe{core::ptr::read_unaligned(urb.cast::<u8>().add(2).cast::<u16>())};
     if function==URB_FUNCTION_BULK_OR_INTERRUPT_TRANSFER{let (s,done)=submit_interrupt_urb(irp,urb.cast());completed_inside=done;s}
     else if function==URB_FUNCTION_CONTROL_TRANSFER{let (s,n)=control_urb(irp,urb.cast());information=n;s}
     else{STATUS_INVALID_DEVICE_REQUEST}
    }
   }else{STATUS_INVALID_DEVICE_REQUEST}
  },
  _=>STATUS_INVALID_DEVICE_REQUEST,
 };
 if status!=STATUS_PENDING&&!completed_inside{if let Some(complete)=io.complete_irp_status{unsafe{complete(irp,status,information,0);}}}
 status
}
unsafe extern "win64" fn ucx_event_sink(context:u64,event:*const XhciTrb){
 if context==0||event.is_null()||!RUNTIME_READY.load(Ordering::Acquire){return}
 let e=unsafe{*event};let ty=((e.control>>10)&0x3f) as u8;if ty!=TRB_TYPE_PORT_STATUS_CHANGE{return}
 let port=(e.parameter>>24) as u8;
 lock_runtime();let result=unsafe{runtime_mut().notifications.notify_port(port)};unlock_runtime();
 if result.is_ok(){try_complete_pending();}
}
unsafe extern "win64" fn ucx_unload(driver:*mut c_void){
 let _=driver;
 if !RUNTIME_READY.swap(false,Ordering::AcqRel){return}
 let sink=EVENT_SINK_SLOT.load(Ordering::Acquire) as *mut AtomicU64;
 if !sink.is_null(){unsafe{(*sink).store(0,Ordering::Release);}}
 let Some(io)=io_table() else{return};
 let mut pending=[Pending::EMPTY;MAX_PENDING];
 lock_runtime();let rt=unsafe{runtime_mut()};
 for i in 0..MAX_PENDING{pending[i]=rt.pending[i];rt.pending[i]=Pending::EMPTY;}
 unlock_runtime();
 for p in pending.into_iter().filter(|p|p.used){finish_pending(p,&[],STATUS_DELETE_PENDING,io);}
 if let Some(delete)=io.delete_device{
  let root=ROOT_HUB_DEVICE.swap(0,Ordering::AcqRel);let controller=CONTROLLER_DEVICE.swap(0,Ordering::AcqRel);
  if root!=0{unsafe{delete(root as *mut c_void);}}
  if controller!=0{unsafe{delete(controller as *mut c_void);}}
 }
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn DriverEntry(driver:PVOID,_registry_path:PVOID)->i32{
 if driver.is_null(){return STATUS_INVALID_PARAMETER}
 let object=unsafe{&mut *(driver as *mut DriverObject)};
 let Some(ext)=(unsafe{object.driver_extension.as_ref()}) else{return STATUS_INVALID_PARAMETER};
 if ext.magic!=EXT_MAGIC||ext.version!=1||ext.size as usize<core::mem::size_of::<UcxDriverExtension>()
  ||ext.kernel_io.is_null()||ext.hal.is_null()||ext.event_sink.is_null()||ext.event_context.is_null()
  ||ext.root_hub_device_out.is_null()||ext.mmio_base==0||ext.operational_offset<0x20||ext.port_count==0||ext.port_count>255{return STATUS_INVALID_PARAMETER}
 let io=unsafe{&*ext.kernel_io};let hal=unsafe{&*ext.hal};
 if !io.compatible()||!hal.compatible()||hal.read_mmio_u32.is_none()||hal.write_mmio_u32.is_none(){return STATUS_INVALID_DEVICE_REQUEST}
 KERNEL_IO.store(ext.kernel_io as u64,Ordering::Release);HAL_TABLE.store(ext.hal as u64,Ordering::Release);
 EVENT_SINK_SLOT.store(ext.event_sink as u64,Ordering::Release);EVENT_CONTEXT_SLOT.store(ext.event_context as u64,Ordering::Release);
 MMIO_BASE.store(ext.mmio_base,Ordering::Release);OPERATIONAL_OFFSET.store(ext.operational_offset as u64,Ordering::Release);PORT_COUNT.store(ext.port_count as u64,Ordering::Release);
 let runtime_ptr=unsafe{(*RUNTIME.0.get()).as_mut_ptr()};unsafe{runtime_ptr.write(Runtime::new(ext.port_count as u8));}
 RUNTIME_READY.store(true,Ordering::Release);
 static CONTROLLER_NAME:[u16;18]=[92,68,101,118,105,99,101,92,67,111,103,110,97,99,85,67,88,0];
 static ROOT_NAME:[u16;22]=[92,68,101,118,105,99,101,92,67,111,103,110,97,99,82,111,111,116,72,117,98,0];
 let mut controller_name=cognacos_nt_abi::object::UNICODE_STRING{length:34,maximum_length:36,buffer:CONTROLLER_NAME.as_ptr() as *mut u16};
 let mut root_name=cognacos_nt_abi::object::UNICODE_STRING{length:42,maximum_length:44,buffer:ROOT_NAME.as_ptr() as *mut u16};
 for f in object.major_function.iter_mut(){*f=Some(ucx_dispatch);}
 object.driver_unload=Some(ucx_unload);
 let Some(create)=io.create_device else{return STATUS_INVALID_DEVICE_REQUEST};
 let mut controller=core::ptr::null_mut();let st=unsafe{create(driver,0,&mut controller_name,4,0,0,&mut controller)};
 if st<0||controller.is_null(){RUNTIME_READY.store(false,Ordering::Release);return st}
 let mut root=core::ptr::null_mut();let st=unsafe{create(driver,0,&mut root_name,0x22,0,0,&mut root)};
 if st<0||root.is_null(){if let Some(delete)=io.delete_device{unsafe{delete(controller);}}RUNTIME_READY.store(false,Ordering::Release);return st}
 CONTROLLER_DEVICE.store(controller as u64,Ordering::Release);ROOT_HUB_DEVICE.store(root as u64,Ordering::Release);
 unsafe{ext.root_hub_device_out.write(root as u64);(*ext.event_context).store(runtime_ptr as u64,Ordering::Release);(*ext.event_sink).store(xhci_event_sink as usize as u64,Ordering::Release);}
 0
}
#[used]
#[unsafe(link_section=".rodata")]
static DRIVER_SIGNATURE:[u8;8]=*b"UCX01020";
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>)->!{loop{unsafe{core::arch::asm!("cli; hlt",options(nomem,nostack));}}}
