/*
 * Native NTDLL bootstrap for CognacOS, x86-64.
 * No Wine headers, Wine server calls, POSIX APIs, or kernel32 import.
 * Incremental implementation; not a complete Windows NTDLL replacement.
 */
#include "cognac_ntdll.h"

#define STATUS_NOT_IMPLEMENTED ((LONG)0xC0000002L)
#define STATUS_INVALID_PARAMETER ((LONG)0xC000000DL)
#define STATUS_SUCCESS ((LONG)0x00000000L)

typedef LONG NTSTATUS;
extern LONG NTAPI NtYieldExecution(void);

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

/* Native x64 PEB accessor; the process launcher installs the TEB in GS. */
__declspec(dllexport)
PVOID NTAPI RtlGetCurrentPeb(void)
{
    PVOID peb;
    __asm__ volatile ("movq %%gs:0x60, %0" : "=r"(peb));
    return peb;
}


/*
 * Windows x64 TEB/PEB access and RTL_CRITICAL_SECTION primitives.
 * The launcher must install a valid Windows-layout TEB in user GS.
 *
 * This ABI-compatible critical section uses recursive ownership and an
 * atomic LockCount. Contenders yield after bounded spinning. It does not yet
 * implement Windows' event-backed waiter wake-up policy, so heavy contention
 * may consume CPU; the native wait/wake backend remains a separate task.
 */
#define STATUS_NOT_OWNER ((LONG)0xC000005AL)
typedef struct _COGNAC_RTL_CRITICAL_SECTION {
    void *DebugInfo;
    volatile LONG LockCount;
    volatile LONG RecursionCount;
    HANDLE OwningThread;
    HANDLE LockSemaphore;
    ULONG_PTR SpinCount;
} COGNAC_RTL_CRITICAL_SECTION;

__declspec(dllexport)
PVOID NTAPI RtlGetCurrentTeb(void)
{
    PVOID teb;
    __asm__ volatile ("movq %%gs:0x30, %0" : "=r"(teb));
    return teb;
}

static HANDLE cognac_current_thread_id(void)
{
    HANDLE id;
    /* CLIENT_ID.UniqueThread is at TEB offset 0x48 on x64. */
    __asm__ volatile ("movq %%gs:0x48, %0" : "=r"(id));
    return id;
}

__declspec(dllexport)
LONG NTAPI RtlEnterCriticalSection(COGNAC_RTL_CRITICAL_SECTION *cs)
{
    HANDLE tid;
    ULONG spins = 0;
    if (!cs) return STATUS_INVALID_PARAMETER;
    tid = cognac_current_thread_id();
    if (!tid) return STATUS_INVALID_PARAMETER;

    if (__atomic_load_n((void * volatile *)&cs->OwningThread, __ATOMIC_ACQUIRE) == tid) {
        __atomic_add_fetch(&cs->RecursionCount, 1, __ATOMIC_RELAXED);
        return STATUS_SUCCESS;
    }

    for (;;) {
        LONG expected = -1;
        if (__atomic_compare_exchange_n(&cs->LockCount, &expected, 0, 0,
                                        __ATOMIC_ACQUIRE, __ATOMIC_RELAXED)) {
            __atomic_store_n(&cs->RecursionCount, 1, __ATOMIC_RELAXED);
            __atomic_store_n((void * volatile *)&cs->OwningThread, tid, __ATOMIC_RELEASE);
            return STATUS_SUCCESS;
        }
        __asm__ volatile ("pause");
        if (++spins >= 4096) {
            spins = 0;
            (void)NtYieldExecution();
        }
    }
}

__declspec(dllexport)
LONG NTAPI RtlLeaveCriticalSection(COGNAC_RTL_CRITICAL_SECTION *cs)
{
    HANDLE tid;
    LONG recursion;
    if (!cs) return STATUS_INVALID_PARAMETER;
    tid = cognac_current_thread_id();
    if (!tid) return STATUS_INVALID_PARAMETER;
    if (__atomic_load_n((void * volatile *)&cs->OwningThread, __ATOMIC_ACQUIRE) != tid)
        return STATUS_NOT_OWNER;

    recursion = __atomic_sub_fetch(&cs->RecursionCount, 1, __ATOMIC_RELAXED);
    if (recursion > 0) return STATUS_SUCCESS;
    if (recursion < 0) {
        __atomic_store_n(&cs->RecursionCount, 0, __ATOMIC_RELAXED);
        return STATUS_INVALID_PARAMETER;
    }

    __atomic_store_n((void * volatile *)&cs->OwningThread, (void *)0, __ATOMIC_RELEASE);
    __atomic_store_n(&cs->LockCount, -1, __ATOMIC_RELEASE);
    return STATUS_SUCCESS;
}

__declspec(dllexport)
void NTAPI RtlAcquirePebLock(void)
{
    unsigned char *peb = (unsigned char *)RtlGetCurrentPeb();
    COGNAC_RTL_CRITICAL_SECTION *lock;
    if (!peb) return;
    /* PEB.FastPebLock is a pointer at x64 offset 0x38. */
    lock = *(COGNAC_RTL_CRITICAL_SECTION **)(peb + 0x38);
    if (lock) (void)RtlEnterCriticalSection(lock);
}

__declspec(dllexport)
void NTAPI RtlReleasePebLock(void)
{
    unsigned char *peb = (unsigned char *)RtlGetCurrentPeb();
    COGNAC_RTL_CRITICAL_SECTION *lock;
    if (!peb) return;
    lock = *(COGNAC_RTL_CRITICAL_SECTION **)(peb + 0x38);
    if (lock) (void)RtlLeaveCriticalSection(lock);
}

/* Native version helper used by applications that query the NT build tuple. */
__declspec(dllexport)
void NTAPI RtlGetNtVersionNumbers(PULONG major, PULONG minor, PULONG build)
{
    if (major) *major = 10;
    if (minor) *minor = 0;
    if (build) *build = 26100;
}

/* Small deterministic mapping for statuses implemented by this bootstrap. */
__declspec(dllexport)
ULONG NTAPI RtlNtStatusToDosError(NTSTATUS status)
{
    switch ((ULONG)status) {
    case 0x00000000u: return 0;   /* ERROR_SUCCESS */
    case 0xC0000002u: return 120; /* ERROR_CALL_NOT_IMPLEMENTED */
    case 0xC0000005u: return 998; /* ERROR_NOACCESS */
    case 0xC0000008u: return 6;   /* ERROR_INVALID_HANDLE */
    case 0xC000000Du: return 87;  /* ERROR_INVALID_PARAMETER */
    case 0xC0000017u: return 8;   /* ERROR_NOT_ENOUGH_MEMORY */
    case 0xC0000022u: return 5;   /* ERROR_ACCESS_DENIED */
    case 0xC0000023u: return 122; /* ERROR_INSUFFICIENT_BUFFER */
    case 0xC0000034u: return 2;   /* ERROR_FILE_NOT_FOUND */
    default: return 317;          /* ERROR_MR_MID_NOT_FOUND */
    }
}

__declspec(dllexport)
ULONG NTAPI RtlNtStatusToDosErrorNoTeb(NTSTATUS status)
{
    return RtlNtStatusToDosError(status);
}

/* TEB LastErrorValue and LastStatusValue on Windows x64. The launcher maps
 * the TEB at the user GS base before entering native code. */
__declspec(dllexport)
ULONG NTAPI RtlGetLastWin32Error(void)
{
    ULONG value;
    __asm__ volatile ("movl %%gs:0x68, %0" : "=r"(value));
    return value;
}

__declspec(dllexport)
void NTAPI RtlSetLastWin32Error(ULONG error)
{
    __asm__ volatile ("movl %0, %%gs:0x68" :: "r"(error) : "memory");
}

__declspec(dllexport)
NTSTATUS NTAPI RtlGetLastNtStatus(void)
{
    NTSTATUS status;
    __asm__ volatile ("movl %%gs:0x1250, %0" : "=r"(status));
    return status;
}

__declspec(dllexport)
void NTAPI RtlSetLastWin32ErrorAndNtStatusFromNtStatus(NTSTATUS status)
{
    ULONG error = RtlNtStatusToDosError(status);
    __asm__ volatile ("movl %0, %%gs:0x68\n\tmovl %1, %%gs:0x1250"
                      :: "r"(error), "r"(status) : "memory");
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
