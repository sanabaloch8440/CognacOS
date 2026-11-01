pub unsafe extern "win64" fn io_build_asynchronous_fsd_request(
 major:u32, device:*mut DeviceObject, buffer:*mut c_void, length:u32,
 starting_offset:*const i64, io_status:*mut IoStatusBlock
)->*mut Irp {
 if device.is_null()||io_status.is_null()||(major!=3&&major!=4)||(length!=0&&buffer.is_null()){return ptr::null_mut()}
 let (stack_size,flags)={let state=WDM.lock();let Some(slot)=state.devices.iter().find(|s|s.used&&!s.deleting&&ptr::eq(&s.object,device)) else{return ptr::null_mut()};(slot.object.stack_size,slot.object.flags)};
 // Buffered I/O requires a dedicated system-buffer pool, so fail closed.
 if flags&0x4!=0{return ptr::null_mut()}
 let offset=if starting_offset.is_null(){0}else{unsafe{starting_offset.read_unaligned()}};
 let irp=unsafe{io_allocate_irp(stack_size,0)};
 if irp.is_null(){return ptr::null_mut()}
 let next=unsafe{io_get_next_irp_stack_location(irp)};
 if next.is_null(){unsafe{io_free_irp(irp)};return ptr::null_mut()}
 let mut owned_mdl=ptr::null_mut();
 if flags&0x2!=0 {
  owned_mdl=unsafe{io_allocate_mdl(buffer,length,0,0,irp)};
  if owned_mdl.is_null(){unsafe{io_free_irp(irp)};return ptr::null_mut()}
  // A READ writes into the caller's buffer; a WRITE reads from it.
  unsafe{crate::ntoskrnl::mm::runtime::mm_probe_and_lock_pages(owned_mdl,0,if major==3{1}else{0});}
  let mdl_flags=unsafe{ptr::read_unaligned(owned_mdl.cast::<u8>().add(0x0a).cast::<i16>())};
  if mdl_flags&0x0002==0 {unsafe{io_free_mdl(owned_mdl);io_free_irp(irp);}return ptr::null_mut()}
  let mut state=WDM.lock();
  if let Some(slot)=state.irps.iter_mut().find(|s|s.used&&ptr::eq(&s.object,irp)){slot.owned_mdl=owned_mdl;}
 }
 unsafe{
  (*next).major_function=major as u8;
  // READ/WRITE layout uses ULONG Length at union offset 0 and
  // LARGE_INTEGER ByteOffset at union offset 0x10 on Windows x64.
  ptr::write_unaligned(next.cast::<u8>().add(4).cast::<u32>(),length);
  ptr::write_unaligned(next.cast::<u8>().add(20).cast::<i64>(),offset);
  write_ptr(ptr::addr_of_mut!((*irp).bytes).cast::<u8>(),0x70,buffer);
  write_ptr(ptr::addr_of_mut!((*irp).bytes).cast::<u8>(),0x48,io_status.cast());
  ptr::write_unaligned(ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(IRP_STATUS_OFFSET).cast::<i32>(),STATUS_PENDING);
  ptr::write_unaligned(ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(IRP_INFORMATION_OFFSET).cast::<usize>(),0);
 }
 irp
}


/// Allocate an MDL descriptor and optionally link it into an IRP's MDL chain.
/// Page locking is deliberately separate, matching MmProbeAndLockPages.
#[unsafe(export_name = "IoAllocateMdl")]
pub unsafe extern "win64" fn io_allocate_mdl(
 base:*mut c_void,length:u32,secondary_buffer:u8,charge_quota:u8,irp:*mut Irp
)->*mut c_void {
 if base.is_null()||length==0||secondary_buffer>1||charge_quota!=0{return ptr::null_mut()}
 if !irp.is_null() {
  let valid={let state=WDM.lock();state.irps.iter().any(|s|s.used&&ptr::eq(&s.object,irp))};
  if !valid{return ptr::null_mut()}
 }
 let mdl=unsafe{crate::ntoskrnl::mm::runtime::mm_create_mdl(ptr::null_mut(),base,length as u64)};
 if mdl.is_null(){return ptr::null_mut()}
 if !irp.is_null() {
  let mut state=WDM.lock();
  let Some(slot)=state.irps.iter_mut().find(|s|s.used&&ptr::eq(&s.object,irp)) else {
   unsafe{crate::ntoskrnl::mm::runtime::mm_free_mdl(mdl);}
   return ptr::null_mut()
  };
  let head=unsafe{read_ptr(slot.object.bytes.as_ptr(),0x08)};
  if secondary_buffer==0 {
   unsafe{write_ptr(mdl.cast(),0,head);}
   unsafe{write_ptr(slot.object.bytes.as_mut_ptr(),0x08,mdl);}
  } else if head.is_null() {
   unsafe{write_ptr(slot.object.bytes.as_mut_ptr(),0x08,mdl);}
  } else {
   let mut tail=head;
   let mut found=false;
   for _ in 0..64 {
    let next=unsafe{read_ptr(tail.cast(),0)};
    if next.is_null(){unsafe{write_ptr(tail.cast(),0,mdl);}found=true;break}
    tail=next;
   }
   if !found {
    drop(state);
    unsafe{crate::ntoskrnl::mm::runtime::mm_free_mdl(mdl);}
    return ptr::null_mut()
   }
  }
 }
 mdl
}

/// Unlink and free an MDL allocated by IoAllocateMdl.
#[unsafe(export_name = "IoFreeMdl")]
pub unsafe extern "win64" fn io_free_mdl(mdl:*mut c_void) {
 if mdl.is_null(){return}
 {
  let mut state=WDM.lock();
  for slot in state.irps.iter_mut().filter(|s|s.used) {
   if slot.owned_mdl==mdl {slot.owned_mdl=ptr::null_mut();}
   let mut head=unsafe{read_ptr(slot.object.bytes.as_ptr(),0x08)};
   if head==mdl {
    head=unsafe{read_ptr(mdl.cast(),0)};
    unsafe{write_ptr(slot.object.bytes.as_mut_ptr(),0x08,head);}
    continue
   }
   let mut cursor=head;
   for _ in 0..64 {
    if cursor.is_null(){break}
    let next=unsafe{read_ptr(cursor.cast(),0)};
    if next==mdl {
     let after=unsafe{read_ptr(mdl.cast(),0)};
     unsafe{write_ptr(cursor.cast(),0,after);}
     break
    }
    cursor=next;
   }
  }
 }
 unsafe{crate::ntoskrnl::mm::runtime::mm_free_mdl(mdl);}
}


