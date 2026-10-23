//! Windows 11 x64 HAL export compatibility catalogue.
//!
//! The Windows 11 x64 HAL export directory exposes these names (the modern
//! HAL forwards many of them to ntoskrnl). This catalogue is a compatibility
//! inventory, not proof that RdotOS implements Windows HAL semantics.
//!
//! Exports remain deliberately non-callable until their exact ABI and semantics
//! are implemented. Returning an arbitrary generic function pointer here would
//! make driver imports appear resolved while risking stack/argument corruption.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StubState {
    /// Export name/ordinal is known; no ABI-safe implementation yet.
    NotImplemented,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HalExport {
    pub ordinal: u16,
    pub name: &'static str,
    pub state: StubState,
}

/// Windows 11 x64 hal.dll export table, based on WinExports' 10.0.26100 data.
/// Ordinals are the table's Windows 11 x64 ordinals; no data exports are
/// currently identified in this inventory.
pub const EXPORTS: &[HalExport] = &[
    HalExport { ordinal: 1, name: "HalAcpiGetTableEx", state: StubState::NotImplemented },
    HalExport { ordinal: 2, name: "HalAcquireDisplayOwnership", state: StubState::NotImplemented },
    HalExport { ordinal: 3, name: "HalAdjustResourceList", state: StubState::NotImplemented },
    HalExport { ordinal: 4, name: "HalAllocateAdapterChannel", state: StubState::NotImplemented },
    HalExport { ordinal: 5, name: "HalAllocateCommonBuffer", state: StubState::NotImplemented },
    HalExport { ordinal: 6, name: "HalAllocateCrashDumpRegisters", state: StubState::NotImplemented },
    HalExport { ordinal: 7, name: "HalAllocateHardwareCounters", state: StubState::NotImplemented },
    HalExport { ordinal: 8, name: "HalAllProcessorsStarted", state: StubState::NotImplemented },
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
    HalExport { ordinal: 37, name: "HalInitializeBios", state: StubState::NotImplemented },
    HalExport { ordinal: 38, name: "HalInitializeOnResume", state: StubState::NotImplemented },
    HalExport { ordinal: 39, name: "HalInitializeProcessor", state: StubState::NotImplemented },
    HalExport { ordinal: 40, name: "HalInitSystem", state: StubState::NotImplemented },
    HalExport { ordinal: 41, name: "HalIsHyperThreadingEnabled", state: StubState::NotImplemented },
    HalExport { ordinal: 42, name: "HalMakeBeep", state: StubState::NotImplemented },
    HalExport { ordinal: 43, name: "HalPerformEndOfInterrupt", state: StubState::NotImplemented },
    HalExport { ordinal: 44, name: "HalProcessorIdle", state: StubState::NotImplemented },
    HalExport { ordinal: 45, name: "HalQueryDisplayParameters", state: StubState::NotImplemented },
    HalExport { ordinal: 46, name: "HalQueryEnvironmentVariableInfoEx", state: StubState::NotImplemented },
    HalExport { ordinal: 47, name: "HalQueryMaximumProcessorCount", state: StubState::NotImplemented },
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
    HalExport { ordinal: 84, name: "KeFlushWriteBuffer", state: StubState::NotImplemented },
    HalExport { ordinal: 85, name: "KeQueryPerformanceCounter", state: StubState::NotImplemented },
    HalExport { ordinal: 86, name: "KeStallExecutionProcessor", state: StubState::NotImplemented },
    HalExport { ordinal: 87, name: "x86BiosAllocateBuffer", state: StubState::NotImplemented },
    HalExport { ordinal: 88, name: "x86BiosCall", state: StubState::NotImplemented },
    HalExport { ordinal: 89, name: "x86BiosFreeBuffer", state: StubState::NotImplemented },
    HalExport { ordinal: 90, name: "x86BiosReadMemory", state: StubState::NotImplemented },
    HalExport { ordinal: 91, name: "x86BiosWriteMemory", state: StubState::NotImplemented },
];

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
        && find_by_name(b"HalAcpiGetTableEx").is_some()
        && find_by_name(b"halrequestipi").is_some()
        && find_by_ordinal(91).map(|entry| entry.name) == Some("x86BiosWriteMemory")
        && find_by_name(b"not-a-hal-export").is_none()
}
