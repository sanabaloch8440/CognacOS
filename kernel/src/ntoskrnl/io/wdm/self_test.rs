static RUNDOWN_SELFTEST_COMPLETIONS:core::sync::atomic::AtomicUsize=core::sync::atomic::AtomicUsize::new(0);
static RUNDOWN_SELFTEST_UNLOADS:core::sync::atomic::AtomicUsize=core::sync::atomic::AtomicUsize::new(0);
unsafe extern "win64" fn rundown_selftest_pending(_device:*mut DeviceObject,_irp:*mut Irp)->i32 { STATUS_PENDING }
unsafe extern "win64" fn rundown_selftest_completion(_device:*mut DeviceObject,_irp:*mut Irp,_context:*mut c_void)->i32 {
 RUNDOWN_SELFTEST_COMPLETIONS.fetch_add(1,core::sync::atomic::Ordering::AcqRel);STATUS_SUCCESS
}
unsafe extern "win64" fn rundown_selftest_unload(_driver:*mut DriverObject) {
 RUNDOWN_SELFTEST_UNLOADS.fetch_add(1,core::sync::atomic::Ordering::AcqRel);
}

/// Boot-time regression for the actual WDM implementation: a pending IRP must
/// pin its device across dispatch return and unload; concurrent/repeated
/// completion must run its callback and the unload callback exactly once.
pub(crate) fn rundown_self_test()->bool {
 RUNDOWN_SELFTEST_COMPLETIONS.store(0,core::sync::atomic::Ordering::Release);
 RUNDOWN_SELFTEST_UNLOADS.store(0,core::sync::atomic::Ordering::Release);
 let mut driver:DriverObject=unsafe{core::mem::zeroed()};
 driver.driver_unload=Some(rundown_selftest_unload);
 driver.major_function[0]=Some(rundown_selftest_pending);
 let mut device: *mut DeviceObject=ptr::null_mut();
 if unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)}!=STATUS_SUCCESS{return false}
 let irp=unsafe{io_allocate_irp(2,0)};
 if irp.is_null(){unsafe{io_delete_device(device)};return false}
 unsafe {
  let next=io_get_next_irp_stack_location(irp);
  if next.is_null(){io_free_irp(irp);io_delete_device(device);return false}
  (*next).major_function=0;
  io_set_completion_routine(irp,Some(rundown_selftest_completion),ptr::null_mut(),1,1,1);
 }
 if unsafe{io_call_driver(device,irp)}!=STATUS_PENDING {
  unsafe{io_free_irp(irp);io_delete_device(device);}
  return false
 }
 if unsafe{io_unload_driver(&mut driver)}!=STATUS_PENDING
  || RUNDOWN_SELFTEST_UNLOADS.load(core::sync::atomic::Ordering::Acquire)!=0 {
  unsafe{io_free_irp(irp);}
  return false
 }
 unsafe{io_complete_request(irp,0);io_complete_request(irp,0);}
 let completion_ok=RUNDOWN_SELFTEST_COMPLETIONS.load(core::sync::atomic::Ordering::Acquire)==1;
 let unload_status=unsafe{io_unload_driver(&mut driver)};
 let unload_ok=unload_status==STATUS_SUCCESS
  && RUNDOWN_SELFTEST_UNLOADS.load(core::sync::atomic::Ordering::Acquire)==1;
 unsafe{io_free_irp(irp);}
 completion_ok&&unload_ok
}
