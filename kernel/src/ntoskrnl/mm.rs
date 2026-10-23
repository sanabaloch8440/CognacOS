//! Virtual/physical memory manager placeholder exports.
pub const NAMES: &[&str] = &[
"MmAllocateContiguousMemory","MmAllocateContiguousMemorySpecifyCache","MmAllocatePagesForMdl",
"MmAllocatePagesForMdlEx","MmBuildMdlForNonPagedPool","MmFreeContiguousMemory","MmFreeContiguousMemorySpecifyCache",
"MmFreePagesFromMdl","MmGetPhysicalAddress","MmGetVirtualForPhysical","MmGetPhysicalMemoryRanges",
"MmGetPhysicalMemoryRangesEx","MmGetSystemRoutineAddress","MmGetSystemAddressForMdlSafe","MmMapIoSpace",
"MmMapIoSpaceEx","MmUnmapIoSpace","MmMapLockedPagesSpecifyCache","MmUnmapLockedPages","MmProbeAndLockPages",
"MmUnlockPages","MmProtectMdlSystemAddress","MmIsAddressValid","MmIsNonPagedSystemAddressValid",
"MmIsNonPagedPoolAddress","MmIsSystemAddressLocked","MmGetMdlByteCount","MmGetMdlVirtualAddress",
"MmGetMdlPhysicalAddress","MmGetMdlPfnArray","MmGetPhysicalAddress","MmAllocateMappingAddress",
"MmFreeMappingAddress","MmGetVirtualForPhysical","MmCopyMemory","MmSecureVirtualMemory","MmUnsecureVirtualMemory",
"MmGetSystemRoutineAddress","MmGetSystemPageFile","MmGetPageFileInformation","MmQuerySystemSize",
"MmGetMaximumFileSectionSize","MmSectionObjectType","MmSystemRangeStart","MmUserProbeAddress",
"MmHighestUserAddress","MmLowestUserAddress","MmGetPhysicalMemoryRanges","MmGetNumberOfPhysicalPages",
];
