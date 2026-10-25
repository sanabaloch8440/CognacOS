/*
 * Canonical Windows x64 NTDLL system-call stubs.
 *
 * The kernel discovers the service numbers from these exported stubs in the
 * staged PE image, then routes the standard SYSCALL ABI to the implemented
 * native NT services. Optional build-time extraction can pin these numbers
 * to a particular Windows NTDLL; the default values match the reference table.
 */
#ifdef _WIN32
#ifdef COGNAC_USE_WINDOWS_SYSCALL_IDS
#include "generated/ntdll_syscalls.h"
#define COGNAC_NT_CLOSE_ID COGNAC_SYSCALL_NTCLOSE
#define COGNAC_NT_YIELD_EXECUTION_ID COGNAC_SYSCALL_NTYIELDEXECUTION
#define COGNAC_NT_QUERY_INFORMATION_PROCESS_ID COGNAC_SYSCALL_NTQUERYINFORMATIONPROCESS
#define COGNAC_NT_QUERY_INFORMATION_THREAD_ID COGNAC_SYSCALL_NTQUERYINFORMATIONTHREAD
#define COGNAC_NT_QUERY_SYSTEM_INFORMATION_ID COGNAC_SYSCALL_NTQUERYSYSTEMINFORMATION
#define COGNAC_NT_OPEN_KEY_ID COGNAC_SYSCALL_NTOPENKEY
#define COGNAC_NT_QUERY_VALUE_KEY_ID COGNAC_SYSCALL_NTQUERYVALUEKEY
#else
#define COGNAC_NT_CLOSE_ID 0x0f
#define COGNAC_NT_YIELD_EXECUTION_ID 0x46
#define COGNAC_NT_QUERY_INFORMATION_PROCESS_ID 0x19
#define COGNAC_NT_QUERY_INFORMATION_THREAD_ID 0x25
#define COGNAC_NT_QUERY_SYSTEM_INFORMATION_ID 0x36
#define COGNAC_NT_OPEN_KEY_ID 0x12
#define COGNAC_NT_QUERY_VALUE_KEY_ID 0x17
#endif

    .text
    .p2align 4
    .globl NtClose
    .def NtClose; .scl 2; .type 32; .endef
NtClose:
    .byte 0x4c, 0x8b, 0xd1
    movl $COGNAC_NT_CLOSE_ID, %eax
    syscall
    ret

    .p2align 4
    .globl NtYieldExecution
    .def NtYieldExecution; .scl 2; .type 32; .endef
NtYieldExecution:
    .byte 0x4c, 0x8b, 0xd1
    movl $COGNAC_NT_YIELD_EXECUTION_ID, %eax
    syscall
    ret

    .p2align 4
    .globl NtQueryInformationProcess
    .def NtQueryInformationProcess; .scl 2; .type 32; .endef
NtQueryInformationProcess:
    .byte 0x4c, 0x8b, 0xd1
    movl $COGNAC_NT_QUERY_INFORMATION_PROCESS_ID, %eax
    syscall
    ret

    .p2align 4
    .globl NtQueryInformationThread
    .def NtQueryInformationThread; .scl 2; .type 32; .endef
NtQueryInformationThread:
    .byte 0x4c, 0x8b, 0xd1
    movl $COGNAC_NT_QUERY_INFORMATION_THREAD_ID, %eax
    syscall
    ret

    .p2align 4
    .globl NtQuerySystemInformation
    .def NtQuerySystemInformation; .scl 2; .type 32; .endef
NtQuerySystemInformation:
    .byte 0x4c, 0x8b, 0xd1
    movl $COGNAC_NT_QUERY_SYSTEM_INFORMATION_ID, %eax
    syscall
    ret

    .p2align 4
    .globl NtOpenKey
    .def NtOpenKey; .scl 2; .type 32; .endef
NtOpenKey:
    .byte 0x4c, 0x8b, 0xd1
    movl $COGNAC_NT_OPEN_KEY_ID, %eax
    syscall
    ret

    .p2align 4
    .globl NtQueryValueKey
    .def NtQueryValueKey; .scl 2; .type 32; .endef
NtQueryValueKey:
    .byte 0x4c, 0x8b, 0xd1
    movl $COGNAC_NT_QUERY_VALUE_KEY_ID, %eax
    syscall
    ret
#endif
