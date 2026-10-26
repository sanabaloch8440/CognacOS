//! Client/Server Runtime Subsystem (CSRSS) capture-buffer helpers.
//! Capture storage uses the native NTDLL heap. Client/server transport is not yet
//! connected to a CognacOS CSR server, so calls requiring IPC fail explicitly.
#![allow(non_snake_case, clippy::missing_safety_doc, dead_code)]
use core::{ffi::c_void, ptr, sync::atomic::{AtomicBool, Ordering}};
const STATUS_SUCCESS:i32=0;
const STATUS_INVALID_PARAMETER:i32=0xC000000Du32 as i32;
const STATUS_NO_MEMORY:i32=0xC0000017u32 as i32;
const STATUS_PORT_DISCONNECTED:i32=0xC0000037u32 as i32;
const HEAP_ZERO_MEMORY:u32=8;
type PVOID=*mut c_void;
extern "system" { fn RtlGetProcessHeap()->PVOID; fn RtlAllocateHeap(heap:PVOID,flags:u32,size:usize)->PVOID; fn RtlFreeHeap(heap:PVOID,flags:u32,mem:PVOID)->u8; }
#[repr(C)] struct CaptureBuffer { size:u32, pointer_count:u32, buffer_end:*mut u8, pointer_offsets:[usize;1] }
#[repr(C)] struct UnicodeString { length:u16, maximum_length:u16, buffer:*mut u16 }
#[repr(C)] struct AnsiString { length:u16, maximum_length:u16, buffer:*mut i8 }
static META_LOCK:AtomicBool=AtomicBool::new(false);
static mut CAPTURE_META:[(usize,u32);256]=[(0,0);256];
struct MetaGuard; impl MetaGuard{fn lock()->Self{while META_LOCK.compare_exchange_weak(false,true,Ordering::Acquire,Ordering::Relaxed).is_err(){core::hint::spin_loop();}Self}} impl Drop for MetaGuard{fn drop(&mut self){META_LOCK.store(false,Ordering::Release);}}
fn register_capacity(cb:*mut CaptureBuffer,capacity:u32)->bool{let _g=MetaGuard::lock();let entries=core::ptr::addr_of_mut!(CAPTURE_META).cast::<(usize,u32)>();for i in 0..256{let e=unsafe{&mut *entries.add(i)};if e.0==0{*e=(cb as usize,capacity);return true;}}false}
fn unregister_capacity(cb:*mut CaptureBuffer){let _g=MetaGuard::lock();let entries=core::ptr::addr_of_mut!(CAPTURE_META).cast::<(usize,u32)>();for i in 0..256{let e=unsafe{&mut *entries.add(i)};if e.0==cb as usize{*e=(0,0);return;}}}
fn get_capacity(cb:*mut CaptureBuffer)->Option<u32>{let _g=MetaGuard::lock();let entries=core::ptr::addr_of!(CAPTURE_META).cast::<(usize,u32)>();for i in 0..256{let e=unsafe{&*entries.add(i)};if e.0==cb as usize{return Some(e.1);}}None}
unsafe fn allocate_capture(count:u32, bytes:u32)->*mut CaptureBuffer {
 if count>65535 || bytes>0x7fff_ffff {return ptr::null_mut();}
 let offsets=(count as usize).checked_mul(core::mem::size_of::<usize>()); let Some(offsets)=offsets else{return ptr::null_mut()};
 let head=core::mem::offset_of!(CaptureBuffer,pointer_offsets); let Some(base)=head.checked_add(offsets) else{return ptr::null_mut()};
 let Some(total)=base.checked_add(bytes as usize).and_then(|v|v.checked_add((count as usize).saturating_mul(3))).filter(|v|*v<=isize::MAX as usize && *v<=u32::MAX as usize) else{return ptr::null_mut()};
 let heap=unsafe{RtlGetProcessHeap()}; if heap.is_null(){return ptr::null_mut();}
 let p=unsafe{RtlAllocateHeap(heap,HEAP_ZERO_MEMORY,total)} as *mut CaptureBuffer; if p.is_null(){return p;}
 unsafe{(*p).size=total as u32;(*p).pointer_count=0;(*p).buffer_end=(p.cast::<u8>().add(base));} if !register_capacity(p,count){unsafe{let _=RtlFreeHeap(heap,0,p.cast());}return ptr::null_mut();} p
}
unsafe fn reserve(cb:*mut CaptureBuffer,len:u32,out:*mut PVOID,source:*const u8)->u32 {
 if cb.is_null()||out.is_null(){return 0;} if len==0 {unsafe{out.write(ptr::null_mut());}return 0;}
 let base=cb.cast::<u8>(); let end=unsafe{base.add((*cb).size as usize)}; let cur=unsafe{(*cb).buffer_end};
 if cur.is_null()||cur<base||cur>end{return 0;} let aligned=match (len as usize).checked_add(3).map(|n|n&!3){Some(n) if n<=u32::MAX as usize=>n,_=>return 0};
 if (cur as usize).checked_add(aligned).filter(|n|*n<=end as usize).is_none(){return 0;}
 let count=unsafe{(*cb).pointer_count}; let Some(capacity)=get_capacity(cb) else{return 0;}; if count>=capacity{return 0;}
 unsafe{out.write(cur.cast()); if !source.is_null(){ptr::copy_nonoverlapping(source,cur,len as usize);} let slot=ptr::addr_of_mut!((*cb).pointer_offsets).cast::<usize>().add(count as usize); slot.write(out as usize); (*cb).pointer_count=count+1; (*cb).buffer_end=cur.add(aligned);}
 aligned as u32
}
#[no_mangle] pub unsafe extern "system" fn CsrAllocateCaptureBuffer(argument_count:u32, buffer_size:u32)->*mut CaptureBuffer {unsafe{allocate_capture(argument_count,buffer_size)}}
#[no_mangle] pub unsafe extern "system" fn CsrAllocateMessagePointer(cb:*mut CaptureBuffer,message_length:u32,captured:*mut PVOID)->u32 {unsafe{reserve(cb,message_length,captured,ptr::null())}}
#[no_mangle] pub unsafe extern "system" fn CsrCaptureMessageBuffer(cb:*mut CaptureBuffer,message:*const c_void,message_length:u32,captured:*mut PVOID)->u32 {if message_length!=0&&message.is_null(){return 0;}unsafe{reserve(cb,message_length,captured,message.cast())}}
#[no_mangle] pub unsafe extern "system" fn CsrFreeCaptureBuffer(cb:*mut CaptureBuffer) {if !cb.is_null(){unregister_capacity(cb);unsafe{let _=RtlFreeHeap(RtlGetProcessHeap(),0,cb.cast());}}}
#[no_mangle] pub unsafe extern "system" fn CsrCaptureMessageString(cb:*mut CaptureBuffer,string:*const i8,string_length:u32,maximum_length:u32,captured:*mut AnsiString) {if captured.is_null(){return;} if string_length>maximum_length||maximum_length>u16::MAX as u32 {unsafe{(*captured).length=0;(*captured).maximum_length=0;(*captured).buffer=ptr::null_mut();}return;} let slot=unsafe{ptr::addr_of_mut!((*captured).buffer).cast::<PVOID>()}; if unsafe{reserve(cb,maximum_length,slot,ptr::null())}==0&&maximum_length!=0{return;} let out=unsafe{(*captured).buffer.cast::<u8>()}; if !out.is_null()&&!string.is_null()&&string_length!=0{unsafe{ptr::copy_nonoverlapping(string.cast::<u8>(),out,string_length as usize);}} unsafe{(*captured).length=string_length as u16;(*captured).maximum_length=maximum_length as u16;if !out.is_null()&&string_length<maximum_length{out.add(string_length as usize).write(0);}}}
#[no_mangle] pub unsafe extern "system" fn CsrCaptureMessageMultiUnicodeStringsInPlace(capture:*mut *mut CaptureBuffer,count:u32,strings:*mut *mut UnicodeString)->i32 {if capture.is_null()||(count!=0&&strings.is_null()){return STATUS_INVALID_PARAMETER;} let mut total=0u32;for i in 0..count {let u=unsafe{*strings.add(i as usize)};if !u.is_null(){if unsafe{(*u).length>(*u).maximum_length||(*u).maximum_length&1!=0}{return STATUS_INVALID_PARAMETER;}total=match total.checked_add(unsafe{(*u).maximum_length as u32}){Some(v)=>v,None=>return STATUS_INVALID_PARAMETER};}} if unsafe{(*capture).is_null()}{let cb=unsafe{allocate_capture(count,total)};if cb.is_null(){return STATUS_NO_MEMORY;}unsafe{capture.write(cb);}} for i in 0..count {let u=unsafe{*strings.add(i as usize)};if u.is_null(){continue;}let len=unsafe{(*u).maximum_length as u32};let src=unsafe{(*u).buffer};let slot=unsafe{ptr::addr_of_mut!((*u).buffer).cast::<PVOID>()};if unsafe{reserve(*capture,len,slot,src.cast())}==0&&len!=0{return STATUS_INVALID_PARAMETER;}} STATUS_SUCCESS}
#[no_mangle] pub unsafe extern "system" fn CsrClientCallServer(_message:*mut c_void,_capture:*mut CaptureBuffer,_api_number:u32,_data_length:u32)->i32 {STATUS_PORT_DISCONNECTED}
#[no_mangle] pub unsafe extern "system" fn CsrClientConnectToServer(_server_name:*const u16,_connect_info:u32,_connection_info:*mut c_void,_connection_info_size:*mut u32,_duplicate_handle:u8)->i32 {STATUS_PORT_DISCONNECTED}

// Remaining CSR exports require a live CognacOS CSR server or session state.

#[no_mangle]
pub unsafe extern "system" fn CsrIdentifyAlertableThread() -> i32 {
    // Windows 6.0+ deprecated the CSR request; current implementations treat
    // the notification as a successful no-op rather than requiring a port call.
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn CsrSetPriorityClass(_process: PVOID, _priority_class: *mut u32) -> i32 {
    // The legacy CSRSRV API was removed after Windows XP. Preserve the x64
    // pointer/pointer ABI and report the invalid legacy request on modern NT.
    STATUS_INVALID_PARAMETER
}

#[no_mangle]
pub unsafe extern "system" fn CsrVerifyRegion(_base_address: PVOID, _size: u32) -> i32 {
    // Windows 6.0+ exports this routine, but verification depends on the
    // server-shared region mapping. CognacOS does not yet map that section.
    STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn CsrCaptureTimeout(milliseconds:u32, timeout:*mut i64)->*mut i64 {
 if milliseconds==u32::MAX{return ptr::null_mut();} if timeout.is_null(){return ptr::null_mut();}
 let ticks=(milliseconds as i64).saturating_mul(-10_000); unsafe{timeout.write(ticks);} timeout
}
#[no_mangle]
pub unsafe extern "system" fn CsrGetProcessId()->u32 {
 let pid:usize; unsafe{core::arch::asm!("mov {}, gs:[0x40]",out(reg) pid,options(nostack,preserves_flags,readonly));} pid as u32
}
