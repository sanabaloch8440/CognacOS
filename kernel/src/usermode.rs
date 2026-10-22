//! User-mode process launch and return path.
//! The bootstrap uses a dedicated DPL3 software interrupt for process exit;
//! normal syscalls return through IRETQ. IST-based exception stacks remain separate.
use core::arch::{asm, global_asm};
use crate::{pe::LoadedImage, vm::{AddressSpace, VmError}};
const PAGE: u64 = 4096;
const USER_STACK: u64 = 0x0000_0082_0000_0000;
const CALLBACK_BASE: u64 = 0x0000_0083_0000_0000;

global_asm!(r#"
    .section .text
    .global rdotos_process_exit_stub
    .type rdotos_process_exit_stub, @function
rdotos_process_exit_stub:
    cli
    cld
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov ecx, 0xc0000101
    mov rax, qword ptr [rip + rdotos_process_saved_gs]
    mov rdx, rax
    shr rdx, 32
    wrmsr
    mov rsp, qword ptr [rip + rdotos_process_saved_rsp]
    jmp qword ptr [rip + rdotos_process_saved_rip]

    .global rdotos_process_enter_user
    .type rdotos_process_enter_user, @function
rdotos_process_enter_user:
    push rbx
    push rbp
    push r12
    push r13
    push r14
    push r15
    mov qword ptr [rip + rdotos_process_saved_rsp], rsp
    lea rax, [rip + .Lrdotos_process_return]
    mov qword ptr [rip + rdotos_process_saved_rip], rax
    mov ecx, 0xc0000101
    rdmsr
    shl rdx, 32
    or rax, rdx
    mov qword ptr [rip + rdotos_process_saved_gs], rax
    mov rax, rdx
    // The TEB is supplied in RDX; preserve it before RDMSR clobbers EDX.
    mov qword ptr [rip + rdotos_process_pending_teb], rdx
    mov rax, qword ptr [rip + rdotos_process_pending_teb]
    mov rdx, rax
    shr rdx, 32
    mov ecx, 0xc0000101
    wrmsr
    mov ax, 0x1b
    mov ds, ax
    mov es, ax
    push 0x1b
    push rsi
    push 0x2
    push 0x23
    push rdi
    iretq
.Lrdotos_process_return:
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbp
    pop rbx
    ret

    .section .bss
    .align 8
    .global rdotos_process_saved_rsp
rdotos_process_saved_rsp: .quad 0
    .global rdotos_process_saved_rip
rdotos_process_saved_rip: .quad 0
    .global rdotos_process_saved_gs
rdotos_process_saved_gs: .quad 0
    .global rdotos_process_pending_teb
rdotos_process_pending_teb: .quad 0
"#);

unsafe extern "C" {
    fn rdotos_process_exit_stub();
    fn rdotos_process_enter_user(entry: u64, stack_top: u64, teb: u64);
}

pub unsafe fn initialize() {
    unsafe { crate::arch::install_user_gate(0x82, rdotos_process_exit_stub as usize as u64); }
}

fn callback_trampoline(callback: u64, image_base: u64) -> [u8; 40] {
    // sub rsp,20h; mov rcx,image_base; mov edx,1; xor r8d,r8d;
    // mov rax,callback; call rax; add rsp,20h; int 82h; ud2.
    let mut b = [0x90u8; 40];
    b[..6].copy_from_slice(&[0x48,0x83,0xec,0x20,0x48,0xb9]);
    b[6..14].copy_from_slice(&image_base.to_le_bytes());
    b[14..19].copy_from_slice(&[0xba,1,0,0,0]);
    b[19..22].copy_from_slice(&[0x45,0x31,0xc0]);
    b[22..24].copy_from_slice(&[0x48,0xb8]);
    b[24..32].copy_from_slice(&callback.to_le_bytes());
    b[32..34].copy_from_slice(&[0xff,0xd0]);
    b[34..37].copy_from_slice(&[0x48,0x83,0xc4]);
    b[37]=0x20; b[38]=0xcd; b[39]=0x82;
    b
}

/// Start a loaded image with a minimal PEB/TEB, execute registered TLS callbacks
/// in user mode, then transfer to the image entry point. The synthetic smoke
/// image exits through SYS_EXIT to exercise the real dispatcher.
pub unsafe fn run_loaded_image(space: &mut AddressSpace, image: LoadedImage) -> Result<(), VmError> {
    let stack_page = unsafe { crate::memory::allocate_page() }.map_err(|_| VmError::OutOfMemory)?;
    unsafe { space.map_user_page(USER_STACK, stack_page, true, false)?; }
    let env = unsafe { crate::process::initialize(space, image.base, USER_STACK + PAGE, USER_STACK) }
        .map_err(|_| VmError::OutOfMemory)?;
    let mut callbacks = [0u64; 64];
    let callback_count = crate::pe_runtime::tls_callbacks(image.base, &mut callbacks)
        .map_err(|_| VmError::InvalidAddress)?;
    let kernel_cr3: u64;
    unsafe { asm!("mov {}, cr3", out(reg) kernel_cr3, options(nomem, nostack, preserves_flags)); }
    crate::syscall::set_current_process(env.process_id, space as *const _);
    unsafe { space.activate(); }
    for (i, callback) in callbacks[..callback_count].iter().copied().enumerate() {
        let va = CALLBACK_BASE + i as u64 * PAGE;
        let physical = unsafe { crate::memory::allocate_page() }.map_err(|_| VmError::OutOfMemory)?;
        unsafe { space.map_user_page(va, physical, true, false)?; }
        let code = callback_trampoline(callback, image.base);
        unsafe { space.copy_to_user(va, &code)?; }
        unsafe { space.protect_user_page(va, false, true)?; }
        unsafe { rdotos_process_enter_user(va, USER_STACK + PAGE, env.teb); }
    }
    unsafe { rdotos_process_enter_user(image.entry, USER_STACK + PAGE, env.teb); }
    unsafe { asm!("mov cr3, {}", in(reg) kernel_cr3, options(nostack, preserves_flags)); }
    crate::syscall::clear_current_process();
    if crate::syscall::dispatch_count() < 2 || crate::syscall::last_exit_code() != 0 {
        return Err(VmError::PermissionDenied);
    }
    Ok(())
}
