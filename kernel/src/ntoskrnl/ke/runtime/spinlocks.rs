//! queued and in-stack spin-lock routines.
// This file is included by the Ke runtime facade so existing shared private
// state and cross-routine helpers retain their original module scope.

// ---- Queued spin locks -------------------------------------------------------
const MAX_QUEUED_LOCKS:usize=128;
struct TicketLockSlot{key:core::sync::atomic::AtomicUsize,next:core::sync::atomic::AtomicU32,owner:core::sync::atomic::AtomicU32}
impl TicketLockSlot{const fn new()->Self{Self{key:core::sync::atomic::AtomicUsize::new(0),next:core::sync::atomic::AtomicU32::new(0),owner:core::sync::atomic::AtomicU32::new(0)}}}
static QUEUED_LOCKS:[TicketLockSlot;MAX_QUEUED_LOCKS]=[const{TicketLockSlot::new()};MAX_QUEUED_LOCKS];
static QUEUED_LOCK_REGISTRY:crate::hal::IrqSpinLock<()> = crate::hal::IrqSpinLock::new(());
#[repr(C)]
pub struct LockQueueHandle{pub entry_next:u64,pub entry_lock:u64,pub lock_queue:*mut u64,pub old_irql:u8,pub reserved:[u8;7]}
fn ticket_slot(key:usize)->Option<usize>{
    if key==0{return None}
    let _guard=QUEUED_LOCK_REGISTRY.lock();
    if let Some(i)=QUEUED_LOCKS.iter().position(|s|s.key.load(Ordering::Acquire)==key){return Some(i)}
    let i=QUEUED_LOCKS.iter().position(|s|s.key.load(Ordering::Acquire)==0)?;
    QUEUED_LOCKS[i].key.store(key,Ordering::Release);Some(i)
}
fn acquire_ticket(key:usize)->Option<usize>{
    let index=ticket_slot(key)?;
    let slot=&QUEUED_LOCKS[index];
    let ticket=slot.next.fetch_add(1,Ordering::AcqRel);
    while slot.owner.load(Ordering::Acquire)!=ticket{core::hint::spin_loop();}
    Some(index)
}
fn try_acquire_ticket(key:usize)->Option<usize>{
    let index=ticket_slot(key)?;
    let slot=&QUEUED_LOCKS[index];
    loop{
        let owner=slot.owner.load(Ordering::Acquire);
        let next=slot.next.load(Ordering::Acquire);
        if next!=owner{return None}
        if slot.next.compare_exchange_weak(next,next.wrapping_add(1),Ordering::AcqRel,Ordering::Acquire).is_ok(){return Some(index)}
    }
}
fn release_ticket(index:usize){
    if index<MAX_QUEUED_LOCKS{QUEUED_LOCKS[index].owner.fetch_add(1,Ordering::Release);}
}
unsafe fn acquire_in_stack(lock:*mut u64,handle:*mut LockQueueHandle,raise:bool){
    if lock.is_null()||handle.is_null()||(lock as usize)%8!=0{return}
    let old=if raise{unsafe{ke_raise_irql_to_dpc_level()}}else{unsafe{ke_get_current_irql()}};
    let Some(index)=acquire_ticket(lock as usize) else{if raise{unsafe{ke_lower_irql(old)}}return};
    if let Some(word)=atomic_lock_word(lock){
        while word.compare_exchange_weak(0,1,Ordering::Acquire,Ordering::Relaxed).is_err(){core::hint::spin_loop();}
    }else{release_ticket(index);if raise{unsafe{ke_lower_irql(old)}}return}
    unsafe{handle.write(LockQueueHandle{entry_next:0,entry_lock:lock as u64,lock_queue:lock,old_irql:old,reserved:[0;7]});}
}
unsafe fn release_in_stack(handle:*mut LockQueueHandle,lower:bool){
    if handle.is_null(){return}
    let lock=unsafe{(*handle).lock_queue};
    if lock.is_null(){return}
    let old=unsafe{(*handle).old_irql};
    if let Some(word)=atomic_lock_word(lock){word.store(0,Ordering::Release);}
    if let Some(index)=ticket_slot(lock as usize){release_ticket(index);}
    unsafe{(*handle).lock_queue=core::ptr::null_mut();}
    if lower{unsafe{ke_lower_irql(old)}}
}
pub unsafe extern "win64" fn ke_acquire_in_stack_queued_spin_lock_raise_to_synch(lock:*mut u64,handle:*mut LockQueueHandle){
    if lock.is_null()||handle.is_null()||(lock as usize)%8!=0{return}
    let old=unsafe{ke_raise_irql_to_synch_level()};
    let Some(index)=acquire_ticket(lock as usize) else{unsafe{ke_lower_irql(old)};return};
    if let Some(word)=atomic_lock_word(lock){
        while word.compare_exchange_weak(0,1,Ordering::Acquire,Ordering::Relaxed).is_err(){core::hint::spin_loop();}
    }else{release_ticket(index);unsafe{ke_lower_irql(old)};return}
    unsafe{handle.write(LockQueueHandle{entry_next:0,entry_lock:lock as u64,lock_queue:lock,old_irql:old,reserved:[0;7]});}
}
pub unsafe extern "win64" fn ke_acquire_in_stack_queued_spin_lock(lock:*mut u64,handle:*mut LockQueueHandle){unsafe{acquire_in_stack(lock,handle,true)}}
pub unsafe extern "win64" fn ke_release_in_stack_queued_spin_lock(handle:*mut LockQueueHandle){unsafe{release_in_stack(handle,true)}}
pub unsafe extern "win64" fn ke_acquire_in_stack_queued_spin_lock_at_dpc_level(lock:*mut u64,handle:*mut LockQueueHandle){unsafe{acquire_in_stack(lock,handle,false)}}
pub unsafe extern "win64" fn ke_release_in_stack_queued_spin_lock_from_dpc_level(handle:*mut LockQueueHandle){unsafe{release_in_stack(handle,false)}}
pub unsafe extern "win64" fn ke_acquire_in_stack_queued_spin_lock_for_dpc(lock:*mut u64,handle:*mut LockQueueHandle){unsafe{acquire_in_stack(lock,handle,true)}}
pub unsafe extern "win64" fn ke_release_in_stack_queued_spin_lock_for_dpc(handle:*mut LockQueueHandle){unsafe{release_in_stack(handle,true)}}
/// Threaded-DPC spin lock: raise only when the callback entered below DISPATCH_LEVEL.
pub unsafe extern "win64" fn ke_acquire_spin_lock_for_dpc(lock:*mut u64)->u8{
    unsafe{ke_acquire_spin_lock_raise_to_dpc(lock)}
}
pub unsafe extern "win64" fn ke_release_spin_lock_for_dpc(lock:*mut u64,old_irql:u8){
    unsafe{ke_release_spin_lock(lock,old_irql)}
}
pub unsafe extern "win64" fn ke_try_to_acquire_queued_spin_lock(queue:u32,old_irql:*mut u8)->u8{
    if old_irql.is_null(){return 0}
    let old=unsafe{ke_raise_irql_to_dpc_level()};
    if try_acquire_ticket(queue_number_key(queue)).is_some(){
        unsafe{old_irql.write(old)};1
    }else{unsafe{ke_lower_irql(old)};0}
}
pub unsafe extern "win64" fn ke_try_to_acquire_queued_spin_lock_raise_to_synch(queue:u32,old_irql:*mut u8)->u8{
    if old_irql.is_null(){return 0}
    let old=unsafe{ke_raise_irql_to_synch_level()};
    if try_acquire_ticket(queue_number_key(queue)).is_some(){
        unsafe{old_irql.write(old)};1
    }else{unsafe{ke_lower_irql(old)};0}
}
fn queue_number_key(queue:u32)->usize{0x8000_0000usize.wrapping_add(queue as usize+1)}
pub unsafe extern "win64" fn ke_acquire_queued_spin_lock(queue:u32)->u8{
    let old=unsafe{ke_raise_irql_to_dpc_level()};
    if acquire_ticket(queue_number_key(queue)).is_none(){unsafe{ke_lower_irql(old)};}
    old
}
pub unsafe extern "win64" fn ke_release_queued_spin_lock(queue:u32,old_irql:u8){
    if let Some(index)=ticket_slot(queue_number_key(queue)){release_ticket(index);}
    unsafe{ke_lower_irql(old_irql);}
}
pub unsafe extern "win64" fn ke_acquire_queued_spin_lock_raise_to_synch(queue:u32)->u8{
    let old=unsafe{ke_raise_irql_to_synch_level()};
    if acquire_ticket(queue_number_key(queue)).is_none(){unsafe{ke_lower_irql(old)};}
    old
}
pub unsafe extern "win64" fn ke_release_queued_spin_lock_from_synch_level(queue:u32,old_irql:u8){
    if let Some(index)=ticket_slot(queue_number_key(queue)){release_ticket(index);}
    unsafe{ke_lower_irql(old_irql);}
}

/// LONG KeReadStateQueue(PRKQUEUE Queue).
pub unsafe extern "win64" fn ke_read_state_queue(queue:*const ExecutiveQueue)->i32{
    if queue.is_null(){return 0}
    unsafe{(&*core::ptr::addr_of!((*queue).header.signal_state).cast::<core::sync::atomic::AtomicI32>()).load(Ordering::Acquire)}
}

