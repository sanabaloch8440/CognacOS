/*
 * Native NTDLL bootstrap for CognacOS, x86-64.
 * No Wine headers, Wine server calls, POSIX APIs, or kernel32 import.
 * Incremental implementation; not a complete Windows NTDLL replacement.
 */
#include "cognac_ntdll.h"

#ifdef COGNAC_USE_WINDOWS_SYSCALL_IDS
#include "generated/ntdll_syscalls.h"
#endif

#define STATUS_NOT_IMPLEMENTED ((LONG)0xC0000002L)
#define STATUS_INVALID_PARAMETER ((LONG)0xC000000DL)
#define STATUS_SUCCESS ((LONG)0x00000000L)

typedef LONG NTSTATUS;
typedef ULONG_PTR NTSYSCALL_NUMBER;
#define COGNAC_NT_MAX_ARGUMENTS 17u

extern NTSTATUS CognacNtSyscallN(NTSYSCALL_NUMBER number,
                                 ULONG argument_count,
                                 const ULONG_PTR *arguments);

enum {
    SYSCALL_CLOSE = 0,
    SYSCALL_YIELD_EXECUTION,
    SYSCALL_QUERY_INFORMATION_PROCESS,
    SYSCALL_QUERY_INFORMATION_THREAD,
    SYSCALL_QUERY_SYSTEM_INFORMATION,
    SYSCALL_COUNT
};

#ifdef COGNAC_USE_WINDOWS_SYSCALL_IDS
/* Generated from the exact Windows ntdll.dll used to build this image. */
static volatile LONG_PTR syscall_numbers[SYSCALL_COUNT] = {
    (LONG_PTR)COGNAC_SYSCALL_NTCLOSE,
    (LONG_PTR)COGNAC_SYSCALL_NTYIELDEXECUTION,
    (LONG_PTR)COGNAC_SYSCALL_NTQUERYINFORMATIONPROCESS,
    (LONG_PTR)COGNAC_SYSCALL_NTQUERYINFORMATIONTHREAD,
    (LONG_PTR)COGNAC_SYSCALL_NTQUERYSYSTEMINFORMATION
};
static volatile LONG configured_mask = (1L << SYSCALL_COUNT) - 1;
#else
/* Stable CognacOS-native service IDs; the loader may replace the complete table. */
static volatile LONG_PTR syscall_numbers[SYSCALL_COUNT] = {
    0x100, /* NtClose */
    0x101, /* NtYieldExecution */
    0x102, /* NtQueryInformationProcess */
    0x103, /* NtQueryInformationThread */
    0x104  /* NtQuerySystemInformation */
};
static volatile LONG configured_mask = (1L << SYSCALL_COUNT) - 1;
#endif

/*
 * Compatibility initializer retained for existing loaders. It configures the
 * two original services without disturbing any additional table entries.
 */
__declspec(dllexport)
NTSTATUS WINAPI CognacNtdllConfigureSyscalls(NTSYSCALL_NUMBER nt_close,
                                              NTSYSCALL_NUMBER nt_yield_execution)
{
    if (!nt_close || !nt_yield_execution) return STATUS_INVALID_PARAMETER;
    __atomic_store_n(&syscall_numbers[SYSCALL_CLOSE], (LONG_PTR)nt_close, __ATOMIC_RELEASE);
    __atomic_store_n(&syscall_numbers[SYSCALL_YIELD_EXECUTION],
                     (LONG_PTR)nt_yield_execution, __ATOMIC_RELEASE);
    __atomic_fetch_or(&configured_mask, (1L << SYSCALL_CLOSE) |
                                         (1L << SYSCALL_YIELD_EXECUTION),
                      __ATOMIC_RELEASE);
    return STATUS_SUCCESS;
}

/* Configure all five services as one explicit, versioned-by-count ABI table. */
__declspec(dllexport)
NTSTATUS WINAPI CognacNtdllConfigureSyscallTable(const NTSYSCALL_NUMBER *numbers,
                                                  ULONG count)
{
    ULONG i;

    if (!numbers || count != (ULONG)SYSCALL_COUNT) return STATUS_INVALID_PARAMETER;
    __atomic_store_n(&configured_mask, 0, __ATOMIC_RELEASE);
    for (i = 0; i < (ULONG)SYSCALL_COUNT; ++i)
        __atomic_store_n(&syscall_numbers[i], (LONG_PTR)numbers[i], __ATOMIC_RELAXED);
    __atomic_store_n(&configured_mask, (1L << SYSCALL_COUNT) - 1, __ATOMIC_RELEASE);
    return STATUS_SUCCESS;
}

static NTSTATUS CognacInvokeSyscall(ULONG index, ULONG argument_count,
                                    const ULONG_PTR *arguments)
{
    LONG mask;
    NTSYSCALL_NUMBER number;

    if (index >= (ULONG)SYSCALL_COUNT || argument_count > COGNAC_NT_MAX_ARGUMENTS ||
        (argument_count && !arguments))
        return STATUS_INVALID_PARAMETER;

    mask = __atomic_load_n(&configured_mask, __ATOMIC_ACQUIRE);
    if (!(mask & (1L << index))) return STATUS_NOT_IMPLEMENTED;
    number = (NTSYSCALL_NUMBER)__atomic_load_n(&syscall_numbers[index], __ATOMIC_ACQUIRE);
    return CognacNtSyscallN(number, argument_count, arguments);
}

__declspec(dllexport)
NTSTATUS NTAPI NtClose(HANDLE handle)
{
    ULONG_PTR arguments[1] = { (ULONG_PTR)handle };
    return CognacInvokeSyscall(SYSCALL_CLOSE, 1, arguments);
}

__declspec(dllexport)
NTSTATUS NTAPI NtYieldExecution(void)
{
    return CognacInvokeSyscall(SYSCALL_YIELD_EXECUTION, 0, NULL);
}

/* These wrappers exercise and expose the complete five-argument NT ABI. */
__declspec(dllexport)
NTSTATUS NTAPI NtQueryInformationProcess(HANDLE process, ULONG information_class,
                                         PVOID information, ULONG information_length,
                                         PULONG return_length)
{
    ULONG_PTR arguments[5] = {
        (ULONG_PTR)process, (ULONG_PTR)information_class,
        (ULONG_PTR)information, (ULONG_PTR)information_length,
        (ULONG_PTR)return_length
    };
    return CognacInvokeSyscall(SYSCALL_QUERY_INFORMATION_PROCESS, 5, arguments);
}

__declspec(dllexport)
NTSTATUS NTAPI NtQueryInformationThread(HANDLE thread, ULONG information_class,
                                        PVOID information, ULONG information_length,
                                        PULONG return_length)
{
    ULONG_PTR arguments[5] = {
        (ULONG_PTR)thread, (ULONG_PTR)information_class,
        (ULONG_PTR)information, (ULONG_PTR)information_length,
        (ULONG_PTR)return_length
    };
    return CognacInvokeSyscall(SYSCALL_QUERY_INFORMATION_THREAD, 5, arguments);
}

__declspec(dllexport)
NTSTATUS NTAPI NtQuerySystemInformation(ULONG information_class, PVOID information,
                                        ULONG information_length, PULONG return_length)
{
    ULONG_PTR arguments[4] = {
        (ULONG_PTR)information_class, (ULONG_PTR)information,
        (ULONG_PTR)information_length, (ULONG_PTR)return_length
    };
    return CognacInvokeSyscall(SYSCALL_QUERY_SYSTEM_INFORMATION, 4, arguments);
}

/* ABI-compatible prefix/layout of RTL_OSVERSIONINFOW from winternl.h. */
typedef struct _COGNAC_RTL_OSVERSIONINFOW {
    ULONG dwOSVersionInfoSize;
    ULONG dwMajorVersion;
    ULONG dwMinorVersion;
    ULONG dwBuildNumber;
    ULONG dwPlatformId;
    WCHAR szCSDVersion[128];
} COGNAC_RTL_OSVERSIONINFOW;

__declspec(dllexport)
NTSTATUS NTAPI RtlGetVersion(COGNAC_RTL_OSVERSIONINFOW *version)
{
    ULONG i;

    if (!version || version->dwOSVersionInfoSize < sizeof(*version))
        return STATUS_INVALID_PARAMETER;

    version->dwMajorVersion = 10;
    version->dwMinorVersion = 0;
    version->dwBuildNumber = 26100;
    version->dwPlatformId = 2; /* VER_PLATFORM_WIN32_NT */
    for (i = 0; i < 128; ++i) version->szCSDVersion[i] = 0;
    return STATUS_SUCCESS;
}

/* The loader entry point must not initialize Wine or call another DLL. */
BOOL WINAPI DllMain(HINSTANCE instance, DWORD reason, LPVOID reserved)
{
    (void)instance; (void)reason; (void)reserved;
    return TRUE;
}

/*
 * Keep one explicit image-relative relocation in the PE. x64 code is otherwise
 * RIP-relative, so ld may omit .reloc entirely; CognacOS must be able to map
 * this DLL away from 0x180000000 in its isolated user address space.
 */
__declspec(dllexport)
BOOL (WINAPI *CognacNtdllImageAnchor)(HINSTANCE, DWORD, LPVOID) = DllMain;
