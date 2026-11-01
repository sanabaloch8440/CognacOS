#[cfg(test)]
mod wdm_tests {
 use super::*;
 #[test]
 fn x64_io_csq_layout_matches_wdk_storage_abi() {
  assert_eq!(core::mem::size_of::<IoCsq>(),0x40);
  assert_eq!(core::mem::offset_of!(IoCsq,type_),0x00);
  assert_eq!(core::mem::offset_of!(IoCsq,insert_irp),0x08);
  assert_eq!(core::mem::offset_of!(IoCsq,remove_irp),0x10);
  assert_eq!(core::mem::offset_of!(IoCsq,peek_next_irp),0x18);
  assert_eq!(core::mem::offset_of!(IoCsq,acquire_lock),0x20);
  assert_eq!(core::mem::offset_of!(IoCsq,release_lock),0x28);
  assert_eq!(core::mem::offset_of!(IoCsq,complete_canceled_irp),0x30);
  assert_eq!(core::mem::offset_of!(IoCsq,reserve_pointer),0x38);
  assert_eq!(core::mem::size_of::<IoCsqIrpContext>(),0x18);
  assert_eq!(core::mem::offset_of!(IoCsqIrpContext,irp),0x08);
  assert_eq!(core::mem::offset_of!(IoCsqIrpContext,csq),0x10);
 }
 #[test]
 fn x64_wdm_prefix_layouts_match_expected_offsets() {
  assert_eq!(core::mem::size_of::<UnicodeString>(),16);
  assert_eq!(core::mem::size_of::<IoStatusBlock>(),16);
  assert_eq!(core::mem::offset_of!(IoStatusBlock,status),0x00);
  assert_eq!(core::mem::offset_of!(IoStatusBlock,information),0x08);
  assert_eq!(core::mem::size_of::<IoStackLocation>(),IO_STACK_LOCATION_SIZE_X64);
  assert_eq!(core::mem::offset_of!(IoStackLocation,major_function),0x00);
  assert_eq!(core::mem::offset_of!(IoStackLocation,parameters),0x04);
  assert_eq!(core::mem::offset_of!(IoStackLocation,device_object),0x28);
  assert_eq!(core::mem::offset_of!(IoStackLocation,file_object),0x30);
  assert_eq!(core::mem::offset_of!(IoStackLocation,completion_routine),0x38);
  assert_eq!(core::mem::offset_of!(IoStackLocation,context),0x40);
  assert_eq!(core::mem::offset_of!(DriverObject,device_object),0x08);
  assert_eq!(core::mem::offset_of!(DriverObject,flags),0x10);
  assert_eq!(core::mem::offset_of!(DriverObject,driver_start),0x18);
  assert_eq!(core::mem::offset_of!(DriverObject,driver_size),0x20);
  assert_eq!(core::mem::offset_of!(DriverObject,driver_section),0x28);
  assert_eq!(core::mem::offset_of!(DriverObject,driver_extension),0x30);
  assert_eq!(core::mem::offset_of!(DriverObject,driver_name),0x38);
  assert_eq!(core::mem::offset_of!(DriverObject,hardware_database),0x48);
  assert_eq!(core::mem::offset_of!(DriverObject,fast_io_dispatch),0x50);
  assert_eq!(core::mem::offset_of!(DriverObject,driver_init),0x58);
  assert_eq!(core::mem::offset_of!(DriverObject,driver_start_io),0x60);
  assert_eq!(core::mem::offset_of!(DriverObject,driver_unload),0x68);
  assert_eq!(core::mem::offset_of!(DriverObject,major_function),0x70);
  assert_eq!(core::mem::size_of::<DriverObject>(),0x150);
  assert_eq!(core::mem::offset_of!(DeviceObject,object_type),0x00);
  assert_eq!(core::mem::offset_of!(DeviceObject,size),0x02);
  assert_eq!(core::mem::offset_of!(DeviceObject,reference_count),0x04);
  assert_eq!(core::mem::offset_of!(DeviceObject,driver_object),0x08);
  assert_eq!(core::mem::offset_of!(DeviceObject,next_device),0x10);
  assert_eq!(core::mem::offset_of!(DeviceObject,attached_device),0x18);
  assert_eq!(core::mem::offset_of!(DeviceObject,current_irp),0x20);
  assert_eq!(core::mem::offset_of!(DeviceObject,timer),0x28);
  assert_eq!(core::mem::offset_of!(DeviceObject,flags),0x30);
  assert_eq!(core::mem::offset_of!(DeviceObject,characteristics),0x34);
  assert_eq!(core::mem::offset_of!(DeviceObject,vpb),0x38);
  assert_eq!(core::mem::offset_of!(DeviceObject,device_extension),0x40);
  assert_eq!(core::mem::offset_of!(DeviceObject,device_type),0x48);
  assert_eq!(core::mem::offset_of!(DeviceObject,stack_size),0x4c);
  assert_eq!(core::mem::offset_of!(DeviceObject,queue),0x50);
  assert_eq!(core::mem::size_of::<KDeviceQueueEntry>(),0x18);
  assert_eq!(core::mem::size_of::<KDeviceQueue>(),0x28);
  assert_eq!(core::mem::offset_of!(KDeviceQueue,device_list_head),0x04);
  assert_eq!(core::mem::offset_of!(KDeviceQueue,lock),0x18);
  assert_eq!(core::mem::offset_of!(KDeviceQueue,busy),0x20);
  assert_eq!(core::mem::offset_of!(DeviceObject,device_queue),0x98);
  assert_eq!(core::mem::offset_of!(DeviceObject,dpc),0xc0);
  assert_eq!(core::mem::offset_of!(DeviceObject,active_thread_count),0x100);
  assert_eq!(core::mem::offset_of!(DeviceObject,security_descriptor),0x108);
  assert_eq!(core::mem::offset_of!(DeviceObject,device_lock),0x110);
  assert_eq!(core::mem::offset_of!(DeviceObject,sector_size),0x128);
  assert_eq!(core::mem::offset_of!(DeviceObject,device_object_extension),0x130);
  assert_eq!(core::mem::offset_of!(DeviceObject,reserved),0x138);
  assert_eq!(core::mem::size_of::<DeviceObject>(),0x150);
  assert_eq!(core::mem::offset_of!(DeviceObject,queue),0x50);
  assert_eq!(core::mem::size_of::<Irp>(),IRP_SIZE_X64);
  assert_eq!(core::mem::size_of::<DeviceObject>(),0x150);
  assert_eq!(IRP_STACK_OFFSET,0xB8);
  assert_eq!(IRP_STACK_OFFSET,0xB8);
  assert_eq!(IRP_STATUS_OFFSET,0x30);
  assert_eq!(IRP_INFORMATION_OFFSET,0x38);
 }
 unsafe extern "win64" fn test_dispatch(device:*mut DeviceObject, irp:*mut Irp)->i32 {
  if device.is_null() || irp.is_null() { return STATUS_INVALID_PARAMETER; }
  unsafe {
   ptr::write_unaligned(ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(IRP_STATUS_OFFSET).cast::<i32>(),STATUS_SUCCESS);
   io_complete_request(irp,0);
  }
  STATUS_SUCCESS
 }
 #[test]
 fn device_dispatch_uses_wdm_device_object_signature() {
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut device: *mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)},STATUS_SUCCESS);
  driver.major_function[0]=Some(test_dispatch);
  let irp=unsafe{io_allocate_irp(2,0)};
  assert!(!irp.is_null());
  let next=unsafe{io_get_next_irp_stack_location(irp)};
  assert!(!next.is_null());
  unsafe { (*next).major_function=0; }
  assert_eq!(unsafe{io_call_driver(device,irp)},STATUS_SUCCESS);
  assert_eq!(unsafe{ptr::read_unaligned(ptr::addr_of!((*irp).bytes).cast::<u8>().add(IRP_STATUS_OFFSET))},0);
  unsafe{io_free_irp(irp);}
  // Completion and IRP free release rundown; they must not implicitly delete
  // a device whose owner has not called IoDeleteDevice.
  assert_eq!(driver.device_object,device);
  unsafe{io_delete_device(device);}
 }
 #[test]
 fn power_capabilities_are_registered_and_cleaned_with_pnp_removal() {
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut device:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)},STATUS_SUCCESS);
  assert!(crate::ntoskrnl::po::device_power::supports_wdm_state(device,1));
  assert_eq!(crate::ntoskrnl::po::device_power::register(device,1|2|8,false),STATUS_SUCCESS);
  assert!(crate::ntoskrnl::po::device_power::supports_wdm_state(device,2));
  assert_eq!(unsafe{io_begin_pnp_transition(device,0x02)},STATUS_SUCCESS);
  assert_eq!(unsafe{io_complete_pnp_transition(device,0x02,STATUS_SUCCESS)},STATUS_SUCCESS);
  assert!(!crate::ntoskrnl::po::device_power::supports_wdm_state(device,2));
  unsafe{io_delete_device(device);}
 }
 #[test]
 fn removal_waits_for_remove_lock_and_reference_rundown() {
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};let mut d:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut d)},STATUS_SUCCESS);
  assert_eq!(unsafe{io_reference_device_object(d)},d);
  assert_eq!(unsafe{io_acquire_device_remove_lock(d)},STATUS_SUCCESS);
  unsafe{io_delete_device(d);}
  assert_eq!(unsafe{io_acquire_device_remove_lock(d)},STATUS_DELETE_PENDING);
  unsafe{io_dereference_device_object(d);}
  assert!(!driver.device_object.is_null());
  unsafe{io_release_device_remove_lock(d);}
  assert!(driver.device_object.is_null());
 }
 #[test]
 fn power_transition_serializes_and_commits_only_success() {
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};let mut d:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut d)},STATUS_SUCCESS);
  assert_eq!(device_power_state(d),0);
  assert_eq!(unsafe{io_begin_device_power_transition(d,DEVICE_POWER_D3)},STATUS_SUCCESS);
  assert_eq!(unsafe{io_begin_device_power_transition(d,DEVICE_POWER_D1)},STATUS_DEVICE_BUSY);
  assert_eq!(unsafe{io_complete_device_power_transition(d,DEVICE_POWER_D3,STATUS_INVALID_DEVICE_REQUEST)},STATUS_INVALID_DEVICE_REQUEST);
  assert_eq!(device_power_state(d),0);
  assert_eq!(unsafe{io_begin_device_power_transition(d,DEVICE_POWER_D3)},STATUS_SUCCESS);
  assert_eq!(unsafe{io_complete_device_power_transition(d,DEVICE_POWER_D3,STATUS_SUCCESS)},STATUS_SUCCESS);
  assert_eq!(device_power_state(d),3);unsafe{io_delete_device(d);}
 }
 #[test]
 fn create_delete_device_and_allocate_irp_stack() {
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut device: *mut DeviceObject=ptr::null_mut();
  let status=unsafe{io_create_device(&mut driver,32,ptr::null_mut(),7,0,0,&mut device)};
  assert_eq!(status,STATUS_SUCCESS);
  assert!(!device.is_null());
  assert_eq!(unsafe{(*device).device_extension.is_null()},false);
  assert_eq!(unsafe{(*device).size},(core::mem::size_of::<DeviceObject>()+32) as u16);
  assert_eq!(driver.device_object,device);
  unsafe{io_delete_device(device);}
  assert!(driver.device_object.is_null());

  let irp=unsafe{io_allocate_irp(32,0)};
  assert!(!irp.is_null());
  assert_eq!(unsafe{*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x42)},32);
  assert_eq!(unsafe{ptr::read_unaligned(ptr::addr_of!((*irp).bytes).cast::<u8>().add(2).cast::<u16>())},IRP_SIZE_X64 as u16);
  assert_eq!(unsafe{io_get_current_irp_stack_location(irp)} as usize,irp as usize+IRP_SIZE_X64+32*IO_STACK_LOCATION_SIZE_X64);
  assert!(!unsafe{io_get_current_irp_stack_location(irp)}.is_null());
  let next=unsafe{io_get_next_irp_stack_location(irp)};
  assert!(!next.is_null());
  assert_eq!(next as usize,irp as usize+IRP_SIZE_X64+IO_STACK_LOCATION_SIZE_X64);
  unsafe{io_free_irp(irp);}
 }
 #[test]
 fn attach_detach_tracks_filter_stack_and_stack_depth() {
  let mut lower_driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut upper_driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut lower:*mut DeviceObject=ptr::null_mut();
  let mut upper:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut lower_driver,0,ptr::null_mut(),7,0,0,&mut lower)},STATUS_SUCCESS);
  assert_eq!(unsafe{io_create_device(&mut upper_driver,0,ptr::null_mut(),7,0,0,&mut upper)},STATUS_SUCCESS);
  assert_eq!(unsafe{io_attach_device_to_device_stack(upper,lower)},lower);
  assert_eq!(unsafe{(*lower).attached_device},upper);
  assert_eq!(unsafe{(*upper).stack_size},2);
  unsafe{io_detach_device(lower);}
  assert!(unsafe{(*lower).attached_device.is_null()});
  assert_eq!(unsafe{(*upper).stack_size},1);
  unsafe{io_delete_device(upper);io_delete_device(lower);}
 }


 #[test]
 fn device_stack_reference_apis_return_owned_references() {
  let mut lower_driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut upper_driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut lower:*mut DeviceObject=ptr::null_mut();
  let mut upper:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut lower_driver,0,ptr::null_mut(),7,0,0,&mut lower)},STATUS_SUCCESS);
  assert_eq!(unsafe{io_create_device(&mut upper_driver,0,ptr::null_mut(),7,0,0,&mut upper)},STATUS_SUCCESS);
  assert_eq!(unsafe{io_attach_device_to_device_stack(upper,lower)},lower);
  let top=unsafe{io_get_attached_device_reference(lower)};
  assert_eq!(top,upper);
  assert_eq!(unsafe{(*upper).reference_count},2);
  unsafe{io_dereference_device_object(top);}
  let base=unsafe{io_get_device_attachment_base_ref(upper)};
  assert_eq!(base,lower);
  assert_eq!(unsafe{(*lower).reference_count},3);
  unsafe{io_dereference_device_object(base);}
  assert_eq!(unsafe{(*lower).reference_count},2);
  unsafe{io_detach_device(lower);}
  assert_eq!(unsafe{(*lower).reference_count},1);
  unsafe{io_delete_device(upper);io_delete_device(lower);}
 }

 #[test]
 fn three_level_filter_stack_detaches_only_from_the_top_down() {
  let mut base_driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut filter1_driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut filter2_driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut base:*mut DeviceObject=ptr::null_mut();
  let mut filter1:*mut DeviceObject=ptr::null_mut();
  let mut filter2:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut base_driver,0,ptr::null_mut(),7,0,0,&mut base)},STATUS_SUCCESS);
  assert_eq!(unsafe{io_create_device(&mut filter1_driver,0,ptr::null_mut(),7,0,0,&mut filter1)},STATUS_SUCCESS);
  assert_eq!(unsafe{io_create_device(&mut filter2_driver,0,ptr::null_mut(),7,0,0,&mut filter2)},STATUS_SUCCESS);
  assert_eq!(unsafe{io_attach_device_to_device_stack(filter1,base)},base);
  assert_eq!(unsafe{io_attach_device_to_device_stack(filter2,base)},filter1);
  assert_eq!(unsafe{(*base).reference_count},2,"first attachment pins the base device");
  assert_eq!(unsafe{(*filter1).reference_count},2,"second attachment pins the immediate lower filter");
  assert_eq!(unsafe{(*filter1).stack_size},2);
  assert_eq!(unsafe{(*filter2).stack_size},3);
  unsafe{io_detach_device(base);}
  assert_eq!(unsafe{(*base).attached_device},filter1,"lower filter must not be detached under an attached upper filter");
  unsafe{io_detach_device(filter1);}
  assert_eq!(unsafe{(*filter1).reference_count},1,"detaching filter2 releases its lower pin");
  unsafe{io_detach_device(base);}
  assert_eq!(unsafe{(*base).reference_count},1,"detaching filter1 releases the base pin");
  assert!(unsafe{(*filter1).attached_device.is_null()});
  assert!(unsafe{(*base).attached_device.is_null()});
  unsafe{io_delete_device(filter2);io_delete_device(filter1);io_delete_device(base);}
 }

 #[test]
 fn initialize_irp_rejects_short_packet_and_sets_stack_bounds() {
  let mut storage=[0u8;IRP_SIZE_X64+2*IO_STACK_LOCATION_SIZE_X64];
  let irp=storage.as_mut_ptr().cast::<Irp>();
  unsafe{io_initialize_irp(irp, (storage.len()-1) as u16, 2);}
  assert_eq!(unsafe{*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x42)},0);
  unsafe{io_initialize_irp(irp, storage.len() as u16, 2);}
  assert_eq!(unsafe{*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x42)},2);
  assert_eq!(unsafe{ptr::read_unaligned(ptr::addr_of!((*irp).bytes).cast::<u8>().add(2).cast::<u16>())},IRP_SIZE_X64 as u16);
  assert_eq!(unsafe{io_get_current_irp_stack_location(irp)} as usize,storage.as_ptr() as usize+storage.len());
 }

 #[test]
 fn attachment_queries_and_irp_reuse_preserve_allocation() {
  let mut lower_driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut upper_driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut lower:*mut DeviceObject=ptr::null_mut();
  let mut upper:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut lower_driver,0,ptr::null_mut(),7,0,0,&mut lower)},STATUS_SUCCESS);
  assert_eq!(unsafe{io_create_device(&mut upper_driver,0,ptr::null_mut(),7,0,0,&mut upper)},STATUS_SUCCESS);
  assert_eq!(unsafe{io_attach_device_to_device_stack(upper,lower)},lower);
  assert_eq!(unsafe{io_get_attached_device(lower)},upper);
  assert_eq!(unsafe{io_get_device_attachment_base_ref(upper)},lower);
  assert_eq!(unsafe{io_get_lower_device_object(upper)},lower);
  let irp=unsafe{io_allocate_irp(2,0)};
  assert!(!irp.is_null());
  unsafe{io_reuse_irp(irp,STATUS_SUCCESS);}
  assert_eq!(unsafe{*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x43)},3);
  assert_eq!(unsafe{io_get_current_irp_stack_location(irp)} as usize,irp as usize+IRP_SIZE_X64+2*IO_STACK_LOCATION_SIZE_X64);
  unsafe{io_free_irp(irp);io_detach_device(lower);io_delete_device(upper);io_delete_device(lower);}
 }

 #[test]
 fn file_system_registration_is_bounded_idempotent_and_removable() {
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut device:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)},STATUS_SUCCESS);
  unsafe{io_register_file_system(device);io_register_file_system(device);}
  assert!(is_file_system_registered(device));
  unsafe{io_unregister_file_system(device);}
  assert!(!is_file_system_registered(device));
  unsafe{io_delete_device(device);}
 }

 #[test]
 fn symbolic_links_validate_names_collide_case_insensitively_and_delete() {
  let mut link=[b'\\' as u16,b'?' as u16,b'?' as u16,b'\\' as u16,b'X' as u16];
  let mut target=[b'\\' as u16,b'D' as u16,b'e' as u16,b'v' as u16,b'i' as u16,b'c' as u16,b'e' as u16,b'\\' as u16,b'E' as u16,b'x' as u16,b't' as u16,b'4' as u16];
  let mut link_name=UnicodeString{length:(link.len()*2) as u16,maximum_length:(link.len()*2) as u16,buffer:link.as_mut_ptr()};
  let mut target_name=UnicodeString{length:(target.len()*2) as u16,maximum_length:(target.len()*2) as u16,buffer:target.as_mut_ptr()};
  assert_eq!(unsafe{io_create_symbolic_link(&mut link_name,&mut target_name)},STATUS_SUCCESS);
  let mut lower_link=[b'\\' as u16,b'?' as u16,b'?' as u16,b'\\' as u16,b'x' as u16];
  let mut lower_name=UnicodeString{length:(lower_link.len()*2) as u16,maximum_length:(lower_link.len()*2) as u16,buffer:lower_link.as_mut_ptr()};
  assert_eq!(unsafe{io_create_symbolic_link(&mut lower_name,&mut target_name)},STATUS_OBJECT_NAME_COLLISION);
  assert_eq!(unsafe{io_delete_symbolic_link(&mut lower_name)},STATUS_SUCCESS);
  assert_eq!(unsafe{io_delete_symbolic_link(&mut lower_name)},STATUS_OBJECT_NAME_NOT_FOUND);
 }

 static COMPLETION_CALLBACKS:core::sync::atomic::AtomicUsize=core::sync::atomic::AtomicUsize::new(0);
 unsafe extern "win64" fn counted_completion(_device:*mut DeviceObject,_irp:*mut Irp,_context:*mut c_void)->i32 {
  COMPLETION_CALLBACKS.fetch_add(1,core::sync::atomic::Ordering::AcqRel);STATUS_SUCCESS
 }
 unsafe extern "win64" fn pending_dispatch(_device:*mut DeviceObject,irp:*mut Irp)->i32 {
  unsafe{io_mark_irp_pending(irp);}
  STATUS_PENDING
 }
 static CANCEL_CALLED:core::sync::atomic::AtomicBool=core::sync::atomic::AtomicBool::new(false);
 unsafe extern "win64" fn test_cancel(device:*mut DeviceObject,irp:*mut Irp) {
  if !irp.is_null() { CANCEL_CALLED.store(true,core::sync::atomic::Ordering::Release); }
  let _=device;
 }
 #[test]
 fn completion_claim_and_driver_unload_wait_for_outstanding_irp() {
  COMPLETION_CALLBACKS.store(0,core::sync::atomic::Ordering::Release);
  UNLOAD_CALLS.store(0,core::sync::atomic::Ordering::Release);
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  driver.driver_unload=Some(counted_driver_unload);
  driver.major_function[0]=Some(pending_dispatch);
  let mut device:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)},STATUS_SUCCESS);
  let irp=unsafe{io_allocate_irp(2,0)};
  assert!(!irp.is_null());
  unsafe {
   (*io_get_next_irp_stack_location(irp)).major_function=0;
   io_set_completion_routine(irp,Some(counted_completion),ptr::null_mut(),1,1,1);
  }
  assert_eq!(unsafe{io_call_driver(device,irp)},STATUS_PENDING);
  // A pending IRP pins its device beyond the return from the dispatch routine.
  assert_eq!(unsafe{io_unload_driver(&mut driver)},STATUS_PENDING);
  assert_eq!(UNLOAD_CALLS.load(core::sync::atomic::Ordering::Acquire),0);
  unsafe{io_complete_request(irp,0);io_complete_request(irp,0);}
  assert_eq!(COMPLETION_CALLBACKS.load(core::sync::atomic::Ordering::Acquire),1);
  assert_eq!(unsafe{io_unload_driver(&mut driver)},STATUS_SUCCESS);
  assert_eq!(UNLOAD_CALLS.load(core::sync::atomic::Ordering::Acquire),1);
  unsafe{io_free_irp(irp);}
 }

 static IOCTL_COMPLETIONS:core::sync::atomic::AtomicUsize=core::sync::atomic::AtomicUsize::new(0);
 unsafe extern "win64" fn ioctl_buffered_dispatch(_device:*mut DeviceObject,irp:*mut Irp)->i32 {
  let stack=unsafe{io_get_current_irp_stack_location(irp)};
  assert_eq!(unsafe{ptr::read_unaligned(stack.cast::<u8>().add(4).cast::<u32>())},128*1024);
  assert_eq!(unsafe{ptr::read_unaligned(stack.cast::<u8>().add(12).cast::<u32>())},2);
  assert_eq!(unsafe{ptr::read_unaligned(stack.cast::<u8>().add(20).cast::<u32>())},0x00222000);
  let system_buffer=unsafe{read_ptr(ptr::addr_of!((*irp).bytes).cast::<u8>(),0x18).cast::<u8>()};
  if !system_buffer.is_null(){unsafe{ptr::copy_nonoverlapping(b"OK".as_ptr(),system_buffer,2);}}
  unsafe{
   ptr::write_unaligned(ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(IRP_STATUS_OFFSET).cast::<i32>(),STATUS_SUCCESS);
   ptr::write_unaligned(ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(IRP_INFORMATION_OFFSET).cast::<usize>(),2);
   io_complete_request(irp,0);
  }
  STATUS_SUCCESS
 }
 unsafe extern "win64" fn ioctl_counted_completion(_device:*mut DeviceObject,_irp:*mut Irp,_context:*mut c_void)->i32 {
  IOCTL_COMPLETIONS.fetch_add(1,core::sync::atomic::Ordering::AcqRel);
  STATUS_SUCCESS
 }
 #[test]
 fn build_device_io_control_request_copies_buffered_output_and_auto_frees() {
  IOCTL_COMPLETIONS.store(0,core::sync::atomic::Ordering::Release);
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  driver.major_function[0x0e]=Some(ioctl_buffered_dispatch);
  let mut device:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)},STATUS_SUCCESS);
  let input=*b"IN";
  let mut output=[0u8;128*1024];
  let mut iosb:IoStatusBlock=unsafe{core::mem::zeroed()};
  let mut event=core::mem::MaybeUninit::<crate::ntoskrnl::ke::runtime::DispatcherHeader>::uninit();
  unsafe{crate::ntoskrnl::ke::runtime::ke_initialize_event(event.as_mut_ptr(),0,0);}
  // METHOD_BUFFERED = 0; the notification event must signal only after the
  // output copy and IOSB publication are complete.
  let irp=unsafe{io_build_device_io_control_request(0x00222000,device,input.as_ptr() as *mut c_void,2,output.as_mut_ptr().cast(),output.len() as u32,0,event.as_mut_ptr().cast(),&mut iosb)};
  assert!(!irp.is_null());
  unsafe{io_set_completion_routine(irp,Some(ioctl_counted_completion),ptr::null_mut(),1,1,1);}
  assert_eq!(unsafe{io_call_driver(device,irp)},STATUS_SUCCESS);
  assert_eq!(unsafe{iosb.status.status},STATUS_SUCCESS);
  assert_eq!(iosb.information,2);
  assert_eq!(unsafe{crate::ntoskrnl::ke::runtime::ke_read_state_event(event.as_ptr())},1);
  assert_eq!(&output[..2],b"OK");
  assert_eq!(IOCTL_COMPLETIONS.load(core::sync::atomic::Ordering::Acquire),1);
  // The builder-owned IRP is reclaimed at terminal completion.
  assert!(!WDM.lock().irps.iter().any(|s|s.used&&ptr::eq(&s.object,irp)));
  unsafe{io_delete_device(device);}
 }

 #[test]
 fn thread_exit_detaches_and_cancels_builder_owned_pending_irps() {
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  driver.major_function[0x0e]=Some(pending_dispatch);
  let mut device:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)},STATUS_SUCCESS);
  let mut iosb:IoStatusBlock=unsafe{core::mem::zeroed()};
  let irp=unsafe{io_build_device_io_control_request(0x00222003,device,ptr::null_mut(),0,ptr::null_mut(),0,0,ptr::null_mut(),&mut iosb)};
  assert!(!irp.is_null());
  // Simulate the current-thread queue entry without requiring a live scheduler
  // thread in this host-side unit test.
  {
   let mut state=WDM.lock();
   let _=sync_queue_detach_locked(&mut state,irp);
   assert!(sync_queue_attach_locked(&mut state,irp,0xBEEF));
  }
  unsafe{io_set_completion_routine(irp,Some(counted_completion),ptr::null_mut(),1,1,1);}
  assert_eq!(unsafe{io_call_driver(device,irp)},STATUS_PENDING);
  assert_eq!(io_cancel_thread_irps(0xBEEF),1);
  assert_eq!(unsafe{*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x44)},1);
  assert!(unsafe{read_ptr(ptr::addr_of!((*irp).bytes).cast::<u8>(),0x48)}.is_null());
  // With no lower cancel routine, the IRP stays alive until the driver completes
  // it; late completion must not touch the exiting thread's IOSB.
  unsafe{io_complete_request(irp,0);}
  assert!(!WDM.lock().irps.iter().any(|s|s.used&&ptr::eq(&s.object,irp)));
  unsafe{io_delete_device(device);}
 }

 #[test]
 fn free_irp_does_not_reclaim_a_pending_request() {
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  driver.major_function[0]=Some(pending_dispatch);
  let mut device:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)},STATUS_SUCCESS);
  let irp=unsafe{io_allocate_irp(2,0)};
  assert!(!irp.is_null());
  unsafe{(*io_get_next_irp_stack_location(irp)).major_function=0;}
  assert_eq!(unsafe{io_call_driver(device,irp)},STATUS_PENDING);
  assert_eq!(unsafe{*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x41)},0);
  assert_ne!(unsafe{(*io_get_current_irp_stack_location(irp)).control}&0x01,0);
  // A premature free is rejected; the same IRP remains registered and
  // its outstanding completion can still finish safely.
  unsafe{io_free_irp(irp);}
  assert!(!unsafe{io_get_current_irp_stack_location(irp)}.is_null());
  unsafe{io_complete_request(irp,0);}
  unsafe{io_free_irp(irp);io_delete_device(device);}
 }

 static MORE_PROCESSING_CALLBACKS:core::sync::atomic::AtomicUsize=core::sync::atomic::AtomicUsize::new(0);
 unsafe extern "win64" fn stop_completion_once(_device:*mut DeviceObject,_irp:*mut Irp,_context:*mut c_void)->i32 {
  MORE_PROCESSING_CALLBACKS.fetch_add(1,core::sync::atomic::Ordering::AcqRel);
  0xC0000016u32 as i32
 }
 unsafe extern "win64" fn free_then_stop_completion(_device:*mut DeviceObject,irp:*mut Irp,_context:*mut c_void)->i32 {
  unsafe{io_free_irp(irp);}
  0xC0000016u32 as i32
 }
 #[test]
 fn free_irp_inside_more_processing_callback_is_deferred_until_safe() {
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  driver.major_function[0]=Some(completing_dispatch);
  let mut device:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)},STATUS_SUCCESS);
  let irp=unsafe{io_allocate_irp(2,0)};
  assert!(!irp.is_null());
  unsafe{
   (*io_get_next_irp_stack_location(irp)).major_function=0;
   io_set_completion_routine(irp,Some(free_then_stop_completion),ptr::null_mut(),1,1,1);
  }
  assert_eq!(unsafe{io_call_driver(device,irp)},STATUS_SUCCESS);
  assert!(!WDM.lock().irps.iter().any(|s|s.used&&ptr::eq(&s.object,irp)));
  unsafe{io_delete_device(device);}
 }

 #[test]
 fn reuse_after_more_processing_required_releases_old_stack_references() {
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  driver.major_function[0]=Some(completing_dispatch);
  let mut device:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)},STATUS_SUCCESS);
  let irp=unsafe{io_allocate_irp(2,0)};
  assert!(!irp.is_null());
  unsafe{
   (*io_get_next_irp_stack_location(irp)).major_function=0;
   io_set_completion_routine(irp,Some(stop_completion_once),ptr::null_mut(),1,1,1);
  }
  assert_eq!(unsafe{io_call_driver(device,irp)},STATUS_SUCCESS);
  unsafe{io_reuse_irp(irp,STATUS_SUCCESS);}
  assert_eq!(unsafe{*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x43)},3);
  assert!(!unsafe{io_get_next_irp_stack_location(irp)}.is_null());
  unsafe{io_free_irp(irp);io_delete_device(device);}
 }

 #[test]
 fn more_processing_required_advances_past_the_callback_before_resume() {
  MORE_PROCESSING_CALLBACKS.store(0,core::sync::atomic::Ordering::Release);
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  driver.major_function[0]=Some(completing_dispatch);
  let mut device:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)},STATUS_SUCCESS);
  let irp=unsafe{io_allocate_irp(2,0)};
  assert!(!irp.is_null());
  unsafe {
   (*io_get_next_irp_stack_location(irp)).major_function=0;
   io_set_completion_routine(irp,Some(stop_completion_once),ptr::null_mut(),1,1,1);
  }
  assert_eq!(unsafe{io_call_driver(device,irp)},STATUS_SUCCESS);
  assert_eq!(MORE_PROCESSING_CALLBACKS.load(core::sync::atomic::Ordering::Acquire),1);
  assert_eq!(unsafe{*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x43)},3);
  unsafe{io_complete_request(irp,0);}
  assert_eq!(MORE_PROCESSING_CALLBACKS.load(core::sync::atomic::Ordering::Acquire),1);
  unsafe{io_free_irp(irp);io_delete_device(device);}
 }

 #[test]
 fn cancel_routine_is_atomically_claimed_and_called_once() {
  CANCEL_CALLED.store(false,core::sync::atomic::Ordering::Release);
  let irp=unsafe{io_allocate_irp(2,0)};
  assert!(!irp.is_null());
  assert!(unsafe{io_set_cancel_routine(irp,Some(test_cancel))}.is_none());
  assert_eq!(unsafe{io_cancel_irp(irp)},1);
  assert!(CANCEL_CALLED.load(core::sync::atomic::Ordering::Acquire));
  assert_eq!(unsafe{io_cancel_irp(irp)},0);
  assert!(unsafe{io_set_cancel_routine(irp,None)}.is_none());
  unsafe{io_free_irp(irp);}
 }

 #[test]
 fn named_device_lookup_and_duplicate_rejection() {
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut duplicate_driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut name=std::string::String::from("\\Device\\Ext4");
  let mut utf16=name.encode_utf16().collect::<std::vec::Vec<u16>>();
  let mut unicode=UnicodeString{length:(utf16.len()*2) as u16,maximum_length:(utf16.len()*2) as u16,buffer:utf16.as_mut_ptr()};
  let mut device:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,&mut unicode,7,0,0,&mut device)},STATUS_SUCCESS);
  assert_eq!(unsafe{lookup_device_object(&mut unicode)},device);
  let mut duplicate:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut duplicate_driver,0,&mut unicode,7,0,0,&mut duplicate)},STATUS_OBJECT_NAME_COLLISION);
  unsafe{io_delete_device(device);}
  assert!(unsafe{lookup_device_object(&mut unicode)}.is_null());
 }

 #[test]
 fn asynchronous_fsd_request_sets_read_parameters_and_iosb() {
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut device:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)},STATUS_SUCCESS);
  let mut status=IoStatusBlock { status: crate::nt_abi::io::IO_STATUS_BLOCK_STATUS { status: STATUS_PENDING }, information: 0 };
  let mut buffer=[0u8;512];
  let offset=4096i64;
  let irp=unsafe{io_build_asynchronous_fsd_request(3,device,buffer.as_mut_ptr().cast(),buffer.len() as u32,&offset,&mut status)};
  assert!(!irp.is_null());
  let stack=unsafe{io_get_next_irp_stack_location(irp)};
  assert_eq!(unsafe{(*stack).major_function},3);
  assert_eq!(unsafe{ptr::read_unaligned(stack.cast::<u8>().add(4).cast::<u32>())},512);
  assert_eq!(unsafe{ptr::read_unaligned(stack.cast::<u8>().add(20).cast::<i64>())},4096);
  unsafe{io_free_irp(irp);io_delete_device(device);}
 }

 unsafe extern "win64" fn completing_dispatch(device:*mut DeviceObject,irp:*mut Irp)->i32 {
  if device.is_null()||irp.is_null(){return STATUS_INVALID_PARAMETER}
  unsafe{
   ptr::write_unaligned(ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(IRP_STATUS_OFFSET).cast::<i32>(),STATUS_SUCCESS);
   ptr::write_unaligned(ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(IRP_INFORMATION_OFFSET).cast::<usize>(),123);
   io_complete_request(irp,0);
  }
  STATUS_SUCCESS
 }
 #[test]
 fn completed_async_request_publishes_iosb_status_and_information() {
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut device:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)},STATUS_SUCCESS);
  driver.major_function[3]=Some(completing_dispatch);
  let mut status=IoStatusBlock { status: crate::nt_abi::io::IO_STATUS_BLOCK_STATUS { status: STATUS_PENDING }, information: 0 };
  let mut buffer=[0u8;512];
  let irp=unsafe{io_build_asynchronous_fsd_request(3,device,buffer.as_mut_ptr().cast(),buffer.len() as u32,ptr::null(),&mut status)};
  assert!(!irp.is_null());
  assert_eq!(unsafe{io_call_driver(device,irp)},STATUS_SUCCESS);
  assert_eq!(unsafe { status.status.status }, STATUS_SUCCESS);
  assert_eq!(status.information,123);
  unsafe{io_free_irp(irp);io_delete_device(device);}
 }


 static UNLOAD_CALLS:core::sync::atomic::AtomicUsize=core::sync::atomic::AtomicUsize::new(0);
 unsafe extern "win64" fn counted_driver_unload(_driver:*mut DriverObject){
  UNLOAD_CALLS.fetch_add(1,core::sync::atomic::Ordering::AcqRel);
 }
 #[test]
 fn driver_unload_waits_for_remove_lock_and_calls_callback_once(){
  UNLOAD_CALLS.store(0,core::sync::atomic::Ordering::Release);
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  driver.driver_unload=Some(counted_driver_unload);
  let mut device:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)},STATUS_SUCCESS);
  assert_eq!(unsafe{io_acquire_device_remove_lock(device)},STATUS_SUCCESS);
  assert_eq!(unsafe{io_unload_driver(&mut driver)},STATUS_PENDING);
  assert_eq!(UNLOAD_CALLS.load(core::sync::atomic::Ordering::Acquire),0);
  unsafe{io_release_device_remove_lock(device);}
  assert_eq!(unsafe{io_unload_driver(&mut driver)},STATUS_SUCCESS);
  assert_eq!(UNLOAD_CALLS.load(core::sync::atomic::Ordering::Acquire),1);
  assert_eq!(unsafe{io_unload_driver(&mut driver)},STATUS_SUCCESS);
  assert_eq!(UNLOAD_CALLS.load(core::sync::atomic::Ordering::Acquire),1);
  let mut rejected:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut rejected)},STATUS_DELETE_PENDING);
  assert!(rejected.is_null());
 }
 unsafe extern "win64" fn pnp_start_pending(_device:*mut DeviceObject,_irp:*mut Irp)->i32 { STATUS_PENDING }
 #[test]
 fn pnp_pending_start_does_not_publish_started_until_completion() {
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut device:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)},STATUS_SUCCESS);
  driver.major_function[0x1b]=Some(pnp_start_pending);
  let irp=unsafe{io_allocate_irp(2,0)};
  assert!(!irp.is_null());
  let stack=unsafe{io_get_next_irp_stack_location(irp)};
  unsafe{(*stack).major_function=0x1b;(*stack).minor_function=0x00;}
  assert_eq!(unsafe{io_call_driver(device,irp)},STATUS_PENDING);
  assert_eq!(io_pnp_state(device),PNP_STATE_NOT_STARTED);
  unsafe{
   ptr::write_unaligned(ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(IRP_STATUS_OFFSET).cast::<i32>(),STATUS_SUCCESS);
   io_complete_request(irp,0);
  }
  assert_eq!(io_pnp_state(device),PNP_STATE_STARTED);
  unsafe{io_free_irp(irp);io_delete_device(device);}
 }
 unsafe extern "win64" fn pnp_start_complete_success(_device:*mut DeviceObject,irp:*mut Irp)->i32 {
  unsafe {
   ptr::write_unaligned(ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(IRP_STATUS_OFFSET).cast::<i32>(),STATUS_SUCCESS);
   io_complete_request(irp,0);
  }
  STATUS_SUCCESS
 }
 unsafe extern "win64" fn pnp_start_complete_failure(_device:*mut DeviceObject,irp:*mut Irp)->i32 {
  unsafe {
   ptr::write_unaligned(ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(IRP_STATUS_OFFSET).cast::<i32>(),STATUS_INVALID_DEVICE_REQUEST);
   io_complete_request(irp,0);
  }
  STATUS_INVALID_DEVICE_REQUEST
 }
 #[test]
 fn pnp_state_changes_only_after_terminal_completion() {
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut device:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)},STATUS_SUCCESS);
  driver.major_function[0x1b]=Some(pnp_start_complete_failure);
  let failed=unsafe{io_allocate_irp(2,0)};
  let stack=unsafe{io_get_next_irp_stack_location(failed)};
  unsafe{(*stack).major_function=0x1b;(*stack).minor_function=0x00;}
  assert_eq!(unsafe{io_call_driver(device,failed)},STATUS_INVALID_DEVICE_REQUEST);
  assert_eq!(io_pnp_state(device),PNP_STATE_NOT_STARTED);
  unsafe{io_free_irp(failed);}
  driver.major_function[0x1b]=Some(pnp_start_complete_success);
  let started=unsafe{io_allocate_irp(2,0)};
  let stack=unsafe{io_get_next_irp_stack_location(started)};
  unsafe{(*stack).major_function=0x1b;(*stack).minor_function=0x00;}
  assert_eq!(unsafe{io_call_driver(device,started)},STATUS_SUCCESS);
  assert_eq!(io_pnp_state(device),PNP_STATE_STARTED);
  unsafe{io_free_irp(started);io_delete_device(device);}
 }

 #[test]
 fn pnp_resource_lists_are_validated_and_conflicts_are_reserved() {
  fn memory_list(start:u64,length:u32)->[u8;36] {
   let mut list=[0u8;36];
   list[0..4].copy_from_slice(&1u32.to_le_bytes()); // one CM_FULL_RESOURCE_DESCRIPTOR
   list[12..14].copy_from_slice(&1u16.to_le_bytes()); // version
   list[14..16].copy_from_slice(&1u16.to_le_bytes()); // revision
   list[16..20].copy_from_slice(&1u32.to_le_bytes()); // one partial descriptor
   list[20]=CM_RESOURCE_MEMORY;
   list[24..32].copy_from_slice(&start.to_le_bytes());
   list[32..36].copy_from_slice(&length.to_le_bytes());
   list
  }
  let mut list=memory_list(0x100000,0x1000);
  let mut driver1:DriverObject=unsafe{core::mem::zeroed()};
  let mut driver2:DriverObject=unsafe{core::mem::zeroed()};
  let mut dev1: *mut DeviceObject=ptr::null_mut();
  let mut dev2: *mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver1,0,ptr::null_mut(),7,0,0,&mut dev1)},STATUS_SUCCESS);
  assert_eq!(unsafe{io_create_device(&mut driver2,0,ptr::null_mut(),7,0,0,&mut dev2)},STATUS_SUCCESS);
  let irp1=unsafe{io_allocate_irp(2,0)};
  let irp2=unsafe{io_allocate_irp(2,0)};
  assert!(!irp1.is_null()&&!irp2.is_null());
  let list_ptr=list.as_ptr().cast::<c_void>();
  let mut wrong_class=list;
  wrong_class[20]=CM_RESOURCE_PORT;
  assert_eq!(unsafe{io_prepare_pnp_resources(dev1,irp1,IRP_MN_START_DEVICE,
   list_ptr,wrong_class.as_ptr().cast::<c_void>())},STATUS_INVALID_PARAMETER);
  assert_eq!(unsafe{io_prepare_pnp_resources(dev1,irp1,IRP_MN_START_DEVICE,list_ptr,list_ptr)},STATUS_SUCCESS);
  assert_eq!(unsafe{io_prepare_pnp_resources(dev2,irp2,IRP_MN_START_DEVICE,list_ptr,list_ptr)},STATUS_DEVICE_BUSY);
  io_finish_pnp_resources(irp1,dev1,IRP_MN_START_DEVICE,STATUS_SUCCESS);
  assert_eq!(unsafe{io_prepare_pnp_resources(dev2,irp2,IRP_MN_START_DEVICE,list_ptr,list_ptr)},STATUS_DEVICE_BUSY);
  io_release_pnp_resources(dev1);
  assert_eq!(unsafe{io_prepare_pnp_resources(dev2,irp2,IRP_MN_START_DEVICE,list_ptr,list_ptr)},STATUS_SUCCESS);
  io_finish_pnp_resources(irp2,dev2,IRP_MN_START_DEVICE,STATUS_INVALID_DEVICE_REQUEST);
  unsafe{io_free_irp(irp1);io_free_irp(irp2);io_delete_device(dev1);io_delete_device(dev2);}
  // Overflowing ranges are rejected before they can enter the arbiter.
  list=memory_list(u64::MAX-3,8);
  let mut parsed=[ParsedResource::EMPTY;MAX_RESOURCES_PER_DEVICE];
  assert_eq!(unsafe{parse_cm_resource_list(list.as_ptr().cast(),&mut parsed)},Err(STATUS_INVALID_PARAMETER));
 }

 #[test]
 fn pnp_manager_dispatches_query_resources_and_requirements_irps() {
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut device:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)},STATUS_SUCCESS);
  driver.major_function[0x1b]=Some(pnp_start_complete_success);
  assert_eq!(unsafe{IoPnPQueryResources(device)},STATUS_SUCCESS);
  assert_eq!(unsafe{IoPnPQueryResourceRequirements(device)},STATUS_SUCCESS);
  unsafe{io_delete_device(device);}
 }

 #[test]
 fn pnp_manager_sends_real_start_stop_and_ordered_remove_irps() {
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut device:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)},STATUS_SUCCESS);
  driver.major_function[0x1b]=Some(pnp_start_complete_success);
  assert_eq!(unsafe{IoPnPStartDevice(device,ptr::null(),ptr::null())},STATUS_SUCCESS);
  assert_eq!(io_pnp_state(device),PNP_STATE_STARTED as i32);
  assert_eq!(unsafe{IoPnPStopDevice(device)},STATUS_SUCCESS);
  assert_eq!(io_pnp_state(device),PNP_STATE_STOPPED as i32);
  assert_eq!(unsafe{IoPnPRemoveDevice(device)},STATUS_SUCCESS);
  assert_eq!(io_pnp_state(device),PNP_STATE_REMOVED as i32);
  unsafe{io_delete_device(device);}
 }

 fn test_fs_mount_and_dismount_dispatch(device:*mut DeviceObject,irp:*mut Irp)->i32 {
  let stack=unsafe{io_get_current_irp_stack_location(irp)};
  if stack.is_null(){return STATUS_INVALID_PARAMETER}
  match unsafe{(*stack).minor_function} {
   IRP_MN_MOUNT_VOLUME=>{
    let vpb=unsafe{ptr::read_unaligned((*stack).parameters.as_ptr().add(12).cast::<*mut Vpb>())};
    if vpb.is_null(){return STATUS_INVALID_PARAMETER}
    let driver=unsafe{(*device).driver_object};
    let mut volume=ptr::null_mut();
    let status=unsafe{io_create_device(driver,0,ptr::null_mut(),FILE_DEVICE_DISK_FILE_SYSTEM,0,0,&mut volume)};
    if status!=STATUS_SUCCESS{return status}
    unsafe{(*vpb).device_object=volume.cast();}
    unsafe{io_complete_irp_status(irp,STATUS_SUCCESS,0,0);}
    STATUS_SUCCESS
   }
   0|IRP_MN_VERIFY_VOLUME=>{
    unsafe{io_complete_irp_status(irp,STATUS_SUCCESS,0,0);}
    STATUS_SUCCESS
   }
   _=>STATUS_INVALID_DEVICE_REQUEST
  }
 }

 #[test]
 fn filesystem_mount_verify_and_dismount_use_real_wdm_irps_and_vpb() {
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  driver.major_function[IRP_MJ_FILE_SYSTEM_CONTROL as usize]=Some(test_fs_mount_and_dismount_dispatch);
  let mut fs_control:*mut DeviceObject=ptr::null_mut();
  let mut disk:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),FILE_DEVICE_DISK_FILE_SYSTEM,0,0,&mut fs_control)},STATUS_SUCCESS);
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),FILE_DEVICE_DISK,0,0,&mut disk)},STATUS_SUCCESS);
  assert!(!unsafe{(*disk).vpb}.is_null());
  unsafe{io_register_file_system(fs_control);}
  assert!(is_file_system_registered(fs_control));
  assert_eq!(unsafe{io_verify_volume(disk,0)},STATUS_SUCCESS);
  let vpb=unsafe{(*disk).vpb.cast::<Vpb>()};
  assert_ne!(unsafe{(*vpb).flags}&crate::nt_abi::io::VPB_MOUNTED,0);
  let volume=unsafe{(*vpb).device_object.cast::<DeviceObject>()};
  assert!(!volume.is_null());
  assert_eq!(unsafe{(*volume).device_type},FILE_DEVICE_DISK_FILE_SYSTEM);
  assert_eq!(io_dismount_volume(disk),STATUS_SUCCESS);
  assert_eq!(unsafe{(*vpb).flags}&crate::nt_abi::io::VPB_MOUNTED,0);
  assert!(unsafe{(*vpb).device_object}.is_null());
  unsafe{
   io_delete_device(volume);
   io_delete_device(fs_control);
   io_delete_device(disk);
  }
  assert!(!is_file_system_registered(fs_control));
 }


 #[test]
 fn filesystem_mount_snapshot_pins_control_device_across_unregister() {
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  driver.major_function[IRP_MJ_FILE_SYSTEM_CONTROL as usize]=Some(test_fs_mount_and_dismount_dispatch);
  let mut fs:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),FILE_DEVICE_DISK_FILE_SYSTEM,0,0,&mut fs)},STATUS_SUCCESS);
  unsafe{io_register_file_system(fs);}
  assert_eq!(unsafe{(*fs).reference_count},2);
  let snapshot=snapshot_registered_filesystems();
  assert_eq!(snapshot.count,1);
  assert_eq!(unsafe{(*fs).reference_count},3);
  unsafe{io_unregister_file_system(fs);}
  assert_eq!(unsafe{(*fs).reference_count},2);
  drop(snapshot);
  assert_eq!(unsafe{(*fs).reference_count},1);
  unsafe{io_delete_device(fs);}
 }


 static FS_NOTIFY_ACTIVE:core::sync::atomic::AtomicUsize=core::sync::atomic::AtomicUsize::new(0);
 static FS_NOTIFY_INACTIVE:core::sync::atomic::AtomicUsize=core::sync::atomic::AtomicUsize::new(0);
 unsafe extern "win64" fn test_fs_registration_callback(device:*mut DeviceObject,active:u8) {
  if device.is_null(){return}
  if active!=0 {FS_NOTIFY_ACTIVE.fetch_add(1,core::sync::atomic::Ordering::SeqCst);}
  else {FS_NOTIFY_INACTIVE.fetch_add(1,core::sync::atomic::Ordering::SeqCst);}
 }
 #[test]
 fn filesystem_registration_notifications_cover_existing_new_and_removed_devices() {
  FS_NOTIFY_ACTIVE.store(0,core::sync::atomic::Ordering::SeqCst);
  FS_NOTIFY_INACTIVE.store(0,core::sync::atomic::Ordering::SeqCst);
  let mut notifier:DriverObject=unsafe{core::mem::zeroed()};
  let mut fs_driver:DriverObject=unsafe{core::mem::zeroed()};
  fs_driver.major_function[IRP_MJ_FILE_SYSTEM_CONTROL as usize]=Some(test_fs_mount_and_dismount_dispatch);
  let mut first:*mut DeviceObject=ptr::null_mut();
  let mut second:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut fs_driver,0,ptr::null_mut(),FILE_DEVICE_DISK_FILE_SYSTEM,0,0,&mut first)},STATUS_SUCCESS);
  assert_eq!(unsafe{io_create_device(&mut fs_driver,0,ptr::null_mut(),FILE_DEVICE_DISK_FILE_SYSTEM,0,0,&mut second)},STATUS_SUCCESS);
  unsafe{io_register_file_system(first);}
  assert_eq!(unsafe{io_register_fs_registration_change(&mut notifier,Some(test_fs_registration_callback))},STATUS_SUCCESS);
  assert!(FS_NOTIFY_ACTIVE.load(core::sync::atomic::Ordering::SeqCst)>=1);
  unsafe{io_register_file_system(second);}
  assert!(FS_NOTIFY_ACTIVE.load(core::sync::atomic::Ordering::SeqCst)>=2);
  unsafe{io_unregister_file_system(second);}
  assert!(FS_NOTIFY_INACTIVE.load(core::sync::atomic::Ordering::SeqCst)>=1);
  unsafe{io_unregister_fs_registration_change(&mut notifier,Some(test_fs_registration_callback));}
  unsafe{io_unregister_file_system(first);io_delete_device(first);io_delete_device(second);}
 }

 #[test]
 fn related_device_lookup_uses_file_object_device() {
  let mut driver:DriverObject=unsafe{core::mem::zeroed()};
  let mut device:*mut DeviceObject=ptr::null_mut();
  assert_eq!(unsafe{io_create_device(&mut driver,0,ptr::null_mut(),7,0,0,&mut device)},STATUS_SUCCESS);
  let mut file:FileObject=unsafe{core::mem::zeroed()};
  file.device_object=device;
  assert_eq!(unsafe{io_get_related_device_object(&mut file)},device.cast());
  assert!(unsafe{io_get_related_device_object(ptr::null_mut())}.is_null());
  unsafe{io_delete_device(device);}
 }

}


// ---- Requestor session query ------------------------------------------------
/// NTSTATUS IoGetRequestorSessionId(PIRP Irp, PULONG pSessionId).
/// Only IRPs owned by the kernel's tracked I/O manager are accepted; unknown
/// requestor identity fails rather than being silently treated as session zero.
#[unsafe(export_name = "IoGetRequestorSessionId")]