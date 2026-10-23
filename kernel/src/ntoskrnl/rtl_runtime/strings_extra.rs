use super::{AnsiString, UnicodeString};

const STATUS_SUCCESS: i32 = 0;
const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
const STATUS_BUFFER_TOO_SMALL: i32 = 0xC000_0023u32 as i32;
const STATUS_INVALID_PARAMETER_2: i32 = 0xC000_00F0u32 as i32;

#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_upper_char(c:u8)->u8 { c.to_ascii_uppercase() }
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_unicode_string_to_ansi_size(s:*const UnicodeString)->u32 {
 if s.is_null(){return 0;}let x=unsafe{&*s};if x.length%2!=0{return 0;}(x.length as u32/2).saturating_add(1)
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_append_asciiz_to_string(dst:*mut AnsiString,src:*const u8)->i32 {
 if dst.is_null(){return STATUS_INVALID_PARAMETER;}let d=unsafe{&mut *dst};if src.is_null(){return STATUS_SUCCESS;}
 if d.buffer.is_null()||d.length>d.maximum_length{return STATUS_INVALID_PARAMETER;}let mut n=0usize;while n<u16::MAX as usize&&unsafe{*src.add(n)}!=0{n+=1;}
 if n>(d.maximum_length-d.length) as usize{return STATUS_BUFFER_TOO_SMALL;}unsafe{core::ptr::copy(src,d.buffer.add(d.length as usize),n);}d.length+=n as u16;STATUS_SUCCESS
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_append_string_to_string(dst:*mut AnsiString,src:*const AnsiString)->i32 {
 if dst.is_null()||src.is_null(){return STATUS_INVALID_PARAMETER;}let d=unsafe{&mut *dst};let s=unsafe{&*src};
 if d.length>d.maximum_length||s.length>s.maximum_length||(s.length>0&&s.buffer.is_null())||(d.length>0&&d.buffer.is_null()){return STATUS_INVALID_PARAMETER;}
 let Some(end)=d.length.checked_add(s.length) else{return STATUS_BUFFER_TOO_SMALL;};if end>d.maximum_length{return STATUS_BUFFER_TOO_SMALL;}
 if s.length>0{unsafe{core::ptr::copy(s.buffer,d.buffer.add(d.length as usize),s.length as usize);}}d.length=end;STATUS_SUCCESS
}
unsafe fn transform_unicode(dst:*mut UnicodeString,src:*const UnicodeString,upper:bool)->i32 {
 if dst.is_null()||src.is_null(){return STATUS_INVALID_PARAMETER;}let d=unsafe{&mut *dst};let s=unsafe{&*src};
 if s.length%2!=0||d.maximum_length<s.length||(s.length>0&&(s.buffer.is_null()||d.buffer.is_null())){return STATUS_BUFFER_TOO_SMALL;}
 if s.length>0{unsafe{core::ptr::copy(s.buffer,d.buffer,(s.length/2) as usize);}}
 for i in 0..(s.length/2) as usize{let c=unsafe{*d.buffer.add(i)};let v=if upper{if (97..=122).contains(&c){c-32}else{c}}else if (65..=90).contains(&c){c+32}else{c};unsafe{*d.buffer.add(i)=v;}}
 d.length=s.length;STATUS_SUCCESS
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_upcase_unicode_string(dst:*mut UnicodeString,src:*const UnicodeString,allocate_destination:u8)->i32 {if allocate_destination!=0{return STATUS_INVALID_PARAMETER_2;}unsafe{transform_unicode(dst,src,true)}}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_downcase_unicode_string(dst:*mut UnicodeString,src:*const UnicodeString,allocate_destination:u8)->i32 {if allocate_destination!=0{return STATUS_INVALID_PARAMETER_2;}unsafe{transform_unicode(dst,src,false)}}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_find_unicode_substring(hay:*const UnicodeString,needle:*const UnicodeString,case_insensitive:u8)->i32 {
 if hay.is_null()||needle.is_null(){return -1;}let h=unsafe{&*hay};let n=unsafe{&*needle};if h.length%2!=0||n.length%2!=0||(h.length>0&&h.buffer.is_null())||(n.length>0&&n.buffer.is_null()){return -1;}
 let hn=(h.length/2) as usize;let nn=(n.length/2) as usize;if nn>hn{return -1;}if nn==0{return 0;}
 for start in 0..=hn-nn{let mut ok=true;for j in 0..nn{let mut a=unsafe{*h.buffer.add(start+j)};let mut b=unsafe{*n.buffer.add(j)};if case_insensitive!=0{if (65..=90).contains(&a){a+=32;}if (65..=90).contains(&b){b+=32;}}if a!=b{ok=false;break;}}if ok{return start as i32;}}-1
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_hash_unicode_string(s:*const UnicodeString,case_insensitive:u8,algorithm:u32,hash:*mut u32)->i32 {
 if s.is_null()||hash.is_null(){return STATUS_INVALID_PARAMETER;}let x=unsafe{&*s};if x.length%2!=0||(x.length>0&&x.buffer.is_null())||algorithm>2{return STATUS_INVALID_PARAMETER;}
 let mut h=if algorithm==1{0x811c9dc5u32}else{0u32};for i in 0..(x.length/2) as usize{let mut c=unsafe{*x.buffer.add(i)};if case_insensitive!=0&&(65..=90).contains(&c){c+=32;}if algorithm==1{h=(h^(c as u32)).wrapping_mul(0x01000193);}else{h=h.rotate_left(5)^(c as u32);h=h.wrapping_mul(0x9e3779b1);}}
 unsafe{*hash=h;}STATUS_SUCCESS
}
