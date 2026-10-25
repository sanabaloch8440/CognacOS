//! Windows 11 x64 NTDLL loader export placeholders.
//! Replace generic signatures with ABI-correct declarations as each routine is implemented.
#![allow(non_snake_case, clippy::missing_safety_doc)]

#[no_mangle]
pub unsafe extern "system" fn LdrAccessResource() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrAddDllDirectory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrAddLoadAsDataTable() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrAddRefDll() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrAppxHandleIntegrityFailure() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrCallEnclave() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrControlFlowGuardEnforced() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrCreateEnclave() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrDeleteEnclave() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrDisableThreadCalloutsForDll() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrEnumerateLoadedModules() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrEnumResources() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrFastFailInLoaderCallout() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrFindEntryForAddress() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrFindResource_U() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrFindResourceDirectory_U() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrFindResourceEx_U() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrFlushAlternateResourceModules() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrGetDllDirectory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrGetDllFullName() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrGetDllHandleByMapping() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrGetDllHandleByName() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrGetDllHandleEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrGetDllPath() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrGetFailureData() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrGetFileNameFromLoadAsDataTable() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrGetKnownDllSectionHandle() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrGetProcedureAddressEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrGetProcedureAddressForCaller() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrHotPatchNotify() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrInitializeEnclave() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrInitializeThunk() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrInitShimEngineDynamic() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrIsModuleSxsRedirected() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrLoadAlternateResourceModule() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrLoadAlternateResourceModuleEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrLoadDll() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrLoadEnclaveModule() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrLockLoaderLock() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrOpenImageFileOptionsKey() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrpResGetMappingSize() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrpResGetResourceDirectory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrProcessInitializationComplete() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrProcessRelocationBlock() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrProcessRelocationBlockEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrQueryImageFileExecutionOptions() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrQueryImageFileExecutionOptionsEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrQueryImageFileKeyOption() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrQueryModuleServiceTags() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrQueryOptionalDelayLoadedAPI() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrQueryProcessModuleInformation() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrRegisterDllNotification() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrRemoveDllDirectory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrRemoveLoadAsDataTable() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrResFindResource() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrResFindResourceDirectory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrResGetRCConfig() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrResolveDelayLoadedAPI() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrResolveDelayLoadsFromDll() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrResRelease() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrResSearchResource() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrRscIsTypeExist() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrSetAppCompatDllRedirectionCallback() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrSetDefaultDllDirectories() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrSetDllDirectory() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrSetDllManifestProber() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrSetImplicitPathOptions() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrSetMUICacheType() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrShutdownProcess() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrShutdownThread() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrStandardizeSystemPath() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrSystemDllInitBlock() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrUnloadAlternateResourceModule() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrUnloadAlternateResourceModuleEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrUnloadDll() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrUnlockLoaderLock() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrUnregisterDllNotification() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrUpdatePackageSearchPath() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrVerifyImageMatchesChecksum() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn LdrVerifyImageMatchesChecksumEx() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtLoadDriver() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn NtUnloadDriver() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlComputePrivatizedDllName_U() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlDllShutdownInProgress() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlIsThreadWithinLoaderCallout() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}

#[no_mangle]
pub unsafe extern "system" fn RtlWnfDllUnloadCallback() -> i32 {
    0xC0000002u32 as i32 // STATUS_NOT_IMPLEMENTED
}
