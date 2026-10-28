//! Minimal kernel-side debug transport and filter primitives.
//!
//! This is intentionally a small UART-backed fallback, not a replacement for
//! a KDNET/USB/1394 debugger transport. Format arguments are not consumed here;
//! DbgPrint's stable Rust declaration accepts the format pointer only.
use core::ffi::c_char;
pub type ULONG = u32;
pub type BOOLEAN = u8;
pub type NTSTATUS = i32;
const STATUS_SUCCESS: NTSTATUS = 0;
const STATUS_NOT_IMPLEMENTED: ULONG = 0xC000_0002;

#[inline]
fn enabled(component: ULONG, level: ULONG) -> bool {
    crate::ntoskrnl::debug::query_filter(component, level)
}

#[inline]
unsafe fn serial_byte(byte: u8) {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let mut ready: u8;
        for _ in 0..100_000 {
            core::arch::asm!("in al, dx", in("dx") 0x3fdu16, out("al") ready, options(nomem, nostack));
            if ready & 0x20 != 0 { break; }
            core::hint::spin_loop();
        }
        core::arch::asm!("out dx, al", in("dx") 0x3f8u16, in("al") byte, options(nomem, nostack));
    }
}

unsafe fn write_c_string(value: *const c_char) -> ULONG {
    if value.is_null() { return STATUS_NOT_IMPLEMENTED; }
    let mut count = 0usize;
    while count < 512 {
        let byte = unsafe { *(value.add(count) as *const u8) };
        if byte == 0 { break; }
        unsafe { serial_byte(byte); }
        count += 1;
    }
    0
}

/// Windows exports this routine from ntoskrnl; callers may supply varargs,
/// which are ignored by this initial transport implementation.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn DbgPrint(format: *const c_char) -> ULONG {
    crate::ntoskrnl::debug::record_unfiltered(
        crate::ntoskrnl::debug::COMPONENT_DEFAULT,
        crate::ntoskrnl::debug::LEVEL_WARNING,
        crate::ntoskrnl::debug::EVENT_DEBUG_PRINT,
        format as usize as u64, 0, 0,
    );
    unsafe { write_c_string(format) }
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn DbgPrintEx(component: ULONG, level: ULONG, format: *const c_char) -> ULONG {
    if !enabled(component, level) { return 0; }
    crate::ntoskrnl::debug::trace(
        component, level, crate::ntoskrnl::debug::EVENT_DEBUG_PRINT,
        format as usize as u64, 0, 0,
    );
    unsafe { write_c_string(format) }
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn DbgQueryDebugFilterState(component: ULONG, level: ULONG) -> NTSTATUS {
    if enabled(component, level) { 1 } else { 0 }
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn DbgSetDebugFilterState(component: ULONG, level: ULONG, state: BOOLEAN) -> NTSTATUS {
    if crate::ntoskrnl::debug::set_filter(component, level, state != 0) {
        STATUS_SUCCESS
    } else {
        0xC000_000D_u32
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn DbgPrompt(_prompt: *const c_char, _response: *mut c_char, _length: ULONG) -> ULONG {
    // A UART output-only console cannot safely claim to have collected a
    // debugger response. A real KD input channel must supply this operation.
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn DbgBreakPoint() {
    #[cfg(target_arch = "x86_64")]
    core::arch::asm!("int3", options(nomem, nostack));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filter_mask_tracks_component_and_level() {
        unsafe {
            assert_eq!(DbgSetDebugFilterState(11, 4, 1), STATUS_SUCCESS);
            assert_eq!(DbgQueryDebugFilterState(11, 4), 1);
            assert_eq!(DbgQueryDebugFilterState(12, 4), 0);
            assert_eq!(DbgSetDebugFilterState(11, 4, 0), STATUS_SUCCESS);
            assert_eq!(DbgQueryDebugFilterState(11, 4), 0);
        }
    }
}
