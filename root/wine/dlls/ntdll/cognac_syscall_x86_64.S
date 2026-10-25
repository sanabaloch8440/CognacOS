/*
 * CognacOS native NT syscall bridge — x86-64, Microsoft x64 ABI.
 *
 * Internal prototype:
 *   NTSTATUS CognacNtSyscallN(ULONG_PTR number, ULONG argument_count,
 *                             const ULONG_PTR *arguments);
 *
 * Stack arguments are copied into the syscall-visible user stack at
 * [RSP+0x28] and onward, following the Windows x64 syscall convention.
 * The C entry points bound argument_count to COGNAC_NT_MAX_ARGUMENTS.
 */
#ifdef _WIN32
    .text
    .p2align 4
    .globl CognacNtSyscallN
    .def CognacNtSyscallN; .scl 2; .type 32; .endef
CognacNtSyscallN:
    /* RCX=service number, RDX=count, R8=pointer to ULONG_PTR arguments. */
    movq    %rdx, %r9
    cmpq    $4, %r9
    jbe     .Lframe_min
    subq    $4, %r9
    shlq    $3, %r9
    addq    $40, %r9
    jmp     .Lframe_round
.Lframe_min:
    movq    $40, %r9
.Lframe_round:
    /* Keep entry RSP alignment and reserve home space plus stack arguments. */
    addq    $15, %r9
    andq    $-16, %r9
    subq    %r9, %rsp
    movq    %r12, 32(%rsp)       /* Save nonvolatile R12 in home space. */
    movq    %r9, %r12             /* Frame size survives SYSCALL (nonvolatile). */
    movl    %ecx, 0(%rsp)         /* Service number. */
    movq    %r8, 8(%rsp)          /* Argument-array pointer. */
    movq    %rdx, 16(%rsp)        /* Argument count. */

    /* Copy arguments 5..N to the stack slots expected by the kernel. */
    movq    16(%rsp), %rcx
    cmpq    $4, %rcx
    jbe     .Lstack_args_done
    movq    8(%rsp), %r11
    movq    $4, %rsi
.Lcopy_stack_args:
    cmpq    %rcx, %rsi
    jae     .Lstack_args_done
    movq    (%r11,%rsi,8), %rax
    movq    %rax, 8(%rsp,%rsi,8)  /* index 4 -> RSP+0x28 */
    incq    %rsi
    jmp     .Lcopy_stack_args

.Lstack_args_done:
    /* Register arguments: arg1=R10, arg2=RDX, arg3=R8, arg4=R9. */
    xorq    %r10, %r10
    xorq    %rdx, %rdx
    xorq    %r8, %r8
    xorq    %r9, %r9
    movq    8(%rsp), %r11
    movq    16(%rsp), %rcx
    cmpq    $0, %rcx
    je      .Largs_loaded
    movq    0(%r11), %r10
    cmpq    $1, %rcx
    jbe     .Largs_loaded
    movq    8(%r11), %rdx
    cmpq    $2, %rcx
    jbe     .Largs_loaded
    movq    16(%r11), %r8
    cmpq    $3, %rcx
    jbe     .Largs_loaded
    movq    24(%r11), %r9

.Largs_loaded:
    movl    0(%rsp), %eax
    syscall

    /* Preserve RAX (NTSTATUS), restore the caller's stack and nonvolatile R12. */
    movq    32(%rsp), %rdx
    addq    %r12, %rsp
    movq    %rdx, %r12
    ret
#endif
