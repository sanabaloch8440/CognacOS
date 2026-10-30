//! CognacOS bcrypt.dll: initial CNG hashing and random-number subset.
//! All unimplemented Windows 11 exports return STATUS_NOT_IMPLEMENTED.
#![allow(non_snake_case)]
#![deny(unsafe_op_in_unsafe_fn)]

use aws_lc_rs::{digest, rand};
use core::{ffi::c_void, ptr};
use std::{boxed::Box, vec::Vec};

pub type NTSTATUS = i32;
pub type ULONG = u32;
pub type PUCHAR = *mut u8;
p type_dummy
pub const STATUS_SUCCESS: NTSTATUS = 0;
pub const STATUS_INVALID_PARAMETER: NTSTATUS = 0xC000000D_u32 as i32;
pub const STATUS_NOT_SUPPORTED: NTSTATUS = 0xC00000BB_u32 as i32;
pub const STATUS_NOT_IMPLEMENTED: NTSTATUS = 0xC0000002_u32 as i32;
pub const STATUS_BUFFER_TOO_SMALL: NTSTATUS = 0xC0000023_u32 as i32;
pub const STATUS_NO_MEMORY: NTSTATUS = 0xC0000017_u32 as i32;
const BCRYPT_USE_SYSTEM_PREFERRED_RNG: ULONG = 0x00000002;
const BCRYPT_ALG_HANDLE_HMAC_FLAG: ULONG = 0x00000008;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HashAlgorithm { Sha256, Sha384, Sha512 }
impl HashAlgorithm {
    fn from_utf16(ptr: *const u16) -> Result<Self, NTSTATUS> {
        if ptr.is_null() { return Err(STATUS_INVALID_PARAMETER); }
        // Read a bounded, NUL-terminated Windows wide string.
        let mut name = Vec::new();
        for i in 0..64 {
            let unit = unsafe { ptr.add(i).read() };
            if unit == 0 { break; }
            name.push(unit);
            if i == 63 { return Err(STATUS_INVALID_PARAMETER); }
        }
        match String::from_utf16(&name).map_err(|_| STATUS_INVALID_PARAMETER)?.as_str() {
            "SHA256" => Ok(Self::Sha256), "SHA384" => Ok(Self::Sha384), "SHA512" => Ok(Self::Sha512),
            _ => Err(STATUS_NOT_SUPPORTED),
        }
    }
    fn provider_name(self) -> &'static str { match self { Self::Sha256 => "SHA256", Self::Sha384 => "SHA384", Self::Sha512 => "SHA512" } }
    fn digest(self, bytes: &[u8]) -> digest::Digest { match self { Self::Sha256 => digest::digest(&digest::SHA256, bytes), Self::Sha384 => digest::digest(&digest::SHA384, bytes), Self::Sha512 => digest::digest(&digest::SHA512, bytes) } }
    fn digest_len(self) -> u32 { match self { Self::Sha256 => 32, Self::Sha384 => 48, Self::Sha512 => 64 } }
    fn block_len(self) -> u32 { 128 }
}

struct AlgorithmHandle { algorithm: HashAlgorithm }
struct HashHandle { algorithm: HashAlgorithm, data: Vec<u8> }

unsafe fn write_u32(out: *mut u8, value: u32) { unsafe { ptr::copy_nonoverlapping(value.to_ne_bytes().as_ptr(), out, 4) }; }
unsafe fn write_wide(out: *mut u8, value: &str) { for (i, unit) in value.encode_utf16().chain(core::iter::once(0)).enumerate() { unsafe { ptr::write_unaligned(out.add(i * 2).cast::<u16>(), unit) }; } }
unsafe fn read_property_name(ptr: *const u16) -> Result<String, NTSTATUS> {
    if ptr.is_null() { return Err(STATUS_INVALID_PARAMETER); }
    let mut units = Vec::new();
    for i in 0..128 { let u = unsafe { ptr.add(i).read() }; if u == 0 { return String::from_utf16(&units).map_err(|_| STATUS_INVALID_PARAMETER); } units.push(u); }
    Err(STATUS_INVALID_PARAMETER)
}

/// BCryptOpenAlgorithmProvider (SHA-256/384/512 only; no HMAC flag yet).
#[no_mangle]
pub unsafe extern "system" fn BCryptOpenAlgorithmProvider(out: *mut *mut c_void, alg_id: *const u16, implementation: *const u16, flags: ULONG) -> NTSTATUS {
    if out.is_null() || !implementation.is_null() || flags & !BCRYPT_ALG_HANDLE_HMAC_FLAG != 0 || flags & BCRYPT_ALG_HANDLE_HMAC_FLAG != 0 { return STATUS_INVALID_PARAMETER; }
    let algorithm = match HashAlgorithm::from_utf16(alg_id) { Ok(a) => a, Err(s) => return s };
    let handle = Box::new(AlgorithmHandle { algorithm });
    unsafe { out.write(Box::into_raw(handle).cast()) };
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn BCryptCloseAlgorithmProvider(handle: *mut c_void, flags: ULONG) -> NTSTATUS {
    if handle.is_null() || flags != 0 { return STATUS_INVALID_PARAMETER; }
    unsafe { drop(Box::from_raw(handle.cast::<AlgorithmHandle>())) };
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn BCryptCreateHash(algorithm: *mut c_void, out: *mut *mut c_void, object: *mut u8, object_len: ULONG, secret: *mut u8, secret_len: ULONG, flags: ULONG) -> NTSTATUS {
    if algorithm.is_null() || out.is_null() || !object.is_null() || object_len != 0 || !secret.is_null() || secret_len != 0 || flags != 0 { return STATUS_INVALID_PARAMETER; }
    let alg = unsafe { &*algorithm.cast::<AlgorithmHandle>() }.algorithm;
    unsafe { out.write(Box::into_raw(Box::new(HashHandle { algorithm: alg, data: Vec::new() })).cast()) };
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn BCryptHashData(hash: *mut c_void, input: *const u8, input_len: ULONG, flags: ULONG) -> NTSTATUS {
    if hash.is_null() || (input.is_null() && input_len != 0) || flags != 0 { return STATUS_INVALID_PARAMETER; }
    let state = unsafe { &mut *hash.cast::<HashHandle>() };
    if input_len != 0 { state.data.extend_from_slice(unsafe { core::slice::from_raw_parts(input, input_len as usize) }); }
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn BCryptFinishHash(hash: *mut c_void, output: *mut u8, output_len: ULONG, flags: ULONG) -> NTSTATUS {
    if hash.is_null() || output.is_null() || flags != 0 { return STATUS_INVALID_PARAMETER; }
    let state = unsafe { &*hash.cast::<HashHandle>() };
    if output_len < state.algorithm.digest_len() { return STATUS_BUFFER_TOO_SMALL; }
    let result = state.algorithm.digest(&state.data);
    unsafe { ptr::copy_nonoverlapping(result.as_ref().as_ptr(), output, result.as_ref().len()) };
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn BCryptDestroyHash(hash: *mut c_void) -> NTSTATUS {
    if hash.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe { drop(Box::from_raw(hash.cast::<HashHandle>())) };
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn BCryptGenRandom(algorithm: *mut c_void, buffer: *mut u8, length: ULONG, flags: ULONG) -> NTSTATUS {
    if buffer.is_null() && length != 0 { return STATUS_INVALID_PARAMETER; }
    if !algorithm.is_null() || flags != BCRYPT_USE_SYSTEM_PREFERRED_RNG { return STATUS_INVALID_PARAMETER; }
    if length == 0 { return STATUS_SUCCESS; }
    match rand::fill(unsafe { core::slice::from_raw_parts_mut(buffer, length as usize) }) { Ok(()) => STATUS_SUCCESS, Err(_) => STATUS_NOT_SUPPORTED }
}

#[no_mangle]
pub unsafe extern "system" fn BCryptGetProperty(object: *mut c_void, property: *const u16, output: *mut u8, output_len: ULONG, result_len: *mut ULONG, flags: ULONG) -> NTSTATUS {
    if object.is_null() || property.is_null() || result_len.is_null() || flags != 0 { return STATUS_INVALID_PARAMETER; }
    let name = match unsafe { read_property_name(property) } { Ok(n) => n, Err(s) => return s };
    let alg = unsafe { &*object.cast::<AlgorithmHandle>() }.algorithm;
    let (kind, value) = match name.as_str() {
        "ObjectLength" => (0, 0), "HashDigestLength" => (0, alg.digest_len()), "HashBlockLength" => (0, alg.block_len()),
        "AlgorithmName" => (1, 0), "ProviderName" => (1, 0), _ => return STATUS_NOT_SUPPORTED,
    };
    let required = if kind == 0 { 4 } else { (alg.provider_name().encode_utf16().count() as u32 + 1) * 2 };
    unsafe { result_len.write(required) };
    if output.is_null() || output_len < required { return STATUS_BUFFER_TOO_SMALL; }
    if kind == 0 { unsafe { write_u32(output, value) }; }
    else { unsafe { write_wide(output, if name == "AlgorithmName" { alg.provider_name() } else { "CognacOS CNG" }) }; }
    STATUS_SUCCESS
}

include!(concat!(env!("OUT_DIR"), "/bcrypt_unsupported_exports.rs"));

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sha256_known_answer() {
        let got = HashAlgorithm::Sha256.digest(b"abc");
        assert_eq!(got.as_ref(), &[0xba,0x78,0x16,0xbf,0x8f,0x01,0xcf,0xea,0x41,0x41,0x40,0xde,0x5d,0xae,0x22,0x23,0xb0,0x03,0x61,0xa3,0x96,0x17,0x7a,0x9c,0xb4,0x10,0xff,0x61,0xf2,0x00,0x15,0xad]);
    }
    #[test]
    fn supported_algorithms_have_expected_sizes() { assert_eq!(HashAlgorithm::Sha256.digest_len(),32); assert_eq!(HashAlgorithm::Sha384.digest_len(),48); assert_eq!(HashAlgorithm::Sha512.digest_len(),64); }
}
