//! CM-family NTOSKRNL import stubs, generated from manifest_19041.json.
//! Names/ordinals are source metadata; every address still points at a generic placeholder.
pub const EXPORTS: &[(&str, u16)] = &[
    ("CmCallbackGetKeyObjectID", 101),
    ("CmCallbackGetKeyObjectIDEx", 102),
    ("CmCallbackReleaseKeyObjectIDEx", 103),
    ("CmGetBoundTransaction", 104),
    ("CmGetCallbackVersion", 105),
    ("CmKeyObjectType", 106),
    ("CmRegisterCallback", 107),
    ("CmRegisterCallbackEx", 108),
    ("CmRegisterMachineHiveLoadedNotification", 109),
    ("CmSetCallbackObjectContext", 110),
    ("CmUnRegisterCallback", 111),
    ("CmUnregisterMachineHiveLoadedNotification", 112),
];
