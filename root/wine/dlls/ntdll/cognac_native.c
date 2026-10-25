/*
 * Native NTDLL bootstrap for CognacOS, x86-64.
 * No Wine headers, Wine server calls, POSIX APIs, or kernel32 import.
 * This is an incremental implementation, not a complete Windows NTDLL.
 */
#include <windows.h>

#ifdef COGNAC_USE_WINDOWS_SYSCALL_IDS
#include "generated/ntdll_syscalls.h"
#endif

#define STATUS_NOT_IMPLEMENTED ((LONG)0xC0000002L)
#define STATUS_INVALID_PARAMETER ((LONG)0xC000000DL)
#define STATUS_SUCCESS ((LONG)0x00000000L)

typedef LONG NTSTATUS;
typedef ULONG_PTR NTSYSCALL_NUMBER;
extern NTSTATUS CognacNtSyscall4(NTSYSCALL_NUMBER number,
                                 ULONG_PTR a1, ULONG_PTR a2,
                                 ULONG_PTR a3, ULONG_PTR a4);

enum { SYSCALL_CLOSE = 0, SYSCALL_YIELD_EXECUTION = 1, SYSCALL_COUNT = 2 };
#ifdef COGNAC_USE_WINDOWS_SYSCALL_IDS
/* Generated from the exact Windows ntdll.dll used to build this image. */
static volatile LONG_PTR syscall_numbers[SYSCALL_COUNT] = {
    (LONG_PTR)COGNAC_SYSCALL_NTCLOSE,
    (LONG_PTR)COGNAC_SYSCALL_NTYIELDEXECUTION
};
#else
/* CognacOS supplies its syscall ABI numbers during native initialization. */
static volatile LONG_PTR syscall_numbers[SYSCALL_COUNT];
#endif

/* Compiler intrinsics avoid importing KernelBase/Kernel32 during NTDLL init. */
__declspec(dllexport)
NTSTATUS WINAPI CognacNtdllConfigureSyscalls(NTSYSCALL_NUMBER nt_close,
                                              NTSYSCALL_NUMBER nt_yield_execution)
{
    if (!nt_close || !nt_yield_execution) return STATUS_INVALID_PARAMETER;
    __atomic_store_n(&syscall_numbers[SYSCALL_CLOSE], (LONG_PTR)nt_close, __ATOMIC_RELEASE);
    __atomic_store_n(&syscall_numbers[SYSCALL_YIELD_EXECUTION],
                     (LONG_PTR)nt_yield_execution, __ATOMIC_RELEASE);
    return STATUS_SUCCESS;
}

__declspec(dllexport)
NTSTATUS NTAPI NtClose(HANDLE handle)
{
    NTSYSCALL_NUMBER number = (NTSYSCALL_NUMBER)
        __atomic_load_n(&syscall_numbers[SYSCALL_CLOSE], __ATOMIC_ACQUIRE);
    if (!number) return STATUS_NOT_IMPLEMENTED;
    return CognacNtSyscall4(number, (ULONG_PTR)handle, 0, 0, 0);
}

__declspec(dllexport)
NTSTATUS NTAPI NtYieldExecution(void)
{
    NTSYSCALL_NUMBER number = (NTSYSCALL_NUMBER)
        __atomic_load_n(&syscall_numbers[SYSCALL_YIELD_EXECUTION], __ATOMIC_ACQUIRE);
    if (!number) return STATUS_NOT_IMPLEMENTED;
    return CognacNtSyscall4(number, 0, 0, 0, 0);
}

/* The loader entry point must not initialize Wine or call another DLL. */
BOOL WINAPI DllMain(HINSTANCE instance, DWORD reason, LPVOID reserved)
{
    (void)instance; (void)reason; (void)reserved;
    return TRUE;
}
