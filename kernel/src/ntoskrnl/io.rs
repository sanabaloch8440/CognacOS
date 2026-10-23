//! I/O manager, device, IRP and driver placeholder exports.
pub const NAMES: &[&str] = &[
"IoCreateDevice","IoCreateDeviceSecure","IoDeleteDevice","IoAttachDevice","IoAttachDeviceToDeviceStack",
"IoDetachDevice","IoGetDeviceObjectPointer","IoGetAttachedDeviceReference","IoGetRelatedDeviceObject",
"IoCallDriver","IofCallDriver","IoCompleteRequest","IofCompleteRequest","IoCancelIrp","IoAllocateIrp",
"IoFreeIrp","IoBuildDeviceIoControlRequest","IoBuildSynchronousFsdRequest","IoBuildAsynchronousFsdRequest",
"IoGetCurrentIrpStackLocation","IoGetNextIrpStackLocation","IoSetCompletionRoutine","IoSetCancelRoutine",
"IoRegisterDriverReinitialization","IoRegisterBootDriverReinitialization","IoRegisterShutdownNotification",
"IoUnregisterShutdownNotification","IoCreateSymbolicLink","IoDeleteSymbolicLink","IoCreateFile",
"IoCreateFileEx","IoCreateFileSpecifyDeviceObjectHint","IoGetDeviceProperty","IoGetDevicePropertyData",
"IoGetDeviceInterfaces","IoGetConfigurationInformation","IoGetDriverObjectExtension","IoAllocateDriverObjectExtension",
"IoRegisterPlugPlayNotification","IoUnregisterPlugPlayNotification","IoWMIRegistrationControl",
"IoWMIQueryAllData","IoWMIQuerySingleInstance","IoWMIExecuteMethod","IoGetTopLevelIrp","IoSetTopLevelIrp",
"IoGetRequestorProcess","IoGetRequestorProcessId","IoGetRequestorSessionId","IoQueueWorkItem",
"IoFreeWorkItem","IoInitializeWorkItem","IoUninitializeWorkItem","IoCreateNotificationEvent",
"IoCreateSynchronizationEvent","IoSetShareAccess","IoCheckShareAccess","IoRemoveShareAccess",
"IoValidateDeviceIoControlAccess","IoGetCurrentProcess","IoGetCurrentIrpStackLocation",
];
