//! x86-64 user-mode transition support. This installs ring-3 descriptors and a
//! minimal TSS RSP0 stack. IST stacks and recoverable exception paths remain future work.
use core::arch::asm;

const KERNEL_CODE: u16 = 0x08;
const KERNEL_DATA: u16 = 0x10;
const USER_DATA: u16 = 0x1b; // GDT index 3, RPL 3
const USER_CODE: u16 = 0x23; // GDT index 4, RPL 3
const TSS_SELECTOR: u16 = 0x28;
const PAGE_SIZE: u64 = 4096;
const USER_START: u64 = 0x1_0000_0000;
const USER_END: u64 = 0x0000_8000_0000_0000;

#[repr(C, packed)]
struct TaskStateSegment {
    reserved0: u32,
    rsp0: u64,
    rsp1: u64,
    rsp2: u64,
    reserved1: u64,
    ist1: u64,
    ist2: u64,
    ist3: u64,
    ist4: u64,
    ist5: u64,
    ist6: u64,
    ist7: u64,
    reserved2: u64,
    reserved3: u16,
    iomap_base: u16,
}
impl TaskStateSegment {
    const fn new() -> Self {
        Self { reserved0: 0, rsp0: 0, rsp1: 0, rsp2: 0, reserved1: 0,
            ist1: 0, ist2: 0, ist3: 0, ist4: 0, ist5: 0, ist6: 0, ist7: 0,
            reserved2: 0, reserved3: 0, iomap_base: core::mem::size_of::<Self>() as u16 }
    }
}

#[repr(C, packed)]
struct DescriptorTablePointer { limit: u16, base: u64 }

static mut GDT: [u64; 7] = [0; 7];
static mut TSS: TaskStateSegment = TaskStateSegment::new();

fn tss_descriptor(base: u64, limit: u32) -> (u64, u64) {
    let low = (limit as u64 & 0xffff)
        | ((base & 0x00ff_ffff) << 16)
        | (0x89u64 << 40)
        | (((limit as u64 >> 16) & 0x0f) << 48)
        | (((base >> 24) & 0xff) << 56);
    (low, base >> 32)
}

/// Install user code/data segments and load a 64-bit TSS with a protected RSP0.
/// The stack is deliberately placed below the currently active kernel stack
/// frames so a ring-3 interrupt cannot overwrite the caller's live frames.
///
/// # Safety
/// Must run once during single-core initialization, before any ring-3 transition.
pub unsafe fn initialize() {
    let mut rsp: u64;
    unsafe { asm!("mov {}, rsp", out(reg) rsp, options(nomem, nostack, preserves_flags)); }
    // Reserve a separate 16 KiB slice below the live startup frames for ring-0
    // entry. TSS/IST exception stacks will be added in the next hardening stage.
    let rsp0 = rsp.saturating_sub(16 * 1024) & !0xf;
    let tss_ptr = core::ptr::addr_of_mut!(TSS);
    unsafe { core::ptr::addr_of_mut!((*tss_ptr).rsp0).write_unaligned(rsp0); }
    unsafe { core::ptr::addr_of_mut!((*tss_ptr).iomap_base).write_unaligned(core::mem::size_of::<TaskStateSegment>() as u16); }

    let base = tss_ptr as u64;
    let (tss_low, tss_high) = tss_descriptor(base, (core::mem::size_of::<TaskStateSegment>() - 1) as u32);
    let gdt = core::ptr::addr_of_mut!(GDT);
    unsafe {
        (*gdt)[0] = 0;
        (*gdt)[1] = 0x00af_9a00_0000_ffff; // ring-0 64-bit code
        (*gdt)[2] = 0x00cf_9200_0000_ffff; // ring-0 data
        (*gdt)[3] = 0x00cf_f200_0000_ffff; // ring-3 data
        (*gdt)[4] = 0x00af_fa00_0000_ffff; // ring-3 64-bit code
        (*gdt)[5] = tss_low;
        (*gdt)[6] = tss_high;
    }
    let pointer = DescriptorTablePointer {
        limit: (7 * core::mem::size_of::<u64>() - 1) as u16,
        base: gdt as u64,
    };
    unsafe {
        asm!(
            "lgdt [{gdtr}]",
            "push {kcs}",
            "lea rax, [rip + 2f]",
            "push rax",
            "retfq",
            "2:",
            "mov ax, {kds}",
            "mov ds, ax",
            "mov es, ax",
            "mov ss, ax",
            "xor eax, eax",
            "mov fs, ax",
            "mov gs, ax",
            "mov ax, {tss}",
            "ltr ax",
            gdtr = in(reg) &pointer,
            kcs = const KERNEL_CODE,
            kds = const KERNEL_DATA,
            tss = const TSS_SELECTOR,
            out("rax") _,
            options(preserves_flags),
        );
    }
}

/// Transfer to a 64-bit ring-3 entry point with IRETQ. This is intentionally
/// one-way until process/thread return and syscall dispatch are implemented.
///
/// # Safety
/// entry must be mapped user-executable; stack_top must point into a mapped,
/// writable user stack with at least 16 bytes available below it.
pub unsafe fn enter(entry: u64, stack_top: u64) -> ! {
    if entry < USER_START || entry >= USER_END || stack_top <= USER_START || stack_top >= USER_END
        || entry & 0x8000_0000_0000_0000 != 0 || stack_top & (PAGE_SIZE - 1) == 0
    {
        crate::halt();
    }
    unsafe {
        asm!(
            "cli",
            "cld",
            "push {ss}",
            "push rsi",
            "push 0x202",
            "push {cs}",
            "push rdi",
            "iretq",
            ss = const USER_DATA,
            cs = const USER_CODE,
            in("rdi") entry,
            in("rsi") stack_top,
            options(noreturn),
        );
    }
}
