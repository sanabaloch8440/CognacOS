//! Windows 11 x64 HAL export compatibility catalogue.
//!
//! The modern Windows x64 HAL forwards these exports to ntoskrnl. This is a
//! name/ordinal inventory only: it does not claim Windows-compatible semantics.
//! No export is callable until its exact ABI and behavior are implemented;
//! generic function pointers would risk corrupting driver calls.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StubState { NotImplemented, Implemented }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HalExport { pub ordinal: u16, pub name: &'static str, pub state: StubState }

/// Windows 11 x64 10.0.26100 HAL exports, in ordinal order.
pub const EXPORTS: &[HalExport] = &[
    HalExport { ordinal: 1, name: "HalAcpiGetTableEx", state: StubState::NotImplemented },
    HalExport { ordinal: 2, name: "HalAcquireDisplayOwnership", state: StubState::NotImplemented },
    HalExport { ordinal: 3, name: "HalAdjustResourceList", state: StubState::NotImplemented },
    HalExport { ordinal: 4, name: "HalAllProcessorsStarted", state: StubState::NotImplemented },
    HalExport { ordinal: 5, name: "HalAllocateAdapterChannel", state: StubState::NotImplemented },
    HalExport { ordinal: 6, name: "HalAllocateCommonBuffer", state: StubState::NotImplemented },
    HalExport { ordinal: 7, name: "HalAllocateCrashDumpRegisters", state: StubState::NotImplemented },
    HalExport { ordinal: 8, name: "HalAllocateHardwareCounters", state: StubState::NotImplemented },
    HalExport { ordinal: 9, name: "HalAssignSlotResources", state: StubState::NotImplemented },
    HalExport { ordinal: 10, name: "HalBugCheckSystem", state: StubState::NotImplemented },
    HalExport { ordinal: 11, name: "HalCalibratePerformanceCounter", state: StubState::NotImplemented },
    HalExport { ordinal: 12, name: "HalClearSoftwareInterrupt", state: StubState::NotImplemented },
    HalExport { ordinal: 13, name: "HalConvertDeviceIdtToIrql", state: StubState::NotImplemented },
    HalExport { ordinal: 14, name: "HalDisableInterrupt", state: StubState::NotImplemented },
    HalExport { ordinal: 15, name: "HalDisplayString", state: StubState::NotImplemented },
    HalExport { ordinal: 16, name: "HalDmaAllocateCrashDumpRegistersEx", state: StubState::NotImplemented },
    HalExport { ordinal: 17, name: "HalDmaFreeCrashDumpRegistersEx", state: StubState::NotImplemented },
    HalExport { ordinal: 18, name: "HalEnableInterrupt", state: StubState::NotImplemented },
    HalExport { ordinal: 19, name: "HalEnumerateEnvironmentVariablesEx", state: StubState::NotImplemented },
    HalExport { ordinal: 20, name: "HalEnumerateProcessors", state: StubState::NotImplemented },
    HalExport { ordinal: 21, name: "HalFlushCommonBuffer", state: StubState::NotImplemented },
    HalExport { ordinal: 22, name: "HalFreeCommonBuffer", state: StubState::NotImplemented },
    HalExport { ordinal: 23, name: "HalFreeHardwareCounters", state: StubState::NotImplemented },
    HalExport { ordinal: 24, name: "HalGetAdapter", state: StubState::NotImplemented },
    HalExport { ordinal: 25, name: "HalGetBusData", state: StubState::NotImplemented },
    HalExport { ordinal: 26, name: "HalGetBusDataByOffset", state: StubState::NotImplemented },
    HalExport { ordinal: 27, name: "HalGetEnvironmentVariable", state: StubState::NotImplemented },
    HalExport { ordinal: 28, name: "HalGetEnvironmentVariableEx", state: StubState::NotImplemented },
    HalExport { ordinal: 29, name: "HalGetInterruptTargetInformation", state: StubState::NotImplemented },
    HalExport { ordinal: 30, name: "HalGetInterruptVector", state: StubState::NotImplemented },
    HalExport { ordinal: 31, name: "HalGetMemoryCachingRequirements", state: StubState::NotImplemented },
    HalExport { ordinal: 32, name: "HalGetMessageRoutingInfo", state: StubState::NotImplemented },
    HalExport { ordinal: 33, name: "HalGetProcessorIdByNtNumber", state: StubState::NotImplemented },
    HalExport { ordinal: 34, name: "HalGetVectorInput", state: StubState::NotImplemented },
    HalExport { ordinal: 35, name: "HalHandleMcheck", state: StubState::NotImplemented },
    HalExport { ordinal: 36, name: "HalHandleNMI", state: StubState::NotImplemented },
    HalExport { ordinal: 37, name: "HalInitSystem", state: StubState::NotImplemented },
    HalExport { ordinal: 38, name: "HalInitializeBios", state: StubState::NotImplemented },
    HalExport { ordinal: 39, name: "HalInitializeOnResume", state: StubState::NotImplemented },
    HalExport { ordinal: 40, name: "HalInitializeProcessor", state: StubState::NotImplemented },
    HalExport { ordinal: 41, name: "HalIsHyperThreadingEnabled", state: StubState::NotImplemented },
    HalExport { ordinal: 42, name: "HalMakeBeep", state: StubState::NotImplemented },
    HalExport { ordinal: 43, name: "HalPerformEndOfInterrupt", state: StubState::NotImplemented },
    HalExport { ordinal: 44, name: "HalProcessorIdle", state: StubState::NotImplemented },
    HalExport { ordinal: 45, name: "HalQueryDisplayParameters", state: StubState::NotImplemented },
    HalExport { ordinal: 46, name: "HalQueryEnvironmentVariableInfoEx", state: StubState::NotImplemented },
    HalExport { ordinal: 47, name: "HalQueryMaximumProcessorCount", state: StubState::Implemented },
    HalExport { ordinal: 48, name: "HalQueryRealTimeClock", state: StubState::NotImplemented },
    HalExport { ordinal: 49, name: "HalReadDmaCounter", state: StubState::NotImplemented },
    HalExport { ordinal: 50, name: "HalRegisterDynamicProcessor", state: StubState::NotImplemented },
    HalExport { ordinal: 51, name: "HalRegisterErrataCallbacks", state: StubState::NotImplemented },
    HalExport { ordinal: 52, name: "HalReportResourceUsage", state: StubState::NotImplemented },
    HalExport { ordinal: 53, name: "HalRequestClockInterrupt", state: StubState::NotImplemented },
    HalExport { ordinal: 54, name: "HalRequestDeferredRecoveryServiceInterrupt", state: StubState::NotImplemented },
    HalExport { ordinal: 55, name: "HalRequestIpi", state: StubState::NotImplemented },
    HalExport { ordinal: 56, name: "HalRequestIpiSpecifyVector", state: StubState::NotImplemented },
    HalExport { ordinal: 57, name: "HalRequestSoftwareInterrupt", state: StubState::NotImplemented },
    HalExport { ordinal: 58, name: "HalReturnToFirmware", state: StubState::NotImplemented },
    HalExport { ordinal: 59, name: "HalSendNMI", state: StubState::NotImplemented },
    HalExport { ordinal: 60, name: "HalSendSoftwareInterrupt", state: StubState::NotImplemented },
    HalExport { ordinal: 61, name: "HalSetBusData", state: StubState::NotImplemented },
    HalExport { ordinal: 62, name: "HalSetBusDataByOffset", state: StubState::NotImplemented },
    HalExport { ordinal: 63, name: "HalSetDisplayParameters", state: StubState::NotImplemented },
    HalExport { ordinal: 64, name: "HalSetEnvironmentVariable", state: StubState::NotImplemented },
    HalExport { ordinal: 65, name: "HalSetEnvironmentVariableEx", state: StubState::NotImplemented },
    HalExport { ordinal: 66, name: "HalSetProfileInterval", state: StubState::NotImplemented },
    HalExport { ordinal: 67, name: "HalSetRealTimeClock", state: StubState::NotImplemented },
    HalExport { ordinal: 68, name: "HalStartDynamicProcessor", state: StubState::NotImplemented },
    HalExport { ordinal: 69, name: "HalStartNextProcessor", state: StubState::NotImplemented },
    HalExport { ordinal: 70, name: "HalStartProfileInterrupt", state: StubState::NotImplemented },
    HalExport { ordinal: 71, name: "HalStopProfileInterrupt", state: StubState::NotImplemented },
    HalExport { ordinal: 72, name: "HalSystemVectorDispatchEntry", state: StubState::NotImplemented },
    HalExport { ordinal: 73, name: "HalTranslateBusAddress", state: StubState::NotImplemented },
    HalExport { ordinal: 74, name: "HalWheaUpdateCmciPolicy", state: StubState::NotImplemented },
    HalExport { ordinal: 75, name: "IoFlushAdapterBuffers", state: StubState::NotImplemented },
    HalExport { ordinal: 76, name: "IoFreeAdapterChannel", state: StubState::NotImplemented },
    HalExport { ordinal: 77, name: "IoFreeMapRegisters", state: StubState::NotImplemented },
    HalExport { ordinal: 78, name: "IoMapTransfer", state: StubState::NotImplemented },
    HalExport { ordinal: 79, name: "IoReadPartitionTable", state: StubState::NotImplemented },
    HalExport { ordinal: 80, name: "IoSetPartitionInformation", state: StubState::NotImplemented },
    HalExport { ordinal: 81, name: "IoWritePartitionTable", state: StubState::NotImplemented },
    HalExport { ordinal: 82, name: "KdComPortInUse", state: StubState::NotImplemented },
    HalExport { ordinal: 83, name: "KdHvComPortInUse", state: StubState::NotImplemented },
    HalExport { ordinal: 84, name: "KeFlushWriteBuffer", state: StubState::Implemented },
    HalExport { ordinal: 85, name: "KeQueryPerformanceCounter", state: StubState::NotImplemented },
    HalExport { ordinal: 86, name: "KeStallExecutionProcessor", state: StubState::NotImplemented },
    HalExport { ordinal: 87, name: "x86BiosAllocateBuffer", state: StubState::NotImplemented },
    HalExport { ordinal: 88, name: "x86BiosCall", state: StubState::NotImplemented },
    HalExport { ordinal: 89, name: "x86BiosFreeBuffer", state: StubState::NotImplemented },
    HalExport { ordinal: 90, name: "x86BiosReadMemory", state: StubState::NotImplemented },
    HalExport { ordinal: 91, name: "x86BiosWriteMemory", state: StubState::NotImplemented },
];

/// Return the maximum logical-processor capacity exposed by this HAL.
pub unsafe extern "win64" fn hal_query_maximum_processor_count() -> u32 {
    super::MAX_CPUS as u32
}

/// Full memory barrier used by the Windows x64 write-buffer flush export.
pub unsafe extern "win64" fn ke_flush_write_buffer() {
    core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
    unsafe { core::arch::asm!("mfence", options(nostack, preserves_flags)); }
}

/// Resolve only ABI-reviewed HAL exports. Unsupported names fail closed.
pub fn resolve(name: &[u8]) -> Option<u64> {
    if name.eq_ignore_ascii_case(b"HalQueryMaximumProcessorCount") {
        Some(hal_query_maximum_processor_count as usize as u64)
    } else if name.eq_ignore_ascii_case(b"KeFlushWriteBuffer")
        || name.eq_ignore_ascii_case(b"HalFlushWriteBuffer") {
        Some(ke_flush_write_buffer as usize as u64)
    } else { None }
}

pub fn resolve_ordinal(ordinal: u16) -> Option<u64> {
    match ordinal {
        47 => Some(hal_query_maximum_processor_count as usize as u64),
        84 => Some(ke_flush_write_buffer as usize as u64),
        _ => None,
    }
}

pub fn find_by_name(name: &[u8]) -> Option<&'static HalExport> {
    EXPORTS.iter().find(|entry| entry.name.as_bytes().eq_ignore_ascii_case(name))
}
pub fn find_by_ordinal(ordinal: u16) -> Option<&'static HalExport> {
    EXPORTS.iter().find(|entry| entry.ordinal == ordinal)
}
pub fn count() -> usize { EXPORTS.len() }

pub fn self_test() -> bool {
    EXPORTS.len() == 91
        && EXPORTS.iter().enumerate().all(|(i, entry)| entry.ordinal as usize == i + 1)
        && find_by_name(b"HalAcpiGetTableEx").map(|e| e.ordinal) == Some(1)
        && find_by_name(b"HalAllProcessorsStarted").map(|e| e.ordinal) == Some(4)
        && find_by_name(b"halrequestipi").map(|e| e.ordinal) == Some(55)
        && find_by_ordinal(91).map(|entry| entry.name) == Some("x86BiosWriteMemory")
        && find_by_name(b"not-a-hal-export").is_none()
        && resolve(b"HalQueryMaximumProcessorCount").is_some()
        && resolve(b"KeFlushWriteBuffer").is_some()
        && resolve(b"HalAcpiGetTableEx").is_none()
        && resolve_ordinal(47).is_some()
        && resolve_ordinal(1).is_none()
}
