//! Source-first Rust port of CognacOS's minimal native NTDLL loader helpers.
//!
//! Source correspondence: root/wine/dlls/ntdll/cognac_loader.c.
//! Supports lookup in already-mapped PE32+ images and modules already present
//! in the x64 PEB loader list. It does not load DLLs, resolve forwarders, walk
//! dependency graphs, or call Wine/Unix services. Build integration is deferred.
//!
//! # Safety
//! These exports consume raw pointers from native callers. The image base,
 //! PEB, loader list, Unicode string, and output pointers must refer to readable
//! or writable memory as appropriate. Bounds checks validate RVAs against the
//! PE SizeOfImage, but cannot prove that arbitrary process addresses are mapped.

#![allow(non_camel_case_types, non_snake_case)]

use core::ptr;

pub type UCHAR = u8;
pub type USHORT = u16;
pub type WCHAR = u16;
pub type ULONG = u32;
pub type LONG = i32;
pub type ULONG_PTR = usize;
pub type PVOID = *mut core::ffi::c_void;
pub type PULONG = *mut ULONG;

pub const STATUS_SUCCESS: LONG = 0;
pub const STATUS_INVALID_PARAMETER: LONG = 0xC000_000D_u32 as LONG;
pub const STATUS_INVALID_IMAGE_FORMAT: LONG = 0xC000_007B_u32 as LONG;
pub const STATUS_PROCEDURE_NOT_FOUND: LONG = 0xC000_007A_u32 as LONG;
pub const STATUS_DLL_NOT_FOUND: LONG = 0xC000_0135_u32 as LONG;

#[repr(C)]
pub struct COGNAC_UNICODE_STRING {
    pub Length: USHORT,
    pub MaximumLength: USHORT,
    pub Buffer: *mut WCHAR,
}

pub type PCCOGNAC_UNICODE_STRING = *const COGNAC_UNICODE_STRING;

#[repr(C)]
pub struct COGNAC_ANSI_STRING {
    pub Length: USHORT,
    pub MaximumLength: USHORT,
    pub Buffer: *mut u8,
}

pub type PCCOGNAC_ANSI_STRING = *const COGNAC_ANSI_STRING;

#[inline]
unsafe fn pe_u16(pointer: *const UCHAR) -> USHORT {
    // SAFETY: Caller guarantees the two bytes are readable.
    unsafe { u16::from_le_bytes([pointer.read(), pointer.add(1).read()]) }
}

#[inline]
unsafe fn pe_u32(pointer: *const UCHAR) -> ULONG {
    // SAFETY: Caller guarantees the four bytes are readable.
    unsafe {
        u32::from_le_bytes([
            pointer.read(),
            pointer.add(1).read(),
            pointer.add(2).read(),
            pointer.add(3).read(),
        ])
    }
}

#[inline]
unsafe fn pe_u64(pointer: *const UCHAR) -> ULONG_PTR {
    // SAFETY: Caller guarantees the eight bytes are readable.
    unsafe {
        u64::from_le_bytes([
            pointer.read(),
            pointer.add(1).read(),
            pointer.add(2).read(),
            pointer.add(3).read(),
            pointer.add(4).read(),
            pointer.add(5).read(),
            pointer.add(6).read(),
            pointer.add(7).read(),
        ]) as ULONG_PTR
    }
}

#[inline]
fn rva_range_ok(rva: ULONG, length: ULONG, image_size: ULONG) -> bool {
    rva <= image_size && length <= image_size - rva
}

#[inline]
fn loader_fold_char(value: WCHAR) -> WCHAR {
    if (b'A' as WCHAR..=b'Z' as WCHAR).contains(&value) {
        value + (b'a' as WCHAR - b'A' as WCHAR)
    } else {
        value
    }
}

#[inline]
unsafe fn current_peb() -> PVOID {
    let peb: PVOID;
    // SAFETY: Native x64 process startup must install a valid TEB at GS base.
    unsafe {
        core::arch::asm!(
            "mov {}, gs:[0x60]",
            out(reg) peb,
            options(nostack, preserves_flags, readonly)
        );
    }
    peb
}

/// Looks up an already-loaded DLL in the x64 PEB load-order list.
/// This does not load an image or resolve a search path.
#[no_mangle]
pub unsafe extern "system" fn LdrGetDllHandle(
    _dll_path: PVOID,
    _dll_characteristics: PULONG,
    dll_name: PCCOGNAC_UNICODE_STRING,
    dll_handle: *mut PVOID,
) -> LONG {
    if dll_name.is_null() || dll_handle.is_null() {
        return STATUS_INVALID_PARAMETER;
    }

    // SAFETY: Non-null pointers must reference valid descriptors/output storage.
    let name = unsafe { &*dll_name };
    if (name.Length & 1) != 0
        || (name.Length != 0 && name.Buffer.is_null())
        || name.Length > 512
    {
        return STATUS_INVALID_PARAMETER;
    }
    unsafe { dll_handle.write(ptr::null_mut()) };

    let peb = unsafe { current_peb() }.cast::<UCHAR>();
    if peb.is_null() {
        return STATUS_DLL_NOT_FOUND;
    }

    // x64 PEB.Ldr at +0x18; PEB_LDR_DATA.InLoadOrderModuleList at +0x10.
    let ldr = unsafe { pe_u64(peb.add(0x18)) } as *const UCHAR;
    if ldr.is_null() {
        return STATUS_DLL_NOT_FOUND;
    }
    let head = unsafe { ldr.add(0x10) };
    let head_address = head as ULONG_PTR;
    let mut link = unsafe { pe_u64(head) };
    let mut entries = 0u32;

    while link != 0 && link != head_address && entries < 64 {
        let entry = link as *const UCHAR;
        // LDR_DATA_TABLE_ENTRY.BaseDllName: UNICODE_STRING at +0x58,
        // buffer pointer at +0x60; DllBase is at +0x30.
        let name_length = unsafe { pe_u16(entry.add(0x58)) };
        let name_buffer = unsafe { pe_u64(entry.add(0x60)) } as *const WCHAR;
        let mut equal = true;

        if (name_length & 1) == 0
            && name_length == name.Length
            && (name_length == 0 || !name_buffer.is_null())
        {
            let count = (name_length / 2) as usize;
            for index in 0..count {
                let left = unsafe { name_buffer.add(index).read() };
                let right = unsafe { name.Buffer.add(index).read() };
                if loader_fold_char(left) != loader_fold_char(right) {
                    equal = false;
                    break;
                }
            }
            if equal {
                let base = unsafe { pe_u64(entry.add(0x30)) } as PVOID;
                if base.is_null() {
                    return STATUS_DLL_NOT_FOUND;
                }
                unsafe { dll_handle.write(base) };
                return STATUS_SUCCESS;
            }
        }

        link = unsafe { pe_u64(entry) };
        entries += 1;
    }

    STATUS_DLL_NOT_FOUND
}

/// Resolves an exported name or ordinal in an already-mapped x64 PE image.
/// Forwarded exports are rejected rather than returned as executable addresses.
#[no_mangle]
pub unsafe extern "system" fn LdrGetProcedureAddress(
    dll_handle: PVOID,
    procedure_name: PCCOGNAC_ANSI_STRING,
    procedure_number: ULONG,
    procedure_address: *mut PVOID,
) -> LONG {
    if dll_handle.is_null() || procedure_address.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    unsafe { procedure_address.write(ptr::null_mut()) };
    let base = dll_handle.cast::<UCHAR>();

    if unsafe { pe_u16(base) } != 0x5A4D {
        return STATUS_INVALID_IMAGE_FORMAT;
    }
    let pe_offset = unsafe { pe_u32(base.add(0x3c)) };
    if pe_offset > 0x100000 {
        return STATUS_INVALID_IMAGE_FORMAT;
    }
    if unsafe { pe_u32(base.add(pe_offset as usize)) } != 0x0000_4550 {
        return STATUS_INVALID_IMAGE_FORMAT;
    }

    let coff = unsafe { base.add(pe_offset as usize + 4) };
    if unsafe { pe_u16(coff) } != 0x8664 {
        return STATUS_INVALID_IMAGE_FORMAT;
    }
    let optional_size = unsafe { pe_u16(coff.add(16)) } as ULONG;
    let optional = unsafe { coff.add(20) };
    if optional_size < 120 || unsafe { pe_u16(optional) } != 0x020B {
        return STATUS_INVALID_IMAGE_FORMAT;
    }

    let image_size = unsafe { pe_u32(optional.add(56)) };
    if image_size < pe_offset.saturating_add(24).saturating_add(optional_size) {
        return STATUS_INVALID_IMAGE_FORMAT;
    }
    if unsafe { pe_u32(optional.add(108)) } < 1 {
        return STATUS_PROCEDURE_NOT_FOUND;
    }

    let export_rva = unsafe { pe_u32(optional.add(112)) };
    let export_size = unsafe { pe_u32(optional.add(116)) };
    if export_rva == 0 || export_size < 40 || !rva_range_ok(export_rva, export_size, image_size) {
        return STATUS_PROCEDURE_NOT_FOUND;
    }

    let exports = unsafe { base.add(export_rva as usize) };
    let ordinal_base = unsafe { pe_u32(exports.add(16)) };
    let function_count = unsafe { pe_u32(exports.add(20)) };
    let name_count = unsafe { pe_u32(exports.add(24)) };
    let functions_rva = unsafe { pe_u32(exports.add(28)) };
    let names_rva = unsafe { pe_u32(exports.add(32)) };
    let ordinals_rva = unsafe { pe_u32(exports.add(36)) };

    if function_count == 0
        || function_count > image_size / 4
        || name_count > image_size / 4
        || !rva_range_ok(functions_rva, function_count.saturating_mul(4), image_size)
        || !rva_range_ok(names_rva, name_count.saturating_mul(4), image_size)
        || !rva_range_ok(ordinals_rva, name_count.saturating_mul(2), image_size)
    {
        return STATUS_INVALID_IMAGE_FORMAT;
    }

    let functions = functions_rva as usize;
    let names = names_rva as usize;
    let ordinals = ordinals_rva as usize;
    let mut function_index = ULONG::MAX;

    if !procedure_name.is_null() {
        let name = unsafe { &*procedure_name };
        if name.Length != 0 && name.Buffer.is_null() {
            return STATUS_INVALID_PARAMETER;
        }

        for index in 0..name_count {
            let name_rva = unsafe { pe_u32(base.add(names + index as usize * 4)) };
            if name_rva >= image_size {
                return STATUS_INVALID_IMAGE_FORMAT;
            }
            let mut length = 0u32;
            while length < image_size - name_rva
                && unsafe { base.add(name_rva as usize + length as usize).read() } != 0
            {
                length += 1;
            }
            if length == image_size - name_rva {
                return STATUS_INVALID_IMAGE_FORMAT;
            }
            if length != name.Length as ULONG {
                continue;
            }

            let mut character = 0u32;
            while character < length {
                let exported = unsafe {
                    base.add(name_rva as usize + character as usize).read()
                };
                let requested = unsafe {
                    name.Buffer.add(character as usize).read()
                };
                if exported != requested {
                    break;
                }
                character += 1;
            }
            if character == length {
                function_index = unsafe {
                    pe_u16(base.add(ordinals + index as usize * 2))
                } as ULONG;
                break;
            }
        }
    } else {
        if procedure_number < ordinal_base {
            return STATUS_PROCEDURE_NOT_FOUND;
        }
        function_index = procedure_number - ordinal_base;
    }

    if function_index >= function_count {
        return STATUS_PROCEDURE_NOT_FOUND;
    }
    let function_rva = unsafe {
        pe_u32(base.add(functions + function_index as usize * 4))
    };
    if function_rva == 0 || function_rva >= image_size {
        return STATUS_PROCEDURE_NOT_FOUND;
    }
    // Export directory RVAs in this range are forwarder strings, not code.
    if function_rva >= export_rva && function_rva - export_rva < export_size {
        return STATUS_PROCEDURE_NOT_FOUND;
    }

    unsafe { procedure_address.write(base.add(function_rva as usize).cast()) };
    STATUS_SUCCESS
}
