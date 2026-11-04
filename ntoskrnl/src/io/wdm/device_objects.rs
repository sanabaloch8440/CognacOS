unsafe fn read_ptr(base:*const u8, offset:usize)->*mut c_void { unsafe { ptr::read_unaligned(base.add(offset).cast::<*mut c_void>()) } }
unsafe fn write_ptr(base:*mut u8, offset:usize, value:*mut c_void) { unsafe { ptr::write_unaligned(base.add(offset).cast::<*mut c_void>(),value); } }
unsafe fn write_usize(base:*mut u8, offset:usize, value:usize) { unsafe { ptr::write_unaligned(base.add(offset).cast::<usize>(),value); } }
unsafe fn write_isize(base:*mut u8, offset:usize, value:isize) { unsafe { ptr::write_unaligned(base.add(offset).cast::<isize>(),value); } }

// StartIo queue state is stored in each stable DeviceSlot, so queue capacity
// grows with the nonpaged device registry rather than imposing a second global
// device-count ceiling. StartIo callbacks always run after dropping WDM.
unsafe fn invoke_start_io(device: *mut DeviceObject, irp: *mut Irp) {
    if device.is_null() || irp.is_null() { return; }
    let driver = unsafe { (*device).driver_object };
    if driver.is_null() { return; }
    let callback = unsafe { (*driver).driver_start_io };
    if callback.is_null() { return; }
    let start: unsafe extern "win64" fn(*mut DeviceObject, *mut Irp) = unsafe { core::mem::transmute(callback) };
    unsafe { start(device, irp); }
}

/// VOID IoSetStartIoAttributes(PDEVICE_OBJECT, BOOLEAN KeyedDevice, BOOLEAN BoostedIo).
pub unsafe extern "win64" fn io_set_start_io_attributes(device: *mut DeviceObject, keyed_device: u8, boosted_io: u8) {
    if device.is_null() { return; }
    let mut state=WDM.lock();
    if let Some(slot)=state.devices.iter_mut().find(|s|s.used&&!s.deleting&&ptr::eq(&s.object,device)){
        slot.start_io.keyed=keyed_device!=0;
        slot.start_io.boosted=boosted_io!=0;
    }
}

/// VOID IoStartPacket(PDEVICE_OBJECT, PIRP, PULONG Key, PDRIVER_CANCEL CancelFunction).
pub unsafe extern "win64" fn io_start_packet(device: *mut DeviceObject, irp: *mut Irp, key: *const u32, cancel_function: Option<DriverCancelRoutine>) {
    if device.is_null() || irp.is_null() { return; }
    // IoStartPacket must publish the cancel routine and queue membership as
    // one cancel-spin-lock transaction. Otherwise cancellation can run between
    // routine installation and insertion, leaving a canceled IRP queued later.
    let mut cancel_old_irql=0u8;
    let cancel_locked=cancel_function.is_some();
    if let Some(cancel)=cancel_function {
        unsafe{io_acquire_cancel_spin_lock(&mut cancel_old_irql);}
        let _=unsafe{io_set_cancel_routine(irp,Some(cancel));};
    }
    let key_value=if key.is_null(){0}else{unsafe{key.read_unaligned()}};
    let queued={
        let mut state=WDM.lock();
        if let Some(slot)=state.devices.iter_mut().find(|s|s.used&&!s.deleting&&ptr::eq(&s.object,device)){
            if unsafe{(*device).current_irp.is_null()} {
                unsafe{(*device).current_irp=irp;}
                Some(true)
            }else{
                let q=&mut slot.start_io;
                if q.count>=START_IO_DEPTH{None}else{
                    let at=if q.keyed{q.keys[..q.count].iter().position(|k|*k>key_value).unwrap_or(q.count)}else{q.count};
                    for n in (at..q.count).rev(){q.entries[n+1]=q.entries[n];q.keys[n+1]=q.keys[n];}
                    q.entries[at]=irp;q.keys[at]=key_value;q.count+=1;
                    Some(false)
                }
            }
        }else{None}
    };
    if queued.is_none()&&cancel_locked {
        // Still holding the cancel lock: prevent a failed enqueue from leaving
        // a callback installed for an IRP that this queue does not own.
        let _=unsafe{io_set_cancel_routine(irp,None);};
    }
    if cancel_locked{unsafe{io_release_cancel_spin_lock(cancel_old_irql);}}
    if let Some(true)=queued{unsafe{invoke_start_io(device,irp);}}
}

/// VOID IoStartNextPacket(PDEVICE_OBJECT, BOOLEAN Cancelable).
pub unsafe extern "win64" fn io_start_next_packet(device: *mut DeviceObject, _cancelable: u8) {
    if device.is_null() { return; }
    let (next,finalize_deleting)={
        let mut state=WDM.lock();
        let Some(slot)=state.devices.iter_mut().find(|s|s.used&&ptr::eq(&s.object,device)) else{return};
        unsafe{(*device).current_irp=ptr::null_mut();}
        let q=&mut slot.start_io;
        let next=if q.count==0{ptr::null_mut()}else{
            let irp=q.entries[0];
            for n in 1..q.count{q.entries[n-1]=q.entries[n];q.keys[n-1]=q.keys[n];}
            q.count-=1;q.entries[q.count]=ptr::null_mut();q.keys[q.count]=0;
            unsafe{(*device).current_irp=irp;}
            irp
        };
        (next,slot.deleting&&next.is_null()&&q.count==0)
    };
    if !next.is_null(){unsafe{invoke_start_io(device,next);}}
    else if finalize_deleting{unsafe{io_delete_device(device);}}
}

#[unsafe(export_name = "IoCreateDevice")]
pub unsafe extern "win64" fn io_create_device(
 driver:*mut DriverObject, extension_size:u32, device_name:*mut UnicodeString,
 device_type:u32, characteristics:u32, exclusive:u8, output:*mut *mut DeviceObject
)->i32 {
 if driver.is_null() || output.is_null() || extension_size as usize>MAX_DEVICE_EXTENSION || exclusive>1 { return STATUS_INVALID_PARAMETER; }
 unsafe { output.write(ptr::null_mut()); }
 let mut counted_name=[0u16;260];
 let name_len=if !device_name.is_null() {
  let name=unsafe{&*device_name};
  let bytes=name.length as usize;
  if name.length>name.maximum_length || name.length%2!=0 || bytes>counted_name.len()*2 || (bytes!=0&&name.buffer.is_null()) {return STATUS_INVALID_PARAMETER;}
  let units=bytes/2;
  if units!=0 {unsafe{ptr::copy_nonoverlapping(name.buffer,counted_name.as_mut_ptr(),units);}}
  units
 } else {0};
 let mut state=WDM.lock();
 if unsafe{(*driver).flags}&(DRIVER_UNLOADING_FLAG|DRIVER_UNLOADED_FLAG)!=0{return STATUS_DELETE_PENDING}
 if name_len!=0&&state.devices.iter().any(|s|s.used&&s.name_len==name_len&&unicode_equal_folded(&s.name[..s.name_len],&counted_name[..name_len])) {return STATUS_OBJECT_NAME_COLLISION}
 // Keep the first slots embedded for boot-time allocations, then grow the
 // nonpaged registry one stable page at a time at PASSIVE_LEVEL. Allocation is
 // deliberately outside WDM's spin lock; published overflow records are never
 // moved and remain reusable after IoDeleteDevice completes rundown.
 if !state.devices.iter().any(|s|!s.used) {
  drop(state);
  if unsafe{crate::ntoskrnl::ke::runtime::ke_get_current_irql()}>1{return STATUS_INSUFFICIENT_RESOURCES}
  let memory=match crate::ntoskrnl::mm::kernel_space::allocate_pages(1){Ok(p)=>p,Err(_)=>return STATUS_INSUFFICIENT_RESOURCES};
  if core::mem::size_of::<DeviceSlot>()>4096 {
   let _=crate::ntoskrnl::mm::kernel_space::free_pages(memory,1);
   return STATUS_INSUFFICIENT_RESOURCES
  }
  let extra=memory.cast::<DeviceSlot>();
  unsafe{ptr::write(extra,DeviceSlot::empty());}
  state=WDM.lock();
  if unsafe{(*driver).flags}&(DRIVER_UNLOADING_FLAG|DRIVER_UNLOADED_FLAG)!=0 {
   drop(state);let _=crate::ntoskrnl::mm::kernel_space::free_pages(memory,1);return STATUS_DELETE_PENDING
  }
  if name_len!=0&&state.devices.iter().any(|s|s.used&&s.name_len==name_len&&unicode_equal_folded(&s.name[..s.name_len],&counted_name[..name_len])) {
   drop(state);let _=crate::ntoskrnl::mm::kernel_space::free_pages(memory,1);return STATUS_OBJECT_NAME_COLLISION
  }
  state.devices.push_overflow(extra);
 }
 let Some(slot)=state.devices.iter_mut().find(|s|!s.used) else {return STATUS_INSUFFICIENT_RESOURCES};
 slot.used=true;slot.deleting=false;slot.active_dispatches=0;slot.outstanding_irps=0;slot.remove_lock_count=0;slot.power_state=0;slot.power_target=0;slot.power_transition=false;slot.power_capabilities=0x09;slot.wake_supported=false;slot.pnp_state=0;slot.pnp_pending=0;slot.pnp_transition_minor=0;slot.pnp_transition=false;slot.mount_in_progress=false;slot.attached_to=ptr::null_mut();slot.start_io=StartIoQueue::EMPTY;
 slot.vpb=Vpb::empty();
 slot.name_len=name_len;slot.name=[0;260];slot.name[..name_len].copy_from_slice(&counted_name[..name_len]);
 slot.extension.fill(0);
 let next_device=unsafe{(*driver).device_object};
 slot.object=DeviceObject {
  object_type:3,size:(core::mem::size_of::<DeviceObject>()+extension_size as usize) as u16,reference_count:1,
  driver_object:driver,next_device,attached_device:ptr::null_mut(),current_irp:ptr::null_mut(),timer:ptr::null_mut(),
  flags:0,characteristics, vpb:ptr::null_mut(),
  device_extension:if extension_size==0 {ptr::null_mut()} else {slot.extension.as_mut_ptr().cast()},
  device_type,stack_size:1,queue_padding:[0;3],queue:[0;0x48],
  alignment_requirement:63,device_queue_padding:[0;4],
  device_queue:KDeviceQueue{type_:0,size:core::mem::size_of::<KDeviceQueue>() as i16,device_list_head:[0;0x10],lock:0,busy:0,padding:[0;7]},
  dpc:[0;0x40],active_thread_count:0,security_descriptor:ptr::null_mut(),device_lock:[0;0x18],
  sector_size:0,spare1:0,device_object_extension:ptr::null_mut(),reserved:ptr::null_mut(),
 };
 let object=&mut slot.object as *mut DeviceObject;
 if matches!(device_type,FILE_DEVICE_DISK|FILE_DEVICE_CD_ROM|FILE_DEVICE_TAPE|FILE_DEVICE_VIRTUAL_DISK) {
  slot.vpb.real_device=object.cast();
  slot.vpb.reference_count=0;
  slot.object.vpb=ptr::addr_of_mut!(slot.vpb).cast();
 }
 unsafe { (*driver).device_object=object; output.write(object); }
 drop(state);
 // The I/O-manager name buffer and the device-slot name buffer share the
 // same 260-WCHAR bound. Do not impose a second, shorter limit when
 // publishing the object to the Object Manager.
 let mut ob_name=[0u16;260];
 ob_name[..name_len].copy_from_slice(&counted_name[..name_len]);
 let type_name=[b'D' as u16,b'e' as u16,b'v' as u16,b'i' as u16,b'c' as u16,b'e' as u16];
 if unsafe{crate::ntoskrnl::ob::runtime::register_external_object_with_finalizer(object.cast(),&type_name,&ob_name[..name_len],false,ptr::null(),0,io_object_manager_finalizer)}.is_err() {
  unsafe{io_delete_device(object)}; return STATUS_INSUFFICIENT_RESOURCES;
 }
 if crate::ntoskrnl::po::device_power::register_default(object)!=STATUS_SUCCESS {
  unsafe{io_delete_device(object)};return STATUS_INSUFFICIENT_RESOURCES;
 }
 STATUS_SUCCESS
}


/// Mark a live device created by a bus driver as a physical device object.
/// The object remains owned by its creating driver and participates in the
/// normal PnP relations/rundown path.
#[unsafe(export_name = "IoMarkBusEnumeratedPdo")]
pub unsafe extern "win64" fn io_mark_bus_enumerated_pdo(device:*mut DeviceObject)->i32{
 if device.is_null(){return STATUS_INVALID_PARAMETER}
 let mut state=WDM.lock();
 let Some(slot)=state.devices.iter_mut().find(|s|s.used&&!s.deleting&&ptr::eq(&s.object,device)) else{return STATUS_INVALID_PARAMETER};
 if slot.pnp_state!=PNP_STATE_NOT_STARTED{return STATUS_DEVICE_BUSY}
 slot.object.flags|=0x0000_0200;
 STATUS_SUCCESS
}

/// Private extension carried by PCI bus-enumerated physical device objects.
/// Identity is immutable for the PDO lifetime and sourced from pci.sys'
/// validated inventory, never from a second kernel configuration-space scan.
pub const PCI_PDO_EXTENSION_MAGIC:u64=0x434f_474e_5044_4f31;
#[repr(C)]
#[derive(Clone,Copy)]
pub struct PciPdoExtension {
 pub magic:u64,
 pub version:u32,
 pub information:crate::nt_abi::bus::PCI_DEVICE_INFORMATION,
}
unsafe extern "win64" fn io_object_manager_finalizer(object:*mut c_void){unsafe{io_delete_device(object.cast::<DeviceObject>());}}

/// Begin driver unload only after all owned devices have completed active
/// dispatch, remove-lock, reference and power-transition rundown. Repeated calls
/// are safe: the unload callback is atomically claimed by clearing its pointer.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn io_unload_driver(driver:*mut DriverObject)->i32 {
 if driver.is_null(){return STATUS_INVALID_PARAMETER}
 // Close the loader's reference gate before device rundown. If clients still
 // hold driver references, unload remains pending until they dereference.
 let references_drained=crate::ntoskrnl::driver_loader::begin_driver_unload(
  driver.cast::<crate::ntoskrnl::driver_loader::DriverObject>()
 );
 {
  let _state=WDM.lock();
  let flags=unsafe{(*driver).flags};
  if flags&DRIVER_UNLOADED_FLAG!=0{return STATUS_SUCCESS}
  // Close IoCreateDevice before returning PENDING for outstanding driver
  // references. The loader reference gate and DRIVER_OBJECT flag must agree.
  unsafe{(*driver).flags=flags|DRIVER_UNLOADING_FLAG;}
 }
 if !references_drained{return STATUS_PENDING}
 // Walk the driver's intrusive DEVICE_OBJECT chain. Pin each next object
 // while the current object is still protected by WDM, so concurrent removal
 // cannot reclaim the traversal target. This covers every device, including
 // page-backed overflow records, without a fixed-size snapshot or retry loop.
 let mut cursor={
  let mut state=WDM.lock();
  let head=unsafe{(*driver).device_object};
  if head.is_null(){ptr::null_mut()}else{
   match state.devices.iter_mut().find(|s|s.used&&!s.deleting&&ptr::eq(&s.object,head)){
    Some(slot)=>match slot.object.reference_count.checked_add(1){
     Some(count)=>{slot.object.reference_count=count;head},
     None=>ptr::null_mut()
    },
    None=>ptr::null_mut()
   }
  }
 };
 while !cursor.is_null(){
  let (next,found_current)={
   let mut state=WDM.lock();
   if let Some(index)=state.devices.iter().position(|s|s.used&&ptr::eq(&s.object,cursor)){
    let next=state.devices[index].object.next_device;
    state.devices[index].deleting=true;
    let pinned_next=if next.is_null(){ptr::null_mut()}else{
     match state.devices.iter_mut().find(|s|s.used&&!s.deleting&&ptr::eq(&s.object,next)){
      Some(next_slot)=>match next_slot.object.reference_count.checked_add(1){
       Some(count)=>{next_slot.object.reference_count=count;next},
       None=>ptr::null_mut()
      },
      None=>ptr::null_mut()
     }
    };
    (pinned_next,true)
   }else{(ptr::null_mut(),false)}
  };
  if !found_current{unsafe{io_dereference_device_object(cursor);}break}
  unsafe{io_delete_device(cursor);}
  unsafe{io_dereference_device_object(cursor);}
  cursor=next;
 }
 let callback={
  let state=WDM.lock();
  if state.devices.iter().any(|s|s.used&&s.object.driver_object==driver){return STATUS_PENDING}
  drop(state);
  let _state=WDM.lock();
  unsafe{
   let callback=(*driver).driver_unload;
   (*driver).driver_unload=None;
   (*driver).flags|=DRIVER_UNLOADED_FLAG|DRIVER_UNLOADING_FLAG;
   callback
  }
 };
 if let Some(callback)=callback{unsafe{callback(driver)}}
 // The driver image remains pinned through device rundown and DriverUnload.
 // Releasing it earlier could evict/free the code while its callback executes.
 let _ = unsafe { crate::ntoskrnl::driver_loader::unload_driver_image(driver.cast::<c_void>()) };
 STATUS_SUCCESS
}

#[unsafe(export_name = "IoDeleteDevice")]
pub unsafe extern "win64" fn io_delete_device(device:*mut DeviceObject) {
 if device.is_null(){return}
 // Stop new filesystem mount discovery before dropping the control device.
 unsafe{io_unregister_file_system(device);}
 let (mounted_vpb,mount_in_progress)={
  let mut state=WDM.lock();
  match state.devices.iter_mut().find(|s|s.used&&ptr::eq(&s.object,device)) {
   Some(slot) if !slot.object.vpb.is_null() => {
    let vpb=slot.object.vpb.cast::<Vpb>();
    let mut vpb_irql=0u8;
    unsafe{io_acquire_vpb_spin_lock(&mut vpb_irql);}
    let is_mounted=unsafe{(*vpb).flags&crate::nt_abi::io::VPB_MOUNTED!=0};
    let mounting=slot.mount_in_progress;
    if is_mounted||mounting {
     unsafe{(*vpb).flags|=crate::nt_abi::io::VPB_REMOVE_PENDING;}
     slot.deleting=true;
    }
    unsafe{io_release_vpb_spin_lock(vpb_irql);}
    (is_mounted,mounting)
   }
   _=>(false,false)
  }
 };
 if mount_in_progress{return}
 if mounted_vpb {
  // Keep the physical device alive until its filesystem has accepted the
  // dismount IRP; asynchronous completion retries deletion automatically.
  if io_dismount_volume(device)!=STATUS_SUCCESS{return}
 }
 let _ = crate::ntoskrnl::ob::runtime::unregister_object(device.cast());
 let mut state=WDM.lock();
 let Some(index)=state.devices.iter().position(|s|s.used&&ptr::eq(&s.object,device)) else{return};
 state.devices[index].deleting=true;
 // Defer reclamation while external references, remove-lock owners, or power transitions remain.
 if state.devices[index].object.reference_count > 1 || state.devices[index].remove_lock_count != 0 || state.devices[index].active_dispatches != 0 || state.devices[index].outstanding_irps != 0 || state.devices[index].power_transition || !state.devices[index].object.current_irp.is_null() || state.devices[index].start_io.count != 0 { return; }
 // Ob references and handles may outlive Io callers; only reclaim the backing slot after Ob has removed its header record.
 if !crate::ntoskrnl::ob::runtime::object_rundown_complete(device.cast()) { return; }
 let lower=state.devices[index].attached_to;
 let upper=state.devices[index].object.attached_device;
 if !lower.is_null() {
  unsafe { if (*lower).attached_device==device { (*lower).attached_device=upper; } }
 }
 if !upper.is_null() {
  if let Some(upper_index)=state.devices.iter().position(|s|s.used&&ptr::eq(&s.object,upper)) {
   state.devices[upper_index].attached_to=lower;
  }
 }
 let driver=state.devices[index].object.driver_object;
 if !driver.is_null() {
  unsafe {
   let mut cursor=ptr::addr_of_mut!((*driver).device_object);
   while !(*cursor).is_null() {
    if *cursor==device { *cursor=(*device).next_device;break; }
    cursor=ptr::addr_of_mut!((**cursor).next_device);
   }
  }
 }
 let slot_ptr=&mut state.devices[index] as *mut DeviceSlot;
 let slot=unsafe{&mut *slot_ptr};
 slot.object.device_extension=ptr::null_mut();
 slot.object.driver_object=ptr::null_mut();
 slot.object.next_device=ptr::null_mut();
 slot.object.attached_device=ptr::null_mut();
 slot.attached_to=ptr::null_mut();
 slot.name_len=0;slot.name.fill(0);slot.start_io=StartIoQueue::EMPTY;
 slot.used=false;slot.deleting=false;
 // Drop stale bus/child links before making the backing device slot reusable.
 {
  let mut manager=PNP_MANAGER.lock();
  for relation in manager.relations.iter_mut().filter(|r|r.used&&(r.bus==device||r.child==device)){*relation=PnpRelation::EMPTY;}
 }
 // Overflow slots are individually page-backed. Remove the registry link while
 // holding WDM, but free the page only after all subsystem rundown is complete.
 let release_overflow_page=state.devices.remove_overflow(slot_ptr);
 drop(state);
 // Drop the upper device's pin on its immediate lower attachment after unlinking.
 if !lower.is_null(){unsafe{io_dereference_device_object(lower);}}
 crate::ntoskrnl::po::device_power::unregister(device);
 io_release_pnp_resources(device);
 if release_overflow_page{let _=crate::ntoskrnl::mm::kernel_space::free_pages(slot_ptr.cast(),1);}

}