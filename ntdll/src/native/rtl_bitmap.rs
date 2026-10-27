//! Rust port of NTDLL RTL bitmap routines from Wine's rtlbitmap.c.
//! x64 ABI definitions and safe-range checks are kept explicit. This module
//! remains source-first until Rust modules are linked into the native NTDLL.

#![allow(non_snake_case)]

pub type ULONG = u32;
pub type BOOLEAN = u8;
pub type CCHAR = i8;

pub const INVALID_BIT: ULONG = u32::MAX;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RTL_BITMAP {
    pub SizeOfBitMap: ULONG,
    pub Buffer: *mut ULONG,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct RTL_BITMAP_RUN {
    pub StartingIndex: ULONG,
    pub NumberOfBits: ULONG,
}

#[inline]
unsafe fn valid(bitmap: *const RTL_BITMAP) -> bool {
    if bitmap.is_null() { return false; }
    // SAFETY: caller supplies a readable RTL_BITMAP descriptor.
    let b = unsafe { &*bitmap };
    b.SizeOfBitMap == 0 || !b.Buffer.is_null()
}

#[inline]
unsafe fn get(bitmap: *const RTL_BITMAP, bit: ULONG) -> bool {
    // SAFETY: caller has validated bitmap and bit is in range.
    let b = unsafe { &*bitmap };
    unsafe { (*b.Buffer.add((bit / 32) as usize) & (1u32 << (bit & 31))) != 0 }
}

#[inline]
unsafe fn put(bitmap: *mut RTL_BITMAP, bit: ULONG, value: bool) {
    // SAFETY: caller has validated bitmap and bit is in range.
    let b = unsafe { &mut *bitmap };
    let word = unsafe { b.Buffer.add((bit / 32) as usize) };
    let mask = 1u32 << (bit & 31);
    unsafe {
        if value { *word |= mask; } else { *word &= !mask; }
    }
}

#[no_mangle]
pub unsafe extern "system" fn RtlInitializeBitMap(bitmap: *mut RTL_BITMAP, buffer: *mut ULONG, size: ULONG) {
    if bitmap.is_null() { return; }
    // SAFETY: non-null output descriptor is caller-writable.
    unsafe { (*bitmap).SizeOfBitMap = size; (*bitmap).Buffer = buffer; }
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetAllBits(bitmap: *mut RTL_BITMAP) {
    if !unsafe { valid(bitmap) } { return; }
    let b = unsafe { &mut *bitmap };
    let words = (b.SizeOfBitMap as usize + 31) / 32;
    if words != 0 { unsafe { core::ptr::write_bytes(b.Buffer, 0xff, words); } }
}

#[no_mangle]
pub unsafe extern "system" fn RtlClearAllBits(bitmap: *mut RTL_BITMAP) {
    if !unsafe { valid(bitmap) } { return; }
    let b = unsafe { &mut *bitmap };
    let words = (b.SizeOfBitMap as usize + 31) / 32;
    if words != 0 { unsafe { core::ptr::write_bytes(b.Buffer, 0, words); } }
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetBits(bitmap: *mut RTL_BITMAP, start: ULONG, count: ULONG) {
    if !unsafe { valid(bitmap) } || count == 0 { return; }
    let size = unsafe { (*bitmap).SizeOfBitMap };
    if start >= size || count > size - start { return; }
    for bit in start..start + count { unsafe { put(bitmap, bit, true); } }
}

#[no_mangle]
pub unsafe extern "system" fn RtlClearBits(bitmap: *mut RTL_BITMAP, start: ULONG, count: ULONG) {
    if !unsafe { valid(bitmap) } || count == 0 { return; }
    let size = unsafe { (*bitmap).SizeOfBitMap };
    if start >= size || count > size - start { return; }
    for bit in start..start + count { unsafe { put(bitmap, bit, false); } }
}

#[no_mangle]
pub unsafe extern "system" fn RtlAreBitsSet(bitmap: *const RTL_BITMAP, start: ULONG, count: ULONG) -> BOOLEAN {
    if !unsafe { valid(bitmap) } || count == 0 { return 0; }
    let size = unsafe { (*bitmap).SizeOfBitMap };
    if start >= size || count > size - start { return 0; }
    for bit in start..start + count { if !unsafe { get(bitmap, bit) } { return 0; } }
    1
}

#[no_mangle]
pub unsafe extern "system" fn RtlAreBitsClear(bitmap: *const RTL_BITMAP, start: ULONG, count: ULONG) -> BOOLEAN {
    if !unsafe { valid(bitmap) } || count == 0 { return 0; }
    let size = unsafe { (*bitmap).SizeOfBitMap };
    if start >= size || count > size - start { return 0; }
    for bit in start..start + count { if unsafe { get(bitmap, bit) } { return 0; } }
    1
}

unsafe fn find_run(bitmap: *const RTL_BITMAP, count: ULONG, hint: ULONG, wanted: bool) -> ULONG {
    if !unsafe { valid(bitmap) } || count == 0 { return INVALID_BIT; }
    let size = unsafe { (*bitmap).SizeOfBitMap };
    if count > size { return INVALID_BIT; }
    let start = if hint >= size || count > size - hint { 0 } else { hint };
    for range in [start..size, 0..start] {
        if range.is_empty() { continue; }
        let mut run = 0u32;
        let mut run_start = range.start;
        for bit in range {
            if unsafe { get(bitmap, bit) } == wanted {
                if run == 0 { run_start = bit; }
                run += 1;
                if run == count { return run_start; }
            } else { run = 0; }
        }
    }
    INVALID_BIT
}

#[no_mangle]
pub unsafe extern "system" fn RtlFindSetBits(bitmap: *const RTL_BITMAP, count: ULONG, hint: ULONG) -> ULONG {
    unsafe { find_run(bitmap, count, hint, true) }
}

#[no_mangle]
pub unsafe extern "system" fn RtlFindClearBits(bitmap: *const RTL_BITMAP, count: ULONG, hint: ULONG) -> ULONG {
    unsafe { find_run(bitmap, count, hint, false) }
}

#[no_mangle]
pub unsafe extern "system" fn RtlFindSetBitsAndClear(bitmap: *mut RTL_BITMAP, count: ULONG, hint: ULONG) -> ULONG {
    let found = unsafe { find_run(bitmap, count, hint, true) };
    if found != INVALID_BIT { unsafe { RtlClearBits(bitmap, found, count); } }
    found
}

#[no_mangle]
pub unsafe extern "system" fn RtlFindClearBitsAndSet(bitmap: *mut RTL_BITMAP, count: ULONG, hint: ULONG) -> ULONG {
    let found = unsafe { find_run(bitmap, count, hint, false) };
    if found != INVALID_BIT { unsafe { RtlSetBits(bitmap, found, count); } }
    found
}

#[no_mangle]
pub unsafe extern "system" fn RtlNumberOfSetBits(bitmap: *const RTL_BITMAP) -> ULONG {
    if !unsafe { valid(bitmap) } { return 0; }
    let b = unsafe { &*bitmap };
    let words = (b.SizeOfBitMap / 32) as usize;
    let mut total = 0u32;
    for i in 0..words { total += unsafe { *b.Buffer.add(i) }.count_ones(); }
    let remainder = b.SizeOfBitMap & 31;
    if remainder != 0 {
        let mask = (1u32 << remainder) - 1;
        total += unsafe { *b.Buffer.add(words) & mask }.count_ones();
    }
    total
}

#[no_mangle]
pub unsafe extern "system" fn RtlNumberOfClearBits(bitmap: *const RTL_BITMAP) -> ULONG {
    if !unsafe { valid(bitmap) } { return 0; }
    unsafe { (*bitmap).SizeOfBitMap - RtlNumberOfSetBits(bitmap) }
}

#[no_mangle]
pub extern "system" fn RtlFindMostSignificantBit(value: u64) -> CCHAR {
    if value == 0 { -1 } else { (63 - value.leading_zeros()) as i8 }
}

#[no_mangle]
pub extern "system" fn RtlFindLeastSignificantBit(value: u64) -> CCHAR {
    if value == 0 { -1 } else { value.trailing_zeros() as i8 }
}

unsafe fn find_forward_run(bitmap: *const RTL_BITMAP, start: ULONG, wanted: bool, position: *mut ULONG) -> ULONG {
    if !unsafe { valid(bitmap) } || position.is_null() { return 0; }
    let size = unsafe { (*bitmap).SizeOfBitMap };
    if start >= size { return 0; }
    let mut i = start;
    while i < size && unsafe { get(bitmap, i) } != wanted { i += 1; }
    if i == size { return 0; }
    let first = i;
    while i < size && unsafe { get(bitmap, i) } == wanted { i += 1; }
    unsafe { *position = first; }
    i - first
}

pub unsafe extern "system" fn RtlFindNextForwardRunSet(bitmap: *const RTL_BITMAP, start: ULONG, position: *mut ULONG) -> ULONG {
    unsafe { find_forward_run(bitmap, start, true, position) }
}

#[no_mangle]
pub unsafe extern "system" fn RtlFindNextForwardRunClear(bitmap: *const RTL_BITMAP, start: ULONG, position: *mut ULONG) -> ULONG {
    unsafe { find_forward_run(bitmap, start, false, position) }
}

unsafe fn find_backward_run(bitmap: *const RTL_BITMAP, start: ULONG, wanted: bool, position: *mut ULONG) -> ULONG {
    if !unsafe { valid(bitmap) } || position.is_null() { return 0; }
    let size = unsafe { (*bitmap).SizeOfBitMap };
    if size == 0 || start >= size { return 0; }
    let mut i = start;
    while unsafe { get(bitmap, i) } != wanted {
        if i == 0 { return 0; }
        i -= 1;
    }
    let mut first = i;
    while first > 0 && unsafe { get(bitmap, first - 1) } == wanted { first -= 1; }
    unsafe { *position = first; }
    i - first + 1
}

pub unsafe extern "system" fn RtlFindLastBackwardRunSet(bitmap: *const RTL_BITMAP, start: ULONG, position: *mut ULONG) -> ULONG {
    unsafe { find_backward_run(bitmap, start, true, position) }
}

#[no_mangle]
pub unsafe extern "system" fn RtlFindLastBackwardRunClear(bitmap: *const RTL_BITMAP, start: ULONG, position: *mut ULONG) -> ULONG {
    unsafe { find_backward_run(bitmap, start, false, position) }
}

unsafe fn find_runs(bitmap: *const RTL_BITMAP, runs: *mut RTL_BITMAP_RUN, capacity: ULONG, longest: BOOLEAN, wanted: bool) -> ULONG {
    if !unsafe { valid(bitmap) } || capacity == 0 { return INVALID_BIT; }
    if runs.is_null() { return 0; }
    let size = unsafe { (*bitmap).SizeOfBitMap };
    let mut stored = 0u32;
    let mut i = 0u32;
    while i < size {
        if unsafe { get(bitmap, i) } != wanted { i += 1; continue; }
        let start = i;
        while i < size && unsafe { get(bitmap, i) } == wanted { i += 1; }
        let candidate = RTL_BITMAP_RUN { StartingIndex: start, NumberOfBits: i - start };
        if longest == 0 {
            unsafe { runs.add(stored as usize).write(candidate); }
            stored += 1;
            if stored == capacity { break; }
        } else if stored < capacity {
            let mut at = stored;
            while at > 0 && unsafe { (*runs.add((at - 1) as usize)).NumberOfBits < candidate.NumberOfBits } {
                unsafe { runs.add(at as usize).write(*runs.add((at - 1) as usize)); }
                at -= 1;
            }
            unsafe { runs.add(at as usize).write(candidate); }
            stored += 1;
        } else if unsafe { (*runs.add((stored - 1) as usize)).NumberOfBits < candidate.NumberOfBits } {
            let mut at = stored - 1;
            while at > 0 && unsafe { (*runs.add((at - 1) as usize)).NumberOfBits < candidate.NumberOfBits } {
                unsafe { runs.add(at as usize).write(*runs.add((at - 1) as usize)); }
                at -= 1;
            }
            unsafe { runs.add(at as usize).write(candidate); }
        }
    }
    stored
}

pub unsafe extern "system" fn RtlFindSetRuns(bitmap: *const RTL_BITMAP, runs: *mut RTL_BITMAP_RUN, count: ULONG, longest: BOOLEAN) -> ULONG {
    unsafe { find_runs(bitmap, runs, count, longest, true) }
}

#[no_mangle]
pub unsafe extern "system" fn RtlFindClearRuns(bitmap: *const RTL_BITMAP, runs: *mut RTL_BITMAP_RUN, count: ULONG, longest: BOOLEAN) -> ULONG {
    unsafe { find_runs(bitmap, runs, count, longest, false) }
}

pub unsafe extern "system" fn RtlFindLongestRunSet(bitmap: *const RTL_BITMAP, start: *mut ULONG) -> ULONG {
    let mut run = RTL_BITMAP_RUN::default();
    let count = unsafe { RtlFindSetRuns(bitmap, &mut run, 1, 1) };
    if count == 1 {
        if !start.is_null() { unsafe { *start = run.StartingIndex; } }
        run.NumberOfBits
    } else { 0 }
}

#[no_mangle]
pub unsafe extern "system" fn RtlFindLongestRunClear(bitmap: *const RTL_BITMAP, start: *mut ULONG) -> ULONG {
    let mut run = RTL_BITMAP_RUN::default();
    let count = unsafe { RtlFindClearRuns(bitmap, &mut run, 1, 1) };
    if count == 1 {
        if !start.is_null() { unsafe { *start = run.StartingIndex; } }
        run.NumberOfBits
    } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bitmap(words: &mut [u32], size: u32) -> RTL_BITMAP {
        RTL_BITMAP { SizeOfBitMap: size, Buffer: words.as_mut_ptr() }
    }

    #[test]
    fn set_clear_and_count_bits() {
        let mut words = [0u32; 2];
        let mut b = bitmap(&mut words, 40);
        unsafe { RtlSetBits(&mut b, 3, 5); }
        assert_eq!(unsafe { RtlNumberOfSetBits(&b) }, 5);
        assert_eq!(unsafe { RtlAreBitsSet(&b, 3, 5) }, 1);
        assert_eq!(unsafe { RtlAreBitsClear(&b, 0, 3) }, 1);
        unsafe { RtlClearBits(&mut b, 5, 2); }
        assert_eq!(unsafe { RtlNumberOfClearBits(&b) }, 37);
        assert_eq!(unsafe { RtlAreBitsSet(&b, 3, 5) }, 0);
    }

    #[test]
    fn find_and_mutate_runs() {
        let mut words = [0u32; 2];
        let mut b = bitmap(&mut words, 40);
        unsafe {
            RtlSetBits(&mut b, 8, 6);
            assert_eq!(RtlFindSetBits(&b, 4, 0), 8);
            assert_eq!(RtlFindSetBitsAndClear(&mut b, 4, 0), 8);
            assert_eq!(RtlAreBitsClear(&b, 8, 4), 1);
            assert_eq!(RtlFindClearBitsAndSet(&mut b, 3, 0), 0);
        }
    }

    #[test]
    fn bit_scans_and_forward_backward_runs() {
        assert_eq!(RtlFindMostSignificantBit(0), -1);
        assert_eq!(RtlFindMostSignificantBit(1u64 << 63), 63);
        assert_eq!(RtlFindLeastSignificantBit(0), -1);
        assert_eq!(RtlFindLeastSignificantBit(0x120), 5);
        let mut words = [0u32; 2];
        let mut b = bitmap(&mut words, 40);
        unsafe {
            RtlSetBits(&mut b, 10, 5);
            let mut position = 0;
            assert_eq!(RtlFindNextForwardRunSet(&b, 0, &mut position), 5);
            assert_eq!(position, 10);
            assert_eq!(RtlFindLastBackwardRunSet(&b, 14, &mut position), 5);
            assert_eq!(position, 10);
        }
    }

    #[test]
    fn longest_runs_and_tail_bits() {
        let mut words = [0u32; 2];
        let mut b = bitmap(&mut words, 35);
        unsafe {
            RtlSetBits(&mut b, 2, 4);
            RtlSetBits(&mut b, 20, 8);
            let mut start = 0;
            assert_eq!(RtlFindLongestRunSet(&b, &mut start), 8);
            assert_eq!(start, 20);
            assert_eq!(RtlNumberOfSetBits(&b), 12);
            RtlSetAllBits(&mut b);
            assert_eq!(RtlNumberOfSetBits(&b), 35);
            RtlClearAllBits(&mut b);
            assert_eq!(RtlNumberOfSetBits(&b), 0);
        }
    }
}
