//! Windows 11 x64 NTDLL extended bitmap routines.
//!
//! The Ex family uses pointer-sized bit indexes and 64-bit storage on x64.
//! As with the Windows RTL_BITMAP contract, callers must pass a valid bitmap
//! descriptor and a buffer large enough for SizeOfBitMap bits.
#![allow(non_snake_case, clippy::missing_safety_doc)]

type ULONG_PTR = usize;

/// x64 RTL_BITMAP_EX layout: pointer-sized bit count followed by a pointer to
/// the backing 64-bit words.
#[repr(C)]
pub struct RTL_BITMAP_EX {
    pub SizeOfBitMap: ULONG_PTR,
    pub Buffer: *mut ULONG_PTR,
}

const NOT_FOUND: ULONG_PTR = ULONG_PTR::MAX;

#[inline]
unsafe fn valid_bitmap(bitmap: *const RTL_BITMAP_EX) -> bool {
    if bitmap.is_null() { return false; }
    let b = unsafe { &*bitmap };
    b.SizeOfBitMap == 0 || !b.Buffer.is_null()
}

#[inline]
unsafe fn test_bit(bitmap: &RTL_BITMAP_EX, index: ULONG_PTR) -> bool {
    let word = unsafe { *bitmap.Buffer.add(index / ULONG_PTR::BITS as usize) };
    (word & (1usize << (index % ULONG_PTR::BITS as usize))) != 0
}

#[inline]
unsafe fn write_bit(bitmap: &RTL_BITMAP_EX, index: ULONG_PTR, set: bool) {
    let word = unsafe { bitmap.Buffer.add(index / ULONG_PTR::BITS as usize) };
    let mask = 1usize << (index % ULONG_PTR::BITS as usize);
    unsafe {
        if set { *word |= mask; } else { *word &= !mask; }
    }
}

unsafe fn modify_range(bitmap: *mut RTL_BITMAP_EX, start: ULONG_PTR, count: ULONG_PTR, set: bool) {
    if !unsafe { valid_bitmap(bitmap) } { return; }
    let b = unsafe { &*bitmap };
    let Some(end) = start.checked_add(count) else { return; };
    if end > b.SizeOfBitMap { return; }
    for index in start..end {
        unsafe { write_bit(b, index, set); }
    }
}

unsafe fn find_run(bitmap: *const RTL_BITMAP_EX, count: ULONG_PTR, hint: ULONG_PTR, want_set: bool) -> ULONG_PTR {
    if !unsafe { valid_bitmap(bitmap) } { return NOT_FOUND; }
    let b = unsafe { &*bitmap };
    if count == 0 || count > b.SizeOfBitMap { return NOT_FOUND; }
    let hint = hint.min(b.SizeOfBitMap.saturating_sub(1));
    // Windows searches from the hint to the end, then wraps to the beginning.
    let search = |begin: ULONG_PTR, end: ULONG_PTR| -> Option<ULONG_PTR> {
        if begin >= end || count > end - begin { return None; }
        let last = end - count;
        let mut start = begin;
        while start <= last {
            let mut matched = true;
            for bit in start..start + count {
                if unsafe { test_bit(b, bit) } != want_set {
                    start = bit + 1;
                    matched = false;
                    break;
                }
            }
            if matched { return Some(start); }
        }
        None
    };
    search(hint, b.SizeOfBitMap)
        .or_else(|| search(0, hint))
        .unwrap_or(NOT_FOUND)
}

#[no_mangle]
pub unsafe extern "system" fn RtlClearBitsEx(bitmap: *mut RTL_BITMAP_EX, starting_index: ULONG_PTR, number_to_clear: ULONG_PTR) {
    unsafe { modify_range(bitmap, starting_index, number_to_clear, false); }
}

#[no_mangle]
pub unsafe extern "system" fn RtlSetBitsEx(bitmap: *mut RTL_BITMAP_EX, starting_index: ULONG_PTR, number_to_set: ULONG_PTR) {
    unsafe { modify_range(bitmap, starting_index, number_to_set, true); }
}

#[no_mangle]
pub unsafe extern "system" fn RtlFindClearBitsEx(bitmap: *const RTL_BITMAP_EX, number_to_find: ULONG_PTR, hint_index: ULONG_PTR) -> ULONG_PTR {
    unsafe { find_run(bitmap, number_to_find, hint_index, false) }
}

#[no_mangle]
pub unsafe extern "system" fn RtlFindSetBitsEx(bitmap: *const RTL_BITMAP_EX, number_to_find: ULONG_PTR, hint_index: ULONG_PTR) -> ULONG_PTR {
    unsafe { find_run(bitmap, number_to_find, hint_index, true) }
}

#[no_mangle]
pub unsafe extern "system" fn RtlFindClearBitsAndSetEx(bitmap: *mut RTL_BITMAP_EX, number_to_find: ULONG_PTR, hint_index: ULONG_PTR) -> ULONG_PTR {
    let start = unsafe { find_run(bitmap, number_to_find, hint_index, false) };
    if start != NOT_FOUND { unsafe { modify_range(bitmap, start, number_to_find, true); } }
    start
}

#[no_mangle]
pub unsafe extern "system" fn RtlFindSetBitsAndClearEx(bitmap: *mut RTL_BITMAP_EX, number_to_find: ULONG_PTR, hint_index: ULONG_PTR) -> ULONG_PTR {
    let start = unsafe { find_run(bitmap, number_to_find, hint_index, true) };
    if start != NOT_FOUND { unsafe { modify_range(bitmap, start, number_to_find, false); } }
    start
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_and_sets_clear_run_across_word_boundary() {
        let mut words = [usize::MAX, usize::MAX];
        // Clear bits 62..66 (crosses the 64-bit boundary).
        words[0] &= !(3usize << 62);
        words[1] &= !3usize;
        let mut bitmap = RTL_BITMAP_EX { SizeOfBitMap: 128, Buffer: words.as_mut_ptr() };
        let found = unsafe { RtlFindClearBitsAndSetEx(&mut bitmap, 4, 60) };
        assert_eq!(found, 62);
        for bit in 62..66 { assert!(unsafe { test_bit(&bitmap, bit) }); }
    }

    #[test]
    fn wraps_hint_and_returns_pointer_sized_not_found() {
        let mut words = [0usize; 2];
        let bitmap = RTL_BITMAP_EX { SizeOfBitMap: 128, Buffer: words.as_mut_ptr() };
        assert_eq!(unsafe { RtlFindSetBitsEx(&bitmap, 1, 90) }, NOT_FOUND);
        words[0] = 1 << 3;
        assert_eq!(unsafe { RtlFindSetBitsEx(&bitmap, 1, 90) }, 3);
    }

    #[test]
    fn rejects_out_of_range_mutations_without_touching_words() {
        let mut words = [0usize; 1];
        let mut bitmap = RTL_BITMAP_EX { SizeOfBitMap: 64, Buffer: words.as_mut_ptr() };
        unsafe { RtlSetBitsEx(&mut bitmap, 63, 2); }
        assert_eq!(words[0], 0);
        unsafe { RtlSetBitsEx(&mut bitmap, 64, 0); }
        assert_eq!(words[0], 0);
    }
}
