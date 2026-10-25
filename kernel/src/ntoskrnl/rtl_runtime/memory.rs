use core::ffi::c_void;
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_compare_memory(a:*const u8,b:*const u8,n:u64)->u64 {
    if n==0{return 0;}if a.is_null()||b.is_null(){return 0;}let mut i=0;while i<n&&unsafe{*a.add(i as usize)==*b.add(i as usize)}{i+=1;}i
}

/// Compare the source against a repeating ULONG pattern and return the
/// matching prefix length in bytes. Windows declares the result as ULONG.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_compare_memory_ulong(a: *const u32, n: u64, value: u32) -> u32 {
    if n == 0 { return 0; }
    if a.is_null() { return 0; }
    let bytes = a as *const u8;
    let pattern = value.to_ne_bytes();
    let mut matched = 0u64;
    while matched < n {
        if unsafe { *bytes.add(matched as usize) } != pattern[(matched as usize) & 3] {
            break;
        }
        matched += 1;
    }
    matched as u32
}

#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_compute_crc32(initial:u32,buffer:*const u8,length:u32)->u32 {
    if buffer.is_null()||length==0{return initial;}let mut crc=!initial;for i in 0..length as usize{crc^=unsafe{*buffer.add(i)} as u32;for _ in 0..8{crc=if crc&1!=0{(crc>>1)^0xedb8_8320}else{crc>>1};}}!crc
}

#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_crc32(initial:u32,buffer:*const u8,length:u32)->u32 { unsafe{rtl_compute_crc32(initial,buffer,length)} }

#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_copy_memory(dst:*mut c_void,src:*const c_void,n:u64) { if n>0&&!dst.is_null()&&!src.is_null(){unsafe{core::ptr::copy(src as *const u8,dst as *mut u8,n as usize);}} }

#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_fill_memory(dst:*mut c_void,n:u64,value:u8) { if n>0&&!dst.is_null(){unsafe{core::ptr::write_bytes(dst as *mut u8,value,n as usize);}} }

#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_copy_memory_non_temporal(dst:*mut c_void,src:*const c_void,n:u64) { unsafe { rtl_copy_memory(dst,src,n) } }

#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_fill_memory_non_temporal(dst:*mut c_void,n:u64,value:u8) { unsafe { rtl_fill_memory(dst,n,value) } }

#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_move_memory(dst:*mut c_void,src:*const c_void,n:u64) { if n>0&&!dst.is_null()&&!src.is_null(){unsafe{core::ptr::copy(src as *const u8,dst as *mut u8,n as usize);}} }

#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_zero_memory(dst:*mut c_void,n:u64) { if n>0&&!dst.is_null(){unsafe{core::ptr::write_bytes(dst as *mut u8,0,n as usize);}} }
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_is_zero_memory(buffer: *const c_void, length: u64) -> u8 {
    if length == 0 { return 1; }
    if buffer.is_null() { return 0; }
    for i in 0..length as usize {
        if unsafe { *((buffer as *const u8).add(i)) } != 0 { return 0; }
    }
    1
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_unsigned_multiply_high(a: u64, b: u64) -> u64 {
    (((a as u128) * (b as u128)) >> 64) as u64
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn memory_and_crc_basics() {
        let data = b"123456789";
        assert_eq!(unsafe { rtl_compare_memory(data.as_ptr(), data.as_ptr(), 9) }, 9);
        assert_eq!(unsafe { rtl_compute_crc32(0, data.as_ptr(), 9) }, 0xcbf43926);
        assert_eq!(unsafe { rtl_is_zero_memory([0u8; 4].as_ptr() as *const c_void, 4) }, 1);
    }
}


// Volatile RTL memory primitives. These deliberately use the compiler's volatile
// accesses so MMIO-like callers do not have their operations elided or folded.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_copy_volatile_memory(dst: *mut c_void, src: *const c_void, n: u64) -> *mut c_void {
    if n != 0 && !dst.is_null() && !src.is_null() {
        for i in 0..n as usize {
            let value = unsafe { core::ptr::read_volatile((src as *const u8).add(i)) };
            unsafe { core::ptr::write_volatile((dst as *mut u8).add(i), value) };
        }
    }
    dst
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_move_volatile_memory(dst: *mut c_void, src: *const c_void, n: u64) -> *mut c_void {
    if n != 0 && !dst.is_null() && !src.is_null() {
        // Match memmove overlap semantics while retaining volatile accesses.
        if (dst as usize) <= (src as usize) || (dst as usize) >= (src as usize).saturating_add(n as usize) {
            for i in 0..n as usize {
                let value = unsafe { core::ptr::read_volatile((src as *const u8).add(i)) };
                unsafe { core::ptr::write_volatile((dst as *mut u8).add(i), value) };
            }
        } else {
            for i in (0..n as usize).rev() {
                let value = unsafe { core::ptr::read_volatile((src as *const u8).add(i)) };
                unsafe { core::ptr::write_volatile((dst as *mut u8).add(i), value) };
            }
        }
    }
    dst
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_zero_volatile_memory(dst: *mut c_void, n: u64) -> *mut c_void {
    if n != 0 && !dst.is_null() {
        for i in 0..n as usize {
            unsafe { core::ptr::write_volatile((dst as *mut u8).add(i), 0) };
        }
    }
    dst
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_fill_memory_ulong(dst: *mut c_void, n: u64, pattern: u32) {
    if n == 0 || dst.is_null() { return; }
    let words = n as usize / core::mem::size_of::<u32>();
    let out = dst as *mut u32;
    for i in 0..words {
        unsafe { core::ptr::write_unaligned(out.add(i), pattern) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_fill_memory_ulonglong(dst: *mut c_void, n: u64, pattern: u64) {
    if n == 0 || dst.is_null() { return; }
    let words = n as usize / core::mem::size_of::<u64>();
    let out = dst as *mut u64;
    for i in 0..words {
        unsafe { core::ptr::write_unaligned(out.add(i), pattern) };
    }
}

#[cfg(test)]
mod volatile_tests {
    use super::*;

    #[test]
    fn volatile_copy_move_zero_and_fill() {
        let src = [1u8, 2, 3, 4];
        let mut dst = [0u8; 4];
        assert_eq!(unsafe { rtl_copy_volatile_memory(dst.as_mut_ptr() as *mut c_void, src.as_ptr() as *const c_void, 4) }, dst.as_mut_ptr() as *mut c_void);
        assert_eq!(dst, src);

        // Overlapping move must copy backwards when destination starts inside source.
        let mut overlap = [0u8, 1, 2, 3, 4, 5];
        unsafe { rtl_move_volatile_memory(overlap.as_mut_ptr().add(1) as *mut c_void, overlap.as_ptr() as *const c_void, 5); }
        assert_eq!(overlap, [0, 0, 1, 2, 3, 4]);

        unsafe { rtl_zero_volatile_memory(dst.as_mut_ptr() as *mut c_void, 4); }
        assert_eq!(dst, [0; 4]);

        let mut words = [0u32; 3];
        unsafe { rtl_fill_memory_ulong(words.as_mut_ptr() as *mut c_void, 12, 0xAABB_CCDD); }
        assert_eq!(words, [0xAABB_CCDD; 3]);

        let mut wide = [0u64; 2];
        unsafe { rtl_fill_memory_ulonglong(wide.as_mut_ptr() as *mut c_void, 16, 0x0123_4567_89AB_CDEF); }
        assert_eq!(wide, [0x0123_4567_89AB_CDEF; 2]);
    }
}


#[cfg(test)]
mod memory_abi_tests {
    use super::*;

    #[test]
    fn memory_primitives_handle_overlap_and_unaligned_words() {
        let mut bytes = [0u8, 1, 2, 3, 4, 5, 6, 7];
        unsafe {
            rtl_move_memory(bytes.as_mut_ptr().add(1) as *mut c_void,
                            bytes.as_ptr() as *const c_void, 7);
        }
        assert_eq!(bytes, [0, 0, 1, 2, 3, 4, 5, 6]);

        let pattern = 0xA1B2_C3D4u32;
        let mut unaligned = [0u8; 13];
        unsafe {
            rtl_fill_memory_ulong(unaligned.as_mut_ptr().add(1) as *mut c_void, 12, pattern);
        }
        let expected = pattern.to_ne_bytes();
        for chunk in unaligned[1..13].chunks_exact(4) {
            assert_eq!(chunk, expected);
        }
        assert_eq!(unsafe {
            rtl_compare_memory_ulong(unaligned.as_ptr().add(1) as *const u32, 12, pattern)
        }, 12);
        unaligned[1 + 4 + 1] ^= 0xff;
        assert_eq!(unsafe {
            rtl_compare_memory_ulong(unaligned.as_ptr().add(1) as *const u32, 12, pattern)
        }, 5);
        // Partial lengths still compare all requested bytes.
        let mut partial = pattern.to_ne_bytes();
        partial[3] ^= 1;
        assert_eq!(unsafe { rtl_compare_memory_ulong(partial.as_ptr() as *const u32, 3, pattern) }, 3);
        assert_eq!(unsafe { rtl_compare_memory_ulong(partial.as_ptr() as *const u32, 4, pattern) }, 3);
    }

    #[test]
    fn zero_length_and_null_inputs_are_bounded() {
        assert_eq!(unsafe { rtl_compare_memory(core::ptr::null(), core::ptr::null(), 0) }, 0);
        assert_eq!(unsafe { rtl_compare_memory(core::ptr::null(), core::ptr::null(), 1) }, 0);
        assert_eq!(unsafe { rtl_compare_memory_ulong(core::ptr::null(), 4, 0) }, 0);
        unsafe {
            rtl_copy_memory(core::ptr::null_mut(), core::ptr::null(), 0);
            rtl_move_memory(core::ptr::null_mut(), core::ptr::null(), 0);
            rtl_fill_memory(core::ptr::null_mut(), 0, 0xff);
            rtl_zero_memory(core::ptr::null_mut(), 0);
        }
    }
}
