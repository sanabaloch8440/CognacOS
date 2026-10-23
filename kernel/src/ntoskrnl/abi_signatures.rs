//! Publicly documented NT kernel ABI declarations and supported signature registry.
//! Function-pointer aliases are x64 ABI types; aliases alone do not implement routines.
use core::ffi::c_void;

pub type NtStatus = i32;
pub type Handle = *mut c_void;
pub type Pvoid = *mut c_void;
pub type Ulong = u32;
pub type Ushort = u16;
pub type UlongPtr = u64;
pub type SizeT = u64;
pub type Boolean = u8;
pub type Kirql = u8;
pub type LargeInteger = i64;

pub type KeGetCurrentIrqlFn = unsafe extern "win64" fn() -> Kirql;
pub type KeQueryActiveProcessorCountExFn = unsafe extern "win64" fn(Ushort) -> Ulong;
pub type KeQueryActiveProcessorCountFn = unsafe extern "win64" fn() -> Ulong;
pub type KeQueryActiveGroupCountFn = unsafe extern "win64" fn() -> Ushort;
#[repr(C)] pub struct ProcessorNumber { pub group: Ushort, pub number: u8, pub reserved: u8 }
pub type KeGetCurrentProcessorNumberExFn = unsafe extern "win64" fn(*mut ProcessorNumber);
pub type KeGetCurrentProcessorIndexFn = unsafe extern "win64" fn() -> Ulong;
pub type KeStallExecutionProcessorFn = unsafe extern "win64" fn(Ulong);
pub type ExAllocatePool2Fn = unsafe extern "win64" fn(UlongPtr, SizeT, Ulong) -> Pvoid;\npub type ExAllocatePool3Fn = unsafe extern "win64" fn(UlongPtr, SizeT, Ulong, *const c_void, Ulong) -> Pvoid;\npub type ExAllocatePoolWithTagFn = unsafe extern "win64" fn(Ulong, SizeT, Ulong) -> Pvoid;\npub type ExAllocatePoolWithTagPriorityFn = unsafe extern "win64" fn(Ulong, SizeT, Ulong, Ulong) -> Pvoid;\npub type ExFreePool2Fn = unsafe extern "win64" fn(Pvoid, Ulong, *const c_void, Ulong);
pub type ExFreePoolWithTagFn = unsafe extern "win64" fn(Pvoid, Ulong);
pub type MmGetPhysicalAddressFn = unsafe extern "win64" fn(Pvoid) -> u64;
pub type ObfReferenceObjectFn = unsafe extern "win64" fn(Pvoid);
pub type ObfDereferenceObjectFn = unsafe extern "win64" fn(Pvoid);
pub type PsGetCurrentProcessIdFn = unsafe extern "win64" fn() -> Handle;
pub type PsGetCurrentThreadIdFn = unsafe extern "win64" fn() -> Handle;
pub type ZwCloseFn = unsafe extern "win64" fn(Handle) -> NtStatus;
pub type ZwWaitForSingleObjectFn = unsafe extern "win64" fn(Handle, Boolean, *const LargeInteger) -> NtStatus;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AbiSignature { KeGetCurrentIrql, KeQueryActiveProcessorCountEx, KeQueryActiveProcessorCount, KeQueryActiveGroupCount, KeGetCurrentProcessorNumberEx, KeGetCurrentProcessorIndex, KeStallExecutionProcessor, ExAllocatePool2, ExAllocatePool3, ExAllocatePoolWithTag, ExAllocatePoolWithTagPriority, ExFreePool2, ExFreePoolWithTag, MmGetPhysicalAddress, ObfReferenceObject, ObfDereferenceObject, PsGetCurrentProcessId, PsGetCurrentThreadId, ZwClose, ZwWaitForSingleObject }

pub fn documented_signature(name: &str) -> Option<AbiSignature> {
    Some(match name {
        "KeGetCurrentIrql" => AbiSignature::KeGetCurrentIrql,
        "KeQueryActiveProcessorCountEx" => AbiSignature::KeQueryActiveProcessorCountEx,
        "KeQueryActiveProcessorCount" => AbiSignature::KeQueryActiveProcessorCount,
        "KeQueryActiveGroupCount" => AbiSignature::KeQueryActiveGroupCount,
        "KeGetCurrentProcessorNumberEx" => AbiSignature::KeGetCurrentProcessorNumberEx,
        "KeGetCurrentProcessorIndex" => AbiSignature::KeGetCurrentProcessorIndex,
        "KeStallExecutionProcessor" => AbiSignature::KeStallExecutionProcessor,
        "ExAllocatePool2" => AbiSignature::ExAllocatePool2,\n        "ExAllocatePool3" => AbiSignature::ExAllocatePool3,\n        "ExAllocatePoolWithTag" => AbiSignature::ExAllocatePoolWithTag,\n        "ExAllocatePoolWithTagPriority" => AbiSignature::ExAllocatePoolWithTagPriority,\n        "ExFreePool2" => AbiSignature::ExFreePool2,
        "ExFreePoolWithTag" => AbiSignature::ExFreePoolWithTag,
        "MmGetPhysicalAddress" => AbiSignature::MmGetPhysicalAddress,
        "ObfReferenceObject" => AbiSignature::ObfReferenceObject,
        "ObfDereferenceObject" => AbiSignature::ObfDereferenceObject,
        "PsGetCurrentProcessId" => AbiSignature::PsGetCurrentProcessId,
        "PsGetCurrentThreadId" => AbiSignature::PsGetCurrentThreadId,
        "ZwClose" => AbiSignature::ZwClose,
        "ZwWaitForSingleObject" => AbiSignature::ZwWaitForSingleObject,
        _ => return None,
    })
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn documented_signatures_are_registered() {
        for name in ["KeGetCurrentIrql","KeQueryActiveProcessorCountEx","KeQueryActiveProcessorCount","KeQueryActiveGroupCount","KeGetCurrentProcessorNumberEx","KeGetCurrentProcessorIndex","KeStallExecutionProcessor","ExAllocatePool2","ExAllocatePool3","ExAllocatePoolWithTag","ExAllocatePoolWithTagPriority","ExFreePool2","ExFreePoolWithTag","MmGetPhysicalAddress","ObfReferenceObject","ObfDereferenceObject","PsGetCurrentProcessId","PsGetCurrentThreadId","ZwClose","ZwWaitForSingleObject"] { assert!(documented_signature(name).is_some(), "missing signature for {name}"); }
        assert!(documented_signature("UnknownRoutine").is_none());
    }
}
