//! C runtime compatibility routines used by CognacOS NTDLL.
#![allow(non_snake_case, clippy::missing_safety_doc)]
use core::{ffi::{c_char,c_void},ptr};

#[no_mangle]
pub unsafe extern "C" fn atoi(s: *const core::ffi::c_char) -> i32 {
    strtol(s, core::ptr::null_mut(), 10) as i32
}

#[no_mangle]
pub unsafe extern "C" fn atol(s: *const core::ffi::c_char) -> isize {
    strtol(s, core::ptr::null_mut(), 10) as isize
}

#[no_mangle]
pub unsafe extern "C" fn strtol(s: *const core::ffi::c_char, end: *mut *mut core::ffi::c_char, base: i32) -> i64 {
    if end.is_null() == false { *end = s as *mut _; }
    if s.is_null() || !(base == 0 || (2..=36).contains(&base)) { return 0; }
    let mut p = s; while (*p as u8).is_ascii_whitespace() { p = p.add(1); }
    let mut neg = false; if *p == b'-' as i8 { neg = true; p = p.add(1); } else if *p == b'+' as i8 { p = p.add(1); }
    let mut radix = base as u32; if radix == 0 { if *p == b'0' as i8 && (*p.add(1) == b'x' as i8 || *p.add(1) == b'X' as i8) { radix = 16; p = p.add(2); } else if *p == b'0' as i8 { radix = 8; } else { radix = 10; } } else if radix == 16 && *p == b'0' as i8 && (*p.add(1) == b'x' as i8 || *p.add(1) == b'X' as i8) { p = p.add(2); }
    let start = p; let limit = if neg { (i64::MAX as u64) + 1 } else { i64::MAX as u64 }; let mut value = 0u64; let mut any = false;
    loop { let ch = *p as u8; let d = if ch >= b'0' && ch <= b'9' { (ch-b'0') as u32 } else if ch >= b'a' && ch <= b'z' { (ch-b'a'+10) as u32 } else if ch >= b'A' && ch <= b'Z' { (ch-b'A'+10) as u32 } else { break; }; if d >= radix { break; } any = true; value = match value.checked_mul(radix as u64).and_then(|v| v.checked_add(d as u64)) { Some(v) if v <= limit => v, _ => limit }; p = p.add(1); }
    if !any { if !end.is_null() { *end = s as *mut _; } return 0; } if !end.is_null() { *end = p as *mut _; } if neg { (value.wrapping_neg()) as i64 } else { value as i64 }
}

#[no_mangle]
pub unsafe extern "C" fn wcslen(s: *const u16) -> usize {
    if s.is_null() { return 0; }
    let mut n = 0usize;
    while *s.add(n) != 0 { n = n.saturating_add(1); }
    n
}

#[no_mangle]
pub unsafe extern "C" fn strcmp(a: *const core::ffi::c_char, b: *const core::ffi::c_char) -> i32 {
    if a.is_null() || b.is_null() { return if a.is_null() { if b.is_null() { 0 } else { -1 } } else { 1 }; }
    let (mut i, mut x, mut y) = (0usize, 0u8, 0u8);
    loop { x = *a.add(i) as u8; y = *b.add(i) as u8; if x != y || x == 0 { return x as i32 - y as i32; } i += 1; }
}

#[no_mangle]
pub unsafe extern "C" fn strncmp(a: *const core::ffi::c_char, b: *const core::ffi::c_char, n: usize) -> i32 {
    if n == 0 { return 0; }
    if a.is_null() || b.is_null() { return if a.is_null() { if b.is_null() { 0 } else { -1 } } else { 1 }; }
    let mut i = 0usize; while i < n { let x = *a.add(i) as u8; let y = *b.add(i) as u8; if x != y || x == 0 { return x as i32 - y as i32; } i += 1; } 0
}

#[no_mangle]
pub unsafe extern "C" fn wcscmp(a: *const u16, b: *const u16) -> i32 {
    if a.is_null() || b.is_null() { return if a.is_null() { if b.is_null() { 0 } else { -1 } } else { 1 }; }
    let mut i = 0usize; loop { let x = *a.add(i); let y = *b.add(i); if x != y || x == 0 { return x as i32 - y as i32; } i += 1; }
}

#[no_mangle]
pub unsafe extern "C" fn wcsncmp(a: *const u16, b: *const u16, n: usize) -> i32 {
    if n == 0 { return 0; }
    if a.is_null() || b.is_null() { return if a.is_null() { if b.is_null() { 0 } else { -1 } } else { 1 }; }
    let mut i = 0usize; while i < n { let x = *a.add(i); let y = *b.add(i); if x != y || x == 0 { return x as i32 - y as i32; } i += 1; } 0
}

#[no_mangle]
pub unsafe extern "C" fn strchr(s: *const core::ffi::c_char, ch: i32) -> *mut core::ffi::c_char {
    if s.is_null() { return core::ptr::null_mut(); } let target = ch as u8; let mut i = 0usize; loop { let v = *s.add(i) as u8; if v == target { return s.add(i) as *mut _; } if v == 0 { return core::ptr::null_mut(); } i += 1; }
}

#[no_mangle]
pub unsafe extern "C" fn wcschr(s: *const u16, ch: i32) -> *mut u16 {
    if s.is_null() { return core::ptr::null_mut(); } let target = ch as u16; let mut i = 0usize; loop { let v = *s.add(i); if v == target { return s.add(i) as *mut _; } if v == 0 { return core::ptr::null_mut(); } i += 1; }
}

#[no_mangle]
pub unsafe extern "C" fn strrchr(s: *const core::ffi::c_char, ch: i32) -> *mut core::ffi::c_char {
    if s.is_null() { return core::ptr::null_mut(); } let target = ch as u8; let mut i = 0usize; let mut found = core::ptr::null_mut(); loop { let v = *s.add(i) as u8; if v == target { found = s.add(i) as *mut _; } if v == 0 { return found; } i += 1; }
}

#[no_mangle]
pub unsafe extern "C" fn wcsrchr(s: *const u16, ch: i32) -> *mut u16 {
    if s.is_null() { return core::ptr::null_mut(); } let target = ch as u16; let mut i = 0usize; let mut found = core::ptr::null_mut(); loop { let v = *s.add(i); if v == target { found = s.add(i) as *mut _; } if v == 0 { return found; } i += 1; }
}

#[no_mangle]
pub unsafe extern "C" fn bsearch(key:*const c_void,base:*const c_void,count:usize,width:usize,compare:Option<unsafe extern "C" fn(*const c_void,*const c_void)->i32>)->*mut c_void {
 if key.is_null()||compare.is_none()||width==0||(base.is_null()&&count!=0){return ptr::null_mut();} let cmp=compare.unwrap();let(mut lo,mut hi)=(0usize,count);while lo<hi{let mid=lo+(hi-lo)/2;let item=(base as *const u8).add(mid*width) as *const c_void;let c=cmp(key,item);if c==0{return item as *mut _;}if c<0{hi=mid;}else{lo=mid+1;}}ptr::null_mut()
}
#[no_mangle]
pub unsafe extern "C" fn qsort(base:*mut c_void,count:usize,width:usize,compare:Option<unsafe extern "C" fn(*const c_void,*const c_void)->i32>){
 if base.is_null()||count<2||width==0||compare.is_none(){return;}let cmp=compare.unwrap();let p=base as *mut u8;let swap=|a:usize,b:usize|unsafe{for j in 0..width{ptr::swap(p.add(a*width+j),p.add(b*width+j));}};let sift=|mut root:usize,end:usize|unsafe{loop{let child=root*2+1;if child>=end{break;}let mut largest=child;if child+1<end&&cmp(p.add(child*width) as _,p.add((child+1)*width) as _) <0{largest=child+1;}if cmp(p.add(root*width) as _,p.add(largest*width) as _)>=0{break;}swap(root,largest);root=largest;}};let mut start=count/2;while start>0{start-=1;sift(start,count);}let mut end=count;while end>1{end-=1;swap(0,end);sift(0,end);}
}
#[no_mangle]
pub unsafe extern "C" fn memchr(s:*const c_void,ch:i32,n:usize)->*mut c_void{if s.is_null(){return ptr::null_mut();}let p=s as *const u8;for i in 0..n{if *p.add(i)==ch as u8{return p.add(i) as *mut _;}}ptr::null_mut()}
#[no_mangle]
pub unsafe extern "C" fn strnlen(s:*const c_char,max:usize)->usize{if s.is_null(){return 0;}let mut i=0;while i<max&&*s.add(i)!=0{i+=1;}i}
#[no_mangle]
pub unsafe extern "C" fn strcpy(d:*mut c_char,s:*const c_char)->*mut c_char{if d.is_null()||s.is_null(){return d;}let mut i=0;loop{let c=*s.add(i);*d.add(i)=c;if c==0{break;}i+=1;}d}
#[no_mangle]
pub unsafe extern "C" fn strncpy(d:*mut c_char,s:*const c_char,n:usize)->*mut c_char{if d.is_null()||s.is_null(){return d;}let mut i=0;while i<n&&*s.add(i)!=0{*d.add(i)=*s.add(i);i+=1;}while i<n{*d.add(i)=0;i+=1;}d}
#[no_mangle]
pub unsafe extern "C" fn strcat(d:*mut c_char,s:*const c_char)->*mut c_char{if d.is_null()||s.is_null(){return d;}let mut i=0;while *d.add(i)!=0{i+=1;}let mut j=0;loop{let c=*s.add(j);*d.add(i+j)=c;if c==0{break;}j+=1;}d}
#[no_mangle]
pub unsafe extern "C" fn strncat(d:*mut c_char,s:*const c_char,n:usize)->*mut c_char{if d.is_null()||s.is_null(){return d;}let mut i=0;while *d.add(i)!=0{i+=1;}let mut j=0;while j<n&&*s.add(j)!=0{*d.add(i+j)=*s.add(j);j+=1;}*d.add(i+j)=0;d}
#[no_mangle]
pub unsafe extern "C" fn wcscpy(d:*mut u16,s:*const u16)->*mut u16{if d.is_null()||s.is_null(){return d;}let mut i=0;loop{let c=*s.add(i);*d.add(i)=c;if c==0{break;}i+=1;}d}
#[no_mangle]
pub unsafe extern "C" fn wcsncpy(d:*mut u16,s:*const u16,n:usize)->*mut u16{if d.is_null()||s.is_null(){return d;}let mut i=0;while i<n&&*s.add(i)!=0{*d.add(i)=*s.add(i);i+=1;}while i<n{*d.add(i)=0;i+=1;}d}
#[no_mangle]
pub unsafe extern "C" fn wcscat(d:*mut u16,s:*const u16)->*mut u16{if d.is_null()||s.is_null(){return d;}let mut i=0;while *d.add(i)!=0{i+=1;}let mut j=0;loop{let c=*s.add(j);*d.add(i+j)=c;if c==0{break;}j+=1;}d}
#[no_mangle]
pub unsafe extern "C" fn wcsncat(d:*mut u16,s:*const u16,n:usize)->*mut u16{if d.is_null()||s.is_null(){return d;}let mut i=0;while *d.add(i)!=0{i+=1;}let mut j=0;while j<n&&*s.add(j)!=0{*d.add(i+j)=*s.add(j);j+=1;}*d.add(i+j)=0;d}
#[cfg(test)] mod tests{use super::*;#[test]fn strtol_radix_and_end(){let s=b"  -42!\\0";let mut e=ptr::null_mut();assert_eq!(unsafe{strtol(s.as_ptr() as _,&mut e,10)},-42);assert_eq!(unsafe{*e},b'!' as i8);}#[test]fn binary_search(){unsafe extern "C" fn cmp(a:*const c_void,b:*const c_void)->i32{unsafe{*(a as *const i32)-*(b as *const i32)}}let a=[1i32,3,5,7];let k=5i32;let p=unsafe{bsearch(&k as *const _ as _,a.as_ptr() as _,4,4,Some(cmp))};assert_eq!(unsafe{*(p as *const i32)},5);}}
