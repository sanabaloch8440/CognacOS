//! Client/Server Runtime Subsystem (CSRSS) capture-buffer helpers.
//! Capture storage uses the native NTDLL heap. Client/server transport is not yet
//! connected to a CognacOS CSR server, so calls requiring IPC fail explicitly.
#![allow(non_snake_case, clippy::missing_safety_doc, dead_code)]
use core::{ffi::c_void, ptr};
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
unsafe fn allocate_capture(count:u32, bytes:u32)->*mut CaptureBuffer {
 if count>65535 || bytes>0x7fff_ffff {return ptr::null_mut();}
 let offsets=(count as usize).checked_mul(core::mem::size_of::<usize>()); let Some(offsets)=offsets else{return ptr::null_mut()};
 let head=core::mem::offset_of!(CaptureBuffer,pointer_offsets); let Some(base)=head.checked_add(offsets) else{return ptr::null_mut()};
 let Some(total)=base.checked_add(bytes as usize).and_then(|v|v.checked_add((count as usize).saturating_mul(3))).filter(|v|*v<=isize::MAX as usize) else{return ptr::null_mut()};
 let heap=unsafe{RtlGetProcessHeap()}; if heap.is_null(){return ptr::null_mut();}
 let p=unsafe{RtlAllocateHeap(heap,HEAP_ZERO_MEMORY,total)} as *mut CaptureBuffer; if p.is_null(){return p;}
 unsafe{(*p).size=total as u32;(*p).pointer_count=0;(*p).buffer_end=(p.cast::<u8>().add(base));} p
}
unsafe fn reserve(cb:*mut CaptureBuffer,len:u32,out:*mut PVOID,source:*const u8)->u32 {
 if cb.is_null()||out.is_null(){return 0;} if len==0 {unsafe{out.write(ptr::null_mut());}return 0;}
 let base=cb.cast::<u8>(); let end=unsafe{base.add((*cb).size as usize)}; let cur=unsafe{(*cb).buffer_end};
 if cur.is_null()||cur<base||cur>end{return 0;} let aligned=match (len as usize).checked_add(3).map(|n|n&!3){Some(n) if n<=u32::MAX as usize=>n,_=>return 0};
 if (cur as usize).checked_add(aligned).filter(|n|*n<=end as usize).is_none(){return 0;}
 let count=unsafe{(*cb).pointer_count}; let max=(unsafe{(*cb).buffer_end as usize-base as usize}); let offsets_start=core::mem::offset_of!(CaptureBuffer,pointer_offsets);
 if count as usize>= (max.saturating_sub(offsets_start)/core::mem::size_of::<usize>()) {return 0;}
 unsafe{out.write(cur.cast()); if !source.is_null(){ptr::copy_nonoverlapping(source,cur,len as usize);} let slot=ptr::addr_of_mut!((*cb).pointer_offsets).cast::<usize>().add(count as usize); slot.write(out as usize); (*cb).pointer_count=count+1; (*cb).buffer_end=cur.add(aligned);}
 aligned as u32
}
#[no_mangle] pub unsafe extern "system" fn CsrAllocateCaptureBuffer(argument_count:u32, buffer_size:u32)->*mut CaptureBuffer {unsafe{allocate_capture(argument_count,buffer_size)}}
#[no_mangle] pub unsafe extern "system" fn CsrAllocateMessagePointer(cb:*mut CaptureBuffer,message_length:u32,captured:*mut PVOID)->u32 {unsafe{reserve(cb,message_length,captured,ptr::null())}}
#[no_mangle] pub unsafe extern "system" fn CsrCaptureMessageBuffer(cb:*mut CaptureBuffer,message:*const c_void,message_length:u32,captured:*mut PVOID)->u32 {if message_length!=0&&message.is_null(){return 0;}unsafe{reserve(cb,message_length,captured,message.cast())}}
#[no_mangle] pub unsafe extern "system" fn CsrFreeCaptureBuffer(cb:*mut CaptureBuffer) {if !cb.is_null(){unsafe{let _=RtlFreeHeap(RtlGetProcessHeap(),0,cb.cast());}}}
#[no_mangle] pub unsafe extern "system" fn CsrCaptureMessageString(cb:*mut CaptureBuffer,string:*const i8,string_length:u32,maximum_length:u32,captured:*mut AnsiString) {if captured.is_null(){return;} if string_length>maximum_length||maximum_length>u16::MAX as u32 {unsafe{(*captured).length=0;(*captured).maximum_length=0;(*captured).buffer=ptr::null_mut();}return;} let mut out=ptr::null_mut(); if unsafe{reserve(cb,maximum_length,&mut out,ptr::null())}==0&&maximum_length!=0{return;} if !out.is_null()&&!string.is_null()&&string_length!=0{unsafe{ptr::copy_nonoverlapping(string.cast::<u8>(),out.cast::<u8>(),string_length as usize);}} unsafe{(*captured).length=string_length as u16;(*captured).maximum_length=maximum_length as u16;(*captured).buffer=out.cast();if !out.is_null()&&string_length<maximum_length{out.cast::<u8>().add(string_length as usize).write(0);}}}
#[no_mangle] pub unsafe extern "system" fn CsrCaptureMessageMultiUnicodeStringsInPlace(capture:*mut *mut CaptureBuffer,count:u32,strings:*mut *mut UnicodeString)->i32 {if capture.is_null()||(count!=0&&strings.is_null()){return STATUS_INVALID_PARAMETER;} let mut total=0u32;for i in 0..count {let u=unsafe{*strings.add(i as usize)};if !u.is_null(){if unsafe{(*u).length>(*u).maximum_length||(*u).maximum_length&1!=0}{return STATUS_INVALID_PARAMETER;}total=match total.checked_add(unsafe{(*u).maximum_length as u32}){Some(v)=>v,None=>return STATUS_INVALID_PARAMETER};}} if unsafe{(*capture).is_null()}{let cb=unsafe{allocate_capture(count,total)};if cb.is_null(){return STATUS_NO_MEMORY;}unsafe{capture.write(cb);}} for i in 0..count {let u=unsafe{*strings.add(i)};if u.is_null(){continue;}let len=unsafe{(*u).maximum_length as u32};let src=unsafe{(*u).buffer};let mut dst=ptr::null_mut();if unsafe{reserve(*capture,len,&mut dst,src.cast())}==0&&len!=0{return STATUS_INVALID_PARAMETER;}unsafe{(*u).buffer=dst.cast();}} STATUS_SUCCESS}
#[no_mangle] pub unsafe extern "system" fn CsrClientCallServer(_message:*mut c_void,_capture:*mut CaptureBuffer,_api_number:u32,_data_length:u32)->i32 {STATUS_PORT_DISCONNECTED}
#[no_mangle] pub unsafe extern "system" fn CsrClientConnectToServer(_server_name:*const u16,_connect_info:u32,_connection_info:*mut c_void,_connection_info_size:*mut u32,_duplicate_handle:u8)->i32 {STATUS_PORT_DISCONNECTED}

// Remaining CSR exports require a live CognacOS CSR server or session state.
#[no_mangle]
pub unsafe extern "system" fn CsrCaptureTimeout() -> i32 { 0xC0000002u32 as i32 }

#[no_mangle]
pub unsafe extern "system" fn CsrGetProcessId() -> i32 { 0xC0000002u32 as i32 }

#[no_mangle]
pub unsafe extern "system" fn CsrIdentifyAlertableThread() -> i32 { 0xC0000002u32 as i32 }

#[no_mangle]
pub unsafe extern "system" fn CsrSetPriorityClass() -> i32 { 0xC0000002u32 as i32 }

#[no_mangle]
pub unsafe extern "system" fn CsrVerifyRegion() -> i32 { 0xC0000002u32 as i32 }
