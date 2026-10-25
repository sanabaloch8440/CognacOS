//! x86-64 GDT/IDT setup and exception-specific fatal diagnostics.
use core::arch::{asm, global_asm};
use core::mem::size_of;

const KERNEL_CODE_SELECTOR: u16 = 0x08;
const IDT_ENTRIES: usize = 256;
const EXCEPTION_VECTORS: usize = 32;

/// Read the current IF bit without changing interrupt state.
pub fn interrupts_enabled()->bool{
    let flags:u64;
    unsafe{asm!("pushfq","pop {}",out(reg) flags,options(preserves_flags));}
    flags&(1<<9)!=0
}
/// Restore the caller's saved interrupt-enable state after a context switch.
pub fn set_interrupts_enabled(enabled:bool){
    if enabled{unsafe{asm!("sti",options(nomem,nostack));}}
    else{unsafe{asm!("cli",options(nomem,nostack));}}
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct IdtEntry {
    offset_low: u16, selector: u16, ist: u8, attributes: u8,
    offset_middle: u16, offset_high: u32, reserved: u32,
}
impl IdtEntry {
    const MISSING: Self = Self { offset_low: 0, selector: 0, ist: 0, attributes: 0,
        offset_middle: 0, offset_high: 0, reserved: 0 };
    fn interrupt_gate(handler: u64) -> Self {
        Self { offset_low: handler as u16, selector: KERNEL_CODE_SELECTOR, ist: 0,
            attributes: 0x8E, offset_middle: (handler >> 16) as u16,
            offset_high: (handler >> 32) as u32, reserved: 0 }
    }
    fn user_interrupt_gate(handler: u64) -> Self {
        Self { offset_low: handler as u16, selector: KERNEL_CODE_SELECTOR, ist: 0,
            attributes: 0xEE, offset_middle: (handler >> 16) as u16,
            offset_high: (handler >> 32) as u32, reserved: 0 }
    }
    fn interrupt_gate_ist(handler: u64, ist: u8) -> Self {
        Self { offset_low: handler as u16, selector: KERNEL_CODE_SELECTOR, ist: ist & 7,
            attributes: 0x8E, offset_middle: (handler >> 16) as u16,
            offset_high: (handler >> 32) as u32, reserved: 0 }
    }
}
#[repr(C, packed)]
struct DescriptorTablePointer { limit: u16, base: u64 }

/// 64-bit architectural TSS. Each CPU owns a distinct descriptor and IST stacks.
#[repr(C, packed)]
struct TaskStateSegment {
    reserved0: u32,
    rsp0: u64,
    rsp1: u64,
    rsp2: u64,
    reserved1: u64,
    ist: [u64; 7],
    reserved2: u64,
    reserved3: u16,
    iomap_base: u16,
}
impl TaskStateSegment {
    const fn new() -> Self {
        Self { reserved0: 0, rsp0: 0, rsp1: 0, rsp2: 0, reserved1: 0,
            ist: [0; 7], reserved2: 0, reserved3: 0,
            iomap_base: core::mem::size_of::<Self>() as u16 }
    }
}
const IST_STACK_BYTES: usize = 16 * 1024;
static mut CPU_GDT: [[u64; 7]; crate::hal::MAX_CPUS] = [[0; 7]; crate::hal::MAX_CPUS];
static mut CPU_TSS: [TaskStateSegment; crate::hal::MAX_CPUS] =
    [const { TaskStateSegment::new() }; crate::hal::MAX_CPUS];
static mut CPU_IST_STACKS: [[[u8; IST_STACK_BYTES]; 3]; crate::hal::MAX_CPUS] =
    [[[0; IST_STACK_BYTES]; 3]; crate::hal::MAX_CPUS];
static mut IDT: [IdtEntry; IDT_ENTRIES] = [IdtEntry::MISSING; IDT_ENTRIES];

// Lock-free trap-frame fixup registry. Entries are published by storing RIP
// last with Release ordering; the exception path never takes a potentially
// already-held lock.
const MAX_TRAP_FIXUPS: usize = 64;
const FIXUP_RESERVED: u64 = u64::MAX;
const PF_ERROR_MATCH_MASK: u64 = 0x1f;
const STATUS_ACCESS_VIOLATION: u64 = 0xC000_0005;

/// Fixed-size exception-table entry. The handler only scans this bounded table
/// and never allocates or acquires a lock. RAX is the documented recovery result.
struct TrapFixup {
    vector: core::sync::atomic::AtomicU64,
    fault_rip: core::sync::atomic::AtomicU64,
    recovery_rip: core::sync::atomic::AtomicU64,
    address: core::sync::atomic::AtomicU64,
    error_mask: core::sync::atomic::AtomicU64,
    error_value: core::sync::atomic::AtomicU64,
    recovery_result: core::sync::atomic::AtomicU64,
    readers: core::sync::atomic::AtomicU32,
}
impl TrapFixup {
    const fn new() -> Self {
        Self {
            vector: core::sync::atomic::AtomicU64::new(0),
            fault_rip: core::sync::atomic::AtomicU64::new(0),
            recovery_rip: core::sync::atomic::AtomicU64::new(0),
            address: core::sync::atomic::AtomicU64::new(0),
            error_mask: core::sync::atomic::AtomicU64::new(0),
            error_value: core::sync::atomic::AtomicU64::new(0),
            recovery_result: core::sync::atomic::AtomicU64::new(0),
            readers: core::sync::atomic::AtomicU32::new(0),
        }
    }
}
static TRAP_FIXUPS: [TrapFixup; MAX_TRAP_FIXUPS] =
    [const { TrapFixup::new() }; MAX_TRAP_FIXUPS];

/// Register an exact-RIP supervisor exception recovery. For vector 14, address=0
/// matches any CR2 and error_mask/value constrain the page-fault error code; for
/// other vectors use address=0 and match the architectural error code. On recovery, RAX is
/// set to recovery_result and RIP is redirected to recovery_rip. The recovery
/// stub must resume/return from the original probe without re-executing it.
pub fn register_trap_frame_exception_fixup(
    vector: u64, fault_rip: u64, recovery_rip: u64, address: u64,
    error_mask: u64, error_value: u64, recovery_result: u64,
) -> bool {
    const KERNEL_BASE: u64 = 0xffff_8000_0000_0000;
    // NMI, double fault, and machine check cannot be safely recovered as probes.
    if vector >= EXCEPTION_VECTORS as u64 || matches!(vector, 2 | 8 | 18)
        || fault_rip < KERNEL_BASE || fault_rip == FIXUP_RESERVED
        || recovery_rip < KERNEL_BASE || recovery_rip == FIXUP_RESERVED
        || error_value & !error_mask != 0 {
        return false;
    }
    for slot in TRAP_FIXUPS.iter() {
        if slot.fault_rip.compare_exchange(
            0, FIXUP_RESERVED,
            core::sync::atomic::Ordering::AcqRel,
            core::sync::atomic::Ordering::Acquire,
        ).is_ok() {
            slot.vector.store(vector, core::sync::atomic::Ordering::Relaxed);
            slot.recovery_rip.store(recovery_rip, core::sync::atomic::Ordering::Relaxed);
            slot.address.store(address, core::sync::atomic::Ordering::Relaxed);
            slot.error_mask.store(error_mask, core::sync::atomic::Ordering::Relaxed);
            slot.error_value.store(error_value, core::sync::atomic::Ordering::Relaxed);
            slot.recovery_result.store(recovery_result, core::sync::atomic::Ordering::Relaxed);
            slot.fault_rip.store(fault_rip, core::sync::atomic::Ordering::Release);
            return true;
        }
    }
    false
}

pub fn register_trap_frame_fixup_with_result(
    fault_rip: u64, recovery_rip: u64, address: u64,
    error_mask: u64, error_value: u64, recovery_result: u64,
) -> bool {
    register_trap_frame_exception_fixup(
        14, fault_rip, recovery_rip, address, error_mask, error_value, recovery_result,
    )
}

/// Compatibility helper for callers whose recovery convention returns zero in RAX.
pub fn register_trap_frame_fixup(
    fault_rip: u64, recovery_rip: u64, address: u64,
    error_mask: u64, error_value: u64,
) -> bool {
    register_trap_frame_fixup_with_result(
        fault_rip, recovery_rip, address, error_mask, error_value, 0,
    )
}

/// Remove one published fixup. Returns false when no entry matches.
pub fn unregister_trap_frame_fixup(fault_rip: u64) -> bool {
    for slot in TRAP_FIXUPS.iter() {
        if slot.fault_rip.compare_exchange(
            fault_rip, FIXUP_RESERVED,
            core::sync::atomic::Ordering::AcqRel,
            core::sync::atomic::Ordering::Acquire,
        ).is_ok() {
            // Hide the record first, then let any lookup already pinned to it finish.
            while slot.readers.load(core::sync::atomic::Ordering::Acquire) != 0 {
                core::hint::spin_loop();
            }
            slot.vector.store(0, core::sync::atomic::Ordering::Relaxed);
            slot.recovery_rip.store(0, core::sync::atomic::Ordering::Relaxed);
            slot.address.store(0, core::sync::atomic::Ordering::Relaxed);
            slot.error_mask.store(0, core::sync::atomic::Ordering::Relaxed);
            slot.error_value.store(0, core::sync::atomic::Ordering::Relaxed);
            slot.recovery_result.store(0, core::sync::atomic::Ordering::Relaxed);
            slot.fault_rip.store(0, core::sync::atomic::Ordering::Release);
            return true;
        }
    }
    false
}

fn apply_trap_frame_fixup(vector: u64, rip: u64, cr2: u64, error: u64, frame: *const u64) -> bool {
    // MAX_TRAP_FIXUPS is deliberately small and fixed: exception dispatch cannot
    // allocate, sleep, or acquire locks which might already be held by the faulting CPU.
    for slot in TRAP_FIXUPS.iter() {
        if slot.fault_rip.load(core::sync::atomic::Ordering::Acquire) != rip { continue; }
        slot.readers.fetch_add(1, core::sync::atomic::Ordering::AcqRel);
        if slot.fault_rip.load(core::sync::atomic::Ordering::Acquire) != rip {
            slot.readers.fetch_sub(1, core::sync::atomic::Ordering::Release);
            continue;
        }
        if slot.vector.load(core::sync::atomic::Ordering::Relaxed) != vector {
            slot.readers.fetch_sub(1, core::sync::atomic::Ordering::Release);
            continue;
        }
        let address = slot.address.load(core::sync::atomic::Ordering::Relaxed);
        let mask = slot.error_mask.load(core::sync::atomic::Ordering::Relaxed);
        let value = slot.error_value.load(core::sync::atomic::Ordering::Relaxed);
        let recovery = slot.recovery_rip.load(core::sync::atomic::Ordering::Relaxed);
        let result = slot.recovery_result.load(core::sync::atomic::Ordering::Relaxed);
        let matches = (address == 0 || address == cr2) && error & mask == value
            && recovery >= 0xffff_8000_0000_0000 && recovery != FIXUP_RESERVED;
        slot.readers.fetch_sub(1, core::sync::atomic::Ordering::Release);
        if matches {
            unsafe {
                let saved = frame as *mut u64;
                saved.add(14).write(result); // RAX: explicit probe result
                saved.add(17).write(recovery); // RIP: resume at the recovery stub
            }
            return true;
        }
    }
    false
}

global_asm!(r#"
    .section .text
    .global cognacos_fault_probe_write_ro
    .type cognacos_fault_probe_write_ro, @function
cognacos_fault_probe_write_ro:
    mov byte ptr [rdi], 0x90
    mov eax, 1
    ret
    .global cognacos_fault_probe_write_ro_recovery
    .type cognacos_fault_probe_write_ro_recovery, @function
cognacos_fault_probe_write_ro_recovery:
    # The dispatcher already publishes the configured recovery result in RAX.
    ret

    .global cognacos_fault_probe_execute_nx
    .type cognacos_fault_probe_execute_nx, @function
cognacos_fault_probe_execute_nx:
    jmp rdi
    .global cognacos_fault_probe_execute_nx_recovery
    .type cognacos_fault_probe_execute_nx_recovery, @function
cognacos_fault_probe_execute_nx_recovery:
    xor eax, eax
    ret

    # Faulting instruction address is the function entry; the dispatcher writes
    # STATUS_ACCESS_VIOLATION to saved RAX and redirects RIP to the ret-only stub.
    .global cognacos_fault_probe_read_invalid
    .type cognacos_fault_probe_read_invalid, @function
cognacos_fault_probe_read_invalid:
    mov rax, qword ptr [rdi]
    ret
    .global cognacos_fault_probe_read_invalid_recovery
    .type cognacos_fault_probe_read_invalid_recovery, @function
cognacos_fault_probe_read_invalid_recovery:
    ret
"#);


global_asm!(r#"
    .section .rodata
    .align 8
.Lcognacos_gdt:
    .quad 0
    .quad 0x00af9a000000ffff
    .quad 0x00cf92000000ffff
.Lcognacos_gdt_pointer:
    .word 23
    .quad .Lcognacos_gdt

    .section .text
    .global cognacos_load_gdt
    .type cognacos_load_gdt, @function
cognacos_load_gdt:
    lgdt [rip + .Lcognacos_gdt_pointer]
    push 0x08
    lea rax, [rip + .Lcognacos_reload_cs]
    push rax
    retfq
.Lcognacos_reload_cs:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    xor eax, eax
    mov fs, ax
    mov gs, ax
    ret

    .macro ISR_NOERR n
    .global cognacos_isr_\n
    .type cognacos_isr_\n, @function
cognacos_isr_\n:
    push 0
    push \n
    jmp cognacos_exception_common
    .endm

    .macro ISR_ERR n
    .global cognacos_isr_\n
    .type cognacos_isr_\n, @function
cognacos_isr_\n:
    push \n
    jmp cognacos_exception_common
    .endm

    ISR_NOERR 0
    ISR_NOERR 1
    ISR_NOERR 2
    ISR_NOERR 3
    ISR_NOERR 4
    ISR_NOERR 5
    ISR_NOERR 6
    ISR_NOERR 7
    ISR_ERR   8
    ISR_NOERR 9
    ISR_ERR   10
    ISR_ERR   11
    ISR_ERR   12
    ISR_ERR   13
    ISR_ERR   14
    ISR_NOERR 15
    ISR_NOERR 16
    ISR_ERR   17
    ISR_NOERR 18
    ISR_NOERR 19
    ISR_NOERR 20
    ISR_ERR   21
    ISR_NOERR 22
    ISR_NOERR 23
    ISR_NOERR 24
    ISR_NOERR 25
    ISR_NOERR 26
    ISR_NOERR 27
    ISR_NOERR 28
    ISR_ERR   29
    ISR_ERR   30
    ISR_NOERR 31

    .global cognacos_exception_common
    .type cognacos_exception_common, @function
cognacos_exception_common:
    cli
    cld
    push rax
    push rbx
    push rcx
    push rdx
    push rsi
    push rdi
    push rbp
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15
    # Normalized exception frame: saved CS is 144 bytes above the GPR frame.
    mov eax, dword ptr [rsp + 144]
    and eax, 3
    cmp eax, 3
    jne .Lcognacos_exception_skip_user_save
    mov rbx, rsp
    and rsp, -16
    call cognacos_save_current_user_xstate_asm
    mov rsp, rbx
.Lcognacos_exception_skip_user_save:
    mov rdx, rsp
    mov rdi, [rsp + 120]
    mov rsi, [rsp + 128]
    mov r12, rsp
    and rsp, -16
    call cognacos_exception_dispatch
    mov rsp, r12
    test rax, rax
    jz .Lcognacos_unhandled_exception
    mov eax, dword ptr [rsp + 144]
    and eax, 3
    cmp eax, 3
    jne .Lcognacos_exception_skip_user_restore
    mov rbx, rsp
    and rsp, -16
    call cognacos_current_user_xstate_ptr
    mov rsp, rbx
    test rax, rax
    jz .Lcognacos_exception_skip_user_restore
    mov r10, rax
    mov eax, 3
    xor edx, edx
    xrstor64 [r10]
.Lcognacos_exception_skip_user_restore:
    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rbp
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rbx
    pop rax
    add rsp, 16
    iretq
.Lcognacos_unhandled_exception:
    ud2

    # Dedicated interrupt-gate entry: preserve all registers and return with IRETQ.
    .global cognacos_tlb_ipi_stub
    .type cognacos_tlb_ipi_stub, @function
cognacos_tlb_ipi_stub:
    cld
    push rax
    push rbx
    push rcx
    push rdx
    push rsi
    push rdi
    push rbp
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15
    mov eax, dword ptr [rsp + 128]
    and eax, 3
    cmp eax, 3
    jne .Lcognacos_tlb_skip_user_save
    mov rbx, rsp
    and rsp, -16
    call cognacos_save_current_user_xstate_asm
    mov rsp, rbx
.Lcognacos_tlb_skip_user_save:
    mov rax, rsp
    and rsp, -16
    sub rsp, 16
    mov [rsp + 8], rax
    call cognacos_tlb_ipi_handler
    mov rsp, [rsp + 8]
    mov eax, dword ptr [rsp + 128]
    and eax, 3
    cmp eax, 3
    jne .Lcognacos_tlb_skip_user_restore
    mov rbx, rsp
    and rsp, -16
    call cognacos_current_user_xstate_ptr
    mov rsp, rbx
    test rax, rax
    jz .Lcognacos_tlb_skip_user_restore
    mov r10, rax
    mov eax, 3
    xor edx, edx
    xrstor64 [r10]
.Lcognacos_tlb_skip_user_restore:
    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rbp
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rbx
    pop rax
    iretq


    # Common external interrupt entry. Hardware does not push a vector or
    # error code for IRQs, so each stub normalizes the stack before dispatch.
    .macro ISR_IRQ n
    .global cognacos_irq_\n
    .type cognacos_irq_\n, @function
cognacos_irq_\n:
    push 0
    push \n
    jmp cognacos_interrupt_common
    .endm

    ISR_IRQ 32
    ISR_IRQ 33
    ISR_IRQ 34
    ISR_IRQ 35
    ISR_IRQ 36
    ISR_IRQ 37
    ISR_IRQ 38
    ISR_IRQ 39
    ISR_IRQ 40
    ISR_IRQ 41
    ISR_IRQ 42
    ISR_IRQ 43
    ISR_IRQ 44
    ISR_IRQ 45
    ISR_IRQ 46
    ISR_IRQ 47
    ISR_IRQ 48
    ISR_IRQ 49
    ISR_IRQ 50
    ISR_IRQ 51
    ISR_IRQ 52
    ISR_IRQ 53
    ISR_IRQ 54
    ISR_IRQ 55
    ISR_IRQ 56
    ISR_IRQ 57
    ISR_IRQ 58
    ISR_IRQ 59
    ISR_IRQ 60
    ISR_IRQ 61
    ISR_IRQ 62
    ISR_IRQ 63
    ISR_IRQ 64
    ISR_IRQ 65
    ISR_IRQ 66
    ISR_IRQ 67
    ISR_IRQ 68
    ISR_IRQ 69
    ISR_IRQ 70
    ISR_IRQ 71
    ISR_IRQ 72
    ISR_IRQ 73
    ISR_IRQ 74
    ISR_IRQ 75
    ISR_IRQ 76
    ISR_IRQ 77
    ISR_IRQ 78
    ISR_IRQ 79
    ISR_IRQ 80
    ISR_IRQ 81
    ISR_IRQ 82
    ISR_IRQ 83
    ISR_IRQ 84
    ISR_IRQ 85
    ISR_IRQ 86
    ISR_IRQ 87
    ISR_IRQ 88
    ISR_IRQ 89
    ISR_IRQ 90
    ISR_IRQ 91
    ISR_IRQ 92
    ISR_IRQ 93
    ISR_IRQ 94
    ISR_IRQ 95
    ISR_IRQ 96
    ISR_IRQ 97
    ISR_IRQ 98
    ISR_IRQ 99
    ISR_IRQ 100
    ISR_IRQ 101
    ISR_IRQ 102
    ISR_IRQ 103
    ISR_IRQ 104
    ISR_IRQ 105
    ISR_IRQ 106
    ISR_IRQ 107
    ISR_IRQ 108
    ISR_IRQ 109
    ISR_IRQ 110
    ISR_IRQ 111
    ISR_IRQ 112
    ISR_IRQ 113
    ISR_IRQ 114
    ISR_IRQ 115
    ISR_IRQ 116
    ISR_IRQ 117
    ISR_IRQ 118
    ISR_IRQ 119
    ISR_IRQ 120
    ISR_IRQ 121
    ISR_IRQ 122
    ISR_IRQ 123
    ISR_IRQ 124
    ISR_IRQ 125
    ISR_IRQ 126
    ISR_IRQ 127
    ISR_IRQ 128
    ISR_IRQ 129
    ISR_IRQ 130
    ISR_IRQ 131
    ISR_IRQ 132
    ISR_IRQ 133
    ISR_IRQ 134
    ISR_IRQ 135
    ISR_IRQ 136
    ISR_IRQ 137
    ISR_IRQ 138
    ISR_IRQ 139
    ISR_IRQ 140
    ISR_IRQ 141
    ISR_IRQ 142
    ISR_IRQ 143
    ISR_IRQ 144
    ISR_IRQ 145
    ISR_IRQ 146
    ISR_IRQ 147
    ISR_IRQ 148
    ISR_IRQ 149
    ISR_IRQ 150
    ISR_IRQ 151
    ISR_IRQ 152
    ISR_IRQ 153
    ISR_IRQ 154
    ISR_IRQ 155
    ISR_IRQ 156
    ISR_IRQ 157
    ISR_IRQ 158
    ISR_IRQ 159
    ISR_IRQ 160
    ISR_IRQ 161
    ISR_IRQ 162
    ISR_IRQ 163
    ISR_IRQ 164
    ISR_IRQ 165
    ISR_IRQ 166
    ISR_IRQ 167
    ISR_IRQ 168
    ISR_IRQ 169
    ISR_IRQ 170
    ISR_IRQ 171
    ISR_IRQ 172
    ISR_IRQ 173
    ISR_IRQ 174
    ISR_IRQ 175
    ISR_IRQ 176
    ISR_IRQ 177
    ISR_IRQ 178
    ISR_IRQ 179
    ISR_IRQ 180
    ISR_IRQ 181
    ISR_IRQ 182
    ISR_IRQ 183
    ISR_IRQ 184
    ISR_IRQ 185
    ISR_IRQ 186
    ISR_IRQ 187
    ISR_IRQ 188
    ISR_IRQ 189
    ISR_IRQ 190
    ISR_IRQ 191
    ISR_IRQ 192
    ISR_IRQ 193
    ISR_IRQ 194
    ISR_IRQ 195
    ISR_IRQ 196
    ISR_IRQ 197
    ISR_IRQ 198
    ISR_IRQ 199
    ISR_IRQ 200
    ISR_IRQ 201
    ISR_IRQ 202
    ISR_IRQ 203
    ISR_IRQ 204
    ISR_IRQ 205
    ISR_IRQ 206
    ISR_IRQ 207
    ISR_IRQ 208
    ISR_IRQ 209
    ISR_IRQ 210
    ISR_IRQ 211
    ISR_IRQ 212
    ISR_IRQ 213
    ISR_IRQ 214
    ISR_IRQ 215
    ISR_IRQ 216
    ISR_IRQ 217
    ISR_IRQ 218
    ISR_IRQ 219
    ISR_IRQ 220
    ISR_IRQ 221
    ISR_IRQ 222
    ISR_IRQ 223

    .global cognacos_interrupt_common
    .type cognacos_interrupt_common, @function
cognacos_interrupt_common:
    cld
    push rax
    push rbx
    push rcx
    push rdx
    push rsi
    push rdi
    push rbp
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15
    # Normalized IRQ frame uses the same hardware-CS offset as exceptions.
    mov eax, dword ptr [rsp + 144]
    and eax, 3
    cmp eax, 3
    jne .Lcognacos_irq_skip_user_save
    mov rbx, rsp
    and rsp, -16
    call cognacos_save_current_user_xstate_asm
    mov rsp, rbx
.Lcognacos_irq_skip_user_save:
    mov rdi, [rsp + 120]
    mov rsi, rsp
    mov r12, rsp
    and rsp, -16
    call cognacos_external_interrupt_dispatch
    mov rsp, r12
    mov eax, dword ptr [rsp + 144]
    and eax, 3
    cmp eax, 3
    jne .Lcognacos_irq_skip_user_restore
    mov rbx, rsp
    and rsp, -16
    call cognacos_current_user_xstate_ptr
    mov rsp, rbx
    test rax, rax
    jz .Lcognacos_irq_skip_user_restore
    mov r10, rax
    mov eax, 3
    xor edx, edx
    xrstor64 [r10]
.Lcognacos_irq_skip_user_restore:
    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rbp
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rbx
    pop rax
    add rsp, 16
    iretq

    .global cognacos_hal_ipi_stub
    .type cognacos_hal_ipi_stub, @function
cognacos_hal_ipi_stub:
    cld
    push rax
    push rbx
    push rcx
    push rdx
    push rsi
    push rdi
    push rbp
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15
    mov eax, dword ptr [rsp + 128]
    and eax, 3
    cmp eax, 3
    jne .Lcognacos_hal_skip_user_save
    mov rbx, rsp
    and rsp, -16
    call cognacos_save_current_user_xstate_asm
    mov rsp, rbx
.Lcognacos_hal_skip_user_save:
    mov rax, rsp
    and rsp, -16
    sub rsp, 16
    mov [rsp + 8], rax
    call cognacos_hal_ipi_handler
    mov rsp, [rsp + 8]
    mov eax, dword ptr [rsp + 128]
    and eax, 3
    cmp eax, 3
    jne .Lcognacos_hal_skip_user_restore
    mov rbx, rsp
    and rsp, -16
    call cognacos_current_user_xstate_ptr
    mov rsp, rbx
    test rax, rax
    jz .Lcognacos_hal_skip_user_restore
    mov r10, rax
    mov eax, 3
    xor edx, edx
    xrstor64 [r10]
.Lcognacos_hal_skip_user_restore:
    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rbp
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rbx
    pop rax
    iretq

    .global cognacos_ke_broadcast_ipi_stub
    .type cognacos_ke_broadcast_ipi_stub, @function
cognacos_ke_broadcast_ipi_stub:
    cld
    push rax
    push rbx
    push rcx
    push rdx
    push rsi
    push rdi
    push rbp
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15
    mov eax, dword ptr [rsp + 128]
    and eax, 3
    cmp eax, 3
    jne .Lcognacos_broadcast_skip_user_save
    mov rbx, rsp
    and rsp, -16
    call cognacos_save_current_user_xstate_asm
    mov rsp, rbx
.Lcognacos_broadcast_skip_user_save:
    mov rax, rsp
    and rsp, -16
    sub rsp, 16
    mov [rsp + 8], rax
    call cognacos_ke_broadcast_ipi_handler
    mov rsp, [rsp + 8]
    mov eax, dword ptr [rsp + 128]
    and eax, 3
    cmp eax, 3
    jne .Lcognacos_broadcast_skip_user_restore
    mov rbx, rsp
    and rsp, -16
    call cognacos_current_user_xstate_ptr
    mov rsp, rbx
    test rax, rax
    jz .Lcognacos_broadcast_skip_user_restore
    mov r10, rax
    mov eax, 3
    xor edx, edx
    xrstor64 [r10]
.Lcognacos_broadcast_skip_user_restore:
    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rbp
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rbx
    pop rax
    iretq

    .global cognacos_tsc_sync_stub
    .type cognacos_tsc_sync_stub, @function
cognacos_tsc_sync_stub:
    cld
    push rax
    push rbx
    push rcx
    push rdx
    push rsi
    push rdi
    push rbp
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15
    mov eax, dword ptr [rsp + 128]
    and eax, 3
    cmp eax, 3
    jne .Lcognacos_tsc_skip_user_save
    mov rbx, rsp
    and rsp, -16
    call cognacos_save_current_user_xstate_asm
    mov rsp, rbx
.Lcognacos_tsc_skip_user_save:
    mov rax, rsp
    and rsp, -16
    sub rsp, 16
    mov [rsp + 8], rax
    call cognacos_tsc_sync_handler
    mov rsp, [rsp + 8]
    mov eax, dword ptr [rsp + 128]
    and eax, 3
    cmp eax, 3
    jne .Lcognacos_tsc_skip_user_restore
    mov rbx, rsp
    and rsp, -16
    call cognacos_current_user_xstate_ptr
    mov rsp, rbx
    test rax, rax
    jz .Lcognacos_tsc_skip_user_restore
    mov r10, rax
    mov eax, 3
    xor edx, edx
    xrstor64 [r10]
.Lcognacos_tsc_skip_user_restore:
    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rbp
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rbx
    pop rax
    iretq

    .global cognacos_spurious_stub
    .type cognacos_spurious_stub, @function
cognacos_spurious_stub:
    iretq

    .global cognacos_timer_stub
    .type cognacos_timer_stub, @function
cognacos_timer_stub:
    cld
    push rax
    push rbx
    push rcx
    push rdx
    push rsi
    push rdi
    push rbp
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15
    # Only a ring-3 interrupt needs the per-thread user XSTATE snapshot.
    mov eax, dword ptr [rsp + 128]
    and eax, 3
    cmp eax, 3
    jne .Lcognacos_timer_skip_user_save
    mov rbx, rsp
    and rsp, -16
    call cognacos_save_current_user_xstate_asm
    mov rsp, rbx
.Lcognacos_timer_skip_user_save:
    mov rax, rsp
    and rsp, -16
    sub rsp, 16
    mov [rsp + 8], rax
    call cognacos_timer_interrupt_handler
    mov rsp, [rsp + 8]
    # The timer handler may have scheduled away and resumed this thread. Read
    # its current XSTATE area and restore it as the final operation before IRETQ.
    mov eax, dword ptr [rsp + 128]
    and eax, 3
    cmp eax, 3
    jne .Lcognacos_timer_skip_user_restore
    mov rbx, rsp
    and rsp, -16
    call cognacos_current_user_xstate_ptr
    mov rsp, rbx
    test rax, rax
    jz .Lcognacos_timer_skip_user_restore
    mov r10, rax
    mov eax, 3
    xor edx, edx
    xrstor64 [r10]
.Lcognacos_timer_skip_user_restore:
    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rbp
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rbx
    pop rax
    iretq
"#);

unsafe extern "C" {
    fn cognacos_load_gdt();
    fn cognacos_isr_0(); fn cognacos_isr_1(); fn cognacos_isr_2(); fn cognacos_isr_3();
    fn cognacos_isr_4(); fn cognacos_isr_5(); fn cognacos_isr_6(); fn cognacos_isr_7();
    fn cognacos_isr_8(); fn cognacos_isr_9(); fn cognacos_isr_10(); fn cognacos_isr_11();
    fn cognacos_isr_12(); fn cognacos_isr_13(); fn cognacos_isr_14(); fn cognacos_isr_15();
    fn cognacos_isr_16(); fn cognacos_isr_17(); fn cognacos_isr_18(); fn cognacos_isr_19();
    fn cognacos_isr_20(); fn cognacos_isr_21(); fn cognacos_isr_22(); fn cognacos_isr_23();
    fn cognacos_isr_24(); fn cognacos_isr_25(); fn cognacos_isr_26(); fn cognacos_isr_27();
    fn cognacos_isr_28(); fn cognacos_isr_29(); fn cognacos_isr_30(); fn cognacos_isr_31();
    fn cognacos_tlb_ipi_stub(); fn cognacos_hal_ipi_stub(); fn cognacos_ke_broadcast_ipi_stub(); fn cognacos_tsc_sync_stub(); fn cognacos_spurious_stub(); fn cognacos_timer_stub();
    fn cognacos_irq_32();
    fn cognacos_irq_33();
    fn cognacos_irq_34();
    fn cognacos_irq_35();
    fn cognacos_irq_36();
    fn cognacos_irq_37();
    fn cognacos_irq_38();
    fn cognacos_irq_39();
    fn cognacos_irq_40();
    fn cognacos_irq_41();
    fn cognacos_irq_42();
    fn cognacos_irq_43();
    fn cognacos_irq_44();
    fn cognacos_irq_45();
    fn cognacos_irq_46();
    fn cognacos_irq_47();
    fn cognacos_irq_48();
    fn cognacos_irq_49();
    fn cognacos_irq_50();
    fn cognacos_irq_51();
    fn cognacos_irq_52();
    fn cognacos_irq_53();
    fn cognacos_irq_54();
    fn cognacos_irq_55();
    fn cognacos_irq_56();
    fn cognacos_irq_57();
    fn cognacos_irq_58();
    fn cognacos_irq_59();
    fn cognacos_irq_60();
    fn cognacos_irq_61();
    fn cognacos_irq_62();
    fn cognacos_irq_63();
    fn cognacos_irq_64();
    fn cognacos_irq_65();
    fn cognacos_irq_66();
    fn cognacos_irq_67();
    fn cognacos_irq_68();
    fn cognacos_irq_69();
    fn cognacos_irq_70();
    fn cognacos_irq_71();
    fn cognacos_irq_72();
    fn cognacos_irq_73();
    fn cognacos_irq_74();
    fn cognacos_irq_75();
    fn cognacos_irq_76();
    fn cognacos_irq_77();
    fn cognacos_irq_78();
    fn cognacos_irq_79();
    fn cognacos_irq_80();
    fn cognacos_irq_81();
    fn cognacos_irq_82();
    fn cognacos_irq_83();
    fn cognacos_irq_84();
    fn cognacos_irq_85();
    fn cognacos_irq_86();
    fn cognacos_irq_87();
    fn cognacos_irq_88();
    fn cognacos_irq_89();
    fn cognacos_irq_90();
    fn cognacos_irq_91();
    fn cognacos_irq_92();
    fn cognacos_irq_93();
    fn cognacos_irq_94();
    fn cognacos_irq_95();
    fn cognacos_irq_96();
    fn cognacos_irq_97();
    fn cognacos_irq_98();
    fn cognacos_irq_99();
    fn cognacos_irq_100();
    fn cognacos_irq_101();
    fn cognacos_irq_102();
    fn cognacos_irq_103();
    fn cognacos_irq_104();
    fn cognacos_irq_105();
    fn cognacos_irq_106();
    fn cognacos_irq_107();
    fn cognacos_irq_108();
    fn cognacos_irq_109();
    fn cognacos_irq_110();
    fn cognacos_irq_111();
    fn cognacos_irq_112();
    fn cognacos_irq_113();
    fn cognacos_irq_114();
    fn cognacos_irq_115();
    fn cognacos_irq_116();
    fn cognacos_irq_117();
    fn cognacos_irq_118();
    fn cognacos_irq_119();
    fn cognacos_irq_120();
    fn cognacos_irq_121();
    fn cognacos_irq_122();
    fn cognacos_irq_123();
    fn cognacos_irq_124();
    fn cognacos_irq_125();
    fn cognacos_irq_126();
    fn cognacos_irq_127();
    fn cognacos_irq_128();
    fn cognacos_irq_129();
    fn cognacos_irq_130();
    fn cognacos_irq_131();
    fn cognacos_irq_132();
    fn cognacos_irq_133();
    fn cognacos_irq_134();
    fn cognacos_irq_135();
    fn cognacos_irq_136();
    fn cognacos_irq_137();
    fn cognacos_irq_138();
    fn cognacos_irq_139();
    fn cognacos_irq_140();
    fn cognacos_irq_141();
    fn cognacos_irq_142();
    fn cognacos_irq_143();
    fn cognacos_irq_144();
    fn cognacos_irq_145();
    fn cognacos_irq_146();
    fn cognacos_irq_147();
    fn cognacos_irq_148();
    fn cognacos_irq_149();
    fn cognacos_irq_150();
    fn cognacos_irq_151();
    fn cognacos_irq_152();
    fn cognacos_irq_153();
    fn cognacos_irq_154();
    fn cognacos_irq_155();
    fn cognacos_irq_156();
    fn cognacos_irq_157();
    fn cognacos_irq_158();
    fn cognacos_irq_159();
    fn cognacos_irq_160();
    fn cognacos_irq_161();
    fn cognacos_irq_162();
    fn cognacos_irq_163();
    fn cognacos_irq_164();
    fn cognacos_irq_165();
    fn cognacos_irq_166();
    fn cognacos_irq_167();
    fn cognacos_irq_168();
    fn cognacos_irq_169();
    fn cognacos_irq_170();
    fn cognacos_irq_171();
    fn cognacos_irq_172();
    fn cognacos_irq_173();
    fn cognacos_irq_174();
    fn cognacos_irq_175();
    fn cognacos_irq_176();
    fn cognacos_irq_177();
    fn cognacos_irq_178();
    fn cognacos_irq_179();
    fn cognacos_irq_180();
    fn cognacos_irq_181();
    fn cognacos_irq_182();
    fn cognacos_irq_183();
    fn cognacos_irq_184();
    fn cognacos_irq_185();
    fn cognacos_irq_186();
    fn cognacos_irq_187();
    fn cognacos_irq_188();
    fn cognacos_irq_189();
    fn cognacos_irq_190();
    fn cognacos_irq_191();
    fn cognacos_irq_192();
    fn cognacos_irq_193();
    fn cognacos_irq_194();
    fn cognacos_irq_195();
    fn cognacos_irq_196();
    fn cognacos_irq_197();
    fn cognacos_irq_198();
    fn cognacos_irq_199();
    fn cognacos_irq_200();
    fn cognacos_irq_201();
    fn cognacos_irq_202();
    fn cognacos_irq_203();
    fn cognacos_irq_204();
    fn cognacos_irq_205();
    fn cognacos_irq_206();
    fn cognacos_irq_207();
    fn cognacos_irq_208();
    fn cognacos_irq_209();
    fn cognacos_irq_210();
    fn cognacos_irq_211();
    fn cognacos_irq_212();
    fn cognacos_irq_213();
    fn cognacos_irq_214();
    fn cognacos_irq_215();
    fn cognacos_irq_216();
    fn cognacos_irq_217();
    fn cognacos_irq_218();
    fn cognacos_irq_219();
    fn cognacos_irq_220();
    fn cognacos_irq_221();
    fn cognacos_irq_222();
    fn cognacos_irq_223();
}

/// Build and load a private GDT/TSS for one logical processor. IST1 is the
/// double-fault stack, IST2 the NMI stack, and IST3 the machine-check stack.
unsafe fn initialize_cpu_tss(cpu: usize, rsp0: u64) -> Result<(), ()> {
    if cpu >= crate::hal::MAX_CPUS || rsp0 == 0 { return Err(()); }
    let tss = unsafe { core::ptr::addr_of_mut!(CPU_TSS[cpu]) };
    unsafe { core::ptr::addr_of_mut!((*tss).rsp0).write_unaligned(rsp0); }
    for slot in 0..3 {
        let base = unsafe { core::ptr::addr_of_mut!(CPU_IST_STACKS[cpu][slot][0]) as u64 };
        let top = base.checked_add(IST_STACK_BYTES as u64).ok_or(())? & !0xf;
        unsafe { core::ptr::addr_of_mut!((*tss).ist[slot]).write_unaligned(top); }
    }
    let tss_base = tss as u64;
    let limit = (size_of::<TaskStateSegment>() - 1) as u64;
    let low = (limit & 0xffff)
        | ((tss_base & 0x00ff_ffff) << 16)
        | (0x89u64 << 40)
        | ((limit & 0x000f_0000) << 32)
        | ((tss_base & 0xff00_0000) << 32);
    unsafe {
        let gdt = core::ptr::addr_of_mut!(CPU_GDT[cpu][0]);
        gdt.write(0);
        gdt.add(1).write(0x00af_9a00_0000_ffff); // ring-0 64-bit code, selector 0x08
        gdt.add(2).write(0x00cf_9200_0000_ffff); // ring-0 data, selector 0x10
        gdt.add(3).write(0x00af_fa00_0000_ffff); // ring-3 64-bit code, selector 0x1b
        gdt.add(4).write(0x00cf_f200_0000_ffff); // ring-3 data, selector 0x23
        gdt.add(5).write(low);                    // 64-bit TSS, selector 0x28
        gdt.add(6).write(tss_base >> 32);
    }
    Ok(())
}

unsafe fn load_cpu_gdt(cpu: usize) -> Result<(), ()> {
    if cpu >= crate::hal::MAX_CPUS { return Err(()); }
    let gdt = unsafe { core::ptr::addr_of!(CPU_GDT[cpu][0]) };
    let pointer = DescriptorTablePointer { limit: (7 * size_of::<u64>() - 1) as u16, base: gdt as u64 };
    unsafe {
        asm!("lgdt [{}]", in(reg) &pointer, options(readonly, nostack, preserves_flags));
        asm!(
            "push 0x08",
            "lea rax, [rip + 2f]",
            "push rax",
            "retfq",
            "2:",
            "mov ax, 0x10",
            "mov ds, ax",
            "mov es, ax",
            "mov ss, ax",
            "xor eax, eax",
            "mov fs, ax",
            "mov gs, ax",
            "mov ax, 0x28",
            "ltr ax",
            out("rax") _,
            options(preserves_flags)
        );
    }
    Ok(())
}

/// Install this CPU's private GDT/TSS and the shared IDT. Must run on the CPU
/// being initialized; the AP stack must already be valid in the permanent CR3.
pub unsafe fn load_cpu_descriptor_tables(logical_id: usize, rsp0: u64) -> Result<(), ()> {
    unsafe { initialize_cpu_tss(logical_id, rsp0)?; load_cpu_gdt(logical_id)?; }
    let idt_ptr = core::ptr::addr_of!(IDT).cast::<IdtEntry>();
    let pointer = DescriptorTablePointer {
        limit: (IDT_ENTRIES * size_of::<IdtEntry>() - 1) as u16,
        base: idt_ptr as u64,
    };
    unsafe { asm!("lidt [{}]", in(reg) &pointer, options(readonly, nostack, preserves_flags)); }
    if !per_cpu_tss_self_test(logical_id) { return Err(()); }
    unsafe { crate::syscall::initialize_windows_syscall_abi(logical_id)?; }
    unsafe { crate::syscall::prepare_windows_syscall_stack(logical_id)?; }
    Ok(())
}

/// Validate the active per-CPU TSS descriptor and the exception IST wiring.
pub fn per_cpu_tss_self_test(cpu: usize) -> bool {
    if cpu >= crate::hal::MAX_CPUS { return false; }
    unsafe {
        let tss = core::ptr::addr_of!(CPU_TSS[cpu]);
        let rsp0 = core::ptr::addr_of!((*tss).rsp0).read_unaligned();
        let ist1 = core::ptr::addr_of!((*tss).ist[0]).read_unaligned();
        let ist2 = core::ptr::addr_of!((*tss).ist[1]).read_unaligned();
        let ist3 = core::ptr::addr_of!((*tss).ist[2]).read_unaligned();
        let descriptor = core::ptr::addr_of!(CPU_GDT[cpu][5]).read();
        let high = core::ptr::addr_of!(CPU_GDT[cpu][6]).read();
        let idt = core::ptr::addr_of!(IDT).cast::<IdtEntry>();
        let nmi_ist = core::ptr::addr_of!((*idt.add(2)).ist).read_unaligned() & 7;
        let df_ist = core::ptr::addr_of!((*idt.add(8)).ist).read_unaligned() & 7;
        let mc_ist = core::ptr::addr_of!((*idt.add(18)).ist).read_unaligned() & 7;
        rsp0 != 0 && ist1 != 0 && ist2 != 0 && ist3 != 0
            && ist1 & 0xf == 0 && ist2 & 0xf == 0 && ist3 & 0xf == 0
            && ist1 != ist2 && ist1 != ist3 && ist2 != ist3
            // LTR marks an available 64-bit TSS descriptor busy (type 0xB).
            && ((descriptor >> 40) & 0xff) == 0x8b
            && high == (tss as u64 >> 32)
            && nmi_ist == 2 && df_ist == 1 && mc_ist == 3
    }
}

/// Update the privilege-transition stack when a kernel thread is dispatched.
pub fn set_kernel_stack(cpu: usize, stack_top: u64) -> bool {
    if cpu >= crate::hal::MAX_CPUS || stack_top == 0 { return false; }
    unsafe { core::ptr::addr_of_mut!(CPU_TSS[cpu].rsp0).write_unaligned(stack_top); }
    true
}

pub unsafe fn initialize() {
    // Keep the BSP quiescent while replacing its GDT/TSS and building the IDT.
    unsafe { asm!("cli", options(nomem, nostack)); }
    let mut rsp0: u64;
    unsafe { asm!("mov {}, rsp", out(reg) rsp0, options(nomem, nostack, preserves_flags)); }
    unsafe {
        if initialize_cpu_tss(0, rsp0).is_err() || load_cpu_gdt(0).is_err() {
            loop { core::hint::spin_loop(); }
        }
    }
    // Install the privileged SYSCALL entry and per-CPU kernel stack before
    // any ring-3 process can execute a Windows-style syscall instruction.
    if unsafe { crate::syscall::initialize_windows_syscall_abi(0) }.is_err() {
        unsafe { crate::serial_write(b"FATAL: Windows SYSCALL MSR/GS initialization self-test failed.\r\n"); }
        loop { core::hint::spin_loop(); }
    }
    let handlers: [usize; EXCEPTION_VECTORS] = [
        cognacos_isr_0 as usize, cognacos_isr_1 as usize, cognacos_isr_2 as usize, cognacos_isr_3 as usize,
        cognacos_isr_4 as usize, cognacos_isr_5 as usize, cognacos_isr_6 as usize, cognacos_isr_7 as usize,
        cognacos_isr_8 as usize, cognacos_isr_9 as usize, cognacos_isr_10 as usize, cognacos_isr_11 as usize,
        cognacos_isr_12 as usize, cognacos_isr_13 as usize, cognacos_isr_14 as usize, cognacos_isr_15 as usize,
        cognacos_isr_16 as usize, cognacos_isr_17 as usize, cognacos_isr_18 as usize, cognacos_isr_19 as usize,
        cognacos_isr_20 as usize, cognacos_isr_21 as usize, cognacos_isr_22 as usize, cognacos_isr_23 as usize,
        cognacos_isr_24 as usize, cognacos_isr_25 as usize, cognacos_isr_26 as usize, cognacos_isr_27 as usize,
        cognacos_isr_28 as usize, cognacos_isr_29 as usize, cognacos_isr_30 as usize, cognacos_isr_31 as usize,
    ];
    let idt_ptr = core::ptr::addr_of_mut!(IDT).cast::<IdtEntry>();
    for vector in 0..EXCEPTION_VECTORS {
        unsafe { idt_ptr.add(vector).write(IdtEntry::interrupt_gate(handlers[vector] as u64)); }
    }
    // Hardware IST selection avoids using a potentially corrupt interrupted stack.
    unsafe {
        idt_ptr.add(2).write(IdtEntry::interrupt_gate_ist(handlers[2] as u64, 2));
        idt_ptr.add(8).write(IdtEntry::interrupt_gate_ist(handlers[8] as u64, 1));
        idt_ptr.add(18).write(IdtEntry::interrupt_gate_ist(handlers[18] as u64, 3));
    }
    let irq_handlers: [usize; 192] = [
        cognacos_irq_32 as usize,
        cognacos_irq_33 as usize,
        cognacos_irq_34 as usize,
        cognacos_irq_35 as usize,
        cognacos_irq_36 as usize,
        cognacos_irq_37 as usize,
        cognacos_irq_38 as usize,
        cognacos_irq_39 as usize,
        cognacos_irq_40 as usize,
        cognacos_irq_41 as usize,
        cognacos_irq_42 as usize,
        cognacos_irq_43 as usize,
        cognacos_irq_44 as usize,
        cognacos_irq_45 as usize,
        cognacos_irq_46 as usize,
        cognacos_irq_47 as usize,
        cognacos_irq_48 as usize,
        cognacos_irq_49 as usize,
        cognacos_irq_50 as usize,
        cognacos_irq_51 as usize,
        cognacos_irq_52 as usize,
        cognacos_irq_53 as usize,
        cognacos_irq_54 as usize,
        cognacos_irq_55 as usize,
        cognacos_irq_56 as usize,
        cognacos_irq_57 as usize,
        cognacos_irq_58 as usize,
        cognacos_irq_59 as usize,
        cognacos_irq_60 as usize,
        cognacos_irq_61 as usize,
        cognacos_irq_62 as usize,
        cognacos_irq_63 as usize,
        cognacos_irq_64 as usize,
        cognacos_irq_65 as usize,
        cognacos_irq_66 as usize,
        cognacos_irq_67 as usize,
        cognacos_irq_68 as usize,
        cognacos_irq_69 as usize,
        cognacos_irq_70 as usize,
        cognacos_irq_71 as usize,
        cognacos_irq_72 as usize,
        cognacos_irq_73 as usize,
        cognacos_irq_74 as usize,
        cognacos_irq_75 as usize,
        cognacos_irq_76 as usize,
        cognacos_irq_77 as usize,
        cognacos_irq_78 as usize,
        cognacos_irq_79 as usize,
        cognacos_irq_80 as usize,
        cognacos_irq_81 as usize,
        cognacos_irq_82 as usize,
        cognacos_irq_83 as usize,
        cognacos_irq_84 as usize,
        cognacos_irq_85 as usize,
        cognacos_irq_86 as usize,
        cognacos_irq_87 as usize,
        cognacos_irq_88 as usize,
        cognacos_irq_89 as usize,
        cognacos_irq_90 as usize,
        cognacos_irq_91 as usize,
        cognacos_irq_92 as usize,
        cognacos_irq_93 as usize,
        cognacos_irq_94 as usize,
        cognacos_irq_95 as usize,
        cognacos_irq_96 as usize,
        cognacos_irq_97 as usize,
        cognacos_irq_98 as usize,
        cognacos_irq_99 as usize,
        cognacos_irq_100 as usize,
        cognacos_irq_101 as usize,
        cognacos_irq_102 as usize,
        cognacos_irq_103 as usize,
        cognacos_irq_104 as usize,
        cognacos_irq_105 as usize,
        cognacos_irq_106 as usize,
        cognacos_irq_107 as usize,
        cognacos_irq_108 as usize,
        cognacos_irq_109 as usize,
        cognacos_irq_110 as usize,
        cognacos_irq_111 as usize,
        cognacos_irq_112 as usize,
        cognacos_irq_113 as usize,
        cognacos_irq_114 as usize,
        cognacos_irq_115 as usize,
        cognacos_irq_116 as usize,
        cognacos_irq_117 as usize,
        cognacos_irq_118 as usize,
        cognacos_irq_119 as usize,
        cognacos_irq_120 as usize,
        cognacos_irq_121 as usize,
        cognacos_irq_122 as usize,
        cognacos_irq_123 as usize,
        cognacos_irq_124 as usize,
        cognacos_irq_125 as usize,
        cognacos_irq_126 as usize,
        cognacos_irq_127 as usize,
        cognacos_irq_128 as usize,
        cognacos_irq_129 as usize,
        cognacos_irq_130 as usize,
        cognacos_irq_131 as usize,
        cognacos_irq_132 as usize,
        cognacos_irq_133 as usize,
        cognacos_irq_134 as usize,
        cognacos_irq_135 as usize,
        cognacos_irq_136 as usize,
        cognacos_irq_137 as usize,
        cognacos_irq_138 as usize,
        cognacos_irq_139 as usize,
        cognacos_irq_140 as usize,
        cognacos_irq_141 as usize,
        cognacos_irq_142 as usize,
        cognacos_irq_143 as usize,
        cognacos_irq_144 as usize,
        cognacos_irq_145 as usize,
        cognacos_irq_146 as usize,
        cognacos_irq_147 as usize,
        cognacos_irq_148 as usize,
        cognacos_irq_149 as usize,
        cognacos_irq_150 as usize,
        cognacos_irq_151 as usize,
        cognacos_irq_152 as usize,
        cognacos_irq_153 as usize,
        cognacos_irq_154 as usize,
        cognacos_irq_155 as usize,
        cognacos_irq_156 as usize,
        cognacos_irq_157 as usize,
        cognacos_irq_158 as usize,
        cognacos_irq_159 as usize,
        cognacos_irq_160 as usize,
        cognacos_irq_161 as usize,
        cognacos_irq_162 as usize,
        cognacos_irq_163 as usize,
        cognacos_irq_164 as usize,
        cognacos_irq_165 as usize,
        cognacos_irq_166 as usize,
        cognacos_irq_167 as usize,
        cognacos_irq_168 as usize,
        cognacos_irq_169 as usize,
        cognacos_irq_170 as usize,
        cognacos_irq_171 as usize,
        cognacos_irq_172 as usize,
        cognacos_irq_173 as usize,
        cognacos_irq_174 as usize,
        cognacos_irq_175 as usize,
        cognacos_irq_176 as usize,
        cognacos_irq_177 as usize,
        cognacos_irq_178 as usize,
        cognacos_irq_179 as usize,
        cognacos_irq_180 as usize,
        cognacos_irq_181 as usize,
        cognacos_irq_182 as usize,
        cognacos_irq_183 as usize,
        cognacos_irq_184 as usize,
        cognacos_irq_185 as usize,
        cognacos_irq_186 as usize,
        cognacos_irq_187 as usize,
        cognacos_irq_188 as usize,
        cognacos_irq_189 as usize,
        cognacos_irq_190 as usize,
        cognacos_irq_191 as usize,
        cognacos_irq_192 as usize,
        cognacos_irq_193 as usize,
        cognacos_irq_194 as usize,
        cognacos_irq_195 as usize,
        cognacos_irq_196 as usize,
        cognacos_irq_197 as usize,
        cognacos_irq_198 as usize,
        cognacos_irq_199 as usize,
        cognacos_irq_200 as usize,
        cognacos_irq_201 as usize,
        cognacos_irq_202 as usize,
        cognacos_irq_203 as usize,
        cognacos_irq_204 as usize,
        cognacos_irq_205 as usize,
        cognacos_irq_206 as usize,
        cognacos_irq_207 as usize,
        cognacos_irq_208 as usize,
        cognacos_irq_209 as usize,
        cognacos_irq_210 as usize,
        cognacos_irq_211 as usize,
        cognacos_irq_212 as usize,
        cognacos_irq_213 as usize,
        cognacos_irq_214 as usize,
        cognacos_irq_215 as usize,
        cognacos_irq_216 as usize,
        cognacos_irq_217 as usize,
        cognacos_irq_218 as usize,
        cognacos_irq_219 as usize,
        cognacos_irq_220 as usize,
        cognacos_irq_221 as usize,
        cognacos_irq_222 as usize,
        cognacos_irq_223 as usize,
    ];
    for (index, handler) in irq_handlers.iter().enumerate() {
        unsafe { idt_ptr.add(0x20 + index).write(IdtEntry::interrupt_gate(*handler as u64)); }
    }
    // 0xF1 is reserved for synchronous TLB shootdowns; 0xFF is the LAPIC
    // spurious vector and intentionally does not send an EOI.
    unsafe {
        idt_ptr.add(crate::hal::TLB_SHOOTDOWN_VECTOR as usize).write(IdtEntry::interrupt_gate(cognacos_tlb_ipi_stub as u64));
        idt_ptr.add(crate::hal::HAL_IPI_VECTOR as usize).write(IdtEntry::interrupt_gate(cognacos_hal_ipi_stub as u64));
        idt_ptr.add(crate::hal::KE_BROADCAST_VECTOR as usize).write(IdtEntry::interrupt_gate(cognacos_ke_broadcast_ipi_stub as u64));
        idt_ptr.add(crate::hal::TSC_SYNC_VECTOR as usize).write(IdtEntry::interrupt_gate(cognacos_tsc_sync_stub as u64));
        idt_ptr.add(0xFF).write(IdtEntry::interrupt_gate(cognacos_spurious_stub as u64));
        idt_ptr.add(crate::hal::LOCAL_TIMER_VECTOR as usize).write(IdtEntry::interrupt_gate(cognacos_timer_stub as u64));
    }
    let pointer = DescriptorTablePointer {
        limit: (IDT_ENTRIES * size_of::<IdtEntry>() - 1) as u16,
        base: idt_ptr as u64,
    };
    unsafe { asm!("lidt [{}]", in(reg) &pointer, options(readonly, nostack, preserves_flags)) };
}

/// Assembly passes the normalized vector/error-code pair and a saved register
/// frame. This is fatal by design: returning to an untrusted exception frame
/// is not supported until recovery and interrupt-stack switching are implemented.
#[unsafe(no_mangle)]
pub extern "C" fn cognacos_exception_fatal(vector: u64, error: u64, frame: *const u64) -> ! {
    unsafe {
        crate::serial_write(b"\r\nCognacOS FATAL CPU exception vector=");
        crate::serial_write_hex(vector);
        crate::serial_write(b" error=");
        crate::serial_write_hex(error);
        crate::serial_write(b" RIP=");
        crate::serial_write_hex(frame.add(17).read());
        crate::serial_write(b" CS=");
        crate::serial_write_hex(frame.add(18).read());
        crate::serial_write(b" RFLAGS=");
        crate::serial_write_hex(frame.add(19).read());
        crate::serial_write(b"\r\nRAX="); crate::serial_write_hex(frame.add(14).read());
        crate::serial_write(b" RBX="); crate::serial_write_hex(frame.add(13).read());
        crate::serial_write(b" RCX="); crate::serial_write_hex(frame.add(12).read());
        crate::serial_write(b" RDX="); crate::serial_write_hex(frame.add(11).read());
        crate::serial_write(b" RSI="); crate::serial_write_hex(frame.add(10).read());
        crate::serial_write(b" RDI="); crate::serial_write_hex(frame.add(9).read());
        crate::serial_write(b" RBP="); crate::serial_write_hex(frame.add(8).read());
        crate::serial_write(b" R8="); crate::serial_write_hex(frame.add(7).read());
        crate::serial_write(b" R9="); crate::serial_write_hex(frame.add(6).read());
        crate::serial_write(b" R10="); crate::serial_write_hex(frame.add(5).read());
        crate::serial_write(b" R11="); crate::serial_write_hex(frame.add(4).read());
        crate::serial_write(b" R12="); crate::serial_write_hex(frame.add(3).read());
        crate::serial_write(b" R13="); crate::serial_write_hex(frame.add(2).read());
        crate::serial_write(b" R14="); crate::serial_write_hex(frame.add(1).read());
        crate::serial_write(b" R15="); crate::serial_write_hex(frame.read());
        if vector == 14 {
            let cr2: u64;
            core::arch::asm!("mov {}, cr2", out(reg) cr2, options(nomem, nostack, preserves_flags));
            crate::serial_write(b" CR2=");
            crate::serial_write_hex(cr2);
        }
        crate::serial_write(b"\r\nSystem halted.\r\n");
        crate::halt()
    }
}

/// Register layout shared by the exception entry stub and CPL3 delivery path.
/// Hardware appends RSP and SS when an exception crosses from CPL3 to CPL0.
#[allow(dead_code)]
#[repr(C)]
struct UserExceptionTrapFrame {
    saved_registers: [u64; 15],
    vector: u64,
    error_code: u64,
    rip: u64,
    cs: u64,
    rflags: u64,
    rsp: u64,
    ss: u64,
}

fn user_exception_status(vector: u64, _error: u64) -> u64 {
    match vector {
        0 => 0xC000_0094, // STATUS_INTEGER_DIVIDE_BY_ZERO
        1 => 0x8000_0004, // STATUS_SINGLE_STEP
        3 => 0x8000_0003, // STATUS_BREAKPOINT
        6 => 0xC000_001D, // STATUS_ILLEGAL_INSTRUCTION
        13 => 0xC000_0096, // STATUS_PRIVILEGED_INSTRUCTION
        17 => 0x8000_0002, // STATUS_DATATYPE_MISALIGNMENT
        _ => 0xC000_0005, // STATUS_ACCESS_VIOLATION
    }
}

/// Unresolved CPL3 exceptions are process-scoped. This is the termination fallback
/// until a user-mode exception/SEH dispatcher can resume or unwind user execution.
fn dispatch_user_mode_exception(vector: u64, error: u64, frame: *const u64) -> ! {
    let trap = unsafe { &*frame.cast::<UserExceptionTrapFrame>() };
    if trap.vector != vector || trap.error_code != error || trap.cs & 3 != 3
        || trap.ss & 3 != 3 {
        // A structurally inconsistent hardware frame is a supervisor integrity fault.
        cognacos_exception_fatal(vector, error, frame);
    }
    let pid = crate::thread::current_process_id();
    if pid == 0 {
        cognacos_exception_fatal(vector, error, frame);
    }
    // Invalid/noncanonical user RIP or RSP is a fault in the process context,
    // not a reason to bugcheck the kernel.
    let valid_user_context = trap.rip >= 0x1000
        && trap.rip < crate::ntoskrnl::mm::address_space::USER_END
        && trap.rsp >= 0x1000
        && trap.rsp < crate::ntoskrnl::mm::address_space::USER_END;
    unsafe {
        crate::serial_write(b"CPL3 exception: vector=");
        crate::serial_write_hex(vector);
        crate::serial_write(b" error=");
        crate::serial_write_hex(error);
        crate::serial_write(b" RIP=");
        crate::serial_write_hex(trap.rip);
        if vector == 14 {
            let cr2: u64;
            core::arch::asm!("mov {}, cr2", out(reg) cr2, options(nomem, nostack, preserves_flags));
            crate::serial_write(b" CR2=");
            crate::serial_write_hex(cr2);
        }
        crate::serial_write(b" process=");
        crate::serial_write_hex(pid);
        crate::serial_write(b"\r\n");
    }
    let status = if valid_user_context {
        user_exception_status(vector, error)
    } else {
        STATUS_ACCESS_VIOLATION
    };
    crate::ntoskrnl::ke::thread::terminate_current_process_from_fault(pid, status)
}

/// Dispatch recoverable faults. A return value of one asks the assembly stub to
/// restore the original frame and IRETQ; unhandled supervisor exceptions halt.
#[unsafe(no_mangle)]
pub extern "C" fn cognacos_exception_dispatch(vector: u64, error: u64, frame: *const u64) -> u64 {
    if vector == 2 && crate::ntoskrnl::ke::runtime::dispatch_nmi_callbacks() { return 1; }
    // Determine privilege from the saved CS. A user fault must never consume a
    // kernel probe fixup, even if its RIP happens to match a registered record.
    if vector < EXCEPTION_VECTORS as u64 {
        let saved_cs = unsafe { frame.add(18).read() };
        if saved_cs & 3 == 3 {
            if vector == 14 {
                let cr2: u64;
                unsafe { core::arch::asm!("mov {}, cr2", out(reg) cr2, options(nomem, nostack, preserves_flags)); }
                if crate::ntoskrnl::mm::address_space::handle_current_user_page_fault(cr2, error) { return 1; }
            }
            dispatch_user_mode_exception(vector, error, frame);
        }
    }
    if vector == 14 {
        let cr2: u64;
        unsafe { core::arch::asm!("mov {}, cr2", out(reg) cr2, options(nomem, nostack, preserves_flags)); }
        if crate::kernel_vm::handle_page_fault(cr2, error) { return 1; }
        let saved_rip = unsafe { frame.add(17).read() };
        if apply_trap_frame_fixup(vector, saved_rip, cr2, error, frame) { return 1; }

    } else if vector < EXCEPTION_VECTORS as u64 {
        // Non-page-fault probes match vector/error/RIP only; CR2 is not meaningful.
        let saved_rip = unsafe { frame.add(17).read() };
        if apply_trap_frame_fixup(vector, saved_rip, 0, error, frame) { return 1; }
    }
    cognacos_exception_fatal(vector, error, frame)
}

/// Install a DPL3 interrupt gate for a controlled user-to-kernel entry point.
/// External interrupts remain disabled; this is intended for software interrupts/syscalls.
pub unsafe fn install_user_gate(vector: usize, handler: u64) {
    if vector < EXCEPTION_VECTORS || vector >= IDT_ENTRIES { return; }
    let idt_ptr = core::ptr::addr_of_mut!(IDT).cast::<IdtEntry>();
    unsafe { idt_ptr.add(vector).write(IdtEntry::user_interrupt_gate(handler)); }
}

/// Switch CR3 to the kernel-owned PML4 after its identity mappings are validated.
pub unsafe fn enable_kernel_paging(pml4_physical: u64) {
    unsafe { asm!("mov cr3, {}", in(reg) pml4_physical, options(nostack, preserves_flags)); }
}


global_asm!(r#"
    .section .text
    .global cognacos_context_switch
    .type cognacos_context_switch, @function
cognacos_context_switch:
    push rbp
    push rbx
    push r12
    push r13
    push r14
    push r15
    mov [rdi], rsp
    mov rsp, rsi
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbx
    pop rbp
    ret

    .global cognacos_context_switch_publish
    .type cognacos_context_switch_publish, @function
cognacos_context_switch_publish:
    push rbp
    push rbx
    push r12
    push r13
    push r14
    push r15
    mov [rdi], rsp
    mov r12, rdi
    mov r13, rsi
    mov r14, rdx
    mov r15, 0
    mov rdi, rcx
    and rsp, -16
    call r14
    mov rsp, r13
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbx
    pop rbp
    ret

    # xstate-aware context switch: callback runs first, then XRSTOR is the last
    # operation touching floating-point state before the new stack is resumed.
    .global cognacos_context_switch_xstate
    .type cognacos_context_switch_xstate, @function
cognacos_context_switch_xstate:
    push rbp
    push rbx
    push r12
    push r13
    push r14
    push r15
    mov [rdi], rsp
    mov r15, rdx
    test r15, r15
    jz .Lctx_xstate_no_restore
    mov eax, 3
    xor edx, edx
    xrstor64 [r15]
.Lctx_xstate_no_restore:
    mov rsp, rsi
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbx
    pop rbp
    ret

    .global cognacos_context_switch_publish_xstate
    .type cognacos_context_switch_publish_xstate, @function
cognacos_context_switch_publish_xstate:
    push rbp
    push rbx
    push r12
    push r13
    push r14
    push r15
    mov [rdi], rsp
    mov r12, rdi
    mov r13, rsi
    mov r14, rdx
    mov r15, r8
    mov rdi, rcx
    and rsp, -16
    call r14
    mov rsp, r13
    test r15, r15
    jz .Lctx_publish_no_restore
    mov eax, 3
    xor edx, edx
    xrstor64 [r15]
.Lctx_publish_no_restore:
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbx
    pop rbp
    ret
"#);

unsafe extern "C" {
    fn cognacos_context_switch(old_rsp: *mut *mut u64, new_rsp: *mut u64);
    fn cognacos_context_switch_publish(old_rsp:*mut *mut u64,new_rsp:*mut u64,publish:extern "C" fn(usize),context:usize);
    fn cognacos_context_switch_xstate(old_rsp:*mut *mut u64,new_rsp:*mut u64,xstate:*const crate::ntoskrnl::ke::runtime::KiThreadXStateArea);
    fn cognacos_context_switch_publish_xstate(old_rsp:*mut *mut u64,new_rsp:*mut u64,publish:extern "C" fn(usize),context:usize,xstate:*const crate::ntoskrnl::ke::runtime::KiThreadXStateArea);
}
unsafe extern "C" {
    fn cognacos_fault_probe_write_ro(address: *mut u8) -> u64;
    fn cognacos_fault_probe_write_ro_recovery();
    fn cognacos_fault_probe_execute_nx(address: *mut u8) -> u64;
    fn cognacos_fault_probe_execute_nx_recovery();
    fn cognacos_fault_probe_read_invalid(address: *const u64) -> u64;
    fn cognacos_fault_probe_read_invalid_recovery();
}


/// Verify that a non-present kernel probe is converted into an explicit error
/// result and returns through its fixup instead of escalating to the fatal handler.
pub fn trap_frame_fixup_self_test() -> bool {
    const INVALID_ADDRESS: u64 = 0x0000_7fff_dead_0000;
    let fault_rip = cognacos_fault_probe_read_invalid as *const () as u64;
    // Reuse the already exercised ret-only recovery stub; the configured RAX result survives it.
    let recovery_rip = cognacos_fault_probe_write_ro_recovery as *const () as u64;
    if !register_trap_frame_fixup_with_result(
        fault_rip, recovery_rip, INVALID_ADDRESS,
        PF_ERROR_MATCH_MASK, 0, STATUS_ACCESS_VIOLATION,
    ) {
        return false;
    }
    let result = unsafe { cognacos_fault_probe_read_invalid(INVALID_ADDRESS as *const u64) };
    let removed = unregister_trap_frame_fixup(fault_rip);
    removed && result == STATUS_ACCESS_VIOLATION
}

/// Deliberately verify CR0.WP and NX enforcement without hiding unrelated faults.
/// The recovery labels are armed for one exact CR2 address and one expected error code.
pub fn permission_fault_self_test(read_only_executable_address: u64) -> bool {
    let write_rip = cognacos_fault_probe_write_ro as *const () as u64;
    let write_recovery = cognacos_fault_probe_write_ro_recovery as *const () as u64;
    let original_byte = unsafe { core::ptr::read_volatile(read_only_executable_address as *const u8) };
    if !register_trap_frame_fixup_with_result(
        write_rip, write_recovery, read_only_executable_address, 0x1f, 0x3, 0,
    ) {
        return false;
    }
    let write_result = unsafe { cognacos_fault_probe_write_ro(read_only_executable_address as *mut u8) };
    let write_removed = unregister_trap_frame_fixup(write_rip);
    if write_result != 0 {
        // If protection is broken, restore the byte before reporting failure.
        unsafe { core::ptr::write_volatile(read_only_executable_address as *mut u8, original_byte); }
    }
    if write_result != 0 || !write_removed { return false; }

    let mut nx_code = [0u8; 16];
    unsafe { core::ptr::write_volatile(nx_code.as_mut_ptr(), 0xc3); }
    let target = nx_code.as_mut_ptr();
    let execute_recovery = cognacos_fault_probe_execute_nx_recovery as *const () as u64;
    // The faulting RIP is the NX target reached by JMP RDI, not the probe's
    // entry instruction. Match the exact instruction-fetch address.
    if !register_trap_frame_fixup_with_result(
        target as u64, execute_recovery, target as u64, 0x1f, 0x11, 0,
    ) {
        return false;
    }
    let execute_result = unsafe { cognacos_fault_probe_execute_nx(target) };
    let execute_removed = unregister_trap_frame_fixup(target as u64);
    execute_result == 0 && execute_removed
}

/// Switch between saved kernel-mode contexts built for this routine.
pub unsafe fn context_switch(old_rsp: *mut *mut u64, new_rsp: *mut u64) {
    if old_rsp.is_null() || new_rsp.is_null() { return; }
    unsafe { cognacos_context_switch(old_rsp, new_rsp); }
}
/// Switch stacks after saving the outgoing context, invoking a tiny publication
/// callback before another CPU can claim the outgoing thread.
pub unsafe fn context_switch_publish(
    old_rsp:*mut *mut u64,new_rsp:*mut u64,publish:extern "C" fn(usize),context:usize,
){
    if old_rsp.is_null()||new_rsp.is_null(){return}
    unsafe{cognacos_context_switch_publish(old_rsp,new_rsp,publish,context);}
}
/// Switch stacks and restore the incoming thread's x87/SSE state after any
/// outgoing-context publication callback has completed.
pub unsafe fn context_switch_xstate(old_rsp:*mut *mut u64,new_rsp:*mut u64,xstate:*const crate::ntoskrnl::ke::runtime::KiThreadXStateArea){
    if old_rsp.is_null()||new_rsp.is_null(){return}
    unsafe{cognacos_context_switch_xstate(old_rsp,new_rsp,xstate);}
}
pub unsafe fn context_switch_publish_xstate(
    old_rsp:*mut *mut u64,new_rsp:*mut u64,publish:extern "C" fn(usize),context:usize,
    xstate:*const crate::ntoskrnl::ke::runtime::KiThreadXStateArea,
){
    if old_rsp.is_null()||new_rsp.is_null(){return}
    unsafe{cognacos_context_switch_publish_xstate(old_rsp,new_rsp,publish,context,xstate);}
}

/// Generic HAL IPI handler. Publish pending work for the local scheduler/DPC
/// path before acknowledging the local APIC.
#[unsafe(no_mangle)]
pub extern "C" fn cognacos_hal_ipi_handler() {
    if let Some(cpu) = crate::hal::current_cpu_local() {
        let id = cpu.logical_id.load(core::sync::atomic::Ordering::Acquire) as usize;
        let _ = crate::hal::mark_ipi_pending(id);
    }
    unsafe { crate::hal::end_local_interrupt(); }
    // A reschedule IPI is also the remote DPC kick. Run the target CPU's
    // queued DPCs before returning so KeFlushQueuedDpcs can wait for a real
    // systemwide drain rather than merely observing a cleared queue bit.
    crate::ntoskrnl::ke::runtime::dispatch_queued_dpcs();
}

/// Dedicated cross-processor Ke broadcast handler. The callback rendezvous
/// owns the logical IRQL transition and all callback/completion bookkeeping.
#[unsafe(no_mangle)]
pub extern "C" fn cognacos_ke_broadcast_ipi_handler() {
    crate::ntoskrnl::ke::runtime::handle_broadcast_ipi();
    unsafe { crate::hal::end_local_interrupt(); }
}

/// Dedicated AP TSC synchronization IPI handler. It samples the raw TSC and
/// issues exactly one EOI; it does not reuse the TLB-shootdown vector.
#[unsafe(no_mangle)]
pub extern "C" fn cognacos_tsc_sync_handler() {
    crate::hal::capture_clock_synchronization_sample();
    if let Some(base) = crate::hal::local_interrupt_controller_base() {
        unsafe { crate::hal::end_local_interrupt(); }
    }
}

/// Rust dispatch target for the common external-interrupt entry stub.
/// The APIC dispatcher performs exactly one EOI after the registered callback.
#[unsafe(no_mangle)]
pub extern "C" fn cognacos_external_interrupt_dispatch(vector: u64, frame: *const u64) {
    let _ = unsafe {
        crate::hal::dispatch_platform_interrupt(
            vector as u8,
            frame as *mut core::ffi::c_void,
        )
    };
}

static TIMER_ISR_DIAGNOSTIC_ONCE: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
#[unsafe(no_mangle)]
pub extern "C" fn cognacos_timer_interrupt_handler() {
    let first = !TIMER_ISR_DIAGNOSTIC_ONCE.swap(true, core::sync::atomic::Ordering::AcqRel);
    if first { unsafe { crate::serial_write(b"APIC timer ISR entered.\r\n"); } }
    crate::hal::local_clock_event_interrupt();
    // Switching here saves the complete interrupt path on the interrupted
    // thread's kernel stack; IRETQ resumes that thread when it is scheduled again.
    crate::thread::preempt_from_timer_interrupt();
    if first { unsafe { crate::serial_write(b"APIC timer ISR EOI completed.\r\n"); } }
}
