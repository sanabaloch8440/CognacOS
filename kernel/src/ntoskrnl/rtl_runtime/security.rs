use super::{AnsiString, UnicodeString, Luid};
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_equal_luid(a:*const Luid,b:*const Luid)->u8 { if a.is_null()||b.is_null(){0}else{(unsafe{(*a).low_part==(*b).low_part&&(*a).high_part==(*b).high_part}) as u8} }

#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_are_all_accesses_granted(granted:u32,desired:u32)->u8 { ((granted&desired)==desired) as u8 }

#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_are_any_accesses_granted(granted:u32,desired:u32)->u8 { ((granted&desired)!=0) as u8 }

#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_copy_luid(dst:*mut Luid,src:*const Luid) { if !dst.is_null()&&!src.is_null(){unsafe{*dst=*src;}} }