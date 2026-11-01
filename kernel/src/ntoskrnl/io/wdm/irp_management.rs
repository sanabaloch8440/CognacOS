fn initialize_irp_slot(slot:&mut IrpSlot,stack_size:i8)->*mut Irp {
 let next=slot.next;
 slot.used=true;slot.owned_mdl=ptr::null_mut();slot.completion_state=0;
 slot.suspended_completion=false;slot.free_requested=false;slot.active_ops=0;
 slot.outstanding_count=0;slot.outstanding_devices.fill(ptr::null_mut());
 slot.system_buffer=ptr::null_mut();slot.system_buffer_pages=0;slot.system_buffer_len=0;
 slot.ioctl_output_buffer=ptr::null_mut();slot.ioctl_output_len=0;slot.built_sync_ioctl=false;
 slot.sync_thread_id=0;slot.requestor_pid=crate::thread::current_process_id();slot.sync_event=ptr::null_mut();slot.sync_next=ptr::null_mut();slot.sync_prev=ptr::null_mut();slot.sync_queued=false;
 slot.object.bytes.fill(0);
 slot.object.bytes[0..2].copy_from_slice(&6i16.to_le_bytes());
 slot.object.bytes[2..4].copy_from_slice(&(IRP_SIZE_X64 as u16).to_le_bytes());
 slot.object.bytes[0x42]=stack_size as u8;slot.object.bytes[0x43]=(stack_size as u8).wrapping_add(1);
 slot.stack.fill(IoStackLocation{major_function:0,minor_function:0,flags:0,control:0,parameters:[0;0x20],device_object:ptr::null_mut(),file_object:ptr::null_mut(),completion_routine:None,context:ptr::null_mut()});
 slot.next=next;
 let object=ptr::addr_of_mut!(slot.object);
 let one_past=unsafe{slot.stack.as_mut_ptr().add(stack_size as usize).cast::<c_void>()};
 unsafe{write_ptr(ptr::addr_of_mut!((*object).bytes).cast::<u8>(),IRP_STACK_OFFSET,one_past);}
 object
}
#[unsafe(export_name = "IoAllocateIrp")]
pub unsafe extern "win64" fn io_allocate_irp(stack_size:i8, charge_quota:u8)->*mut Irp {
 if stack_size<=0||stack_size as usize>MAX_WDM_STACK_LOCATIONS||charge_quota>1{return ptr::null_mut()}
 {
  let mut state=WDM.lock();
  if let Some(slot)=state.irps.iter_mut().find(|s|!s.used){return initialize_irp_slot(slot,stack_size)}
 }
 // Growing the nonpaged metadata pool can enter the VM allocator. Never do
 // that at DISPATCH_LEVEL; pre-grown slots remain usable at elevated IRQL.
 if unsafe{crate::ntoskrnl::ke::runtime::ke_get_current_irql()}>1{return ptr::null_mut()}
 // Grow in slabs, not one VM extent per IRP. This avoids turning the VM
 // extent-descriptor count into a hidden small IRP limit. Slots remain reusable
 // after IoFreeIrp; backing pages are retained as a nonpaged high-water pool.
 const IRP_SLAB_PAGES:usize=256;
 let slab_bytes=IRP_SLAB_PAGES*PAGE_SIZE;
 let slot_bytes=core::mem::size_of::<IrpSlot>();
 let count=slab_bytes/slot_bytes;
 if count==0{return ptr::null_mut()}
 let memory=match crate::ntoskrnl::mm::kernel_space::allocate_pages(IRP_SLAB_PAGES){Ok(p)=>p,Err(_)=>return ptr::null_mut()};
 let mut state=WDM.lock();
 for index in 0..count {
  let candidate=unsafe{memory.add(index*slot_bytes).cast::<IrpSlot>()};
  unsafe{ptr::write(candidate,IrpSlot::empty());}
  state.irps.push_front(candidate);
 }
 let Some(slot)=state.irps.iter_mut().find(|s|!s.used) else{return ptr::null_mut()};
 initialize_irp_slot(slot,stack_size)
}
/// Copy the original requestor identity to a derived IRP. Internal forwarding
/// paths must use this when an IRP is created on behalf of a request from a
/// different thread/process (for example, a worker that builds a lower-stack IRP).
pub(crate) unsafe fn io_inherit_requestor_identity(destination: *mut Irp, source: *const Irp) -> bool {
 if destination.is_null() || source.is_null() { return false; }
 let mut state=WDM.lock();
 let source_pid=state.irps.iter().find(|slot|slot.used&&ptr::eq(&slot.object,source)).map(|slot|slot.requestor_pid);
 let Some(source_pid)=source_pid else { return false; };
 let Some(destination_slot)=state.irps.iter_mut().find(|slot|slot.used&&ptr::eq(&slot.object,destination)) else { return false; };
 destination_slot.requestor_pid=source_pid;
 true
}

#[unsafe(export_name = "IoFreeIrp")]
pub unsafe extern "win64" fn io_free_irp(irp:*mut Irp) {
 if irp.is_null(){return}
 {
  let mut state=WDM.lock();
  let Some(index)=state.irps.iter().position(|slot|slot.used&&ptr::eq(&slot.object,irp)) else{return};
  if state.irps[index].completion_state==1 {state.irps[index].free_requested=true;return}
  if state.irps[index].completion_state==3||state.irps[index].active_ops!=0||(state.irps[index].completion_state==0&&state.irps[index].outstanding_count!=0){return}
  state.irps[index].free_requested=false;state.irps[index].suspended_completion=false;state.irps[index].completion_state=2;
  let _=sync_queue_detach_locked(&mut state,irp);
 }
 release_irp_device_rundown(irp);
 let (owned_mdl,system_buffer,system_buffer_pages)={
  let mut state=WDM.lock();
  let Some(slot)=state.irps.iter_mut().find(|s|s.used&&ptr::eq(&s.object,irp)) else{return};
  let mdl=slot.owned_mdl;slot.owned_mdl=ptr::null_mut();
  let buffer=slot.system_buffer;let pages=slot.system_buffer_pages;
  slot.object.bytes.fill(0);slot.system_buffer=ptr::null_mut();slot.system_buffer_pages=0;slot.system_buffer_len=0;
  slot.ioctl_output_buffer=ptr::null_mut();slot.ioctl_output_len=0;slot.built_sync_ioctl=false;
  slot.sync_thread_id=0;slot.sync_event=ptr::null_mut();slot.used=false;
  (mdl,buffer,pages)
 };
 crate::dma::release_irp_mappings(irp.cast());
 if !owned_mdl.is_null(){unsafe{crate::ntoskrnl::mm::runtime::mm_free_mdl(owned_mdl);}}
 if !system_buffer.is_null()&&system_buffer_pages!=0{let _=crate::ntoskrnl::mm::kernel_space::free_pages(system_buffer,system_buffer_pages);}
}
#[unsafe(export_name = "IoGetCurrentIrpStackLocation")]
pub unsafe extern "win64" fn io_get_current_irp_stack_location(irp:*mut Irp)->*mut IoStackLocation {
 if irp.is_null(){return ptr::null_mut()}
 unsafe{read_ptr(ptr::addr_of!((*irp).bytes).cast::<u8>(),IRP_STACK_OFFSET).cast()}
}
#[unsafe(export_name = "IoGetNextIrpStackLocation")]
pub unsafe extern "win64" fn io_get_next_irp_stack_location(irp:*mut Irp)->*mut IoStackLocation {
 if irp.is_null(){return ptr::null_mut()}
 let current=unsafe{io_get_current_irp_stack_location(irp)};
 if current.is_null(){return ptr::null_mut()}
 // The WDM ABI places stack locations immediately after the fixed IRP.
 let count=unsafe{*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x42) as usize};
 if count==0||count>0x7f{return ptr::null_mut()}
 let base=(irp as usize).saturating_add(IRP_SIZE_X64);
 let end=base.saturating_add(count*IO_STACK_LOCATION_SIZE_X64);
 let cur=current as usize;
 if cur<=base||cur>end||(cur-base)%IO_STACK_LOCATION_SIZE_X64!=0{return ptr::null_mut()}
 unsafe{current.cast::<u8>().sub(IO_STACK_LOCATION_SIZE_X64).cast()}
}
#[unsafe(no_mangle)]

/// Pin a device for the lifetime of an IRP, not merely for IoCallDriver.
/// This bounded list prevents unload from reclaiming devices with callbacks
/// still in flight or an asynchronous request still outstanding.
fn acquire_irp_active_op(irp:*mut Irp)->bool {
 let mut state=WDM.lock();
 let Some(slot)=state.irps.iter_mut().find(|s|s.used&&ptr::eq(&s.object,irp)) else{return false};
 if slot.completion_state==2 || slot.active_ops==u32::MAX {return false}
 slot.active_ops+=1;true
}

/// Pin a device for the lifetime of an IRP, not merely for IoCallDriver.
fn track_irp_device(irp:*mut Irp, device:*mut DeviceObject)->bool {
 let mut state=WDM.lock();
 let Some(ir)=state.irps.iter().position(|s|s.used&&ptr::eq(&s.object,irp)) else{return false};
 if state.irps[ir].completion_state!=0 || state.irps[ir].outstanding_count>=state.irps[ir].outstanding_devices.len(){return false}
 if state.irps[ir].outstanding_devices[..state.irps[ir].outstanding_count].contains(&device){return true}
 let Some(di)=state.devices.iter().position(|s|s.used&&!s.deleting&&ptr::eq(&s.object,device)) else{return false};
 if state.devices[di].outstanding_irps==u32::MAX{return false}
 state.devices[di].outstanding_irps+=1;
 let n=state.irps[ir].outstanding_count;
 state.irps[ir].outstanding_devices[n]=device;
 state.irps[ir].outstanding_count=n+1;
 true
}

/// Release the IRP's device references once. The caller must keep the IRP
/// allocated until this function returns.
fn release_irp_active_op(irp:*mut Irp) {
 let (sync_terminal,release_regular_rundown)={
  let mut state=WDM.lock();
  let Some(index)=state.irps.iter().position(|s|s.used&&ptr::eq(&s.object,irp)) else{return};
  if state.irps[index].active_ops==0{return}
  state.irps[index].active_ops-=1;
  let drained=state.irps[index].active_ops==0;
  (drained&&state.irps[index].completion_state==3,
   drained&&state.irps[index].completion_state==2&&state.irps[index].outstanding_count!=0)
 };
 if sync_terminal {finalize_terminal_sync_irp(irp);return}
 if release_regular_rundown {release_irp_device_rundown(irp);}
 try_free_completed_sync_ioctl_irp(irp);
}

/// Drop every device rundown reference attached to an IRP exactly once.
fn try_finalize_deleting_device(device:*mut DeviceObject) {
 if device.is_null(){return}
 let finalize={
  let state=WDM.lock();
  state.devices.iter().find(|s|s.used&&ptr::eq(&s.object,device))
   .is_some_and(|s|s.deleting&&s.object.reference_count<=1&&s.remove_lock_count==0
    &&s.active_dispatches==0&&s.outstanding_irps==0&&!s.power_transition)
 };
 if finalize {unsafe{io_delete_device(device);}}
}

fn release_irp_device_rundown(irp:*mut Irp) {
 let devices={
  let mut state=WDM.lock();
  let Some(i)=state.irps.iter().position(|s|s.used&&ptr::eq(&s.object,irp)) else{return};
  let count=state.irps[i].outstanding_count.min(state.irps[i].outstanding_devices.len());
  let devices=state.irps[i].outstanding_devices;
  state.irps[i].outstanding_count=0;
  state.irps[i].outstanding_devices.fill(ptr::null_mut());
  for device in devices[..count].iter().copied() {
   if let Some(d)=state.devices.iter_mut().find(|s|s.used&&ptr::eq(&s.object,device)) {
    if d.outstanding_irps!=0 {d.outstanding_irps-=1;}
   }
  }
  devices
 };
 // Releasing an IRP's rundown pin must never request device deletion. Only
 // finalize objects whose owner has already called IoDeleteDevice.
 for device in devices.into_iter().filter(|d|!d.is_null()) {try_finalize_deleting_device(device);}
}

fn release_device_dispatch_rundown(device:*mut DeviceObject){
 let finalize={
  let mut state=WDM.lock();
  let Some(slot)=state.devices.iter_mut().find(|s|s.used&&ptr::eq(&s.object,device)) else{return};
  if slot.active_dispatches!=0{slot.active_dispatches-=1;}
  slot.deleting&&slot.active_dispatches==0&&slot.object.reference_count<=1&&slot.remove_lock_count==0&&!slot.power_transition
 };
 if finalize{unsafe{io_delete_device(device)}}
}

#[unsafe(export_name = "IoCallDriver")]
pub unsafe extern "win64" fn io_call_driver(device:*mut DeviceObject, irp:*mut Irp)->i32 {
 if device.is_null()||irp.is_null(){return STATUS_INVALID_PARAMETER}
 let (driver, deleting)={
  let mut state=WDM.lock();
  match state.devices.iter_mut().find(|s|s.used&&ptr::eq(&s.object,device)) {
   Some(slot) if !slot.deleting&&slot.pnp_state!=PNP_STATE_REMOVED&&slot.object.driver_object!=ptr::null_mut() => {
    if slot.active_dispatches==u32::MAX{return STATUS_DEVICE_BUSY}
    slot.active_dispatches+=1;(slot.object.driver_object,false)
   },
   Some(_)=>(ptr::null_mut(),true),None=>(ptr::null_mut(),true)
  }
 };
 if deleting||driver.is_null(){return STATUS_DELETE_PENDING}
 // Windows requires a completion routine when a synchronous builder IRP has
 // no Event to signal. Validate before mutating the IRP stack or taking rundown.
 let needs_completion={
  let state=WDM.lock();
  state.irps.iter().find(|s|s.used&&ptr::eq(&s.object,irp))
   .is_some_and(|s|s.built_sync_ioctl&&s.sync_event.is_null())
 };
 if needs_completion {
  let next=unsafe{io_get_next_irp_stack_location(irp)};
  if next.is_null()||unsafe{(*next).completion_routine.is_none()} {
   // Persist structured state before writing serial text. This survives later
   // faults and is included in the kernel's fatal trace dump.
   let current=unsafe{io_get_current_irp_stack_location(irp)} as usize as u64;
   let next_address=next as usize as u64;
   let callback=if next.is_null(){0}else{unsafe{(*next).completion_routine.map(|routine|routine as usize as u64).unwrap_or(0)}};
   let stack_count=unsafe{*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x42) as u64};
   let current_location=unsafe{*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x43) as u64};
   crate::ntoskrnl::debug::record_unfiltered(
    crate::ntoskrnl::debug::COMPONENT_IO,
    crate::ntoskrnl::debug::LEVEL_ERROR,
    crate::ntoskrnl::debug::EVENT_IO_CALL_REJECTED,
    next_address,
    (stack_count<<32)|current_location,
    current,
   );
   unsafe {
    crate::serial_write(b"WDM IRP COMPLETION GUARD: irp=");
    crate::serial_write_hex(irp as usize as u64);
    crate::serial_write(b" current=");
    crate::serial_write_hex(current);
    crate::serial_write(b" next=");
    crate::serial_write_hex(next_address);
    crate::serial_write(b" callback=");
    crate::serial_write_hex(callback);
    crate::serial_write(b" stack_count=");
    crate::serial_write_decimal(stack_count as usize);
    crate::serial_write(b" current_location=");
    crate::serial_write_decimal(current_location as usize);
    crate::serial_write(b"\\r\\n");
   }
   release_device_dispatch_rundown(device);
   return STATUS_INVALID_PARAMETER;
  }
 }
 if !track_irp_device(irp,device) { release_device_dispatch_rundown(device); return STATUS_DELETE_PENDING; }
 if !acquire_irp_active_op(irp) { release_device_dispatch_rundown(device); return STATUS_DELETE_PENDING; }
 let irp_stack_count=unsafe{*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x42) as usize};
 let required_stack_size=unsafe{(*device).stack_size};
 if required_stack_size<=0 {release_irp_active_op(irp);release_device_dispatch_rundown(device);return STATUS_INVALID_DEVICE_REQUEST;}
 let required_stack_count=required_stack_size as usize;
 if irp_stack_count<required_stack_count {
  release_irp_active_op(irp);release_device_dispatch_rundown(device);
  return STATUS_INVALID_DEVICE_REQUEST;
 }
 let stack=unsafe{io_get_next_irp_stack_location(irp)};
 if stack.is_null(){release_irp_active_op(irp);release_device_dispatch_rundown(device);return STATUS_INVALID_PARAMETER}
 let location=unsafe{*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x43)};
 if location<=1{release_irp_active_op(irp);release_device_dispatch_rundown(device);return STATUS_INVALID_DEVICE_REQUEST}
 unsafe {
  write_ptr(ptr::addr_of_mut!((*irp).bytes).cast::<u8>(),IRP_STACK_OFFSET,stack.cast());
  *ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(0x43)=location-1;
  (*stack).device_object=device;
 }
 let major=unsafe{(*stack).major_function as usize};
 if major>=28{release_irp_active_op(irp);release_device_dispatch_rundown(device);return STATUS_INVALID_DEVICE_REQUEST}
 let dispatch=unsafe{(*driver).major_function[major]};
 let Some(dispatch)=dispatch else{release_irp_active_op(irp);release_device_dispatch_rundown(device);return STATUS_INVALID_DEVICE_REQUEST};
 let parameters=unsafe{ptr::read_unaligned((*stack).parameters.as_ptr().cast::<[u64;4]>())};
 let prepare=unsafe{crate::ntoskrnl::po::runtime::po_prepare_irp_dispatch(device,irp,major as u8,(*stack).minor_function,parameters)};
 if prepare!=STATUS_SUCCESS{
  unsafe{write_ptr(ptr::addr_of_mut!((*irp).bytes).cast::<u8>(),IRP_STACK_OFFSET,stack.cast::<u8>().add(IO_STACK_LOCATION_SIZE_X64).cast());*ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(0x43)=location;}
  release_irp_active_op(irp);release_device_dispatch_rundown(device);return prepare
 }
 let minor=unsafe{(*stack).minor_function};
 let routine_address=dispatch as *const () as usize as u64;
 let driver_base=unsafe{(*driver).driver_start as usize as u64};
 let routine_rva=routine_address.checked_sub(driver_base).unwrap_or(u64::MAX);
 let debug_driver=crate::ntoskrnl::debug::query_filter(
  crate::ntoskrnl::debug::COMPONENT_DRIVER,
  crate::ntoskrnl::debug::LEVEL_INFO,
 );
 crate::ntoskrnl::debug::trace(
  crate::ntoskrnl::debug::COMPONENT_DRIVER,
  crate::ntoskrnl::debug::LEVEL_INFO,
  crate::ntoskrnl::debug::EVENT_DRIVER_DISPATCH,
  device as usize as u64,
  ((major as u64)<<32)|(minor as u64),
  routine_address,
 );
 if debug_driver {
  unsafe {
   crate::serial_write(b"DRIVER DEBUG: dispatch enter image=");
   crate::serial_write_hex(driver_base);
   crate::serial_write(b" RVA=");
   crate::serial_write_hex(routine_rva);
   crate::serial_write(b" routine=");
   crate::serial_write_hex(routine_address);
   crate::serial_write(b" major=");
   crate::serial_write_hex(major as u64);
   crate::serial_write(b" minor=");
   crate::serial_write_hex(minor as u64);
   crate::serial_write(b" device=");
   crate::serial_write_hex(device as usize as u64);
   crate::serial_write(b" irp=");
   crate::serial_write_hex(irp as usize as u64);
   crate::serial_write(b"\\r\\n");
  }
 }
 let status=unsafe{dispatch(device,irp)};
 crate::ntoskrnl::debug::trace(
  crate::ntoskrnl::debug::COMPONENT_DRIVER,
  crate::ntoskrnl::debug::LEVEL_INFO,
  crate::ntoskrnl::debug::EVENT_DRIVER_DISPATCH_RETURN,
  irp as usize as u64,
  status as u32 as u64,
  routine_address,
 );
 if debug_driver {
  unsafe {
   crate::serial_write(b"DRIVER DEBUG: dispatch return status=");
   crate::serial_write_hex(status as u32 as u64);
   crate::serial_write(b" image=");
   crate::serial_write_hex(driver_base);
   crate::serial_write(b" RVA=");
   crate::serial_write_hex(routine_rva);
   crate::serial_write(b" routine=");
   crate::serial_write_hex(routine_address);
   crate::serial_write(b" irp=");
   crate::serial_write_hex(irp as usize as u64);
   crate::serial_write(b"\\r\\n");
  }
 }
 // STATUS_PENDING is not a substitute for IoMarkIrpPending. The lower
 // driver marks its own stack location; completion unwind propagates it.
 crate::ntoskrnl::po::runtime::po_finish_dispatch_return(irp,device,status);
 release_irp_active_op(irp);
 try_free_completed_sync_ioctl_irp(irp);
 release_device_dispatch_rundown(device);
 status
}
/// Build a synchronous device-control IRP. METHOD_BUFFERED and the two
/// direct methods are supported; METHOD_NEITHER preserves the caller's kernel
/// pointers. A supplied event is signaled after the IOSB is finalized and the
/// IRP reaches terminal completion. Event == NULL requires the caller to install
/// a completion routine before calling IoCallDriver.
#[unsafe(export_name = "IoBuildDeviceIoControlRequest")]
pub unsafe extern "win64" fn io_build_device_io_control_request(
 ioctl_code:u32, device:*mut DeviceObject, input_buffer:*mut c_void,
 input_length:u32, output_buffer:*mut c_void, output_length:u32,
 internal_device_control:u8, event:*mut c_void, io_status:*mut IoStatusBlock
)->*mut Irp {
 let creator_thread=crate::thread::current_thread_id().unwrap_or(0);
 // IoBuildDeviceIoControlRequest is a synchronous, thread-associated DDI and
 // Windows documents it for IRQL <= APC_LEVEL. Host-side tests can validate
 // packet construction without a live scheduler thread.
 if unsafe{crate::ntoskrnl::ke::runtime::ke_get_current_irql()}>1{return ptr::null_mut()}
 #[cfg(not(test))]
 if creator_thread==0 {return ptr::null_mut()}
 if device.is_null()||io_status.is_null()||internal_device_control>1
  ||(input_length!=0&&input_buffer.is_null())||(output_length!=0&&output_buffer.is_null()) {
  return ptr::null_mut()
 }
 let stack_size={
  let state=WDM.lock();
  let Some(slot)=state.devices.iter().find(|s|s.used&&!s.deleting&&ptr::eq(&s.object,device)) else{return ptr::null_mut()};
  slot.object.stack_size
 };
 if stack_size<=0{return ptr::null_mut()}
 let method=(ioctl_code&3) as u8;
 let required_buffer=match method {0=>input_length.max(output_length) as usize,1|2=>input_length as usize,_=>0};
 let irp=unsafe{io_allocate_irp(stack_size,0)};
 if irp.is_null(){return ptr::null_mut()}
 let next=unsafe{io_get_next_irp_stack_location(irp)};
 if next.is_null(){unsafe{io_free_irp(irp)};return ptr::null_mut()}
 let (system_buffer,system_buffer_pages)=if required_buffer==0 {(ptr::null_mut(),0)} else {
  let pages=match required_buffer.checked_add(PAGE_SIZE-1){Some(n)=>n/PAGE_SIZE,None=>{unsafe{io_free_irp(irp)};return ptr::null_mut()}};
  match crate::ntoskrnl::mm::kernel_space::allocate_pages(pages) {
   Ok(p)=>{unsafe{ptr::write_bytes(p,0,pages*PAGE_SIZE);} (p,pages)},
   Err(_)=>{unsafe{io_free_irp(irp)};return ptr::null_mut()}
  }
 };
 {
  let mut state=WDM.lock();
  let Some(index)=state.irps.iter().position(|s|s.used&&ptr::eq(&s.object,irp)) else{drop(state);unsafe{io_free_irp(irp)};return ptr::null_mut()};
  let slot=&mut state.irps[index];
  if method!=3 {
   slot.system_buffer=system_buffer;slot.system_buffer_pages=system_buffer_pages;
   slot.system_buffer_len=if method==0 {required_buffer}else{input_length as usize};
   if input_length!=0 {unsafe{ptr::copy_nonoverlapping(input_buffer.cast::<u8>(),slot.system_buffer,input_length as usize);}}
   write_ptr(slot.object.bytes.as_mut_ptr(),0x18,slot.system_buffer.cast());
   if method==0 {
    slot.ioctl_output_buffer=output_buffer;
    slot.ioctl_output_len=output_length as usize;
    slot.object.bytes[0x10..0x14].copy_from_slice(&(0x04u32|0x10u32|0x20u32).to_le_bytes());
   } else {
    slot.object.bytes[0x10..0x14].copy_from_slice(&(0x04u32|0x10u32|0x20u32).to_le_bytes());
   }
   slot.built_sync_ioctl=true;
   slot.sync_thread_id=creator_thread;
   slot.sync_event=event;
  } else {
   // METHOD_NEITHER's Type3InputBuffer and Irp.UserBuffer are kernel pointers
   // supplied by this kernel-mode caller, as required by this DDI contract.
   slot.object.bytes[0x10..0x14].copy_from_slice(&0x04u32.to_le_bytes());
   slot.built_sync_ioctl=true;
   slot.sync_thread_id=crate::thread::current_thread_id().unwrap_or(0);
   slot.sync_event=event;
  }
  // UserBuffer retains the caller's output pointer for all transfer methods;
  // drivers use SystemBuffer or MdlAddress according to the IOCTL method.
  write_ptr(slot.object.bytes.as_mut_ptr(),0x70,output_buffer);
  write_ptr(slot.object.bytes.as_mut_ptr(),0x48,io_status.cast());
  ptr::write_unaligned(slot.object.bytes.as_mut_ptr().add(IRP_STATUS_OFFSET).cast::<i32>(),STATUS_PENDING);
  ptr::write_unaligned(slot.object.bytes.as_mut_ptr().add(IRP_INFORMATION_OFFSET).cast::<usize>(),0);
 }
 let attached={let mut state=WDM.lock();sync_queue_attach_locked(&mut state,irp,creator_thread)};
 if !attached {unsafe{io_free_irp(irp)};return ptr::null_mut()}
 unsafe {
  (*next).major_function=if internal_device_control!=0{0x0f}else{0x0e};
  // On x64, InputBufferLength and IoControlCode are POINTER_ALIGNMENT
  // fields in the DeviceIoControl union (offsets 8 and 16 within Parameters).
  ptr::write_unaligned(next.cast::<u8>().add(4).cast::<u32>(),output_length);
  ptr::write_unaligned(next.cast::<u8>().add(12).cast::<u32>(),input_length);
  ptr::write_unaligned(next.cast::<u8>().add(20).cast::<u32>(),ioctl_code);
  if method==3 {
   ptr::write_unaligned(next.cast::<u8>().add(28).cast::<*mut c_void>(),input_buffer);
  }
 }
 if method==1||method==2 {
  if output_length!=0 {
   let mdl=unsafe{io_allocate_mdl(output_buffer,output_length,0,0,irp)};
   if mdl.is_null(){unsafe{io_free_irp(irp)};return ptr::null_mut()}
   // METHOD_IN_DIRECT grants the driver read access to the second buffer;
   // METHOD_OUT_DIRECT grants write access from the driver's perspective.
   unsafe{crate::ntoskrnl::mm::runtime::mm_probe_and_lock_pages(mdl,0,if method==1{0}else{1});}
   let flags=unsafe{ptr::read_unaligned(mdl.cast::<u8>().add(0x0a).cast::<i16>())};
   if flags&0x0002==0{unsafe{io_free_mdl(mdl);io_free_irp(irp);}return ptr::null_mut()}
   let mut state=WDM.lock();
   if let Some(slot)=state.irps.iter_mut().find(|s|s.used&&ptr::eq(&s.object,irp)){slot.owned_mdl=mdl;}
  }
 }
 irp
}

/// Internal synchronous IRP builder used by file-object services. The
/// Object Manager/I/O Manager retain the request until completion has copied
/// the IOSB and signalled the caller's event.
pub(crate) unsafe fn io_build_synchronous_irp(
    major: u8,
    device: *mut DeviceObject,
    buffer: *mut c_void,
    length: u32,
    starting_offset: i64,
    event: *mut c_void,
    io_status: *mut IoStatusBlock,
) -> *mut Irp {
    let creator_thread = crate::thread::current_thread_id().unwrap_or(0);
    if unsafe { crate::ntoskrnl::ke::runtime::ke_get_current_irql() } > 1
        || device.is_null() || event.is_null() || io_status.is_null()
        || major >= 28
        || ((major == IRP_MJ_READ || major == IRP_MJ_WRITE) && length != 0 && buffer.is_null()) {
        return ptr::null_mut();
    }
    #[cfg(not(test))]
    if creator_thread == 0 { return ptr::null_mut(); }
    let stack_size = {
        let state = WDM.lock();
        let Some(slot) = state.devices.iter().find(|slot| slot.used && !slot.deleting && ptr::eq(&slot.object, device)) else {
            return ptr::null_mut();
        };
        slot.object.stack_size
    };
    if stack_size <= 0 { return ptr::null_mut(); }
    let irp = unsafe { io_allocate_irp(stack_size, 0) };
    if irp.is_null() { return ptr::null_mut(); }
    let stack = unsafe { io_get_next_irp_stack_location(irp) };
    if stack.is_null() { unsafe { io_free_irp(irp); } return ptr::null_mut(); }
    {
        let mut state = WDM.lock();
        let Some(index) = state.irps.iter().position(|slot| slot.used && ptr::eq(&slot.object, irp)) else {
            drop(state);
            unsafe { io_free_irp(irp); }
            return ptr::null_mut();
        };
        let slot = &mut state.irps[index];
        slot.built_sync_ioctl = true;
        slot.sync_thread_id = creator_thread;
        slot.sync_event = event;
        slot.requestor_pid = crate::thread::current_process_id();
        slot.object.bytes[0x10..0x14].copy_from_slice(
            &(0x04u32 | if major == IRP_MJ_WRITE { 0x40u32 } else { 0 }).to_le_bytes()
        );
        unsafe {
            write_ptr(slot.object.bytes.as_mut_ptr(), 0x48, io_status.cast());
            write_ptr(slot.object.bytes.as_mut_ptr(), 0x70, buffer);
            ptr::write_unaligned(io_status.cast::<u8>().cast::<i32>(), STATUS_PENDING);
            ptr::write_unaligned(io_status.cast::<u8>().add(8).cast::<usize>(), 0);
            ptr::write_unaligned(slot.object.bytes.as_mut_ptr().add(IRP_STATUS_OFFSET).cast::<i32>(), STATUS_PENDING);
            ptr::write_unaligned(slot.object.bytes.as_mut_ptr().add(IRP_INFORMATION_OFFSET).cast::<usize>(), 0);
        }
    }
    if !{ let mut state = WDM.lock(); sync_queue_attach_locked(&mut state, irp, creator_thread) } {
        unsafe { io_free_irp(irp); }
        return ptr::null_mut();
    }
    unsafe {
        (*stack).major_function = major;
        if major == IRP_MJ_READ || major == IRP_MJ_WRITE {
            let _ = io_write_stack_u32(stack, 0, length);
            let _ = io_write_stack_i64(stack, 8, starting_offset);
        }
    }
    irp
}

/// Windows x64 IoBuildSynchronousFsdRequest ABI.
#[unsafe(export_name = "IoBuildSynchronousFsdRequest")]
pub unsafe extern "win64" fn io_build_synchronous_fsd_request(
    major_function: u32,
    device: *mut DeviceObject,
    buffer: *mut c_void,
    length: u32,
    starting_offset: *mut i64,
    event: *mut c_void,
    io_status: *mut IoStatusBlock,
) -> *mut Irp {
    if major_function > u8::MAX as u32 { return ptr::null_mut(); }
    let major = major_function as u8;
    if !(major == IRP_MJ_READ || major == IRP_MJ_WRITE || major == 0x09 || major == 0x10) {
        return ptr::null_mut();
    }
    if (major == IRP_MJ_READ || major == IRP_MJ_WRITE) && starting_offset.is_null() {
        return ptr::null_mut();
    }
    let offset = if starting_offset.is_null() { 0 } else { unsafe { *starting_offset } };
    unsafe { io_build_synchronous_irp(major, device, buffer, length, offset, event, io_status) }
}

fn try_free_completed_sync_ioctl_irp(irp:*mut Irp) {
 if irp.is_null(){return}
 let ready={
  let state=WDM.lock();
  state.irps.iter().find(|s|s.used&&ptr::eq(&s.object,irp))
   .is_some_and(|s|(s.built_sync_ioctl||s.free_requested)&&s.completion_state==2&&s.active_ops==0)
 };
 if ready {unsafe{io_free_irp(irp);}}
}

/// Finish the last phase of a builder-owned synchronous IRP. State 3 means the
/// IRP is terminal but still pinned so thread exit cannot release the caller's
/// event/IOSB storage between queue detachment and event signaling.
fn finalize_terminal_sync_irp(irp:*mut Irp) {
 let event={
  let mut state=WDM.lock();
  let Some(index)=state.irps.iter().position(|s|s.used&&ptr::eq(&s.object,irp)) else{return};
  if !state.irps[index].built_sync_ioctl||state.irps[index].completion_state!=3||state.irps[index].active_ops!=0{return}
  state.irps[index].active_ops=1; // temporary notification pin
  state.irps[index].sync_event
 };
 release_irp_device_rundown(irp);
 if !event.is_null(){unsafe{crate::ntoskrnl::ke::runtime::ke_set_event(event.cast(),0,0);}}
 {
  let mut state=WDM.lock();
  if let Some(index)=state.irps.iter().position(|s|s.used&&ptr::eq(&s.object,irp)) {
   if state.irps[index].completion_state==3&&state.irps[index].active_ops==1 {
    state.irps[index].active_ops=0;
    state.irps[index].completion_state=2;
    let _=sync_queue_detach_locked(&mut state,irp);
   }
  }
 }
 try_free_completed_sync_ioctl_irp(irp);
}

fn finalize_irp_status(irp:*mut Irp) {
 if irp.is_null(){return}
 let base=unsafe{ptr::addr_of_mut!((*irp).bytes).cast::<u8>()};
 let iosb=unsafe{read_ptr(base,0x48)};
 if iosb.is_null(){return}
 let status=unsafe{ptr::read_unaligned(base.add(IRP_STATUS_OFFSET).cast::<i32>())};
 let information=unsafe{ptr::read_unaligned(base.add(IRP_INFORMATION_OFFSET).cast::<usize>())};
 let copy_out={
  let mut state=WDM.lock();
  let Some(slot)=state.irps.iter_mut().find(|s|s.used&&ptr::eq(&s.object,irp)) else{return};
  if slot.built_sync_ioctl && (unsafe{*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x10)&0x10})!=0 && !slot.ioctl_output_buffer.is_null() {
   let n=information.min(slot.ioctl_output_len).min(slot.system_buffer_len);
   if n!=0 {unsafe{ptr::copy_nonoverlapping(slot.system_buffer,slot.ioctl_output_buffer.cast::<u8>(),n);}}
  }
  slot.built_sync_ioctl
 };
 unsafe {
  ptr::write_unaligned(iosb.cast::<u8>().cast::<i32>(),status);
  ptr::write_unaligned(iosb.cast::<u8>().add(8).cast::<usize>(),information);
 }
 let _=copy_out;
}

/// Complete a kernel-owned IRP with an explicit status and information value.
/// Native drivers use this service instead of depending on private IRP offsets.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn io_complete_irp_status(
 irp:*mut Irp,status:i32,information:usize,priority_boost:i8
) {
 if irp.is_null(){return}
 unsafe {
  ptr::write_unaligned(ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(IRP_STATUS_OFFSET).cast::<i32>(),status);
  ptr::write_unaligned(ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(IRP_INFORMATION_OFFSET).cast::<usize>(),information);
  io_complete_request(irp,priority_boost);
 }
}

#[unsafe(export_name = "IoCompleteRequest")]
pub unsafe extern "win64" fn io_complete_request(irp:*mut Irp, _priority_boost:i8) {
 if irp.is_null(){return}
 // Claim completion under WDM's lock. A competing completion cannot execute
 // the same completion routine twice or publish the IOSB twice.
 {
  let mut state=WDM.lock();
  let Some(i)=state.irps.iter().position(|s|s.used&&ptr::eq(&s.object,irp)) else{return};
  if state.irps[i].completion_state!=0{return}
  state.irps[i].suspended_completion=false;
  state.irps[i].completion_state=1;
 }
 let mut finished=false;
 for _ in 0..(MAX_WDM_STACK_LOCATIONS+1) {
  let (location,stack_count)=unsafe{(*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x43) as usize,*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x42) as usize)};
  if stack_count==0||stack_count>MAX_WDM_STACK_LOCATIONS||location==0 {break}
  if location>stack_count {
   finalize_irp_status(irp);
   let status=unsafe{ptr::read_unaligned(ptr::addr_of!((*irp).bytes).cast::<u8>().add(IRP_STATUS_OFFSET).cast::<i32>())};
   crate::dma::release_irp_mappings(irp.cast());
   crate::ntoskrnl::po::runtime::po_complete_irp(irp,status);
   finished=true;
   break
  }
  let stack=unsafe{io_get_current_irp_stack_location(irp)};
  if stack.is_null(){break}
  let control=unsafe{(*stack).control};
  // SL_PENDING_RETURNED is converted into IRP.PendingReturned during unwind.
  if control&0x01!=0 {unsafe{*ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(0x41)=1;}}
  let routine=unsafe{(*stack).completion_routine};
  let context=unsafe{(*stack).context};
  let status=unsafe{ptr::read_unaligned(ptr::addr_of!((*irp).bytes).cast::<u8>().add(IRP_STATUS_OFFSET).cast::<i32>())};
  let cancelled=unsafe{*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x44)!=0};
  let invoke=(status>=0&&(control&0x40)!=0)||(status<0&&(control&0x80)!=0)||(cancelled&&(control&0x20)!=0);
  // The I/O manager advances the current stack location before invoking the
  // completion callback. If it returns STATUS_MORE_PROCESSING_REQUIRED, that
  // advanced location is retained so a later IoCompleteRequest resumes above
  // this callback instead of invoking the same callback twice.
  let next=unsafe{stack.cast::<u8>().add(IO_STACK_LOCATION_SIZE_X64).cast::<IoStackLocation>()};
  // The completion callback belongs to the next-higher driver. Windows passes
  // that driver's DeviceObject, not the lower target stored in this stack slot.
  let completion_device_object=if location<stack_count {unsafe{(*next).device_object}} else {ptr::null_mut()};
  unsafe {
   *ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(0x43)=(location+1).min(0xff) as u8;
   write_ptr(ptr::addr_of_mut!((*irp).bytes).cast::<u8>(),IRP_STACK_OFFSET,next.cast());
   // Pending state must be visible to each higher driver's completion frame.
   if *ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x41)!=0 && (location+1)<=stack_count {
    (*next).control|=0x01;
   }
  }
  if invoke {
   if let Some(routine)=routine {
    let result=unsafe{routine(completion_device_object.cast(),irp.cast(),context)};
    if result==0xC0000016u32 as i32 {
     {
      let mut state=WDM.lock();
      if let Some(i)=state.irps.iter().position(|s|s.used&&ptr::eq(&s.object,irp)) {
       if state.irps[i].free_requested {
        state.irps[i].completion_state=2;
        state.irps[i].suspended_completion=false;
       } else {
        state.irps[i].completion_state=0;
        state.irps[i].suspended_completion=true;
       }
      } else {return}
     }
     try_free_completed_sync_ioctl_irp(irp);
     return
    } else {
     // Freeing an IRP is only legal from a completion callback when that
     // callback stops completion; otherwise retain the packet.
     let mut state=WDM.lock();
     if let Some(i)=state.irps.iter().position(|s|s.used&&ptr::eq(&s.object,irp)) {
      state.irps[i].free_requested=false;
     }
    }
   }
  }
  if unsafe{*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x43)>*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x42)} {
   finalize_irp_status(irp);
   let status=unsafe{ptr::read_unaligned(ptr::addr_of!((*irp).bytes).cast::<u8>().add(IRP_STATUS_OFFSET).cast::<i32>())};
   crate::dma::release_irp_mappings(irp.cast());
   crate::ntoskrnl::po::runtime::po_complete_irp(irp,status);
   finished=true;
   break
  }
 }
 {
  let mut state=WDM.lock();
  if let Some(i)=state.irps.iter().position(|s|s.used&&ptr::eq(&s.object,irp)) {
   // Synchronous builder IRPs stay queued/pinned through notification so
   // thread exit cannot race the last event signal.
   state.irps[i].completion_state=if !finished {0}else if state.irps[i].built_sync_ioctl {3}else{2};
  }
 }
 if finished {
  io_pnp_manager_irp_completed(irp);
  let sync_builder={
   let state=WDM.lock();
   state.irps.iter().find(|slot|slot.used&&ptr::eq(&slot.object,irp))
    .is_some_and(|slot|slot.built_sync_ioctl)
  };
  if sync_builder {
   finalize_terminal_sync_irp(irp);
  } else {
   let callbacks_drained={
    let state=WDM.lock();
    state.irps.iter().find(|slot|slot.used&&ptr::eq(&slot.object,irp))
     .is_some_and(|slot|slot.active_ops==0)
   };
   if callbacks_drained {release_irp_device_rundown(irp);}
   try_free_completed_sync_ioctl_irp(irp);
  }
 }
}
#[unsafe(export_name = "IoMarkIrpPending")]
pub unsafe extern "win64" fn io_mark_irp_pending(irp:*mut Irp) {
 if irp.is_null(){return}
 // Windows sets SL_PENDING_RETURNED on the current stack location; the I/O
 // manager propagates this into IRP.PendingReturned while unwinding.
 let stack=unsafe{io_get_current_irp_stack_location(irp)};
 if !stack.is_null(){unsafe{(*stack).control|=0x01;}}
}

#[unsafe(export_name = "IoSetCompletionRoutine")]
pub unsafe extern "win64" fn io_set_completion_routine(
 irp:*mut Irp, routine:Option<IoCompletionRoutine>, context:*mut c_void,
 invoke_success:u8, invoke_error:u8, invoke_cancel:u8
) {
 if irp.is_null(){return}
 let stack=unsafe{io_get_next_irp_stack_location(irp)};
 if stack.is_null(){return}
 unsafe {
  (*stack).completion_routine=routine;(*stack).context=context;
  (*stack).control=(if invoke_success!=0{0x40}else{0})|(if invoke_error!=0{0x80}else{0})|(if invoke_cancel!=0{0x20}else{0});
 }
}


