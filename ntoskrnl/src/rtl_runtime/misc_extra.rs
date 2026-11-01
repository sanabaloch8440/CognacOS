use super::{AnsiString,UnicodeString};
const STATUS_SUCCESS:i32=0;const STATUS_INVALID_PARAMETER:i32=0xC000_000Du32 as i32;const STATUS_BUFFER_TOO_SMALL:i32=0xC000_0023u32 as i32;
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_crc64(initial:u64,buffer:*const u8,length:u32)->u64 {
 if length>0&&buffer.is_null(){return initial;}let mut crc=initial;for i in 0..length as usize{crc^=(unsafe{*buffer.add(i)} as u64)<<56;for _ in 0..8{crc=if crc&(1u64<<63)!=0{(crc<<1)^0x42f0_e1eb_a9ea_3693}else{crc<<1};}}crc
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_oem_string_to_unicode_size(s:*const AnsiString)->u32 {
 if s.is_null(){return 0;}((unsafe{(*s).length} as u32).saturating_add(1)).saturating_mul(2)
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_unicode_string_to_oem_size(s:*const UnicodeString)->u32 {
 if s.is_null(){return 0;}let x=unsafe{&*s};if x.length%2!=0{return 0;}(x.length as u32/2).saturating_add(1)
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtlx_oem_string_to_unicode_size(s:*const AnsiString)->u32 {unsafe{rtl_oem_string_to_unicode_size(s)}}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtlx_unicode_string_to_oem_size(s:*const UnicodeString)->u32 {unsafe{rtl_unicode_string_to_oem_size(s)}}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_compare_altitudes(a:*const UnicodeString,b:*const UnicodeString)->i32 {
 if a.is_null()||b.is_null(){return 0;}let x=unsafe{&*a};let y=unsafe{&*b};if x.length%2!=0||y.length%2!=0||(x.length>0&&x.buffer.is_null())||(y.length>0&&y.buffer.is_null()){return 0;}
 let n=(x.length.min(y.length)/2) as usize;for i in 0..n{let l=unsafe{*x.buffer.add(i)};let r=unsafe{*y.buffer.add(i)};if l!=r{return if l<r{-1}else{1};}}if x.length==y.length{0}else if x.length<y.length{-1}else{1}
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_multi_byte_to_unicode_n(dst:*mut u16,dst_bytes:u32,written:*mut u32,src:*const u8,src_bytes:u32)->i32 {
 if written.is_null()||(src_bytes>0&&src.is_null())||(dst_bytes>0&&dst.is_null()){return STATUS_INVALID_PARAMETER;}let n=(src_bytes as usize).min((dst_bytes/2) as usize);for i in 0..n{unsafe{*dst.add(i)=*src.add(i) as u16;}}unsafe{*written=(n*2) as u32;}if n<src_bytes as usize{STATUS_BUFFER_TOO_SMALL}else{STATUS_SUCCESS}
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_unicode_to_multi_byte_n(dst:*mut u8,dst_bytes:u32,written:*mut u32,src:*const u16,src_bytes:u32)->i32 {
 if written.is_null()||src_bytes%2!=0||(src_bytes>0&&src.is_null())||(dst_bytes>0&&dst.is_null()){return STATUS_INVALID_PARAMETER;}let count=(src_bytes/2) as usize;let n=count.min(dst_bytes as usize);for i in 0..n{let c=unsafe{*src.add(i)};unsafe{*dst.add(i)=if c<=255{c as u8}else{b'?'};}}unsafe{*written=n as u32;}if n<count{STATUS_BUFFER_TOO_SMALL}else{STATUS_SUCCESS}
}
