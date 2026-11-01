const MAX_REGISTERED_FILESYSTEMS:usize=16;
const STATUS_UNRECOGNIZED_VOLUME:i32=0xC000_014F_u32 as i32;
const STATUS_WRONG_VOLUME:i32=0xC000_0012_u32 as i32;
const STATUS_MORE_PROCESSING_REQUIRED:i32=0xC000_0016_u32 as i32;
const FILE_DEVICE_DISK_FILE_SYSTEM:u32=0x0000_0008;
const FSCTL_DISMOUNT_VOLUME:u32=0x0009_0020;

pub type DriverFsNotification=unsafe extern "win64" fn(*mut DeviceObject,u8);
const MAX_FS_NOTIFICATIONS:usize=16;
#[derive(Clone,Copy)]
struct FsNotification { driver:*mut DriverObject, callback:Option<DriverFsNotification> }
unsafe impl Send for FsNotification {}
impl FsNotification { const EMPTY:Self=Self{driver:ptr::null_mut(),callback:None}; }
struct FileSystemRegistry {
 devices:[*mut DeviceObject;MAX_REGISTERED_FILESYSTEMS], count:usize,
 notifications:[FsNotification;MAX_FS_NOTIFICATIONS],
}
unsafe impl Send for FileSystemRegistry {}
impl FileSystemRegistry {
 const fn new()->Self{Self{
  devices:[ptr::null_mut();MAX_REGISTERED_FILESYSTEMS],count:0,
  notifications:[FsNotification::EMPTY;MAX_FS_NOTIFICATIONS],
 }}
}
static FILESYSTEMS:IrqSpinLock<FileSystemRegistry>=IrqSpinLock::new(FileSystemRegistry::new());

/// A stable, referenced snapshot of the registered filesystem devices.
/// Holding these references closes the unregister/delete race between
/// filesystem enumeration and dispatch of IRP_MN_MOUNT_VOLUME.
struct FileSystemSnapshot {
 devices:[*mut DeviceObject;MAX_REGISTERED_FILESYSTEMS],
 count:usize,
}
impl Drop for FileSystemSnapshot {
 fn drop(&mut self) {
  for device in self.devices[..self.count].iter().copied() {
   if !device.is_null() { unsafe { io_dereference_device_object(device); } }
  }
 }
}
fn snapshot_registered_filesystems()->FileSystemSnapshot {
 let mut snapshot=FileSystemSnapshot{devices:[ptr::null_mut();MAX_REGISTERED_FILESYSTEMS],count:0};
 // Keep the registry lock while taking object references. Unregister removes
 // under this lock and drops its registry reference only after releasing it.
 let registry=FILESYSTEMS.lock();
 for device in registry.devices[..registry.count].iter().copied() {
  if snapshot.count==MAX_REGISTERED_FILESYSTEMS { break; }
  let referenced=unsafe{io_reference_device_object(device)};
  if !referenced.is_null() {
   snapshot.devices[snapshot.count]=referenced;
   snapshot.count+=1;
  }
 }
 snapshot
}

/// Completion owns the mount IRP. A successful FSD mount must publish a live
/// FILE_DEVICE_DISK_FILE_SYSTEM volume device in VPB.DeviceObject; the I/O
/// manager then commits VPB_MOUNTED and pins that volume device until dismount.
unsafe extern "win64" fn mount_volume_completion(_device:*mut DeviceObject,irp:*mut Irp,context:*mut c_void)->i32 {
 if irp.is_null()||context.is_null(){return STATUS_MORE_PROCESSING_REQUIRED}
 let vpb=context.cast::<Vpb>();
 let mut retry_device=ptr::null_mut();
 let mut status=unsafe{io_get_irp_status(irp)};
 if status>=0 {
  let mut state=WDM.lock();
  let real_index_any=state.devices.iter().position(|s|s.used&&ptr::eq(&s.vpb,vpb));
  let real_index=real_index_any;
  let mut vpb_irql=0u8;
  unsafe{io_acquire_vpb_spin_lock(&mut vpb_irql);}
  let volume=unsafe{(*vpb).device_object.cast::<DeviceObject>()};
  let volume_index=state.devices.iter().position(|s|s.used&&!s.deleting&&ptr::eq(&s.object,volume));
  let already_mounted_elsewhere=state.devices.iter().any(|s|s.used&&!ptr::eq(&s.vpb,vpb)
   &&s.vpb.device_object.cast::<DeviceObject>()==volume&&s.vpb.flags&crate::nt_abi::io::VPB_MOUNTED!=0);
  match (real_index,volume_index) {
   (Some(_),Some(vi)) if !volume.is_null()&&!already_mounted_elsewhere
      &&state.devices[vi].object.device_type==FILE_DEVICE_DISK_FILE_SYSTEM
      &&state.devices[vi].object.reference_count<i32::MAX => {
    state.devices[vi].object.reference_count+=1;
    unsafe{(*vpb).flags|=crate::nt_abi::io::VPB_MOUNTED;}
   }
   _=>{
    status=STATUS_INVALID_DEVICE_REQUEST;
    unsafe{
     ptr::write_unaligned(ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(IRP_STATUS_OFFSET).cast::<i32>(),status);
     (*vpb).device_object=ptr::null_mut();
     (*vpb).flags&=!crate::nt_abi::io::VPB_MOUNTED;
    }
   }
  }
  unsafe{io_release_vpb_spin_lock(vpb_irql);}
  if let Some(ri)=real_index_any {
   state.devices[ri].mount_in_progress=false;
   if state.devices[ri].deleting {retry_device=ptr::addr_of_mut!(state.devices[ri].object);}
  }
 } else {
  let mut state=WDM.lock();
  if let Some(ri)=state.devices.iter().position(|s|s.used&&ptr::eq(&s.vpb,vpb)){
   state.devices[ri].mount_in_progress=false;
   if state.devices[ri].deleting {retry_device=ptr::addr_of_mut!(state.devices[ri].object);}
  }
  let mut vpb_irql=0u8;
  unsafe{io_acquire_vpb_spin_lock(&mut vpb_irql);}
  unsafe{
   (*vpb).device_object=ptr::null_mut();
   (*vpb).flags&=!crate::nt_abi::io::VPB_MOUNTED;
  }
  unsafe{io_release_vpb_spin_lock(vpb_irql);}
 }
 unsafe{io_free_irp(irp);}
 if !retry_device.is_null(){unsafe{io_delete_device(retry_device);}}
 STATUS_MORE_PROCESSING_REQUIRED
}

/// A verification IRP is allocated by the I/O manager and freed by its
/// completion routine, including when the filesystem completes asynchronously.
fn release_mount_rundown(device:*mut DeviceObject) {
 if device.is_null(){return}
 let retry_device={
  let mut state=WDM.lock();
  match state.devices.iter_mut().find(|s|s.used&&ptr::eq(&s.object,device)) {
   Some(slot)=>{
    slot.mount_in_progress=false;
    if slot.deleting {ptr::addr_of_mut!(slot.object)} else {ptr::null_mut()}
   }
   None=>ptr::null_mut()
  }
 };
 if !retry_device.is_null(){unsafe{io_delete_device(retry_device);}}
}

fn wdm_device_is_live(device:*mut DeviceObject)->bool {
 if device.is_null(){return false}
 let state=WDM.lock();
 state.devices.iter().any(|s|s.used&&!s.deleting&&ptr::eq(&s.object,device))
}

unsafe extern "win64" fn verify_volume_completion(_device:*mut DeviceObject,irp:*mut Irp,context:*mut c_void)->i32 {
 if !irp.is_null(){unsafe{io_free_irp(irp);}}
 if !context.is_null(){release_mount_rundown(context.cast::<DeviceObject>());}
 STATUS_MORE_PROCESSING_REQUIRED
}

/// Commit VPB teardown only after the filesystem accepts FSCTL_DISMOUNT_VOLUME.
/// The VPB's volume-device reference is released under the I/O-manager lock.
unsafe extern "win64" fn dismount_volume_completion(_device:*mut DeviceObject,irp:*mut Irp,context:*mut c_void)->i32 {
 if irp.is_null()||context.is_null(){return STATUS_MORE_PROCESSING_REQUIRED}
 let vpb=context.cast::<Vpb>();
 let status=unsafe{ptr::read_unaligned(ptr::addr_of!((*irp).bytes).cast::<u8>().add(IRP_STATUS_OFFSET).cast::<i32>())};
 let mut retry_device=ptr::null_mut();
 if status>=0 {
  let mut state=WDM.lock();
  let mut vpb_irql=0u8;
  unsafe{io_acquire_vpb_spin_lock(&mut vpb_irql);}
  let volume=unsafe{(*vpb).device_object.cast::<DeviceObject>()};
  let volume_index=state.devices.iter().position(|s|s.used&&ptr::eq(&s.object,volume));
  if let Some(vi)=volume_index {
   unsafe{
    (*vpb).device_object=ptr::null_mut();
    (*vpb).flags&=!(crate::nt_abi::io::VPB_MOUNTED|crate::nt_abi::io::VPB_LOCKED);
    (*vpb).volume_label_length=0;
    (*vpb).volume_label=[0;32];
   }
   if state.devices[vi].object.reference_count>1 {state.devices[vi].object.reference_count-=1;}
   if let Some(ri)=state.devices.iter().position(|s|s.used&&s.deleting&&ptr::eq(&s.vpb,vpb)) {
    if unsafe{(*vpb).flags&crate::nt_abi::io::VPB_REMOVE_PENDING!=0} {
     retry_device=ptr::addr_of_mut!(state.devices[ri].object);
    }
   }
  }
  unsafe{io_release_vpb_spin_lock(vpb_irql);}
 }
 unsafe{io_free_irp(irp);}
 if !retry_device.is_null(){unsafe{io_delete_device(retry_device);}}
 STATUS_MORE_PROCESSING_REQUIRED
}

/// Send the documented user-FS-request dismount control to the mounted volume.
/// Open VPB references prevent teardown; pending requests retain the IRP and
/// volume-device rundown until the filesystem completes the request.
fn io_dismount_volume(real_device:*mut DeviceObject)->i32 {
 let (vpb,volume,refs)={
  let state=WDM.lock();
  let Some(slot)=state.devices.iter().find(|s|s.used&&ptr::eq(&s.object,real_device)) else{return STATUS_INVALID_PARAMETER};
  if slot.object.vpb.is_null(){return STATUS_SUCCESS}
  let vpb=slot.object.vpb.cast::<Vpb>();
  let mut vpb_irql=0u8;
  unsafe{io_acquire_vpb_spin_lock(&mut vpb_irql);}
  let snapshot=unsafe{(vpb,(*vpb).device_object,(*vpb).reference_count)};
  unsafe{io_release_vpb_spin_lock(vpb_irql);}
  snapshot
 };
 if volume.is_null(){return STATUS_SUCCESS}
 if refs!=0{return STATUS_DEVICE_BUSY}
 let target=unsafe{io_reference_device_object(volume.cast::<DeviceObject>())};
 if target.is_null(){return STATUS_DELETE_PENDING}
 let stack_size=unsafe{(*target).stack_size};
 if stack_size<=0 {unsafe{io_dereference_device_object(target);}return STATUS_INVALID_DEVICE_REQUEST}
 let irp=unsafe{io_allocate_irp(stack_size,0)};
 if irp.is_null(){unsafe{io_dereference_device_object(target);}return STATUS_INSUFFICIENT_RESOURCES}
 let stack=unsafe{io_get_next_irp_stack_location(irp)};
 if stack.is_null(){unsafe{io_free_irp(irp);io_dereference_device_object(target);}return STATUS_INSUFFICIENT_RESOURCES}
 unsafe{
  (*stack).major_function=IRP_MJ_FILE_SYSTEM_CONTROL;
  (*stack).minor_function=0; // IRP_MN_USER_FS_REQUEST
  ptr::write_unaligned((*stack).parameters.as_mut_ptr().add(8).cast::<u32>(),FSCTL_DISMOUNT_VOLUME);
  io_set_completion_routine(irp,Some(dismount_volume_completion),vpb.cast(),1,1,1);
 }
 let status=unsafe{io_call_driver(target,irp)};
 if status!=STATUS_PENDING&&irp_completion_state(irp)==Some(0){unsafe{io_complete_irp_status(irp,status,0,0);}}
 unsafe{io_dereference_device_object(target);}
 status
}

fn irp_completion_state(irp:*mut Irp)->Option<u8>{
 let state=WDM.lock();
 state.irps.iter().find(|s|s.used&&ptr::eq(&s.object,irp)).map(|s|s.completion_state)
}

/// Build and dispatch the documented IRP_MJ_FILE_SYSTEM_CONTROL mount request.
/// The FSD supplies VPB.DeviceObject; the I/O manager validates and publishes
/// the mounted association from the completion routine.
fn dispatch_mount_request(real_device:*mut DeviceObject,vpb:*mut Vpb,fs_device:*mut DeviceObject)->i32 {
 if real_device.is_null()||vpb.is_null()||fs_device.is_null(){return STATUS_INVALID_PARAMETER}
 let fs=unsafe{io_reference_device_object(fs_device)};
 if fs.is_null(){return STATUS_DELETE_PENDING}
 let stack_size=unsafe{(*fs).stack_size};
 if stack_size<=0 {unsafe{io_dereference_device_object(fs);}return STATUS_INVALID_DEVICE_REQUEST}
 let irp=unsafe{io_allocate_irp(stack_size,0)};
 if irp.is_null(){unsafe{io_dereference_device_object(fs);}return STATUS_INSUFFICIENT_RESOURCES}
 let stack=unsafe{io_get_next_irp_stack_location(irp)};
 if stack.is_null(){unsafe{io_free_irp(irp);io_dereference_device_object(fs);}return STATUS_INSUFFICIENT_RESOURCES}
 unsafe{
  (*stack).major_function=IRP_MJ_FILE_SYSTEM_CONTROL;
  (*stack).minor_function=IRP_MN_MOUNT_VOLUME;
  let parameters=(*stack).parameters.as_mut_ptr();
  ptr::write_unaligned(parameters.add(4).cast::<*mut DeviceObject>(),real_device);
  ptr::write_unaligned(parameters.add(12).cast::<*mut Vpb>(),vpb);
  io_set_completion_routine(irp,Some(mount_volume_completion),vpb.cast(),1,1,1);
 }
 let status=unsafe{io_call_driver(fs,irp)};
 // WDM dispatch routines must complete synchronous requests. Complete a
 // returned non-pending request if a driver failed to do so, preserving the
 // returned status and ensuring the completion callback releases the packet.
 if status!=STATUS_PENDING&&irp_completion_state(irp)==Some(0) {
  unsafe{io_complete_irp_status(irp,status,0,0);}
 }
 unsafe{io_dereference_device_object(fs);}
 status
}

/// Register an active filesystem control device. A filesystem must expose the
/// FILE_SYSTEM_CONTROL dispatch entry before it can participate in mounts.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn io_register_file_system(device:*mut DeviceObject) {
 if device.is_null(){return}
 let valid={
  let state=WDM.lock();
  state.devices.iter().any(|s|s.used&&!s.deleting&&ptr::eq(&s.object,device)
   &&s.object.device_type==FILE_DEVICE_DISK_FILE_SYSTEM
   &&!s.object.driver_object.is_null()
   &&unsafe{(*s.object.driver_object).major_function[IRP_MJ_FILE_SYSTEM_CONTROL as usize].is_some()})
 };
 if !valid||unsafe{io_reference_device_object(device)}.is_null(){return}
 let (callbacks,event_reference)={
  let mut registry=FILESYSTEMS.lock();
  if registry.devices[..registry.count].contains(&device)||registry.count==MAX_REGISTERED_FILESYSTEMS {
   drop(registry);
   unsafe{io_dereference_device_object(device);}
   return
  }
  let insert_index=registry.count;
  registry.devices[insert_index]=device;
  registry.count=insert_index+1;
  let mut callbacks=[None;MAX_FS_NOTIFICATIONS];
  for (i,entry) in registry.notifications.iter().enumerate(){callbacks[i]=entry.callback;}
  let event_reference=unsafe{io_reference_device_object(device)};
  (callbacks,event_reference)
 };
 if !event_reference.is_null(){
  for callback in callbacks.into_iter().flatten(){unsafe{callback(device,1);}}
  unsafe{io_dereference_device_object(event_reference);}
 }
}

/// Remove a filesystem from new mount discovery. In-flight IRPs and mounted
/// volume device references independently keep the driver image alive.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn io_unregister_file_system(device:*mut DeviceObject) {
 if device.is_null(){return}
 let (callbacks,event_reference)={
  let mut registry=FILESYSTEMS.lock();
  let Some(index)=registry.devices[..registry.count].iter().position(|d|*d==device) else{return};
  for i in index..registry.count-1 {registry.devices[i]=registry.devices[i+1];}
  let new_count=registry.count-1;
  registry.count=new_count;
  registry.devices[new_count]=ptr::null_mut();
  let mut callbacks=[None;MAX_FS_NOTIFICATIONS];
  for (i,entry) in registry.notifications.iter().enumerate(){callbacks[i]=entry.callback;}
  let event_reference=unsafe{io_reference_device_object(device)};
  (callbacks,event_reference)
 };
 if !event_reference.is_null(){
  for callback in callbacks.into_iter().flatten(){unsafe{callback(device,0);}}
  unsafe{io_dereference_device_object(event_reference);}
 }
 unsafe{io_dereference_device_object(device);}
}

pub fn is_file_system_registered(device:*mut DeviceObject)->bool {
 if device.is_null(){return false}
 let registry=FILESYSTEMS.lock();
 registry.devices[..registry.count].contains(&device)
}

/// Register a WDK filesystem-registration notification callback. Existing
/// filesystems are reported once with Active=TRUE after the callback is stored;
/// future registrations and removals are delivered outside the registry lock.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn io_register_fs_registration_change(
 driver:*mut DriverObject,callback:Option<DriverFsNotification>
)->i32 {
 let Some(callback)=callback else{return STATUS_INVALID_PARAMETER};
 if driver.is_null()||unsafe{(*driver).flags}&(DRIVER_UNLOADING_FLAG|DRIVER_UNLOADED_FLAG)!=0 {
  return STATUS_INVALID_PARAMETER
 }
 let (devices,count)={
  let mut registry=FILESYSTEMS.lock();
  if registry.notifications.iter().any(|n|n.driver==driver&&n.callback.is_some_and(|c|c as usize==callback as usize)){
   return STATUS_OBJECT_NAME_COLLISION
  }
  let Some(slot)=registry.notifications.iter_mut().find(|n|n.callback.is_none()) else{return STATUS_INSUFFICIENT_RESOURCES};
  *slot=FsNotification{driver,callback:Some(callback)};
  let mut devices=[ptr::null_mut::<DeviceObject>();MAX_REGISTERED_FILESYSTEMS];
  let mut count=0usize;
  for device in registry.devices[..registry.count].iter().copied(){
   let reference=unsafe{io_reference_device_object(device)};
   if !reference.is_null(){devices[count]=reference;count+=1;}
  }
  (devices,count)
 };
 for device in devices[..count].iter().copied(){
  unsafe{callback(device,1);io_dereference_device_object(device);}
 }
 STATUS_SUCCESS
}

/// Remove a previously registered filesystem notification callback.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn io_unregister_fs_registration_change(
 driver:*mut DriverObject,callback:Option<DriverFsNotification>
) {
 let Some(callback)=callback else{return};
 if driver.is_null(){return}
 let mut registry=FILESYSTEMS.lock();
 if let Some(slot)=registry.notifications.iter_mut().find(|n|
  n.driver==driver&&n.callback.is_some_and(|c|c as usize==callback as usize)
 ){*slot=FsNotification::EMPTY;}
}

/// Windows IoVerifyVolume contract: verify an existing mounted volume, or
/// arbitrate an IRP_MN_MOUNT_VOLUME request across registered filesystem
/// control devices when the media has not yet been mounted.
#[unsafe(export_name="IoVerifyVolume")]
pub unsafe extern "win64" fn io_verify_volume(device:*mut DeviceObject,allow_raw_mount:u8)->i32 {
 if device.is_null()||allow_raw_mount>1{return STATUS_INVALID_PARAMETER}
 if unsafe{crate::ntoskrnl::ke::runtime::ke_get_current_irql()}>=2{return STATUS_INVALID_DEVICE_REQUEST}
 let (vpb,volume,remove_pending,mounted)={
  let mut state=WDM.lock();
  let Some(index)=state.devices.iter().position(|s|s.used&&!s.deleting&&ptr::eq(&s.object,device)) else{return STATUS_INVALID_PARAMETER};
  if state.devices[index].object.vpb.is_null(){return STATUS_INVALID_DEVICE_REQUEST}
  let vpb=state.devices[index].object.vpb.cast::<Vpb>();
  let mut vpb_irql=0u8;
  unsafe{io_acquire_vpb_spin_lock(&mut vpb_irql);}
  let snapshot=unsafe{(vpb,(*vpb).device_object,(*vpb).flags&crate::nt_abi::io::VPB_REMOVE_PENDING!=0,(*vpb).flags&crate::nt_abi::io::VPB_MOUNTED!=0)};
  unsafe{io_release_vpb_spin_lock(vpb_irql);}
  if !snapshot.2 {
   if state.devices[index].mount_in_progress{return STATUS_PENDING;}
   state.devices[index].mount_in_progress=true;
  }
  snapshot
 };
 if remove_pending{return STATUS_DELETE_PENDING}
 if mounted {
  if volume.is_null(){release_mount_rundown(device);return STATUS_INVALID_DEVICE_REQUEST}
  let target=unsafe{io_reference_device_object(volume.cast::<DeviceObject>())};
  if target.is_null(){release_mount_rundown(device);return STATUS_DELETE_PENDING}
  let stack_size=unsafe{(*target).stack_size};
  if stack_size<=0 {unsafe{io_dereference_device_object(target);}release_mount_rundown(device);return STATUS_INVALID_DEVICE_REQUEST}
  let irp=unsafe{io_allocate_irp(stack_size,0)};
  if irp.is_null(){unsafe{io_dereference_device_object(target);}release_mount_rundown(device);return STATUS_INSUFFICIENT_RESOURCES}
  let stack=unsafe{io_get_next_irp_stack_location(irp)};
  if stack.is_null(){unsafe{io_free_irp(irp);io_dereference_device_object(target);}release_mount_rundown(device);return STATUS_INSUFFICIENT_RESOURCES}
  unsafe{
   (*stack).major_function=IRP_MJ_FILE_SYSTEM_CONTROL;
   (*stack).minor_function=IRP_MN_VERIFY_VOLUME;
   ptr::write_unaligned((*stack).parameters.as_mut_ptr().add(4).cast::<*mut Vpb>(),vpb);
   io_set_completion_routine(irp,Some(verify_volume_completion),device.cast(),1,1,1);
  }
  let status=unsafe{io_call_driver(target,irp)};
  if status!=STATUS_PENDING&&irp_completion_state(irp)==Some(0){unsafe{io_complete_irp_status(irp,status,0,0);}}
  unsafe{io_dereference_device_object(target);}
  if !wdm_device_is_live(device){return STATUS_DELETE_PENDING}
  if status==STATUS_WRONG_VOLUME {
   let dismount=io_dismount_volume(device);
   if dismount==STATUS_SUCCESS{return unsafe{io_verify_volume(device,allow_raw_mount)}}
   return dismount
  }
  return status
 }
 let candidates=snapshot_registered_filesystems();
 if candidates.count==0 {
  release_mount_rundown(device);
  return STATUS_UNRECOGNIZED_VOLUME
 }
 let pass_count=if allow_raw_mount!=0 {2}else{1};
 for pass in 0..pass_count {
  let raw_mount=pass==1;
  {
   let _state=WDM.lock();
   let mut vpb_irql=0u8;
   unsafe{io_acquire_vpb_spin_lock(&mut vpb_irql);}
   unsafe{
    if raw_mount{(*vpb).flags|=crate::nt_abi::io::VPB_RAW_MOUNT;}
    else{(*vpb).flags&=!crate::nt_abi::io::VPB_RAW_MOUNT;}
   }
   unsafe{io_release_vpb_spin_lock(vpb_irql);}
  }
  if pass!=0 {
   let mut state=WDM.lock();
   let Some(slot)=state.devices.iter_mut().find(|s|s.used&&ptr::eq(&s.object,device)) else{return STATUS_DELETE_PENDING};
   if slot.mount_in_progress{return STATUS_PENDING;}
   slot.mount_in_progress=true;
  }
  let mut owns_reservation=true;
  for fs in candidates.devices[..candidates.count].iter().copied() {
   if !owns_reservation {
    let mut state=WDM.lock();
    let Some(slot)=state.devices.iter_mut().find(|s|s.used&&ptr::eq(&s.object,device)) else{return STATUS_DELETE_PENDING};
    if slot.mount_in_progress{return STATUS_PENDING;}
    slot.mount_in_progress=true;
   }
   owns_reservation=false;
   let status=dispatch_mount_request(device,vpb,fs);
   if status==STATUS_PENDING{return STATUS_PENDING}
   // Completion normally releases this reservation. Also clear it for early
   // allocation/dispatch failures that never reached a completion routine.
   {
    let mut state=WDM.lock();
    if let Some(slot)=state.devices.iter_mut().find(|s|s.used&&ptr::eq(&s.object,device)){slot.mount_in_progress=false;}
   }
   if !wdm_device_is_live(device){return STATUS_DELETE_PENDING}
   let mut vpb_irql=0u8;
   unsafe{io_acquire_vpb_spin_lock(&mut vpb_irql);}
   let is_mounted=unsafe{(*vpb).flags&crate::nt_abi::io::VPB_MOUNTED!=0};
   unsafe{io_release_vpb_spin_lock(vpb_irql);}
   if status>=0&&is_mounted{return STATUS_SUCCESS}
   if status>=0&&!is_mounted {
    let mut vpb_irql=0u8;
    unsafe{io_acquire_vpb_spin_lock(&mut vpb_irql);}
    unsafe{(*vpb).flags&=!crate::nt_abi::io::VPB_RAW_MOUNT;}
    unsafe{io_release_vpb_spin_lock(vpb_irql);}
    release_mount_rundown(device);
    return STATUS_INVALID_DEVICE_REQUEST
   }
   if status!=STATUS_UNRECOGNIZED_VOLUME&&status!=STATUS_INVALID_DEVICE_REQUEST {
    let mut vpb_irql=0u8;
    unsafe{io_acquire_vpb_spin_lock(&mut vpb_irql);}
    unsafe{(*vpb).flags&=!crate::nt_abi::io::VPB_RAW_MOUNT;}
    unsafe{io_release_vpb_spin_lock(vpb_irql);}
    release_mount_rundown(device);
    return status
   }
  }
 }
 let mut vpb_irql=0u8;
 unsafe{io_acquire_vpb_spin_lock(&mut vpb_irql);}
 unsafe{(*vpb).flags&=!crate::nt_abi::io::VPB_RAW_MOUNT;}
 unsafe{io_release_vpb_spin_lock(vpb_irql);}
 release_mount_rundown(device);
 STATUS_UNRECOGNIZED_VOLUME
}
