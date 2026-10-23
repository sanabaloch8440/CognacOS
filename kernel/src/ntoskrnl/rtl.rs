//! Kernel RTL helpers. These placeholders are not user-mode ntdll implementations.
pub const NAMES: &[&str] = &[
"RtlInitUnicodeString","RtlInitAnsiString","RtlFreeUnicodeString","RtlFreeAnsiString",
"RtlUnicodeStringToAnsiString","RtlAnsiStringToUnicodeString","RtlCompareUnicodeString",
"RtlEqualUnicodeString","RtlCopyUnicodeString","RtlAppendUnicodeStringToString","RtlAppendUnicodeToString",
"RtlInitString","RtlCompareString","RtlEqualString","RtlZeroMemory","RtlFillMemory","RtlCopyMemory",
"RtlMoveMemory","RtlSecureZeroMemory","RtlRandomEx","RtlRandom","RtlNtStatusToDosError",
"RtlGetVersion","RtlGetNtVersionNumbers","RtlQueryRegistryValues","RtlWriteRegistryValue",
"RtlCreateRegistryKey","RtlDeleteRegistryValue","RtlCheckRegistryKey","RtlGetFullPathName_U",
"RtlPcToFileHeader","RtlLookupFunctionEntry","RtlVirtualUnwind","RtlCaptureContext","RtlRaiseException",
"RtlUnwindEx","RtlWalkFrameChain","RtlGetCallersAddress","RtlInitializeSListHead","RtlInterlockedPushEntrySList",
"RtlInterlockedPopEntrySList","RtlLengthSid","RtlValidSid","RtlEqualSid","RtlCopySid","RtlSubAuthoritySid",
"RtlHashUnicodeString","RtlUpcaseUnicodeChar","RtlDowncaseUnicodeChar","RtlUpcaseUnicodeString",
"RtlDowncaseUnicodeString","RtlIsNameInExpression","RtlCompareMemory","RtlFindClearBits","RtlFindSetBits",
"RtlSetBits","RtlClearBits","RtlTestBit","RtlInitializeBitMap","RtlNumberOfSetBits","RtlTimeToTimeFields",
"RtlTimeFieldsToTime","RtlGetDaclSecurityDescriptor","RtlGetOwnerSecurityDescriptor",
];
