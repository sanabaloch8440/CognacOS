/*
 * Minimal native NTDLL bootstrap for x86-64 Windows.
 * No Wine headers, Wine server calls, POSIX APIs, or Unix-library dependency.
 * This is a buildable starting point, not a complete NTDLL replacement.
 */
#include <windows.h>

#define STATUS_NOT_IMPLEMENTED ((LONG)0xC0000002L)
#define STATUS_INVALID_PARAMETER ((LONG)0xC000000DL)
#define STATUS_SUCCESS ((LONG)0x00000000L)

typedef LONG NTSTATUS;
typedef ULONG_PTR NTSYSCALL_NUMBER;
extern NTSTATUS CognacNtSyscall4(NTSYSCALL_NUMBER number,
                                 ULONG_PTR a1, ULONG_PTR a2,
                                 ULONG_PTR a3, ULONG_PTR a4);

enum { SYSCALL_CLOSE = 0, SYSCALL_YIELD_EXECUTION = 1, SYSCALL_COUNT = 2 };
static volatile LONG_PTR syscall_numbers[SYSCALL_COUNT];

__declspec(dllexport)
NTSTATUS WINAPI CognacNtdllConfigureSyscalls(NTSYSCALL_NUMBER nt_close,
                                              NTSYSCALL_NUMBER nt_yield_execution)
{
    if (!nt_close || !nt_yield_execution) return STATUS_INVALID_PARAMETER;
    InterlockedExchangePointer((PVOID volatile *)&syscall_numbers[SYSCALL_CLOSE], (PVOID)nt_close);
    InterlockedExchangePointer((PVOID volatile *)&syscall_numbers[SYSCALL_YIELD_EXECUTION],
                               (PVOID)nt_yield_execution);
    return STATUS_SUCCESS;
}

__declspec(dllexport)
NTSTATUS NTAPI NtClose(HANDLE handle)
{
    NTSYSCALL_NUMBER number = (NTSYSCALL_NUMBER)InterlockedCompareExchangePointer(
        (PVOID volatile *)&syscall_numbers[SYSCALL_CLOSE], NULL, NULL);
    if (!number) return STATUS_NOT_IMPLEMENTED;
    return CognacNtSyscall4(number, (ULONG_PTR)handle, 0, 0, 0);
}

__declspec(dllexport)
NTSTATUS NTAPI NtYieldExecution(void)
{
    NTSYSCALL_NUMBER number = (NTSYSCALL_NUMBER)InterlockedCompareExchangePointer(
        (PVOID volatile *)&syscall_numbers[SYSCALL_YIELD_EXECUTION], NULL, NULL);
    if (!number) return STATUS_NOT_IMPLEMENTED;
    return CognacNtSyscall4(number, 0, 0, 0, 0);
}

BOOL WINAPI DllMain(HINSTANCE instance, DWORD reason, LPVOID reserved)
{
    (void)instance; (void)reason; (void)reserved;
    return TRUE;
}
