pub unsafe extern "win64" fn io_attach_device_to_device_stack(
    source:*mut DeviceObject, target:*mut DeviceObject
)->*mut DeviceObject {
    if source.is_null() || target.is_null() || source==target { return ptr::null_mut(); }
    let mut state=WDM.lock();
    let Some(source_index)=state.devices.iter().position(|s|s.used&&!s.deleting&&ptr::eq(&s.object,source)) else { return ptr::null_mut(); };
    let Some(target_index)=state.devices.iter().position(|s|s.used&&!s.deleting&&ptr::eq(&s.object,target)) else { return ptr::null_mut(); };
    if !state.devices[source_index].attached_to.is_null() { return ptr::null_mut(); }
    let mut top=target;
    for _ in 0..MAX_WDM_DEVICES {
        let next=unsafe{(*top).attached_device};
        if next.is_null() { break; }
        if next==source { return ptr::null_mut(); }
        if !state.devices.iter().any(|s|s.used&&ptr::eq(&s.object,next)) { return ptr::null_mut(); }
        top=next;
    }
    if !unsafe{(*top).attached_device.is_null()} { return ptr::null_mut(); }
    let stack_size=unsafe{(*top).stack_size};
    if stack_size<=0||stack_size>=127 { return ptr::null_mut(); }
    let Some(top_index)=state.devices.iter().position(|s|s.used&&!s.deleting&&ptr::eq(&s.object,top)) else { return ptr::null_mut(); };
    // Pin the actual immediate lower object, which may be an existing filter
    // above the original TargetDevice in a multi-level stack.
    let Some(next_reference_count)=state.devices[top_index].object.reference_count.checked_add(1) else { return ptr::null_mut(); };
    unsafe {
        (*top).attached_device=source;
        (*source).attached_device=ptr::null_mut();
        (*source).stack_size=stack_size+1;
    }
    state.devices[top_index].object.reference_count=next_reference_count;
    state.devices[source_index].attached_to=top;
    top
}

/// WDK-safe attachment form. The output is cleared on entry and receives
/// the lower device returned by IoAttachDeviceToDeviceStack only on success.
#[unsafe(export_name = "IoAttachDeviceToDeviceStackSafe")]
pub unsafe extern "win64" fn io_attach_device_to_device_stack_safe(
 source:*mut DeviceObject,target:*mut DeviceObject,attached_to:*mut *mut DeviceObject
) {
 if attached_to.is_null(){return}
 unsafe{attached_to.write(ptr::null_mut());}
 let lower=unsafe{io_attach_device_to_device_stack(source,target)};
 if !lower.is_null(){unsafe{attached_to.write(lower);}}
}

/// Detach the device attached above the supplied lower device, as returned by
/// IoAttachDeviceToDeviceStack. Existing unrelated stack links are preserved.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn io_detach_device(target:*mut DeviceObject) {
    if target.is_null(){return}
    let released_lower={
        let mut state=WDM.lock();
        let Some(index)=state.devices.iter().position(|s|s.used&&s.attached_to==target) else{return};
        let Some(target_index)=state.devices.iter().position(|s|s.used&&ptr::eq(&s.object,target)) else{return};
        let source=&mut state.devices[index].object as *mut DeviceObject;
        if unsafe{(*target).attached_device}!=source{return}
        if !unsafe{(*source).attached_device.is_null()} {return}
        // A live IRP or remove-lock owner must finish before stack topology changes.
        if state.devices[index].active_dispatches!=0
            ||state.devices[index].outstanding_irps!=0
            ||state.devices[index].remove_lock_count!=0
            ||state.devices[index].object.reference_count>1
            ||state.devices[target_index].active_dispatches!=0
            ||state.devices[target_index].outstanding_irps!=0
            ||state.devices[target_index].remove_lock_count!=0
        {return}
        unsafe{(*target).attached_device=ptr::null_mut();(*source).attached_device=ptr::null_mut();(*source).stack_size=1;}
        state.devices[index].attached_to=ptr::null_mut();
        target
    };
    // Release outside WDM's lock: this may complete a pending lower deletion.
    unsafe{io_dereference_device_object(released_lower);}
}

/// Initialize a caller-allocated WDM IRP and its trailing stack locations.
/// PacketSize must cover the fixed x64 IRP plus StackSize full stack locations.
#[unsafe(export_name = "IoInitializeIrp")]
pub unsafe extern "win64" fn io_initialize_irp(
    irp:*mut Irp, packet_size:u16, stack_size:i8
) {
    if irp.is_null() || stack_size<=0 {return}
    let required=IRP_SIZE_X64.checked_add(stack_size as usize*IO_STACK_LOCATION_SIZE_X64);
    if required.is_none_or(|n|n>packet_size as usize){return}
    unsafe {
        ptr::write_bytes(irp.cast::<u8>(),0,packet_size as usize);
        ptr::write_unaligned(ptr::addr_of_mut!((*irp).bytes).cast::<u8>().cast::<i16>(),6);
        ptr::write_unaligned(ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(2).cast::<u16>(),IRP_SIZE_X64 as u16);
        *ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(0x42)=stack_size as u8;
        *ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(0x43)=(stack_size as u8).wrapping_add(1);
        let stack_base=irp.cast::<u8>().add(IRP_SIZE_X64);
        let current=stack_base.add(stack_size as usize*IO_STACK_LOCATION_SIZE_X64);
        write_ptr(ptr::addr_of_mut!((*irp).bytes).cast::<u8>(),IRP_STACK_OFFSET,current.cast());
    }
}

/// Copy the current stack parameters into the next lower driver's stack slot.
#[unsafe(export_name = "IoCopyCurrentIrpStackLocationToNext")]
pub unsafe extern "win64" fn io_copy_current_irp_stack_location_to_next(irp:*mut Irp) {
 if irp.is_null(){return}
 let count=unsafe{*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x42) as usize};
 if count==0||count>0x7f{return}
 let current=unsafe{io_get_current_irp_stack_location(irp)};
 if current.is_null(){return}
 let base=(irp as usize).saturating_add(IRP_SIZE_X64);
 let end=base.saturating_add(count*IO_STACK_LOCATION_SIZE_X64);
 let cur=current as usize;
 if cur<base||cur>=end||(cur-base)%IO_STACK_LOCATION_SIZE_X64!=0{return}
 let next=cur.checked_sub(IO_STACK_LOCATION_SIZE_X64).unwrap_or(0);
 if next<base{return}
 unsafe{ptr::copy_nonoverlapping(current,next as *mut IoStackLocation,1);}
}

/// Arrange for IoCallDriver to reuse the current stack slot instead of
/// consuming another one: advance the current pointer so the dispatch decrement
/// returns it to the same slot.
#[unsafe(export_name = "IoSkipCurrentIrpStackLocation")]
pub unsafe extern "win64" fn io_skip_current_irp_stack_location(irp:*mut Irp) {
 if irp.is_null(){return}
 let count=unsafe{*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x42) as usize};
 if count==0||count>0x7f{return}
 let current=unsafe{io_get_current_irp_stack_location(irp)};
 if current.is_null(){return}
 let base=(irp as usize).saturating_add(IRP_SIZE_X64);
 let end=base.saturating_add(count*IO_STACK_LOCATION_SIZE_X64);
 let cur=current as usize;
 if cur<base||cur>=end||(cur-base)%IO_STACK_LOCATION_SIZE_X64!=0{return}
 if cur+IO_STACK_LOCATION_SIZE_X64>end{return}
 unsafe {
  write_ptr(ptr::addr_of_mut!((*irp).bytes).cast::<u8>(),IRP_STACK_OFFSET,(cur+IO_STACK_LOCATION_SIZE_X64) as *mut c_void);
  {let location=*ptr::addr_of!((*irp).bytes).cast::<u8>().add(0x43);*ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(0x43)=location.saturating_add(1);}
 }
}


/// Return the highest device object currently attached above the input device.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn io_get_attached_device(device:*mut DeviceObject)->*mut DeviceObject {
    if device.is_null(){return ptr::null_mut()}
    let state=WDM.lock();
    if !state.devices.iter().any(|s|s.used&&ptr::eq(&s.object,device)){return ptr::null_mut()}
    let mut top=device;
    for _ in 0..MAX_WDM_DEVICES {
        let next=unsafe{(*top).attached_device};
        if next.is_null(){return top}
        if !state.devices.iter().any(|s|s.used&&ptr::eq(&s.object,next)){return ptr::null_mut()}
        top=next;
    }
    ptr::null_mut()
}

/// Return the bottom/base object of an attachment stack.
#[unsafe(export_name = "IoGetDeviceAttachmentBaseRef")]
pub unsafe extern "win64" fn io_get_device_attachment_base_ref(device:*mut DeviceObject)->*mut DeviceObject {
    if device.is_null(){return ptr::null_mut()}
    let mut state=WDM.lock();
    let Some(mut index)=state.devices.iter().position(|s|s.used&&ptr::eq(&s.object,device)&&!s.deleting) else{return ptr::null_mut()};
    for _ in 0..MAX_WDM_DEVICES {
        let lower=state.devices[index].attached_to;
        if lower.is_null(){
            let Some(next)=state.devices[index].object.reference_count.checked_add(1) else{return ptr::null_mut()};
            state.devices[index].object.reference_count=next;
            return ptr::addr_of_mut!(state.devices[index].object);
        }
        let Some(lower_index)=state.devices.iter().position(|s|s.used&&ptr::eq(&s.object,lower)&&!s.deleting) else{return ptr::null_mut()};
        index=lower_index;
    }
    ptr::null_mut()
}

/// Return and reference the top object in a device attachment stack.
#[unsafe(export_name = "IoGetAttachedDeviceReference")]
pub unsafe extern "win64" fn io_get_attached_device_reference(device:*mut DeviceObject)->*mut DeviceObject {
    if device.is_null(){return ptr::null_mut()}
    let mut state=WDM.lock();
    let Some(mut index)=state.devices.iter().position(|s|s.used&&ptr::eq(&s.object,device)&&!s.deleting) else{return ptr::null_mut()};
    for _ in 0..MAX_WDM_DEVICES {
        let top=ptr::addr_of_mut!(state.devices[index].object);
        let next=unsafe{(*top).attached_device};
        if next.is_null(){
            let Some(count)=state.devices[index].object.reference_count.checked_add(1) else{return ptr::null_mut()};
            state.devices[index].object.reference_count=count;
            return top;
        }
        let Some(next_index)=state.devices.iter().position(|s|s.used&&ptr::eq(&s.object,next)&&!s.deleting) else{return ptr::null_mut()};
        index=next_index;
    }
    ptr::null_mut()
}

/// Return the next lower object in the stack, or NULL for a base device.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn io_get_lower_device_object(device:*mut DeviceObject)->*mut DeviceObject {
    if device.is_null(){return ptr::null_mut()}
    let state=WDM.lock();
    let Some(slot)=state.devices.iter().find(|s|s.used&&ptr::eq(&s.object,device)) else{return ptr::null_mut()};
    slot.attached_to
}

/// Return the top device object associated with a file object.
///
/// This is the WDM IoGetRelatedDeviceObject contract: file systems and filter
/// drivers must send subsequent requests to the current top of the device stack,
/// not retain a stale lower-device pointer from the original create operation.
#[unsafe(export_name = "IoGetRelatedDeviceObject")]
pub unsafe extern "win64" fn io_get_related_device_object(file_object: *mut FileObject) -> *mut crate::nt_abi::io::DEVICE_OBJECT {
    if file_object.is_null() { return ptr::null_mut(); }
    let device = unsafe { (*file_object).device_object };
    if device.is_null() { return ptr::null_mut(); }
    unsafe { io_get_attached_device(device.cast()) }.cast()
}

/// Reset an allocated IRP for reuse without releasing its allocation.
#[unsafe(export_name = "IoReuseIrp")]
pub unsafe extern "win64" fn io_reuse_irp(irp:*mut Irp, status:i32) {
 if irp.is_null(){return}
 let release_old_refs={
  let mut state=WDM.lock();
  let Some(i)=state.irps.iter().position(|s|s.used&&ptr::eq(&s.object,irp)) else{return};
  let slot=&mut state.irps[i];
  let count=slot.object.bytes[0x42] as usize;
  if count==0||count>slot.stack.len()||slot.completion_state==1||slot.completion_state==3
    ||slot.active_ops!=0||slot.built_sync_ioctl||!slot.owned_mdl.is_null()
    || !(unsafe{read_ptr(slot.object.bytes.as_ptr(),0x08)}).is_null()
    ||(slot.outstanding_count!=0&&!slot.suspended_completion) {return}
  if slot.suspended_completion {slot.completion_state=3;slot.suspended_completion=false;true}else{false}
 };
 if release_old_refs {release_irp_device_rundown(irp);}
 let mut state=WDM.lock();
 let Some(i)=state.irps.iter().position(|s|s.used&&ptr::eq(&s.object,irp)) else{return};
 let slot=&mut state.irps[i];
 let count=slot.object.bytes[0x42] as usize;
 if count==0||count>slot.stack.len()||slot.active_ops!=0||slot.outstanding_count!=0{return}
 slot.completion_state=0;slot.suspended_completion=false;slot.free_requested=false;
 slot.stack.fill(IoStackLocation { major_function:0,minor_function:0,flags:0,control:0,parameters:[0;0x20],device_object:ptr::null_mut(),file_object:ptr::null_mut(),completion_routine:None,context:ptr::null_mut() });
 slot.object.bytes[0x08..0x30].fill(0);
 slot.object.bytes[IRP_STATUS_OFFSET..IRP_STATUS_OFFSET+4].copy_from_slice(&status.to_le_bytes());
 slot.object.bytes[IRP_INFORMATION_OFFSET..IRP_INFORMATION_OFFSET+8].fill(0);
 slot.object.bytes[0x41]=0;
 slot.object.bytes[0x44..0x48].fill(0);
 slot.object.bytes[0x48..0x78].fill(0);
 slot.object.bytes[0x43]=(count+1) as u8;
 let current=unsafe{slot.stack.as_mut_ptr().add(count).cast::<c_void>()};
 unsafe{write_ptr(slot.object.bytes.as_mut_ptr(),IRP_STACK_OFFSET,current);}
}

