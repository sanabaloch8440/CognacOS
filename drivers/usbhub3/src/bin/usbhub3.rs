#![no_std]
#![no_main]
use core::{cell::UnsafeCell,ffi::c_void,mem::MaybeUninit,sync::atomic::{AtomicBool,AtomicU64,Ordering}};
use cognacos_hal_contract::KernelIoServiceTable;
use cognacos_nt_abi::types::PVOID;
const EXT_MAGIC:u64=0x434f_474e_4143_4833;
const CHILD_MAGIC:u64=0x434f_474e_5553_4233;
const STATUS_SUCCESS:i32=0;
const STATUS_INVALID_PARAMETER:i32=0xC000000D_u32 as i32;
const STATUS_INVALID_DEVICE_REQUEST:i32=0xC0000010_u32 as i32;
const STATUS_INSUFFICIENT_RESOURCES:i32=0xC000009A_u32 as i32;
const MAX_CHILDREN:usize=64;
#[repr(C)] #[derive(Clone,Copy)] struct DeviceRelations{count:u32,objects:[*mut c_void;64]}
#[repr(C)] pub struct HubDriverExtension{
 magic:u64,size:u32,version:u32,kernel_io:*const KernelIoServiceTable,
 root_hub_device:u64,port_count:u32,event_sink:*mut AtomicU64,event_context:*mut AtomicU64,
 hub_device_out:*mut u64,child_factory_out:*mut u64,
}
type DriverDispatch=unsafe extern "win64" fn(*mut c_void,*mut c_void)->i32;
#[repr(C)] struct DriverObject{
 object_type:i16,size:i16,device_object:*mut c_void,flags:u32,driver_start:*mut c_void,
 driver_size:u32,driver_section:*mut c_void,driver_extension:*mut HubDriverExtension,
 driver_name:cognacos_nt_abi::object::UNICODE_STRING,hardware_database:*mut cognacos_nt_abi::object::UNICODE_STRING,
 fast_io_dispatch:*mut c_void,driver_init:*mut c_void,driver_start_io:*mut c_void,
 driver_unload:Option<unsafe extern "win64" fn(*mut c_void)>,major_function:[Option<DriverDispatch>;28],
}
#[repr(C)] struct DeviceObjectPrefix{
 object_type:i16,size:u16,reference_count:i32,driver_object:*mut c_void,next_device:*mut c_void,
 attached_device:*mut c_void,current_irp:*mut c_void,timer:*mut c_void,flags:u32,
 characteristics:u32,vpb:*mut c_void,device_extension:*mut c_void,
}
#[repr(C)] struct HubFdoExtension{magic:u64,lower_device:*mut c_void,root_hub:*mut c_void,port_count:u32}
#[repr(C)] struct ChildExtension{
 magic:u64,version:u32,port:u32,vendor_id:u16,product_id:u16,descriptor:[u8;18],
 device_id:[u16;40],instance_id:[u16;24],hardware_ids:[u16;80],compatible_ids:[u16;80],
}
#[derive(Clone,Copy)] struct ChildRecord{used:bool,port:u8,pdo:u64}
impl ChildRecord{const EMPTY:Self=Self{used:false,port:0,pdo:0};}
struct HubRuntime{children:[ChildRecord;MAX_CHILDREN],pending_ports:[AtomicU64;4],relations:DeviceRelations,target_relations:[DeviceRelations;MAX_CHILDREN]}
impl HubRuntime{const fn new()->Self{Self{children:[ChildRecord::EMPTY;MAX_CHILDREN],pending_ports:[const{AtomicU64::new(0)};4],relations:DeviceRelations{count:0,objects:[core::ptr::null_mut();64]},target_relations:[const{DeviceRelations{count:0,objects:[core::ptr::null_mut();64]}};MAX_CHILDREN]}}}
struct RuntimeCell(UnsafeCell<MaybeUninit<HubRuntime>>);unsafe impl Sync for RuntimeCell{}
static RUNTIME:RuntimeCell=RuntimeCell(UnsafeCell::new(MaybeUninit::uninit()));
static RUNTIME_READY:AtomicBool=AtomicBool::new(false);static RUNTIME_LOCK:AtomicBool=AtomicBool::new(false);
static KERNEL_IO:AtomicU64=AtomicU64::new(0);static DRIVER_OBJECT:AtomicU64=AtomicU64::new(0);
static HUB_DEVICE:AtomicU64=AtomicU64::new(0);static ROOT_HUB:AtomicU64=AtomicU64::new(0);static PORT_COUNT:AtomicU64=AtomicU64::new(0);
static HUB_EVENT_SINK_SLOT:AtomicU64=AtomicU64::new(0);
fn lock(){while RUNTIME_LOCK.compare_exchange_weak(false,true,Ordering::Acquire,Ordering::Relaxed).is_err(){core::hint::spin_loop();}}
fn unlock(){RUNTIME_LOCK.store(false,Ordering::Release);}
unsafe fn runtime_mut()->&'static mut HubRuntime{unsafe{(*RUNTIME.0.get()).assume_init_mut()}}
fn io_table()->Option<&'static KernelIoServiceTable>{let p=KERNEL_IO.load(Ordering::Acquire) as *const KernelIoServiceTable;if p.is_null(){None}else{unsafe{p.as_ref()}}}
fn child_ids(ext:&mut ChildExtension){
 let mut p=0;for b in b"USB\\VID_"{ext.device_id[p]=*b as u16;p+=1;}
 for shift in [12,8,4,0]{ext.device_id[p]=b"0123456789ABCDEF"[((ext.vendor_id>>shift)&15) as usize] as u16;p+=1;}
 for b in b"&PID_"{ext.device_id[p]=*b as u16;p+=1;}
 for shift in [12,8,4,0]{ext.device_id[p]=b"0123456789ABCDEF"[((ext.product_id>>shift)&15) as usize] as u16;p+=1;}ext.device_id[p]=0;
 let mut q=0;for b in b"PORT_"{ext.instance_id[q]=*b as u16;q+=1;}
 ext.instance_id[q]=b"0123456789ABCDEF"[(ext.port>>4) as usize] as u16;q+=1;ext.instance_id[q]=b"0123456789ABCDEF"[(ext.port&15) as usize] as u16;q+=1;ext.instance_id[q]=0;
 let mut h=0;for ch in ext.device_id.iter().take(p){ext.hardware_ids[h]=*ch;h+=1;}ext.hardware_ids[h]=0;h+=1;
 for b in b"USB\\VID_"{ext.hardware_ids[h]=*b as u16;h+=1;}
 for shift in [12,8,4,0]{ext.hardware_ids[h]=b"0123456789ABCDEF"[((ext.vendor_id>>shift)&15) as usize] as u16;h+=1;}ext.hardware_ids[h]=0;h+=1;ext.hardware_ids[h]=0;
 let mut k=0;for b in b"USB\\Class_00&SubClass_00&Prot_00"{ext.compatible_ids[k]=*b as u16;k+=1;}ext.compatible_ids[k]=0;ext.compatible_ids[k+1]=0;
}
fn publish_children()->i32{
 let Some(io)=io_table() else{return STATUS_INVALID_DEVICE_REQUEST};let Some(publish)=io.publish_bus_relations else{return STATUS_INVALID_DEVICE_REQUEST};
 let mut list=[core::ptr::null_mut::<c_void>();MAX_CHILDREN];let mut n=0usize;
 lock();let rt=unsafe{runtime_mut()};for child in rt.children.iter().filter(|c|c.used){list[n]=child.pdo as *mut c_void;n+=1;}unlock();
 unsafe{publish(HUB_DEVICE.load(Ordering::Acquire) as *mut c_void,list.as_ptr(),n as u32)}
}
#[repr(C)]#[derive(Clone,Copy)]struct XhciTrb{parameter:u64,status:u32,control:u32}
unsafe extern "win64" fn hub_event_sink(context:u64,event:*const XhciTrb){
 if context==0||event.is_null()||!RUNTIME_READY.load(Ordering::Acquire){return}
 let e=unsafe{*event};if ((e.control>>10)&0x3f)!=34{return}
 let port=(e.parameter>>24) as usize;if port==0||port>255{return}
 let rt=unsafe{&*(context as *const HubRuntime)};rt.pending_ports[(port-1)/64].fetch_or(1u64<<((port-1)%64),Ordering::Release);
}
#[unsafe(no_mangle)]pub unsafe extern "win64" fn Usbhub3TakePendingPort(port_out:*mut u8)->u8{
 if port_out.is_null()||!RUNTIME_READY.load(Ordering::Acquire){return 0}
 let rt=unsafe{(*RUNTIME.0.get()).as_mut_ptr()};for i in 0..4{let cell=unsafe{&(*rt).pending_ports[i]};let mut v=cell.load(Ordering::Acquire);while v!=0{let bit=v.trailing_zeros() as usize;let mask=1u64<<bit;match cell.compare_exchange_weak(v,v&!mask,Ordering::AcqRel,Ordering::Acquire){Ok(_)=>{unsafe{port_out.write((i*64+bit+1) as u8);}return 1},Err(n)=>v=n}}}0
}
#[unsafe(no_mangle)]pub unsafe extern "win64" fn Usbhub3CreateChildPdo(port:u8,descriptor:*const u8,length:u32,output:*mut *mut c_void)->i32{
 if port==0||port as u64>PORT_COUNT.load(Ordering::Acquire)||descriptor.is_null()||length<18||!RUNTIME_READY.load(Ordering::Acquire){return STATUS_INVALID_PARAMETER}
 let d=unsafe{core::slice::from_raw_parts(descriptor,18)};
 if d[0]<18||d[1]!=1||!matches!(d[7],8|16|32|64|9){return STATUS_INVALID_PARAMETER}
 let vid=u16::from_le_bytes([d[8],d[9]]);let pid=u16::from_le_bytes([d[10],d[11]]);if vid==0||vid==0xffff{return STATUS_INVALID_PARAMETER}
 let Some(io)=io_table() else{return STATUS_INVALID_DEVICE_REQUEST};let (Some(create),Some(mark))=(io.create_device,io.mark_bus_enumerated_pdo) else{return STATUS_INVALID_DEVICE_REQUEST};
 let driver=DRIVER_OBJECT.load(Ordering::Acquire) as *mut c_void;if driver.is_null(){return STATUS_INVALID_DEVICE_REQUEST}
 lock();let rt=unsafe{runtime_mut()};if rt.children.iter().any(|x|x.used&&x.port==port){unlock();return STATUS_INVALID_PARAMETER}let free=rt.children.iter().position(|x|!x.used);unlock();
 let Some(index)=free else{return STATUS_INSUFFICIENT_RESOURCES};let mut pdo=core::ptr::null_mut();
 let status=unsafe{create(driver,core::mem::size_of::<ChildExtension>() as u32,core::ptr::null_mut(),0x22,0,0,&mut pdo)};
 if status<0||pdo.is_null(){return status}
 let ext=unsafe{(*(pdo as *mut DeviceObjectPrefix)).device_extension.cast::<ChildExtension>()};
 if ext.is_null(){if let Some(delete)=io.delete_device{unsafe{delete(pdo);}}return STATUS_INSUFFICIENT_RESOURCES}
 let mut v=ChildExtension{magic:CHILD_MAGIC,version:1,port:port as u32,vendor_id:vid,product_id:pid,descriptor:[0;18],device_id:[0;40],instance_id:[0;24],hardware_ids:[0;80],compatible_ids:[0;80]};
 v.descriptor.copy_from_slice(d);child_ids(&mut v);unsafe{ext.write(v);}
 let st=unsafe{mark(pdo)};if st<0{if let Some(delete)=io.delete_device{unsafe{delete(pdo);}}return st}
 lock();let rt=unsafe{runtime_mut()};rt.children[index]=ChildRecord{used:true,port,pdo:pdo as u64};rt.target_relations[index].count=1;rt.target_relations[index].objects.fill(core::ptr::null_mut());rt.target_relations[index].objects[0]=pdo;unlock();
 let st=publish_children();if st<0{lock();let rt=unsafe{runtime_mut()};rt.children[index]=ChildRecord::EMPTY;rt.target_relations[index].count=0;rt.target_relations[index].objects.fill(core::ptr::null_mut());unlock();if let Some(delete)=io.delete_device{unsafe{delete(pdo);}}return st}
 if !output.is_null(){unsafe{output.write(pdo);}}STATUS_SUCCESS
}
#[unsafe(no_mangle)]pub unsafe extern "win64" fn Usbhub3RemoveChildPdo(port:u8)->i32{
 if port==0{return STATUS_INVALID_PARAMETER}let mut removed=0u64;
 lock();let rt=unsafe{runtime_mut()};if let Some(i)=rt.children.iter().position(|c|c.used&&c.port==port){removed=rt.children[i].pdo;rt.children[i]=ChildRecord::EMPTY;rt.target_relations[i].count=0;rt.target_relations[i].objects.fill(core::ptr::null_mut());}unlock();
 if removed==0{return STATUS_INVALID_PARAMETER}publish_children()
}
fn snapshot_relations()->*const DeviceRelations{
 lock();let rt=unsafe{runtime_mut()};rt.relations.count=0;rt.relations.objects.fill(core::ptr::null_mut());
 for child in rt.children.iter().filter(|c|c.used){let n=rt.relations.count as usize;if n<64{rt.relations.objects[n]=child.pdo as *mut c_void;rt.relations.count+=1;}}
 let p=core::ptr::addr_of!(rt.relations);unlock();p
}
unsafe extern "win64" fn hub_dispatch(device:*mut c_void,irp:*mut c_void)->i32{
 if irp.is_null(){return STATUS_INVALID_PARAMETER}let Some(io)=io_table() else{return STATUS_INVALID_DEVICE_REQUEST};
 let Some(get)=io.current_stack_location else{return STATUS_INVALID_DEVICE_REQUEST};let stack=unsafe{get(irp)};if stack.is_null(){return STATUS_INVALID_PARAMETER}
 let major=unsafe{core::ptr::read(stack.cast::<u8>())};let minor=unsafe{core::ptr::read(stack.cast::<u8>().add(1))};
 let hub=HUB_DEVICE.load(Ordering::Acquire) as *mut c_void;
 if device!=hub{
  let ext=unsafe{(*(device as *mut DeviceObjectPrefix)).device_extension.cast::<ChildExtension>()};
  if ext.is_null()||unsafe{(*ext).magic}!=CHILD_MAGIC{return STATUS_INVALID_DEVICE_REQUEST}
  if major==0x1b{
   let status=match minor{
    0x00|0x01|0x03|0x04|0x05|0x06|0x09|0x0a|0x17=>STATUS_SUCCESS,
    0x13=>{let id=unsafe{core::ptr::read_unaligned(stack.cast::<u8>().add(4).cast::<u32>())};let p=unsafe{match id{0=>(*ext).device_id.as_ptr(),1=>(*ext).instance_id.as_ptr(),2=>(*ext).hardware_ids.as_ptr(),3=>(*ext).compatible_ids.as_ptr(),_=>core::ptr::null()}};if p.is_null(){STATUS_INVALID_DEVICE_REQUEST}else{if let Some(done)=io.complete_irp_status{unsafe{done(irp,STATUS_SUCCESS,p as usize,0);}}return STATUS_SUCCESS}},
    0x07=>{let relation=unsafe{core::ptr::read_unaligned(stack.cast::<u8>().add(4).cast::<u32>())};if relation==1{lock();let rt=unsafe{runtime_mut()};let p=rt.children.iter().position(|c|c.used&&c.pdo==device as u64).map(|i|core::ptr::addr_of!(rt.target_relations[i]));unlock();if let Some(p)=p{if let Some(done)=io.complete_irp_status{unsafe{done(irp,STATUS_SUCCESS,p as usize,0);}}return STATUS_SUCCESS}else{STATUS_INVALID_DEVICE_REQUEST}}else{STATUS_SUCCESS}},
    0x02=>{lock();let rt=unsafe{runtime_mut()};if let Some(i)=rt.children.iter().position(|x|x.used&&x.pdo==device as u64){rt.children[i]=ChildRecord::EMPTY;}unlock();STATUS_SUCCESS},_=>STATUS_INVALID_DEVICE_REQUEST,
   };
   if let Some(done)=io.complete_irp_status{unsafe{done(irp,status,0,0);}}return status
  }
  if let Some(done)=io.complete_irp_status{unsafe{done(irp,STATUS_INVALID_DEVICE_REQUEST,0,0);}}return STATUS_INVALID_DEVICE_REQUEST
 }
 if major==0x1b&&minor==0x07{
  let relation=unsafe{core::ptr::read_unaligned(stack.cast::<u8>().add(4).cast::<u32>())};
  if relation==0{let p=snapshot_relations();if let Some(done)=io.complete_irp_status{unsafe{done(irp,STATUS_SUCCESS,p as usize,0);}}return STATUS_SUCCESS}
 }
 let ext=unsafe{(*(hub as *mut DeviceObjectPrefix)).device_extension.cast::<HubFdoExtension>()};
 let lower=if ext.is_null()||unsafe{(*ext).magic}!=EXT_MAGIC{core::ptr::null_mut()}else{unsafe{(*ext).lower_device}};
 if lower.is_null(){if let Some(done)=io.complete_irp_status{unsafe{done(irp,STATUS_INVALID_DEVICE_REQUEST,0,0);}}return STATUS_INVALID_DEVICE_REQUEST}
 if let Some(skip)=io.skip_current_stack{unsafe{skip(irp);}}
 if let Some(call)=io.call_driver{
  let status=unsafe{call(lower,irp)};
  if major==0x1b&&minor==0x02&&status!=0x103{
   if let Some(detach)=io.detach_device{unsafe{detach(lower);}}
   if let Some(delete)=io.delete_device{unsafe{delete(hub);}}
   HUB_DEVICE.store(0,Ordering::Release);
  }
  status
 }else{STATUS_INVALID_DEVICE_REQUEST}
}
unsafe extern "win64" fn hub_unload(_driver:*mut c_void){
 if !RUNTIME_READY.swap(false,Ordering::AcqRel){return}
 let sink=HUB_EVENT_SINK_SLOT.load(Ordering::Acquire) as *mut AtomicU64;if !sink.is_null(){unsafe{(*sink).store(0,Ordering::Release);}}
 let Some(io)=io_table() else{return};let hub=HUB_DEVICE.swap(0,Ordering::AcqRel);
 if hub!=0{let ext=unsafe{(*(hub as *mut DeviceObjectPrefix)).device_extension.cast::<HubFdoExtension>()};if !ext.is_null(){let lower=unsafe{(*ext).lower_device};if !lower.is_null(){if let Some(detach)=io.detach_device{unsafe{detach(lower);}}}}if let Some(delete)=io.delete_device{unsafe{delete(hub as *mut c_void);}}}
}
#[unsafe(no_mangle)]pub unsafe extern "win64" fn DriverEntry(driver:PVOID,_registry_path:PVOID)->i32{
 if driver.is_null(){return STATUS_INVALID_PARAMETER}let object=unsafe{&mut *(driver as *mut DriverObject)};
 let Some(ext)=(unsafe{object.driver_extension.as_ref()}) else{return STATUS_INVALID_PARAMETER};
 if ext.magic!=EXT_MAGIC||ext.version!=1||ext.size as usize<core::mem::size_of::<HubDriverExtension>()||ext.kernel_io.is_null()||ext.root_hub_device==0||ext.port_count==0||ext.port_count>255||ext.event_sink.is_null()||ext.event_context.is_null()||ext.hub_device_out.is_null()||ext.child_factory_out.is_null(){return STATUS_INVALID_PARAMETER}
 let io=unsafe{&*ext.kernel_io};if !io.compatible(){return STATUS_INVALID_DEVICE_REQUEST}
 KERNEL_IO.store(ext.kernel_io as u64,Ordering::Release);ROOT_HUB.store(ext.root_hub_device,Ordering::Release);PORT_COUNT.store(ext.port_count as u64,Ordering::Release);DRIVER_OBJECT.store(driver as u64,Ordering::Release);
 let runtime_ptr=unsafe{(*RUNTIME.0.get()).as_mut_ptr()};unsafe{runtime_ptr.write(HubRuntime::new());}RUNTIME_READY.store(true,Ordering::Release);
 static NAME:[u16;22]=[92,68,101,118,105,99,101,92,67,111,103,110,97,99,85,115,98,72,117,98,51,0];
 let mut name=cognacos_nt_abi::object::UNICODE_STRING{length:42,maximum_length:44,buffer:NAME.as_ptr() as *mut u16};
 for f in object.major_function.iter_mut(){*f=Some(hub_dispatch);}object.driver_unload=Some(hub_unload);
 let Some(create)=io.create_device else{return STATUS_INVALID_DEVICE_REQUEST};let mut hub=core::ptr::null_mut();
 let st=unsafe{create(driver,core::mem::size_of::<HubFdoExtension>() as u32,&mut name,0x22,0,0,&mut hub)};
 if st<0||hub.is_null(){RUNTIME_READY.store(false,Ordering::Release);return st}
 let de=unsafe{(*(hub as *mut DeviceObjectPrefix)).device_extension.cast::<HubFdoExtension>()};
 if de.is_null(){if let Some(delete)=io.delete_device{unsafe{delete(hub);}}RUNTIME_READY.store(false,Ordering::Release);return STATUS_INSUFFICIENT_RESOURCES}
 let Some(attach)=io.attach_device else{if let Some(delete)=io.delete_device{unsafe{delete(hub);}}return STATUS_INVALID_DEVICE_REQUEST};
 let lower=unsafe{attach(hub,ext.root_hub_device as *mut c_void)};
 if lower.is_null(){if let Some(delete)=io.delete_device{unsafe{delete(hub);}}RUNTIME_READY.store(false,Ordering::Release);return STATUS_INVALID_DEVICE_REQUEST}
 unsafe{de.write(HubFdoExtension{magic:EXT_MAGIC,lower_device:lower,root_hub:ext.root_hub_device as *mut c_void,port_count:ext.port_count});}
 HUB_DEVICE.store(hub as u64,Ordering::Release);HUB_EVENT_SINK_SLOT.store(ext.event_sink as u64,Ordering::Release);
 unsafe{(*ext.event_context).store(runtime_ptr as u64,Ordering::Release);(*ext.event_sink).store(hub_event_sink as usize as u64,Ordering::Release);ext.hub_device_out.write(hub as u64);ext.child_factory_out.write(Usbhub3CreateChildPdo as usize as u64);}
 STATUS_SUCCESS
}
#[used]#[unsafe(link_section=".rodata")]static DRIVER_SIGNATURE:[u8;8]=*b"USBHUB3_";
#[panic_handler]fn panic(_: &core::panic::PanicInfo<'_>)->!{loop{unsafe{core::arch::asm!("cli; hlt",options(nomem,nostack));}}}
