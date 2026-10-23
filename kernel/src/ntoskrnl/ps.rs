//! Process and thread manager placeholder exports.
pub const NAMES: &[&str] = &[
"PsCreateSystemThread","PsCreateSystemThreadEx","PsTerminateSystemThread","PsGetCurrentProcess",
"PsGetCurrentThread","PsGetCurrentProcessId","PsGetCurrentThreadId","PsGetProcessId","PsGetThreadId",
"PsGetThreadProcess","PsGetProcessImageFileName","PsGetProcessPeb","PsGetThreadTeb","PsGetProcessWow64Process",
"PsGetProcessSectionBaseAddress","PsGetProcessExitStatus","PsGetProcessInheritedFromUniqueProcessId",
"PsGetProcessSessionId","PsGetCurrentServerSilo","PsGetProcessProtection","PsIsSystemThread",
"PsIsThreadTerminating","PsSuspendThread","PsResumeThread","PsTerminateProcess","PsLookupProcessByProcessId",
"PsLookupThreadByThreadId","PsReferencePrimaryToken","PsDereferencePrimaryToken","PsAssignImpersonationToken",
"PsImpersonateClient","PsRevertToSelf","PsGetVersion","PsGetVersionEx","PsSetCreateProcessNotifyRoutine",
"PsSetCreateProcessNotifyRoutineEx","PsSetCreateThreadNotifyRoutine","PsSetLoadImageNotifyRoutine",
"PsRemoveLoadImageNotifyRoutine","PsGetNextProcess","PsGetNextProcessThread","PsGetNextThread",
"PsAcquireProcessExitSynchronization","PsReleaseProcessExitSynchronization","PsGetProcessJob",
"PsGetThreadWin32Thread","PsGetCurrentThreadStackBase","PsGetCurrentThreadStackLimit",
"PsGetThreadId","PsGetProcessSessionId","PsGetProcessInheritedFromUniqueProcessId",
];
