//! CognacOS bcrypt.dll: Windows CNG-compatible hashing, KDF, AES, RNG and bounded RSA subset.
//! All unimplemented Windows 11 exports return STATUS_NOT_IMPLEMENTED.
#![allow(non_snake_case)]
#![deny(unsafe_op_in_unsafe_fn)]

use aws_lc_rs::{digest, hmac, pbkdf2, rand as aws_rand};
use rsa::{pkcs1v15::{Signature as RsaSignature, VerifyingKey, SigningKey}, signature::{hazmat::{PrehashVerifier, PrehashSigner}, SignatureEncoding}, traits::PublicKeyParts, BigUint, RsaPublicKey, RsaPrivateKey};
use rand::rngs::OsRng;
use sha2::{Sha256, Sha384, Sha512};
use aes::{Aes128, Aes192, Aes256};
use aes_gcm::{aead::AeadInPlace, AesGcm};
use aes_gcm::aead::generic_array::typenum::{U1, U2, U3, U4, U5, U6, U7, U8, U9, U10, U11, U12, U13, U14, U15, U16};
use cipher::{BlockDecrypt, BlockEncrypt, KeyInit, generic_array::GenericArray};
use core::num::NonZeroU32;
use zeroize::Zeroize;
use core::{ffi::c_void, ptr};
use std::{boxed::Box, collections::{HashMap,HashSet}, sync::{Mutex, OnceLock}, vec::Vec};
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
const BCRYPT_HASH_REUSABLE_FLAG: ULONG = 0x00000020;
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
struct AlgorithmHandle { algorithm: HashAlgorithm, hmac: bool, multi: bool, reusable: bool, cipher: bool, kdf: bool, rsa: bool, rng: bool, gcm: bool, cbc: bool }
#[derive(Clone)]
struct KeyHandle { key: Vec<u8>, gcm: bool, cbc: bool }
#[derive(Clone)]
struct KdfKeyHandle { secret: Vec<u8> }
impl Drop for KdfKeyHandle { fn drop(&mut self) { self.secret.zeroize(); } }
impl Drop for KeyHandle { fn drop(&mut self) { self.key.zeroize(); } }
#[derive(Clone)]
struct RsaPrivateHandle { key: RsaPrivateKey, finalized: bool }
#[derive(Clone)]
enum RegisteredHandle { Algorithm(AlgorithmHandle), Hash(HashHandle), MultiHash(MultiHashHandle), Key(KeyHandle), KdfKey(KdfKeyHandle), RsaPublic(RsaPublicKey), RsaPrivate(RsaPrivateHandle) }
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
        (Some(RegisteredHandle::Key(_)), 2) | (Some(RegisteredHandle::RsaPublic(_)), 2) |
        (Some(RegisteredHandle::KdfKey(_)), 3) | (Some(RegisteredHandle::KdfKey(_)), 2) | (Some(RegisteredHandle::RsaPrivate(_)), 2));
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
fn rsa_public_handle(token: *mut c_void) -> Result<RsaPublicKey, NTSTATUS> {
    if token.is_null() { return Err(STATUS_INVALID_PARAMETER); }
    match handle_table().lock().map_err(|_| STATUS_NO_MEMORY)?.get(&(token as usize)) {
        Some(RegisteredHandle::RsaPublic(v)) => Ok(v.clone()),
        Some(RegisteredHandle::RsaPrivate(v)) if v.finalized => Ok(v.key.to_public_key()),
        _ => Err(STATUS_INVALID_PARAMETER),
    }
}

#[derive(Clone)]
struct HashHandle { algorithm: HashAlgorithm, hmac: bool, reusable: bool, finished: bool, secret: Vec<u8>, data: Vec<u8> }
#[derive(Clone)]
struct MultiHashHandle { states: Vec<HashHandle> }
impl Drop for HashHandle { fn drop(&mut self) { self.secret.zeroize(); self.data.zeroize(); } }

#[inline]
fn valid_object_buffer(object:*mut u8,object_len:ULONG)->bool {
    object.is_null()==(object_len==0)
}

unsafe fn write_u32(out: *mut u8, value: u32) { unsafe { ptr::copy_nonoverlapping(value.to_ne_bytes().as_ptr(), out, 4) }; }
unsafe fn write_wide(out: *mut u8, value: &str) { for (i, unit) in value.encode_utf16().chain(core::iter::once(0)).enumerate() { unsafe { ptr::write_unaligned(out.add(i * 2).cast::<u16>(), unit) }; } }
unsafe fn read_property_name(ptr: *const u16) -> Result<String, NTSTATUS> {
    if ptr.is_null() { return Err(STATUS_INVALID_PARAMETER); }
    let mut units = Vec::new();
    for i in 0..128 { let u = unsafe { ptr.add(i).read() }; if u == 0 { return String::from_utf16(&units).map_err(|_| STATUS_INVALID_PARAMETER); } units.push(u); }
    Err(STATUS_INVALID_PARAMETER)
}

/// Opens the implemented SHA-2, AES, PBKDF2, RSA and RNG providers.
#[no_mangle]
pub unsafe extern "system" fn BCryptOpenAlgorithmProvider(out: *mut *mut c_void, alg_id: *const u16, implementation: *const u16, flags: ULONG) -> NTSTATUS {
    if out.is_null() || flags & !(BCRYPT_ALG_HANDLE_HMAC_FLAG | BCRYPT_MULTI_FLAG | BCRYPT_HASH_REUSABLE_FLAG) != 0 { return STATUS_INVALID_PARAMETER; }
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
    if flags&BCRYPT_HASH_REUSABLE_FLAG!=0 && (cipher||kdf||rsa_provider||rng_provider) { return STATUS_NOT_SUPPORTED; }
    let token=match register_handle(RegisteredHandle::Algorithm(AlgorithmHandle{algorithm,hmac:flags&BCRYPT_ALG_HANDLE_HMAC_FLAG!=0,multi:flags&BCRYPT_MULTI_FLAG!=0,reusable:flags&BCRYPT_HASH_REUSABLE_FLAG!=0,cipher,kdf,rsa:rsa_provider,rng:rng_provider,gcm:false,cbc:cipher})) {Ok(v)=>v,Err(e)=>return e};
    unsafe{out.write(token)}; STATUS_SUCCESS
}


#[no_mangle]
pub unsafe extern "system" fn BCryptCloseAlgorithmProvider(handle:*mut c_void,flags:ULONG)->NTSTATUS {
 if flags!=0{return STATUS_INVALID_PARAMETER;} remove_handle(handle,0)
}


#[no_mangle]
pub unsafe extern "system" fn BCryptCreateHash(algorithm:*mut c_void,out:*mut *mut c_void,object:*mut u8,object_len:ULONG,secret:*mut u8,secret_len:ULONG,flags:ULONG)->NTSTATUS {
 if out.is_null()||!valid_object_buffer(object,object_len)||(secret.is_null()&&secret_len!=0)||flags&!BCRYPT_HASH_REUSABLE_FLAG!=0{return STATUS_INVALID_PARAMETER;}
 let p=match algorithm_handle(algorithm){Ok(v)=>v,Err(e)=>return e}; if p.cipher||p.kdf||p.rsa||p.rng{return STATUS_INVALID_PARAMETER;}
 let reusable=flags&BCRYPT_HASH_REUSABLE_FLAG!=0;
 if reusable&&!p.reusable{return STATUS_NOT_SUPPORTED;}
 if !p.hmac&&secret_len!=0{return STATUS_INVALID_PARAMETER;}
 let key=if secret_len==0{Vec::new()}else{unsafe{core::slice::from_raw_parts(secret,secret_len as usize)}.to_vec()};
 let h=HashHandle{algorithm:p.algorithm,hmac:p.hmac,reusable,finished:false,secret:key,data:Vec::new()};
 let token=match register_handle(RegisteredHandle::Hash(h)){Ok(v)=>v,Err(e)=>return e}; unsafe{out.write(token)};STATUS_SUCCESS
}


#[no_mangle]
pub unsafe extern "system" fn BCryptHashData(hash:*mut c_void,input:*const u8,input_len:ULONG,flags:ULONG)->NTSTATUS {
 if (input.is_null()&&input_len!=0)||flags!=0{return STATUS_INVALID_PARAMETER;}
 let Ok(mut table)=handle_table().lock() else{return STATUS_NO_MEMORY;};
 let Some(RegisteredHandle::Hash(state))=table.get_mut(&(hash as usize)) else{return STATUS_INVALID_PARAMETER;};
 if state.finished || state.data.len().checked_add(input_len as usize).is_none(){return STATUS_INVALID_PARAMETER;}
 if input_len!=0 {if state.data.try_reserve(input_len as usize).is_err(){return STATUS_NO_MEMORY;} state.data.extend_from_slice(unsafe{core::slice::from_raw_parts(input,input_len as usize)});}
 STATUS_SUCCESS
}


#[no_mangle]
pub unsafe extern "system" fn BCryptFinishHash(hash:*mut c_void,output:*mut u8,output_len:ULONG,flags:ULONG)->NTSTATUS {
 if output.is_null()||flags!=0{return STATUS_INVALID_PARAMETER;}
 let Ok(mut table)=handle_table().lock()else{return STATUS_NO_MEMORY;};
 let Some(RegisteredHandle::Hash(state))=table.get_mut(&(hash as usize))else{return STATUS_INVALID_PARAMETER;};
 if state.finished{return STATUS_INVALID_PARAMETER;}
 if output_len!=state.algorithm.digest_len(){return STATUS_INVALID_PARAMETER;}
 if state.hmac {
     let key=hmac::Key::new(state.algorithm.hmac_algorithm(),&state.secret);
     let digest=hmac::sign(&key,&state.data);
     unsafe{ptr::copy_nonoverlapping(digest.as_ref().as_ptr(),output,digest.as_ref().len())};
 } else {
     let digest=state.algorithm.digest(&state.data);
     unsafe{ptr::copy_nonoverlapping(digest.as_ref().as_ptr(),output,digest.as_ref().len())};
 }
 if state.reusable {state.data.zeroize();state.data.clear();} else {state.finished=true;}
 STATUS_SUCCESS
}


#[no_mangle]
pub unsafe extern "system" fn BCryptDuplicateHash(hash:*mut c_void,out:*mut *mut c_void,object:*mut u8,object_len:ULONG,flags:ULONG)->NTSTATUS {
 if out.is_null()||!valid_object_buffer(object,object_len)||flags!=0{return STATUS_INVALID_PARAMETER;}let state=match hash_handle(hash){Ok(v)=>v,Err(e)=>return e};
 if state.finished{return STATUS_INVALID_PARAMETER;}
 let token=match register_handle(RegisteredHandle::Hash(state)){Ok(v)=>v,Err(e)=>return e};unsafe{out.write(token)};STATUS_SUCCESS
}



#[repr(C)]
#[derive(Clone, Copy)]
struct BcryptMultiHashOperation { iHash: ULONG, hashOperation: ULONG, pbBuffer: *mut u8, cbBuffer: ULONG }

/// Creates a bounded multi-hash object. Caller-owned object buffers are not
/// accepted yet; the state is held in the opaque handle table.
#[no_mangle]
pub unsafe extern "system" fn BCryptCreateMultiHash(algorithm:*mut c_void,out:*mut *mut c_void,count:ULONG,object:*mut u8,object_len:ULONG,secret:*mut u8,secret_len:ULONG,flags:ULONG)->NTSTATUS {
    if out.is_null() || count==0 || count>4096 || !valid_object_buffer(object,object_len) || (secret.is_null()&&secret_len!=0) || flags!=0 { return STATUS_INVALID_PARAMETER; }
    let provider=match algorithm_handle(algorithm){Ok(v)=>v,Err(e)=>return e};
    if provider.cipher || !provider.multi || (!provider.hmac&&secret_len!=0) { return STATUS_INVALID_PARAMETER; }
    let key=if secret_len==0 { Vec::new() } else { unsafe{core::slice::from_raw_parts(secret,secret_len as usize)}.to_vec() };
    let mut states=Vec::new();
    if states.try_reserve_exact(count as usize).is_err(){return STATUS_NO_MEMORY;}
    for _ in 0..count { states.push(HashHandle{algorithm:provider.algorithm,hmac:provider.hmac,reusable:true,finished:false,secret:key.clone(),data:Vec::new()}); }
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
    let mut additional=Vec::<usize>::new();
    if additional.try_reserve_exact(state.states.len()).is_err(){return STATUS_NO_MEMORY;}
    additional.resize(state.states.len(),0);
    let mut finalized:Vec<bool>=state.states.iter().map(|h|h.finished).collect();
    for i in 0..count {
        let op=unsafe{operations.add(i).read_unaligned()};
        if op.iHash as usize>=state.states.len() || (op.pbBuffer.is_null()&&op.cbBuffer!=0) {return STATUS_INVALID_PARAMETER;}
        let index=op.iHash as usize;
        match op.hashOperation {
            OP_HASH_DATA => {
                if finalized[index]{return STATUS_INVALID_PARAMETER;}
                let slot=&mut additional[index];
                *slot=match slot.checked_add(op.cbBuffer as usize){Some(v)=>v,None=>return STATUS_INVALID_PARAMETER};
                if state.states[index].data.len().checked_add(*slot).is_none(){return STATUS_INVALID_PARAMETER;}
            },
            OP_FINISH_HASH => {
                if finalized[index]{return STATUS_INVALID_PARAMETER;}
                if op.pbBuffer.is_null()||op.cbBuffer!=state.states[index].algorithm.digest_len(){return STATUS_INVALID_PARAMETER;}
                if !state.states[index].reusable{finalized[index]=true;}
            },
            _=>return STATUS_INVALID_PARAMETER,
        }
    }
    // Reserve every affected stream before applying any operation, so an
    // allocation failure cannot leave a partially processed operation batch.
    for (hash,extra) in state.states.iter_mut().zip(additional.iter()) {
        if *extra!=0 && hash.data.try_reserve(*extra).is_err(){return STATUS_NO_MEMORY;}
    }
    for i in 0..count {
        let op=unsafe{operations.add(i).read_unaligned()};
        let hash=&mut state.states[op.iHash as usize];
        if op.hashOperation==OP_HASH_DATA {
            if op.cbBuffer!=0 {
                hash.data.extend_from_slice(unsafe{core::slice::from_raw_parts(op.pbBuffer,op.cbBuffer as usize)});
            }
        } else {
            let digest_bytes:Vec<u8>=if hash.hmac {
                let key=hmac::Key::new(hash.algorithm.hmac_algorithm(),&hash.secret);
                hmac::sign(&key,&hash.data).as_ref().to_vec()
            } else { hash.algorithm.digest(&hash.data).as_ref().to_vec() };
            unsafe{ptr::copy_nonoverlapping(digest_bytes.as_ptr(),op.pbBuffer,digest_bytes.len())};
            hash.data.zeroize();hash.data.clear();
            if !hash.reusable{hash.finished=true;}
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
    match aws_rand::fill(unsafe { core::slice::from_raw_parts_mut(buffer, length as usize) }) { Ok(()) => STATUS_SUCCESS, Err(_) => STATUS_NOT_SUPPORTED }
}

#[repr(C)]
#[derive(Clone,Copy)]
struct BcryptKeyLengths { dwMinLength:ULONG, dwMaxLength:ULONG, dwIncrement:ULONG }

#[no_mangle]
pub unsafe extern "system" fn BCryptGetProperty(object:*mut c_void,property:*const u16,output:*mut u8,output_len:ULONG,result_len:*mut ULONG,flags:ULONG)->NTSTATUS {
    if property.is_null() || result_len.is_null() || flags != 0 { return STATUS_INVALID_PARAMETER; }
    let name = match unsafe { read_property_name(property) } { Ok(v) => v, Err(e) => return e };
    if name=="KeyLengths" {
        if let Ok(provider)=algorithm_handle(object) {
            let lengths=if provider.cipher {Some(BcryptKeyLengths{dwMinLength:128,dwMaxLength:256,dwIncrement:64})}
                else if provider.rsa {Some(BcryptKeyLengths{dwMinLength:512,dwMaxLength:16384,dwIncrement:64})}
                else {None};
            if let Some(lengths)=lengths {
                let required=core::mem::size_of::<BcryptKeyLengths>() as ULONG;
                unsafe{result_len.write(required)};
                if output.is_null(){return STATUS_SUCCESS;}
                if output_len<required{return STATUS_BUFFER_TOO_SMALL;}
                unsafe{ptr::write_unaligned(output.cast::<BcryptKeyLengths>(),lengths)};
                return STATUS_SUCCESS;
            }
        }
    }
    if name=="AuthTagLength" {
        let is_gcm=key_handle(object).map(|k|k.gcm).unwrap_or_else(|_|algorithm_handle(object).map(|p|p.cipher&&p.gcm).unwrap_or(false));
        if !is_gcm{return STATUS_NOT_SUPPORTED;}
        let lengths=BcryptKeyLengths{dwMinLength:12,dwMaxLength:16,dwIncrement:1};
        let required=core::mem::size_of::<BcryptKeyLengths>() as ULONG;
        unsafe{result_len.write(required)};
        if output.is_null(){return STATUS_SUCCESS;}
        if output_len<required{return STATUS_BUFFER_TOO_SMALL;}
        unsafe{ptr::write_unaligned(output.cast::<BcryptKeyLengths>(),lengths)};
        return STATUS_SUCCESS;
    }
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
    } else if let Ok(k) = rsa_public_handle(object) {
        let bits=k.n().bits() as u32;
        match name.as_str() {
            "ObjectLength" => (0, 0, ""),
            "KeyLength" | "PublicKeyLength" => (0, bits, ""),
            "AlgorithmName" => (1, 0, "RSA"),
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
        } else if p.kdf {
            match name.as_str() {
                "ObjectLength" => (0, 0, ""),
                "AlgorithmName" => (1, 0, "PBKDF2"),
                _ => return STATUS_NOT_SUPPORTED,
            }
        } else if p.rsa {
            match name.as_str() {
                "ObjectLength" => (0, 0, ""),
                "AlgorithmName" => (1, 0, "RSA"),
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

/// Enumerates the algorithm classes currently implemented by this provider.
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
 let Ok(mut t)=handle_table().lock()else{return STATUS_NO_MEMORY;};
 match t.get_mut(&(object as usize)) {
     Some(RegisteredHandle::Algorithm(a)) if a.cipher=>{a.gcm=is_gcm;a.cbc=is_cbc;STATUS_SUCCESS},
     Some(RegisteredHandle::Key(k))=>{k.gcm=is_gcm;k.cbc=is_cbc;STATUS_SUCCESS},
     _=>STATUS_INVALID_PARAMETER,
 }
}

#[repr(C)]
pub struct BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO {
    pub cbSize: ULONG, pub dwInfoVersion: ULONG, pub pbNonce: *mut u8, pub cbNonce: ULONG,
    pub pbAuthData: *mut u8, pub cbAuthData: ULONG, pub pbTag: *mut u8, pub cbTag: ULONG,
    pub pbMacContext: *mut u8, pub cbMacContext: ULONG, pub cbAAD: ULONG, pub cbData: u64, pub dwFlags: ULONG,
}

#[no_mangle]
pub unsafe extern "system" fn BCryptGenerateSymmetricKey(algorithm:*mut c_void,out:*mut *mut c_void,object:*mut u8,object_len:ULONG,secret:*mut u8,secret_len:ULONG,flags:ULONG)->NTSTATUS {
 if out.is_null()||!valid_object_buffer(object,object_len)||(secret.is_null()&&secret_len!=0)||flags!=0{return STATUS_INVALID_PARAMETER;}
 let p=match algorithm_handle(algorithm){Ok(v)=>v,Err(e)=>return e};
 if p.kdf {
     if secret_len>1024*1024{return STATUS_INVALID_PARAMETER;}
     let secret=if secret_len==0{Vec::new()}else{unsafe{core::slice::from_raw_parts(secret,secret_len as usize)}.to_vec()};
     let token=match register_handle(RegisteredHandle::KdfKey(KdfKeyHandle{secret})){Ok(v)=>v,Err(e)=>return e};
     unsafe{out.write(token)};return STATUS_SUCCESS;
 }
 if secret_len==0{return STATUS_INVALID_PARAMETER;}
 if !p.cipher||!(p.gcm && (secret_len==16||secret_len==24||secret_len==32) || p.cbc && (secret_len==16||secret_len==24||secret_len==32)){return STATUS_NOT_SUPPORTED;}
 let key=unsafe{core::slice::from_raw_parts(secret,secret_len as usize)}.to_vec();
 let token=match register_handle(RegisteredHandle::Key(KeyHandle{key,gcm:p.gcm,cbc:p.cbc})){Ok(v)=>v,Err(e)=>return e};
 unsafe{out.write(token)};STATUS_SUCCESS
}
macro_rules! gcm_encrypt_tag {
    ($aes:ty,$nonce_type:ty,$tag:ty,$key:expr,$nonce:expr,$aad:expr,$buffer:expr) => {{
        type G=AesGcm<$aes,$nonce_type,$tag>;
        let cipher=match G::new_from_slice($key){Ok(v)=>v,Err(_)=>return STATUS_INVALID_PARAMETER};
        let nonce=aes_gcm::Nonce::<$nonce_type>::from_slice($nonce);
        match cipher.encrypt_in_place_detached(nonce,$aad.as_slice(),$buffer) {
            Ok(value)=>Ok(value.to_vec()),Err(_)=>Err(STATUS_INVALID_PARAMETER)
        }
    }};
}
macro_rules! gcm_decrypt_tag {
    ($aes:ty,$nonce_type:ty,$tag:ty,$key:expr,$nonce:expr,$aad:expr,$buffer:expr,$tag_bytes:expr) => {{
        type G=AesGcm<$aes,$nonce_type,$tag>;
        let cipher=match G::new_from_slice($key){Ok(v)=>v,Err(_)=>return STATUS_INVALID_PARAMETER};
        let nonce=aes_gcm::Nonce::<$nonce_type>::from_slice($nonce);
        let tag=aes_gcm::Tag::<$tag>::from_slice($tag_bytes);
        cipher.decrypt_in_place_detached(nonce,$aad.as_slice(),$buffer,tag).is_ok()
    }};
}
macro_rules! gcm_encrypt_for_nonce {
    ($aes:ty,$nonce_type:ty,$key:expr,$nonce:expr,$aad:expr,$buffer:expr,$tag_len:expr) => {{
        match $tag_len {
            12=>gcm_encrypt_tag!($aes,$nonce_type,U12,$key,$nonce,$aad,$buffer),
            13=>gcm_encrypt_tag!($aes,$nonce_type,U13,$key,$nonce,$aad,$buffer),
            14=>gcm_encrypt_tag!($aes,$nonce_type,U14,$key,$nonce,$aad,$buffer),
            15=>gcm_encrypt_tag!($aes,$nonce_type,U15,$key,$nonce,$aad,$buffer),
            16=>gcm_encrypt_tag!($aes,$nonce_type,U16,$key,$nonce,$aad,$buffer),
            _=>Err(STATUS_INVALID_PARAMETER),
        }
    }};
}
macro_rules! gcm_decrypt_for_nonce {
    ($aes:ty,$nonce_type:ty,$key:expr,$nonce:expr,$aad:expr,$buffer:expr,$tag_bytes:expr,$tag_len:expr) => {{
        match $tag_len {
            12=>gcm_decrypt_tag!($aes,$nonce_type,U12,$key,$nonce,$aad,$buffer,$tag_bytes),
            13=>gcm_decrypt_tag!($aes,$nonce_type,U13,$key,$nonce,$aad,$buffer,$tag_bytes),
            14=>gcm_decrypt_tag!($aes,$nonce_type,U14,$key,$nonce,$aad,$buffer,$tag_bytes),
            15=>gcm_decrypt_tag!($aes,$nonce_type,U15,$key,$nonce,$aad,$buffer,$tag_bytes),
            16=>gcm_decrypt_tag!($aes,$nonce_type,U16,$key,$nonce,$aad,$buffer,$tag_bytes),
            _=>false,
        }
    }};
}
macro_rules! gcm_encrypt_key {
    ($aes:ty,$key:expr,$nonce:expr,$aad:expr,$buffer:expr,$nonce_len:expr,$tag_len:expr) => {{
        match $nonce_len {
            1=>gcm_encrypt_for_nonce!($aes,U1,$key,$nonce,$aad,$buffer,$tag_len),
            2=>gcm_encrypt_for_nonce!($aes,U2,$key,$nonce,$aad,$buffer,$tag_len),
            3=>gcm_encrypt_for_nonce!($aes,U3,$key,$nonce,$aad,$buffer,$tag_len),
            4=>gcm_encrypt_for_nonce!($aes,U4,$key,$nonce,$aad,$buffer,$tag_len),
            5=>gcm_encrypt_for_nonce!($aes,U5,$key,$nonce,$aad,$buffer,$tag_len),
            6=>gcm_encrypt_for_nonce!($aes,U6,$key,$nonce,$aad,$buffer,$tag_len),
            7=>gcm_encrypt_for_nonce!($aes,U7,$key,$nonce,$aad,$buffer,$tag_len),
            8=>gcm_encrypt_for_nonce!($aes,U8,$key,$nonce,$aad,$buffer,$tag_len),
            9=>gcm_encrypt_for_nonce!($aes,U9,$key,$nonce,$aad,$buffer,$tag_len),
            10=>gcm_encrypt_for_nonce!($aes,U10,$key,$nonce,$aad,$buffer,$tag_len),
            11=>gcm_encrypt_for_nonce!($aes,U11,$key,$nonce,$aad,$buffer,$tag_len),
            12=>gcm_encrypt_for_nonce!($aes,U12,$key,$nonce,$aad,$buffer,$tag_len),
            13=>gcm_encrypt_for_nonce!($aes,U13,$key,$nonce,$aad,$buffer,$tag_len),
            14=>gcm_encrypt_for_nonce!($aes,U14,$key,$nonce,$aad,$buffer,$tag_len),
            15=>gcm_encrypt_for_nonce!($aes,U15,$key,$nonce,$aad,$buffer,$tag_len),
            16=>gcm_encrypt_for_nonce!($aes,U16,$key,$nonce,$aad,$buffer,$tag_len),
            _=>Err(STATUS_INVALID_PARAMETER),
        }
    }};
}
macro_rules! gcm_decrypt_key {
    ($aes:ty,$key:expr,$nonce:expr,$aad:expr,$buffer:expr,$tag_bytes:expr,$nonce_len:expr,$tag_len:expr) => {{
        match $nonce_len {
            1=>gcm_decrypt_for_nonce!($aes,U1,$key,$nonce,$aad,$buffer,$tag_bytes,$tag_len),
            2=>gcm_decrypt_for_nonce!($aes,U2,$key,$nonce,$aad,$buffer,$tag_bytes,$tag_len),
            3=>gcm_decrypt_for_nonce!($aes,U3,$key,$nonce,$aad,$buffer,$tag_bytes,$tag_len),
            4=>gcm_decrypt_for_nonce!($aes,U4,$key,$nonce,$aad,$buffer,$tag_bytes,$tag_len),
            5=>gcm_decrypt_for_nonce!($aes,U5,$key,$nonce,$aad,$buffer,$tag_bytes,$tag_len),
            6=>gcm_decrypt_for_nonce!($aes,U6,$key,$nonce,$aad,$buffer,$tag_bytes,$tag_len),
            7=>gcm_decrypt_for_nonce!($aes,U7,$key,$nonce,$aad,$buffer,$tag_bytes,$tag_len),
            8=>gcm_decrypt_for_nonce!($aes,U8,$key,$nonce,$aad,$buffer,$tag_bytes,$tag_len),
            9=>gcm_decrypt_for_nonce!($aes,U9,$key,$nonce,$aad,$buffer,$tag_bytes,$tag_len),
            10=>gcm_decrypt_for_nonce!($aes,U10,$key,$nonce,$aad,$buffer,$tag_bytes,$tag_len),
            11=>gcm_decrypt_for_nonce!($aes,U11,$key,$nonce,$aad,$buffer,$tag_bytes,$tag_len),
            12=>gcm_decrypt_for_nonce!($aes,U12,$key,$nonce,$aad,$buffer,$tag_bytes,$tag_len),
            13=>gcm_decrypt_for_nonce!($aes,U13,$key,$nonce,$aad,$buffer,$tag_bytes,$tag_len),
            14=>gcm_decrypt_for_nonce!($aes,U14,$key,$nonce,$aad,$buffer,$tag_bytes,$tag_len),
            15=>gcm_decrypt_for_nonce!($aes,U15,$key,$nonce,$aad,$buffer,$tag_bytes,$tag_len),
            16=>gcm_decrypt_for_nonce!($aes,U16,$key,$nonce,$aad,$buffer,$tag_bytes,$tag_len),
            _=>false,
        }
    }};
}

unsafe fn auth_info(info:*mut c_void)->Result<(Vec<u8>,Vec<u8>,*mut u8,usize),NTSTATUS>{
 if info.is_null(){return Err(STATUS_INVALID_PARAMETER);}let i=unsafe{&mut *info.cast::<BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO>()};
 if i.cbSize as usize!=core::mem::size_of::<BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO>()||i.dwInfoVersion!=1||i.pbNonce.is_null()||!(1..=16).contains(&i.cbNonce)||i.pbTag.is_null()||!(12..=16).contains(&i.cbTag)||(i.pbAuthData.is_null()&&i.cbAuthData!=0)||!i.pbMacContext.is_null()||i.cbMacContext!=0||i.dwFlags!=0{return Err(STATUS_INVALID_PARAMETER);}
 let nonce=unsafe{core::slice::from_raw_parts(i.pbNonce,i.cbNonce as usize)}.to_vec();let aad=if i.cbAuthData==0{Vec::new()}else{unsafe{core::slice::from_raw_parts(i.pbAuthData,i.cbAuthData as usize)}.to_vec()};Ok((nonce,aad,i.pbTag,i.cbTag as usize))
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
 if result_len.is_null()||(input.is_null()&&input_len!=0)||flags!=0||!iv.is_null()||iv_len!=0{return STATUS_INVALID_PARAMETER;}let k=match key_handle(key){Ok(v)=>v,Err(e)=>return e};if !k.gcm{return STATUS_NOT_SUPPORTED;}let(nonce,aad,tag,tag_len)=match unsafe{auth_info(padding_info)}{Ok(v)=>v,Err(e)=>return e};unsafe{result_len.write(input_len)};if (output.is_null()&&input_len!=0)||output_len<input_len{return STATUS_BUFFER_TOO_SMALL;}
 let mut buf=if input_len==0{Vec::new()}else{unsafe{core::slice::from_raw_parts(input,input_len as usize)}.to_vec()};
 let tag_value=match k.key.len() {
     16=>gcm_encrypt_key!(Aes128,&k.key,&nonce,&aad,&mut buf,nonce.len(),tag_len),
     24=>gcm_encrypt_key!(Aes192,&k.key,&nonce,&aad,&mut buf,nonce.len(),tag_len),
     32=>gcm_encrypt_key!(Aes256,&k.key,&nonce,&aad,&mut buf,nonce.len(),tag_len),
     _=>return STATUS_INVALID_PARAMETER,
 };
 let tag_value=match tag_value{Ok(v)=>v,Err(e)=>return e};
 unsafe{if input_len!=0{ptr::copy_nonoverlapping(buf.as_ptr(),output,input_len as usize);}ptr::copy_nonoverlapping(tag_value.as_ptr(),tag,tag_len);let info=&mut *padding_info.cast::<BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO>();info.cbAAD=info.cbAuthData;info.cbData=input_len as u64};STATUS_SUCCESS
}
#[no_mangle]
pub unsafe extern "system" fn BCryptDecrypt(key:*mut c_void,input:*mut u8,input_len:ULONG,padding_info:*mut c_void,iv:*mut u8,iv_len:ULONG,output:*mut u8,output_len:ULONG,result_len:*mut ULONG,flags:ULONG)->NTSTATUS {
 if padding_info.is_null(){return unsafe{bcrypt_cbc(key,input,input_len,iv,iv_len,output,output_len,result_len,flags,false)};}
 if result_len.is_null()||(input.is_null()&&input_len!=0)||flags!=0||!iv.is_null()||iv_len!=0{return STATUS_INVALID_PARAMETER;}let k=match key_handle(key){Ok(v)=>v,Err(e)=>return e};if !k.gcm{return STATUS_NOT_SUPPORTED;}let(nonce,aad,tag,tag_len)=match unsafe{auth_info(padding_info)}{Ok(v)=>v,Err(e)=>return e};unsafe{result_len.write(input_len)};if (output.is_null()&&input_len!=0)||output_len<input_len{return STATUS_BUFFER_TOO_SMALL;}
 let mut buf=if input_len==0{Vec::new()}else{unsafe{core::slice::from_raw_parts(input,input_len as usize)}.to_vec()};
 let tagbytes=unsafe{core::slice::from_raw_parts(tag,tag_len)};
 let valid=match k.key.len() {
     16=>gcm_decrypt_key!(Aes128,&k.key,&nonce,&aad,&mut buf,tagbytes,nonce.len(),tag_len),
     24=>gcm_decrypt_key!(Aes192,&k.key,&nonce,&aad,&mut buf,tagbytes,nonce.len(),tag_len),
     32=>gcm_decrypt_key!(Aes256,&k.key,&nonce,&aad,&mut buf,tagbytes,nonce.len(),tag_len),
     _=>return STATUS_INVALID_PARAMETER,
 };
 if !valid{return STATUS_AUTH_TAG_MISMATCH;}
 unsafe{if !buf.is_empty(){ptr::copy_nonoverlapping(buf.as_ptr(),output,buf.len());}result_len.write(buf.len() as ULONG);let info=&mut *padding_info.cast::<BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO>();info.cbAAD=info.cbAuthData;info.cbData=buf.len() as u64};STATUS_SUCCESS
}
const BCRYPT_KEY_DATA_BLOB_MAGIC: ULONG = 0x4d42_444b;
const BCRYPT_KEY_DATA_BLOB_VERSION1: ULONG = 1;
const BCRYPT_KEY_DATA_BLOB: &str = "KeyDataBlob";

#[repr(C)]
#[derive(Clone, Copy)]
struct BcryptKeyDataBlobHeader { dwMagic: ULONG, dwVersion: ULONG, cbKeyData: ULONG }

#[no_mangle]
pub unsafe extern "system" fn BCryptDuplicateKey(key:*mut c_void,out:*mut *mut c_void,object:*mut u8,object_len:ULONG,flags:ULONG)->NTSTATUS {
    if out.is_null() || !valid_object_buffer(object,object_len) || flags != 0 || key.is_null() { return STATUS_INVALID_PARAMETER; }
    let duplicate=match handle_table().lock() {
        Ok(table)=>match table.get(&(key as usize)) {
            Some(RegisteredHandle::Key(v))=>RegisteredHandle::Key(v.clone()),
            Some(RegisteredHandle::KdfKey(v))=>RegisteredHandle::KdfKey(v.clone()),
            Some(RegisteredHandle::RsaPublic(v))=>RegisteredHandle::RsaPublic(v.clone()),
            Some(RegisteredHandle::RsaPrivate(v))=>RegisteredHandle::RsaPrivate(v.clone()),
            _=>return STATUS_INVALID_PARAMETER,
        },
        Err(_)=>return STATUS_NO_MEMORY,
    };
    match register_handle(duplicate) { Ok(token) => { unsafe { out.write(token) }; STATUS_SUCCESS }, Err(e) => e }
}

#[no_mangle]
pub unsafe extern "system" fn BCryptImportKey(algorithm:*mut c_void,import_key:*mut c_void,blob_type:*const u16,out:*mut *mut c_void,object:*mut u8,object_len:ULONG,input:*mut u8,input_len:ULONG,flags:ULONG)->NTSTATUS {
    if out.is_null() || blob_type.is_null() || !import_key.is_null() || !valid_object_buffer(object,object_len) || input.is_null() || flags != 0 { return STATUS_INVALID_PARAMETER; }
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
    let mut iteration_seen=false;
    let mut iterations:u64=10_000;
    for i in 0..desc.cBuffers as usize {
        let b=unsafe{ptr::read_unaligned(desc.pBuffers.add(i))};
        if b.pvBuffer.is_null() && b.cbBuffer!=0 { return STATUS_INVALID_PARAMETER; }
        match b.BufferType {
            KDF_HASH_ALGORITHM => {
                if algorithm.is_some() || b.pvBuffer.is_null() || b.cbBuffer<2 || b.cbBuffer%2!=0 { return STATUS_INVALID_PARAMETER; }
                let units=unsafe{core::slice::from_raw_parts(b.pvBuffer.cast::<u16>(),(b.cbBuffer/2) as usize)};
                let Some(end)=units.iter().position(|u|*u==0) else{return STATUS_INVALID_PARAMETER;};
                if end+1!=units.len(){return STATUS_INVALID_PARAMETER;}
                let name=match String::from_utf16(&units[..end]) { Ok(v)=>v,Err(_)=>return STATUS_INVALID_PARAMETER };
                algorithm=Some(match name.as_str() {"SHA256"=>HashAlgorithm::Sha256,"SHA384"=>HashAlgorithm::Sha384,"SHA512"=>HashAlgorithm::Sha512,_=>return STATUS_NOT_SUPPORTED});
            }
            KDF_SALT | KDF_GENERIC_PARAMETER => {
                if salt.is_some() || b.cbBuffer>1024*1024 { return STATUS_INVALID_PARAMETER; }
                salt=Some(if b.cbBuffer==0 {Vec::new()} else {unsafe{core::slice::from_raw_parts(b.pvBuffer.cast::<u8>(),b.cbBuffer as usize)}.to_vec()});
            }
            KDF_ITERATION_COUNT => {
                if iteration_seen || b.pvBuffer.is_null() || b.cbBuffer!=8 { return STATUS_INVALID_PARAMETER; }
                iteration_seen=true;
                iterations=unsafe{ptr::read_unaligned(b.pvBuffer.cast::<u64>())};
            }
            _ => return STATUS_NOT_SUPPORTED,
        }
    }
    let Some(alg)=algorithm else{return STATUS_INVALID_PARAMETER};
    let salt=salt.unwrap_or_default();
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
/// Creates an unfinalized RSA key pair. The private material remains in a
/// zeroizing backend-owned key object; caller-supplied key-object storage is not
/// used by this implementation.
#[no_mangle]
pub unsafe extern "system" fn BCryptGenerateKeyPair(algorithm:*mut c_void,out:*mut *mut c_void,bits:ULONG,flags:ULONG)->NTSTATUS {
    if out.is_null() || flags!=0 || !(512..=16384).contains(&bits) || bits%8!=0 { return STATUS_INVALID_PARAMETER; }
    let provider=match algorithm_handle(algorithm){Ok(v)=>v,Err(e)=>return e};
    if !provider.rsa { return STATUS_NOT_SUPPORTED; }
    let private=match RsaPrivateKey::new(&mut OsRng,bits as usize){Ok(v)=>v,Err(_)=>return STATUS_NO_MEMORY};
    let token=match register_handle(RegisteredHandle::RsaPrivate(RsaPrivateHandle{key:private,finalized:false})){Ok(v)=>v,Err(e)=>return e};
    unsafe{out.write(token)};STATUS_SUCCESS
}

/// Completes a generated RSA key pair. RSA generation in this provider creates
/// a mathematically complete backend key immediately; this call seals the CNG
/// lifecycle and rejects duplicate finalization.
#[no_mangle]
pub unsafe extern "system" fn BCryptFinalizeKeyPair(key:*mut c_void,flags:ULONG)->NTSTATUS {
    if flags!=0 || key.is_null(){return STATUS_INVALID_PARAMETER;}
    let mut table=match handle_table().lock(){Ok(v)=>v,Err(_)=>return STATUS_NO_MEMORY};
    match table.get_mut(&(key as usize)){
        Some(RegisteredHandle::RsaPrivate(state)) if !state.finalized=>{state.finalized=true;STATUS_SUCCESS},
        _=>STATUS_INVALID_PARAMETER,
    }
}

/// Signs a precomputed SHA-2 digest using RSA PKCS#1 v1.5.
#[no_mangle]
pub unsafe extern "system" fn BCryptSignHash(key:*mut c_void,padding_info:*mut c_void,input:*mut u8,input_len:ULONG,output:*mut u8,output_len:ULONG,result_len:*mut ULONG,flags:ULONG)->NTSTATUS {
    const BCRYPT_PAD_PKCS1:ULONG=0x00000002;
    if result_len.is_null() || input.is_null() || flags!=BCRYPT_PAD_PKCS1 || padding_info.is_null(){return STATUS_INVALID_PARAMETER;}
    let info=unsafe{&*padding_info.cast::<BcryptPkcs1PaddingInfo>()};
    let alg=match unsafe{HashAlgorithm::from_utf16(info.pszAlgId)}{Ok(v)=>v,Err(e)=>return e};
    if input_len!=alg.digest_len(){return STATUS_INVALID_PARAMETER;}
    let private=match handle_table().lock(){
        Ok(t)=>match t.get(&(key as usize)){Some(RegisteredHandle::RsaPrivate(v))=>v.clone(),_=>return STATUS_INVALID_PARAMETER},
        Err(_)=>return STATUS_NO_MEMORY,
    };
    let required=private.key.size() as ULONG;
    unsafe{result_len.write(required)};
    if output.is_null(){return STATUS_SUCCESS;}
    if output_len<required{return STATUS_BUFFER_TOO_SMALL;}
    let digest=unsafe{core::slice::from_raw_parts(input,input_len as usize)};
    let signature=match alg{
        HashAlgorithm::Sha256=>match SigningKey::<Sha256>::new(private.key).sign_prehash(digest){Ok(v)=>v.to_bytes().to_vec(),Err(_)=>return STATUS_INVALID_PARAMETER},
        HashAlgorithm::Sha384=>match SigningKey::<Sha384>::new(private.key).sign_prehash(digest){Ok(v)=>v.to_vec(),Err(_)=>return STATUS_INVALID_PARAMETER},
        HashAlgorithm::Sha512=>match SigningKey::<Sha512>::new(private.key).sign_prehash(digest){Ok(v)=>v.to_vec(),Err(_)=>return STATUS_INVALID_PARAMETER},
    };
    unsafe{ptr::copy_nonoverlapping(signature.as_ptr(),output,signature.len())};STATUS_SUCCESS
}

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

#[repr(C)]
struct BcryptPkcs1PaddingInfo { pszAlgId: *const u16 }
#[cfg(target_pointer_width = "64")]
const _: [(); 8] = [(); core::mem::size_of::<BcryptPkcs1PaddingInfo>()];

/// CAPI-compatible hash-to-key derivation helper. It supports SHA-2 hash
/// handles and emits the requested number of bytes using the legacy CryptoAPI
/// ipad/opad expansion for output longer than one digest block.
#[no_mangle]
pub unsafe extern "system" fn BCryptDeriveKeyCapi(hash:*mut c_void,target_alg:*mut c_void,output:*mut u8,output_len:ULONG,flags:ULONG)->NTSTATUS {
    if output.is_null() || output_len==0 || flags!=0 {return STATUS_INVALID_PARAMETER;}
    // The target algorithm is an ALG_ID encoded in the historical handle slot;
    // this primitive does not inspect provider state through that value.
    let state=match hash_handle(hash){Ok(v)=>v,Err(e)=>return e};
    if state.finished{return STATUS_INVALID_PARAMETER;}
    let digest=if state.hmac{
        let key=hmac::Key::new(state.algorithm.hmac_algorithm(),&state.secret);
        hmac::sign(&key,&state.data).as_ref().to_vec()
    }else{state.algorithm.digest(&state.data).as_ref().to_vec()};
    if output_len as usize>digest.len().saturating_mul(2){return STATUS_INVALID_PARAMETER;}
    let mut material=Vec::with_capacity(output_len as usize);
    material.extend_from_slice(&digest);
    if output_len as usize>digest.len(){
        let block_len=state.algorithm.block_len() as usize;
        let mut inner=vec![0x36u8;block_len];let mut outer=vec![0x5cu8;block_len];
        for (i,b) in digest.iter().enumerate(){inner[i]^=*b;outer[i]^=*b;}
        let expanded=match state.algorithm {
            HashAlgorithm::Sha256=>{
                let a=aws_lc_rs::digest::digest(&aws_lc_rs::digest::SHA256,&[inner.as_slice(),digest.as_slice()].concat());
                let b=aws_lc_rs::digest::digest(&aws_lc_rs::digest::SHA256,&[outer.as_slice(),a.as_ref()].concat());
                [a.as_ref(),b.as_ref()].concat()
            },
            HashAlgorithm::Sha384=>{
                let a=aws_lc_rs::digest::digest(&aws_lc_rs::digest::SHA384,&[inner.as_slice(),digest.as_slice()].concat());
                let b=aws_lc_rs::digest::digest(&aws_lc_rs::digest::SHA384,&[outer.as_slice(),a.as_ref()].concat());
                [a.as_ref(),b.as_ref()].concat()
            },
            HashAlgorithm::Sha512=>{
                let a=aws_lc_rs::digest::digest(&aws_lc_rs::digest::SHA512,&[inner.as_slice(),digest.as_slice()].concat());
                let b=aws_lc_rs::digest::digest(&aws_lc_rs::digest::SHA512,&[outer.as_slice(),a.as_ref()].concat());
                [a.as_ref(),b.as_ref()].concat()
            },
        };
        material=expanded;
    }
    unsafe{ptr::copy_nonoverlapping(material.as_ptr(),output,output_len as usize)};
    let _=target_alg;
    STATUS_SUCCESS
}

/// Verifies a precomputed SHA-2 digest using the Windows PKCS#1 padding-info ABI.
/// The padding descriptor is
/// mandatory, as it is for BCryptVerifySignature callers using PKCS#1.
#[no_mangle]
pub unsafe extern "system" fn BCryptVerifySignature(key:*mut c_void,padding_info:*mut c_void,hash:*const u8,hash_len:ULONG,signature:*const u8,signature_len:ULONG,flags:ULONG)->NTSTATUS {
    const BCRYPT_PAD_PKCS1:ULONG=0x00000002;
    if padding_info.is_null() || hash.is_null() || signature.is_null() || flags!=BCRYPT_PAD_PKCS1 {return STATUS_INVALID_PARAMETER;}
    let info=unsafe{&*padding_info.cast::<BcryptPkcs1PaddingInfo>()};
    let alg=match unsafe{HashAlgorithm::from_utf16(info.pszAlgId)}{Ok(v)=>v,Err(e)=>return e};
    let expected_len=alg.digest_len();
    if hash_len!=expected_len{return STATUS_INVALID_PARAMETER;}
    let public=match rsa_public_handle(key){Ok(v)=>v,Err(e)=>return e};
    if signature_len as usize!=public.size(){return STATUS_INVALID_PARAMETER;}
    let sig=match RsaSignature::try_from(unsafe{core::slice::from_raw_parts(signature,signature_len as usize)}){Ok(v)=>v,Err(_)=>return STATUS_INVALID_PARAMETER};
    let digest=unsafe{core::slice::from_raw_parts(hash,hash_len as usize)};
    let valid=match alg {
        HashAlgorithm::Sha256=>VerifyingKey::<Sha256>::new(public).verify_prehash(digest,&sig).is_ok(),
        HashAlgorithm::Sha384=>VerifyingKey::<Sha384>::new(public).verify_prehash(digest,&sig).is_ok(),
        HashAlgorithm::Sha512=>VerifyingKey::<Sha512>::new(public).verify_prehash(digest,&sig).is_ok(),
    };
    if valid{STATUS_SUCCESS}else{STATUS_INVALID_SIGNATURE}
}




#[repr(C)] pub struct CryptProviderRef{pub dwInterface:ULONG,pub pszFunction:*mut u16,pub pszProvider:*mut u16,pub cProperties:ULONG,pub rgpProperties:*mut *mut c_void,pub pUM:*mut c_void,pub pKM:*mut c_void}
#[repr(C)] pub struct CryptProviderRefs{pub cProviders:ULONG,pub rgpProviders:*mut *mut CryptProviderRef}
#[no_mangle] pub unsafe extern "system" fn BCryptResolveProviders(context:*const u16,interface:ULONG,function:*const u16,provider:*const u16,mode:ULONG,flags:ULONG,size:*mut ULONG,out:*mut *mut CryptProviderRefs)->NTSTATUS{
 const CRYPT_MIN_DEPENDENCIES:ULONG=1;const CRYPT_PROCESS_ISOLATE:ULONG=0x0001_0000;
 if size.is_null()||out.is_null()||!matches!(mode,1|2|3|4)||flags&!(CRYPT_MIN_DEPENDENCIES|CRYPT_PROCESS_ISOLATE)!=0||interface>7{return STATUS_INVALID_PARAMETER;}
 let context_name=if context.is_null(){None}else{match config_name(context){Ok(v)=>Some(v),Err(e)=>return e}};
 let function_name=if function.is_null(){None}else{match config_name(function){Ok(v)=>Some(v),Err(e)=>return e}};
 let provider_name=if provider.is_null(){ "Microsoft Primitive Provider".to_string() }else{match config_name(provider){Ok(v)=>v,Err(e)=>return e}};
 let registered=match registered_providers().lock(){Ok(v)=>v.contains(&provider_name),Err(_)=>return STATUS_NO_MEMORY};if !registered{return STATUS_NOT_FOUND;}
 if let (Some(c),Some(f))=(&context_name,&function_name){
  let found=match contexts().lock(){Ok(m)=>m.get(c).is_some_and(|ctx|ctx.functions.get(&interface).is_some_and(|v|v.contains(f))),Err(_)=>return STATUS_NO_MEMORY};
  if !found{return STATUS_NOT_FOUND;}
 }
 let function_name=function_name.unwrap_or_default();
 let header=core::mem::size_of::<CryptProviderRefs>();let ptr_bytes=core::mem::size_of::<*mut CryptProviderRef>();
 let ref_offset=(header+ptr_bytes+7)&!7;let ref_size=core::mem::size_of::<CryptProviderRef>();
 let fn_off=ref_offset+ref_size;let provider_off=fn_off+(function_name.encode_utf16().count()+1)*2;
 let total=provider_off+(provider_name.encode_utf16().count()+1)*2;
 let base=match store_enum_buffer(total){Ok(v)=>v,Err(e)=>return e};
 let array=unsafe{base.add(header).cast::<*mut CryptProviderRef>()};
 let reference=unsafe{base.add(ref_offset).cast::<CryptProviderRef>()};
 let fn_ptr=enum_write_wide(base,fn_off,&function_name);let provider_ptr=enum_write_wide(base,provider_off,&provider_name);
 unsafe{
  reference.write(CryptProviderRef{dwInterface:interface,pszFunction:fn_ptr,pszProvider:provider_ptr,cProperties:0,rgpProperties:ptr::null_mut(),pUM:ptr::null_mut(),pKM:ptr::null_mut()});
  array.write(reference);
  base.cast::<CryptProviderRefs>().write(CryptProviderRefs{cProviders:1,rgpProviders:array});
  size.write(total as ULONG);out.write(base.cast());
 }
 STATUS_SUCCESS
}

#[repr(C)] pub struct CryptProviders{pub cProviders:ULONG,pub rgpszProviders:*mut *mut u16}
#[repr(C)] pub struct CryptProviderReg{pub cAliases:ULONG,pub rgpszAliases:*mut *mut u16,pub pUM:*mut c_void,pub pKM:*mut c_void}
static REGISTERED_PROVIDERS:OnceLock<Mutex<HashSet<String>>>=OnceLock::new();
fn registered_providers()->&'static Mutex<HashSet<String>>{REGISTERED_PROVIDERS.get_or_init(||Mutex::new(["Microsoft Primitive Provider".to_string()].into_iter().collect()))}
#[no_mangle] pub unsafe extern "system" fn BCryptRegisterProvider(provider:*const u16,flags:ULONG,registration:*const CryptProviderReg)->NTSTATUS{
 if flags&!1!=0||registration.is_null(){return STATUS_INVALID_PARAMETER;}let name=match config_name(provider){Ok(v) if !v.is_empty()=>v,_=>return STATUS_INVALID_PARAMETER};
 match registered_providers().lock(){Ok(mut set)=>{if set.contains(&name)&&flags&1==0{return STATUS_OBJECT_NAME_COLLISION;}set.insert(name);STATUS_SUCCESS},Err(_)=>STATUS_NO_MEMORY}
}
#[no_mangle] pub unsafe extern "system" fn BCryptUnregisterProvider(provider:*const u16)->NTSTATUS{
 let name=match config_name(provider){Ok(v)=>v,Err(e)=>return e};if name=="Microsoft Primitive Provider"{return STATUS_NOT_SUPPORTED;}
 match registered_providers().lock(){Ok(mut set)=>if set.remove(&name){STATUS_SUCCESS}else{STATUS_NOT_FOUND},Err(_)=>STATUS_NO_MEMORY}
}
#[no_mangle] pub unsafe extern "system" fn BCryptEnumRegisteredProviders(size:*mut ULONG,out:*mut *mut CryptProviders)->NTSTATUS{
 if size.is_null()||out.is_null(){return STATUS_INVALID_PARAMETER;}let mut names=match registered_providers().lock(){Ok(v)=>v.iter().cloned().collect::<Vec<_>>(),Err(_)=>return STATUS_NO_MEMORY};names.sort();
 let header=core::mem::size_of::<CryptProviders>();let ptr_bytes=names.len()*core::mem::size_of::<*mut u16>();let strings=names.iter().map(|v|(v.encode_utf16().count()+1)*2).sum::<usize>();let total=header+ptr_bytes+strings;
 let base=match store_enum_buffer(total){Ok(v)=>v,Err(e)=>return e};let array=unsafe{base.add(header).cast::<*mut u16>()};let mut off=header+ptr_bytes;
 for (i,name) in names.iter().enumerate(){let p=enum_write_wide(base,off,name);unsafe{array.add(i).write(p)};off+=(name.encode_utf16().count()+1)*2;}
 unsafe{base.cast::<CryptProviders>().write(CryptProviders{cProviders:names.len() as ULONG,rgpszProviders:array});size.write(total as ULONG);out.write(base.cast())};STATUS_SUCCESS
}
#[no_mangle] pub unsafe extern "system" fn BCryptQueryProviderRegistration(provider:*const u16,mode:ULONG,interface:ULONG,size:*mut ULONG,out:*mut *mut CryptProviderReg)->NTSTATUS{
 if size.is_null()||out.is_null()||!matches!(mode,1|2|3|4){return STATUS_INVALID_PARAMETER;}let name=match config_name(provider){Ok(v)=>v,Err(e)=>return e};
 let found=match registered_providers().lock(){Ok(set)=>set.contains(&name),Err(_)=>return STATUS_NO_MEMORY};if !found{return STATUS_NOT_FOUND;}
 let required=core::mem::size_of::<CryptProviderReg>() as ULONG;unsafe{size.write(required)};
 let base=match store_enum_buffer(required as usize){Ok(v)=>v,Err(e)=>return e};
 unsafe{base.cast::<CryptProviderReg>().write(CryptProviderReg{cAliases:0,rgpszAliases:ptr::null_mut(),pUM:ptr::null_mut(),pKM:ptr::null_mut()});out.write(base.cast())};let _=interface;STATUS_SUCCESS
}

const CRYPT_LOCAL:ULONG=1;
const CRYPT_DOMAIN:ULONG=2;
const CRYPT_PRIORITY_TOP:ULONG=0;
const CRYPT_PRIORITY_BOTTOM:ULONG=u32::MAX;
const STATUS_NOT_FOUND:NTSTATUS=0xC0000225_u32 as i32;
const STATUS_OBJECT_NAME_COLLISION:NTSTATUS=0xC0000035_u32 as i32;
#[repr(C)] #[derive(Clone,Copy,Default)]
pub struct CryptContextConfig{pub dwFlags:ULONG,pub dwReserved:ULONG}
#[repr(C)] #[derive(Clone,Copy,Default)]
pub struct CryptContextFunctionConfig{pub dwFlags:ULONG,pub dwReserved:ULONG}
#[repr(C)] pub struct CryptContexts{pub cContexts:ULONG,pub rgpszContexts:*mut *mut u16}
#[repr(C)] pub struct CryptContextFunctions{pub cFunctions:ULONG,pub rgpszFunctions:*mut *mut u16}
#[repr(C)] pub struct CryptContextFunctionProviders{pub cProviders:ULONG,pub rgpszProviders:*mut *mut u16}
#[derive(Clone,Default)]
struct ContextEntry{config:CryptContextConfig,functions:HashMap<ULONG,Vec<String>>,providers:HashMap<(ULONG,String),Vec<String>>,function_configs:HashMap<(ULONG,String),CryptContextFunctionConfig>,properties:HashMap<(ULONG,String,String),Vec<u8>>}
static CONTEXTS:OnceLock<Mutex<HashMap<String,ContextEntry>>>=OnceLock::new();
fn contexts()->&'static Mutex<HashMap<String,ContextEntry>>{CONTEXTS.get_or_init(||Mutex::new(HashMap::new()))}
fn config_name(p:*const u16)->Result<String,NTSTATUS>{unsafe{read_property_name(p)}}
fn config_table(table:ULONG)->NTSTATUS{if table==CRYPT_LOCAL{STATUS_SUCCESS}else{STATUS_NOT_SUPPORTED}}
fn context_list_buffer(names:&[String])->Result<(*mut u8,ULONG),NTSTATUS>{
 let header=core::mem::size_of::<CryptContexts>();
 let ptr_bytes=names.len().checked_mul(core::mem::size_of::<*mut u16>()).ok_or(STATUS_NO_MEMORY)?;
 let strings=names.iter().try_fold(0usize,|n,v|n.checked_add((v.encode_utf16().count()+1)*2).ok_or(STATUS_NO_MEMORY))?;
 let total=header.checked_add(ptr_bytes).and_then(|v|v.checked_add(strings)).ok_or(STATUS_NO_MEMORY)?;
 let base=store_enum_buffer(total)?;let array=unsafe{base.add(header).cast::<*mut u16>()};let mut off=header+ptr_bytes;
 for (i,name) in names.iter().enumerate(){let p=enum_write_wide(base,off,name);unsafe{array.add(i).write(p)};off+=(name.encode_utf16().count()+1)*2;}
 unsafe{base.cast::<CryptContexts>().write(CryptContexts{cContexts:names.len() as ULONG,rgpszContexts:array})};
 Ok((base,total as ULONG))
}
fn context_functions_buffer(names:&[String])->Result<(*mut u8,ULONG),NTSTATUS>{
 let header=core::mem::size_of::<CryptContextFunctions>();let ptr_bytes=names.len().checked_mul(8).ok_or(STATUS_NO_MEMORY)?;
 let strings=names.iter().try_fold(0usize,|n,v|n.checked_add((v.encode_utf16().count()+1)*2).ok_or(STATUS_NO_MEMORY))?;
 let total=header.checked_add(ptr_bytes).and_then(|v|v.checked_add(strings)).ok_or(STATUS_NO_MEMORY)?;
 let base=store_enum_buffer(total)?;let array=unsafe{base.add(header).cast::<*mut u16>()};let mut off=header+ptr_bytes;
 for (i,name) in names.iter().enumerate(){let p=enum_write_wide(base,off,name);unsafe{array.add(i).write(p)};off+=(name.encode_utf16().count()+1)*2;}
 unsafe{base.cast::<CryptContextFunctions>().write(CryptContextFunctions{cFunctions:names.len() as ULONG,rgpszFunctions:array})};
 Ok((base,total as ULONG))
}
fn output_context_buffer<T>(size:*mut ULONG,out:*mut *mut T,base:*mut u8,total:ULONG)->NTSTATUS{
 if size.is_null()||out.is_null(){return STATUS_INVALID_PARAMETER;}
 unsafe{size.write(total);out.write(base.cast())};STATUS_SUCCESS
}
#[no_mangle] pub unsafe extern "system" fn BCryptCreateContext(table:ULONG,name:*const u16,cfg:*const CryptContextConfig)->NTSTATUS{
 let st=config_table(table);if st!=STATUS_SUCCESS{return st;}let name=match config_name(name){Ok(v) if !v.is_empty()=>v,_=>return STATUS_INVALID_PARAMETER};
 let config=if cfg.is_null(){CryptContextConfig::default()}else{unsafe{ptr::read_unaligned(cfg)}};
 match contexts().lock(){Ok(mut m)=>{if m.contains_key(&name){return STATUS_OBJECT_NAME_COLLISION;}m.insert(name,ContextEntry{config,..Default::default()});STATUS_SUCCESS},Err(_)=>STATUS_NO_MEMORY}
}
#[no_mangle] pub unsafe extern "system" fn BCryptDeleteContext(table:ULONG,name:*const u16)->NTSTATUS{
 let st=config_table(table);if st!=STATUS_SUCCESS{return st;}let name=match config_name(name){Ok(v)=>v,Err(e)=>return e};
 match contexts().lock(){Ok(mut m)=>if m.remove(&name).is_some(){STATUS_SUCCESS}else{STATUS_NOT_FOUND},Err(_)=>STATUS_NO_MEMORY}
}
#[no_mangle] pub unsafe extern "system" fn BCryptConfigureContext(table:ULONG,name:*const u16,cfg:*const CryptContextConfig)->NTSTATUS{
 let st=config_table(table);if st!=STATUS_SUCCESS{return st;}if cfg.is_null(){return STATUS_INVALID_PARAMETER;}let name=match config_name(name){Ok(v)=>v,Err(e)=>return e};let cfg=unsafe{ptr::read_unaligned(cfg)};
 match contexts().lock(){Ok(mut m)=>match m.get_mut(&name){Some(c)=>{c.config=cfg;STATUS_SUCCESS},None=>STATUS_NOT_FOUND},Err(_)=>STATUS_NO_MEMORY}
}
#[no_mangle] pub unsafe extern "system" fn BCryptQueryContextConfiguration(table:ULONG,name:*const u16,size:*mut ULONG,out:*mut *mut CryptContextConfig)->NTSTATUS{
 let st=config_table(table);if st!=STATUS_SUCCESS{return st;}if size.is_null()||out.is_null(){return STATUS_INVALID_PARAMETER;}let name=match config_name(name){Ok(v)=>v,Err(e)=>return e};
 let cfg=match contexts().lock(){Ok(m)=>match m.get(&name){Some(c)=>c.config,None=>return STATUS_NOT_FOUND},Err(_)=>return STATUS_NO_MEMORY};
 let n=core::mem::size_of::<CryptContextConfig>() as ULONG;unsafe{size.write(n)};
 let base=match store_enum_buffer(n as usize){Ok(v)=>v,Err(e)=>return e};unsafe{base.cast::<CryptContextConfig>().write(cfg);out.write(base.cast())};STATUS_SUCCESS
}
#[no_mangle] pub unsafe extern "system" fn BCryptEnumContexts(table:ULONG,size:*mut ULONG,out:*mut *mut CryptContexts)->NTSTATUS{
 let st=config_table(table);if st!=STATUS_SUCCESS{return st;}if size.is_null()||out.is_null(){return STATUS_INVALID_PARAMETER;}
 let mut names=match contexts().lock(){Ok(m)=>m.keys().cloned().collect::<Vec<_>>(),Err(_)=>return STATUS_NO_MEMORY};names.sort();
 let (base,total)=match context_list_buffer(&names){Ok(v)=>v,Err(e)=>return e};unsafe{size.write(total);out.write(base.cast())};STATUS_SUCCESS
}
#[no_mangle] pub unsafe extern "system" fn BCryptAddContextFunction(table:ULONG,context:*const u16,interface:ULONG,function:*const u16,position:ULONG)->NTSTATUS{
 let st=config_table(table);if st!=STATUS_SUCCESS{return st;}let c=match config_name(context){Ok(v)=>v,Err(e)=>return e};let f=match config_name(function){Ok(v) if !v.is_empty()=>v,_=>return STATUS_INVALID_PARAMETER};
 match contexts().lock(){Ok(mut m)=>match m.get_mut(&c){None=>STATUS_NOT_FOUND,Some(ctx)=>{let list=ctx.functions.entry(interface).or_default();list.retain(|x|x!=&f);if position==CRYPT_PRIORITY_TOP{list.insert(0,f);}else if position==CRYPT_PRIORITY_BOTTOM{list.push(f);}else{let p=(position as usize).min(list.len());list.insert(p,f);}STATUS_SUCCESS}},Err(_)=>STATUS_NO_MEMORY}
}
#[no_mangle] pub unsafe extern "system" fn BCryptRemoveContextFunction(table:ULONG,context:*const u16,interface:ULONG,function:*const u16)->NTSTATUS{
 let st=config_table(table);if st!=STATUS_SUCCESS{return st;}let c=match config_name(context){Ok(v)=>v,Err(e)=>return e};let f=match config_name(function){Ok(v)=>v,Err(e)=>return e};
 match contexts().lock(){Ok(mut m)=>match m.get_mut(&c){None=>STATUS_NOT_FOUND,Some(ctx)=>{let Some(list)=ctx.functions.get_mut(&interface)else{return STATUS_NOT_FOUND};let old=list.len();list.retain(|x|x!=&f);if old==list.len(){STATUS_NOT_FOUND}else{STATUS_SUCCESS}}},Err(_)=>STATUS_NO_MEMORY}
}
#[no_mangle] pub unsafe extern "system" fn BCryptEnumContextFunctions(table:ULONG,context:*const u16,interface:ULONG,size:*mut ULONG,out:*mut *mut CryptContextFunctions)->NTSTATUS{
 let st=config_table(table);if st!=STATUS_SUCCESS{return st;}if size.is_null()||out.is_null(){return STATUS_INVALID_PARAMETER;}let c=match config_name(context){Ok(v)=>v,Err(e)=>return e};
 let names=match contexts().lock(){Ok(m)=>match m.get(&c){Some(ctx)=>ctx.functions.get(&interface).cloned().unwrap_or_default(),None=>return STATUS_NOT_FOUND},Err(_)=>return STATUS_NO_MEMORY};
 let (base,total)=match context_functions_buffer(&names){Ok(v)=>v,Err(e)=>return e};unsafe{size.write(total);out.write(base.cast())};STATUS_SUCCESS
}
#[no_mangle] pub unsafe extern "system" fn BCryptConfigureContextFunction(table:ULONG,context:*const u16,interface:ULONG,function:*const u16,cfg:*const CryptContextFunctionConfig)->NTSTATUS{
 let st=config_table(table);if st!=STATUS_SUCCESS{return st;}if cfg.is_null(){return STATUS_INVALID_PARAMETER;}let c=match config_name(context){Ok(v)=>v,Err(e)=>return e};let f=match config_name(function){Ok(v)=>v,Err(e)=>return e};let value=unsafe{ptr::read_unaligned(cfg)};
 match contexts().lock(){Ok(mut m)=>match m.get_mut(&c){Some(ctx) if ctx.functions.get(&interface).is_some_and(|v|v.contains(&f))=>{ctx.function_configs.insert((interface,f),value);STATUS_SUCCESS},Some(_)=>STATUS_NOT_FOUND,None=>STATUS_NOT_FOUND},Err(_)=>STATUS_NO_MEMORY}
}
#[no_mangle] pub unsafe extern "system" fn BCryptQueryContextFunctionConfiguration(table:ULONG,context:*const u16,interface:ULONG,function:*const u16,size:*mut ULONG,out:*mut *mut CryptContextFunctionConfig)->NTSTATUS{
 let st=config_table(table);if st!=STATUS_SUCCESS{return st;}if size.is_null()||out.is_null(){return STATUS_INVALID_PARAMETER;}let c=match config_name(context){Ok(v)=>v,Err(e)=>return e};let f=match config_name(function){Ok(v)=>v,Err(e)=>return e};
 let value=match contexts().lock(){Ok(m)=>match m.get(&c){Some(ctx)=>ctx.function_configs.get(&(interface,f)).copied().unwrap_or_default(),None=>return STATUS_NOT_FOUND},Err(_)=>return STATUS_NO_MEMORY};
 let n=core::mem::size_of::<CryptContextFunctionConfig>() as ULONG;unsafe{size.write(n)};let base=match store_enum_buffer(n as usize){Ok(v)=>v,Err(e)=>return e};unsafe{base.cast::<CryptContextFunctionConfig>().write(value);out.write(base.cast())};STATUS_SUCCESS
}
#[no_mangle] pub unsafe extern "system" fn BCryptEnumContextFunctionProviders(table:ULONG,context:*const u16,interface:ULONG,function:*const u16,size:*mut ULONG,out:*mut *mut CryptContextFunctionProviders)->NTSTATUS{
 let st=config_table(table);if st!=STATUS_SUCCESS{return st;}if size.is_null()||out.is_null(){return STATUS_INVALID_PARAMETER;}let c=match config_name(context){Ok(v)=>v,Err(e)=>return e};let f=match config_name(function){Ok(v)=>v,Err(e)=>return e};
 let names=match contexts().lock(){Ok(m)=>match m.get(&c){Some(ctx) if ctx.functions.get(&interface).is_some_and(|v|v.contains(&f))=>ctx.providers.get(&(interface,f)).cloned().unwrap_or_else(||vec!["Microsoft Primitive Provider".to_string()]),_=>return STATUS_NOT_FOUND},Err(_)=>return STATUS_NO_MEMORY};
 let header=core::mem::size_of::<CryptContextFunctionProviders>();let ptr_bytes=names.len()*core::mem::size_of::<*mut u16>();let strings=names.iter().map(|v|(v.encode_utf16().count()+1)*2).sum::<usize>();let total=header+ptr_bytes+strings;let base=match store_enum_buffer(total){Ok(v)=>v,Err(e)=>return e};let array=unsafe{base.add(header).cast::<*mut u16>()};let mut off=header+ptr_bytes;for (i,name) in names.iter().enumerate(){let p=enum_write_wide(base,off,name);unsafe{array.add(i).write(p)};off+=(name.encode_utf16().count()+1)*2;}
 unsafe{base.cast::<CryptContextFunctionProviders>().write(CryptContextFunctionProviders{cProviders:names.len() as ULONG,rgpszProviders:array});size.write(total as ULONG);out.write(base.cast())};STATUS_SUCCESS
}
#[no_mangle] pub unsafe extern "system" fn BCryptAddContextFunctionProvider(table:ULONG,context:*const u16,interface:ULONG,function:*const u16,provider:*const u16,position:ULONG)->NTSTATUS{
 let st=config_table(table);if st!=STATUS_SUCCESS{return st;}let c=match config_name(context){Ok(v)=>v,Err(e)=>return e};let f=match config_name(function){Ok(v)=>v,Err(e)=>return e};let p=match config_name(provider){Ok(v) if !v.is_empty()=>v,_=>return STATUS_INVALID_PARAMETER};
 match contexts().lock(){Ok(mut m)=>match m.get_mut(&c){Some(ctx) if ctx.functions.get(&interface).is_some_and(|v|v.contains(&f))=>{let list=ctx.providers.entry((interface,f)).or_insert_with(||vec!["Microsoft Primitive Provider".to_string()]);list.retain(|x|x!=&p);if position==CRYPT_PRIORITY_TOP{list.insert(0,p);}else{list.push(p);}STATUS_SUCCESS},Some(_)=>STATUS_NOT_FOUND,None=>STATUS_NOT_FOUND},Err(_)=>STATUS_NO_MEMORY}
}
#[no_mangle] pub unsafe extern "system" fn BCryptRemoveContextFunctionProvider(table:ULONG,context:*const u16,interface:ULONG,function:*const u16,provider:*const u16)->NTSTATUS{
 let st=config_table(table);if st!=STATUS_SUCCESS{return st;}let c=match config_name(context){Ok(v)=>v,Err(e)=>return e};let f=match config_name(function){Ok(v)=>v,Err(e)=>return e};let p=match config_name(provider){Ok(v)=>v,Err(e)=>return e};
 match contexts().lock(){Ok(mut m)=>match m.get_mut(&c){Some(ctx)=>match ctx.providers.get_mut(&(interface,f)){Some(list)=>{let n=list.len();list.retain(|x|x!=&p);if n==list.len(){STATUS_NOT_FOUND}else{STATUS_SUCCESS}},None=>STATUS_NOT_FOUND},None=>STATUS_NOT_FOUND},Err(_)=>STATUS_NO_MEMORY}
}
#[no_mangle] pub unsafe extern "system" fn BCryptSetContextFunctionProperty(table:ULONG,context:*const u16,interface:ULONG,function:*const u16,property:*const u16,len:ULONG,value:*const u8)->NTSTATUS{
 let st=config_table(table);if st!=STATUS_SUCCESS{return st;}if value.is_null()&&len!=0{return STATUS_INVALID_PARAMETER;}let c=match config_name(context){Ok(v)=>v,Err(e)=>return e};let f=match config_name(function){Ok(v)=>v,Err(e)=>return e};let p=match config_name(property){Ok(v)=>v,Err(e)=>return e};let bytes=if len==0{Vec::new()}else{unsafe{core::slice::from_raw_parts(value,len as usize)}.to_vec()};
 match contexts().lock(){Ok(mut m)=>match m.get_mut(&c){Some(ctx) if ctx.functions.get(&interface).is_some_and(|v|v.contains(&f))=>{ctx.properties.insert((interface,f,p),bytes);STATUS_SUCCESS},_=>STATUS_NOT_FOUND},Err(_)=>STATUS_NO_MEMORY}
}
#[no_mangle] pub unsafe extern "system" fn BCryptQueryContextFunctionProperty(table:ULONG,context:*const u16,interface:ULONG,function:*const u16,property:*const u16,len:*mut ULONG,value:*mut *mut u8)->NTSTATUS{
 let st=config_table(table);if st!=STATUS_SUCCESS{return st;}if len.is_null()||value.is_null(){return STATUS_INVALID_PARAMETER;}let c=match config_name(context){Ok(v)=>v,Err(e)=>return e};let f=match config_name(function){Ok(v)=>v,Err(e)=>return e};let p=match config_name(property){Ok(v)=>v,Err(e)=>return e};
 let bytes=match contexts().lock(){Ok(m)=>match m.get(&c).and_then(|ctx|ctx.properties.get(&(interface,f,p))).cloned(){Some(v)=>v,None=>return STATUS_NOT_FOUND},Err(_)=>return STATUS_NO_MEMORY};
 let base=match store_enum_buffer(bytes.len()){Ok(v)=>v,Err(e)=>return e};if !bytes.is_empty(){unsafe{ptr::copy_nonoverlapping(bytes.as_ptr(),base,bytes.len())};}unsafe{len.write(bytes.len() as ULONG);value.write(base)};STATUS_SUCCESS
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
        let mut duplicate=ptr::null_mut();
        assert_eq!(unsafe{BCryptDuplicateKey(key,&mut duplicate,ptr::null_mut(),0,0)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptDestroyKey(key)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptDestroyKey(duplicate)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptCloseAlgorithmProvider(provider,0)},STATUS_SUCCESS);
    }

    #[test]
    fn pbkdf2_key_derivation_defaults_to_empty_salt_and_ten_thousand_iterations() {
        let wide=|v:&str|v.encode_utf16().chain(core::iter::once(0)).collect::<Vec<u16>>();
        let provider_name=wide("PBKDF2");let mut provider=ptr::null_mut();
        assert_eq!(unsafe{BCryptOpenAlgorithmProvider(&mut provider,provider_name.as_ptr(),ptr::null(),0)},STATUS_SUCCESS);
        let password=b"password";let mut key=ptr::null_mut();
        assert_eq!(unsafe{BCryptGenerateSymmetricKey(provider,&mut key,ptr::null_mut(),0,password.as_ptr() as *mut u8,password.len() as ULONG,0)},STATUS_SUCCESS);
        let hash_name=wide("SHA256");
        let mut buffer=BcryptBuffer{cbBuffer:(hash_name.len()*2) as ULONG,BufferType:0,pvBuffer:hash_name.as_ptr() as *mut c_void};
        let mut desc=BcryptBufferDesc{ulVersion:0,cBuffers:1,pBuffers:&mut buffer};
        let mut derived=[0u8;32];let mut written=0;
        assert_eq!(unsafe{BCryptKeyDerivation(key,&mut desc,derived.as_mut_ptr(),32,&mut written,0)},STATUS_SUCCESS);
        assert_eq!(written,32);
        let mut prf=ptr::null_mut();
        let sha=wide("SHA256");
        assert_eq!(unsafe{BCryptOpenAlgorithmProvider(&mut prf,sha.as_ptr(),ptr::null(),BCRYPT_ALG_HANDLE_HMAC_FLAG)},STATUS_SUCCESS);
        let mut expected=[0u8;32];
        assert_eq!(unsafe{BCryptDeriveKeyPBKDF2(prf,password.as_ptr() as *mut u8,password.len() as ULONG,ptr::null_mut(),0,10_000,expected.as_mut_ptr(),32,0)},STATUS_SUCCESS);
        assert_eq!(derived,expected);
        assert_eq!(unsafe{BCryptDestroyKey(key)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptCloseAlgorithmProvider(provider,0)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptCloseAlgorithmProvider(prf,0)},STATUS_SUCCESS);
    }

    #[test]
    fn pbkdf2_provider_accepts_an_empty_password() {
        let wide=|v:&str|v.encode_utf16().chain(core::iter::once(0)).collect::<Vec<u16>>();
        let name=wide("PBKDF2");
        let mut provider=ptr::null_mut();
        assert_eq!(unsafe{BCryptOpenAlgorithmProvider(&mut provider,name.as_ptr(),ptr::null(),0)},STATUS_SUCCESS);
        let mut key=ptr::null_mut();
        assert_eq!(unsafe{BCryptGenerateSymmetricKey(provider,&mut key,ptr::null_mut(),0,ptr::null_mut(),0,0)},STATUS_SUCCESS);
        let hash=wide("SHA256");
        let salt=b"salt";
        let iterations:u64=1;
        let mut buffers=[
            BcryptBuffer{cbBuffer:(hash.len()*2) as u32,BufferType:0,pvBuffer:hash.as_ptr() as *mut c_void},
            BcryptBuffer{cbBuffer:salt.len() as u32,BufferType:15,pvBuffer:salt.as_ptr() as *mut c_void},
            BcryptBuffer{cbBuffer:8,BufferType:16,pvBuffer:(&iterations as *const u64) as *mut c_void},
        ];
        let mut desc=BcryptBufferDesc{ulVersion:0,cBuffers:3,pBuffers:buffers.as_mut_ptr()};
        let mut output=[0u8;32];
        let mut written=0;
        assert_eq!(unsafe{BCryptKeyDerivation(key,&mut desc,output.as_mut_ptr(),32,&mut written,0)},STATUS_SUCCESS);
        assert_eq!(written,32);
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
    fn bcrypt_reusable_hash_flag_resets_hash_state_after_finish() {
        let wide=|v:&str|v.encode_utf16().chain(core::iter::once(0)).collect::<Vec<u16>>();
        let name=wide("SHA256");
        let mut provider=ptr::null_mut();
        assert_eq!(unsafe{BCryptOpenAlgorithmProvider(&mut provider,name.as_ptr(),ptr::null(),BCRYPT_HASH_REUSABLE_FLAG)},STATUS_SUCCESS);
        let mut hash=ptr::null_mut();
        assert_eq!(unsafe{BCryptCreateHash(provider,&mut hash,ptr::null_mut(),0,ptr::null_mut(),0,BCRYPT_HASH_REUSABLE_FLAG)},STATUS_SUCCESS);
        let mut output=[0u8;32];
        assert_eq!(unsafe{BCryptHashData(hash,b"abc".as_ptr(),3,0)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptFinishHash(hash,output.as_mut_ptr(),32,0)},STATUS_SUCCESS);
        assert_eq!(output.as_slice(),HashAlgorithm::Sha256.digest(b"abc").as_ref());
        assert_eq!(unsafe{BCryptHashData(hash,b"def".as_ptr(),3,0)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptFinishHash(hash,output.as_mut_ptr(),32,0)},STATUS_SUCCESS);
        assert_eq!(output.as_slice(),HashAlgorithm::Sha256.digest(b"def").as_ref());
        assert_eq!(unsafe{BCryptDestroyHash(hash)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptCloseAlgorithmProvider(provider,0)},STATUS_SUCCESS);
    }

    #[test]
    fn non_reusable_hash_handles_are_invalid_after_finish() {
        let wide=|v:&str|v.encode_utf16().chain(core::iter::once(0)).collect::<Vec<u16>>();
        let name=wide("SHA256");let mut provider=ptr::null_mut();
        assert_eq!(unsafe{BCryptOpenAlgorithmProvider(&mut provider,name.as_ptr(),ptr::null(),0)},STATUS_SUCCESS);
        let mut hash=ptr::null_mut();
        assert_eq!(unsafe{BCryptCreateHash(provider,&mut hash,ptr::null_mut(),0,ptr::null_mut(),0,0)},STATUS_SUCCESS);
        let mut output=[0u8;32];
        assert_eq!(unsafe{BCryptFinishHash(hash,output.as_mut_ptr(),32,0)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptHashData(hash,b"x".as_ptr(),1,0)},STATUS_INVALID_PARAMETER);
        assert_eq!(unsafe{BCryptFinishHash(hash,output.as_mut_ptr(),32,0)},STATUS_INVALID_PARAMETER);
        assert_eq!(unsafe{BCryptDestroyHash(hash)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptCloseAlgorithmProvider(provider,0)},STATUS_SUCCESS);
    }

    #[test]
    fn hash_apis_accept_caller_owned_object_buffers() {
        let name="SHA256".encode_utf16().chain(core::iter::once(0)).collect::<Vec<u16>>();
        let mut provider=ptr::null_mut();
        assert_eq!(unsafe{BCryptOpenAlgorithmProvider(&mut provider,name.as_ptr(),ptr::null(),0)},STATUS_SUCCESS);
        let mut object=[0u8;256];
        let mut hash=ptr::null_mut();
        assert_eq!(unsafe{BCryptCreateHash(provider,&mut hash,object.as_mut_ptr(),object.len() as ULONG,ptr::null_mut(),0,0)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptHashData(hash,b"abc".as_ptr(),3,0)},STATUS_SUCCESS);
        let mut duplicate_object=[0u8;256];let mut duplicate=ptr::null_mut();
        assert_eq!(unsafe{BCryptDuplicateHash(hash,&mut duplicate,duplicate_object.as_mut_ptr(),duplicate_object.len() as ULONG,0)},STATUS_SUCCESS);
        let mut a=[0u8;32];let mut b=[0u8;32];
        assert_eq!(unsafe{BCryptFinishHash(hash,a.as_mut_ptr(),32,0)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptFinishHash(duplicate,b.as_mut_ptr(),32,0)},STATUS_SUCCESS);
        assert_eq!(a,b);
        assert_eq!(unsafe{BCryptDestroyHash(hash)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptDestroyHash(duplicate)},STATUS_SUCCESS);
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
        let provider = register_handle(RegisteredHandle::Algorithm(AlgorithmHandle { algorithm: HashAlgorithm::Sha256, hmac: false, multi: false, reusable: false, cipher: false, gcm: false, cbc: false, kdf: false, rsa: false, rng: false })).unwrap();
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
        let provider = register_handle(RegisteredHandle::Algorithm(AlgorithmHandle { algorithm: HashAlgorithm::Sha256, hmac: false, multi: false, reusable: false, cipher: false, kdf: false, rsa: false, rng: false, gcm: false, cbc: false })).unwrap();
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
    fn aes_defaults_to_cbc_and_gcm_reports_supported_authentication_tags() {
        let wide=|v:&str|v.encode_utf16().chain(core::iter::once(0)).collect::<Vec<u16>>();
        let algorithm=wide("AES");let chaining=wide("ChainingMode");let auth=wide("AuthTagLength");
        let mut provider=ptr::null_mut();
        assert_eq!(unsafe{BCryptOpenAlgorithmProvider(&mut provider,algorithm.as_ptr(),ptr::null(),0)},STATUS_SUCCESS);
        let mut mode=[0u8;64];let mut required=0;
        assert_eq!(unsafe{BCryptGetProperty(provider,chaining.as_ptr(),mode.as_mut_ptr(),64,&mut required,0)},STATUS_SUCCESS);
        let units=unsafe{core::slice::from_raw_parts(mode.as_ptr().cast::<u16>(),required as usize/2-1)};
        assert_eq!(String::from_utf16(units).unwrap(),"ChainingModeCBC");
        let mut tag_lengths=BcryptKeyLengths{dwMinLength:0,dwMaxLength:0,dwIncrement:0};
        assert_eq!(unsafe{BCryptGetProperty(provider,auth.as_ptr(),(&mut tag_lengths as *mut BcryptKeyLengths).cast::<u8>(),12,&mut required,0)},STATUS_NOT_SUPPORTED);
        assert_eq!(unsafe{BCryptSetProperty(provider,chaining.as_ptr(),wide("ChainingModeGCM").as_ptr().cast(),(wide("ChainingModeGCM").len()*2) as ULONG,0)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptGetProperty(provider,auth.as_ptr(),ptr::null_mut(),0,&mut required,0)},STATUS_SUCCESS);
        assert_eq!(required,12);
        assert_eq!(unsafe{BCryptGetProperty(provider,auth.as_ptr(),(&mut tag_lengths as *mut BcryptKeyLengths).cast::<u8>(),12,&mut required,0)},STATUS_SUCCESS);
        assert_eq!(tag_lengths.dwMinLength,12);assert_eq!(tag_lengths.dwMaxLength,16);assert_eq!(tag_lengths.dwIncrement,1);
        assert_eq!(unsafe{BCryptCloseAlgorithmProvider(provider,0)},STATUS_SUCCESS);
    }

    #[test]
    fn aes_chaining_mode_can_be_changed_on_a_key_handle() {
        let wide=|v:&str|v.encode_utf16().chain(core::iter::once(0)).collect::<Vec<u16>>();
        let algorithm=wide("AES");let chaining=wide("ChainingMode");let gcm=wide("ChainingModeGCM");let auth=wide("AuthTagLength");
        let mut provider=ptr::null_mut();
        assert_eq!(unsafe{BCryptOpenAlgorithmProvider(&mut provider,algorithm.as_ptr(),ptr::null(),0)},STATUS_SUCCESS);
        let mut material=[0u8;16];let mut key=ptr::null_mut();
        assert_eq!(unsafe{BCryptGenerateSymmetricKey(provider,&mut key,ptr::null_mut(),0,material.as_mut_ptr(),16,0)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptSetProperty(key,chaining.as_ptr(),gcm.as_ptr().cast(),(gcm.len()*2) as ULONG,0)},STATUS_SUCCESS);
        let mut lengths=BcryptKeyLengths{dwMinLength:0,dwMaxLength:0,dwIncrement:0};let mut required=0;
        assert_eq!(unsafe{BCryptGetProperty(key,auth.as_ptr(),(&mut lengths as *mut BcryptKeyLengths).cast::<u8>(),12,&mut required,0)},STATUS_SUCCESS);
        assert_eq!(lengths.dwMinLength,12);assert_eq!(lengths.dwMaxLength,16);
        assert_eq!(unsafe{BCryptDestroyKey(key)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptCloseAlgorithmProvider(provider,0)},STATUS_SUCCESS);
    }

    #[test]
    fn aes_and_rsa_providers_report_windows_key_length_ranges() {
        let wide=|v:&str|v.encode_utf16().chain(core::iter::once(0)).collect::<Vec<u16>>();
        for (algorithm,expected) in [("AES",BcryptKeyLengths{dwMinLength:128,dwMaxLength:256,dwIncrement:64}),("RSA",BcryptKeyLengths{dwMinLength:512,dwMaxLength:16384,dwIncrement:64})] {
            let name=wide(algorithm);let property=wide("KeyLengths");let mut provider=ptr::null_mut();
            assert_eq!(unsafe{BCryptOpenAlgorithmProvider(&mut provider,name.as_ptr(),ptr::null(),0)},STATUS_SUCCESS);
            let mut actual=BcryptKeyLengths{dwMinLength:0,dwMaxLength:0,dwIncrement:0};let mut required=0;
            assert_eq!(unsafe{BCryptGetProperty(provider,property.as_ptr(),(&mut actual as *mut BcryptKeyLengths).cast::<u8>(),12,&mut required,0)},STATUS_SUCCESS);
            assert_eq!(required,12);assert_eq!(actual.dwMinLength,expected.dwMinLength);assert_eq!(actual.dwMaxLength,expected.dwMaxLength);assert_eq!(actual.dwIncrement,expected.dwIncrement);
            assert_eq!(unsafe{BCryptCloseAlgorithmProvider(provider,0)},STATUS_SUCCESS);
        }
    }

    #[test]
    fn algorithm_properties_report_the_opened_cng_provider() {
        let wide=|v:&str|v.encode_utf16().chain(core::iter::once(0)).collect::<Vec<u16>>();
        for (algorithm,expected) in [("PBKDF2","PBKDF2"),("RSA","RSA"),("RNG","RNG")] {
            let name=wide(algorithm);
            let mut handle=ptr::null_mut();
            assert_eq!(unsafe{BCryptOpenAlgorithmProvider(&mut handle,name.as_ptr(),ptr::null(),0)},STATUS_SUCCESS);
            let property=wide("AlgorithmName");
            let mut output=[0u8;64];
            let mut required=0;
            assert_eq!(unsafe{BCryptGetProperty(handle,property.as_ptr(),output.as_mut_ptr(),output.len() as ULONG,&mut required,0)},STATUS_SUCCESS);
            let units=unsafe{core::slice::from_raw_parts(output.as_ptr().cast::<u16>(),required as usize/2-1)};
            assert_eq!(String::from_utf16(units).unwrap(),expected);
            assert_eq!(unsafe{BCryptCloseAlgorithmProvider(handle,0)},STATUS_SUCCESS);
        }
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
    fn rsa_verify_accepts_all_implemented_sha2_pkcs1_padding_descriptors() {
        let public=RsaPublicKey::new(BigUint::from_bytes_be(&[0xffu8;64]),BigUint::from(65537u32)).unwrap();
        let key=register_handle(RegisteredHandle::RsaPublic(public)).unwrap();
        for (algorithm,length) in [("SHA256",32usize),("SHA384",48),("SHA512",64)] {
            let name=algorithm.encode_utf16().chain(core::iter::once(0)).collect::<Vec<u16>>();
            let info=BcryptPkcs1PaddingInfo{pszAlgId:name.as_ptr()};
            let hash=vec![0u8;length];let signature=[0u8;64];
            assert_eq!(unsafe{BCryptVerifySignature(key,(&info as *const BcryptPkcs1PaddingInfo) as *mut c_void,hash.as_ptr(),length as ULONG,signature.as_ptr(),64,2)},STATUS_INVALID_SIGNATURE);
        }
        assert_eq!(unsafe{BCryptDestroyKey(key)},STATUS_SUCCESS);
    }

    #[test]
    fn rsa_public_key_property_and_duplicate_handle_behavior() {
        let modulus=BigUint::from_bytes_be(&[0xffu8;64]);
        let exponent=BigUint::from(65537u32);
        let public=RsaPublicKey::new(modulus,exponent).unwrap();
        let handle=register_handle(RegisteredHandle::RsaPublic(public)).unwrap();
        let property="KeyLength".encode_utf16().chain(core::iter::once(0)).collect::<Vec<u16>>();
        let mut bits=0u32;let mut required=0;
        assert_eq!(unsafe{BCryptGetProperty(handle,property.as_ptr(),(&mut bits as *mut u32).cast(),4,&mut required,0)},STATUS_SUCCESS);
        assert_eq!(bits,512);assert_eq!(required,4);
        let mut duplicate=ptr::null_mut();
        assert_eq!(unsafe{BCryptDuplicateKey(handle,&mut duplicate,ptr::null_mut(),0,0)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptDestroyKey(handle)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptGetProperty(duplicate,property.as_ptr(),(&mut bits as *mut u32).cast(),4,&mut required,0)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptDestroyKey(duplicate)},STATUS_SUCCESS);
    }

    #[test]
    fn local_provider_registration_query_and_enumeration_roundtrip() {
        let wide=|v:&str|v.encode_utf16().chain(core::iter::once(0)).collect::<Vec<u16>>();
        let name=wide("Cognac Test Provider");
        let reg=CryptProviderReg{cAliases:0,rgpszAliases:ptr::null_mut(),pUM:ptr::null_mut(),pKM:ptr::null_mut()};
        assert_eq!(unsafe{BCryptRegisterProvider(name.as_ptr(),0,&reg)},STATUS_SUCCESS);
        let mut size=0;let mut providers=ptr::null_mut();
        assert_eq!(unsafe{BCryptEnumRegisteredProviders(&mut size,&mut providers)},STATUS_SUCCESS);
        assert!(size>core::mem::size_of::<CryptProviders>() as ULONG);
        let list=unsafe{&*providers};assert!(list.cProviders>=2);
        let mut query_size=0;let mut queried=ptr::null_mut();
        assert_eq!(unsafe{BCryptQueryProviderRegistration(name.as_ptr(),4,2,&mut query_size,&mut queried)},STATUS_SUCCESS);
        assert_eq!(query_size,core::mem::size_of::<CryptProviderReg>() as ULONG);
        unsafe{BCryptFreeBuffer(providers.cast());BCryptFreeBuffer(queried.cast())};
        assert_eq!(unsafe{BCryptUnregisterProvider(name.as_ptr())},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptUnregisterProvider(name.as_ptr())},STATUS_NOT_FOUND);
    }

    #[test]
    fn local_cng_context_configuration_roundtrips_functions_providers_and_properties() {
        let wide=|v:&str|v.encode_utf16().chain(core::iter::once(0)).collect::<Vec<u16>>();
        let context=wide("CognacTest");let function=wide("SHA256");let provider=wide("Microsoft Primitive Provider");
        let mut cfg=CryptContextConfig{dwFlags:0,dwReserved:0};
        assert_eq!(unsafe{BCryptCreateContext(CRYPT_LOCAL,context.as_ptr(),&cfg)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptAddContextFunction(CRYPT_LOCAL,context.as_ptr(),2,function.as_ptr(),CRYPT_PRIORITY_BOTTOM)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptAddContextFunctionProvider(CRYPT_LOCAL,context.as_ptr(),2,function.as_ptr(),provider.as_ptr(),CRYPT_PRIORITY_BOTTOM)},STATUS_SUCCESS);
        let property=wide("TestProperty");let bytes=[1u8,2,3,4];
        assert_eq!(unsafe{BCryptSetContextFunctionProperty(CRYPT_LOCAL,context.as_ptr(),2,function.as_ptr(),property.as_ptr(),4,bytes.as_ptr())},STATUS_SUCCESS);
        let mut size=0;let mut value=ptr::null_mut();
        assert_eq!(unsafe{BCryptQueryContextFunctionProperty(CRYPT_LOCAL,context.as_ptr(),2,function.as_ptr(),property.as_ptr(),&mut size,&mut value)},STATUS_SUCCESS);
        assert_eq!(size,4);assert_eq!(unsafe{core::slice::from_raw_parts(value,size as usize)},bytes);
        unsafe{BCryptFreeBuffer(value.cast())};
        let mut fn_size=0;let mut functions=ptr::null_mut();
        assert_eq!(unsafe{BCryptEnumContextFunctions(CRYPT_LOCAL,context.as_ptr(),2,&mut fn_size,&mut functions)},STATUS_SUCCESS);
        assert_eq!(unsafe{(*functions).cFunctions},1);unsafe{BCryptFreeBuffer(functions.cast())};
        let mut p_size=0;let mut providers=ptr::null_mut();
        assert_eq!(unsafe{BCryptEnumContextFunctionProviders(CRYPT_LOCAL,context.as_ptr(),2,function.as_ptr(),&mut p_size,&mut providers)},STATUS_SUCCESS);
        assert_eq!(unsafe{(*providers).cProviders},1);unsafe{BCryptFreeBuffer(providers.cast())};
        assert_eq!(unsafe{BCryptRemoveContextFunction(CRYPT_LOCAL,context.as_ptr(),2,function.as_ptr())},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptDeleteContext(CRYPT_LOCAL,context.as_ptr())},STATUS_SUCCESS);
    }

    #[test]
    fn rsa_generate_finalize_sign_verify_and_destroy_lifecycle() {
        let wide=|v:&str|v.encode_utf16().chain(core::iter::once(0)).collect::<Vec<u16>>();
        let name=wide("RSA");let mut provider=ptr::null_mut();
        assert_eq!(unsafe{BCryptOpenAlgorithmProvider(&mut provider,name.as_ptr(),ptr::null(),0)},STATUS_SUCCESS);
        let mut key=ptr::null_mut();
        assert_eq!(unsafe{BCryptGenerateKeyPair(provider,&mut key,1024,0)},STATUS_SUCCESS);
        let algorithm=wide("SHA256");
        let mut padding=BcryptPkcs1PaddingInfo{pszAlgId:algorithm.as_ptr()};
        let digest=HashAlgorithm::Sha256.digest(b"cognacos-rsa-sign-test");
        let mut required=0;
        assert_eq!(unsafe{BCryptSignHash(key,(&mut padding as *mut BcryptPkcs1PaddingInfo).cast::<c_void>(),digest.as_ref().as_ptr() as *mut u8,32,ptr::null_mut(),0,&mut required,2)},STATUS_INVALID_PARAMETER);
        assert_eq!(unsafe{BCryptFinalizeKeyPair(key,0)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptSignHash(key,(&mut padding as *mut BcryptPkcs1PaddingInfo).cast::<c_void>(),digest.as_ref().as_ptr() as *mut u8,32,ptr::null_mut(),0,&mut required,2)},STATUS_SUCCESS);
        assert_eq!(required,128);
        let mut signature=vec![0u8;required as usize];
        assert_eq!(unsafe{BCryptSignHash(key,(&mut padding as *mut BcryptPkcs1PaddingInfo).cast::<c_void>(),digest.as_ref().as_ptr() as *mut u8,32,signature.as_mut_ptr(),required,&mut required,2)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptVerifySignature(key,(&mut padding as *mut BcryptPkcs1PaddingInfo).cast::<c_void>(),digest.as_ref().as_ptr(),32,signature.as_ptr(),signature.len() as ULONG,2)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptDestroyKey(key)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptCloseAlgorithmProvider(provider,0)},STATUS_SUCCESS);
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
        let p = register_handle(RegisteredHandle::Algorithm(AlgorithmHandle { algorithm: HashAlgorithm::Sha256, hmac: false, multi: false, reusable: false, cipher: false, kdf: false, rsa: false, rng: false, gcm: false, cbc: false })).unwrap();
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
    fn aes_gcm_supports_every_windows_tag_length() {
        let wide=|v:&str|v.encode_utf16().chain(core::iter::once(0)).collect::<Vec<u16>>();
        let algorithm=wide("AES");let property=wide("ChainingMode");let mode=wide("ChainingModeGCM");
        let mut provider=ptr::null_mut();
        assert_eq!(unsafe{BCryptOpenAlgorithmProvider(&mut provider,algorithm.as_ptr(),ptr::null(),0)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptSetProperty(provider,property.as_ptr(),mode.as_ptr().cast(),(mode.len()*2) as ULONG,0)},STATUS_SUCCESS);
        let mut key_material=[0x5au8;16];let mut key=ptr::null_mut();
        assert_eq!(unsafe{BCryptGenerateSymmetricKey(provider,&mut key,ptr::null_mut(),0,key_material.as_mut_ptr(),16,0)},STATUS_SUCCESS);
        for tag_len in 12usize..=16 {
            let nonce=[0x33u8;12];let mut tag=[0u8;16];
            let mut info=BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO{cbSize:core::mem::size_of::<BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO>() as ULONG,dwInfoVersion:1,pbNonce:nonce.as_ptr() as *mut u8,cbNonce:12,pbAuthData:ptr::null_mut(),cbAuthData:0,pbTag:tag.as_mut_ptr(),cbTag:tag_len as ULONG,pbMacContext:ptr::null_mut(),cbMacContext:0,cbAAD:0,cbData:0,dwFlags:0};
            let mut plain=*b"tag length test!";let mut encrypted=[0u8;16];let mut written=0;
            assert_eq!(unsafe{BCryptEncrypt(key,plain.as_mut_ptr(),16,(&mut info as *mut BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO).cast::<c_void>(),ptr::null_mut(),0,encrypted.as_mut_ptr(),16,&mut written,0)},STATUS_SUCCESS);
            assert_eq!(written,16);
            let mut recovered=[0u8;16];
            assert_eq!(unsafe{BCryptDecrypt(key,encrypted.as_mut_ptr(),16,(&mut info as *mut BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO).cast::<c_void>(),ptr::null_mut(),0,recovered.as_mut_ptr(),16,&mut written,0)},STATUS_SUCCESS);
            assert_eq!(recovered,plain);
            tag[0]^=1;
            assert_eq!(unsafe{BCryptDecrypt(key,encrypted.as_mut_ptr(),16,(&mut info as *mut BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO).cast::<c_void>(),ptr::null_mut(),0,recovered.as_mut_ptr(),16,&mut written,0)},STATUS_AUTH_TAG_MISMATCH);
        }
        assert_eq!(unsafe{BCryptDestroyKey(key)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptCloseAlgorithmProvider(provider,0)},STATUS_SUCCESS);
    }

    #[test]
    fn aes_gcm_supports_empty_payload_with_null_input_and_output_buffers() {
        let wide=|v:&str|v.encode_utf16().chain(core::iter::once(0)).collect::<Vec<u16>>();
        let algorithm=wide("AES");let property=wide("ChainingMode");let mode=wide("ChainingModeGCM");
        let mut provider=ptr::null_mut();
        assert_eq!(unsafe{BCryptOpenAlgorithmProvider(&mut provider,algorithm.as_ptr(),ptr::null(),0)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptSetProperty(provider,property.as_ptr(),mode.as_ptr().cast(),(mode.len()*2) as ULONG,0)},STATUS_SUCCESS);
        let mut material=[0x17u8;16];let mut key=ptr::null_mut();
        assert_eq!(unsafe{BCryptGenerateSymmetricKey(provider,&mut key,ptr::null_mut(),0,material.as_mut_ptr(),16,0)},STATUS_SUCCESS);
        let nonce=[0x28u8;12];let mut tag=[0u8;16];
        let mut info=BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO{cbSize:core::mem::size_of::<BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO>() as ULONG,dwInfoVersion:1,pbNonce:nonce.as_ptr() as *mut u8,cbNonce:12,pbAuthData:ptr::null_mut(),cbAuthData:0,pbTag:tag.as_mut_ptr(),cbTag:16,pbMacContext:ptr::null_mut(),cbMacContext:0,cbAAD:0,cbData:0,dwFlags:0};
        let mut written=99;
        assert_eq!(unsafe{BCryptEncrypt(key,ptr::null_mut(),0,(&mut info as *mut BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO).cast::<c_void>(),ptr::null_mut(),0,ptr::null_mut(),0,&mut written,0)},STATUS_SUCCESS);
        assert_eq!(written,0);
        assert_eq!(unsafe{BCryptDecrypt(key,ptr::null_mut(),0,(&mut info as *mut BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO).cast::<c_void>(),ptr::null_mut(),0,ptr::null_mut(),0,&mut written,0)},STATUS_SUCCESS);
        assert_eq!(written,0);
        assert_eq!(unsafe{BCryptDestroyKey(key)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptCloseAlgorithmProvider(provider,0)},STATUS_SUCCESS);
    }

    #[test]
    fn aes_gcm_accepts_non_96_bit_nonce_lengths() {
        let wide=|v:&str|v.encode_utf16().chain(core::iter::once(0)).collect::<Vec<u16>>();
        let algorithm=wide("AES");let property=wide("ChainingMode");let mode=wide("ChainingModeGCM");
        let mut provider=ptr::null_mut();
        assert_eq!(unsafe{BCryptOpenAlgorithmProvider(&mut provider,algorithm.as_ptr(),ptr::null(),0)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptSetProperty(provider,property.as_ptr(),mode.as_ptr().cast(),(mode.len()*2) as ULONG,0)},STATUS_SUCCESS);
        let mut material=[0x72u8;16];let mut key=ptr::null_mut();
        assert_eq!(unsafe{BCryptGenerateSymmetricKey(provider,&mut key,ptr::null_mut(),0,material.as_mut_ptr(),16,0)},STATUS_SUCCESS);
        for nonce_len in [8usize,16] {
            let nonce=vec![0x19u8;nonce_len];let mut tag=[0u8;16];
            let mut info=BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO{cbSize:core::mem::size_of::<BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO>() as ULONG,dwInfoVersion:1,pbNonce:nonce.as_ptr() as *mut u8,cbNonce:nonce_len as ULONG,pbAuthData:ptr::null_mut(),cbAuthData:0,pbTag:tag.as_mut_ptr(),cbTag:16,pbMacContext:ptr::null_mut(),cbMacContext:0,cbAAD:0,cbData:0,dwFlags:0};
            let mut plain=*b"non96bit nonce!!";let mut encrypted=[0u8;16];let mut written=0;
            assert_eq!(unsafe{BCryptEncrypt(key,plain.as_mut_ptr(),16,(&mut info as *mut BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO).cast::<c_void>(),ptr::null_mut(),0,encrypted.as_mut_ptr(),16,&mut written,0)},STATUS_SUCCESS);
            let mut recovered=[0u8;16];
            assert_eq!(unsafe{BCryptDecrypt(key,encrypted.as_mut_ptr(),16,(&mut info as *mut BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO).cast::<c_void>(),ptr::null_mut(),0,recovered.as_mut_ptr(),16,&mut written,0)},STATUS_SUCCESS);
            assert_eq!(recovered,plain);
        }
        assert_eq!(unsafe{BCryptDestroyKey(key)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptCloseAlgorithmProvider(provider,0)},STATUS_SUCCESS);
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
    fn aes256_gcm_nist_known_answer_and_roundtrip() {
        let wide=|v:&str|v.encode_utf16().chain(core::iter::once(0)).collect::<Vec<u16>>();
        let algorithm=wide("AES");let property=wide("ChainingMode");let mode=wide("ChainingModeGCM");
        let mut provider=ptr::null_mut();
        assert_eq!(unsafe{BCryptOpenAlgorithmProvider(&mut provider,algorithm.as_ptr(),ptr::null(),0)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptSetProperty(provider,property.as_ptr(),mode.as_ptr().cast(),(mode.len()*2) as ULONG,0)},STATUS_SUCCESS);
        let mut material=[0u8;32];let mut key=ptr::null_mut();
        assert_eq!(unsafe{BCryptGenerateSymmetricKey(provider,&mut key,ptr::null_mut(),0,material.as_mut_ptr(),32,0)},STATUS_SUCCESS);
        let nonce=[0u8;12];let mut tag=[0u8;16];
        let mut info=BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO{cbSize:core::mem::size_of::<BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO>() as ULONG,dwInfoVersion:1,pbNonce:nonce.as_ptr() as *mut u8,cbNonce:12,pbAuthData:ptr::null_mut(),cbAuthData:0,pbTag:tag.as_mut_ptr(),cbTag:16,pbMacContext:ptr::null_mut(),cbMacContext:0,cbAAD:0,cbData:0,dwFlags:0};
        let mut plain=[0u8;16];let mut encrypted=[0u8;16];let mut written=0;
        assert_eq!(unsafe{BCryptEncrypt(key,plain.as_mut_ptr(),16,(&mut info as *mut BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO).cast::<c_void>(),ptr::null_mut(),0,encrypted.as_mut_ptr(),16,&mut written,0)},STATUS_SUCCESS);
        assert_eq!(encrypted,[0xce,0xa7,0x40,0x3d,0x4d,0x60,0x6b,0x6e,0x07,0x4e,0xc5,0xd3,0xba,0xf3,0x9d,0x18]);
        assert_eq!(tag,[0xd0,0xd1,0xc8,0xa7,0x99,0x99,0x6b,0xf0,0x26,0x5b,0x98,0xb5,0xd4,0x8a,0xb9,0x19]);
        let mut recovered=[0u8;16];
        assert_eq!(unsafe{BCryptDecrypt(key,encrypted.as_mut_ptr(),16,(&mut info as *mut BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO).cast::<c_void>(),ptr::null_mut(),0,recovered.as_mut_ptr(),16,&mut written,0)},STATUS_SUCCESS);
        assert_eq!(recovered,plain);
        assert_eq!(unsafe{BCryptDestroyKey(key)},STATUS_SUCCESS);
        assert_eq!(unsafe{BCryptCloseAlgorithmProvider(provider,0)},STATUS_SUCCESS);
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
    fn aes_cbc_roundtrips_all_windows_aes_key_sizes() {
        let wide=|v:&str|v.encode_utf16().chain(core::iter::once(0)).collect::<Vec<u16>>();
        for key_bytes in [16usize,24,32] {
            let name=wide("AES");
            let mut provider=ptr::null_mut();
            assert_eq!(unsafe{BCryptOpenAlgorithmProvider(&mut provider,name.as_ptr(),ptr::null(),0)},STATUS_SUCCESS);
            let property=wide("ChainingMode");
            let mode=wide("ChainingModeCBC");
            assert_eq!(unsafe{BCryptSetProperty(provider,property.as_ptr(),mode.as_ptr().cast(),(mode.len()*2) as ULONG,0)},STATUS_SUCCESS);
            let mut material=vec![0x31u8;key_bytes];
            let mut key=ptr::null_mut();
            assert_eq!(unsafe{BCryptGenerateSymmetricKey(provider,&mut key,ptr::null_mut(),0,material.as_mut_ptr(),key_bytes as ULONG,0)},STATUS_SUCCESS);
            let plain=b"Windows CNG AES CBC ABI test";
            let mut iv=[0x42u8;16];
            let mut encrypted=[0u8;64];
            let mut encrypted_len=0;
            assert_eq!(unsafe{BCryptEncrypt(key,plain.as_ptr() as *mut u8,plain.len() as ULONG,ptr::null_mut(),iv.as_mut_ptr(),16,encrypted.as_mut_ptr(),64,&mut encrypted_len,BCRYPT_BLOCK_PADDING)},STATUS_SUCCESS);
            let mut decrypt_iv=[0x42u8;16];
            let mut decrypted=[0u8;64];
            let mut decrypted_len=0;
            assert_eq!(unsafe{BCryptDecrypt(key,encrypted.as_mut_ptr(),encrypted_len,ptr::null_mut(),decrypt_iv.as_mut_ptr(),16,decrypted.as_mut_ptr(),64,&mut decrypted_len,BCRYPT_BLOCK_PADDING)},STATUS_SUCCESS);
            assert_eq!(&decrypted[..decrypted_len as usize],plain);
            assert_eq!(unsafe{BCryptDestroyKey(key)},STATUS_SUCCESS);
            assert_eq!(unsafe{BCryptCloseAlgorithmProvider(provider,0)},STATUS_SUCCESS);
        }
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
