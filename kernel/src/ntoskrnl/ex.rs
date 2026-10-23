//! Executive and executive resource/timer/pool placeholder exports.
pub const NAMES: &[&str] = &[
"ExAcquireFastMutex","ExAcquireFastMutexUnsafe","ExAcquirePushLockExclusiveEx","ExAcquirePushLockSharedEx",
"ExAllocatePool","ExAllocatePool2","ExAllocatePool3","ExAllocatePoolWithTag","ExAllocatePoolWithQuotaTag",
"ExFreePool","ExFreePoolWithTag","ExGetPreviousMode","ExInitializeRundownProtection","ExReInitializeRundownProtection",
"ExAcquireRundownProtection","ExReleaseRundownProtection","ExWaitForRundownProtectionRelease","ExCreateCallback",
"ExRegisterCallback","ExUnregisterCallback","ExNotifyCallback","ExInitializeLookasideListEx","ExDeleteLookasideListEx",
"ExAllocateTimer","ExSetTimer","ExCancelTimer","ExDeleteTimer","ExInitializeWorkItem","ExQueueWorkItem",
"ExCreatePool","ExQueryPoolBlockSize","ExGetPoolTagInfo","ExSystemTimeToLocalTime","ExLocalTimeToSystemTime",
"ExRaiseStatus","ExRaiseAccessViolation","ExRaiseDatatypeMisalignment","ExInterlockedInsertHeadList",
"ExInterlockedInsertTailList","ExInterlockedRemoveHeadList","ExInterlockedRemoveTailList","ExfAcquirePushLockExclusive",
"ExfReleasePushLockExclusive","ExfAcquirePushLockShared","ExfReleasePushLockShared","ExUuidCreate",
"ExGetFirmwareEnvironmentVariable","ExSetFirmwareEnvironmentVariable","ExIsProcessorFeaturePresent",
"ExRegisterExtension","ExUnregisterExtension","ExGetExtensionTable",
];
