static IO_CANCEL_SPIN_LOCK:core::sync::atomic::AtomicBool=core::sync::atomic::AtomicBool::new(false);

#[unsafe(export_name = "IoAcquireCancelSpinLock")]
pub unsafe extern "win64" fn io_acquire_cancel_spin_lock(old_irql:*mut u8){
 if old_irql.is_null(){return}
 let old=unsafe{crate::ntoskrnl::ke::runtime::ke_raise_irql_to_dpc_level()};
 while IO_CANCEL_SPIN_LOCK.compare_exchange_weak(false,true,core::sync::atomic::Ordering::Acquire,core::sync::atomic::Ordering::Relaxed).is_err(){core::hint::spin_loop();}
 unsafe{old_irql.write(old);}
}
#[unsafe(export_name = "IoReleaseCancelSpinLock")]
pub unsafe extern "win64" fn io_release_cancel_spin_lock(old_irql:u8){
 IO_CANCEL_SPIN_LOCK.store(false,core::sync::atomic::Ordering::Release);
 unsafe{crate::ntoskrnl::ke::runtime::ke_lower_irql(old_irql);}
}

#[repr(C)]
pub struct IoCsq {
 pub type_:u32,pub padding:u32,
 pub insert_irp:*mut c_void,pub remove_irp:*mut c_void,pub peek_next_irp:*mut c_void,
 pub acquire_lock:*mut c_void,pub release_lock:*mut c_void,pub complete_canceled_irp:*mut c_void,
 pub reserve_pointer:*mut c_void,
}
#[repr(C)]
pub struct IoCsqIrpContext { pub type_:u32,pub padding:u32,pub irp:*mut Irp,pub csq:*mut IoCsq }
pub type CsqInsert=unsafe extern "win64" fn(*mut IoCsq,*mut Irp);
pub type CsqInsertEx=unsafe extern "win64" fn(*mut IoCsq,*mut Irp,*mut c_void)->i32;
pub type CsqRemove=unsafe extern "win64" fn(*mut IoCsq,*mut Irp);
pub type CsqPeek=unsafe extern "win64" fn(*mut IoCsq,*mut Irp,*mut c_void)->*mut Irp;
pub type CsqAcquire=unsafe extern "win64" fn(*mut IoCsq,*mut u8);
pub type CsqRelease=unsafe extern "win64" fn(*mut IoCsq,u8);
pub type CsqCompleteCanceled=unsafe extern "win64" fn(*mut IoCsq,*mut Irp);
const CSQ_IRP_CONTEXT_OFFSET:usize=0x90;
const IO_TYPE_CSQ:u32=2;
const IO_TYPE_CSQ_EX:u32=3;
const CSQ_CONTEXT_TYPE:u32=1;

unsafe fn csq_callback<T:Copy>(csq:*mut IoCsq,offset:usize)->T {
 unsafe{ptr::read_unaligned((csq.cast::<u8>().add(offset)).cast::<T>())}
}
unsafe fn csq_lock(csq:*mut IoCsq)->u8 {
 let acquire:CsqAcquire=unsafe{core::mem::transmute(csq_callback::<*mut c_void>(csq,32))};
 let mut irql=0u8;unsafe{acquire(csq,&mut irql)};irql
}
unsafe fn csq_unlock(csq:*mut IoCsq,irql:u8) {
 let release:CsqRelease=unsafe{core::mem::transmute(csq_callback::<*mut c_void>(csq,40))};
 unsafe{release(csq,irql)}
}
unsafe fn csq_set_cancel_routine(irp:*mut Irp,routine:*mut c_void)->*mut c_void {
 let field=unsafe{ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(IRP_CANCEL_ROUTINE_OFFSET).cast::<core::sync::atomic::AtomicPtr<c_void>>()};
 unsafe{(&*field).swap(routine,core::sync::atomic::Ordering::AcqRel)}
}
unsafe extern "win64" fn csq_cancel_routine(_device:*mut DeviceObject,irp:*mut Irp) {
 if irp.is_null(){return}
 let csq=unsafe{read_ptr(ptr::addr_of!((*irp).bytes).cast::<u8>(),CSQ_IRP_CONTEXT_OFFSET).cast::<IoCsq>()};
 if csq.is_null(){return}
 let mut old_irql=0u8;unsafe{io_acquire_cancel_spin_lock(&mut old_irql);}
 let queue_irql=unsafe{csq_lock(csq)};
 let remove:CsqRemove=unsafe{core::mem::transmute(csq_callback::<*mut c_void>(csq,16))};
 unsafe{remove(csq,irp);}
 write_ptr(unsafe{ptr::addr_of_mut!((*irp).bytes).cast::<u8>()},CSQ_IRP_CONTEXT_OFFSET,ptr::null_mut());
 unsafe{csq_unlock(csq,queue_irql);}
 unsafe{io_release_cancel_spin_lock(old_irql);}
 let complete:CsqCompleteCanceled=unsafe{core::mem::transmute(csq_callback::<*mut c_void>(csq,48))};
 unsafe{complete(csq,irp);}
}
unsafe fn csq_initialize_common(csq:*mut IoCsq,insert:*mut c_void,extended:bool,remove:*mut c_void,peek:*mut c_void,acquire:*mut c_void,release:*mut c_void,complete:*mut c_void)->i32 {
 if csq.is_null()||insert.is_null()||remove.is_null()||peek.is_null()||acquire.is_null()||release.is_null()||complete.is_null(){return STATUS_INVALID_PARAMETER}
 unsafe{
  ptr::write_unaligned(ptr::addr_of_mut!((*csq).type_),if extended {IO_TYPE_CSQ_EX}else{IO_TYPE_CSQ});
  ptr::write_unaligned(ptr::addr_of_mut!((*csq).padding),0);
  ptr::write_unaligned(ptr::addr_of_mut!((*csq).insert_irp),insert);
  ptr::write_unaligned(ptr::addr_of_mut!((*csq).remove_irp),remove);
  ptr::write_unaligned(ptr::addr_of_mut!((*csq).peek_next_irp),peek);
  ptr::write_unaligned(ptr::addr_of_mut!((*csq).acquire_lock),acquire);
  ptr::write_unaligned(ptr::addr_of_mut!((*csq).release_lock),release);
  ptr::write_unaligned(ptr::addr_of_mut!((*csq).complete_canceled_irp),complete);
  ptr::write_unaligned(ptr::addr_of_mut!((*csq).reserve_pointer),ptr::null_mut());
 }
 STATUS_SUCCESS
}
#[unsafe(export_name = "IoCsqInitialize")]
pub unsafe extern "win64" fn io_csq_initialize(csq:*mut IoCsq,insert:Option<CsqInsert>,remove:Option<CsqRemove>,peek:Option<CsqPeek>,acquire:Option<CsqAcquire>,release:Option<CsqRelease>,complete:Option<CsqCompleteCanceled>)->i32 {
 let (Some(insert),Some(remove),Some(peek),Some(acquire),Some(release),Some(complete))=(insert,remove,peek,acquire,release,complete) else{return STATUS_INVALID_PARAMETER};
 unsafe{csq_initialize_common(csq,insert as *const () as *mut c_void,false,remove as *const () as *mut c_void,peek as *const () as *mut c_void,acquire as *const () as *mut c_void,release as *const () as *mut c_void,complete as *const () as *mut c_void)}
}
#[unsafe(export_name = "IoCsqInitializeEx")]
pub unsafe extern "win64" fn io_csq_initialize_ex(csq:*mut IoCsq,insert:Option<CsqInsertEx>,remove:Option<CsqRemove>,peek:Option<CsqPeek>,acquire:Option<CsqAcquire>,release:Option<CsqRelease>,complete:Option<CsqCompleteCanceled>)->i32 {
 let (Some(insert),Some(remove),Some(peek),Some(acquire),Some(release),Some(complete))=(insert,remove,peek,acquire,release,complete) else{return STATUS_INVALID_PARAMETER};
 unsafe{csq_initialize_common(csq,insert as *const () as *mut c_void,true,remove as *const () as *mut c_void,peek as *const () as *mut c_void,acquire as *const () as *mut c_void,release as *const () as *mut c_void,complete as *const () as *mut c_void)}
}
unsafe fn csq_insert_impl(csq:*mut IoCsq,irp:*mut Irp,context:*mut IoCsqIrpContext,insert_context:*mut c_void,extended_api:bool)->i32 {
 if csq.is_null()||irp.is_null(){return STATUS_INVALID_PARAMETER}
 let extended=unsafe{ptr::read_unaligned(csq.cast::<u8>().cast::<u32>())}==IO_TYPE_CSQ_EX;
 let mut old_irql=0u8;unsafe{io_acquire_cancel_spin_lock(&mut old_irql);}
 let queue_irql=unsafe{csq_lock(csq)};
 let cancelled=unsafe{*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x44)!=0};
 let mut status=STATUS_SUCCESS;
 if !cancelled {
  write_ptr(unsafe{ptr::addr_of_mut!((*irp).bytes).cast::<u8>()},CSQ_IRP_CONTEXT_OFFSET,csq.cast());
  let routine=csq_cancel_routine as *const () as *mut c_void;
  unsafe{csq_set_cancel_routine(irp,routine);}
  if extended {
   let insert:CsqInsertEx=unsafe{core::mem::transmute(csq_callback::<*mut c_void>(csq,8))};
   status=unsafe{insert(csq,irp,insert_context)};
  } else {
   let insert:CsqInsert=unsafe{core::mem::transmute(csq_callback::<*mut c_void>(csq,8))};
   unsafe{insert(csq,irp);}
  }
  if status<0 {
   unsafe{csq_set_cancel_routine(irp,ptr::null_mut());}
   write_ptr(unsafe{ptr::addr_of_mut!((*irp).bytes).cast::<u8>()},CSQ_IRP_CONTEXT_OFFSET,ptr::null_mut());
  } else {
   if !context.is_null() {
    unsafe{ptr::write_unaligned(ptr::addr_of_mut!((*context).type_),CSQ_CONTEXT_TYPE);}
    unsafe{ptr::write_unaligned(ptr::addr_of_mut!((*context).irp),irp);}
    unsafe{ptr::write_unaligned(ptr::addr_of_mut!((*context).csq),csq);}
   }
   // Mark pending while the cancel spin lock still excludes IoCancelIrp; a
   // cancellation callback can otherwise complete/free this packet first.
   if status>=0 {unsafe{io_mark_irp_pending(irp);}}
  }
 }
 unsafe{csq_unlock(csq,queue_irql);}
 unsafe{io_release_cancel_spin_lock(old_irql);}
 if cancelled {
  let complete:CsqCompleteCanceled=unsafe{core::mem::transmute(csq_callback::<*mut c_void>(csq,48))};
  unsafe{complete(csq,irp);}
  return 0xC0000120u32 as i32;
 }
 if extended_api {status}else{STATUS_SUCCESS}
}
#[unsafe(export_name = "IoCsqInsertIrp")]
pub unsafe extern "win64" fn io_csq_insert_irp(csq:*mut IoCsq,irp:*mut Irp,context:*mut IoCsqIrpContext) {
 let _=unsafe{csq_insert_impl(csq,irp,context,ptr::null_mut(),false)};
}
#[unsafe(export_name = "IoCsqInsertIrpEx")]
pub unsafe extern "win64" fn io_csq_insert_irp_ex(csq:*mut IoCsq,irp:*mut Irp,context:*mut IoCsqIrpContext,insert_context:*mut c_void)->i32 {
 unsafe{csq_insert_impl(csq,irp,context,insert_context,true)}
}
#[unsafe(export_name = "IoCsqRemoveIrp")]
pub unsafe extern "win64" fn io_csq_remove_irp(csq:*mut IoCsq,context:*mut IoCsqIrpContext)->*mut Irp {
 if csq.is_null()||context.is_null(){return ptr::null_mut()}
 let context_type=unsafe{ptr::read_unaligned(ptr::addr_of!((*context).type_))};
 let irp=unsafe{ptr::read_unaligned(ptr::addr_of!((*context).irp))};
 if context_type!=CSQ_CONTEXT_TYPE||irp.is_null()||unsafe{ptr::read_unaligned(ptr::addr_of!((*context).csq))}!=csq{return ptr::null_mut()}
 let mut old_irql=0u8;unsafe{io_acquire_cancel_spin_lock(&mut old_irql);}
 let queue_irql=unsafe{csq_lock(csq)};
 let cancel=unsafe{*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x44)!=0};
 let routine=unsafe{read_ptr(ptr::addr_of!((*irp).bytes).cast::<u8>(),IRP_CANCEL_ROUTINE_OFFSET)};
 let queue=unsafe{read_ptr(ptr::addr_of!((*irp).bytes).cast::<u8>(),CSQ_IRP_CONTEXT_OFFSET)};
 if cancel||routine!=(csq_cancel_routine as *const () as *mut c_void)||queue!=csq.cast(){
  unsafe{csq_unlock(csq,queue_irql);}unsafe{io_release_cancel_spin_lock(old_irql);}
  return ptr::null_mut()
 }
 unsafe{csq_set_cancel_routine(irp,ptr::null_mut());}
 let remove:CsqRemove=unsafe{core::mem::transmute(csq_callback::<*mut c_void>(csq,16))};
 unsafe{remove(csq,irp);}
 write_ptr(unsafe{ptr::addr_of_mut!((*irp).bytes).cast::<u8>()},CSQ_IRP_CONTEXT_OFFSET,ptr::null_mut());
 unsafe{csq_unlock(csq,queue_irql);}unsafe{io_release_cancel_spin_lock(old_irql);}
 unsafe{
  ptr::write_unaligned(ptr::addr_of_mut!((*context).type_),0);
  ptr::write_unaligned(ptr::addr_of_mut!((*context).irp),ptr::null_mut());
  ptr::write_unaligned(ptr::addr_of_mut!((*context).csq),ptr::null_mut());
 }
 irp
}
#[unsafe(export_name = "IoCsqRemoveNextIrp")]
pub unsafe extern "win64" fn io_csq_remove_next_irp(csq:*mut IoCsq,peek_context:*mut c_void)->*mut Irp {
 if csq.is_null(){return ptr::null_mut()}
 let mut old_irql=0u8;unsafe{io_acquire_cancel_spin_lock(&mut old_irql);}
 let queue_irql=unsafe{csq_lock(csq)};
 let peek:CsqPeek=unsafe{core::mem::transmute(csq_callback::<*mut c_void>(csq,24))};
 let candidate=unsafe{peek(csq,ptr::null_mut(),peek_context)};
 if candidate.is_null()||unsafe{*ptr::addr_of!((*candidate).bytes).cast::<u8>().add(0x44)!=0} {
  unsafe{csq_unlock(csq,queue_irql);}unsafe{io_release_cancel_spin_lock(old_irql);}
  return ptr::null_mut()
 }
 let routine=unsafe{read_ptr(ptr::addr_of!((*candidate).bytes).cast::<u8>(),IRP_CANCEL_ROUTINE_OFFSET)};
 let queue=unsafe{read_ptr(ptr::addr_of!((*candidate).bytes).cast::<u8>(),CSQ_IRP_CONTEXT_OFFSET)};
 if routine!=(csq_cancel_routine as *const () as *mut c_void)||queue!=csq.cast(){
  unsafe{csq_unlock(csq,queue_irql);}unsafe{io_release_cancel_spin_lock(old_irql);}
  return ptr::null_mut()
 }
 unsafe{csq_set_cancel_routine(candidate,ptr::null_mut());}
 let remove:CsqRemove=unsafe{core::mem::transmute(csq_callback::<*mut c_void>(csq,16))};
 unsafe{remove(csq,candidate);}
 write_ptr(unsafe{ptr::addr_of_mut!((*candidate).bytes).cast::<u8>()},CSQ_IRP_CONTEXT_OFFSET,ptr::null_mut());
 unsafe{csq_unlock(csq,queue_irql);}unsafe{io_release_cancel_spin_lock(old_irql);}
 candidate
}

const IRP_CANCEL_ROUTINE_OFFSET:usize=0x68;
/// Atomically replace the IRP cancel callback and return the previous callback.
#[unsafe(export_name = "IoSetCancelRoutine")]
pub unsafe extern "win64" fn io_set_cancel_routine(
 irp:*mut Irp, routine:Option<DriverCancelRoutine>
)->Option<DriverCancelRoutine> {
 if irp.is_null()||!acquire_irp_active_op(irp){return None}
 let slot=unsafe{ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(IRP_CANCEL_ROUTINE_OFFSET).cast::<core::sync::atomic::AtomicPtr<c_void>>()};
 let replacement=routine.map_or(ptr::null_mut(),|r|r as *const () as *mut c_void);
 let previous=unsafe{&*slot}.swap(replacement,core::sync::atomic::Ordering::AcqRel);
 release_irp_active_op(irp);
 if previous.is_null(){None}else{Some(unsafe{core::mem::transmute::<*mut c_void,DriverCancelRoutine>(previous)})}
}

/// Cancel all builder-owned synchronous IRPs associated with an exiting thread.
/// Detach stack-backed IOSB/event pointers before cancellation: a lower driver
/// may finish later if it has no cancel routine or cannot stop I/O immediately.
pub fn io_cancel_thread_irps(thread_id:u64)->usize {
 if thread_id==0{return 0}
 let mut total=0usize;
 loop {
  let mut pending=[(ptr::null_mut::<Irp>(),false);IRP_CANCEL_BATCH];
  let mut count=0usize;
  let mut completion_in_progress=false;
  let mut more=false;
  {
   let mut state=WDM.lock();
   let Some(qi)=state.thread_irps.iter().position(|q|q.used&&q.thread_id==thread_id) else{break};
   let mut cursor=state.thread_irps[qi].head;
   while !cursor.is_null(){
    let Some(index)=state.irps.iter().position(|slot|slot.used&&ptr::eq(&slot.object,cursor)) else{break};
    let next=state.irps[index].sync_next;
    if state.irps[index].completion_state==1||state.irps[index].completion_state==3||state.irps[index].active_ops!=0 {
     completion_in_progress=true;cursor=next;continue
    }
    if count==pending.len(){more=true;break}
    let never_dispatched=state.irps[index].outstanding_count==0&&state.irps[index].completion_state==0;
    let _=sync_queue_detach_locked(&mut state,cursor);
    unsafe { write_ptr(ptr::addr_of_mut!((*cursor).bytes).cast::<u8>(),0x48,ptr::null_mut()); }
    pending[count]=(cursor,never_dispatched);count+=1;cursor=next;
   }
   more|=state.thread_irps.iter().any(|q|q.used&&q.thread_id==thread_id);
  }
  for (irp,never_dispatched) in pending[..count].iter().copied(){
   let cancel_routine_called=unsafe{io_cancel_irp(irp)}!=0;
   if never_dispatched&&!cancel_routine_called {
    unsafe{
     ptr::write_unaligned(ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(IRP_STATUS_OFFSET).cast::<i32>(),0xC0000120u32 as i32);
     ptr::write_unaligned(ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(IRP_INFORMATION_OFFSET).cast::<usize>(),0);
     io_complete_request(irp,0);
    }
   }
  }
  total+=count;
  if more {
   if count==0&&completion_in_progress{core::hint::spin_loop();}
   continue
  }
  if !completion_in_progress {break}
  core::hint::spin_loop();
 }
 total
}

/// Request cancellation using the global cancel-spin-lock protocol. A driver
/// callback is invoked only after the lock and raised IRQL have been released.
#[unsafe(export_name = "IoCancelIrp")]
pub unsafe extern "win64" fn io_cancel_irp(irp:*mut Irp)->u8 {
 if irp.is_null(){return 0}
 if !acquire_irp_active_op(irp){return 0}
 let mut old_irql=0u8;
 unsafe{io_acquire_cancel_spin_lock(&mut old_irql);}
 unsafe{*ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(0x44)=1;}
 let cancel_slot=unsafe{ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(IRP_CANCEL_ROUTINE_OFFSET).cast::<core::sync::atomic::AtomicPtr<c_void>>()};
 let callback=unsafe{&*cancel_slot}.swap(ptr::null_mut(),core::sync::atomic::Ordering::AcqRel);
 unsafe{io_release_cancel_spin_lock(old_irql);}
 if callback.is_null(){release_irp_active_op(irp);try_free_completed_sync_ioctl_irp(irp);return 0}
 let routine:DriverCancelRoutine=unsafe{core::mem::transmute(callback)};
 let device={
  let state=WDM.lock();
  if let Some(slot)=state.irps.iter().find(|s|s.used&&ptr::eq(&s.object,irp)) {
   let count=slot.object.bytes[0x42] as usize;
   let stack=unsafe{io_get_current_irp_stack_location(irp)};
   let base=slot.stack.as_ptr() as usize;
   let end=base.saturating_add(count*IO_STACK_LOCATION_SIZE_X64);
   let cur=stack as usize;
   if count>0&&count<=slot.stack.len()&&cur>=base&&cur<end&&(cur-base)%IO_STACK_LOCATION_SIZE_X64==0 {
    unsafe{(*stack).device_object}
   } else {ptr::null_mut()}
  } else {ptr::null_mut()}
 };
 unsafe{routine(device,irp);}
 // The cancel routine owns stopping the device; once it returns, drop any
 // remaining IRP-bound IOMMU mappings. Completion performs the same idempotent rundown.
 crate::dma::release_irp_mappings(irp.cast());
 release_irp_active_op(irp);
 try_free_completed_sync_ioctl_irp(irp);
 1
}


static WDM_CANCEL_SELF_TEST_CALLBACKS: core::sync::atomic::AtomicUsize =
    core::sync::atomic::AtomicUsize::new(0);
const STATUS_CANCELLED_SELF_TEST: i32 = 0xC000_0120u32 as i32;

unsafe extern "win64" fn wdm_cancel_self_test_cancel(_device: *mut DeviceObject, irp: *mut Irp) {
    WDM_CANCEL_SELF_TEST_CALLBACKS.fetch_add(1, core::sync::atomic::Ordering::AcqRel);
    unsafe { io_complete_irp_status(irp, STATUS_CANCELLED_SELF_TEST, 0, 0); }
}
unsafe extern "win64" fn wdm_cancel_self_test_dispatch(_device: *mut DeviceObject, irp: *mut Irp) -> i32 {
    unsafe {
        let _ = io_set_cancel_routine(irp, Some(wdm_cancel_self_test_cancel));
        io_mark_irp_pending(irp);
    }
    STATUS_PENDING
}

/// Exercise the real WDM cancel-spin-lock, cancel callback, and IRP completion
/// path after the executive scheduler and SMP processors are online.
pub fn io_cancel_self_test() -> bool {
    WDM_CANCEL_SELF_TEST_CALLBACKS.store(0, core::sync::atomic::Ordering::Release);
    let mut driver: DriverObject = unsafe { core::mem::zeroed() };
    driver.major_function[IRP_MJ_READ as usize] = Some(wdm_cancel_self_test_dispatch);
    let mut device: *mut DeviceObject = ptr::null_mut();
    if unsafe { io_create_device(&mut driver, 0, ptr::null_mut(), 7, 0, 0, &mut device) } != STATUS_SUCCESS {
        return false;
    }
    let irp = unsafe { io_allocate_irp(1, 0) };
    if irp.is_null() {
        unsafe { io_delete_device(device); }
        return false;
    }
    let stack = unsafe { io_get_next_irp_stack_location(irp) };
    if stack.is_null() {
        unsafe { io_free_irp(irp); io_delete_device(device); }
        return false;
    }
    unsafe { (*stack).major_function = IRP_MJ_READ; }
    let dispatch_status = unsafe { io_call_driver(device, irp) };
    if dispatch_status != STATUS_PENDING {
        unsafe { io_free_irp(irp); io_delete_device(device); }
        return false;
    }
    let cancelled = unsafe { io_cancel_irp(irp) };
    let final_status = unsafe { io_get_irp_status(irp) };
    let callbacks = WDM_CANCEL_SELF_TEST_CALLBACKS.load(core::sync::atomic::Ordering::Acquire);
    unsafe { io_free_irp(irp); io_delete_device(device); }
    cancelled != 0 && final_status == STATUS_CANCELLED_SELF_TEST && callbacks == 1
}

/// Resolve a counted device name to a registered device object. This is the
/// bounded name lookup primitive used by future IoGetDeviceObjectPointer and
/// mount-manager integration; it does not create a FILE_OBJECT.
pub unsafe fn lookup_device_object(name:*mut UnicodeString)->*mut DeviceObject {
    let Some((len,buffer))=valid_unicode_name(name) else{return ptr::null_mut()};
    let mut copy=[0u16;MAX_SYMBOLIC_LINK_CHARS];
    unsafe{ptr::copy_nonoverlapping(buffer,copy.as_mut_ptr(),len);}
    let state=WDM.lock();
    state.devices.iter().find(|s|s.used&&!s.deleting&&s.name_len==len&&unicode_equal_folded(&s.name[..s.name_len],&copy[..len])).map_or(ptr::null_mut(),|s|ptr::addr_of!(s.object) as *mut DeviceObject)
}


