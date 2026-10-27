//! CognacOS's native Rust implementation of the Windows 11 x64 NTDLL.
//!
//! Subsystems are linked into one freestanding PE32+ DLL. The pinned Windows 11
//! x64 export inventory defines the public name surface; canonical Windows x64
//! SYSCALL stubs connect implemented Nt* APIs to the kernel's single syscall
//! entry. Remaining exports are explicitly incomplete and must not be treated
//! as full Windows semantic implementations merely because their names exist.
#![cfg_attr(target_os = "windows", no_std)]
#![allow(non_camel_case_types, non_snake_case, dead_code, improper_ctypes)]

extern crate alloc;

// Linker-only compatibility entry points. Assembly definitions satisfy the PE
// linker without creating Rust #[no_mangle] exports for implementation symbols.
#[cfg(target_os = "windows")]
core::arch::global_asm!(r#"
    .text
    .p2align 4
    .globl DllMain
DllMain:
    movl $1, %eax
    ret

    .p2align 4
    .globl __CxxFrameHandler3
__CxxFrameHandler3:
    // ExceptionContinueSearch: link-only fallback, not a C++ EH implementation.
    movl $1, %eax
    ret
"#, options(att_syntax));

pub mod alpc;
pub mod atom;
pub mod crypt;
pub mod debugbuffer;
pub mod env;
pub mod exception;
pub mod handletable;
pub mod heap;
pub mod math;
pub mod misc;
pub mod path;
pub mod printf;
pub mod process;
pub mod rbtree;
pub mod reg;
pub mod resource;
pub mod rtl;
pub mod rtl_bitmap;
pub mod rtl_error;
pub mod rtl_heap;
pub mod rtl_large_int;
pub mod rtl_loader;
pub mod rtl_memory;
pub mod rtl_native;
pub mod rtl_peb;
pub mod rtl_string;
pub mod string;
#[cfg(not(target_os = "windows"))]
pub mod syscall_x86_64;
pub mod time;
pub mod trace;
pub mod version;

// Committed Windows 11 x64 export surface: real Rust symbols, not build-generated names.
pub mod windows11_exports;

#[cfg(target_os = "windows")]
struct NtdllGlobalAllocator;

#[cfg(target_os = "windows")]
unsafe impl core::alloc::GlobalAlloc for NtdllGlobalAllocator {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        let alignment = layout.align().max(core::mem::align_of::<usize>());
        let Some(total) = layout.size().max(1)
            .checked_add(alignment - 1)
            .and_then(|n| n.checked_add(core::mem::size_of::<usize>())) else {
                return core::ptr::null_mut();
            };
        let heap = unsafe { crate::rtl_heap::RtlGetProcessHeap() };
        if heap.is_null() { return core::ptr::null_mut(); }
        let base = unsafe { crate::rtl_heap::RtlAllocateHeap(heap, 0, total) };
        if base.is_null() { return core::ptr::null_mut(); }
        let Some(start) = (base as usize).checked_add(core::mem::size_of::<usize>()) else {
            let _ = unsafe { crate::rtl_heap::RtlFreeHeap(heap, 0, base) };
            return core::ptr::null_mut();
        };
        let Some(aligned) = start.checked_add(alignment - 1).map(|v| v & !(alignment - 1)) else {
            let _ = unsafe { crate::rtl_heap::RtlFreeHeap(heap, 0, base) };
            return core::ptr::null_mut();
        };
        unsafe { ((aligned - core::mem::size_of::<usize>()) as *mut usize).write(base as usize); }
        aligned as *mut u8
    }

    unsafe fn dealloc(&self, pointer: *mut u8, _layout: core::alloc::Layout) {
        if pointer.is_null() { return; }
        let base = unsafe {
            pointer.sub(core::mem::size_of::<usize>()).cast::<usize>().read()
        } as *mut core::ffi::c_void;
        let heap = unsafe { crate::rtl_heap::RtlGetProcessHeap() };
        if !heap.is_null() {
            let _ = unsafe { crate::rtl_heap::RtlFreeHeap(heap, 0, base) };
        }
    }
}

#[cfg(target_os = "windows")]
#[global_allocator]
static NTDLL_GLOBAL_ALLOCATOR: NtdllGlobalAllocator = NtdllGlobalAllocator;

/// Rust panics are fatal in this freestanding DLL. Use the Windows x64 fast-fail
/// instruction directly: no formatting, allocator, exception handlers, or
/// potentially recursive NTDLL calls are permitted on this path.
#[cfg(target_os = "windows")]
#[panic_handler]
fn cognac_ntdll_panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    unsafe {
        core::arch::asm!(
            "int 0x29",
            in("rcx") 7usize, // FAST_FAIL_FATAL_APP_EXIT
            options(noreturn, nostack)
        );
    }
}

// Generate ABI placeholders for exports not yet implemented by the Rust ports.
include!(concat!(env!("OUT_DIR"), "/ntdll_export_stubs.rs"));
