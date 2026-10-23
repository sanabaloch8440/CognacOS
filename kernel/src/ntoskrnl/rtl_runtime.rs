//! ABI-correct, freestanding subset of Windows NT RTL services.
//! Only routines whose signatures and behavior are implemented here are resolved.
use core::ffi::c_void;

#[repr(C)]
pub struct RtlBitmap { pub size_of_bitmap: u32, pub buffer: *mut u32 }
#[repr(C)]
pub struct AnsiString { pub length: u16, pub maximum_length: u16, pub buffer: *mut u8 }
#[repr(C)]
pub struct UnicodeString { pub length: u16, pub maximum_length: u16, pub buffer: *mut u16 }
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Luid { pub low_part: u32, pub high_part: i32 }

#[inline] unsafe fn bit(bitmap: *const RtlBitmap, index: u32) -> bool {
    if bitmap.is_null() || index >= unsafe { (*bitmap).size_of_bitmap } || unsafe { (*bitmap).buffer.is_null() } { return false; }
    unsafe { ((*(*bitmap).buffer.add((index / 32) as usize)) & (1u32 << (index % 32))) != 0 }
}
#[inline] unsafe fn set_bit(bitmap: *mut RtlBitmap, index: u32, value: bool) {
    if bitmap.is_null() || index >= unsafe { (*bitmap).size_of_bitmap } || unsafe { (*bitmap).buffer.is_null() } { return; }
    let p = unsafe { (*bitmap).buffer.add((index / 32) as usize) };
    let mask = 1u32 << (index % 32);
    unsafe { if value { *p |= mask } else { *p &= !mask } }
}
#[inline] unsafe fn valid_range(bitmap: *const RtlBitmap, start: u32, count: u32) -> bool {
    !bitmap.is_null() && start.checked_add(count).is_some_and(|end| end <= unsafe { (*bitmap).size_of_bitmap }) && (count == 0 || !unsafe { (*bitmap).buffer.is_null() })
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_are_bits_clear(b:*const RtlBitmap,s:u32,n:u32)->u8 {
    if !unsafe { valid_range(b,s,n) } { return 0; }
    for i in s..s+n { if unsafe { bit(b,i) } { return 0; } } 1
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_are_bits_set(b:*const RtlBitmap,s:u32,n:u32)->u8 {
    if !unsafe { valid_range(b,s,n) } { return 0; }
    for i in s..s+n { if !unsafe { bit(b,i) } { return 0; } } 1
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_clear_all_bits(b:*mut RtlBitmap) {
    if b.is_null() || unsafe { (*b).buffer.is_null() } { return; }
    let words = (unsafe { (*b).size_of_bitmap } as usize + 31) / 32;
    for i in 0..words { unsafe { *(*b).buffer.add(i) = 0; } }
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_clear_bit(b:*mut RtlBitmap,i:u32) { unsafe { set_bit(b,i,false) } }
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_clear_bits(b:*mut RtlBitmap,s:u32,n:u32) {
    if !unsafe { valid_range(b,s,n) } { return; } for i in s..s+n { unsafe { set_bit(b,i,false); } }
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_set_all_bits(b:*mut RtlBitmap) {
    if b.is_null() || unsafe { (*b).buffer.is_null() } { return; }
    let bits=unsafe { (*b).size_of_bitmap }; let words=(bits as usize+31)/32;
    for i in 0..words { unsafe { *(*b).buffer.add(i)=u32::MAX; } }
    if bits%32 != 0 && words>0 { unsafe { *(*b).buffer.add(words-1) &= (1u32 << (bits%32))-1; } }
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_set_bit(b:*mut RtlBitmap,i:u32) { unsafe { set_bit(b,i,true) } }
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_set_bits(b:*mut RtlBitmap,s:u32,n:u32) {
    if !unsafe { valid_range(b,s,n) } { return; } for i in s..s+n { unsafe { set_bit(b,i,true); } }
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_test_bit(b:*const RtlBitmap,i:u32)->u8 { unsafe { bit(b,i) as u8 } }
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_number_of_set_bits(b:*const RtlBitmap)->u32 {
    if b.is_null() || unsafe { (*b).buffer.is_null() } { return 0; }
    let bits=unsafe { (*b).size_of_bitmap }; let words=(bits as usize+31)/32; let mut n=0;
    for i in 0..words { let mut v=unsafe { *(*b).buffer.add(i) }; if i+1==words && bits%32!=0 { v &= (1u32<<(bits%32))-1; } n+=v.count_ones(); } n
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_number_of_clear_bits(b:*const RtlBitmap)->u32 {
    if b.is_null() { return 0; } unsafe { (*b).size_of_bitmap }.saturating_sub(unsafe { rtl_number_of_set_bits(b) })
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_find_clear_bits(b:*const RtlBitmap,n:u32,hint:u32)->u32 {
    if b.is_null() || n==0 || n>unsafe { (*b).size_of_bitmap } { return u32::MAX; }
    let size=unsafe { (*b).size_of_bitmap }; let start=hint.min(size);
    for pass in 0..2 { let (lo,hi)=if pass==0 {(start,size)} else {(0,start)}; if hi<lo || hi-lo<n {continue;}
        let mut i=lo; while i+n<=hi { let mut j=0; while j<n && !unsafe { bit(b,i+j) } {j+=1;} if j==n{return i;} i+=j+1; }
    } u32::MAX
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_find_clear_bits_and_set(b:*mut RtlBitmap,n:u32,h:u32)->u32 {
    let i=unsafe { rtl_find_clear_bits(b,n,h) }; if i!=u32::MAX { unsafe { rtl_set_bits(b,i,n); } } i
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_find_set_bits(b:*const RtlBitmap,n:u32,hint:u32)->u32 {
    if b.is_null() || n==0 || n>unsafe { (*b).size_of_bitmap } { return u32::MAX; }
    let size=unsafe { (*b).size_of_bitmap }; let start=hint.min(size);
    for pass in 0..2 { let (lo,hi)=if pass==0 {(start,size)} else {(0,start)}; if hi<lo || hi-lo<n {continue;}
        let mut i=lo; while i+n<=hi { let mut j=0; while j<n && unsafe { bit(b,i+j) } {j+=1;} if j==n{return i;} i+=j+1; }
    } u32::MAX
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_find_set_bits_and_clear(b:*mut RtlBitmap,n:u32,h:u32)->u32 {
    let i=unsafe { rtl_find_set_bits(b,n,h) }; if i!=u32::MAX { unsafe { rtl_clear_bits(b,i,n); } } i
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_find_least_significant_bit(v:u64)->i8 { if v==0 {-1} else {v.trailing_zeros() as i8} }
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_find_most_significant_bit(v:u64)->i8 { if v==0 {-1} else {(63-v.leading_zeros()) as i8} }
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_find_first_run_clear(b:*const RtlBitmap,run_len:*mut u32)->u32 {
    if !run_len.is_null(){unsafe{*run_len=0;}} if b.is_null(){return u32::MAX;} let size=unsafe{(*b).size_of_bitmap}; let mut i=0;
    while i<size { if !unsafe{bit(b,i)} {let s=i; while i<size && !unsafe{bit(b,i)} {i+=1;} if !run_len.is_null(){unsafe{*run_len=i-s;}} return s;} i+=1;} u32::MAX
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_find_longest_run_clear(b:*const RtlBitmap,starting_index:*mut u32)->u32 {
    if !starting_index.is_null(){unsafe{*starting_index=0;}} if b.is_null(){return 0;} let size=unsafe{(*b).size_of_bitmap}; let mut i=0;let mut best=0;let mut bests=0;
    while i<size {if !unsafe{bit(b,i)} {let s=i;while i<size&&!unsafe{bit(b,i)}{i+=1;}if i-s>best{best=i-s;bests=s;}}else{i+=1;}}
    if !starting_index.is_null(){unsafe{*starting_index=bests;}} best
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_find_next_forward_run_clear(b:*const RtlBitmap,from:u32,run_len:*mut u32)->u32 {
    if !run_len.is_null(){unsafe{*run_len=0;}} if b.is_null(){return u32::MAX;} let size=unsafe{(*b).size_of_bitmap};let mut i=from.min(size);
    while i<size&&unsafe{bit(b,i)}{i+=1;} if i==size{return u32::MAX;}let s=i;while i<size&&!unsafe{bit(b,i)}{i+=1;}if !run_len.is_null(){unsafe{*run_len=i-s;}}s
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_find_next_forward_run_set(b:*const RtlBitmap,from:u32,run_len:*mut u32)->u32 {
    if !run_len.is_null(){unsafe{*run_len=0;}} if b.is_null(){return u32::MAX;}let size=unsafe{(*b).size_of_bitmap};let mut i=from.min(size);
    while i<size&&!unsafe{bit(b,i)}{i+=1;}if i==size{return u32::MAX;}let s=i;while i<size&&unsafe{bit(b,i)}{i+=1;}if !run_len.is_null(){unsafe{*run_len=i-s;}}s
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_find_last_backward_run_clear(b:*const RtlBitmap,from:u32,run_len:*mut u32)->u32 {
    if !run_len.is_null(){unsafe{*run_len=0;}}if b.is_null(){return u32::MAX;}let size=unsafe{(*b).size_of_bitmap};let mut i=from.min(size.saturating_sub(1));
    while i>0&&unsafe{bit(b,i)}{i-=1;}if unsafe{bit(b,i)}{return u32::MAX;}let end=i;while i>0&&!unsafe{bit(b,i-1)}{i-=1;}if !run_len.is_null(){unsafe{*run_len=end-i+1;}}i
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_compare_memory(a:*const u8,b:*const u8,n:u64)->u64 {
    if n==0{return 0;}if a.is_null()||b.is_null(){return 0;}let mut i=0;while i<n&&unsafe{*a.add(i as usize)==*b.add(i as usize)}{i+=1;}i
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_compare_memory_ulong(a:*const u32,n:u64,value:u32)->u64 {
    if n==0{return 0;}if a.is_null(){return 0;}let mut i=0;while i<n/4&&unsafe{*a.add(i as usize)==value}{i+=1;}i*4
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_compute_crc32(initial:u32,buffer:*const u8,length:i32)->u32 {
    if buffer.is_null()||length<=0{return initial;}let mut crc=!initial;for i in 0..length as usize{crc^=unsafe{*buffer.add(i)} as u32;for _ in 0..8{crc=if crc&1!=0{(crc>>1)^0xedb8_8320}else{crc>>1};}}!crc
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_crc32(initial:u32,buffer:*const u8,length:i32)->u32 { unsafe{rtl_compute_crc32(initial,buffer,length)} }
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_downcase_unicode_char(c:u16)->u16 { if (b'A' as u16..=b'Z' as u16).contains(&c){c+32}else{c} }
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_upcase_unicode_char(c:u16)->u16 { if (b'a' as u16..=b'z' as u16).contains(&c){c-32}else{c} }
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_equal_luid(a:*const Luid,b:*const Luid)->u8 { if a.is_null()||b.is_null(){0}else{(unsafe{(*a).low_part==(*b).low_part&&(*a).high_part==(*b).high_part}) as u8} }
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_are_all_accesses_granted(granted:u32,desired:u32)->u8 { ((granted&desired)==desired) as u8 }
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_are_any_accesses_granted(granted:u32,desired:u32)->u8 { ((granted&desired)!=0) as u8 }
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_equal_string(a:*const AnsiString,b:*const AnsiString,case_insensitive:u8)->u8 {
    if a.is_null()||b.is_null(){return 0;}let x=unsafe{&*a};let y=unsafe{&*b};if x.length!=y.length{return 0;}if x.length==0{return 1;}if x.buffer.is_null()||y.buffer.is_null(){return 0;}
    for i in 0..x.length as usize{let mut l=unsafe{*x.buffer.add(i)};let mut r=unsafe{*y.buffer.add(i)};if case_insensitive!=0{if l.is_ascii_uppercase(){l+=32;}if r.is_ascii_uppercase(){r+=32;}}if l!=r{return 0;}}1
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_compare_string(a:*const AnsiString,b:*const AnsiString,case_insensitive:u8)->i32 {
    if a.is_null()||b.is_null(){return 0;}let x=unsafe{&*a};let y=unsafe{&*b};if (x.length!=0&&x.buffer.is_null())||(y.length!=0&&y.buffer.is_null()){return 0;}
    let n=(x.length.min(y.length)) as usize;for i in 0..n{let mut l=unsafe{*x.buffer.add(i)};let mut r=unsafe{*y.buffer.add(i)};if case_insensitive!=0{l=l.to_ascii_lowercase();r=r.to_ascii_lowercase();}if l!=r{return l as i32-r as i32;}}x.length as i32-y.length as i32
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_equal_unicode_string(a:*const UnicodeString,b:*const UnicodeString,case_insensitive:u8)->u8 {
    if a.is_null()||b.is_null(){return 0;}let x=unsafe{&*a};let y=unsafe{&*b};if x.length!=y.length||x.length%2!=0{return 0;}if x.length==0{return 1;}if x.buffer.is_null()||y.buffer.is_null(){return 0;}
    for i in 0..(x.length/2) as usize{let mut l=unsafe{*x.buffer.add(i)};let mut r=unsafe{*y.buffer.add(i)};if case_insensitive!=0{l=unsafe{rtl_downcase_unicode_char(l)};r=unsafe{rtl_downcase_unicode_char(r)};}if l!=r{return 0;}}1
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_compare_unicode_string(a:*const UnicodeString,b:*const UnicodeString,case_insensitive:u8)->i32 {
    if a.is_null()||b.is_null(){return 0;}let x=unsafe{&*a};let y=unsafe{&*b};if x.length%2!=0||y.length%2!=0{return 0;}if (x.length>0&&x.buffer.is_null())||(y.length>0&&y.buffer.is_null()){return 0;}
    let n=(x.length.min(y.length)/2) as usize;for i in 0..n{let mut l=unsafe{*x.buffer.add(i)};let mut r=unsafe{*y.buffer.add(i)};if case_insensitive!=0{l=unsafe{rtl_downcase_unicode_char(l)};r=unsafe{rtl_downcase_unicode_char(r)};}if l!=r{return l as i32-r as i32;}}(x.length as i32)-(y.length as i32)
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_copy_luid(dst:*mut Luid,src:*const Luid) { if !dst.is_null()&&!src.is_null(){unsafe{*dst=*src;}} }
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_copy_string(dst:*mut AnsiString,src:*const AnsiString) {
    if dst.is_null(){return;}let d=unsafe{&mut *dst};if d.buffer.is_null()||d.maximum_length==0{d.length=0;return;}let n=if src.is_null(){0}else{unsafe{(*src).length.min(d.maximum_length)}};if n>0&&!unsafe{(*src).buffer.is_null()}{unsafe{core::ptr::copy((*src).buffer,d.buffer,n as usize);}}d.length=n;
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_copy_unicode_string(dst:*mut UnicodeString,src:*const UnicodeString) {
    if dst.is_null(){return;}let d=unsafe{&mut *dst};if d.buffer.is_null()||d.maximum_length<2{d.length=0;return;}let mut n=if src.is_null(){0}else{unsafe{(*src).length.min(d.maximum_length-2)}};n&=!1;if n>0&&!unsafe{(*src).buffer.is_null()}{unsafe{core::ptr::copy((*src).buffer,d.buffer,(n/2) as usize);}}unsafe{*d.buffer.add((n/2) as usize)=0;}d.length=n;
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_copy_memory(dst:*mut c_void,src:*const c_void,n:u64) { if n>0&&!dst.is_null()&&!src.is_null(){unsafe{core::ptr::copy(src as *const u8,dst as *mut u8,n as usize);}} }
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_fill_memory(dst:*mut c_void,n:u64,value:u8) { if n>0&&!dst.is_null(){unsafe{core::ptr::write_bytes(dst as *mut u8,value,n as usize);}} }

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn bitmap_ranges_and_search_work() { let mut words=[0u32;2];let mut b=RtlBitmap{size_of_bitmap:64,buffer:words.as_mut_ptr()};unsafe{rtl_set_bits(&mut b,5,3);assert_eq!(rtl_number_of_set_bits(&b),3);assert_eq!(rtl_find_clear_bits(&b,3,0),0);assert_eq!(rtl_find_set_bits(&b,3,0),5);assert_eq!(rtl_are_bits_set(&b,5,3),1);rtl_clear_bits(&mut b,5,3);assert_eq!(rtl_number_of_set_bits(&b),0);} }
    #[test] fn crc_and_memory_match_expected_basics() { let data=b"123456789";assert_eq!(unsafe{rtl_compute_crc32(0,data.as_ptr(),9)},0xcbf43926);assert_eq!(unsafe{rtl_compare_memory(data.as_ptr(),data.as_ptr(),9)},9); }
}


#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_copy_memory_non_temporal(dst:*mut c_void,src:*const c_void,n:u64) { unsafe { rtl_copy_memory(dst,src,n) } }
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_fill_memory_non_temporal(dst:*mut c_void,n:u64,value:u8) { unsafe { rtl_fill_memory(dst,n,value) } }
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_move_memory(dst:*mut c_void,src:*const c_void,n:u64) { if n>0&&!dst.is_null()&&!src.is_null(){unsafe{core::ptr::copy(src as *const u8,dst as *mut u8,n as usize);}} }
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_zero_memory(dst:*mut c_void,n:u64) { if n>0&&!dst.is_null(){unsafe{core::ptr::write_bytes(dst as *mut u8,0,n as usize);}} }
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_init_ansi_string(dst:*mut AnsiString,src:*const u8) {
    if dst.is_null(){return;}let d=unsafe{&mut *dst};d.buffer=src as *mut u8;if src.is_null(){d.length=0;d.maximum_length=0;return;}
    let mut n=0usize;while n<u16::MAX as usize-1&&unsafe{*src.add(n)}!=0{n+=1;}d.length=n as u16;d.maximum_length=(n+1) as u16;
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_init_string(dst:*mut AnsiString,src:*const u8) { unsafe{rtl_init_ansi_string(dst,src)} }
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_init_unicode_string(dst:*mut UnicodeString,src:*const u16) {
    if dst.is_null(){return;}let d=unsafe{&mut *dst};d.buffer=src as *mut u16;if src.is_null(){d.length=0;d.maximum_length=0;return;}
    let mut n=0usize;while n<(u16::MAX as usize/2)-1&&unsafe{*src.add(n)}!=0{n+=1;}d.length=(n*2) as u16;d.maximum_length=((n+1)*2) as u16;
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_prefix_unicode_string(prefix:*const UnicodeString,string:*const UnicodeString,case_insensitive:u8)->u8 {
    if prefix.is_null()||string.is_null(){return 0;}let p=unsafe{&*prefix};let s=unsafe{&*string};if p.length>s.length||p.length%2!=0||s.length%2!=0{return 0;}
    if p.length==0{return 1;}if p.buffer.is_null()||s.buffer.is_null(){return 0;}
    for i in 0..(p.length/2) as usize{let mut a=unsafe{*p.buffer.add(i)};let mut b=unsafe{*s.buffer.add(i)};if case_insensitive!=0{a=unsafe{rtl_downcase_unicode_char(a)};b=unsafe{rtl_downcase_unicode_char(b)};}if a!=b{return 0;}}1
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_append_unicode_string_to_string(dst:*mut UnicodeString,src:*const UnicodeString)->i32 {
    const STATUS_SUCCESS:i32=0;const STATUS_BUFFER_TOO_SMALL:i32=0xC000_0023u32 as i32;
    if dst.is_null(){return STATUS_BUFFER_TOO_SMALL;}let d=unsafe{&mut *dst};if src.is_null(){return STATUS_SUCCESS;}let s=unsafe{&*src};
    if s.length==0{return STATUS_SUCCESS;}if s.buffer.is_null()||d.buffer.is_null()||d.length%2!=0||s.length%2!=0{return STATUS_BUFFER_TOO_SMALL;}
    let Some(end)=d.length.checked_add(s.length) else{return STATUS_BUFFER_TOO_SMALL;};if end>d.maximum_length{return STATUS_BUFFER_TOO_SMALL;}
    unsafe{core::ptr::copy(s.buffer,d.buffer.add((d.length/2) as usize),(s.length/2) as usize);}d.length=end;STATUS_SUCCESS
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_append_unicode_to_string(dst:*mut UnicodeString,src:*const u16)->i32 {
    const STATUS_SUCCESS:i32=0;const STATUS_BUFFER_TOO_SMALL:i32=0xC000_0023u32 as i32;
    if dst.is_null(){return STATUS_BUFFER_TOO_SMALL;}let d=unsafe{&mut *dst};if src.is_null(){return STATUS_SUCCESS;}if d.buffer.is_null()||d.length%2!=0{return STATUS_BUFFER_TOO_SMALL;}
    let mut n=0usize;while n<(u16::MAX as usize/2)&&unsafe{*src.add(n)}!=0{n+=1;}let bytes=n.saturating_mul(2);let Some(end)=d.length.checked_add(bytes as u16) else{return STATUS_BUFFER_TOO_SMALL;};if end>=d.maximum_length{return STATUS_BUFFER_TOO_SMALL;}
    unsafe{core::ptr::copy(src,d.buffer.add((d.length/2) as usize),n);*d.buffer.add((end/2) as usize)=0;}d.length=end;STATUS_SUCCESS
}
