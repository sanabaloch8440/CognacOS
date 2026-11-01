// WDM KINTERRUPT-compatible connection and teardown exports.
const INTERRUPT_SIGNATURE:u64=0x434f_474e_494e_5431;
pub type InterruptServiceRoutine=unsafe extern "win64" fn(*mut core::ffi::c_void,*mut core::ffi::c_void)->u8;
#[repr(C)]
pub struct InterruptObject{
    signature:core::sync::atomic::AtomicU64,vector:core::sync::atomic::AtomicU32,
    irql:core::sync::atomic::AtomicU8,_reserved:[u8;3],active:core::sync::atomic::AtomicBool,
    routine:core::sync::atomic::AtomicUsize,context:core::sync::atomic::AtomicUsize,
}
unsafe impl Sync for InterruptObject{}
impl InterruptObject{
 const EMPTY:Self=Self{signature:core::sync::atomic::AtomicU64::new(0),vector:core::sync::atomic::AtomicU32::new(0),
  irql:core::sync::atomic::AtomicU8::new(0),_reserved:[0;3],active:core::sync::atomic::AtomicBool::new(false),
  routine:core::sync::atomic::AtomicUsize::new(0),context:core::sync::atomic::AtomicUsize::new(0)};
}
static INTERRUPT_OBJECTS:[InterruptObject;256]=[const{InterruptObject::EMPTY};256];
unsafe extern "win64" fn io_interrupt_dispatch(_vector:u8,_frame:*mut core::ffi::c_void,context:*mut core::ffi::c_void)->bool{
 if context.is_null(){return false}
 let object=unsafe{&*(context.cast::<InterruptObject>())};
 if object.signature.load(core::sync::atomic::Ordering::Acquire)!=INTERRUPT_SIGNATURE||!object.active.load(core::sync::atomic::Ordering::Acquire){return false}
 let raw=object.routine.load(core::sync::atomic::Ordering::Acquire);if raw==0{return false}
 let routine:InterruptServiceRoutine=unsafe{core::mem::transmute(raw)};
 let device_context=object.context.load(core::sync::atomic::Ordering::Acquire) as *mut core::ffi::c_void;
 unsafe{routine(context,device_context)!=0}
}
#[unsafe(export_name="IoConnectInterrupt")]
pub unsafe extern "win64" fn io_connect_interrupt(
 interrupt_object_out:*mut *mut core::ffi::c_void,service_routine:Option<InterruptServiceRoutine>,
 service_context:*mut core::ffi::c_void,_spin_lock:*mut core::ffi::c_void,vector:u32,irql:u8,
 synchronize_irql:u8,interrupt_mode:u32,share_vector:u8,processor_enable_mask:u64,floating_save:u8,
)->i32{
 const STATUS_INVALID_PARAMETER:i32=0xC000_000D_u32 as i32;
 const STATUS_NOT_SUPPORTED:i32=0xC000_00BB_u32 as i32;
 const STATUS_INSUFFICIENT_RESOURCES:i32=0xC000_009A_u32 as i32;
 if interrupt_object_out.is_null()||service_context.is_null()||service_routine.is_none()
  ||!(0x20..=0xdf).contains(&vector)||irql==0||synchronize_irql>irql||interrupt_mode>1||processor_enable_mask==0{return STATUS_INVALID_PARAMETER}
 unsafe{interrupt_object_out.write(core::ptr::null_mut())}
 if share_vector!=0||floating_save!=0{return STATUS_NOT_SUPPORTED}
 let slot=&INTERRUPT_OBJECTS[vector as usize];
 if slot.active.compare_exchange(false,true,core::sync::atomic::Ordering::AcqRel,core::sync::atomic::Ordering::Acquire).is_err(){return STATUS_INSUFFICIENT_RESOURCES}
 slot.signature.store(INTERRUPT_SIGNATURE,core::sync::atomic::Ordering::Release);
 slot.vector.store(vector,core::sync::atomic::Ordering::Relaxed);slot.irql.store(irql,core::sync::atomic::Ordering::Relaxed);
 slot.context.store(service_context as usize,core::sync::atomic::Ordering::Relaxed);
 slot.routine.store(service_routine.unwrap() as usize,core::sync::atomic::Ordering::Release);
 let object=slot as *const InterruptObject as *mut core::ffi::c_void;
 if crate::hal::register_device_interrupt_handler(vector as u8,io_interrupt_dispatch,object.cast()).is_err(){
  slot.active.store(false,core::sync::atomic::Ordering::Release);slot.routine.store(0,core::sync::atomic::Ordering::Release);
  slot.context.store(0,core::sync::atomic::Ordering::Release);return STATUS_NOT_SUPPORTED;
 }
 unsafe{interrupt_object_out.write(object)};0
}
#[unsafe(export_name="IoDisconnectInterrupt")]
pub unsafe extern "win64" fn io_disconnect_interrupt(interrupt_object:*mut core::ffi::c_void){
 if interrupt_object.is_null(){return}
 let object=interrupt_object.cast::<InterruptObject>();
 let Some((vector,slot))=INTERRUPT_OBJECTS.iter().enumerate().find(|(_,candidate)|core::ptr::eq(*candidate as *const InterruptObject,object as *const InterruptObject)) else{return};
 if slot.signature.load(core::sync::atomic::Ordering::Acquire)!=INTERRUPT_SIGNATURE||!slot.active.swap(false,core::sync::atomic::Ordering::AcqRel){return}
 let _=unsafe{crate::hal::set_device_interrupt_vector_enabled(vector as u8,false)};
 let _=crate::hal::release_device_interrupt_vector(vector as u8);
 slot.routine.store(0,core::sync::atomic::Ordering::Release);slot.context.store(0,core::sync::atomic::Ordering::Release);
 slot.signature.store(0,core::sync::atomic::Ordering::Release);
}
