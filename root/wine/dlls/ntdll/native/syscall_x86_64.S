/*
 * CognacOS native NT syscall bridge — x86-64, Windows ABI.
 *
 * This file is intentionally separate from Wine's Unix syscall dispatcher.
 * The caller must obtain the syscall number from the exact Windows build's
 * native NTDLL stub; syscall numbers are not stable across Windows releases.
 *
 * Prototype (Microsoft x64 ABI):
 *   NTSTATUS CognacNtSyscall4(ULONG number, ULONG_PTR a1, ULONG_PTR a2,
 *                             ULONG_PTR a3, ULONG_PTR a4);
 *
 * This minimal bridge supports four register arguments only. It does not
 * marshal stack arguments, switch to a kernel stack, or replace the complete
 * NTDLL implementation. Do not use it for syscalls with more than four args.
 */

#ifdef _WIN32
    .text
    .p2align 4
    .globl CognacNtSyscall4
    .def CognacNtSyscall4; .scl 2; .type 32; .endef
CognacNtSyscall4:
    /* Windows x64 entry: RCX=number, RDX=a1, R8=a2, R9=a3, [RSP+40]=a4 */
    movl    %ecx, %eax
    movq    %rdx, %r10
    movq    %r8,  %rdx
    movq    %r9,  %r8
    movq    40(%rsp), %r9
    syscall
    ret
#endif
