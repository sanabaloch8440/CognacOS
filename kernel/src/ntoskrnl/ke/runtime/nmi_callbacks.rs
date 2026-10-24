// Lock-free NMI callback dispatch. NMI can interrupt code holding ordinary
// kernel locks, so the dispatch path must never acquire a spin lock.
use core::sync::atomic::{AtomicU8,AtomicUsize};
const MAX_NMI_CALLBACKS:usize=64;
type NmiRoutine=unsafe extern "win64" fn(*mut core::ffi::c_void,u8)->u8;
static NMI_STATE:[AtomicU8;MAX_NMI_CALLBACKS]=[const{AtomicU8::new(0)};MAX_NMI_CALLBACKS];
static NMI_ROUTINE:[AtomicUsize;MAX_NMI_CALLBACKS]=[const{AtomicUsize::new(0)};MAX_NMI_CALLBACKS];
static NMI_CONTEXT:[AtomicUsize;MAX_NMI_CALLBACKS]=[const{AtomicUsize::new(0)};MAX_NMI_CALLBACKS];
static NMI_IN_FLIGHT:[AtomicUsize;MAX_NMI_CALLBACKS]=[const{AtomicUsize::new(0)};MAX_NMI_CALLBACKS];

/// PVOID KeRegisterNmiCallback(PNMI_CALLBACK CallbackRoutine, PVOID Context).
pub unsafe extern "win64" fn ke_register_nmi_callback(
 callback:Option<NmiRoutine>,context:*mut core::ffi::c_void,
)->*mut core::ffi::c_void{
 let Some(callback)=callback else{return core::ptr::null_mut()};
 for slot in 0..MAX_NMI_CALLBACKS{
  if NMI_IN_FLIGHT[slot].load(Ordering::Acquire)!=0{continue}
  if NMI_STATE[slot].compare_exchange(0,1,Ordering::AcqRel,Ordering::Acquire).is_ok(){
   NMI_ROUTINE[slot].store(callback as usize,Ordering::Relaxed);
   NMI_CONTEXT[slot].store(context as usize,Ordering::Relaxed);
   NMI_STATE[slot].store(2,Ordering::Release);
   return (slot+1) as *mut core::ffi::c_void;
  }
 }
 core::ptr::null_mut()
}

/// VOID KeDeregisterNmiCallback(PVOID Handle).
pub unsafe extern "win64" fn ke_deregister_nmi_callback(handle:*mut core::ffi::c_void){
 let raw=handle as usize;
 if raw==0||raw>MAX_NMI_CALLBACKS{return}
 let slot=raw-1;
 if NMI_STATE[slot].swap(0,Ordering::AcqRel)!=2{return}
 while NMI_IN_FLIGHT[slot].load(Ordering::Acquire)!=0{core::hint::spin_loop();}
 NMI_ROUTINE[slot].store(0,Ordering::Relaxed);
 NMI_CONTEXT[slot].store(0,Ordering::Relaxed);
}

/// Run callbacks in registration-slot order without locks. Returns whether any
/// callback claimed the NMI; the exception dispatcher retains fatal handling if
/// no registered callback handles the interrupt.
pub fn dispatch_nmi_callbacks()->bool{
 let mut handled=0u8;
 for slot in 0..MAX_NMI_CALLBACKS{
  if NMI_STATE[slot].load(Ordering::Acquire)!=2{continue}
  NMI_IN_FLIGHT[slot].fetch_add(1,Ordering::AcqRel);
  if NMI_STATE[slot].load(Ordering::Acquire)==2{
   let routine=NMI_ROUTINE[slot].load(Ordering::Acquire);
   let context=NMI_CONTEXT[slot].load(Ordering::Acquire);
   if routine!=0{
    let callback:NmiRoutine=unsafe{core::mem::transmute(routine)};
    handled=unsafe{callback(context as *mut _,handled)};
   }
  }
  NMI_IN_FLIGHT[slot].fetch_sub(1,Ordering::AcqRel);
 }
 handled!=0
}

#[cfg(test)]
mod tests{
 use super::*;
 static CALLS:AtomicUsize=AtomicUsize::new(0);
 unsafe extern "win64" fn callback(_: *mut core::ffi::c_void,handled:u8)->u8{
  CALLS.fetch_add(1,Ordering::AcqRel); if handled!=0{handled}else{1}
 }
 #[test]
 fn nmi_callback_register_dispatch_deregister(){
  CALLS.store(0,Ordering::Release);
  let handle=unsafe{ke_register_nmi_callback(Some(callback),core::ptr::null_mut())};
  assert!(!handle.is_null());
  assert!(dispatch_nmi_callbacks());
  assert_eq!(CALLS.load(Ordering::Acquire),1);
  unsafe{ke_deregister_nmi_callback(handle);}
  assert!(!dispatch_nmi_callbacks());
  assert_eq!(CALLS.load(Ordering::Acquire),1);
 }
}
