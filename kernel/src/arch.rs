//! x86-64 GDT/IDT setup and exception-specific fatal diagnostics.
use core::arch::{asm, global_asm};
use core::mem::size_of;

const KERNEL_CODE_SELECTOR: u16 = 0x08;
const IDT_ENTRIES: usize = 256;
const EXCEPTION_VECTORS: usize = 32;

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
}
#[repr(C, packed)]
struct DescriptorTablePointer { limit: u16, base: u64 }
static mut IDT: [IdtEntry; IDT_ENTRIES] = [IdtEntry::MISSING; IDT_ENTRIES];

global_asm!(r#"
    .section .rodata
    .align 8
.Lrdotos_gdt:
    .quad 0
    .quad 0x00af9a000000ffff
    .quad 0x00cf92000000ffff
.Lrdotos_gdt_pointer:
    .word 23
    .quad .Lrdotos_gdt

    .section .text
    .global rdotos_load_gdt
    .type rdotos_load_gdt, @function
rdotos_load_gdt:
    lgdt [rip + .Lrdotos_gdt_pointer]
    push 0x08
    lea rax, [rip + .Lrdotos_reload_cs]
    push rax
    retfq
.Lrdotos_reload_cs:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    xor eax, eax
    mov fs, ax
    mov gs, ax
    ret

    .macro ISR_NOERR n
    .global rdotos_isr_\n
    .type rdotos_isr_\n, @function
rdotos_isr_\n:
    push 0
    push \n
    jmp rdotos_exception_common
    .endm

    .macro ISR_ERR n
    .global rdotos_isr_\n
    .type rdotos_isr_\n, @function
rdotos_isr_\n:
    push \n
    jmp rdotos_exception_common
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

    .global rdotos_exception_common
    .type rdotos_exception_common, @function
rdotos_exception_common:
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
    mov rdx, rsp
    mov rdi, [rsp + 120]
    mov rsi, [rsp + 128]
    mov r12, rsp
    and rsp, -16
    call rdotos_exception_dispatch
    mov rsp, r12
    test rax, rax
    jz .Lrdotos_unhandled_exception
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
.Lrdotos_unhandled_exception:
    ud2

    # Dedicated interrupt-gate entry: preserve all registers and return with IRETQ.
    .global rdotos_tlb_ipi_stub
    .type rdotos_tlb_ipi_stub, @function
rdotos_tlb_ipi_stub:
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
    mov rax, rsp
    and rsp, -16
    sub rsp, 16
    mov [rsp + 8], rax
    call rdotos_tlb_ipi_handler
    mov rsp, [rsp + 8]
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
    .global rdotos_irq_\n
    .type rdotos_irq_\n, @function
rdotos_irq_\n:
    push 0
    push \n
    jmp rdotos_interrupt_common
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

    .global rdotos_interrupt_common
    .type rdotos_interrupt_common, @function
rdotos_interrupt_common:
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
    mov rdi, [rsp + 120]
    mov rsi, rsp
    mov r12, rsp
    and rsp, -16
    call rdotos_external_interrupt_dispatch
    mov rsp, r12
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

    .global rdotos_tsc_sync_stub
    .type rdotos_tsc_sync_stub, @function
rdotos_tsc_sync_stub:
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
    mov rax, rsp
    and rsp, -16
    sub rsp, 16
    mov [rsp + 8], rax
    call rdotos_tsc_sync_handler
    mov rsp, [rsp + 8]
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

    .global rdotos_spurious_stub
    .type rdotos_spurious_stub, @function
rdotos_spurious_stub:
    iretq

    .global rdotos_timer_stub
    .type rdotos_timer_stub, @function
rdotos_timer_stub:
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
    mov rax, rsp
    and rsp, -16
    sub rsp, 16
    mov [rsp + 8], rax
    call rdotos_timer_interrupt_handler
    mov rsp, [rsp + 8]
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
    fn rdotos_load_gdt();
    fn rdotos_isr_0(); fn rdotos_isr_1(); fn rdotos_isr_2(); fn rdotos_isr_3();
    fn rdotos_isr_4(); fn rdotos_isr_5(); fn rdotos_isr_6(); fn rdotos_isr_7();
    fn rdotos_isr_8(); fn rdotos_isr_9(); fn rdotos_isr_10(); fn rdotos_isr_11();
    fn rdotos_isr_12(); fn rdotos_isr_13(); fn rdotos_isr_14(); fn rdotos_isr_15();
    fn rdotos_isr_16(); fn rdotos_isr_17(); fn rdotos_isr_18(); fn rdotos_isr_19();
    fn rdotos_isr_20(); fn rdotos_isr_21(); fn rdotos_isr_22(); fn rdotos_isr_23();
    fn rdotos_isr_24(); fn rdotos_isr_25(); fn rdotos_isr_26(); fn rdotos_isr_27();
    fn rdotos_isr_28(); fn rdotos_isr_29(); fn rdotos_isr_30(); fn rdotos_isr_31();
    fn rdotos_tlb_ipi_stub(); fn rdotos_tsc_sync_stub(); fn rdotos_spurious_stub(); fn rdotos_timer_stub();
    fn rdotos_irq_32();
    fn rdotos_irq_33();
    fn rdotos_irq_34();
    fn rdotos_irq_35();
    fn rdotos_irq_36();
    fn rdotos_irq_37();
    fn rdotos_irq_38();
    fn rdotos_irq_39();
    fn rdotos_irq_40();
    fn rdotos_irq_41();
    fn rdotos_irq_42();
    fn rdotos_irq_43();
    fn rdotos_irq_44();
    fn rdotos_irq_45();
    fn rdotos_irq_46();
    fn rdotos_irq_47();
    fn rdotos_irq_48();
    fn rdotos_irq_49();
    fn rdotos_irq_50();
    fn rdotos_irq_51();
    fn rdotos_irq_52();
    fn rdotos_irq_53();
    fn rdotos_irq_54();
    fn rdotos_irq_55();
    fn rdotos_irq_56();
    fn rdotos_irq_57();
    fn rdotos_irq_58();
    fn rdotos_irq_59();
    fn rdotos_irq_60();
    fn rdotos_irq_61();
    fn rdotos_irq_62();
    fn rdotos_irq_63();
    fn rdotos_irq_64();
    fn rdotos_irq_65();
    fn rdotos_irq_66();
    fn rdotos_irq_67();
    fn rdotos_irq_68();
    fn rdotos_irq_69();
    fn rdotos_irq_70();
    fn rdotos_irq_71();
    fn rdotos_irq_72();
    fn rdotos_irq_73();
    fn rdotos_irq_74();
    fn rdotos_irq_75();
    fn rdotos_irq_76();
    fn rdotos_irq_77();
    fn rdotos_irq_78();
    fn rdotos_irq_79();
    fn rdotos_irq_80();
    fn rdotos_irq_81();
    fn rdotos_irq_82();
    fn rdotos_irq_83();
    fn rdotos_irq_84();
    fn rdotos_irq_85();
    fn rdotos_irq_86();
    fn rdotos_irq_87();
    fn rdotos_irq_88();
    fn rdotos_irq_89();
    fn rdotos_irq_90();
    fn rdotos_irq_91();
    fn rdotos_irq_92();
    fn rdotos_irq_93();
    fn rdotos_irq_94();
    fn rdotos_irq_95();
    fn rdotos_irq_96();
    fn rdotos_irq_97();
    fn rdotos_irq_98();
    fn rdotos_irq_99();
    fn rdotos_irq_100();
    fn rdotos_irq_101();
    fn rdotos_irq_102();
    fn rdotos_irq_103();
    fn rdotos_irq_104();
    fn rdotos_irq_105();
    fn rdotos_irq_106();
    fn rdotos_irq_107();
    fn rdotos_irq_108();
    fn rdotos_irq_109();
    fn rdotos_irq_110();
    fn rdotos_irq_111();
    fn rdotos_irq_112();
    fn rdotos_irq_113();
    fn rdotos_irq_114();
    fn rdotos_irq_115();
    fn rdotos_irq_116();
    fn rdotos_irq_117();
    fn rdotos_irq_118();
    fn rdotos_irq_119();
    fn rdotos_irq_120();
    fn rdotos_irq_121();
    fn rdotos_irq_122();
    fn rdotos_irq_123();
    fn rdotos_irq_124();
    fn rdotos_irq_125();
    fn rdotos_irq_126();
    fn rdotos_irq_127();
    fn rdotos_irq_128();
    fn rdotos_irq_129();
    fn rdotos_irq_130();
    fn rdotos_irq_131();
    fn rdotos_irq_132();
    fn rdotos_irq_133();
    fn rdotos_irq_134();
    fn rdotos_irq_135();
    fn rdotos_irq_136();
    fn rdotos_irq_137();
    fn rdotos_irq_138();
    fn rdotos_irq_139();
    fn rdotos_irq_140();
    fn rdotos_irq_141();
    fn rdotos_irq_142();
    fn rdotos_irq_143();
    fn rdotos_irq_144();
    fn rdotos_irq_145();
    fn rdotos_irq_146();
    fn rdotos_irq_147();
    fn rdotos_irq_148();
    fn rdotos_irq_149();
    fn rdotos_irq_150();
    fn rdotos_irq_151();
    fn rdotos_irq_152();
    fn rdotos_irq_153();
    fn rdotos_irq_154();
    fn rdotos_irq_155();
    fn rdotos_irq_156();
    fn rdotos_irq_157();
    fn rdotos_irq_158();
    fn rdotos_irq_159();
    fn rdotos_irq_160();
    fn rdotos_irq_161();
    fn rdotos_irq_162();
    fn rdotos_irq_163();
    fn rdotos_irq_164();
    fn rdotos_irq_165();
    fn rdotos_irq_166();
    fn rdotos_irq_167();
    fn rdotos_irq_168();
    fn rdotos_irq_169();
    fn rdotos_irq_170();
    fn rdotos_irq_171();
    fn rdotos_irq_172();
    fn rdotos_irq_173();
    fn rdotos_irq_174();
    fn rdotos_irq_175();
    fn rdotos_irq_176();
    fn rdotos_irq_177();
    fn rdotos_irq_178();
    fn rdotos_irq_179();
    fn rdotos_irq_180();
    fn rdotos_irq_181();
    fn rdotos_irq_182();
    fn rdotos_irq_183();
    fn rdotos_irq_184();
    fn rdotos_irq_185();
    fn rdotos_irq_186();
    fn rdotos_irq_187();
    fn rdotos_irq_188();
    fn rdotos_irq_189();
    fn rdotos_irq_190();
    fn rdotos_irq_191();
    fn rdotos_irq_192();
    fn rdotos_irq_193();
    fn rdotos_irq_194();
    fn rdotos_irq_195();
    fn rdotos_irq_196();
    fn rdotos_irq_197();
    fn rdotos_irq_198();
    fn rdotos_irq_199();
    fn rdotos_irq_200();
    fn rdotos_irq_201();
    fn rdotos_irq_202();
    fn rdotos_irq_203();
    fn rdotos_irq_204();
    fn rdotos_irq_205();
    fn rdotos_irq_206();
    fn rdotos_irq_207();
    fn rdotos_irq_208();
    fn rdotos_irq_209();
    fn rdotos_irq_210();
    fn rdotos_irq_211();
    fn rdotos_irq_212();
    fn rdotos_irq_213();
    fn rdotos_irq_214();
    fn rdotos_irq_215();
    fn rdotos_irq_216();
    fn rdotos_irq_217();
    fn rdotos_irq_218();
    fn rdotos_irq_219();
    fn rdotos_irq_220();
    fn rdotos_irq_221();
    fn rdotos_irq_222();
    fn rdotos_irq_223();
}

/// Reload the already initialized shared kernel GDT and IDT on a secondary CPU.
/// This does not mutate the global IDT and is safe to call concurrently on APs.
pub unsafe fn load_cpu_descriptor_tables() {
    unsafe { rdotos_load_gdt(); }
    let idt_ptr = core::ptr::addr_of!(IDT).cast::<IdtEntry>();
    let pointer = DescriptorTablePointer {
        limit: (IDT_ENTRIES * size_of::<IdtEntry>() - 1) as u16,
        base: idt_ptr as u64,
    };
    unsafe { asm!("lidt [{}]", in(reg) &pointer, options(readonly, nostack, preserves_flags)); }
}

pub unsafe fn initialize() {
    unsafe { rdotos_load_gdt() };
    let handlers: [usize; EXCEPTION_VECTORS] = [
        rdotos_isr_0 as usize, rdotos_isr_1 as usize, rdotos_isr_2 as usize, rdotos_isr_3 as usize,
        rdotos_isr_4 as usize, rdotos_isr_5 as usize, rdotos_isr_6 as usize, rdotos_isr_7 as usize,
        rdotos_isr_8 as usize, rdotos_isr_9 as usize, rdotos_isr_10 as usize, rdotos_isr_11 as usize,
        rdotos_isr_12 as usize, rdotos_isr_13 as usize, rdotos_isr_14 as usize, rdotos_isr_15 as usize,
        rdotos_isr_16 as usize, rdotos_isr_17 as usize, rdotos_isr_18 as usize, rdotos_isr_19 as usize,
        rdotos_isr_20 as usize, rdotos_isr_21 as usize, rdotos_isr_22 as usize, rdotos_isr_23 as usize,
        rdotos_isr_24 as usize, rdotos_isr_25 as usize, rdotos_isr_26 as usize, rdotos_isr_27 as usize,
        rdotos_isr_28 as usize, rdotos_isr_29 as usize, rdotos_isr_30 as usize, rdotos_isr_31 as usize,
    ];
    let idt_ptr = core::ptr::addr_of_mut!(IDT).cast::<IdtEntry>();
    for vector in 0..EXCEPTION_VECTORS {
        unsafe { idt_ptr.add(vector).write(IdtEntry::interrupt_gate(handlers[vector] as u64)); }
    }
    let irq_handlers: [usize; 192] = [
        rdotos_irq_32 as usize,
        rdotos_irq_33 as usize,
        rdotos_irq_34 as usize,
        rdotos_irq_35 as usize,
        rdotos_irq_36 as usize,
        rdotos_irq_37 as usize,
        rdotos_irq_38 as usize,
        rdotos_irq_39 as usize,
        rdotos_irq_40 as usize,
        rdotos_irq_41 as usize,
        rdotos_irq_42 as usize,
        rdotos_irq_43 as usize,
        rdotos_irq_44 as usize,
        rdotos_irq_45 as usize,
        rdotos_irq_46 as usize,
        rdotos_irq_47 as usize,
        rdotos_irq_48 as usize,
        rdotos_irq_49 as usize,
        rdotos_irq_50 as usize,
        rdotos_irq_51 as usize,
        rdotos_irq_52 as usize,
        rdotos_irq_53 as usize,
        rdotos_irq_54 as usize,
        rdotos_irq_55 as usize,
        rdotos_irq_56 as usize,
        rdotos_irq_57 as usize,
        rdotos_irq_58 as usize,
        rdotos_irq_59 as usize,
        rdotos_irq_60 as usize,
        rdotos_irq_61 as usize,
        rdotos_irq_62 as usize,
        rdotos_irq_63 as usize,
        rdotos_irq_64 as usize,
        rdotos_irq_65 as usize,
        rdotos_irq_66 as usize,
        rdotos_irq_67 as usize,
        rdotos_irq_68 as usize,
        rdotos_irq_69 as usize,
        rdotos_irq_70 as usize,
        rdotos_irq_71 as usize,
        rdotos_irq_72 as usize,
        rdotos_irq_73 as usize,
        rdotos_irq_74 as usize,
        rdotos_irq_75 as usize,
        rdotos_irq_76 as usize,
        rdotos_irq_77 as usize,
        rdotos_irq_78 as usize,
        rdotos_irq_79 as usize,
        rdotos_irq_80 as usize,
        rdotos_irq_81 as usize,
        rdotos_irq_82 as usize,
        rdotos_irq_83 as usize,
        rdotos_irq_84 as usize,
        rdotos_irq_85 as usize,
        rdotos_irq_86 as usize,
        rdotos_irq_87 as usize,
        rdotos_irq_88 as usize,
        rdotos_irq_89 as usize,
        rdotos_irq_90 as usize,
        rdotos_irq_91 as usize,
        rdotos_irq_92 as usize,
        rdotos_irq_93 as usize,
        rdotos_irq_94 as usize,
        rdotos_irq_95 as usize,
        rdotos_irq_96 as usize,
        rdotos_irq_97 as usize,
        rdotos_irq_98 as usize,
        rdotos_irq_99 as usize,
        rdotos_irq_100 as usize,
        rdotos_irq_101 as usize,
        rdotos_irq_102 as usize,
        rdotos_irq_103 as usize,
        rdotos_irq_104 as usize,
        rdotos_irq_105 as usize,
        rdotos_irq_106 as usize,
        rdotos_irq_107 as usize,
        rdotos_irq_108 as usize,
        rdotos_irq_109 as usize,
        rdotos_irq_110 as usize,
        rdotos_irq_111 as usize,
        rdotos_irq_112 as usize,
        rdotos_irq_113 as usize,
        rdotos_irq_114 as usize,
        rdotos_irq_115 as usize,
        rdotos_irq_116 as usize,
        rdotos_irq_117 as usize,
        rdotos_irq_118 as usize,
        rdotos_irq_119 as usize,
        rdotos_irq_120 as usize,
        rdotos_irq_121 as usize,
        rdotos_irq_122 as usize,
        rdotos_irq_123 as usize,
        rdotos_irq_124 as usize,
        rdotos_irq_125 as usize,
        rdotos_irq_126 as usize,
        rdotos_irq_127 as usize,
        rdotos_irq_128 as usize,
        rdotos_irq_129 as usize,
        rdotos_irq_130 as usize,
        rdotos_irq_131 as usize,
        rdotos_irq_132 as usize,
        rdotos_irq_133 as usize,
        rdotos_irq_134 as usize,
        rdotos_irq_135 as usize,
        rdotos_irq_136 as usize,
        rdotos_irq_137 as usize,
        rdotos_irq_138 as usize,
        rdotos_irq_139 as usize,
        rdotos_irq_140 as usize,
        rdotos_irq_141 as usize,
        rdotos_irq_142 as usize,
        rdotos_irq_143 as usize,
        rdotos_irq_144 as usize,
        rdotos_irq_145 as usize,
        rdotos_irq_146 as usize,
        rdotos_irq_147 as usize,
        rdotos_irq_148 as usize,
        rdotos_irq_149 as usize,
        rdotos_irq_150 as usize,
        rdotos_irq_151 as usize,
        rdotos_irq_152 as usize,
        rdotos_irq_153 as usize,
        rdotos_irq_154 as usize,
        rdotos_irq_155 as usize,
        rdotos_irq_156 as usize,
        rdotos_irq_157 as usize,
        rdotos_irq_158 as usize,
        rdotos_irq_159 as usize,
        rdotos_irq_160 as usize,
        rdotos_irq_161 as usize,
        rdotos_irq_162 as usize,
        rdotos_irq_163 as usize,
        rdotos_irq_164 as usize,
        rdotos_irq_165 as usize,
        rdotos_irq_166 as usize,
        rdotos_irq_167 as usize,
        rdotos_irq_168 as usize,
        rdotos_irq_169 as usize,
        rdotos_irq_170 as usize,
        rdotos_irq_171 as usize,
        rdotos_irq_172 as usize,
        rdotos_irq_173 as usize,
        rdotos_irq_174 as usize,
        rdotos_irq_175 as usize,
        rdotos_irq_176 as usize,
        rdotos_irq_177 as usize,
        rdotos_irq_178 as usize,
        rdotos_irq_179 as usize,
        rdotos_irq_180 as usize,
        rdotos_irq_181 as usize,
        rdotos_irq_182 as usize,
        rdotos_irq_183 as usize,
        rdotos_irq_184 as usize,
        rdotos_irq_185 as usize,
        rdotos_irq_186 as usize,
        rdotos_irq_187 as usize,
        rdotos_irq_188 as usize,
        rdotos_irq_189 as usize,
        rdotos_irq_190 as usize,
        rdotos_irq_191 as usize,
        rdotos_irq_192 as usize,
        rdotos_irq_193 as usize,
        rdotos_irq_194 as usize,
        rdotos_irq_195 as usize,
        rdotos_irq_196 as usize,
        rdotos_irq_197 as usize,
        rdotos_irq_198 as usize,
        rdotos_irq_199 as usize,
        rdotos_irq_200 as usize,
        rdotos_irq_201 as usize,
        rdotos_irq_202 as usize,
        rdotos_irq_203 as usize,
        rdotos_irq_204 as usize,
        rdotos_irq_205 as usize,
        rdotos_irq_206 as usize,
        rdotos_irq_207 as usize,
        rdotos_irq_208 as usize,
        rdotos_irq_209 as usize,
        rdotos_irq_210 as usize,
        rdotos_irq_211 as usize,
        rdotos_irq_212 as usize,
        rdotos_irq_213 as usize,
        rdotos_irq_214 as usize,
        rdotos_irq_215 as usize,
        rdotos_irq_216 as usize,
        rdotos_irq_217 as usize,
        rdotos_irq_218 as usize,
        rdotos_irq_219 as usize,
        rdotos_irq_220 as usize,
        rdotos_irq_221 as usize,
        rdotos_irq_222 as usize,
        rdotos_irq_223 as usize,
    ];
    for (index, handler) in irq_handlers.iter().enumerate() {
        unsafe { idt_ptr.add(0x20 + index).write(IdtEntry::interrupt_gate(*handler as u64)); }
    }
    // 0xF1 is reserved for synchronous TLB shootdowns; 0xFF is the LAPIC
    // spurious vector and intentionally does not send an EOI.
    unsafe {
        idt_ptr.add(crate::apic::TLB_SHOOTDOWN_VECTOR as usize).write(IdtEntry::interrupt_gate(rdotos_tlb_ipi_stub as u64));
        idt_ptr.add(crate::apic::TSC_SYNC_VECTOR as usize).write(IdtEntry::interrupt_gate(rdotos_tsc_sync_stub as u64));
        idt_ptr.add(0xFF).write(IdtEntry::interrupt_gate(rdotos_spurious_stub as u64));
        idt_ptr.add(crate::apic::LOCAL_TIMER_VECTOR as usize).write(IdtEntry::interrupt_gate(rdotos_timer_stub as u64));
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
pub extern "C" fn rdotos_exception_fatal(vector: u64, error: u64, frame: *const u64) -> ! {
    unsafe {
        crate::serial_write(b"\r\nRdotOS FATAL CPU exception vector=");
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

/// Dispatch recoverable faults. A return value of one asks the assembly stub to
/// restore the original frame and IRETQ; fatal/unrecognized exceptions halt.
#[unsafe(no_mangle)]
pub extern "C" fn rdotos_exception_dispatch(vector: u64, error: u64, frame: *const u64) -> u64 {
    if vector == 14 {
        let cr2: u64;
        unsafe { core::arch::asm!("mov {}, cr2", out(reg) cr2, options(nomem, nostack, preserves_flags)); }
        if crate::kernel_vm::handle_page_fault(cr2, error) { return 1; }
    }
    rdotos_exception_fatal(vector, error, frame)
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
    .global rdotos_context_switch
    .type rdotos_context_switch, @function
rdotos_context_switch:
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
"#);

unsafe extern "C" {
    fn rdotos_context_switch(old_rsp: *mut *mut u64, new_rsp: *mut u64);
}

/// Switch between saved kernel-mode contexts built for this routine.
pub unsafe fn context_switch(old_rsp: *mut *mut u64, new_rsp: *mut u64) {
    if old_rsp.is_null() || new_rsp.is_null() { return; }
    unsafe { rdotos_context_switch(old_rsp, new_rsp); }
}

/// Dedicated AP TSC synchronization IPI handler. It samples the raw TSC and
/// issues exactly one EOI; it does not reuse the TLB-shootdown vector.
#[unsafe(no_mangle)]
pub extern "C" fn rdotos_tsc_sync_handler() {
    crate::apic::capture_tsc_sync_sample();
    if let Some(base) = crate::apic::local_apic_base() {
        unsafe { crate::apic::local_apic_eoi(base); }
    }
}

/// Rust dispatch target for the common external-interrupt entry stub.
/// The APIC dispatcher performs exactly one EOI after the registered callback.
#[unsafe(no_mangle)]
pub extern "C" fn rdotos_external_interrupt_dispatch(vector: u64, frame: *const u64) {
    let _ = unsafe {
        crate::apic::dispatch_external_interrupt(
            vector as u8,
            frame as *mut core::ffi::c_void,
        )
    };
}

static TIMER_ISR_DIAGNOSTIC_ONCE: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
#[unsafe(no_mangle)]
pub extern "C" fn rdotos_timer_interrupt_handler() {
    let first = !TIMER_ISR_DIAGNOSTIC_ONCE.swap(true, core::sync::atomic::Ordering::AcqRel);
    if first { unsafe { crate::serial_write(b"APIC timer ISR entered.\\r\\n"); } }
    crate::apic::timer_interrupt();
    if first { unsafe { crate::serial_write(b"APIC timer ISR EOI completed.\\r\\n"); } }
}
