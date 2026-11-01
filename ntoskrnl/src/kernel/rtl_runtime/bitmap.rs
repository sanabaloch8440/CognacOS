use super::RtlBitmap;
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
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_initialize_bit_map(bitmap: *mut RtlBitmap, buffer: *mut u32, size: u32) {
    if bitmap.is_null() { return; }
    unsafe { (*bitmap).size_of_bitmap = size; (*bitmap).buffer = buffer; }
}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_is_bitmap_clear(bitmap: *const RtlBitmap) -> u8 {
    if bitmap.is_null() { return 0; }
    (unsafe { rtl_number_of_set_bits(bitmap) == 0 }) as u8
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bitmap_basic_operations() {
        let mut words = [0u32; 2];
        let mut b = RtlBitmap { size_of_bitmap: 64, buffer: words.as_mut_ptr() };
        unsafe {
            rtl_set_bits(&mut b, 5, 3);
            assert_eq!(rtl_number_of_set_bits(&b), 3);
            assert_eq!(rtl_find_set_bits(&b, 3, 0), 5);
            assert_eq!(rtl_are_bits_set(&b, 5, 3), 1);
            rtl_clear_bits(&mut b, 5, 3);
            assert_eq!(rtl_number_of_set_bits(&b), 0);
        }
    }
}
