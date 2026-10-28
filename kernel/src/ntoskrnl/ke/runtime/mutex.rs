// mutant and mutex ownership routines.
// This file is included by the Ke runtime facade so existing shared private
// state and cross-routine helpers retain their original module scope.

// ---- Mutant/mutex dispatcher ownership --------------------------------------
const STATUS_ABANDONED_WAIT_0: i32 = 0x0000_0080;
#[derive(Clone,Copy)]
struct MutantRecord{object:usize,owner:u64,recursion:u32,abandoned:bool}
impl MutantRecord{const EMPTY:Self=Self{object:0,owner:0,recursion:0,abandoned:false};}
struct MutantTable{records:[MutantRecord;128]}
impl MutantTable{const fn new()->Self{Self{records:[MutantRecord::EMPTY;128]}}}
static MUTANTS:crate::hal::IrqSpinLock<MutantTable>=crate::hal::IrqSpinLock::new(MutantTable::new());
unsafe fn initialize_mutant(object:*mut DispatcherHeader,initial_owner:u8){
    if object.is_null(){return}
    let current=crate::thread::current_thread_id().unwrap_or(0);
    let owns=initial_owner!=0&&current!=0;
    unsafe{object.write(DispatcherHeader{kind:MUTANT_OBJECT,absolute:0,size:14,inserted:0,signal_state:if owns{0}else{1},wait_list_flink:object as u64+8,wait_list_blink:object as u64+8});}
    let mut table=MUTANTS.lock();
    let slot=table.records.iter().position(|r|r.object==object as usize).or_else(||table.records.iter().position(|r|r.object==0));
    if let Some(i)=slot{table.records[i]=MutantRecord{object:object as usize,owner:if owns{current}else{0},recursion:if owns{1}else{0},abandoned:false};}
}
pub unsafe extern "win64" fn ke_initialize_mutant(object:*mut DispatcherHeader,initial_owner:u8){unsafe{initialize_mutant(object,initial_owner)}}
pub unsafe extern "win64" fn ke_initialize_mutex(object:*mut DispatcherHeader,_level:u32){unsafe{initialize_mutant(object,0)}}
fn try_acquire_mutant_status(object:*mut DispatcherHeader)->Option<i32>{
    if object.is_null(){return None}
    let thread=crate::thread::current_thread_id()?;
    let mut table=MUTANTS.lock();
    let record=table.records.iter_mut().find(|r|r.object==object as usize)?;
    if record.owner==thread{
        record.recursion=record.recursion.saturating_add(1);
        unsafe{(&*core::ptr::addr_of!((*object).signal_state).cast::<core::sync::atomic::AtomicI32>()).store(1-(record.recursion as i32),Ordering::Release);}
        return Some(STATUS_SUCCESS)
    }
    if record.owner!=0{return None}
    record.owner=thread;record.recursion=1;
    let abandoned=record.abandoned;record.abandoned=false;
    unsafe{(&*core::ptr::addr_of!((*object).signal_state).cast::<core::sync::atomic::AtomicI32>()).store(0,Ordering::Release);}
    Some(if abandoned{STATUS_ABANDONED_WAIT_0}else{STATUS_SUCCESS})
}
fn try_acquire_mutant(object:*mut DispatcherHeader)->bool{try_acquire_mutant_status(object).is_some()}
fn release_mutant(object:*mut DispatcherHeader,abandoned:bool)->i32{
    if object.is_null(){return 0}
    let Some(current)=crate::thread::current_thread_id() else{return 0};
    let _object_guard=DISPATCHER_OBJECT_LOCK.lock();
    let mut timed_out=[0u64;crate::dispatcher::MAX_WAITERS];let mut timed_count=0usize;
    let (old,waiter_id,final_status)={
        let mut table=MUTANTS.lock();
        let Some(record)=table.records.iter_mut().find(|r|r.object==object as usize) else{return 0};
        if record.owner!=current||record.recursion==0{return unsafe{(&*core::ptr::addr_of!((*object).signal_state).cast::<core::sync::atomic::AtomicI32>()).load(Ordering::Acquire)}}
        let old=1-record.recursion as i32;
        if record.recursion>1{
            record.recursion-=1;
            unsafe{(&*core::ptr::addr_of!((*object).signal_state).cast::<core::sync::atomic::AtomicI32>()).store(1-record.recursion as i32,Ordering::Release);}
            return old;
        }
        record.owner=0;record.recursion=0;record.abandoned=abandoned;
        unsafe{(&*core::ptr::addr_of!((*object).signal_state).cast::<core::sync::atomic::AtomicI32>()).store(1,Ordering::Release);}
        loop{
            let Some(waiter)=crate::dispatcher::wake_one_timed(object as u64) else{break (old,None,STATUS_SUCCESS)};
            if waiter.timed_out{
                if timed_count<timed_out.len(){timed_out[timed_count]=waiter.thread;timed_count+=1;}
                continue
            }
            record.owner=waiter.thread;record.recursion=1;
            let was_abandoned=record.abandoned;record.abandoned=false;
            unsafe{(&*core::ptr::addr_of!((*object).signal_state).cast::<core::sync::atomic::AtomicI32>()).store(0,Ordering::Release);}
            break (old,Some(waiter.thread),if was_abandoned{STATUS_ABANDONED_WAIT_0}else{STATUS_SUCCESS})
        }
    };
    for id in timed_out[..timed_count].iter().copied(){let status=crate::dispatcher::complete_wait_group(id,object as u64,STATUS_TIMEOUT,STATUS_SUCCESS,STATUS_TIMEOUT).unwrap_or(STATUS_TIMEOUT);let _=crate::thread::complete_wait(id,status);}
    if let Some(id)=waiter_id{
        let status=crate::dispatcher::complete_wait_group(id,object as u64,final_status,STATUS_SUCCESS,STATUS_TIMEOUT).unwrap_or(final_status);
        let _=crate::thread::complete_wait(id,status);
    }
    complete_ready_wait_all_groups();
    old
}
pub unsafe extern "win64" fn ke_release_mutex(object:*mut DispatcherHeader,wait:u8)->i32{
    let old=release_mutant(object,false);let _=wait;old
}
pub unsafe extern "win64" fn ke_release_mutant(object:*mut DispatcherHeader,_increment:i32,abandoned:u8,_wait:u8)->i32{
    release_mutant(object,abandoned!=0)
}
pub unsafe extern "win64" fn ke_read_state_mutex(object:*const DispatcherHeader)->i32{
    if object.is_null(){return 0}
    unsafe{(&*core::ptr::addr_of!((*object).signal_state).cast::<core::sync::atomic::AtomicI32>()).load(Ordering::Acquire)}
}
pub unsafe extern "win64" fn ke_read_state_mutant(object:*const DispatcherHeader)->i32{
    unsafe{ke_read_state_mutex(object)}
}
/// Mark mutexes owned by a terminating thread abandoned and wake their next waiter.
pub fn thread_exit_cleanup(thread:u64){
    if thread==0{return}
    let mut objects=[0usize;128];let mut count=0;
    {
        let mut table=MUTANTS.lock();
        for record in table.records.iter_mut(){
            if record.object!=0&&record.owner==thread{
                record.owner=0;record.recursion=0;record.abandoned=true;
                objects[count]=record.object;count+=1;
            }
        }
    }
    for object in objects[..count].iter().copied(){
        let header=object as *mut DispatcherHeader;
        let _guard=DISPATCHER_OBJECT_LOCK.lock();
        unsafe{(&*core::ptr::addr_of!((*header).signal_state).cast::<core::sync::atomic::AtomicI32>()).store(1,Ordering::Release);}
        let mut timed_out=[0u64;crate::dispatcher::MAX_WAITERS];let mut timed_count=0usize;
        let mut owner_waiter=None;
        loop{
            let Some(waiter)=crate::dispatcher::wake_one_timed(object as u64) else{break};
            if waiter.timed_out{
                if timed_count<timed_out.len(){timed_out[timed_count]=waiter.thread;timed_count+=1;}
                continue
            }
            owner_waiter=Some(waiter.thread);break
        }
        for id in timed_out[..timed_count].iter().copied(){let status=crate::dispatcher::complete_wait_group(id,object as u64,STATUS_TIMEOUT,STATUS_SUCCESS,STATUS_TIMEOUT).unwrap_or(STATUS_TIMEOUT);let _=crate::thread::complete_wait(id,status);}
        if let Some(id)=owner_waiter{
            let mut table=MUTANTS.lock();
            if let Some(record)=table.records.iter_mut().find(|r|r.object==object){record.owner=id;record.recursion=1;record.abandoned=false;}
            unsafe{(&*core::ptr::addr_of!((*header).signal_state).cast::<core::sync::atomic::AtomicI32>()).store(0,Ordering::Release);}
            let status=crate::dispatcher::complete_wait_group(id,object as u64,STATUS_ABANDONED_WAIT_0,STATUS_SUCCESS,STATUS_TIMEOUT).unwrap_or(STATUS_ABANDONED_WAIT_0);
            let _=crate::thread::complete_wait(id,status);
        }
    }
    complete_ready_wait_all_groups();
}




// ---- FAST_MUTEX ABI entry points -------------------------------------------
// x64 FAST_MUTEX layout: Count@0, Owner@8, Contention@16, KEVENT@24,
// OldIrql@48. Contended acquisitions sleep on the embedded synchronization
// event using the same dispatcher wait queue as KeWaitForSingleObject.

#[inline]
unsafe fn fast_mutex_atomic_i32(object: *mut core::ffi::c_void, offset: usize)
    -> Option<&'static core::sync::atomic::AtomicI32>
{
    if object.is_null() || (object as usize).checked_add(offset)? % core::mem::align_of::<i32>() != 0 {
        return None;
    }
    Some(unsafe { &*((object as *mut u8).add(offset).cast::<core::sync::atomic::AtomicI32>()) })
}

#[inline]
unsafe fn fast_mutex_event(object: *mut core::ffi::c_void) -> *mut super::DispatcherHeader {
    unsafe { object.cast::<u8>().add(24).cast::<super::DispatcherHeader>() }
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn ex_initialize_fast_mutex(fast_mutex: *mut core::ffi::c_void) {
    if fast_mutex.is_null() || (fast_mutex as usize) % core::mem::align_of::<u64>() != 0 { return; }
    unsafe {
        fast_mutex.cast::<core::sync::atomic::AtomicI32>().write(core::sync::atomic::AtomicI32::new(1));
        fast_mutex.add(8).cast::<u64>().write(0);
        fast_mutex.add(16).cast::<u32>().write(0);
        // Synchronization event wakes one waiter per release.
        super::ke_initialize_event(fast_mutex_event(fast_mutex), 1, 0);
        fast_mutex.add(48).cast::<u8>().write(0);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn ex_acquire_fast_mutex(fast_mutex: *mut core::ffi::c_void) {
    let Some(count) = (unsafe { fast_mutex_atomic_i32(fast_mutex, 0) }) else { return };
    let old_irql = unsafe { super::kf_raise_irql(1) };
    let previous = count.fetch_sub(1, core::sync::atomic::Ordering::Acquire);
    if previous > 0 {
        unsafe {
            fast_mutex.add(48).cast::<u8>().write(old_irql);
            fast_mutex.add(8).cast::<u64>().write_unaligned(crate::thread::current_thread_id().unwrap_or(0));
        }
        return;
    }

    // Count remains negative while there are queued owners. The synchronization
    // event and dispatcher wait queue close the release-before-sleep race:
    // after enqueue, KeWaitForSingleObject rechecks the event before blocking.
    if let Some(contention) = unsafe {
        if fast_mutex.is_null() { None } else {
            Some(&*((fast_mutex as *mut u8).add(16).cast::<core::sync::atomic::AtomicU32>()))
        }
    } {
        let _ = contention.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    }
    loop {
        let status = unsafe {
            super::ke_wait_for_single_object(
                fast_mutex_event(fast_mutex), 0, 0, 0, core::ptr::null()
            )
        };
        if status == 0 { break; }
        // A supported kernel thread must be able to block. If the dispatcher
        // rejects the wait context, do not return with the mutex acquired
        // falsely; yield only as a last-resort recovery while retaining Count.
        core::hint::spin_loop();
    }
    unsafe {
        fast_mutex.add(48).cast::<u8>().write(old_irql);
        fast_mutex.add(8).cast::<u64>().write_unaligned(crate::thread::current_thread_id().unwrap_or(0));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn ex_release_fast_mutex(fast_mutex: *mut core::ffi::c_void) {
    let Some(count) = (unsafe { fast_mutex_atomic_i32(fast_mutex, 0) }) else { return };
    let owner = unsafe { fast_mutex.add(8).cast::<u64>().read_unaligned() };
    let current = crate::thread::current_thread_id().unwrap_or(0);
    if owner != 0 && owner != current { return; }
    let old_irql = unsafe { fast_mutex.add(48).cast::<u8>().read() };
    unsafe { fast_mutex.add(8).cast::<u64>().write_unaligned(0); }
    let previous = count.fetch_add(1, core::sync::atomic::Ordering::Release);
    if previous < 0 {
        // KeSetEvent follows the normal dispatcher synchronization-event path,
        // waking one queued waiter and consuming the signal as it is released.
        let _ = unsafe { super::ke_set_event(fast_mutex_event(fast_mutex), 0, 0) };
        let contention = unsafe { &*((fast_mutex as *mut u8).add(16).cast::<core::sync::atomic::AtomicU32>()) };
        let value = contention.load(core::sync::atomic::Ordering::Relaxed);
        if value != 0 { let _ = contention.fetch_sub(1, core::sync::atomic::Ordering::Relaxed); }
    }
    unsafe { super::ke_lower_irql(old_irql); }
}
