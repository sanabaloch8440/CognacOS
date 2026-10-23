//! Kernel dispatcher, synchronization, processor and interrupt placeholders.
pub const NAMES: &[&str] = &[
"KeAcquireSpinLock","KeAcquireSpinLockAtDpcLevel","KeReleaseSpinLock","KeReleaseSpinLockFromDpcLevel",
"KeInitializeSpinLock","KeInitializeEvent","KeSetEvent","KeResetEvent","KeClearEvent","KeWaitForSingleObject",
"KeWaitForMultipleObjects","KeDelayExecutionThread","KeQuerySystemTime","KeQueryInterruptTime",
"KeQueryTickCount","KeQueryPerformanceCounter","KeStallExecutionProcessor","KeBugCheck","KeBugCheckEx",
"KeRaiseIrql","KeLowerIrql","KeGetCurrentIrql","KeGetCurrentProcessorNumber","KeGetCurrentProcessorNumberEx",
"KeGetProcessorNumberFromIndex","KeSetPriorityThread","KeQueryPriorityThread","KeSetBasePriorityThread",
"KeSetAffinityThread","KeSetSystemAffinityThread","KeRevertToUserAffinityThread","KeInitializeDpc",
"KeInsertQueueDpc","KeRemoveQueueDpc","KeFlushQueuedDpcs","KeInitializeApc","KeInsertQueueApc",
"KeRemoveQueueApc","KeEnterCriticalRegion","KeLeaveCriticalRegion","KeEnterGuardedRegion","KeLeaveGuardedRegion",
"KeInitializeMutex","KeReleaseMutex","KeInitializeSemaphore","KeReleaseSemaphore","KeAcquireInStackQueuedSpinLock",
"KeReleaseInStackQueuedSpinLock","KeFlushIoBuffers","KeMemoryBarrier","KeQueryActiveProcessorCount",
"KeQueryActiveProcessorCountEx","KeQueryMaximumProcessorCount","KeQueryMaximumProcessorCountEx",
"KeIpiGenericCall","KeSignalCallDpcDone","KeSignalCallDpcSynchronize","KeSetSystemTime","KeSetTimer",
"KeSetTimerEx","KeCancelTimer","KeInitializeTimer","KeInitializeTimerEx","KeSetEventBoostPriority",
"KeYieldProcessor","KeYieldExecution","KeExpandKernelStackAndCallout",
];
