//! NTOSKRNL export catalogue for Windows 11 x64 build 10.0.26100.8036-AMD64.
//! Entries describe export names/ordinals only; they are not all implemented APIs.
pub const EXPORTS: &[(&str, u16)] = &[
    ("CmCallbackGetKeyObjectID", 132),
    ("CmCallbackGetKeyObjectIDEx", 133),
    ("CmCallbackReleaseKeyObjectIDEx", 134),
    ("CmGetBoundTransaction", 135),
    ("CmGetCallbackVersion", 136),
    ("CmKeyObjectType", 137),
    ("CmRegisterCallback", 138),
    ("CmRegisterCallbackEx", 139),
    ("CmRegisterMachineHiveLoadedNotification", 140),
    ("CmSetCallbackObjectContext", 141),
    ("CmUnRegisterCallback", 142),
    ("CmUnregisterMachineHiveLoadedNotification", 143),
];

#[path = "cm/runtime.rs"] pub(crate) mod runtime;

#[path = "cm/hive.rs"] pub(crate) mod hive;
#[path = "cm/driver_config.rs"] pub(crate) mod driver_config;
