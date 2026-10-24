//! Publicly documented NT kernel ABI declarations and supported signature registry.
//! Function-pointer aliases are x64 ABI types; aliases alone do not implement routines.
use core::ffi::c_void;
use crate::ntoskrnl::se::runtime::{SeGenericMapping, SeSecuritySubjectContext};

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
pub type ExAllocatePool2Fn = unsafe extern "win64" fn(UlongPtr, SizeT, Ulong) -> Pvoid;
pub type ExAllocatePool3Fn = unsafe extern "win64" fn(UlongPtr, SizeT, Ulong, *const c_void, Ulong) -> Pvoid;
pub type ExAllocatePoolWithTagFn = unsafe extern "win64" fn(Ulong, SizeT, Ulong) -> Pvoid;
pub type ExAllocatePoolWithTagPriorityFn = unsafe extern "win64" fn(Ulong, SizeT, Ulong, Ulong) -> Pvoid;
pub type ExFreePool2Fn = unsafe extern "win64" fn(Pvoid, Ulong, *const c_void, Ulong);
pub type ExFreePoolWithTagFn = unsafe extern "win64" fn(Pvoid, Ulong);
pub type MmGetPhysicalAddressFn = unsafe extern "win64" fn(Pvoid) -> u64;
pub type MmAllocateContiguousMemoryFn = unsafe extern "win64" fn(SizeT, LargeInteger) -> Pvoid;
pub type MmAllocateContiguousMemorySpecifyCacheFn = unsafe extern "win64" fn(SizeT, LargeInteger, LargeInteger, Ulong) -> Pvoid;
pub type MmFreeContiguousMemoryFn = unsafe extern "win64" fn(Pvoid);
pub type MmFreeContiguousMemorySpecifyCacheFn = unsafe extern "win64" fn(Pvoid, SizeT, Ulong);
pub type MmSizeOfMdlFn = unsafe extern "win64" fn(Pvoid, SizeT) -> Ulong;
pub type MmGetVirtualForPhysicalFn = unsafe extern "win64" fn(u64) -> Pvoid;
pub type MmIsAddressValidFn = unsafe extern "win64" fn(Pvoid) -> Boolean;
pub type MmIsNonPagedSystemAddressValidFn = unsafe extern "win64" fn(Pvoid) -> Boolean;
pub type MmCopyMemoryFn = unsafe extern "win64" fn(Pvoid, u64, SizeT, Ulong, *mut SizeT) -> NtStatus;
pub type MmBuildMdlForNonPagedPoolFn = unsafe extern "win64" fn(Pvoid);
pub type MmAdvanceMdlFn = unsafe extern "win64" fn(Pvoid, Ulong);
pub type MmCreateMdlFn = unsafe extern "win64" fn(Pvoid, Pvoid, SizeT) -> Pvoid;
pub type MmFreeMdlFn = unsafe extern "win64" fn(Pvoid);
pub type MmPrepareMdlForReuseFn = unsafe extern "win64" fn(Pvoid);
pub type MmBuildPartialMdlFn = unsafe extern "win64" fn(Pvoid, Pvoid, Pvoid, Ulong);
pub type MmProbeAndLockPagesFn = unsafe extern "win64" fn(Pvoid, i8, Ulong);
pub type MmUnlockPagesFn = unsafe extern "win64" fn(Pvoid);
pub type MmMapLockedPagesSpecifyCacheFn = unsafe extern "win64" fn(Pvoid, i8, i32, Pvoid, Boolean, Ulong) -> Pvoid;
pub type MmUnmapLockedPagesFn = unsafe extern "win64" fn(Pvoid, Pvoid);
pub type MmCopyVirtualMemoryFn = unsafe extern "win64" fn(Pvoid, Pvoid, Pvoid, Pvoid, SizeT, i8, *mut SizeT) -> NtStatus;
pub type ObfReferenceObjectFn = unsafe extern "win64" fn(Pvoid);
pub type ObfDereferenceObjectFn = unsafe extern "win64" fn(Pvoid);
pub type PsLookupProcessByProcessIdFn = unsafe extern "win64" fn(Handle, *mut Pvoid) -> NtStatus;
pub type PsGetCurrentProcessIdFn = unsafe extern "win64" fn() -> Handle;
pub type PsGetCurrentThreadIdFn = unsafe extern "win64" fn() -> Handle;
pub type ZwCloseFn = unsafe extern "win64" fn(Handle) -> NtStatus;
pub type ZwWaitForSingleObjectFn = unsafe extern "win64" fn(Handle, Boolean, *const LargeInteger) -> NtStatus;
pub type SeAccessCheckFn = unsafe extern "win64" fn(
    *const c_void, *mut SeSecuritySubjectContext, *mut Boolean, Ulong, Ulong,
    *mut *mut c_void, *const SeGenericMapping, i8, *mut Ulong, *mut NtStatus,
) -> Boolean;
pub type SePrivilegeCheckFn = unsafe extern "win64" fn(*const c_void, *mut SeSecuritySubjectContext, i8) -> Boolean;
pub type SeSinglePrivilegeCheckFn = unsafe extern "win64" fn(u64, i8) -> Boolean;
pub type SeValidSecurityDescriptorFn = unsafe extern "win64" fn(*const c_void) -> Boolean;
pub type SeSubjectContextFn = unsafe extern "win64" fn(*mut SeSecuritySubjectContext);
pub type PsAssignImpersonationTokenFn = unsafe extern "win64" fn(Handle, Handle) -> NtStatus;
pub type PsReferencePrimaryTokenFn = unsafe extern "win64" fn(Pvoid) -> Pvoid;
pub type PsDereferencePrimaryTokenFn = unsafe extern "win64" fn(Pvoid);


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AbiSignature { MmAllocateContiguousMemory, MmAllocateContiguousMemorySpecifyCache, MmFreeContiguousMemory, MmFreeContiguousMemorySpecifyCache, MmSizeOfMdl, MmGetPhysicalAddress, MmGetVirtualForPhysical, MmIsAddressValid, MmIsNonPagedSystemAddressValid, MmCopyMemory, MmBuildMdlForNonPagedPool, MmAdvanceMdl, MmCreateMdl, MmFreeMdl, MmPrepareMdlForReuse, MmBuildPartialMdl, MmProbeAndLockPages, MmUnlockPages, MmMapLockedPagesSpecifyCache, MmUnmapLockedPages, MmCopyVirtualMemory, PsLookupProcessByProcessId, KeGetCurrentIrql, KeQueryActiveProcessorCountEx, KeQueryActiveProcessorCount, KeQueryActiveGroupCount, KeGetCurrentProcessorNumberEx, KeGetCurrentProcessorIndex, KeStallExecutionProcessor, ExAllocatePool2, ExAllocatePool3, ExAllocatePoolWithTag, ExAllocatePoolWithTagPriority, ExFreePool2, ExFreePoolWithTag, ObfReferenceObject, ObfDereferenceObject, PsGetCurrentProcessId, PsGetCurrentThreadId, ZwClose, ZwWaitForSingleObject, SeAccessCheck, SePrivilegeCheck, SeSinglePrivilegeCheck, SeValidSecurityDescriptor, SeCaptureSubjectContext, SeReleaseSubjectContext, SeLockSubjectContext, SeUnlockSubjectContext, PsAssignImpersonationToken, PsReferencePrimaryToken, PsDereferencePrimaryToken }

pub fn documented_signature(name: &str) -> Option<AbiSignature> {
    Some(match name {
        "KeGetCurrentIrql" => AbiSignature::KeGetCurrentIrql,
        "KeQueryActiveProcessorCountEx" => AbiSignature::KeQueryActiveProcessorCountEx,
        "KeQueryActiveProcessorCount" => AbiSignature::KeQueryActiveProcessorCount,
        "KeQueryActiveGroupCount" => AbiSignature::KeQueryActiveGroupCount,
        "KeGetCurrentProcessorNumberEx" => AbiSignature::KeGetCurrentProcessorNumberEx,
        "KeGetCurrentProcessorIndex" => AbiSignature::KeGetCurrentProcessorIndex,
        "KeStallExecutionProcessor" => AbiSignature::KeStallExecutionProcessor,
        "ExAllocatePool2" => AbiSignature::ExAllocatePool2,
        "ExAllocatePool3" => AbiSignature::ExAllocatePool3,
        "ExAllocatePoolWithTag" => AbiSignature::ExAllocatePoolWithTag,
        "ExAllocatePoolWithTagPriority" => AbiSignature::ExAllocatePoolWithTagPriority,
        "ExFreePool2" => AbiSignature::ExFreePool2,
        "ExFreePoolWithTag" => AbiSignature::ExFreePoolWithTag,
        "MmAllocateContiguousMemory" => AbiSignature::MmAllocateContiguousMemory,
        "MmAllocateContiguousMemorySpecifyCache" => AbiSignature::MmAllocateContiguousMemorySpecifyCache,
        "MmFreeContiguousMemory" => AbiSignature::MmFreeContiguousMemory,
        "MmFreeContiguousMemorySpecifyCache" => AbiSignature::MmFreeContiguousMemorySpecifyCache,
        "MmSizeOfMdl" => AbiSignature::MmSizeOfMdl,
        "MmGetPhysicalAddress" => AbiSignature::MmGetPhysicalAddress,
        "MmGetVirtualForPhysical" => AbiSignature::MmGetVirtualForPhysical,
        "MmIsAddressValid" => AbiSignature::MmIsAddressValid,
        "MmIsNonPagedSystemAddressValid" => AbiSignature::MmIsNonPagedSystemAddressValid,
        "MmCopyMemory" => AbiSignature::MmCopyMemory,
        "MmBuildMdlForNonPagedPool" => AbiSignature::MmBuildMdlForNonPagedPool,
        "MmAdvanceMdl" => AbiSignature::MmAdvanceMdl,
        "MmCreateMdl" => AbiSignature::MmCreateMdl,
        "MmFreeMdl" => AbiSignature::MmFreeMdl,
        "MmPrepareMdlForReuse" => AbiSignature::MmPrepareMdlForReuse,
        "MmBuildPartialMdl" => AbiSignature::MmBuildPartialMdl,
        "MmProbeAndLockPages" => AbiSignature::MmProbeAndLockPages,
        "MmUnlockPages" => AbiSignature::MmUnlockPages,
        "MmMapLockedPagesSpecifyCache" => AbiSignature::MmMapLockedPagesSpecifyCache,
        "MmUnmapLockedPages" => AbiSignature::MmUnmapLockedPages,
        "MmCopyVirtualMemory" => AbiSignature::MmCopyVirtualMemory,
        "PsLookupProcessByProcessId" => AbiSignature::PsLookupProcessByProcessId,
        "ObfReferenceObject" => AbiSignature::ObfReferenceObject,
        "ObfDereferenceObject" => AbiSignature::ObfDereferenceObject,
        "PsGetCurrentProcessId" => AbiSignature::PsGetCurrentProcessId,
        "PsGetCurrentThreadId" => AbiSignature::PsGetCurrentThreadId,
        "ZwClose" => AbiSignature::ZwClose,
        "ZwWaitForSingleObject" => AbiSignature::ZwWaitForSingleObject,
        "SeAccessCheck" => AbiSignature::SeAccessCheck,
        "SePrivilegeCheck" => AbiSignature::SePrivilegeCheck,
        "SeSinglePrivilegeCheck" => AbiSignature::SeSinglePrivilegeCheck,
        "SeValidSecurityDescriptor" => AbiSignature::SeValidSecurityDescriptor,
        "SeCaptureSubjectContext" => AbiSignature::SeCaptureSubjectContext,
        "SeReleaseSubjectContext" => AbiSignature::SeReleaseSubjectContext,
        "SeLockSubjectContext" => AbiSignature::SeLockSubjectContext,
        "SeUnlockSubjectContext" => AbiSignature::SeUnlockSubjectContext,
        "PsAssignImpersonationToken" => AbiSignature::PsAssignImpersonationToken,
        "PsReferencePrimaryToken" => AbiSignature::PsReferencePrimaryToken,
        "PsDereferencePrimaryToken" => AbiSignature::PsDereferencePrimaryToken,

        _ => return None,
    })
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn documented_signatures_are_registered() {
        for name in ["MmAllocateContiguousMemory","MmAllocateContiguousMemorySpecifyCache","MmFreeContiguousMemory","MmFreeContiguousMemorySpecifyCache","MmSizeOfMdl","MmGetPhysicalAddress","MmGetVirtualForPhysical","MmIsAddressValid","MmIsNonPagedSystemAddressValid","MmCopyMemory","MmBuildMdlForNonPagedPool","MmAdvanceMdl","MmCreateMdl","MmFreeMdl","MmPrepareMdlForReuse","MmBuildPartialMdl","MmProbeAndLockPages","MmUnlockPages","MmMapLockedPagesSpecifyCache","MmUnmapLockedPages","MmCopyVirtualMemory","PsLookupProcessByProcessId","KeGetCurrentIrql","KeQueryActiveProcessorCountEx","KeQueryActiveProcessorCount","KeQueryActiveGroupCount","KeGetCurrentProcessorNumberEx","KeGetCurrentProcessorIndex","KeStallExecutionProcessor","ExAllocatePool2","ExAllocatePool3","ExAllocatePoolWithTag","ExAllocatePoolWithTagPriority","ExFreePool2","ExFreePoolWithTag","ObfReferenceObject","ObfDereferenceObject","PsGetCurrentProcessId","PsGetCurrentThreadId","ZwClose","ZwWaitForSingleObject"] { assert!(documented_signature(name).is_some(), "missing signature for {name}"); }
        assert!(documented_signature("UnknownRoutine").is_none());
    }
    #[test]
    fn implemented_security_exports_match_declared_abis() {
        let _: SeAccessCheckFn = crate::ntoskrnl::se::runtime::SeAccessCheck;
        let _: SePrivilegeCheckFn = crate::ntoskrnl::se::runtime::SePrivilegeCheck;
        let _: SeSinglePrivilegeCheckFn = crate::ntoskrnl::se::runtime::SeSinglePrivilegeCheck;
        let _: SeValidSecurityDescriptorFn = crate::ntoskrnl::se::runtime::SeValidSecurityDescriptor;
        let _: SeSubjectContextFn = crate::ntoskrnl::se::runtime::SeCaptureSubjectContext;
        let _: SeSubjectContextFn = crate::ntoskrnl::se::runtime::SeReleaseSubjectContext;
        let _: SeSubjectContextFn = crate::ntoskrnl::se::runtime::SeLockSubjectContext;
        let _: SeSubjectContextFn = crate::ntoskrnl::se::runtime::SeUnlockSubjectContext;
        let _: PsAssignImpersonationTokenFn = crate::thread::PsAssignImpersonationToken;
        let _: PsReferencePrimaryTokenFn = crate::process::PsReferencePrimaryToken;
        let _: PsDereferencePrimaryTokenFn = crate::process::PsDereferencePrimaryToken;
        for name in ["SeAccessCheck", "SePrivilegeCheck", "SeSinglePrivilegeCheck", "SeValidSecurityDescriptor", "SeCaptureSubjectContext",
            "SeReleaseSubjectContext", "SeLockSubjectContext", "SeUnlockSubjectContext",
            "PsAssignImpersonationToken", "PsReferencePrimaryToken", "PsDereferencePrimaryToken"] {
            assert!(documented_signature(name).is_some(), "missing ABI registry entry for {name}");
        }
    }
}
