//! extended processor affinity routines.
// This file is included by the Ke runtime facade so existing shared private
// state and cross-routine helpers retain their original module scope.

// ---- Windows x64 extended affinity bitmap (current topology: group zero) ----
// KAFFINITY_EX starts with USHORT Count, USHORT Size, ULONG Reserved and a
// KAFFINITY bitmap. RdotOS currently exposes one processor group, so these
// routines deliberately operate on the first bitmap word and never touch
// higher group words supplied by a Windows driver.
#[repr(C)]
pub struct AffinityEx {
    pub count:u16,
    pub size:u16,
    pub reserved:u32,
    pub bitmap:[u64;1],
}
unsafe fn affinity_word(ptr:*mut AffinityEx)->Option<&'static core::sync::atomic::AtomicU64>{
    if ptr.is_null() || (ptr as usize)%8!=0{return None}
    Some(unsafe{&*core::ptr::addr_of!((*ptr).bitmap[0]).cast::<core::sync::atomic::AtomicU64>()})
}
unsafe fn affinity_mask(ptr:*const AffinityEx)->u64{
    if ptr.is_null(){return 0}
    unsafe{(*ptr).bitmap[0]}
}
pub unsafe extern "win64" fn ke_initialize_affinity_ex(a:*mut AffinityEx){
    if a.is_null(){return}
    unsafe{(*a).count=1;(*a).size=1;(*a).reserved=0;(*a).bitmap[0]=0;}
}
pub unsafe extern "win64" fn ke_add_processor_affinity_ex(a:*mut AffinityEx,processor:u32){
    if processor>=64{return} if let Some(w)=unsafe{affinity_word(a)}{w.fetch_or(1u64<<processor,Ordering::AcqRel);}
    if !a.is_null(){unsafe{(*a).count=1;}}
}
pub unsafe extern "win64" fn ke_remove_processor_affinity_ex(a:*mut AffinityEx,processor:u32){
    if processor>=64{return} if let Some(w)=unsafe{affinity_word(a)}{w.fetch_and(!(1u64<<processor),Ordering::AcqRel);}
}
pub unsafe extern "win64" fn ke_and_affinity_ex(a:*const AffinityEx,b:*const AffinityEx,result:*mut AffinityEx)->u8{
    if a.is_null()||b.is_null(){return 0}
    let value=unsafe{affinity_mask(a)}&unsafe{affinity_mask(b)};
    if !result.is_null(){let Some(w)=(unsafe{affinity_word(result)}) else{return 0;};w.store(value,Ordering::Release);}
    (value!=0) as u8
}
pub unsafe extern "win64" fn ke_or_affinity_ex(a:*const AffinityEx,b:*const AffinityEx,result:*mut AffinityEx)->u8{
    if a.is_null()||b.is_null(){return 0}
    let value=unsafe{affinity_mask(a)}|unsafe{affinity_mask(b)};
    if !result.is_null(){let Some(w)=(unsafe{affinity_word(result)}) else{return 0;};w.store(value,Ordering::Release);}
    (value!=0) as u8
}
pub unsafe extern "win64" fn ke_subtract_affinity_ex(a:*const AffinityEx,b:*const AffinityEx,result:*mut AffinityEx)->u8{
    if a.is_null()||b.is_null(){return 0}
    let value=unsafe{affinity_mask(a)}&!unsafe{affinity_mask(b)};
    if !result.is_null(){let Some(w)=(unsafe{affinity_word(result)}) else{return 0;};w.store(value,Ordering::Release);}
    (value!=0) as u8
}
pub unsafe extern "win64" fn ke_complement_affinity_ex(result:*mut AffinityEx,input:*const AffinityEx){
    if result.is_null()||input.is_null(){return}
    let valid=if crate::hal::online_cpu_count()>=64{u64::MAX}else{(1u64<<crate::hal::online_cpu_count().max(1))-1};
    let value=valid&!unsafe{affinity_mask(input)};
    if let Some(w)=unsafe{affinity_word(result)}{w.store(value,Ordering::Release);}
}
pub unsafe extern "win64" fn ke_copy_affinity_ex(dst:*mut AffinityEx,src:*const AffinityEx){
    if dst.is_null()||src.is_null()||dst==src as *mut AffinityEx{return}
    let value=unsafe{affinity_mask(src)};
    if let Some(w)=unsafe{affinity_word(dst)}{w.store(value,Ordering::Release);}
    unsafe{(*dst).count=1;(*dst).size=1;(*dst).reserved=0;}
}
pub unsafe extern "win64" fn ke_count_set_bits_affinity_ex(a:*const AffinityEx)->u32{unsafe{affinity_mask(a).count_ones()}}
pub unsafe extern "win64" fn ke_find_first_set_right_affinity_ex(a:*const AffinityEx)->u32{
    let bits=unsafe{affinity_mask(a)};if bits==0{u32::MAX}else{bits.trailing_zeros()}
}
pub unsafe extern "win64" fn ke_find_first_set_left_affinity_ex(a:*const AffinityEx)->u32{
    let bits=unsafe{affinity_mask(a)};if bits==0{u32::MAX}else{63-bits.leading_zeros()}
}
pub unsafe extern "win64" fn ke_check_processor_affinity_ex(a:*const AffinityEx,processor:u32)->u8{
    (processor<64&&(unsafe{affinity_mask(a)}&(1u64<<processor))!=0) as u8
}
pub unsafe extern "win64" fn ke_is_empty_affinity_ex(a:*const AffinityEx)->u8{(unsafe{affinity_mask(a)}==0) as u8}
pub unsafe extern "win64" fn ke_is_equal_affinity_ex(a:*const AffinityEx,b:*const AffinityEx)->u8{
    (unsafe{affinity_mask(a)}==affinity_mask(b)) as u8
}
pub unsafe extern "win64" fn ke_is_single_group_affinity_ex(_a:*const AffinityEx)->u8{1}
pub unsafe extern "win64" fn ke_is_subset_affinity_ex(a:*const AffinityEx,b:*const AffinityEx)->u8{
    let av=unsafe{affinity_mask(a)};let bv=unsafe{affinity_mask(b)};(av&!bv==0) as u8
}
pub unsafe extern "win64" fn ke_interlocked_set_processor_affinity_ex(a:*mut AffinityEx,processor:u32)->u8{
    if processor>=64{return 0}
    let Some(w)=(unsafe{affinity_word(a)}) else{return 0;};
    ((w.fetch_or(1u64<<processor,Ordering::AcqRel)&(1u64<<processor))!=0) as u8
}
pub unsafe extern "win64" fn ke_interlocked_clear_processor_affinity_ex(a:*mut AffinityEx,processor:u32)->u8{
    if processor>=64{return 0}
    let Some(w)=(unsafe{affinity_word(a)}) else{return 0;};
    ((w.fetch_and(!(1u64<<processor),Ordering::AcqRel)&(1u64<<processor))!=0) as u8
}
pub unsafe extern "win64" fn ke_add_group_affinity_ex(a:*mut AffinityEx,group:u16,mask:u64){
    if group!=0||a.is_null(){return} if let Some(w)=unsafe{affinity_word(a)}{w.fetch_or(mask,Ordering::AcqRel);}
}
pub unsafe extern "win64" fn ke_remove_group_affinity_ex(a:*mut AffinityEx,group:u16,mask:u64){
    if group!=0||a.is_null(){return} if let Some(w)=unsafe{affinity_word(a)}{w.fetch_and(!mask,Ordering::AcqRel);}
}
pub unsafe extern "win64" fn ke_and_group_affinity_ex(a:*mut AffinityEx,group:u16,mask:u64){
    if group!=0||a.is_null(){return} if let Some(w)=unsafe{affinity_word(a)}{w.fetch_and(mask,Ordering::AcqRel);}
}
pub unsafe extern "win64" fn ke_query_group_affinity_ex(a:*const AffinityEx,group:u16)->u64{
    if group!=0{0}else{unsafe{affinity_mask(a)}}
}
pub unsafe extern "win64" fn ke_get_xsave_feature_flags()->u64{
    let max=unsafe{core::arch::x86_64::__cpuid(0).eax};
    if max<0xD{return 0}
    let leaf1=unsafe{core::arch::x86_64::__cpuid(1)};
    if leaf1.ecx&(1<<26)==0||leaf1.ecx&(1<<27)==0{return 0}
    let lo:u32;let hi:u32;
    unsafe{core::arch::asm!("xgetbv",in("ecx")0u32,out("eax")lo,out("edx")hi,options(nomem,nostack));}
    ((hi as u64)<<32)|lo as u64
}
pub unsafe extern "win64" fn ke_dynamic_partitioning_supported()->u8{0}

