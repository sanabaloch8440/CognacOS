//! CM/IO memory resource descriptor encoding and decoding.
#![allow(non_snake_case, clippy::missing_safety_doc)]
use core::{ffi::c_void, ptr};
use super::virtual_memory::{PVOID, NTSTATUS};
const STATUS_SUCCESS: NTSTATUS = 0;
const STATUS_INVALID_PARAMETER: NTSTATUS = 0xC000_000Du32 as i32;
pub(super) const STATUS_UNSUCCESSFUL: NTSTATUS = 0xC000_0001u32 as i32;

/// Windows CM_PARTIAL_RESOURCE_DESCRIPTOR: 4-byte header and 12-byte union.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CM_PARTIAL_RESOURCE_DESCRIPTOR { pub type_: u8, pub share_disposition: u8, pub flags: u16, pub data: [u8; 12] }
const CM_RESOURCE_TYPE_PORT: u8 = 3;
const CM_RESOURCE_TYPE_MEMORY: u8 = 4;
const CM_RESOURCE_TYPE_MEMORY_LARGE: u8 = 7;
const CM_RESOURCE_MEMORY_LARGE_40: u16 = 0x0200;
const CM_RESOURCE_MEMORY_LARGE_48: u16 = 0x0400;
const CM_RESOURCE_MEMORY_LARGE_64: u16 = 0x0800;
unsafe fn decode_mem_io_resource(resource: *const CM_PARTIAL_RESOURCE_DESCRIPTOR, start: *mut u64) -> u64 {
    if resource.is_null() { return 0; }
    let descriptor = unsafe { ptr::read_unaligned(resource) };
    if !matches!(descriptor.type_, CM_RESOURCE_TYPE_PORT | CM_RESOURCE_TYPE_MEMORY | CM_RESOURCE_TYPE_MEMORY_LARGE) { return 0; }
    let address = u64::from_le_bytes(descriptor.data[0..8].try_into().unwrap_or([0; 8]));
    let raw_length = u32::from_le_bytes(descriptor.data[8..12].try_into().unwrap_or([0; 4])) as u64;
    let length = if descriptor.type_ == CM_RESOURCE_TYPE_MEMORY_LARGE {
        if !matches!(descriptor.flags & 0x0e00, CM_RESOURCE_MEMORY_LARGE_40 | CM_RESOURCE_MEMORY_LARGE_48 | CM_RESOURCE_MEMORY_LARGE_64) { return 0; }
        raw_length
    } else { raw_length };
    if length == 0 || address.checked_add(length).is_none() { return 0; }
    if !start.is_null() { unsafe { ptr::write_unaligned(start, address); } }
    length
}
unsafe fn encode_mem_io_resource(resource: *mut CM_PARTIAL_RESOURCE_DESCRIPTOR, type_: u8, length: u64, start: u64) -> NTSTATUS {
    if resource.is_null() || length == 0 || start.checked_add(length).is_none() { return STATUS_INVALID_PARAMETER; }
    let flags = match type_ {
        CM_RESOURCE_TYPE_PORT | CM_RESOURCE_TYPE_MEMORY => 0u16,
        CM_RESOURCE_TYPE_MEMORY_LARGE if start & 0xff == 0 && start < (1u64 << 40) => CM_RESOURCE_MEMORY_LARGE_40,
        CM_RESOURCE_TYPE_MEMORY_LARGE if start & 0xffff == 0 && start < (1u64 << 48) => CM_RESOURCE_MEMORY_LARGE_48,
        CM_RESOURCE_TYPE_MEMORY_LARGE if start & 0xffff_ffff == 0 => CM_RESOURCE_MEMORY_LARGE_64,
        CM_RESOURCE_TYPE_MEMORY_LARGE => return STATUS_UNSUCCESSFUL,
        _ => return STATUS_INVALID_PARAMETER,
    };
    if length > u32::MAX as u64 { return STATUS_UNSUCCESSFUL; }
    if type_ == CM_RESOURCE_TYPE_PORT && start > u32::MAX as u64 { return STATUS_UNSUCCESSFUL; }
    let encoded_length = length as u32;
    let mut descriptor = CM_PARTIAL_RESOURCE_DESCRIPTOR { type_: type_, share_disposition: 0, flags, data: [0; 12] };
    descriptor.data[..8].copy_from_slice(&start.to_le_bytes());
    descriptor.data[8..12].copy_from_slice(&encoded_length.to_le_bytes());
    unsafe { ptr::write_unaligned(resource, descriptor); }
    STATUS_SUCCESS
}
#[no_mangle]
pub unsafe extern "system" fn RtlCmDecodeMemIoResource(resource: PVOID, start: *mut u64) -> u64 {
    unsafe { decode_mem_io_resource(resource.cast(), start) }
}
#[no_mangle]
pub unsafe extern "system" fn RtlCmEncodeMemIoResource(resource: PVOID, type_: u8, length: u64, start: u64) -> NTSTATUS {
    unsafe { encode_mem_io_resource(resource.cast(), type_, length, start) }
}
#[no_mangle]
pub unsafe extern "system" fn RtlIoDecodeMemIoResource(resource: PVOID, start: *mut u64) -> u64 {
    unsafe { decode_mem_io_resource(resource.cast(), start) }
}
#[no_mangle]
pub unsafe extern "system" fn RtlIoEncodeMemIoResource(resource: PVOID, type_: u8, length: u64, start: u64) -> NTSTATUS {
    unsafe { encode_mem_io_resource(resource.cast(), type_, length, start) }
}
