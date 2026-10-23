use super::{AnsiString,UnicodeString};
const STATUS_SUCCESS:i32=0;const STATUS_INVALID_PARAMETER:i32=0xC000_000Du32 as i32;const STATUS_BUFFER_TOO_SMALL:i32=0xC000_0023u32 as i32;
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_validate_unicode_string(_flags:u32,s:*const UnicodeString)->i32 {
 if s.is_null(){return STATUS_INVALID_PARAMETER;}let x=unsafe{&*s};if x.length%2!=0||x.maximum_length%2!=0||x.length>x.maximum_length||(x.length>0&&x.buffer.is_null()){return STATUS_INVALID_PARAMETER;}STATUS_SUCCESS
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_ansi_string_to_unicode_string(dst:*mut UnicodeString,src:*const AnsiString,allocate:u8)->i32 {
 if dst.is_null()||src.is_null()||allocate!=0{return STATUS_INVALID_PARAMETER;}let d=unsafe{&mut *dst};let s=unsafe{&*src};if s.length>0&&s.buffer.is_null(){return STATUS_INVALID_PARAMETER;}
 let bytes=(s.length as usize).saturating_mul(2);if bytes>d.maximum_length as usize||bytes>u16::MAX as usize{return STATUS_BUFFER_TOO_SMALL;}if bytes>0&&d.buffer.is_null(){return STATUS_INVALID_PARAMETER;}
 for i in 0..s.length as usize{unsafe{*d.buffer.add(i)=*s.buffer.add(i) as u16;}}d.length=bytes as u16;STATUS_SUCCESS
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_unicode_string_to_ansi_string(dst:*mut AnsiString,src:*const UnicodeString,allocate:u8)->i32 {
 if dst.is_null()||src.is_null()||allocate!=0{return STATUS_INVALID_PARAMETER;}let d=unsafe{&mut *dst};let s=unsafe{&*src};if s.length%2!=0||(s.length>0&&s.buffer.is_null()){return STATUS_INVALID_PARAMETER;}
 let n=(s.length/2) as usize;if n>d.maximum_length as usize{return STATUS_BUFFER_TOO_SMALL;}if n>0&&d.buffer.is_null(){return STATUS_INVALID_PARAMETER;}
 for i in 0..n{let c=unsafe{*s.buffer.add(i)};if c>255{return STATUS_INVALID_PARAMETER;}unsafe{*d.buffer.add(i)=c as u8;}}d.length=(n as u16);STATUS_SUCCESS
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_upcase_unicode_string_to_ansi_string(dst:*mut AnsiString,src:*const UnicodeString,allocate:u8)->i32 {
 if dst.is_null()||src.is_null()||allocate!=0{return STATUS_INVALID_PARAMETER;}let d=unsafe{&mut *dst};let s=unsafe{&*src};if s.length%2!=0||(s.length>0&&s.buffer.is_null()){return STATUS_INVALID_PARAMETER;}let n=(s.length/2) as usize;if n>d.maximum_length as usize{return STATUS_BUFFER_TOO_SMALL;}if n>0&&d.buffer.is_null(){return STATUS_INVALID_PARAMETER;}
 for i in 0..n{let mut c=unsafe{*s.buffer.add(i)};if (97..=122).contains(&c){c-=32;}if c>255{return STATUS_INVALID_PARAMETER;}unsafe{*d.buffer.add(i)=c as u8;}}d.length=n as u16;STATUS_SUCCESS
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_downcase_unicode_string_to_ansi_string(dst:*mut AnsiString,src:*const UnicodeString,allocate:u8)->i32 {
 if dst.is_null()||src.is_null()||allocate!=0{return STATUS_INVALID_PARAMETER;}let d=unsafe{&mut *dst};let s=unsafe{&*src};if s.length%2!=0||(s.length>0&&s.buffer.is_null()){return STATUS_INVALID_PARAMETER;}let n=(s.length/2) as usize;if n>d.maximum_length as usize{return STATUS_BUFFER_TOO_SMALL;}if n>0&&d.buffer.is_null(){return STATUS_INVALID_PARAMETER;}
 for i in 0..n{let mut c=unsafe{*s.buffer.add(i)};if (65..=90).contains(&c){c+=32;}if c>255{return STATUS_INVALID_PARAMETER;}unsafe{*d.buffer.add(i)=c as u8;}}d.length=n as u16;STATUS_SUCCESS
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_ansi_char_to_unicode_char(source:*mut *const u8)->u16 {
 if source.is_null()||unsafe{*source}.is_null(){return 0;}let p=unsafe{*source};let c=unsafe{*p};unsafe{*source=p.add(1);}c as u16
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_unicode_string_to_int64(s:*const UnicodeString,base:u32,value:*mut i64)->i32 {
 if s.is_null()||value.is_null(){return STATUS_INVALID_PARAMETER;}let x=unsafe{&*s};if x.length%2!=0||(x.length>0&&x.buffer.is_null()){return STATUS_INVALID_PARAMETER;}let n=(x.length/2) as usize;let mut i=0;while i<n&&matches!(unsafe{*x.buffer.add(i)},32|9|10|13){i+=1;}
 let mut neg=false;if i<n&&matches!(unsafe{*x.buffer.add(i)},43|45){neg=unsafe{*x.buffer.add(i)}==45;i+=1;}
 let mut radix=base;if radix==0{radix=10;if i+1<n&&unsafe{*x.buffer.add(i)}==48&&matches!(unsafe{*x.buffer.add(i+1)},120|88){radix=16;i+=2;}}
 if !(2..=36).contains(&radix){return STATUS_INVALID_PARAMETER;}let mut v=0u64;let mut digits=0;while i<n{let c=unsafe{*x.buffer.add(i)};let d=match c{48..=57=>(c-48) as u32,65..=90=>(c-65+10) as u32,97..=122=>(c-97+10) as u32,_=>break};if d>=radix{break;}let Some(next)=v.checked_mul(radix as u64).and_then(|q|q.checked_add(d as u64))else{return 0xC000_0095u32 as i32;};v=next;digits+=1;i+=1;}
 if digits==0||v>i64::MAX as u64+(neg as u64){return STATUS_INVALID_PARAMETER;}unsafe{*value=if neg{(v.wrapping_neg()) as i64}else{v as i64};}STATUS_SUCCESS
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_int64_to_unicode_string(value:u64,base:u32,dst:*mut UnicodeString)->i32 {
 if dst.is_null(){return STATUS_INVALID_PARAMETER;}let d=unsafe{&mut *dst};let radix=if base==0{10}else{base};if !(2..=36).contains(&radix){return STATUS_INVALID_PARAMETER;}
 let digits=b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";let mut v=value;let mut tmp=[0u16;65];let mut n=0usize;loop{tmp[n]=digits[(v%radix as u64) as usize] as u16;n+=1;v/=radix as u64;if v==0{break;}}
 let bytes=n*2;if bytes>d.maximum_length as usize||d.buffer.is_null(){return STATUS_BUFFER_TOO_SMALL;}for i in 0..n{unsafe{*d.buffer.add(i)=tmp[n-i-1];}}d.length=bytes as u16;STATUS_SUCCESS
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_large_integer_to_char(value:*const i64,base:u32,length:u32,dst:*mut u8)->i32 {
 if value.is_null()||dst.is_null(){return STATUS_INVALID_PARAMETER;}let radix=if base==0{10}else{base};if !(2..=36).contains(&radix){return STATUS_INVALID_PARAMETER;}let val=unsafe{*value};let neg=val<0;let mut v=val.unsigned_abs();let chars=b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";let mut tmp=[0u8;66];let mut n=0usize;loop{tmp[n]=chars[(v%radix as u64) as usize];n+=1;v/=radix as u64;if v==0{break;}}if neg{tmp[n]=b'-';n+=1;}if n+1>length as usize{return STATUS_BUFFER_TOO_SMALL;}for i in 0..n{unsafe{*dst.add(i)=tmp[n-i-1];}}unsafe{*dst.add(n)=0;}STATUS_SUCCESS
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_upper_string(dst:*mut AnsiString,src:*const AnsiString) {
 if dst.is_null()||src.is_null(){return;}let d=unsafe{&mut *dst};let s=unsafe{&*src};if d.buffer.is_null()||s.buffer.is_null(){d.length=0;return;}let n=s.length.min(d.maximum_length);for i in 0..n as usize{unsafe{*d.buffer.add(i)=(*s.buffer.add(i)).to_ascii_uppercase();}}d.length=n;
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_multi_byte_to_unicode_size(size:*mut u32,src:*const u8,bytes:u32)->i32 {
 if size.is_null()||(bytes>0&&src.is_null()){return STATUS_INVALID_PARAMETER;}unsafe{*size=bytes.saturating_mul(2);}STATUS_SUCCESS
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_unicode_to_multi_byte_size(size:*mut u32,src:*const u16,bytes:u32)->i32 {
 if size.is_null()||bytes%2!=0||(bytes>0&&src.is_null()){return STATUS_INVALID_PARAMETER;}unsafe{*size=bytes/2;}STATUS_SUCCESS
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtlx_ansi_string_to_unicode_size(s:*const AnsiString)->u32 {
 if s.is_null(){return 0;}let x=unsafe{&*s};(x.length as u32).saturating_add(1).saturating_mul(2)
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtlx_unicode_string_to_ansi_size(s:*const UnicodeString)->u32 {
 if s.is_null(){return 0;}let x=unsafe{&*s};if x.length%2!=0{return 0;}(x.length as u32/2).saturating_add(1)
}

#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_integer_to_unicode(value:u32,base:u32,dst:*mut u16)->i32 {
 if dst.is_null(){return STATUS_INVALID_PARAMETER;}let radix=if base==0{10}else{base};if !(2..=36).contains(&radix){return STATUS_INVALID_PARAMETER;}let chars=b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";let mut v=value;let mut tmp=[0u16;32];let mut n=0usize;loop{tmp[n]=chars[(v%radix) as usize] as u16;n+=1;v/=radix;if v==0{break;}}for i in 0..n{unsafe{*dst.add(i)=tmp[n-i-1];}}unsafe{*dst.add(n)=0;}STATUS_SUCCESS
}
