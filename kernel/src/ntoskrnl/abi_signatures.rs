//! ABI signatures transcribed from publicly documented WDK kernel routines.
//! These aliases preserve the x64 argument/return ABI for the listed routines only.
//! They are signature metadata, not implementations and are not yet used as callable dispatch targets.
//! References: Microsoft Learn WDK documentation for each named routine.
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
pub type KeStallExecutionProcessorFn = unsafe extern "win64" fn(Ulong);
pub type ExAllocatePool2Fn = unsafe extern "win64" fn(UlongPtr, SizeT, Ulong) -> Pvoid;
pub type ExFreePoolWithTagFn = unsafe extern "win64" fn(Pvoid, Ulong);
pub type MmGetPhysicalAddressFn = unsafe extern "win64" fn(Pvoid) -> u64;
pub type ObfReferenceObjectFn = unsafe extern "win64" fn(Pvoid);
pub type ObfDereferenceObjectFn = unsafe extern "win64" fn(Pvoid);
pub type PsGetCurrentProcessIdFn = unsafe extern "win64" fn() -> Handle;
pub type PsGetCurrentThreadIdFn = unsafe extern "win64" fn() -> Handle;
pub type ZwCloseFn = unsafe extern "win64" fn(Handle) -> NtStatus;
pub type ZwWaitForSingleObjectFn = unsafe extern "win64" fn(Handle, Boolean, *const LargeInteger) -> NtStatus;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AbiSignature { KeGetCurrentIrql, KeQueryActiveProcessorCountEx, KeStallExecutionProcessor, ExAllocatePool2, ExFreePoolWithTag, MmGetPhysicalAddress, ObfReferenceObject, ObfDereferenceObject, PsGetCurrentProcessId, PsGetCurrentThreadId, ZwClose, ZwWaitForSingleObject }

pub fn documented_signature(name: &str) -> Option<AbiSignature> {
    Some(match name {
        "KeGetCurrentIrql" => AbiSignature::KeGetCurrentIrql,
        "KeQueryActiveProcessorCountEx" => AbiSignature::KeQueryActiveProcessorCountEx,
        "KeStallExecutionProcessor" => AbiSignature::KeStallExecutionProcessor,
        "ExAllocatePool2" => AbiSignature::ExAllocatePool2,
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn documented_signatures_are_registered() {
        for name in ["KeGetCurrentIrql", "KeQueryActiveProcessorCountEx", "KeStallExecutionProcessor", "ExAllocatePool2", "ExFreePoolWithTag", "MmGetPhysicalAddress", "ObfReferenceObject", "ObfDereferenceObject", "PsGetCurrentProcessId", "PsGetCurrentThreadId", "ZwClose", "ZwWaitForSingleObject"] { assert!(documented_signature(name).is_some(), "missing signature for {name}"); }
        assert!(documented_signature("UndocumentedOrUnknownRoutine").is_none());
    }
}
