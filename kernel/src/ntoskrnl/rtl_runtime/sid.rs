#[repr(C)] #[derive(Clone,Copy)] pub struct SidIdentifierAuthority { pub value:[u8;6] }
#[repr(C)] pub struct SidHeader { pub revision:u8,pub sub_authority_count:u8,pub identifier_authority:SidIdentifierAuthority }
const SID_REVISION:u8=1;const STATUS_SUCCESS:i32=0;const STATUS_INVALID_PARAMETER:i32=0xC000_000Du32 as i32;const STATUS_BUFFER_TOO_SMALL:i32=0xC000_0023u32 as i32;
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_length_required_sid(count:u8)->u32 {8u32.saturating_add((count as u32).saturating_mul(4))}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_length_sid(sid:*const SidHeader)->u32 {if sid.is_null(){return 0;}let s=unsafe{&*sid};if s.revision!=SID_REVISION||s.sub_authority_count>15{0}else{rtl_length_required_sid(s.sub_authority_count)}}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_valid_sid(sid:*const SidHeader)->u8 {(unsafe{rtl_length_sid(sid)}!=0) as u8}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_equal_sid(a:*const SidHeader,b:*const SidHeader)->u8 {
 if a.is_null()||b.is_null(){return 0;}let na=unsafe{rtl_length_sid(a)} as usize;let nb=unsafe{rtl_length_sid(b)} as usize;if na==0||na!=nb{return 0;}
 for i in 0..na{if unsafe{*((a as *const u8).add(i))}!=unsafe{*((b as *const u8).add(i))}{return 0;}}1
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_copy_sid(destination_length:u32,destination:*mut SidHeader,source:*const SidHeader)->i32 {
 if destination.is_null()||source.is_null(){return STATUS_INVALID_PARAMETER;}let n=unsafe{rtl_length_sid(source)};if n==0{return STATUS_INVALID_PARAMETER;}if destination_length<n{return STATUS_BUFFER_TOO_SMALL;}unsafe{core::ptr::copy(source as *const u8,destination as *mut u8,n as usize);}STATUS_SUCCESS
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_initialize_sid(sid:*mut SidHeader,authority:*const SidIdentifierAuthority,count:u8)->i32 {
 if sid.is_null()||authority.is_null()||count>15{return STATUS_INVALID_PARAMETER;}unsafe{(*sid).revision=SID_REVISION;(*sid).sub_authority_count=count;(*sid).identifier_authority=*authority;}STATUS_SUCCESS
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_sub_authority_sid(sid:*mut SidHeader,index:u32)->*mut u32 {
 if sid.is_null(){return core::ptr::null_mut();}let s=unsafe{&mut *sid};if s.revision!=SID_REVISION||index>=s.sub_authority_count as u32||s.sub_authority_count>15{return core::ptr::null_mut();}unsafe{(sid as *mut u8).add(8+index as usize*4) as *mut u32}
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_sub_authority_count_sid(sid:*mut SidHeader)->*mut u8 {
 if sid.is_null(){return core::ptr::null_mut();}let s=unsafe{&mut *sid};if s.revision!=SID_REVISION||s.sub_authority_count>15{return core::ptr::null_mut();}unsafe{(sid as *mut u8).add(1)}
}
