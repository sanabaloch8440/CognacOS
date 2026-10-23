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
    and rsp, -16
    call rdotos_exception_fatal
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
    mov r12, rsp
    and rsp, -16
    call rdotos_tlb_ipi_handler
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
    iretq

    .global rdotos_spurious_stub
    .type rdotos_spurious_stub, @function
rdotos_spurious_stub:
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
    fn rdotos_tlb_ipi_stub(); fn rdotos_spurious_stub();
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
    // 0xF1 is reserved for synchronous TLB shootdowns; 0xFF is the LAPIC
    // spurious vector and intentionally does not send an EOI.
    unsafe {
        idt_ptr.add(crate::apic::TLB_SHOOTDOWN_VECTOR as usize).write(IdtEntry::interrupt_gate(rdotos_tlb_ipi_stub as u64));
        idt_ptr.add(0xFF).write(IdtEntry::interrupt_gate(rdotos_spurious_stub as u64));
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
