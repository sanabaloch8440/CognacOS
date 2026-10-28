//! CognacOS shell32.dll: Rust x64 shell API placeholder layer.
#![cfg_attr(target_os = "windows", no_std)]
#![allow(non_snake_case, dead_code)]

use cognacos_nt_abi::windows::{NTSTATUS, ULONG_PTR};

// HRESULT E_NOTIMPL. NTSTATUS is used only as the shared 32-bit signed ABI carrier.
const HRESULT_E_NOTIMPL: NTSTATUS = 0x8000_4001u32 as i32;

include!(concat!(env!("OUT_DIR"), "/shell32_export_stubs.rs"));

use core::ffi::c_void;

// These shell APIs are exported by Windows 11 shell32. Until CognacOS has the
// shell namespace, known-folder database, and PIDL allocator, fail explicitly
// instead of returning success with an invalid interface or uninitialized output.
#[no_mangle]
pub unsafe extern "system" fn SHCreateItemFromParsingName(
    _path:*const u16,_bind_context:*mut c_void,_iid:*const c_void,
    item:*mut *mut c_void,
)->i32{
    if !item.is_null(){unsafe{item.write(core::ptr::null_mut());}}
    HRESULT_E_NOTIMPL
}
#[no_mangle]
pub unsafe extern "system" fn SHGetDesktopFolder(folder:*mut *mut c_void)->i32{
    if !folder.is_null(){unsafe{folder.write(core::ptr::null_mut());}}
    HRESULT_E_NOTIMPL
}
#[no_mangle]
pub unsafe extern "system" fn SHGetFolderPathA(
    _window:*mut c_void,_csidl:i32,_token:*mut c_void,_flags:u32,path:*mut u8,
)->i32{
    if !path.is_null(){unsafe{path.write(0);}}
    HRESULT_E_NOTIMPL
}
#[no_mangle]
pub unsafe extern "system" fn SHGetFolderPathW(
    _window:*mut c_void,_csidl:i32,_token:*mut c_void,_flags:u32,path:*mut u16,
)->i32{
    if !path.is_null(){unsafe{path.write(0);}}
    HRESULT_E_NOTIMPL
}
#[no_mangle]
pub unsafe extern "system" fn SHGetKnownFolderPath(
    _folder_id:*const c_void,_flags:u32,_token:*mut c_void,path:*mut *mut u16,
)->i32{
    if !path.is_null(){unsafe{path.write(core::ptr::null_mut());}}
    HRESULT_E_NOTIMPL
}
#[no_mangle]
pub unsafe extern "system" fn SHParseDisplayName(
    _name:*const u16,_bind_context:*mut c_void,pidl:*mut *mut c_void,
    _attributes_in:u32,attributes_out:*mut u32,
)->i32{
    if !pidl.is_null(){unsafe{pidl.write(core::ptr::null_mut());}}
    if !attributes_out.is_null(){unsafe{attributes_out.write(0);}}
    HRESULT_E_NOTIMPL
}


#[cfg(target_os = "windows")]
core::arch::global_asm!(r#"
    .text
    .p2align 4
    .globl DllMain
DllMain:
    movl $1, %eax
    ret
"#, options(att_syntax));

#[cfg(target_os = "windows")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! { loop { core::hint::spin_loop(); } }
