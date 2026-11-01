// device queue routines.
// This file is included by the Ke runtime facade so existing shared private
// state and cross-routine helpers retain their original module scope.

/// Public x64 KDEVICE_QUEUE / KDEVICE_QUEUE_ENTRY prefix. Queue manipulation is
/// serialized by an IRQ-safe lock; the caller-supplied lock is retained for ABI.
#[repr(C)]
pub struct DeviceQueueEntry{pub entry_type:i16,pub size:i16,pub device_list_entry:ListEntry,pub sort_key:u32,pub inserted:u8,pub reserved:[u8;3]}
#[repr(C)]
pub struct DeviceQueue{pub queue_type:i16,pub size:i16,pub device_list_head:ListEntry,pub lock:u64,pub busy:u8,pub reserved:[u8;3],pub hint:*mut DeviceQueueEntry}
static DEVICE_QUEUE_LOCK:crate::hal::IrqSpinLock<()> = crate::hal::IrqSpinLock::new(());
pub unsafe extern "win64" fn ke_initialize_device_queue(queue:*mut DeviceQueue){
 if queue.is_null(){return}
 unsafe{
  (*queue).queue_type=0x15;(*queue).size=core::mem::size_of::<DeviceQueue>() as i16;
  list_initialize(core::ptr::addr_of_mut!((*queue).device_list_head));
  (*queue).lock=0;(*queue).busy=0;(*queue).reserved=[0;3];(*queue).hint=core::ptr::null_mut();
 }
}
unsafe fn device_queue_insert(queue:*mut DeviceQueue,entry:*mut DeviceQueueEntry,key:u32,sorted:bool)->u8{
 if queue.is_null()||entry.is_null(){return 0}
 let _guard=DEVICE_QUEUE_LOCK.lock();
 // Do not splice an entry twice or into two device queues.
 if unsafe { (*entry).inserted != 0 } { return 0; }
 if unsafe{(*queue).busy==0}{unsafe{(*queue).busy=1;(*entry).inserted=0;}return 0}
 let head=unsafe{core::ptr::addr_of_mut!((*queue).device_list_head)};
 unsafe{
  (*entry).sort_key=key;
  if sorted{
   let mut cursor=(*head).flink;
   while cursor!=head{
    let queued=(cursor as *mut u8).sub(8).cast::<DeviceQueueEntry>();
    if (*queued).sort_key>key{break} cursor=(*cursor).flink;
   }
   let prev=(*cursor).blink;let link=core::ptr::addr_of_mut!((*entry).device_list_entry);
   (*link).flink=cursor;(*link).blink=prev;(*prev).flink=link;(*cursor).blink=link;
  }else{list_insert_tail(head,core::ptr::addr_of_mut!((*entry).device_list_entry));}
  (*entry).inserted=1;
 }
 1
}
pub unsafe extern "win64" fn ke_insert_device_queue(queue:*mut DeviceQueue,entry:*mut DeviceQueueEntry)->u8{unsafe{device_queue_insert(queue,entry,0,false)}}
pub unsafe extern "win64" fn ke_insert_by_key_device_queue(queue:*mut DeviceQueue,entry:*mut DeviceQueueEntry,key:u32)->u8{unsafe{device_queue_insert(queue,entry,key,true)}}
unsafe fn device_queue_remove_entry(queue:*mut DeviceQueue,entry:*mut DeviceQueueEntry)->bool{
 if queue.is_null()||entry.is_null(){return false}
 let _guard=DEVICE_QUEUE_LOCK.lock();
 if unsafe{(*entry).inserted}==0{return false}
 let head=unsafe{core::ptr::addr_of_mut!((*queue).device_list_head)};
 let link=unsafe{core::ptr::addr_of_mut!((*entry).device_list_entry)};
 // Confirm membership in the supplied queue before unlinking the entry.
 let mut cursor=unsafe{(*head).flink};
 let mut found=false;
 while !cursor.is_null() && cursor!=head {
  if cursor==link { found=true; break; }
  cursor=unsafe{(*cursor).flink};
 }
 if !found{return false}
 unsafe{let prev=(*link).blink;let next=(*link).flink;
  if prev.is_null()||next.is_null(){return false}
  (*prev).flink=next;(*next).blink=prev;(*link).flink=link;(*link).blink=link;
  (*entry).inserted=0;}
 true
}
pub unsafe extern "win64" fn ke_remove_entry_device_queue(queue:*mut DeviceQueue,entry:*mut DeviceQueueEntry)->u8{
 (unsafe{device_queue_remove_entry(queue,entry)}) as u8
}
pub unsafe extern "win64" fn ke_remove_by_key_device_queue(queue:*mut DeviceQueue,key:u32)->*mut DeviceQueueEntry{
 if queue.is_null(){return core::ptr::null_mut()}
 let _guard=DEVICE_QUEUE_LOCK.lock();
 let head=unsafe{core::ptr::addr_of_mut!((*queue).device_list_head)};
 let mut link=unsafe{(*head).flink};
 while link!=head{
  let entry=unsafe{(link as *mut u8).sub(8).cast::<DeviceQueueEntry>()};
  if unsafe{(*entry).sort_key}==key{
   unsafe{let prev=(*link).blink;let next=(*link).flink;(*prev).flink=next;(*next).blink=prev;(*link).flink=link;(*link).blink=link;(*entry).inserted=0;(*queue).hint=entry;}
   return entry
  }
  link=unsafe{(*link).flink};
 }
 core::ptr::null_mut()
}
pub unsafe extern "win64" fn ke_remove_by_key_device_queue_if_busy(queue:*mut DeviceQueue,key:u32)->*mut DeviceQueueEntry{
 if queue.is_null()||unsafe{(*queue).busy}==0{return core::ptr::null_mut()}
 unsafe{ke_remove_by_key_device_queue(queue,key)}
}
pub unsafe extern "win64" fn ke_remove_device_queue(queue:*mut DeviceQueue)->*mut DeviceQueueEntry{
 if queue.is_null(){return core::ptr::null_mut()}
 let _guard=DEVICE_QUEUE_LOCK.lock();let head=unsafe{core::ptr::addr_of_mut!((*queue).device_list_head)};
 let link=unsafe{list_remove_head(head)};
 if link.is_null(){unsafe{(*queue).busy=0;(*queue).hint=core::ptr::null_mut();}return core::ptr::null_mut()}
 let entry=unsafe{(link as *mut u8).sub(8).cast::<DeviceQueueEntry>()};
 unsafe{(*entry).inserted=0;(*queue).hint=entry;}entry
}



#[cfg(test)]
mod layout_tests {
    use super::*;

    #[test]
    fn device_queue_x64_layout_is_stable() {
        assert_eq!(core::mem::offset_of!(DeviceQueueEntry, device_list_entry), 8);
        assert_eq!(core::mem::offset_of!(DeviceQueueEntry, sort_key), 24);
        assert_eq!(core::mem::size_of::<DeviceQueueEntry>(), 32);
        assert_eq!(core::mem::offset_of!(DeviceQueue, device_list_head), 8);
        assert_eq!(core::mem::offset_of!(DeviceQueue, lock), 24);
        assert_eq!(core::mem::offset_of!(DeviceQueue, hint), 40);
        assert_eq!(core::mem::size_of::<DeviceQueue>(), 48);
    }
}
