//! CognacOS bcrypt.dll: initial CNG hashing and random-number subset.
//! All unimplemented Windows 11 exports return STATUS_NOT_IMPLEMENTED.
#![allow(non_snake_case)]
#![deny(unsafe_op_in_unsafe_fn)]

use aws_lc_rs::{aead, digest, hmac, pbkdf2, rand};
use rsa::{pkcs1v15::{Signature as RsaSignature, VerifyingKey}, signature::hazmat::PrehashVerifier, traits::PublicKeyParts, BigUint, RsaPublicKey};
use sha2::Sha256;
use aes::{Aes128, Aes192, Aes256};
use aes_gcm::{aead::AeadInPlace, AesGcm};
use aes_gcm::aead::generic_array::typenum::{U12, U16};
use cipher::{BlockDecrypt, BlockEncrypt, KeyInit, generic_array::GenericArray};
use core::num::NonZeroU32;
use zeroize::Zeroize;
use core::{ffi::c_void, ptr};
use std::{boxed::Box, collections::HashMap, sync::{Mutex, OnceLock}, vec::Vec};
use std::sync::atomic::{AtomicUsize, Ordering};

pub type NTSTATUS = i32;
pub type ULONG = u32;
pub const STATUS_SUCCESS: NTSTATUS = 0;
pub const STATUS_INVALID_PARAMETER: NTSTATUS = 0xC000000D_u32 as i32;
pub const STATUS_NOT_SUPPORTED: NTSTATUS = 0xC00000BB_u32 as i32;
pub const STATUS_NOT_IMPLEMENTED: NTSTATUS = 0xC0000002_u32 as i32;
pub const STATUS_BUFFER_TOO_SMALL: NTSTATUS = 0xC0000023_u32 as i32;
pub const STATUS_NO_MEMORY: NTSTATUS = 0xC0000017_u32 as i32;
pub const STATUS_AUTH_TAG_MISMATCH: NTSTATUS = 0xC000A002_u32 as i32;
pub const STATUS_INVALID_SIGNATURE: NTSTATUS = 0xC000A000_u32 as i32;
const BCRYPT_USE_SYSTEM_PREFERRED_RNG: ULONG = 0x00000002;
const BCRYPT_ALG_HANDLE_HMAC_FLAG: ULONG = 0x00000008;
const BCRYPT_MULTI_FLAG: ULONG = 0x00000040;
const BCRYPT_BLOCK_PADDING: ULONG = 0x00000001;

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
    fn hmac_algorithm(self) -> hmac::Algorithm { match self { Self::Sha256 => hmac::HMAC_SHA256, Self::Sha384 => hmac::HMAC_SHA384, Self::Sha512 => hmac::HMAC_SHA512 } }
    fn pbkdf2_algorithm(self) -> pbkdf2::Algorithm { match self { Self::Sha256 => pbkdf2::PBKDF2_HMAC_SHA256, Self::Sha384 => pbkdf2::PBKDF2_HMAC_SHA384, Self::Sha512 => pbkdf2::PBKDF2_HMAC_SHA512 } }
    fn provider_name(self) -> &'static str { match self { Self::Sha256 => "SHA256", Self::Sha384 => "SHA384", Self::Sha512 => "SHA512" } }
    fn digest(self, bytes: &[u8]) -> digest::Digest { match self { Self::Sha256 => digest::digest(&digest::SHA256, bytes), Self::Sha384 => digest::digest(&digest::SHA384, bytes), Self::Sha512 => digest::digest(&digest::SHA512, bytes) } }
    fn digest_len(self) -> u32 { match self { Self::Sha256 => 32, Self::Sha384 => 48, Self::Sha512 => 64 } }
    fn block_len(self) -> u32 { match self { Self::Sha256 => 64, Self::Sha384 | Self::Sha512 => 128 } }
}

#[derive(Clone)]
struct AlgorithmHandle { algorithm: HashAlgorithm, hmac: bool, multi: bool, cipher: bool, kdf: bool, rsa: bool, rng: bool, gcm: bool, cbc: bool }
#[derive(Clone)]
struct KeyHandle { key: Vec<u8>, gcm: bool, cbc: bool }
#[derive(Clone)]
struct KdfKeyHandle { secret: Vec<u8> }
impl Drop for KdfKeyHandle { fn drop(&mut self) { self.secret.zeroize(); } }
impl Drop for KeyHandle { fn drop(&mut self) { self.key.zeroize(); } }
#[derive(Clone)]
enum RegisteredHandle { Algorithm(AlgorithmHandle), Hash(HashHandle), MultiHash(MultiHashHandle), Key(KeyHandle), KdfKey(KdfKeyHandle), RsaPublic(RsaPublicKey) }
static HANDLE_TABLE: OnceLock<Mutex<HashMap<usize, RegisteredHandle>>> = OnceLock::new();
static NEXT_HANDLE: AtomicUsize = AtomicUsize::new(0x10000);
fn handle_table() -> &'static Mutex<HashMap<usize, RegisteredHandle>> {
    HANDLE_TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}
fn register_handle(value: RegisteredHandle) -> Result<*mut c_void, NTSTATUS> {
    let token = NEXT_HANDLE.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| current.checked_add(16)).map_err(|_| STATUS_NO_MEMORY)?;
    if token == 0 { return Err(STATUS_NO_MEMORY); }
    handle_table().lock().map_err(|_| STATUS_NO_MEMORY)?.insert(token, value);
    Ok(token as *mut c_void)
}
fn algorithm_handle(token: *mut c_void) -> Result<AlgorithmHandle, NTSTATUS> {
    if token.is_null() { return Err(STATUS_INVALID_PARAMETER); }
    match handle_table().lock().map_err(|_| STATUS_NO_MEMORY)?.get(&(token as usize)) {
        Some(RegisteredHandle::Algorithm(v)) => Ok(v.clone()),
        _ => Err(STATUS_INVALID_PARAMETER),
    }
}
fn hash_handle(token: *mut c_void) -> Result<HashHandle, NTSTATUS> {
    if token.is_null() { return Err(STATUS_INVALID_PARAMETER); }
    match handle_table().lock().map_err(|_| STATUS_NO_MEMORY)?.get(&(token as usize)) {
        Some(RegisteredHandle::Hash(v)) => Ok(v.clone()),
        _ => Err(STATUS_INVALID_PARAMETER),
    }
}
fn remove_handle(token: *mut c_void, expected: u8) -> NTSTATUS {
    if token.is_null() { return STATUS_INVALID_PARAMETER; }
    let Ok(mut table) = handle_table().lock() else { return STATUS_INVALID_PARAMETER; };
    let key = token as usize;
    let valid = matches!((table.get(&key), expected),
        (Some(RegisteredHandle::Algorithm(_)), 0) |
        (Some(RegisteredHandle::Hash(_)), 1) |
        (Some(RegisteredHandle::MultiHash(_)), 1) |
        (Some(RegisteredHandle::Key(_)), 2) | (Some(RegisteredHandle::RsaPublic(_)), 2) | (Some(RegisteredHandle::KdfKey(_)), 3));
    if !valid { return STATUS_INVALID_PARAMETER; }
    table.remove(&key);
    STATUS_SUCCESS
}
fn key_handle(token: *mut c_void) -> Result<KeyHandle, NTSTATUS> {
    if token.is_null() { return Err(STATUS_INVALID_PARAMETER); }
    match handle_table().lock().map_err(|_| STATUS_NO_MEMORY)?.get(&(token as usize)) {
        Some(RegisteredHandle::Key(v)) => Ok(v.clone()),
        _ => Err(STATUS_INVALID_PARAMETER),
    }
}

#[derive(Clone)]
struct HashHandle { algorithm: HashAlgorithm, hmac: bool, secret: Vec<u8>, data: Vec<u8> }
#[derive(Clone)]
struct MultiHashHandle { states: Vec<HashHandle> }
impl Drop for HashHandle { fn drop(&mut self) { self.secret.zeroize(); self.data.zeroize(); } }

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
    if out.is_null() || flags & !(BCRYPT_ALG_HANDLE_HMAC_FLAG | BCRYPT_MULTI_FLAG) != 0 { return STATUS_INVALID_PARAMETER; }
    if alg_id.is_null() { return STATUS_INVALID_PARAMETER; }
    let mut units=Vec::new();
    for i in 0..64 { let u=unsafe{alg_id.add(i).read()}; if u==0 {break;} units.push(u); if i==63{return STATUS_INVALID_PARAMETER;} }
    let name=match String::from_utf16(&units){Ok(v)=>v,Err(_)=>return STATUS_INVALID_PARAMETER};
    if !implementation.is_null() {
        let mut provider=Vec::new(); for i in 0..64 { let u=unsafe{implementation.add(i).read()}; if u==0 { break; } provider.push(u); if i==63{return STATUS_INVALID_PARAMETER;} }
        match String::from_utf16(&provider).ok().as_deref() { Some("Microsoft Primitive Provider")=>{}, _=>return 0xC0000225_u32 as i32 }
    }
    let (algorithm,cipher,kdf,rsa_provider,rng_provider)=if name=="AES" { if flags!=0{return STATUS_INVALID_PARAMETER;} (HashAlgorithm::Sha256,true,false,false,false) }
    else if name=="PBKDF2" { if flags!=0{return STATUS_INVALID_PARAMETER;} (HashAlgorithm::Sha256,false,true,false,false) }
    else if name=="RSA" { if flags!=0{return STATUS_INVALID_PARAMETER;} (HashAlgorithm::Sha256,false,false,true,false) }
    else if name=="RNG" { if flags!=0{return STATUS_INVALID_PARAMETER;} (HashAlgorithm::Sha256,false,false,false,true) }
    else { match HashAlgorithm::from_utf16(alg_id) { Ok(a)=>(a,false,false,false,false),Err(e)=>return e } };
    let token=match register_handle(RegisteredHandle::Algorithm(AlgorithmHandle{algorithm,hmac:flags&BCRYPT_ALG_HANDLE_HMAC_FLAG!=0,multi:flags&BCRYPT_MULTI_FLAG!=0,cipher,kdf,rsa:rsa_provider,rng:rng_provider,gcm:false,cbc:false})) {Ok(v)=>v,Err(e)=>return e};
    unsafe{out.write(token)}; STATUS_SUCCESS
}


#[no_mangle]
pub unsafe extern "system" fn BCryptCloseAlgorithmProvider(handle:*mut c_void,flags:ULONG)->NTSTATUS {
 if flags!=0{return STATUS_INVALID_PARAMETER;} remove_handle(handle,0)
}


#[no_mangle]
pub unsafe extern "system" fn BCryptCreateHash(algorithm:*mut c_void,out:*mut *mut c_void,object:*mut u8,object_len:ULONG,secret:*mut u8,secret_len:ULONG,flags:ULONG)->NTSTATUS {
 if out.is_null()||!object.is_null()||object_len!=0||(secret.is_null()&&secret_len!=0)||flags!=0{return STATUS_INVALID_PARAMETER;}
 let p=match algorithm_handle(algorithm){Ok(v)=>v,Err(e)=>return e}; if p.cipher{return STATUS_INVALID_PARAMETER;} if !p.hmac&&secret_len!=0{return STATUS_INVALID_PARAMETER;}
 let key=if secret_len==0{Vec::new()}else{unsafe{core::slice::from_raw_parts(secret,secret_len as usize)}.to_vec()};
 let h=HashHandle{algorithm:p.algorithm,hmac:p.hmac,secret:key,data:Vec::new()};
 let token=match register_handle(RegisteredHandle::Hash(h)){Ok(v)=>v,Err(e)=>return e}; unsafe{out.write(token)};STATUS_SUCCESS
}


#[no_mangle]
pub unsafe extern "system" fn BCryptHashData(hash:*mut c_void,input:*const u8,input_len:ULONG,flags:ULONG)->NTSTATUS {
 if (input.is_null()&&input_len!=0)||flags!=0{return STATUS_INVALID_PARAMETER;}
 let Ok(mut table)=handle_table().lock() else{return STATUS_NO_MEMORY;};
 let Some(RegisteredHandle::Hash(state))=table.get_mut(&(hash as usize)) else{return STATUS_INVALID_PARAMETER;};
 if state.data.len().saturating_add(input_len as usize)>64*1024*1024{return STATUS_INVALID_PARAMETER;}
 if input_len!=0 {if state.data.try_reserve(input_len as usize).is_err(){return STATUS_NO_MEMORY;} state.data.extend_from_slice(unsafe{core::slice::from_raw_parts(input,input_len as usize)});}
 STATUS_SUCCESS
}


#[no_mangle]
pub unsafe extern "system" fn BCryptFinishHash(hash:*mut c_void,output:*mut u8,output_len:ULONG,flags:ULONG)->NTSTATUS {
 if output.is_null()||flags!=0{return STATUS_INVALID_PARAMETER;} let state=match hash_handle(hash){Ok(v)=>v,Err(e)=>return e};
 if state.hmac {if output_len<state.algorithm.digest_len(){return STATUS_BUFFER_TOO_SMALL;} let k=hmac::Key::new(state.algorithm.hmac_algorithm(),&state.secret);let d=hmac::sign(&k,&state.data);unsafe{ptr::copy_nonoverlapping(d.as_ref().as_ptr(),output,d.as_ref().len())};}
 else {if output_len<state.algorithm.digest_len(){return STATUS_BUFFER_TOO_SMALL;}let d=state.algorithm.digest(&state.data);unsafe{ptr::copy_nonoverlapping(d.as_ref().as_ptr(),output,d.as_ref().len())};} STATUS_SUCCESS
}


#[no_mangle]
pub unsafe extern "system" fn BCryptDuplicateHash(hash:*mut c_void,out:*mut *mut c_void,object:*mut u8,object_len:ULONG,flags:ULONG)->NTSTATUS {
 if out.is_null()||!object.is_null()||object_len!=0||flags!=0{return STATUS_INVALID_PARAMETER;}let state=match hash_handle(hash){Ok(v)=>v,Err(e)=>return e};
 let token=match register_handle(RegisteredHandle::Hash(state)){Ok(v)=>v,Err(e)=>return e};unsafe{out.write(token)};STATUS_SUCCESS
}



#[repr(C)]
#[derive(Clone, Copy)]
struct BcryptMultiHashOperation { iHash: ULONG, hashOperation: ULONG, pbBuffer: *mut u8, cbBuffer: ULONG }

/// Creates a bounded multi-hash object. Caller-owned object buffers are not
/// accepted yet; the state is held in the opaque handle table.
#[no_mangle]
pub unsafe extern "system" fn BCryptCreateMultiHash(algorithm:*mut c_void,out:*mut *mut c_void,count:ULONG,object:*mut u8,object_len:ULONG,secret:*mut u8,secret_len:ULONG,flags:ULONG)->NTSTATUS {
    if out.is_null() || count==0 || count>64 || !object.is_null() || object_len!=0 || (secret.is_null()&&secret_len!=0) || flags!=0 { return STATUS_INVALID_PARAMETER; }
    let provider=match algorithm_handle(algorithm){Ok(v)=>v,Err(e)=>return e};
    if provider.cipher || !provider.multi || (!provider.hmac&&secret_len!=0) { return STATUS_INVALID_PARAMETER; }
    let key=if secret_len==0 { Vec::new() } else { unsafe{core::slice::from_raw_parts(secret,secret_len as usize)}.to_vec() };
    let mut states=Vec::new();
    if states.try_reserve_exact(count as usize).is_err(){return STATUS_NO_MEMORY;}
    for _ in 0..count { states.push(HashHandle{algorithm:provider.algorithm,hmac:provider.hmac,secret:key.clone(),data:Vec::new()}); }
    let token=match register_handle(RegisteredHandle::MultiHash(MultiHashHandle{states})){Ok(v)=>v,Err(e)=>return e};
    unsafe{out.write(token)};STATUS_SUCCESS
}

/// Processes BCRYPT_MULTI_HASH_OPERATION records in caller order. All records
/// are structurally validated before any state is changed.
#[no_mangle]
pub unsafe extern "system" fn BCryptProcessMultiOperations(object:*mut c_void,operation_type:ULONG,operations:*mut BcryptMultiHashOperation,operations_len:ULONG,flags:ULONG)->NTSTATUS {
    const OP_HASH_DATA:ULONG=1;const OP_FINISH_HASH:ULONG=2;const TYPE_HASH:ULONG=1;
    if operation_type!=TYPE_HASH || flags!=0 || (operations.is_null()&&operations_len!=0) || operations_len as usize % core::mem::size_of::<BcryptMultiHashOperation>() != 0 { return STATUS_INVALID_PARAMETER; }
    let count=operations_len as usize/core::mem::size_of::<BcryptMultiHashOperation>();
    if count>4096{return STATUS_INVALID_PARAMETER;}
    let mut table=match handle_table().lock(){Ok(v)=>v,Err(_)=>return STATUS_NO_MEMORY};
    let Some(RegisteredHandle::MultiHash(state))=table.get_mut(&(object as usize)) else{return STATUS_INVALID_PARAMETER;};
    for i in 0..count {
        let op=unsafe{operations.add(i).read_unaligned()};
        if op.iHash as usize>=state.states.len() || (op.pbBuffer.is_null()&&op.cbBuffer!=0) {return STATUS_INVALID_PARAMETER;}
        match op.hashOperation {
            OP_HASH_DATA => if state.states[op.iHash as usize].data.len().saturating_add(op.cbBuffer as usize)>64*1024*1024{return STATUS_INVALID_PARAMETER;},
            OP_FINISH_HASH => if op.pbBuffer.is_null() || op.cbBuffer<state.states[op.iHash as usize].algorithm.digest_len() {return STATUS_BUFFER_TOO_SMALL;},
            _=>return STATUS_INVALID_PARAMETER,
        }
    }
    for i in 0..count {
        let op=unsafe{operations.add(i).read_unaligned()};
        let hash=&mut state.states[op.iHash as usize];
        if op.hashOperation==OP_HASH_DATA {
            if op.cbBuffer!=0 {
                if hash.data.try_reserve(op.cbBuffer as usize).is_err(){return STATUS_NO_MEMORY;}
                hash.data.extend_from_slice(unsafe{core::slice::from_raw_parts(op.pbBuffer,op.cbBuffer as usize)});
            }
        } else {
            let digest_bytes:Vec<u8>=if hash.hmac {
                let key=hmac::Key::new(hash.algorithm.hmac_algorithm(),&hash.secret);
                hmac::sign(&key,&hash.data).as_ref().to_vec()
            } else { hash.algorithm.digest(&hash.data).as_ref().to_vec() };
            unsafe{ptr::copy_nonoverlapping(digest_bytes.as_ptr(),op.pbBuffer,digest_bytes.len())};
            hash.data.zeroize();hash.data.clear();
        }
    }
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn BCryptDestroyHash(hash:*mut c_void)->NTSTATUS{remove_handle(hash,1)}


#[no_mangle]
pub unsafe extern "system" fn BCryptGenRandom(algorithm: *mut c_void, buffer: *mut u8, length: ULONG, flags: ULONG) -> NTSTATUS {
    if buffer.is_null() && length != 0 { return STATUS_INVALID_PARAMETER; }
    if algorithm.is_null() { if flags != BCRYPT_USE_SYSTEM_PREFERRED_RNG { return STATUS_INVALID_PARAMETER; } }
    else { if flags != 0 { return STATUS_INVALID_PARAMETER; } let provider=match algorithm_handle(algorithm) { Ok(v)=>v, Err(e)=>return e }; if !provider.rng { return STATUS_INVALID_PARAMETER; } }
    if length == 0 { return STATUS_SUCCESS; }
    match rand::fill(unsafe { core::slice::from_raw_parts_mut(buffer, length as usize) }) { Ok(()) => STATUS_SUCCESS, Err(_) => STATUS_NOT_SUPPORTED }
}

#[no_mangle]
pub unsafe extern "system" fn BCryptGetProperty(object:*mut c_void,property:*const u16,output:*mut u8,output_len:ULONG,result_len:*mut ULONG,flags:ULONG)->NTSTATUS {
    if property.is_null() || result_len.is_null() || flags != 0 { return STATUS_INVALID_PARAMETER; }
    let name = match unsafe { read_property_name(property) } { Ok(v) => v, Err(e) => return e };
    let (kind, value, wide) = if let Ok(k) = key_handle(object) {
        match name.as_str() {
            "ObjectLength" => (0, 0, ""),
            "KeyLength" => (0, (k.key.len() as u32) * 8, ""),
            "BlockLength" => (0, 16, ""),
            "AlgorithmName" => (1, 0, "AES"),
            "ChainingMode" if k.gcm => (1, 0, "ChainingModeGCM"),
            "ChainingMode" if k.cbc => (1, 0, "ChainingModeCBC"),
            _ => return STATUS_NOT_SUPPORTED,
        }
    } else {
        let p = match algorithm_handle(object) { Ok(v) => v, Err(e) => return e };
        if p.rng {
            match name.as_str() {
                "ObjectLength" => (0, 0, ""),
                "AlgorithmName" => (1, 0, "RNG"),
                _ => return STATUS_NOT_SUPPORTED,
            }
        } else if p.cipher {
            match name.as_str() {
                "ObjectLength" => (0, 0, ""),
                "BlockLength" => (0, 16, ""),
                "AlgorithmName" => (1, 0, "AES"),
                "ChainingMode" if p.gcm => (1, 0, "ChainingModeGCM"),
                "ChainingMode" if p.cbc => (1, 0, "ChainingModeCBC"),
                _ => return STATUS_NOT_SUPPORTED,
            }
        } else {
            match name.as_str() {
                "ObjectLength" => (0, 0, ""),
                "HashDigestLength" => (0, p.algorithm.digest_len(), ""),
                "HashBlockLength" => (0, p.algorithm.block_len(), ""),
                "AlgorithmName" => (1, 0, p.algorithm.provider_name()),
                "HashMode" if p.hmac => (1, 0, "HMAC"),
                _ => return STATUS_NOT_SUPPORTED,
            }
        }
    };
    let required = if kind == 0 { 4 } else { (wide.encode_utf16().count() as u32 + 1) * 2 };
    unsafe { result_len.write(required) };
    if output.is_null() || output_len < required { return STATUS_BUFFER_TOO_SMALL; }
    if kind == 0 { unsafe { write_u32(output, value) }; }
    else { unsafe { write_wide(output, wide) }; }
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn BCryptDeriveKeyPBKDF2(prf:*mut c_void,password:*mut u8,password_len:ULONG,salt:*mut u8,salt_len:ULONG,iterations:u64,derived:*mut u8,derived_len:ULONG,flags:ULONG)->NTSTATUS {
 if (password.is_null()&&password_len!=0)||(salt.is_null()&&salt_len!=0)||(derived.is_null()&&derived_len!=0)||iterations==0||flags!=0{return STATUS_INVALID_PARAMETER;}
 let p=match algorithm_handle(prf){Ok(v)=>v,Err(e)=>return e};if !p.hmac||p.cipher{return STATUS_INVALID_PARAMETER;}
 let iterations=match u32::try_from(iterations){Ok(v)=>v,Err(_)=>return STATUS_INVALID_PARAMETER};let nz=match NonZeroU32::new(iterations){Some(v)=>v,None=>return STATUS_INVALID_PARAMETER};let pass=if password_len==0{&[][..]}else{unsafe{core::slice::from_raw_parts(password,password_len as usize)}};let salt=if salt_len==0{&[][..]}else{unsafe{core::slice::from_raw_parts(salt,salt_len as usize)}};let out=if derived_len==0{&mut [][..]}else{unsafe{core::slice::from_raw_parts_mut(derived,derived_len as usize)}};
 pbkdf2::derive(p.algorithm.pbkdf2_algorithm(),nz,salt,pass,out);STATUS_SUCCESS
}


#[no_mangle]
pub unsafe extern "system" fn BCryptHash(algorithm:*mut c_void,secret:*const u8,secret_len:ULONG,input:*const u8,input_len:ULONG,output:*mut u8,output_len:ULONG)->NTSTATUS {
 if (secret.is_null()&&secret_len!=0)||(input.is_null()&&input_len!=0)||output.is_null(){return STATUS_INVALID_PARAMETER;}let p=match algorithm_handle(algorithm){Ok(v)=>v,Err(e)=>return e};if p.cipher||(!p.hmac&&secret_len!=0){return STATUS_INVALID_PARAMETER;}if output_len<p.algorithm.digest_len(){return STATUS_BUFFER_TOO_SMALL;}
 let data=if input_len==0{&[][..]}else{unsafe{core::slice::from_raw_parts(input,input_len as usize)}};
 if p.hmac{let kbytes=if secret_len==0{&[][..]}else{unsafe{core::slice::from_raw_parts(secret,secret_len as usize)}};let k=hmac::Key::new(p.algorithm.hmac_algorithm(),kbytes);let d=hmac::sign(&k,data);unsafe{ptr::copy_nonoverlapping(d.as_ref().as_ptr(),output,d.as_ref().len())};}
 else{let d=p.algorithm.digest(data);unsafe{ptr::copy_nonoverlapping(d.as_ref().as_ptr(),output,d.as_ref().len())};}STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn BCryptGetFipsAlgorithmMode(enabled: *mut u8) -> NTSTATUS {
    if enabled.is_null() { return STATUS_INVALID_PARAMETER; }
    unsafe { enabled.write(0) };
    STATUS_SUCCESS
}



// CNG enumeration buffers are owned by bcrypt.dll and released only through
// BCryptFreeBuffer. The registry avoids trusting caller-supplied pointers when
// freeing an allocation and keeps the returned storage stable.
#[repr(C)]
struct BcryptAlgorithmIdentifier {
    name: *mut u16,
    class: ULONG,
    flags: ULONG,
}
#[repr(C)]
struct BcryptProvider {
    name: *mut u16,
}

#[cfg(target_pointer_width = "64")]
const _: [(); 8] = [(); core::mem::size_of::<BcryptProvider>()];

static ENUM_BUFFERS: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<usize, Box<[u64]>>>
> = std::sync::OnceLock::new();

fn enum_buffers() -> &'static std::sync::Mutex<std::collections::HashMap<usize, Box<[u64]>>> {
    ENUM_BUFFERS.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

fn store_enum_buffer(bytes: usize) -> Result<*mut u8, NTSTATUS> {
    let words = bytes.checked_add(7).ok_or(STATUS_NO_MEMORY)? / 8;
    let mut storage = Vec::<u64>::new();
    storage.try_reserve_exact(words).map_err(|_| STATUS_NO_MEMORY)?;
    storage.resize(words, 0);
    let mut storage = storage.into_boxed_slice();
    let pointer = storage.as_mut_ptr().cast::<u8>();
    let key = pointer as usize;
    enum_buffers().lock().map_err(|_| STATUS_NO_MEMORY)?.insert(key, storage);
    Ok(pointer)
}

fn enum_write_wide(base: *mut u8, offset: usize, value: &str) -> *mut u16 {
    let target = unsafe { base.add(offset).cast::<u16>() };
    for (index, unit) in value.encode_utf16().chain(core::iter::once(0)).enumerate() {
        unsafe { target.add(index).write_unaligned(unit) };
    }
    target
}

/// Enumerates the SHA-2 hash providers and the system RNG implemented here.
/// Only operations actually supported by this DLL are advertised.
#[no_mangle]
pub unsafe extern "system" fn BCryptEnumAlgorithms(operations:ULONG,count:*mut ULONG,list:*mut *mut BcryptAlgorithmIdentifier,flags:ULONG)->NTSTATUS {
 const H:ULONG=2;const R:ULONG=32;const KDF:ULONG=64;if count.is_null()||list.is_null()||flags!=0||operations==0||operations&!(1|H|4|8|16|R|KDF)!=0{return STATUS_INVALID_PARAMETER;}
 let mut e:Vec<(&str,ULONG)>=Vec::new();if operations&1!=0{e.push(("AES",1));}if operations&H!=0{e.extend([("SHA256",2),("SHA384",2),("SHA512",2)]);}if operations&4!=0{e.push(("RSA",3));}if operations&R!=0{e.push(("RNG",6));}if operations&KDF!=0{e.push(("PBKDF2",7));}
 if e.is_empty(){unsafe{count.write(0);list.write(ptr::null_mut())};return STATUS_SUCCESS;}
 let sb=e.len()*core::mem::size_of::<BcryptAlgorithmIdentifier>();let nb:usize=e.iter().map(|(n,_)|(n.encode_utf16().count()+1)*2).sum();let base=match store_enum_buffer(sb+nb){Ok(v)=>v,Err(x)=>return x};let records=base.cast::<BcryptAlgorithmIdentifier>();let mut off=sb;
 for(i,(n,c))in e.iter().enumerate(){let np=enum_write_wide(base,off,n);unsafe{records.add(i).write(BcryptAlgorithmIdentifier{name:np,class:*c,flags:0})};off+=(n.encode_utf16().count()+1)*2;}
 unsafe{count.write(e.len() as ULONG);list.write(records)};STATUS_SUCCESS
}


#[no_mangle]
pub unsafe extern "system" fn BCryptEnumProviders(algorithm:*const u16,count:*mut ULONG,providers:*mut *mut BcryptProvider,flags:ULONG)->NTSTATUS {
 if algorithm.is_null()||count.is_null()||providers.is_null()||flags!=0{return STATUS_INVALID_PARAMETER;}
 let alg=match unsafe{read_property_name(algorithm)}{Ok(v)=>v,Err(e)=>return e};
 if !matches!(alg.as_str(),"AES"|"SHA256"|"SHA384"|"SHA512"|"RSA"|"RNG"|"PBKDF2"){unsafe{count.write(0);providers.write(ptr::null_mut())};return 0xC0000225_u32 as i32;}
 const P:&str="Microsoft Primitive Provider";let sb=core::mem::size_of::<BcryptProvider>();let base=match store_enum_buffer(sb+(P.encode_utf16().count()+1)*2){Ok(v)=>v,Err(e)=>return e};let name=enum_write_wide(base,sb,P);let rec=base.cast::<BcryptProvider>();unsafe{rec.write(BcryptProvider{name});count.write(1);providers.write(rec)};STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn BCryptFreeBuffer(buffer: *mut c_void) {
    if buffer.is_null() { return; }
    if let Ok(mut buffers) = enum_buffers().lock() {
        buffers.remove(&(buffer as usize));
    }
}


#[no_mangle]
pub unsafe extern "system" fn BCryptSetProperty(object:*mut c_void,property:*const u16,input:*const u8,input_len:ULONG,flags:ULONG)->NTSTATUS {
 if property.is_null()||input.is_null()||flags!=0{return STATUS_INVALID_PARAMETER;}let n=match unsafe{read_property_name(property)}{Ok(v)=>v,Err(e)=>return e};if n!="ChainingMode"{return STATUS_NOT_SUPPORTED;}
 let val=unsafe{core::slice::from_raw_parts(input,input_len as usize)};let gcm:Vec<u16>="ChainingModeGCM".encode_utf16().chain(core::iter::once(0)).collect();let cbc:Vec<u16>="ChainingModeCBC".encode_utf16().chain(core::iter::once(0)).collect();let matches=|v:&[u16]| val.len()==v.len()*2 && v.iter().enumerate().all(|(i,u)|unsafe{ptr::read_unaligned(input.add(i*2).cast::<u16>())}==*u);let is_gcm=matches(&gcm);let is_cbc=matches(&cbc);if !is_gcm&&!is_cbc{return STATUS_NOT_SUPPORTED;}
 let Ok(mut t)=handle_table().lock()else{return STATUS_NO_MEMORY;};let Some(RegisteredHandle::Algorithm(a))=t.get_mut(&(object as usize))else{return STATUS_INVALID_PARAMETER;};if !a.cipher{return STATUS_INVALID_PARAMETER;}a.gcm=is_gcm;a.cbc=is_cbc;STATUS_SUCCESS
}

#[repr(C)]
pub struct BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO {
    pub cbSize: ULONG, pub dwInfoVersion: ULONG, pub pbNonce: *mut u8, pub cbNonce: ULONG,
    pub pbAuthData: *mut u8, pub cbAuthData: ULONG, pub pbTag: *mut u8, pub cbTag: ULONG,
    pub pbMacContext: *mut u8, pub cbMacContext: ULONG, pub cbAAD: ULONG, pub cbData: u64, pub dwFlags: ULONG,
}

#[no_mangle]
pub unsafe extern "system" fn BCryptGenerateSymmetricKey(algorithm:*mut c_void,out:*mut *mut c_void,object:*mut u8,object_len:ULONG,secret:*mut u8,secret_len:ULONG,flags:ULONG)->NTSTATUS {
 if out.is_null()||!object.is_null()||object_len!=0||secret.is_null()||secret_len==0||flags!=0{return STATUS_INVALID_PARAMETER;}let p=match algorithm_handle(algorithm){Ok(v)=>v,Err(e)=>return e};
 if p.kdf { if secret_len>1024*1024{return STATUS_INVALID_PARAMETER;} let secret=unsafe{core::slice::from_raw_parts(secret,secret_len as usize)}.to_vec();let token=match register_handle(RegisteredHandle::KdfKey(KdfKeyHandle{secret})){Ok(v)=>v,Err(e)=>return e};unsafe{out.write(token)};return STATUS_SUCCESS; }
 if !p.cipher||!(p.gcm && (secret_len==16||secret_len==24||secret_len==32) || p.cbc && (secret_len==16||secret_len==24||secret_len==32)){return STATUS_NOT_SUPPORTED;}
 let key=unsafe{core::slice::from_raw_parts(secret,secret_len as usize)}.to_vec();let token=match register_handle(RegisteredHandle::Key(KeyHandle{key,gcm:p.gcm,cbc:p.cbc})){Ok(v)=>v,Err(e)=>return e};unsafe{out.write(token)};STATUS_SUCCESS
}
unsafe fn auth_info(info:*mut c_void)->Result<(Vec<u8>,Vec<u8>,*mut u8,usize),NTSTATUS>{
 if info.is_null(){return Err(STATUS_INVALID_PARAMETER);}let i=unsafe{&mut *info.cast::<BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO>()};
 if i.cbSize as usize!=core::mem::size_of::<BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO>()||i.dwInfoVersion!=1||i.pbNonce.is_null()||i.cbNonce!=12||i.pbTag.is_null()||i.cbTag!=16||(i.pbAuthData.is_null()&&i.cbAuthData!=0)||!i.pbMacContext.is_null()||i.cbMacContext!=0||i.dwFlags!=0{return Err(STATUS_INVALID_PARAMETER);}
 let nonce=unsafe{core::slice::from_raw_parts(i.pbNonce,12)}.to_vec();let aad=if i.cbAuthData==0{Vec::new()}else{unsafe{core::slice::from_raw_parts(i.pbAuthData,i.cbAuthData as usize)}.to_vec()};Ok((nonce,aad,i.pbTag,i.cbTag as usize))
}

fn aes_cbc_crypt(key: &[u8], input: &[u8], iv: &mut [u8;16], encrypt: bool, padding: bool) -> Result<Vec<u8>, NTSTATUS> {
    if key.len()!=16 && key.len()!=24 && key.len()!=32 { return Err(STATUS_INVALID_PARAMETER); }
    if !encrypt && (input.is_empty() || input.len()%16!=0) { return Err(STATUS_INVALID_PARAMETER); }
    let mut data=input.to_vec();
    if encrypt && padding { let n=(16-data.len()%16) as u8; data.resize(data.len()+n as usize,n); }
    if data.len()%16!=0 { return Err(STATUS_INVALID_PARAMETER); }
    let mut chain=*iv;
    for chunk in data.chunks_exact_mut(16) {
        let mut block=[0u8;16]; block.copy_from_slice(chunk);
        if encrypt {
            for i in 0..16 { block[i]^=chain[i]; }
            let b=GenericArray::from_mut_slice(&mut block);
            match key.len() {16=>Aes128::new_from_slice(key).map_err(|_|STATUS_INVALID_PARAMETER)?.encrypt_block(b),24=>Aes192::new_from_slice(key).map_err(|_|STATUS_INVALID_PARAMETER)?.encrypt_block(b),_=>Aes256::new_from_slice(key).map_err(|_|STATUS_INVALID_PARAMETER)?.encrypt_block(b)};
            chunk.copy_from_slice(&block); chain=block;
        } else {
            let ciphertext=block; let b=GenericArray::from_mut_slice(&mut block);
            match key.len() {16=>Aes128::new_from_slice(key).map_err(|_|STATUS_INVALID_PARAMETER)?.decrypt_block(b),24=>Aes192::new_from_slice(key).map_err(|_|STATUS_INVALID_PARAMETER)?.decrypt_block(b),_=>Aes256::new_from_slice(key).map_err(|_|STATUS_INVALID_PARAMETER)?.decrypt_block(b)};
            for i in 0..16 { chunk[i]=block[i]^chain[i]; } chain=ciphertext;
        }
    }
    if !encrypt && padding { let n=*data.last().ok_or(STATUS_INVALID_PARAMETER)? as usize; if n==0||n>16||n>data.len()||!data[data.len()-n..].iter().all(|x|*x==n as u8){data.zeroize();return Err(STATUS_INVALID_PARAMETER);} data.truncate(data.len()-n); }
    *iv=chain; Ok(data)
}
unsafe fn bcrypt_cbc(key:*mut c_void,input:*mut u8,input_len:ULONG,iv:*mut u8,iv_len:ULONG,output:*mut u8,output_len:ULONG,result_len:*mut ULONG,flags:ULONG,encrypt:bool)->NTSTATUS {
    if result_len.is_null()||(input.is_null()&&input_len!=0)||iv.is_null()||iv_len!=16||flags&!BCRYPT_BLOCK_PADDING!=0{return STATUS_INVALID_PARAMETER;}
    let k=match key_handle(key){Ok(v)=>v,Err(e)=>return e}; if !k.cbc{return STATUS_NOT_SUPPORTED;} let mut chain=[0u8;16]; unsafe{ptr::copy_nonoverlapping(iv,chain.as_mut_ptr(),16)};
    let source=if input_len==0{&[][..]}else{unsafe{core::slice::from_raw_parts(input,input_len as usize)}};
    let result=match aes_cbc_crypt(&k.key,source,&mut chain,encrypt,flags&BCRYPT_BLOCK_PADDING!=0){Ok(v)=>v,Err(e)=>return e};
    unsafe{result_len.write(result.len() as ULONG)};if output.is_null()||(output_len as usize)<result.len(){return STATUS_BUFFER_TOO_SMALL;}
    if !result.is_empty(){unsafe{ptr::copy_nonoverlapping(result.as_ptr(),output,result.len())};} unsafe{ptr::copy_nonoverlapping(chain.as_ptr(),iv,16)}; STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn BCryptEncrypt(key:*mut c_void,input:*mut u8,input_len:ULONG,padding_info:*mut c_void,iv:*mut u8,iv_len:ULONG,output:*mut u8,output_len:ULONG,result_len:*mut ULONG,flags:ULONG)->NTSTATUS {
 if padding_info.is_null(){return unsafe{bcrypt_cbc(key,input,input_len,iv,iv_len,output,output_len,result_len,flags,true)};}
 if result_len.is_null()||(input.is_null()&&input_len!=0)||flags!=0||!iv.is_null()||iv_len!=0{return STATUS_INVALID_PARAMETER;}let k=match key_handle(key){Ok(v)=>v,Err(e)=>return e};if !k.gcm{return STATUS_NOT_SUPPORTED;}let(nonce,aad,tag,_)=match unsafe{auth_info(padding_info)}{Ok(v)=>v,Err(e)=>return e};unsafe{result_len.write(input_len)};if output.is_null()||output_len<input_len{return STATUS_BUFFER_TOO_SMALL;}
 let mut buf=if input_len==0{Vec::new()}else{unsafe{core::slice::from_raw_parts(input,input_len as usize)}.to_vec()};
 if k.key.len()==24 { type G=AesGcm<Aes192,U12,U16>; let cipher=match G::new_from_slice(&k.key){Ok(v)=>v,Err(_)=>return STATUS_INVALID_PARAMETER};let n=aes_gcm::Nonce::<U12>::from_slice(&nonce);let t=match cipher.encrypt_in_place_detached(n,aad.as_slice(),&mut buf){Ok(v)=>v,Err(_)=>return STATUS_INVALID_PARAMETER};unsafe{ptr::copy_nonoverlapping(buf.as_ptr(),output,input_len as usize);ptr::copy_nonoverlapping(t.as_slice().as_ptr(),tag,16);let info=&mut *padding_info.cast::<BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO>();info.cbAAD=info.cbAuthData;info.cbData=input_len as u64};return STATUS_SUCCESS;}
 let alg=match k.key.len(){16=>&aead::AES_128_GCM,32=>&aead::AES_256_GCM,_=>return STATUS_INVALID_PARAMETER};let unbound=match aead::UnboundKey::new(alg,&k.key){Ok(v)=>v,Err(_)=>return STATUS_INVALID_PARAMETER};let less=aead::LessSafeKey::new(unbound);let nonce=match aead::Nonce::try_assume_unique_for_key(&nonce){Ok(v)=>v,Err(_)=>return STATUS_INVALID_PARAMETER};
 let t=match less.seal_in_place_separate_tag(nonce,aead::Aad::from(aad.as_slice()),&mut buf){Ok(v)=>v,Err(_)=>return STATUS_INVALID_PARAMETER};unsafe{ptr::copy_nonoverlapping(buf.as_ptr(),output,input_len as usize);ptr::copy_nonoverlapping(t.as_ref().as_ptr(),tag,16);let info=&mut *padding_info.cast::<BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO>();info.cbAAD=info.cbAuthData;info.cbData=input_len as u64};STATUS_SUCCESS
}
#[no_mangle]
pub unsafe extern "system" fn BCryptDecrypt(key:*mut c_void,input:*mut u8,input_len:ULONG,padding_info:*mut c_void,iv:*mut u8,iv_len:ULONG,output:*mut u8,output_len:ULONG,result_len:*mut ULONG,flags:ULONG)->NTSTATUS {
 if padding_info.is_null(){return unsafe{bcrypt_cbc(key,input,input_len,iv,iv_len,output,output_len,result_len,flags,false)};}
 if result_len.is_null()||(input.is_null()&&input_len!=0)||flags!=0||!iv.is_null()||iv_len!=0{return STATUS_INVALID_PARAMETER;}let k=match key_handle(key){Ok(v)=>v,Err(e)=>return e};let(nonce,aad,tag,_)=match unsafe{auth_info(padding_info)}{Ok(v)=>v,Err(e)=>return e};unsafe{result_len.write(input_len)};if output.is_null()||output_len<input_len{return STATUS_BUFFER_TOO_SMALL;}
 let mut buf=if input_len==0{Vec::new()}else{unsafe{core::slice::from_raw_parts(input,input_len as usize)}.to_vec()};
 if k.key.len()==24 { type G=AesGcm<Aes192,U12,U16>;let cipher=match G::new_from_slice(&k.key){Ok(v)=>v,Err(_)=>return STATUS_INVALID_PARAMETER};let n=aes_gcm::Nonce::<U12>::from_slice(&nonce);let t=unsafe{core::slice::from_raw_parts(tag,16)};let tag_value=aes_gcm::Tag::<U16>::from_slice(t);if cipher.decrypt_in_place_detached(n,aad.as_slice(),&mut buf,tag_value).is_err(){return STATUS_AUTH_TAG_MISMATCH;}unsafe{ptr::copy_nonoverlapping(buf.as_ptr(),output,buf.len());result_len.write(buf.len() as ULONG);let info=&mut *padding_info.cast::<BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO>();info.cbAAD=info.cbAuthData;info.cbData=buf.len() as u64};return STATUS_SUCCESS;}
 let alg=match k.key.len(){16=>&aead::AES_128_GCM,32=>&aead::AES_256_GCM,_=>return STATUS_INVALID_PARAMETER};let unbound=match aead::UnboundKey::new(alg,&k.key){Ok(v)=>v,Err(_)=>return STATUS_INVALID_PARAMETER};let less=aead::LessSafeKey::new(unbound);let nonce=match aead::Nonce::try_assume_unique_for_key(&nonce){Ok(v)=>v,Err(_)=>return STATUS_INVALID_PARAMETER};
 let tagbytes=unsafe{core::slice::from_raw_parts(tag,16)};let plain=match less.open_in_place_separate_tag(nonce,aead::Aad::from(aad.as_slice()),tagbytes,&mut buf){Ok(v)=>v,Err(_)=>return STATUS_AUTH_TAG_MISMATCH};unsafe{ptr::copy_nonoverlapping(plain.as_ptr(),output,plain.len());result_len.write(plain.len() as ULONG);let info=&mut *padding_info.cast::<BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO>();info.cbAAD=info.cbAuthData;info.cbData=plain.len() as u64};STATUS_SUCCESS
}
const BCRYPT_KEY_DATA_BLOB_MAGIC: ULONG = 0x4d42_444b;
const BCRYPT_KEY_DATA_BLOB_VERSION1: ULONG = 1;
const BCRYPT_KEY_DATA_BLOB: &str = "KeyDataBlob";

#[repr(C)]
#[derive(Clone, Copy)]
struct BcryptKeyDataBlobHeader { dwMagic: ULONG, dwVersion: ULONG, cbKeyData: ULONG }

#[no_mangle]
pub unsafe extern "system" fn BCryptDuplicateKey(key:*mut c_void,out:*mut *mut c_void,object:*mut u8,object_len:ULONG,flags:ULONG)->NTSTATUS {
    if out.is_null() || !object.is_null() || object_len != 0 || flags != 0 { return STATUS_INVALID_PARAMETER; }
    let state = match key_handle(key) { Ok(v) => v, Err(e) => return e };
    match register_handle(RegisteredHandle::Key(state)) { Ok(token) => { unsafe { out.write(token) }; STATUS_SUCCESS }, Err(e) => e }
}

#[no_mangle]
pub unsafe extern "system" fn BCryptImportKey(algorithm:*mut c_void,import_key:*mut c_void,blob_type:*const u16,out:*mut *mut c_void,object:*mut u8,object_len:ULONG,input:*mut u8,input_len:ULONG,flags:ULONG)->NTSTATUS {
    if out.is_null() || blob_type.is_null() || !import_key.is_null() || !object.is_null() || object_len != 0 || input.is_null() || flags != 0 { return STATUS_INVALID_PARAMETER; }
    let name = match unsafe { read_property_name(blob_type) } { Ok(v) => v, Err(e) => return e };
    if name != BCRYPT_KEY_DATA_BLOB { return STATUS_NOT_SUPPORTED; }
    let provider = match algorithm_handle(algorithm) { Ok(v) => v, Err(e) => return e };
    if !provider.cipher || (!provider.cbc && !provider.gcm) { return STATUS_INVALID_PARAMETER; }
    if (input_len as usize) < core::mem::size_of::<BcryptKeyDataBlobHeader>() { return STATUS_INVALID_PARAMETER; }
    let header = unsafe { ptr::read_unaligned(input.cast::<BcryptKeyDataBlobHeader>()) };
    let key_len = header.cbKeyData as usize;
    let expected = match core::mem::size_of::<BcryptKeyDataBlobHeader>().checked_add(key_len) { Some(v) => v, None => return STATUS_INVALID_PARAMETER };
    if header.dwMagic != BCRYPT_KEY_DATA_BLOB_MAGIC || header.dwVersion != BCRYPT_KEY_DATA_BLOB_VERSION1 || expected != input_len as usize || !matches!(key_len,16|24|32) { return STATUS_INVALID_PARAMETER; }
    let key_bytes = unsafe { core::slice::from_raw_parts(input.add(core::mem::size_of::<BcryptKeyDataBlobHeader>()), key_len) }.to_vec();
    let token = match register_handle(RegisteredHandle::Key(KeyHandle { key:key_bytes, gcm:provider.gcm, cbc:provider.cbc })) { Ok(v) => v, Err(e) => return e };
    unsafe { out.write(token) }; STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn BCryptExportKey(key:*mut c_void,export_key:*mut c_void,blob_type:*const u16,output:*mut u8,output_len:ULONG,result_len:*mut ULONG,flags:ULONG)->NTSTATUS {
    if result_len.is_null() || blob_type.is_null() || !export_key.is_null() || flags != 0 { return STATUS_INVALID_PARAMETER; }
    let name = match unsafe { read_property_name(blob_type) } { Ok(v) => v, Err(e) => return e };
    if name != BCRYPT_KEY_DATA_BLOB { return STATUS_NOT_SUPPORTED; }
    let state = match key_handle(key) { Ok(v) => v, Err(e) => return e };
    let required = match core::mem::size_of::<BcryptKeyDataBlobHeader>().checked_add(state.key.len()) { Some(v) => v, None => return STATUS_INVALID_PARAMETER };
    unsafe { result_len.write(required as ULONG) };
    if output.is_null() || (output_len as usize) < required { return STATUS_BUFFER_TOO_SMALL; }
    let header = BcryptKeyDataBlobHeader { dwMagic:BCRYPT_KEY_DATA_BLOB_MAGIC, dwVersion:BCRYPT_KEY_DATA_BLOB_VERSION1, cbKeyData:state.key.len() as ULONG };
    unsafe { ptr::write_unaligned(output.cast::<BcryptKeyDataBlobHeader>(),header); ptr::copy_nonoverlapping(state.key.as_ptr(),output.add(core::mem::size_of::<BcryptKeyDataBlobHeader>()),state.key.len()); }
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn BCryptDestroyKey(key:*mut c_void)->NTSTATUS{remove_handle(key,2)}

#[repr(C)]
struct BcryptBuffer { cbBuffer: ULONG, BufferType: ULONG, pvBuffer: *mut c_void }
#[repr(C)]
struct BcryptBufferDesc { ulVersion: ULONG, cBuffers: ULONG, pBuffers: *mut BcryptBuffer }

/// Windows CNG PBKDF2 provider entry point. Accepts KDF_HASH_ALGORITHM,
/// KDF_SALT (or KDF_GENERIC_PARAMETER), and optional KDF_ITERATION_COUNT.
#[no_mangle]
pub unsafe extern "system" fn BCryptKeyDerivation(key:*mut c_void,params:*mut BcryptBufferDesc,output:*mut u8,output_len:ULONG,result_len:*mut ULONG,flags:ULONG)->NTSTATUS {
    const KDF_HASH_ALGORITHM:ULONG=0;
    const KDF_SALT:ULONG=15;
    const KDF_ITERATION_COUNT:ULONG=16;
    const KDF_GENERIC_PARAMETER:ULONG=17;
    if result_len.is_null() || output.is_null() || output_len==0 || flags!=0 || params.is_null() { return STATUS_INVALID_PARAMETER; }
    let secret = match handle_table().lock() {
        Ok(t) => match t.get(&(key as usize)) { Some(RegisteredHandle::KdfKey(v))=>v.secret.clone(), _=>return STATUS_INVALID_PARAMETER },
        Err(_) => return STATUS_NO_MEMORY,
    };
    let desc=unsafe{ptr::read_unaligned(params)};
    if desc.ulVersion!=0 || desc.cBuffers==0 || desc.cBuffers>16 || desc.pBuffers.is_null() { return STATUS_INVALID_PARAMETER; }
    let mut algorithm:Option<HashAlgorithm>=None;
    let mut salt:Option<Vec<u8>>=None;
    let mut iterations:u64=10_000;
    for i in 0..desc.cBuffers as usize {
        let b=unsafe{ptr::read_unaligned(desc.pBuffers.add(i))};
        if b.pvBuffer.is_null() && b.cbBuffer!=0 { return STATUS_INVALID_PARAMETER; }
        match b.BufferType {
            KDF_HASH_ALGORITHM => {
                if algorithm.is_some() || b.pvBuffer.is_null() || b.cbBuffer<2 || b.cbBuffer%2!=0 { return STATUS_INVALID_PARAMETER; }
                let units=unsafe{core::slice::from_raw_parts(b.pvBuffer.cast::<u16>(),(b.cbBuffer/2) as usize)};
                let end=units.iter().position(|u|*u==0).unwrap_or(units.len());
                let name=match String::from_utf16(&units[..end]) { Ok(v)=>v,Err(_)=>return STATUS_INVALID_PARAMETER };
                algorithm=Some(match name.as_str() {"SHA256"=>HashAlgorithm::Sha256,"SHA384"=>HashAlgorithm::Sha384,"SHA512"=>HashAlgorithm::Sha512,_=>return STATUS_NOT_SUPPORTED});
            }
            KDF_SALT | KDF_GENERIC_PARAMETER => {
                if salt.is_some() || b.cbBuffer>1024*1024 { return STATUS_INVALID_PARAMETER; }
                salt=Some(if b.cbBuffer==0 {Vec::new()} else {unsafe{core::slice::from_raw_parts(b.pvBuffer.cast::<u8>(),b.cbBuffer as usize)}.to_vec()});
            }
            KDF_ITERATION_COUNT => {
                if b.pvBuffer.is_null() || b.cbBuffer!=8 { return STATUS_INVALID_PARAMETER; }
                iterations=unsafe{ptr::read_unaligned(b.pvBuffer.cast::<u64>())};
            }
            _ => return STATUS_NOT_SUPPORTED,
        }
    }
    let Some(alg)=algorithm else{return STATUS_INVALID_PARAMETER};
    let Some(salt)=salt else{return STATUS_INVALID_PARAMETER};
    if iterations==0 || iterations>u32::MAX as u64 || iterations>10_000_000 { return STATUS_INVALID_PARAMETER; }
    let nz=match NonZeroU32::new(iterations as u32){Some(v)=>v,None=>return STATUS_INVALID_PARAMETER};
    let out=unsafe{core::slice::from_raw_parts_mut(output,output_len as usize)};
    pbkdf2::derive(alg.pbkdf2_algorithm(),nz,&salt,&secret,out);
    unsafe{result_len.write(output_len)};
    STATUS_SUCCESS
}


/// Destroys a CNG shared-secret handle. Secret bytes are zeroized by Drop.
#[no_mangle]
pub unsafe extern "system" fn BCryptDestroySecret(secret:*mut c_void)->NTSTATUS { remove_handle(secret,3) }

/// BCryptDeriveKey with the CNG RAW_SECRET ("TRUNCATE") KDF.
#[no_mangle]
pub unsafe extern "system" fn BCryptDeriveKey(secret:*mut c_void,kdf:*const u16,params:*mut BcryptBufferDesc,output:*mut u8,output_len:ULONG,result_len:*mut ULONG,flags:ULONG)->NTSTATUS {
    if kdf.is_null() || result_len.is_null() || flags!=0 || output.is_null() || output_len==0 || !params.is_null() { return STATUS_INVALID_PARAMETER; }
    let mut name=Vec::new();
    for i in 0..64 { let u=unsafe{kdf.add(i).read()}; if u==0 {break;} name.push(u); if i==63{return STATUS_INVALID_PARAMETER;} }
    if String::from_utf16(&name).ok().as_deref()!=Some("TRUNCATE") { return STATUS_NOT_SUPPORTED; }
    let material=match handle_table().lock() { Ok(t)=>match t.get(&(secret as usize)){Some(RegisteredHandle::KdfKey(v))=>v.secret.clone(),_=>return STATUS_INVALID_PARAMETER},Err(_)=>return STATUS_NO_MEMORY };
    let mut little_endian=material; little_endian.reverse();
    let n=core::cmp::min(output_len as usize,little_endian.len());
    if n==0 { return STATUS_INVALID_PARAMETER; }
    unsafe { ptr::copy_nonoverlapping(little_endian.as_ptr(),output,n); result_len.write(n as ULONG); }
    STATUS_SUCCESS
}

#[repr(C)]
#[derive(Clone,Copy)]
struct BcryptRsaBlobHeader { magic:ULONG, bit_length:ULONG, cb_public_exp:ULONG, cb_modulus:ULONG, cb_prime1:ULONG, cb_prime2:ULONG }

/// Imports a Windows BCRYPT_RSAPUBLIC_BLOB into an opaque RSA public-key handle.
#[no_mangle]
pub unsafe extern "system" fn BCryptImportKeyPair(algorithm:*mut c_void,import_key:*mut c_void,blob_type:*const u16,out:*mut *mut c_void,input:*const u8,input_len:ULONG,flags:ULONG)->NTSTATUS {
    const BCRYPT_RSAPUBLIC_MAGIC:ULONG=0x31415352;
    if out.is_null() || import_key.is_null()==false || blob_type.is_null() || input.is_null() || flags!=0 { return STATUS_INVALID_PARAMETER; }
    let provider=match algorithm_handle(algorithm){Ok(v)=>v,Err(e)=>return e};
    if !provider.rsa { return STATUS_INVALID_PARAMETER; }
    let mut name=Vec::new(); for i in 0..64 {let u=unsafe{blob_type.add(i).read()};if u==0{break;}name.push(u);if i==63{return STATUS_INVALID_PARAMETER;}}
    if String::from_utf16(&name).ok().as_deref()!=Some("RSAPUBLICBLOB") { return STATUS_NOT_SUPPORTED; }
    if (input_len as usize)<core::mem::size_of::<BcryptRsaBlobHeader>() { return STATUS_INVALID_PARAMETER; }
    let h=unsafe{ptr::read_unaligned(input.cast::<BcryptRsaBlobHeader>())};
    if h.magic!=BCRYPT_RSAPUBLIC_MAGIC || h.bit_length<512 || h.bit_length>16384 || h.cb_public_exp==0 || h.cb_public_exp>8 || h.cb_modulus as u64!=(h.bit_length as u64+7)/8 || h.cb_prime1!=0 || h.cb_prime2!=0 {return STATUS_INVALID_PARAMETER;}
    let total=core::mem::size_of::<BcryptRsaBlobHeader>().checked_add(h.cb_public_exp as usize).and_then(|v|v.checked_add(h.cb_modulus as usize));
    if total!=Some(input_len as usize) {return STATUS_INVALID_PARAMETER;}
    let bytes=unsafe{core::slice::from_raw_parts(input.add(core::mem::size_of::<BcryptRsaBlobHeader>()),input_len as usize-core::mem::size_of::<BcryptRsaBlobHeader>())};
    let e=BigUint::from_bytes_be(&bytes[..h.cb_public_exp as usize]);
    let n=BigUint::from_bytes_be(&bytes[h.cb_public_exp as usize..]);
    let key=match RsaPublicKey::new(n,e){Ok(v)=>v,Err(_)=>return STATUS_INVALID_PARAMETER};
    if key.n().bits()!=h.bit_length as usize {return STATUS_INVALID_PARAMETER;}
    let token=match register_handle(RegisteredHandle::RsaPublic(key)){Ok(v)=>v,Err(e)=>return e};
    unsafe{out.write(token)};STATUS_SUCCESS
}

/// Verifies a precomputed SHA-256 digest using RSA PKCS#1 v1.5.
#[no_mangle]
pub unsafe extern "system" fn BCryptVerifySignature(key:*mut c_void,padding_info:*mut c_void,hash:*const u8,hash_len:ULONG,signature:*const u8,signature_len:ULONG,flags:ULONG)->NTSTATUS {
    const BCRYPT_PAD_PKCS1:ULONG=0x00000002;
    if !padding_info.is_null() || hash.is_null() || hash_len!=32 || signature.is_null() || flags!=BCRYPT_PAD_PKCS1 {return STATUS_INVALID_PARAMETER;}
    let public=match handle_table().lock(){Ok(t)=>match t.get(&(key as usize)){Some(RegisteredHandle::RsaPublic(v))=>v.clone(),_=>return STATUS_INVALID_PARAMETER},Err(_)=>return STATUS_NO_MEMORY};
    if signature_len as usize!=public.size(){return STATUS_INVALID_PARAMETER;}
    let sig=match RsaSignature::try_from(unsafe{core::slice::from_raw_parts(signature,signature_len as usize)}){Ok(v)=>v,Err(_)=>return STATUS_INVALID_PARAMETER};
    let verifier=VerifyingKey::<Sha256>::new(public);
    match verifier.verify_prehash(unsafe{core::slice::from_raw_parts(hash,hash_len as usize)},&sig){Ok(())=>STATUS_SUCCESS,Err(_)=>STATUS_INVALID_SIGNATURE}
}

// x64 ABI layout guards: these structures are consumed directly by Windows callers.
#[cfg(target_pointer_width = "64")]
const _: [(); 16] = [(); core::mem::size_of::<BcryptAlgorithmIdentifier>()];
#[cfg(target_pointer_width = "64")]
const _: [(); 16] = [(); core::mem::size_of::<BcryptBuffer>()];
#[cfg(target_pointer_width = "64")]
const _: [(); 16] = [(); core::mem::size_of::<BcryptBufferDesc>()];
#[cfg(target_pointer_width = "64")]
const _: [(); 88] = [(); core::mem::size_of::<BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO>()];
#[cfg(target_pointer_width = "64")]
const _: [(); 24] = [(); core::mem::size_of::<BcryptMultiHashOperation>()];
#[cfg(target_pointer_width = "64")]
const _: [(); 24] = [(); core::mem::size_of::<BcryptRsaBlobHeader>()];

include!(concat!(env!("OUT_DIR"), "/bcrypt_unsupported_exports.rs"));

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bcrypt_raw_secret_derivation_and_destroy() {
        let wide=|v:&str|v.encode_utf16().chain(core::iter::once(0)).collect::<Vec<u16>>();
        let provider_name=wide("PBKDF2");
        let mut provider=ptr::null_mut();
        assert_eq!(unsafe{BCryptOpenAlgorithmProvider(&mut provider,provider_name.as_ptr(),ptr::null(),0)},STATUS_SUCCESS);
        let mut secret=ptr::null_mut();
        let source=b"shared secret material";
        assert_eq!(unsafe{BCryptGenerateSymmetricKey(provider,&mut secret,ptr::null_mut(),0,source.as_ptr() as *mut u8,source.len() as u32,0)},STATUS_SUCCESS);
        let kdf=wide("TRUNCATE");
        let mut output=[0u8;8]; let mut written=0;
        assert_eq!(unsafe{BCryptDeriveKey(secret,kdf.as_ptr(),ptr::null_mut(),output.as_mut_ptr(),output.len() as u32,&mut written,0)},STATUS_SUCCESS);
        assert_eq!(written,8); assert_eq!(&output,b"lairetam");
        assert_eq!(unsafe{BCryptDestroySecret(secret)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptDestroySecret(secret)},STATUS_INVALID_PARAMETER);
        assert_eq!(unsafe{BCryptCloseAlgorithmProvider(provider,0)},STATUS_SUCCESS);
    }

    #[test]
    fn bcrypt_key_derivation_pbkdf2_sha256_known_answer() {
        let wide=|v:&str|v.encode_utf16().chain(core::iter::once(0)).collect::<Vec<u16>>();
        let name=wide("PBKDF2");
        let mut provider=ptr::null_mut();
        assert_eq!(unsafe{BCryptOpenAlgorithmProvider(&mut provider,name.as_ptr(),ptr::null(),0)},STATUS_SUCCESS);
        let password=b"password";
        let mut key=ptr::null_mut();
        assert_eq!(unsafe{BCryptGenerateSymmetricKey(provider,&mut key,ptr::null_mut(),0,password.as_ptr() as *mut u8,password.len() as u32,0)},STATUS_SUCCESS);
        let hash=wide("SHA256");
        let salt=b"salt";
        let iterations:u64=4096;
        let mut buffers=[
            BcryptBuffer{cbBuffer:(hash.len()*2) as u32,BufferType:0,pvBuffer:hash.as_ptr() as *mut c_void},
            BcryptBuffer{cbBuffer:salt.len() as u32,BufferType:15,pvBuffer:salt.as_ptr() as *mut c_void},
            BcryptBuffer{cbBuffer:core::mem::size_of::<u64>() as u32,BufferType:16,pvBuffer:(&iterations as *const u64) as *mut c_void},
        ];
        let mut desc=BcryptBufferDesc{ulVersion:0,cBuffers:3,pBuffers:buffers.as_mut_ptr()};
        let mut output=[0u8;32];
        let mut written=0;
        assert_eq!(unsafe{BCryptKeyDerivation(key,&mut desc,output.as_mut_ptr(),32,&mut written,0)},STATUS_SUCCESS);
        assert_eq!(written,32);
        assert_eq!(output,[0xc5,0xe4,0x78,0xd5,0x92,0x88,0xc8,0x41,0xaa,0x53,0x0d,0xb6,0x84,0x5c,0x4c,0x8d,0x96,0x28,0x93,0xa0,0x01,0xce,0x4e,0x11,0xa4,0x96,0x38,0x73,0xaa,0x98,0x13,0x4a]);
        assert_eq!(unsafe{BCryptDestroyKey(key)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptCloseAlgorithmProvider(provider,0)},STATUS_SUCCESS);
    }

    #[test]
    fn bcrypt_direct_pbkdf2_uses_64_bit_iteration_abi() {
        let wide=|v:&str|v.encode_utf16().chain(core::iter::once(0)).collect::<Vec<u16>>();
        let name=wide("SHA256");
        let mut provider=ptr::null_mut();
        assert_eq!(unsafe{BCryptOpenAlgorithmProvider(&mut provider,name.as_ptr(),ptr::null(),BCRYPT_ALG_HANDLE_HMAC_FLAG)},STATUS_SUCCESS);
        let mut password=b"password".to_vec();
        let mut salt=b"salt".to_vec();
        let mut output=[0u8;32];
        assert_eq!(unsafe{BCryptDeriveKeyPBKDF2(provider,password.as_mut_ptr(),password.len() as ULONG,salt.as_mut_ptr(),salt.len() as ULONG,4096,output.as_mut_ptr(),output.len() as ULONG,0)},STATUS_SUCCESS);
        assert_eq!(output,[0xc5,0xe4,0x78,0xd5,0x92,0x88,0xc8,0x41,0xaa,0x53,0x0d,0xb6,0x84,0x5c,0x4c,0x8d,0x96,0x28,0x93,0xa0,0x01,0xce,0x4e,0x11,0xa4,0x96,0x38,0x73,0xaa,0x98,0x13,0x4a]);
        assert_eq!(unsafe{BCryptDeriveKeyPBKDF2(provider,password.as_mut_ptr(),password.len() as ULONG,salt.as_mut_ptr(),salt.len() as ULONG,u32::MAX as u64+1,output.as_mut_ptr(),output.len() as ULONG,0)},STATUS_INVALID_PARAMETER);
        assert_eq!(unsafe{BCryptCloseAlgorithmProvider(provider,0)},STATUS_SUCCESS);
    }

    #[test]
    fn sha256_known_answer() {
        let got = HashAlgorithm::Sha256.digest(b"abc");
        assert_eq!(got.as_ref(), &[0xba,0x78,0x16,0xbf,0x8f,0x01,0xcf,0xea,0x41,0x41,0x40,0xde,0x5d,0xae,0x22,0x23,0xb0,0x03,0x61,0xa3,0x96,0x17,0x7a,0x9c,0xb4,0x10,0xff,0x61,0xf2,0x00,0x15,0xad]);
    }
    #[test]
    fn hmac_sha256_known_answer() {
        let key = hmac::Key::new(hmac::HMAC_SHA256, b"key");
        let tag = hmac::sign(&key, b"The quick brown fox jumps over the lazy dog");
        assert_eq!(tag.as_ref(), &[0xf7,0xbc,0x83,0xf4,0x30,0x53,0x84,0x24,0xb1,0x32,0x98,0xe6,0xaa,0x6f,0xb1,0x43,0xef,0x4d,0x59,0xa1,0x49,0x46,0x17,0x59,0x97,0x47,0x9d,0xbc,0x2d,0x1a,0x3c,0xd8]);
    }
    #[test]
    fn pbkdf2_sha256_known_answer() {
        let mut out = [0u8; 32];
        pbkdf2::derive(pbkdf2::PBKDF2_HMAC_SHA256, NonZeroU32::new(1).unwrap(), b"salt", b"password", &mut out);
        assert_eq!(out, [0x12,0x0f,0xb6,0xcf,0xfc,0xf8,0xb3,0x2c,0x43,0xe7,0x22,0x52,0x56,0xc4,0xf8,0x37,0xa8,0x65,0x48,0xc9,0x2c,0xcc,0x35,0x48,0x08,0x05,0x98,0x7c,0xb7,0x0b,0xe1,0x7b]);
    }
    #[test]
    fn bcrypt_hash_one_shot_sha256_matches_known_answer() {
        let provider = register_handle(RegisteredHandle::Algorithm(AlgorithmHandle { algorithm: HashAlgorithm::Sha256, hmac: false, multi: false, cipher: false, gcm: false, cbc: false, kdf: false, rsa: false, rng: false })).unwrap();
        let input = b"abc";
        let mut output = [0u8; 32];
        let status = unsafe { BCryptHash(provider, ptr::null(), 0, input.as_ptr(), input.len() as u32, output.as_mut_ptr(), output.len() as u32) };
        assert_eq!(status, STATUS_SUCCESS);
        assert_eq!(output, [
            0xba,0x78,0x16,0xbf,0x8f,0x01,0xcf,0xea,0x41,0x41,0x40,0xde,0x5d,0xae,0x22,0x23,
            0xb0,0x03,0x61,0xa3,0x96,0x17,0x7a,0x9c,0xb4,0x10,0xff,0x61,0xf2,0x00,0x15,0xad
        ]);
        assert_eq!(unsafe { BCryptCloseAlgorithmProvider(provider, 0) }, STATUS_SUCCESS);
    }

    #[test]
    fn duplicate_hash_preserves_incremental_state() {
        let provider = register_handle(RegisteredHandle::Algorithm(AlgorithmHandle { algorithm: HashAlgorithm::Sha256, hmac: false, multi: false, cipher: false, kdf: false, rsa: false, rng: false, gcm: false, cbc: false })).unwrap();
        let mut original = ptr::null_mut();
        assert_eq!(unsafe { BCryptCreateHash(provider.cast(), &mut original, ptr::null_mut(), 0, ptr::null_mut(), 0, 0) }, STATUS_SUCCESS);
        assert_eq!(unsafe { BCryptHashData(original, b"ab".as_ptr(), 2, 0) }, STATUS_SUCCESS);
        let mut duplicate = ptr::null_mut();
        assert_eq!(unsafe { BCryptDuplicateHash(original, &mut duplicate, ptr::null_mut(), 0, 0) }, STATUS_SUCCESS);
        assert_eq!(unsafe { BCryptHashData(original, b"c".as_ptr(), 1, 0) }, STATUS_SUCCESS);
        assert_eq!(unsafe { BCryptHashData(duplicate, b"c".as_ptr(), 1, 0) }, STATUS_SUCCESS);
        let mut a = [0u8; 32];
        let mut b = [0u8; 32];
        assert_eq!(unsafe { BCryptFinishHash(original, a.as_mut_ptr(), 32, 0) }, STATUS_SUCCESS);
        assert_eq!(unsafe { BCryptFinishHash(duplicate, b.as_mut_ptr(), 32, 0) }, STATUS_SUCCESS);
        assert_eq!(a, b);
        assert_eq!(a, HashAlgorithm::Sha256.digest(b"abc").as_ref());
        assert_eq!(unsafe { BCryptDestroyHash(original) }, STATUS_SUCCESS);
        assert_eq!(unsafe { BCryptDestroyHash(duplicate) }, STATUS_SUCCESS);
        assert_eq!(unsafe { BCryptCloseAlgorithmProvider(provider.cast(), 0) }, STATUS_SUCCESS);
    }

    #[test]
    fn enum_algorithms_returns_only_implemented_classes_and_frees() {
        let mut count = 0;
        let mut list = ptr::null_mut();
        let status = unsafe { BCryptEnumAlgorithms(0x2 | 0x20, &mut count, &mut list, 0) };
        assert_eq!(status, STATUS_SUCCESS);
        assert_eq!(count, 4);
        let names: Vec<String> = (0..count as usize).map(|i| unsafe {
            let entry = &*list.add(i);
            let mut n = Vec::new();
            for j in 0..32 {
                let c = entry.name.add(j).read_unaligned();
                if c == 0 { break; }
                n.push(c);
            }
            String::from_utf16(&n).unwrap()
        }).collect();
        assert_eq!(names, ["SHA256", "SHA384", "SHA512", "RNG"]);
        unsafe { BCryptFreeBuffer(list.cast()) };
        assert_eq!(enum_buffers().lock().unwrap().len(), 0);
    }

    #[test]
    fn enum_providers_returns_single_provider_and_frees() {
        let algorithm = "SHA256".encode_utf16().chain(core::iter::once(0)).collect::<Vec<u16>>();
        let mut count = 0;
        let mut providers = ptr::null_mut();
        assert_eq!(unsafe { BCryptEnumProviders(algorithm.as_ptr(), &mut count, &mut providers, 0) }, STATUS_SUCCESS);
        assert_eq!(count, 1);
        let mut name = Vec::new();
        unsafe {
            for i in 0..64 {
                let unit = (*providers).name.add(i).read_unaligned();
                if unit == 0 { break; }
                name.push(unit);
            }
            BCryptFreeBuffer(providers.cast());
        }
        assert_eq!(String::from_utf16(&name).unwrap(), "Microsoft Primitive Provider");
    }

    #[test]
    fn rng_provider_and_system_preferred_rng_follow_cng_flags() {
        let wide = |v: &str| v.encode_utf16().chain(core::iter::once(0)).collect::<Vec<u16>>();
        let name = wide("RNG");
        let implementation = wide("Microsoft Primitive Provider");
        let mut provider = ptr::null_mut();
        assert_eq!(unsafe { BCryptOpenAlgorithmProvider(&mut provider, name.as_ptr(), implementation.as_ptr(), 0) }, STATUS_SUCCESS);
        let mut bytes = [0u8; 32];
        assert_eq!(unsafe { BCryptGenRandom(provider, bytes.as_mut_ptr(), bytes.len() as ULONG, 0) }, STATUS_SUCCESS);
        assert_eq!(unsafe { BCryptGenRandom(provider, bytes.as_mut_ptr(), bytes.len() as ULONG, BCRYPT_USE_SYSTEM_PREFERRED_RNG) }, STATUS_INVALID_PARAMETER);
        assert_eq!(unsafe { BCryptCloseAlgorithmProvider(provider, 0) }, STATUS_SUCCESS);
        assert_eq!(unsafe { BCryptGenRandom(ptr::null_mut(), bytes.as_mut_ptr(), bytes.len() as ULONG, BCRYPT_USE_SYSTEM_PREFERRED_RNG) }, STATUS_SUCCESS);
    }

    #[test]
    fn enum_algorithms_uses_windows_operation_mask_and_interface_classes() {
        let mut count = 0;
        let mut list = ptr::null_mut();
        assert_eq!(unsafe { BCryptEnumAlgorithms(1 | 2 | 4 | 32, &mut count, &mut list, 0) }, STATUS_SUCCESS);
        assert_eq!(count, 6);
        let expected = [("AES", 1), ("SHA256", 2), ("SHA384", 2), ("SHA512", 2), ("RSA", 3), ("RNG", 6)];
        for (i, (name, class)) in expected.iter().enumerate() {
            unsafe {
                let entry = &*list.add(i);
                assert_eq!(entry.class, *class);
                let mut units = Vec::new();
                for j in 0..32 {
                    let u = entry.name.add(j).read_unaligned();
                    if u == 0 { break; }
                    units.push(u);
                }
                assert_eq!(String::from_utf16(&units).unwrap(), *name);
            }
        }
        unsafe { BCryptFreeBuffer(list.cast()) };
        // KDFs and secret-agreement operations are not falsely advertised as
        // BCryptEnumAlgorithms operation classes.
        assert_eq!(unsafe { BCryptEnumAlgorithms(8 | 16, &mut count, &mut list, 0) }, STATUS_SUCCESS);
        assert_eq!(count, 0);
        assert!(list.is_null());
        assert_eq!(unsafe { BCryptEnumAlgorithms(0x40, &mut count, &mut list, 0) }, STATUS_SUCCESS);
        assert_eq!(count, 1);
        assert_eq!(unsafe { (*list).class }, 7);
        unsafe { BCryptFreeBuffer(list.cast()) };
    }

    #[test]
    fn cng_export_function_types_match_windows_x64_prototypes() {
        let _: unsafe extern "system" fn(
            *mut c_void, *mut u8, ULONG, *mut u8, ULONG, u64, *mut u8, ULONG, ULONG
        ) -> NTSTATUS = BCryptDeriveKeyPBKDF2;
        let _: unsafe extern "system" fn(
            *const u16, *mut ULONG, *mut *mut BcryptProvider, ULONG
        ) -> NTSTATUS = BCryptEnumProviders;
        let _: unsafe extern "system" fn(
            *mut c_void, *mut u8, ULONG, ULONG
        ) -> NTSTATUS = BCryptGenRandom;
        let _: unsafe extern "system" fn(*mut u8) -> NTSTATUS = BCryptGetFipsAlgorithmMode;
    }

    #[test]
    fn x64_cng_public_structures_match_windows_layouts() {
        assert_eq!(core::mem::size_of::<BcryptAlgorithmIdentifier>(), 16);
        assert_eq!(core::mem::size_of::<BcryptProvider>(), 8);
        assert_eq!(core::mem::size_of::<BcryptBuffer>(), 16);
        assert_eq!(core::mem::size_of::<BcryptBufferDesc>(), 16);
        assert_eq!(core::mem::size_of::<BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO>(), 88);
        assert_eq!(core::mem::size_of::<BcryptMultiHashOperation>(), 24);
        assert_eq!(core::mem::size_of::<BcryptRsaBlobHeader>(), 24);
    }

    #[test]
    fn free_buffer_ignores_unknown_pointer() {
        unsafe { BCryptFreeBuffer(1usize as *mut c_void) };
    }


    #[test]
    fn opaque_handles_reject_forged_stale_and_wrong_type_tokens() {
        assert_eq!(unsafe { BCryptCloseAlgorithmProvider(1usize as *mut c_void, 0) }, STATUS_INVALID_PARAMETER);
        let p = register_handle(RegisteredHandle::Algorithm(AlgorithmHandle { algorithm: HashAlgorithm::Sha256, hmac: false, multi: false, cipher: false, kdf: false, rsa: false, rng: false, gcm: false, cbc: false })).unwrap();
        assert_eq!(unsafe { BCryptDestroyHash(p) }, STATUS_INVALID_PARAMETER);
        assert_eq!(unsafe { BCryptCloseAlgorithmProvider(p, 0) }, STATUS_SUCCESS);
        assert_eq!(unsafe { BCryptCloseAlgorithmProvider(p, 0) }, STATUS_INVALID_PARAMETER);
    }

    #[test]
    fn aes128_gcm_nist_known_answer_and_authentication_failure() {
        let wide = |s: &str| s.encode_utf16().chain(core::iter::once(0)).collect::<Vec<u16>>();
        let aes_name = wide("AES");
        let mode_name = wide("ChainingMode");
        let mode = wide("ChainingModeGCM");
        let mut provider = ptr::null_mut();
        assert_eq!(unsafe { BCryptOpenAlgorithmProvider(&mut provider, aes_name.as_ptr(), ptr::null(), 0) }, STATUS_SUCCESS);
        let mode_bytes = unsafe { core::slice::from_raw_parts(mode.as_ptr().cast::<u8>(), mode.len()*2) };
        assert_eq!(unsafe { BCryptSetProperty(provider, mode_name.as_ptr(), mode_bytes.as_ptr(), mode_bytes.len() as u32, 0) }, STATUS_SUCCESS);
        let key_bytes = [0u8; 16];
        let mut key = ptr::null_mut();
        assert_eq!(unsafe { BCryptGenerateSymmetricKey(provider, &mut key, ptr::null_mut(), 0, key_bytes.as_ptr() as *mut u8, 16, 0) }, STATUS_SUCCESS);
        let nonce = [0u8; 12];
        let mut tag = [0u8; 16];
        let mut info = BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO {
            cbSize: core::mem::size_of::<BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO>() as u32,
            dwInfoVersion: 1, pbNonce: nonce.as_ptr() as *mut u8, cbNonce: 12,
            pbAuthData: ptr::null_mut(), cbAuthData: 0, pbTag: tag.as_mut_ptr(), cbTag: 16,
            pbMacContext: ptr::null_mut(), cbMacContext: 0, cbAAD: 0, cbData: 0, dwFlags: 0,
        };
        let mut plain = [0u8; 16];
        let mut cipher = [0u8; 16];
        let mut written = 0;
        assert_eq!(unsafe { BCryptEncrypt(key, plain.as_mut_ptr(), 16, (&mut info as *mut BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO).cast::<c_void>(), ptr::null_mut(), 0, cipher.as_mut_ptr(), 16, &mut written, 0) }, STATUS_SUCCESS);
        assert_eq!(written, 16);
        assert_eq!(cipher, [0x03,0x88,0xda,0xce,0x60,0xb6,0xa3,0x92,0xf3,0x28,0xc2,0xb9,0x71,0xb2,0xfe,0x78]);
        assert_eq!(tag, [0xab,0x6e,0x47,0xd4,0x2c,0xec,0x13,0xbd,0xf5,0x3a,0x67,0xb2,0x12,0x57,0xbd,0xdf]);
        let mut recovered = [0u8; 16];
        assert_eq!(unsafe { BCryptDecrypt(key, cipher.as_mut_ptr(), 16, (&mut info as *mut BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO).cast::<c_void>(), ptr::null_mut(), 0, recovered.as_mut_ptr(), 16, &mut written, 0) }, STATUS_SUCCESS);
        assert_eq!(recovered, plain);
        tag[0] ^= 1;
        let mut rejected = [0u8; 16];
        assert_eq!(unsafe { BCryptDecrypt(key, cipher.as_mut_ptr(), 16, (&mut info as *mut BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO).cast::<c_void>(), ptr::null_mut(), 0, rejected.as_mut_ptr(), 16, &mut written, 0) }, STATUS_AUTH_TAG_MISMATCH);
        assert_eq!(unsafe { BCryptDestroyKey(key) }, STATUS_SUCCESS);
        assert_eq!(unsafe { BCryptCloseAlgorithmProvider(provider, 0) }, STATUS_SUCCESS);
    }

    #[test]
    fn aes192_gcm_nist_known_answer_and_roundtrip() {
        let wide = |s: &str| s.encode_utf16().chain(core::iter::once(0)).collect::<Vec<u16>>();
        let aes_name = wide("AES");
        let mode_name = wide("ChainingMode");
        let mode = wide("ChainingModeGCM");
        let mut provider = ptr::null_mut();
        assert_eq!(unsafe { BCryptOpenAlgorithmProvider(&mut provider, aes_name.as_ptr(), ptr::null(), 0) }, STATUS_SUCCESS);
        let mode_bytes = unsafe { core::slice::from_raw_parts(mode.as_ptr().cast::<u8>(), mode.len()*2) };
        assert_eq!(unsafe { BCryptSetProperty(provider, mode_name.as_ptr(), mode_bytes.as_ptr(), mode_bytes.len() as u32, 0) }, STATUS_SUCCESS);
        let key_bytes = [0u8; 24];
        let mut key = ptr::null_mut();
        assert_eq!(unsafe { BCryptGenerateSymmetricKey(provider, &mut key, ptr::null_mut(), 0, key_bytes.as_ptr() as *mut u8, 24, 0) }, STATUS_SUCCESS);
        let nonce = [0u8; 12];
        let mut tag = [0u8; 16];
        let mut info = BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO {
            cbSize: core::mem::size_of::<BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO>() as u32,
            dwInfoVersion: 1, pbNonce: nonce.as_ptr() as *mut u8, cbNonce: 12,
            pbAuthData: ptr::null_mut(), cbAuthData: 0, pbTag: tag.as_mut_ptr(), cbTag: 16,
            pbMacContext: ptr::null_mut(), cbMacContext: 0, cbAAD: 0, cbData: 0, dwFlags: 0,
        };
        let mut plain = [0u8; 16];
        let mut cipher = [0u8; 16];
        let mut written = 0;
        assert_eq!(unsafe { BCryptEncrypt(key, plain.as_mut_ptr(), 16, (&mut info as *mut BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO).cast::<c_void>(), ptr::null_mut(), 0, cipher.as_mut_ptr(), 16, &mut written, 0) }, STATUS_SUCCESS);
        assert_eq!(cipher, [0x98,0xe7,0x24,0x7c,0x07,0xf0,0xfe,0x41,0x1c,0x26,0x7e,0x43,0x84,0xb0,0xf6,0x00]);
        assert_eq!(tag, [0x2f,0xf5,0x8d,0x80,0x03,0x39,0x27,0xab,0x8e,0xf4,0xd4,0x58,0x75,0x14,0xf0,0xfb]);
        let mut recovered = [0u8; 16];
        assert_eq!(unsafe { BCryptDecrypt(key, cipher.as_mut_ptr(), 16, (&mut info as *mut BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO).cast::<c_void>(), ptr::null_mut(), 0, recovered.as_mut_ptr(), 16, &mut written, 0) }, STATUS_SUCCESS);
        assert_eq!(recovered, plain);
        assert_eq!(unsafe { BCryptDestroyKey(key) }, STATUS_SUCCESS);
        assert_eq!(unsafe { BCryptCloseAlgorithmProvider(provider, 0) }, STATUS_SUCCESS);
    }

    #[test]
    fn aes_cbc_nist_known_answer_and_pkcs7_roundtrip() {
        let key=[0x2b,0x7e,0x15,0x16,0x28,0xae,0xd2,0xa6,0xab,0xf7,0x15,0x88,0x09,0xcf,0x4f,0x3c];
        let mut iv=[0x00,0x01,0x02,0x03,0x04,0x05,0x06,0x07,0x08,0x09,0x0a,0x0b,0x0c,0x0d,0x0e,0x0f];
        let plain=[0x6b,0xc1,0xbe,0xe2,0x2e,0x40,0x9f,0x96,0xe9,0x3d,0x7e,0x11,0x73,0x93,0x17,0x2a];
        let encrypted=aes_cbc_crypt(&key,&plain,&mut iv,true,false).unwrap();
        assert_eq!(encrypted,[0x76,0x49,0xab,0xac,0x81,0x19,0xb2,0x46,0xce,0xe9,0x8e,0x9b,0x12,0xe9,0x19,0x7d]);
        let mut decrypt_iv=[0x00,0x01,0x02,0x03,0x04,0x05,0x06,0x07,0x08,0x09,0x0a,0x0b,0x0c,0x0d,0x0e,0x0f];
        assert_eq!(aes_cbc_crypt(&key,&encrypted,&mut decrypt_iv,false,false).unwrap(),plain);
        let mut pad_iv=[0u8;16];let padded=aes_cbc_crypt(&key,b"cng padding",&mut pad_iv,true,true).unwrap();
        let mut unpad_iv=[0u8;16];assert_eq!(aes_cbc_crypt(&key,&padded,&mut unpad_iv,false,true).unwrap(),b"cng padding");
    }

    #[test]
    fn symmetric_key_duplicate_and_key_data_blob_roundtrip() {
        let wide = |s:&str| s.encode_utf16().chain(core::iter::once(0)).collect::<Vec<u16>>();
        let aes_name=wide("AES"); let mode_name=wide("ChainingMode"); let cbc_name=wide("ChainingModeCBC"); let blob_name=wide("KeyDataBlob");
        let mut provider=ptr::null_mut();
        assert_eq!(unsafe{BCryptOpenAlgorithmProvider(&mut provider,aes_name.as_ptr(),ptr::null(),0)},STATUS_SUCCESS);
        let mode_bytes=unsafe{core::slice::from_raw_parts(cbc_name.as_ptr().cast::<u8>(),cbc_name.len()*2)};
        assert_eq!(unsafe{BCryptSetProperty(provider,mode_name.as_ptr(),mode_bytes.as_ptr(),mode_bytes.len() as u32,0)},STATUS_SUCCESS);
        let raw=[0x11u8;16]; let mut original=ptr::null_mut();
        assert_eq!(unsafe{BCryptGenerateSymmetricKey(provider,&mut original,ptr::null_mut(),0,raw.as_ptr() as *mut u8,16,0)},STATUS_SUCCESS);
        let mut duplicate=ptr::null_mut();
        assert_eq!(unsafe{BCryptDuplicateKey(original,&mut duplicate,ptr::null_mut(),0,0)},STATUS_SUCCESS);
        let mut needed=0;
        assert_eq!(unsafe{BCryptExportKey(original,ptr::null_mut(),blob_name.as_ptr(),ptr::null_mut(),0,&mut needed,0)},STATUS_BUFFER_TOO_SMALL);
        let mut blob=vec![0u8;needed as usize];
        assert_eq!(unsafe{BCryptExportKey(original,ptr::null_mut(),blob_name.as_ptr(),blob.as_mut_ptr(),needed,&mut needed,0)},STATUS_SUCCESS);
        let mut imported=ptr::null_mut();
        assert_eq!(unsafe{BCryptImportKey(provider,ptr::null_mut(),blob_name.as_ptr(),&mut imported,ptr::null_mut(),0,blob.as_mut_ptr(),blob.len() as u32,0)},STATUS_SUCCESS);
        let mut exported=vec![0u8;blob.len()]; let mut exported_len=0;
        assert_eq!(unsafe{BCryptExportKey(imported,ptr::null_mut(),blob_name.as_ptr(),exported.as_mut_ptr(),exported.len() as u32,&mut exported_len,0)},STATUS_SUCCESS);
        assert_eq!(blob,exported);
        assert_eq!(unsafe{BCryptDestroyKey(imported)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptDestroyKey(duplicate)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptDestroyKey(original)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptCloseAlgorithmProvider(provider,0)},STATUS_SUCCESS);
    }

    #[test]
    fn key_data_blob_import_rejects_bad_magic_version_length_and_key_size() {
        let wide=|s:&str|s.encode_utf16().chain(core::iter::once(0)).collect::<Vec<u16>>();
        let aes=wide("AES");let mode_name=wide("ChainingMode");let mode=wide("ChainingModeCBC");let blob_name=wide("KeyDataBlob");
        let mut provider=ptr::null_mut();
        assert_eq!(unsafe{BCryptOpenAlgorithmProvider(&mut provider,aes.as_ptr(),ptr::null(),0)},STATUS_SUCCESS);
        let mb=unsafe{core::slice::from_raw_parts(mode.as_ptr().cast::<u8>(),mode.len()*2)};
        assert_eq!(unsafe{BCryptSetProperty(provider,mode_name.as_ptr(),mb.as_ptr(),mb.len() as u32,0)},STATUS_SUCCESS);
        let mut bytes=Vec::new();
        bytes.extend_from_slice(&BCRYPT_KEY_DATA_BLOB_MAGIC.to_ne_bytes());
        bytes.extend_from_slice(&BCRYPT_KEY_DATA_BLOB_VERSION1.to_ne_bytes());
        bytes.extend_from_slice(&16u32.to_ne_bytes());bytes.extend_from_slice(&[0x22;16]);
        let mut imported=ptr::null_mut();
        let mut bad=bytes.clone();bad[0]^=1;
        assert_eq!(unsafe{BCryptImportKey(provider,ptr::null_mut(),blob_name.as_ptr(),&mut imported,ptr::null_mut(),0,bad.as_mut_ptr(),bad.len() as u32,0)},STATUS_INVALID_PARAMETER);
        let mut bad=bytes.clone();bad[4]=2;
        assert_eq!(unsafe{BCryptImportKey(provider,ptr::null_mut(),blob_name.as_ptr(),&mut imported,ptr::null_mut(),0,bad.as_mut_ptr(),bad.len() as u32,0)},STATUS_INVALID_PARAMETER);
        let mut bad=bytes.clone();bad[8]=15;
        assert_eq!(unsafe{BCryptImportKey(provider,ptr::null_mut(),blob_name.as_ptr(),&mut imported,ptr::null_mut(),0,bad.as_mut_ptr(),bad.len() as u32,0)},STATUS_INVALID_PARAMETER);
        assert_eq!(unsafe{BCryptImportKey(provider,ptr::null_mut(),blob_name.as_ptr(),&mut imported,ptr::null_mut(),0,bytes.as_mut_ptr(),(bytes.len()-1) as u32,0)},STATUS_INVALID_PARAMETER);
        assert_eq!(unsafe{BCryptCloseAlgorithmProvider(provider,0)},STATUS_SUCCESS);
    }

    #[test]
    fn key_data_blob_roundtrips_all_aes_key_sizes() {
        let wide=|v:&str|v.encode_utf16().chain(core::iter::once(0)).collect::<Vec<u16>>();
        let aes=wide("AES");let mode_name=wide("ChainingMode");let mode=wide("ChainingModeGCM");let blob_name=wide("KeyDataBlob");
        let mut provider=ptr::null_mut();
        assert_eq!(unsafe{BCryptOpenAlgorithmProvider(&mut provider,aes.as_ptr(),ptr::null(),0)},STATUS_SUCCESS);
        let mb=unsafe{core::slice::from_raw_parts(mode.as_ptr().cast::<u8>(),mode.len()*2)};
        assert_eq!(unsafe{BCryptSetProperty(provider,mode_name.as_ptr(),mb.as_ptr(),mb.len() as u32,0)},STATUS_SUCCESS);
        for key_len in [16usize,24,32] {
            let raw=vec![key_len as u8;key_len];let mut key=ptr::null_mut();
            assert_eq!(unsafe{BCryptGenerateSymmetricKey(provider,&mut key,ptr::null_mut(),0,raw.as_ptr() as *mut u8,key_len as u32,0)},STATUS_SUCCESS);
            let mut needed=0;
            assert_eq!(unsafe{BCryptExportKey(key,ptr::null_mut(),blob_name.as_ptr(),ptr::null_mut(),0,&mut needed,0)},STATUS_BUFFER_TOO_SMALL);
            let mut blob=vec![0u8;needed as usize];
            assert_eq!(unsafe{BCryptExportKey(key,ptr::null_mut(),blob_name.as_ptr(),blob.as_mut_ptr(),needed,&mut needed,0)},STATUS_SUCCESS);
            let mut restored=ptr::null_mut();
            assert_eq!(unsafe{BCryptImportKey(provider,ptr::null_mut(),blob_name.as_ptr(),&mut restored,ptr::null_mut(),0,blob.as_mut_ptr(),blob.len() as u32,0)},STATUS_SUCCESS);
            let prop=wide("KeyLength");let mut bits=0u32;let mut result_len=0;
            assert_eq!(unsafe{BCryptGetProperty(restored,prop.as_ptr(),(&mut bits as *mut u32).cast(),4,&mut result_len,0)},STATUS_SUCCESS);
            assert_eq!(bits,(key_len*8) as u32);
            assert_eq!(unsafe{BCryptDestroyKey(restored)},STATUS_SUCCESS);
            assert_eq!(unsafe{BCryptDestroyKey(key)},STATUS_SUCCESS);
        }
        assert_eq!(unsafe{BCryptCloseAlgorithmProvider(provider,0)},STATUS_SUCCESS);
    }


    #[test]
    fn multi_hash_processes_independent_hashes_and_reuses_states() {
        let wide=|v:&str|v.encode_utf16().chain(core::iter::once(0)).collect::<Vec<u16>>();
        let name=wide("SHA256");
        let mut provider=ptr::null_mut();
        assert_eq!(unsafe{BCryptOpenAlgorithmProvider(&mut provider,name.as_ptr(),ptr::null(),BCRYPT_MULTI_FLAG)},STATUS_SUCCESS);
        let mut multi=ptr::null_mut();
        assert_eq!(unsafe{BCryptCreateMultiHash(provider,&mut multi,2,ptr::null_mut(),0,ptr::null_mut(),0,0)},STATUS_SUCCESS);
        let mut a=[0u8;32];let mut b=[0u8;32];
        let mut ops=[
            BcryptMultiHashOperation{iHash:0,hashOperation:1,pbBuffer:b"abc".as_ptr() as *mut u8,cbBuffer:3},
            BcryptMultiHashOperation{iHash:1,hashOperation:1,pbBuffer:b"xyz".as_ptr() as *mut u8,cbBuffer:3},
            BcryptMultiHashOperation{iHash:0,hashOperation:2,pbBuffer:a.as_mut_ptr(),cbBuffer:32},
            BcryptMultiHashOperation{iHash:1,hashOperation:2,pbBuffer:b.as_mut_ptr(),cbBuffer:32},
        ];
        assert_eq!(unsafe{BCryptProcessMultiOperations(multi,1,ops.as_mut_ptr(),core::mem::size_of_val(&ops) as u32,0)},STATUS_SUCCESS);
        assert_eq!(a,HashAlgorithm::Sha256.digest(b"abc").as_ref());
        assert_eq!(b,HashAlgorithm::Sha256.digest(b"xyz").as_ref());
        let mut reused=[0u8;32];
        let mut again=[
            BcryptMultiHashOperation{iHash:0,hashOperation:1,pbBuffer:b"abc".as_ptr() as *mut u8,cbBuffer:3},
            BcryptMultiHashOperation{iHash:0,hashOperation:2,pbBuffer:reused.as_mut_ptr(),cbBuffer:32},
        ];
        assert_eq!(unsafe{BCryptProcessMultiOperations(multi,1,again.as_mut_ptr(),core::mem::size_of_val(&again) as u32,0)},STATUS_SUCCESS);
        assert_eq!(reused,a);
        assert_eq!(unsafe{BCryptDestroyHash(multi)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptCloseAlgorithmProvider(provider,0)},STATUS_SUCCESS);
    }

    #[test]
    fn multi_hash_rejects_invalid_batch_before_mutating_state() {
        let wide=|v:&str|v.encode_utf16().chain(core::iter::once(0)).collect::<Vec<u16>>();
        let name=wide("SHA256");let mut provider=ptr::null_mut();
        assert_eq!(unsafe{BCryptOpenAlgorithmProvider(&mut provider,name.as_ptr(),ptr::null(),BCRYPT_MULTI_FLAG)},STATUS_SUCCESS);
        let mut multi=ptr::null_mut();
        assert_eq!(unsafe{BCryptCreateMultiHash(provider,&mut multi,1,ptr::null_mut(),0,ptr::null_mut(),0,0)},STATUS_SUCCESS);
        let mut out=[0u8;32];
        let mut invalid=[
            BcryptMultiHashOperation{iHash:0,hashOperation:1,pbBuffer:b"bad".as_ptr() as *mut u8,cbBuffer:3},
            BcryptMultiHashOperation{iHash:0,hashOperation:99,pbBuffer:ptr::null_mut(),cbBuffer:0},
        ];
        assert_eq!(unsafe{BCryptProcessMultiOperations(multi,1,invalid.as_mut_ptr(),core::mem::size_of_val(&invalid) as u32,0)},STATUS_INVALID_PARAMETER);
        let mut valid=[
            BcryptMultiHashOperation{iHash:0,hashOperation:1,pbBuffer:b"abc".as_ptr() as *mut u8,cbBuffer:3},
            BcryptMultiHashOperation{iHash:0,hashOperation:2,pbBuffer:out.as_mut_ptr(),cbBuffer:32},
        ];
        assert_eq!(unsafe{BCryptProcessMultiOperations(multi,1,valid.as_mut_ptr(),core::mem::size_of_val(&valid) as u32,0)},STATUS_SUCCESS);
        assert_eq!(out,HashAlgorithm::Sha256.digest(b"abc").as_ref());
        assert_eq!(unsafe{BCryptDestroyHash(multi)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptCloseAlgorithmProvider(provider,0)},STATUS_SUCCESS);
    }


    #[test]
    fn multi_hash_supports_hmac_secret_for_all_hash_states() {
        let wide=|v:&str|v.encode_utf16().chain(core::iter::once(0)).collect::<Vec<u16>>();
        let name=wide("SHA256");let mut provider=ptr::null_mut();
        assert_eq!(unsafe{BCryptOpenAlgorithmProvider(&mut provider,name.as_ptr(),ptr::null(),BCRYPT_MULTI_FLAG|BCRYPT_ALG_HANDLE_HMAC_FLAG)},STATUS_SUCCESS);
        let secret=b"multi-hmac-secret";let mut multi=ptr::null_mut();
        assert_eq!(unsafe{BCryptCreateMultiHash(provider,&mut multi,2,ptr::null_mut(),0,secret.as_ptr() as *mut u8,secret.len() as u32,0)},STATUS_SUCCESS);
        let mut a=[0u8;32];let mut b=[0u8;32];
        let mut ops=[
            BcryptMultiHashOperation{iHash:0,hashOperation:1,pbBuffer:b"message-a".as_ptr() as *mut u8,cbBuffer:9},
            BcryptMultiHashOperation{iHash:1,hashOperation:1,pbBuffer:b"message-b".as_ptr() as *mut u8,cbBuffer:9},
            BcryptMultiHashOperation{iHash:0,hashOperation:2,pbBuffer:a.as_mut_ptr(),cbBuffer:32},
            BcryptMultiHashOperation{iHash:1,hashOperation:2,pbBuffer:b.as_mut_ptr(),cbBuffer:32},
        ];
        assert_eq!(unsafe{BCryptProcessMultiOperations(multi,1,ops.as_mut_ptr(),core::mem::size_of_val(&ops) as u32,0)},STATUS_SUCCESS);
        let k=hmac::Key::new(hmac::HMAC_SHA256,secret);
        assert_eq!(a,hmac::sign(&k,b"message-a").as_ref());
        assert_eq!(b,hmac::sign(&k,b"message-b").as_ref());
        assert_eq!(unsafe{BCryptDestroyHash(multi)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptCloseAlgorithmProvider(provider,0)},STATUS_SUCCESS);
    }

    #[test]
    fn supported_algorithms_have_expected_sizes() { assert_eq!(HashAlgorithm::Sha256.digest_len(),32); assert_eq!(HashAlgorithm::Sha384.digest_len(),48); assert_eq!(HashAlgorithm::Sha512.digest_len(),64); }
}
