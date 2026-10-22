//! Early x86-64 descriptor tables and fatal exception path.
//! This stage intentionally keeps interrupts disabled: APIC/PIC routing and
//! interrupt-safe scheduling are not ready yet.

use core::arch::{asm, global_asm};
use core::mem::size_of;

const KERNEL_CODE_SELECTOR: u16 = 0x08;
const IDT_ENTRIES: usize = 256;
const EXCEPTION_VECTORS: usize = 32;

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    attributes: u8,
    offset_middle: u16,
    offset_high: u32,
    reserved: u32,
}

impl IdtEntry {
    const MISSING: Self = Self {
        offset_low: 0, selector: 0, ist: 0, attributes: 0,
        offset_middle: 0, offset_high: 0, reserved: 0,
    };

    fn interrupt_gate(handler: u64) -> Self {
        Self {
            offset_low: handler as u16,
            selector: KERNEL_CODE_SELECTOR,
            ist: 0,
            attributes: 0x8E, // present, ring 0, 64-bit interrupt gate
            offset_middle: (handler >> 16) as u16,
            offset_high: (handler >> 32) as u32,
            reserved: 0,
        }
    }
}

#[repr(C, packed)]
struct DescriptorTablePointer {
    limit: u16,
    base: u64,
}

static mut IDT: [IdtEntry; IDT_ENTRIES] = [IdtEntry::MISSING; IDT_ENTRIES];

global_asm!(
    r#"
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

    .global rdotos_exception_entry
    .type rdotos_exception_entry, @function
rdotos_exception_entry:
    cli
    cld
    and rsp, -16
    call rdotos_exception_fatal
    ud2
"#
);

unsafe extern "C" {
    fn rdotos_load_gdt();
    fn rdotos_exception_entry();
}

/// Install a private flat GDT and exception IDT. Hardware interrupts remain
/// disabled until a later stage configures the interrupt controller and timer.
pub unsafe fn initialize() {
    // SAFETY: The GDT is static kernel image data and its selector layout is
    // defined alongside the assembly that loads it.
    unsafe { rdotos_load_gdt() };

    let handler = rdotos_exception_entry as usize as u64;
    let idt_ptr = core::ptr::addr_of_mut!(IDT).cast::<IdtEntry>();
    for vector in 0..EXCEPTION_VECTORS {
        // SAFETY: vectors are bounded by the fixed static IDT array. Entries
        // are written before LIDT and exceptions cannot run concurrently (CLI).
        unsafe { idt_ptr.add(vector).write(IdtEntry::interrupt_gate(handler)) };
    }

    let pointer = DescriptorTablePointer {
        limit: (EXCEPTION_VECTORS * size_of::<IdtEntry>() - 1) as u16,
        base: idt_ptr as u64,
    };
    // SAFETY: pointer references the static IDT and remains valid indefinitely.
    unsafe { asm!("lidt [{}]", in(reg) &pointer, options(readonly, nostack, preserves_flags)) };
}

/// Called only by the assembly exception entry. The original CPU exception
/// frame is deliberately not returned to: until full diagnostics and recovery
/// exist, all CPU exceptions stop the machine deterministically.
#[unsafe(no_mangle)]
pub extern "C" fn rdotos_exception_fatal() -> ! {
    unsafe {
        crate::serial_write(b"\r\nRdotOS FATAL: CPU exception; system halted.\r\n");
        crate::halt()
    }
}
