// executive KQUEUE routines.
// This file is included by the Ke runtime facade so existing shared private
// state and cross-routine helpers retain their original module scope.

// ---- Executive KQUEUE --------------------------------------------------------
#[repr(C)]
pub struct ListEntry{pub flink:*mut ListEntry,pub blink:*mut ListEntry}
#[repr(C)]
pub struct ExecutiveQueue{
    pub header:DispatcherHeader,
    pub entry_list_head:ListEntry,
    pub current_count:u32,
    pub maximum_count:u32,
    pub thread_list_head:ListEntry,
}
unsafe fn list_initialize(head:*mut ListEntry){unsafe{(*head).flink=head;(*head).blink=head;}}
unsafe fn list_insert_tail(head:*mut ListEntry,entry:*mut ListEntry){
    unsafe{let tail=(*head).blink;(*entry).flink=head;(*entry).blink=tail;(*tail).flink=entry;(*head).blink=entry;}
}
unsafe fn list_insert_head(head:*mut ListEntry,entry:*mut ListEntry){
    unsafe{let first=(*head).flink;(*entry).flink=first;(*entry).blink=head;(*first).blink=entry;(*head).flink=entry;}
}
unsafe fn list_remove_head(head:*mut ListEntry)->*mut ListEntry{
    unsafe{let first=(*head).flink;if first==head{return core::ptr::null_mut()}let next=(*first).flink;(*head).flink=next;(*next).blink=head;(*first).flink=first;(*first).blink=first;first}
}
#[derive(Clone,Copy)]
struct QueueFlag{object:usize,rundown:bool}
impl QueueFlag{const EMPTY:Self=Self{object:0,rundown:false};}
struct QueueFlagTable{records:[QueueFlag;128]}
impl QueueFlagTable{const fn new()->Self{Self{records:[QueueFlag::EMPTY;128]}}}
static QUEUE_FLAGS:crate::hal::IrqSpinLock<QueueFlagTable>=crate::hal::IrqSpinLock::new(QueueFlagTable::new());
fn queue_rundown_state(object:usize,set:Option<bool>)->bool{
    let mut table=QUEUE_FLAGS.lock();
    let index=table.records.iter().position(|r|r.object==object).or_else(||table.records.iter().position(|r|r.object==0));
    let Some(i)=index else{return false};
    if table.records[i].object==0{table.records[i].object=object;}
    if let Some(value)=set{table.records[i].rundown=value;}
    table.records[i].rundown
}
pub unsafe extern "win64" fn ke_initialize_queue(queue:*mut ExecutiveQueue,count:u32){
    if queue.is_null(){return}
    let max=if count==0{crate::hal::online_cpu_count().max(1) as u32}else{count};
    let _=queue_rundown_state(queue as usize,Some(false));
    unsafe{
        core::ptr::addr_of_mut!((*queue).header).write(DispatcherHeader{kind:QUEUE_OBJECT,absolute:0,size:(core::mem::size_of::<ExecutiveQueue>()/4) as u8,inserted:0,signal_state:0,wait_list_flink:queue as u64+8,wait_list_blink:queue as u64+8});
        list_initialize(core::ptr::addr_of_mut!((*queue).entry_list_head));
        (*queue).current_count=0;(*queue).maximum_count=max;
        list_initialize(core::ptr::addr_of_mut!((*queue).thread_list_head));
    }
}
unsafe fn queue_insert(queue:*mut ExecutiveQueue,entry:*mut ListEntry,at_head:bool)->i32{
    if queue.is_null()||entry.is_null(){return 0}
    if queue_rundown_state(queue as usize,None){return STATUS_ABANDONED_WAIT_0}
    let _guard=DISPATCHER_OBJECT_LOCK.lock();
    let state=unsafe{&*core::ptr::addr_of!((*queue).header.signal_state).cast::<core::sync::atomic::AtomicI32>()};
    let previous=state.load(Ordering::Acquire);
    loop{
        let Some(waiter)=crate::dispatcher::wake_one_timed(queue as u64) else{break};
        if waiter.timed_out{
            let status=crate::dispatcher::complete_wait_group(waiter.thread,queue as u64,STATUS_TIMEOUT,STATUS_SUCCESS,STATUS_TIMEOUT).unwrap_or(STATUS_TIMEOUT);
            let _=crate::thread::complete_wait(waiter.thread,status);continue
        }
        let _=crate::thread::set_queue_entry(waiter.thread,entry as u64);
        let status=crate::dispatcher::complete_wait_group(waiter.thread,queue as u64,STATUS_SUCCESS,STATUS_SUCCESS,STATUS_TIMEOUT).unwrap_or(STATUS_SUCCESS);
        let _=crate::thread::complete_wait(waiter.thread,status);
        return previous
    }
    unsafe{if at_head{list_insert_head(core::ptr::addr_of_mut!((*queue).entry_list_head),entry)}else{list_insert_tail(core::ptr::addr_of_mut!((*queue).entry_list_head),entry)}}
    state.fetch_add(1,Ordering::AcqRel);previous
}
pub unsafe extern "win64" fn ke_insert_queue(queue:*mut ExecutiveQueue,entry:*mut ListEntry)->i32{unsafe{queue_insert(queue,entry,false)}}
pub unsafe extern "win64" fn ke_insert_head_queue(queue:*mut ExecutiveQueue,entry:*mut ListEntry)->i32{unsafe{queue_insert(queue,entry,true)}}
pub unsafe extern "win64" fn ke_remove_queue(queue:*mut ExecutiveQueue,wait_mode:i32,timeout:*const i64)->*mut ListEntry{
    if queue.is_null(){return STATUS_INVALID_PARAMETER as isize as *mut ListEntry}
    if queue_rundown_state(queue as usize,None){return STATUS_ABANDONED_WAIT_0 as isize as *mut ListEntry}
    let popped={
        let _guard=DISPATCHER_OBJECT_LOCK.lock();
        let state=unsafe{&*core::ptr::addr_of!((*queue).header.signal_state).cast::<core::sync::atomic::AtomicI32>()};
        let entry=unsafe{list_remove_head(core::ptr::addr_of_mut!((*queue).entry_list_head))};
        if !entry.is_null(){state.fetch_sub(1,Ordering::AcqRel);}entry
    };
    if !popped.is_null(){return popped}
    if !timeout.is_null()&&unsafe{timeout.read_unaligned()}==0{return STATUS_TIMEOUT as isize as *mut ListEntry}
    let status=unsafe{ke_wait_for_single_object(core::ptr::addr_of_mut!((*queue).header),0,wait_mode,0,timeout)};
    if status!=STATUS_SUCCESS{return status as isize as *mut ListEntry}
    if let Some(entry)=crate::thread::take_queue_entry(){return entry as *mut ListEntry}
    let _guard=DISPATCHER_OBJECT_LOCK.lock();
    let entry=unsafe{list_remove_head(core::ptr::addr_of_mut!((*queue).entry_list_head))};
    if entry.is_null(){STATUS_TIMEOUT as isize as *mut ListEntry}else{entry}
}
pub unsafe extern "win64" fn ke_rundown_queue(queue:*mut ExecutiveQueue)->*mut ListEntry{
    if queue.is_null(){return core::ptr::null_mut()}
    let _guard=DISPATCHER_OBJECT_LOCK.lock();
    let _=queue_rundown_state(queue as usize,Some(true));
    let head=unsafe{core::ptr::addr_of_mut!((*queue).entry_list_head)};
    let first=unsafe{(*head).flink};
    unsafe{
        (&*core::ptr::addr_of!((*queue).header.signal_state).cast::<core::sync::atomic::AtomicI32>()).store(0,Ordering::Release);
        if first==head{list_initialize(head)}else{
            let tail=(*head).blink;(*tail).flink=first;(*first).blink=tail;
            (*head).flink=head;(*head).blink=head;
        }
    }
    let mut waiters=[crate::dispatcher::WokenWaiter{thread:0,timed_out:false};crate::dispatcher::MAX_WAITERS];
    let count=crate::dispatcher::wake_all_timed(queue as u64,&mut waiters);
    for waiter in waiters[..count].iter().copied(){
        let status=crate::dispatcher::complete_wait_group(waiter.thread,queue as u64,STATUS_ABANDONED_WAIT_0,STATUS_SUCCESS,STATUS_TIMEOUT).unwrap_or(STATUS_ABANDONED_WAIT_0);
        let _=crate::thread::complete_wait(waiter.thread,status);
    }
    if first==head{core::ptr::null_mut()}else{first}
}

