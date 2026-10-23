//! RTL string constructors that allocate from the bounded RTL pool.
use super::UnicodeString;

/// RtlCreateUnicodeString: allocates a NUL-terminated copy of a UTF-16 string.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_create_unicode_string(dst:*mut UnicodeString,src:*const u16)->u8 {
 if dst.is_null()||src.is_null(){return 0;}
 let mut n=0usize;
 while n < (u16::MAX as usize / 2) && unsafe{*src.add(n)}!=0 { n+=1; }
 let Some(bytes)=n.checked_add(1).and_then(|v|v.checked_mul(2)) else{return 0;};
 if bytes>u16::MAX as usize{return 0;}
 let p=super::pool::allocate(bytes);if p.is_null(){return 0;}
 unsafe{core::ptr::copy_nonoverlapping(src,p.cast::<u16>(),n);*p.cast::<u16>().add(n)=0;}
 let d=unsafe{&mut *dst};d.buffer=p.cast::<u16>();d.length=(n*2) as u16;d.maximum_length=bytes as u16;1
}

/// RtlCreateUnicodeStringFromAsciiz: widens each input byte to UTF-16.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_create_unicode_string_from_asciiz(dst:*mut UnicodeString,src:*const u8)->u8 {
 if dst.is_null()||src.is_null(){return 0;}
 let mut n=0usize;
 while n < (u16::MAX as usize / 2) && unsafe{*src.add(n)}!=0 { n+=1; }
 let Some(bytes)=n.checked_add(1).and_then(|v|v.checked_mul(2)) else{return 0;};
 if bytes>u16::MAX as usize{return 0;}
 let p=super::pool::allocate(bytes);if p.is_null(){return 0;}
 for i in 0..n {unsafe{*p.cast::<u16>().add(i)=*src.add(i) as u16;}}
 unsafe{*p.cast::<u16>().add(n)=0;}
 let d=unsafe{&mut *dst};d.buffer=p.cast::<u16>();d.length=(n*2) as u16;d.maximum_length=bytes as u16;1
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn allocated_unicode_constructors_release_pool_storage() {
        let src = [b'A' as u16, b'B' as u16, 0];
        let mut out = UnicodeString { length: 0, maximum_length: 0, buffer: core::ptr::null_mut() };
        assert_eq!(unsafe { rtl_create_unicode_string(&mut out, src.as_ptr()) }, 1);
        assert_eq!(out.length, 4);
        assert_eq!(unsafe { *out.buffer.add(0) }, b'A' as u16);
        assert_eq!(unsafe { *out.buffer.add(1) }, b'B' as u16);
        unsafe { super::super::rtl_free_unicode_string(&mut out); }
        assert!(out.buffer.is_null());
        assert_eq!(out.length, 0);
    }
}
