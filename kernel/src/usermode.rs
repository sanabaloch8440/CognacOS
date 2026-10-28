//! User-mode process launch and return path.
//! The bootstrap uses a dedicated DPL3 software interrupt for process exit;
//! normal syscalls return through IRETQ. IST-based exception stacks remain separate.
use core::arch::{asm, global_asm};
use crate::{pe::LoadedImage, ntoskrnl::mm::address_space::{AddressSpace, VmError}};
const PAGE: u64 = 4096;
const USER_STACK: u64 = 0x0000_0082_0000_0000;
// Bounded stack for real NTDLL routines and nested TLS callbacks; one guard page remains below it.
const USER_STACK_PAGES: u64 = 16;
const USER_STACK_TOP: u64 = USER_STACK + USER_STACK_PAGES * PAGE;
const CALLBACK_BASE: u64 = 0x0000_0083_0000_0000;
const TLS_DATA_BASE: u64 = 0x0000_0080_8000_0000;
const TLS_STRIDE: u64 = 0x0100_0000;

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
    mov qword ptr [rip + rdotos_process_pending_teb], rdx
    mov ecx, 0xc0000101
    rdmsr
    shl rdx, 32
    or rax, rdx
    mov qword ptr [rip + rdotos_process_saved_gs], rax
    // Load the saved user TEB into IA32_GS_BASE.
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
    # Initialize the user FP/SSE state from its per-thread (or bootstrap)
    # area immediately before entering ring 3.
    mov rbx, rsp
    and rsp, -16
    call cognacos_reset_current_user_xstate
    mov rsp, rbx
    and rsp, -16
    call cognacos_current_user_xstate_ptr
    mov rsp, rbx
    test rax, rax
    jz .Lrdotos_process_no_xstate
    mov r10, rax
    mov eax, 3
    xor edx, edx
    xrstor64 [r10]
.Lrdotos_process_no_xstate:
    # Do not leak kernel stack pointers or callee-saved register contents to
    # the initial user context. The IRET frame already holds user RIP/RSP.
    xor eax, eax
    xor ebx, ebx
    xor ecx, ecx
    xor edx, edx
    xor esi, esi
    xor edi, edi
    xor ebp, ebp
    xor r8d, r8d
    xor r9d, r9d
    xor r10d, r10d
    xor r11d, r11d
    xor r12d, r12d
    xor r13d, r13d
    xor r14d, r14d
    xor r15d, r15d
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
    unsafe { crate::ntoskrnl::ke::arch::install_user_gate(0x83, rdotos_process_exit_stub as usize as u64); }
}

fn initialize_tls_block(
    space:&mut AddressSpace,
    layout:crate::pe_runtime::TlsLayout,
    index:u32,
    tls_vector:u64,
)->Result<(),VmError>{
    if !layout.present{return Ok(());}
    let total=(layout.template_size as u64).checked_add(layout.zero_fill as u64).ok_or(VmError::InvalidAddress)?;
    if total>8*1024*1024{return Err(VmError::InvalidAddress);}
    let allocation=core::cmp::max(total,1);
    let base=TLS_DATA_BASE.checked_add(index as u64*TLS_STRIDE).ok_or(VmError::InvalidAddress)?;
    let pages=((allocation+PAGE-1)/PAGE) as usize;
    for i in 0..pages{
        let physical=unsafe{crate::memory::allocate_page()}.map_err(|_|VmError::OutOfMemory)?;
        unsafe{space.map_user_page(base+i as u64*PAGE,physical,true,false)?;}
    }
    let mut copied=0u64;let mut buffer=[0u8;4096];
    while copied<layout.template_size as u64{
        let count=core::cmp::min(buffer.len() as u64,layout.template_size as u64-copied) as usize;
        unsafe{space.copy_from_user(layout.template_start+copied,&mut buffer[..count])?;}
        unsafe{space.copy_to_user(base+copied,&buffer[..count])?;}
        copied+=count as u64;
    }
    if layout.index_address!=0{
        unsafe{space.copy_to_user(layout.index_address,&index.to_le_bytes())?;}
        let mut check=[0u8;4];unsafe{space.copy_from_user(layout.index_address,&mut check)?;}
        if u32::from_le_bytes(check)!=index{return Err(VmError::PermissionDenied);}
    }
    let vector_slot=tls_vector+index as u64*8;
    unsafe{space.copy_to_user(vector_slot,&base.to_le_bytes())?;}
    let mut vector_check=[0u8;8];unsafe{space.copy_from_user(vector_slot,&mut vector_check)?;}
    if u64::from_le_bytes(vector_check)!=base{return Err(VmError::PermissionDenied);}
    Ok(())
}

unsafe fn invoke_user_function(
    space: &mut AddressSpace,
    function: u64,
    image_base: u64,
    teb: u64,
    stack_top: u64,
    ordinal: usize,
) -> Result<(), VmError> {
    let va=CALLBACK_BASE.checked_add(ordinal as u64*PAGE).ok_or(VmError::InvalidAddress)?;
    let physical=unsafe{crate::memory::allocate_page()}.map_err(|_|VmError::OutOfMemory)?;
    unsafe{space.map_user_page(va,physical,true,false)?;}
    let code=callback_trampoline(function,image_base);
    unsafe{space.copy_to_user(va,&code)?;}
    unsafe{space.protect_user_page(va,false,true)?;}
    unsafe {
        crate::serial_write(b"[DBG][NTOSKRNL][USER] trampoline enter ordinal=");
        crate::serial_write_hex(ordinal as u64);
        crate::serial_write(b" trampoline=");
        crate::serial_write_hex(va);
        crate::serial_write(b" target=");
        crate::serial_write_hex(function);
        crate::serial_write(b" image=");
        crate::serial_write_hex(image_base);
        crate::serial_write(b" teb=");
        crate::serial_write_hex(teb);
        crate::serial_write(b" rsp=");
        crate::serial_write_hex(stack_top);
        crate::serial_write(b"\r\n");
        rdotos_process_enter_user(va,stack_top,teb);
        crate::serial_write(b"[DBG][NTOSKRNL][USER] trampoline returned ordinal=");
        crate::serial_write_hex(ordinal as u64);
        crate::serial_write(b" target=");
        crate::serial_write_hex(function);
        crate::serial_write(b"\r\n");
    }
    Ok(())
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
    b[37]=0x20; b[38]=0xcd; b[39]=0x83;
    b
}

fn cleanup_process(space:&mut AddressSpace,image_base:u64,pid:u64)->Result<(),VmError>{
    if let Some(snapshot)=crate::process::query(pid) {
        if snapshot.state==crate::process::ProcessState::Running {
            let _=crate::process::mark_exited(pid,u64::MAX-1);
        }
    }
    let mut modules=[crate::winabi::ModuleInfo::EMPTY;32];
    let count=crate::winabi::module_infos(&mut modules);
    for module in modules[..count].iter() {
        crate::pe_runtime::unregister_image(module.base);
        crate::winabi::unregister_module(&module.name[..module.len]);
    }
    crate::pe_runtime::unregister_image(image_base);
    // Reap only after destroy detaches the process address-space root. Calling
    // reap first always fails its root==0 precondition and leaked the EPROCESS.
    unsafe{space.destroy()?;}
    let _=crate::process::reap(pid);
    Ok(())
}

/// Start a loaded image with a minimal PEB/TEB, execute registered TLS callbacks
/// in user mode, then transfer to the image entry point. The synthetic smoke
/// image exits through SYS_EXIT to exercise the real dispatcher.
pub unsafe fn run_loaded_image(space: &mut AddressSpace, image: LoadedImage) -> Result<(), VmError> {
    unsafe { run_loaded_image_in_session(space, image, 0) }
}

/// Run a loaded image with an explicit session identity. Session IDs flow into
/// the registered process object and therefore into IRP requestor-session queries.
pub unsafe fn run_loaded_image_in_session(space: &mut AddressSpace, image: LoadedImage, session_id: u32) -> Result<(), VmError> {
    // Real NTDLL calls need more than one page for nested helpers and callbacks.
    // Leave USER_STACK-PAGE unmapped as a guard below this bounded stack.
    for index in 0..USER_STACK_PAGES {
        let stack_page = unsafe { crate::memory::allocate_page() }.map_err(|_| VmError::OutOfMemory)?;
        unsafe { space.map_user_page(USER_STACK + index * PAGE, stack_page, true, false)?; }
    }
    // Map the staged Rust NTDLL into this process before creating its PEB/LDR
    // so the loader list, process heap, TLS, and DllMain callout refer to the
    // image actually mapped in this address space.
    let ntdll_already_registered = crate::winabi::resolve_import(
        b"ntdll.dll",
        crate::pe::ImportSymbol::Name(b"NtQuerySystemInformation"),
    ).is_some();
    let _process_ntdll = if !image.is_dll && !ntdll_already_registered {
        if let Some(ntdll_bytes) = crate::boot_ntdll_bytes() {
            let loaded = unsafe { crate::pe::load_dll(space, ntdll_bytes, b"ntdll.dll", None) }
                .map_err(|_| VmError::InvalidAddress)?;
            if !loaded.is_dll { return Err(VmError::InvalidAddress); }
            unsafe { crate::serial_write(b"Rust NTDLL mapped into process before PEB/LDR initialization.\\r\\n"); }
            Some(loaded)
        } else {
            None
        }
    } else {
        // The isolated native-NTDLL smoke has already registered and mapped
        // ntdll.dll in this address space; don't map the same DLL twice.
        None
    };
    unsafe { crate::serial_write(b"[DBG][NTOSKRNL][USER] process-init begin\r\n"); }
    let env = unsafe { crate::process::initialize_in_session(space, image.base, image.entry, image.image_size, USER_STACK_TOP, USER_STACK, session_id) }
        .map_err(|_| VmError::OutOfMemory)?;
    // Verify the exact Windows 11 24H2 XState ABI through the same checked
    // user-copy path native NTDLL uses, and verify the shared page is read-only.
    let mut xstate_header = [0u8; 24];
    unsafe {
        space.copy_from_user(crate::process::KUSER_SHARED_DATA_ADDRESS + crate::process::KUSER_XSTATE_OFFSET,
            &mut xstate_header)?;
    }
    let enabled_xstate = u64::from_le_bytes(xstate_header[0..8].try_into().unwrap());
    let xstate_size = u32::from_le_bytes(xstate_header[16..20].try_into().unwrap());
    // XSAVE is optional on this boot path. If the CPU/kernel could not
    // enable it, the published KUSER configuration must be consistently
    // disabled (mask=0, size=0). Otherwise require exactly x87+SSE and a
    // standard area large enough for legacy x87/SSE state.
    let xstate_config_valid = match enabled_xstate {
        0 => xstate_size == 0,
        3 => xstate_size >= 576,
        _ => false,
    };
    if !xstate_config_valid {
        unsafe {
            crate::serial_write(b"[DBG][NTOSKRNL][USER] invalid KUSER XState config mask=");
            crate::serial_write_hex(enabled_xstate);
            crate::serial_write(b" size=");
            crate::serial_write_hex(xstate_size as u64);
            crate::serial_write(b"\\r\\n");
        }
        return Err(VmError::PermissionDenied);
    }
    if unsafe { space.copy_to_user(crate::process::KUSER_SHARED_DATA_ADDRESS + crate::process::KUSER_XSTATE_OFFSET,
        &[0u8; 1]) }.is_ok() {
        return Err(VmError::PermissionDenied);
    }
    unsafe { crate::serial_write(b"KUSER_SHARED_DATA XState mapping and read-only ABI test passed.\r\n"); }
    // PEB.ProcessHeap starts null and is initialized by Rust NTDLL's
    // RtlGetProcessHeap, rather than a Cognac-only PE export.
    let mut pointer_check=[0u8;8];
    unsafe { space.copy_from_user(env.teb+0x60,&mut pointer_check)?; }
    if u64::from_le_bytes(pointer_check)!=env.peb { return Err(VmError::PermissionDenied); }
    unsafe { space.copy_from_user(env.peb+0x10,&mut pointer_check)?; }
    if u64::from_le_bytes(pointer_check)!=image.base { return Err(VmError::PermissionDenied); }
    unsafe { space.copy_from_user(env.peb+0x18,&mut pointer_check)?; }
    if u64::from_le_bytes(pointer_check)!=crate::process::LDR_ADDRESS { return Err(VmError::PermissionDenied); }
    unsafe { space.copy_from_user(env.peb+0x20,&mut pointer_check)?; }
    if u64::from_le_bytes(pointer_check)!=crate::process::PROCESS_PARAMS_ADDRESS { return Err(VmError::PermissionDenied); }
    unsafe { space.copy_from_user(crate::process::LDR_ENTRY_ADDRESS+0x30,&mut pointer_check)?; }
    if u64::from_le_bytes(pointer_check)!=image.base { return Err(VmError::PermissionDenied); }
    unsafe { space.copy_from_user(crate::process::LDR_ENTRY_ADDRESS,&mut pointer_check)?; }
    if u64::from_le_bytes(pointer_check)!=crate::process::LDR_ENTRY_ADDRESS+0x100 { return Err(VmError::PermissionDenied); }
    let mut unicode_len=[0u8;2];unsafe{space.copy_from_user(crate::process::PROCESS_PARAMS_ADDRESS+0x70,&mut unicode_len)?;}
    if u16::from_le_bytes(unicode_len)!=20{return Err(VmError::PermissionDenied);}
    let mut modules=[crate::winabi::ModuleInfo::EMPTY;32];
    let module_count=crate::winabi::module_infos(&mut modules);
    for (index,module) in modules[..module_count].iter().copied().enumerate() {
        let layout=crate::pe_runtime::tls_layout(module.base).map_err(|_|VmError::InvalidAddress)?;
        initialize_tls_block(space,layout,index as u32,env.tls_vector)?;
    }
    let main_tls=crate::pe_runtime::tls_layout(image.base).map_err(|_|VmError::InvalidAddress)?;
    initialize_tls_block(space,main_tls,module_count as u32,env.tls_vector)?;
    unsafe { crate::serial_write(b"[DBG][NTOSKRNL][USER] PEB/TEB, loader list, TLS vector validated\r\n"); }
    let calls_before=crate::syscall::dispatch_count();
    let kernel_cr3: u64;
    unsafe { asm!("mov {}, cr3", out(reg) kernel_cr3, options(nomem, nostack, preserves_flags)); }
    unsafe { crate::serial_write(b"[DBG][NTOSKRNL][USER] activating address space and setting current process\r\n"); }
    crate::syscall::set_current_process(env.process_id, space as *const _);
    unsafe { space.activate(); }
    // DLL TLS callbacks run before DllMain; the executable TLS callbacks run
    // before the executable entry point. All callbacks execute in ring 3.
    let execution_result = (|| -> Result<(), VmError> {
        let mut trampoline_index=0usize;
        unsafe { crate::serial_write(b"[DBG][NTOSKRNL][LOADER] begin DLL TLS/DllMain sequence\r\n"); }
        for module in modules[..module_count].iter().copied() {
            let mut dll_callbacks=[0u64;64];
            let n=crate::pe_runtime::tls_callbacks(module.base,&mut dll_callbacks)
                .map_err(|_|VmError::InvalidAddress)?;
            for callback in dll_callbacks[..n].iter().copied() {
                unsafe { crate::serial_write(b"[DBG][NTOSKRNL][TLS] entering DLL TLS callback VA="); crate::serial_write_hex(callback); crate::serial_write(b" module="); crate::serial_write_hex(module.base); crate::serial_write(b" stack-top="); crate::serial_write_hex(USER_STACK_TOP); crate::serial_write(b"\r\n"); }
                unsafe{invoke_user_function(space,callback,module.base,env.teb,USER_STACK_TOP,trampoline_index)?;}
                trampoline_index+=1;
            }
            if module.entrypoint!=0 {
                unsafe { crate::serial_write(b"[DBG][NTOSKRNL][LOADER] entering DllMain VA="); crate::serial_write_hex(module.entrypoint); crate::serial_write(b" module="); crate::serial_write_hex(module.base); crate::serial_write(b"\r\n"); }
                unsafe{invoke_user_function(space,module.entrypoint,module.base,env.teb,USER_STACK_TOP,trampoline_index)?;}
                trampoline_index+=1;
            }
        }
        let mut callbacks=[0u64;64];
        let callback_count=crate::pe_runtime::tls_callbacks(image.base,&mut callbacks)
            .map_err(|_|VmError::InvalidAddress)?;
        for callback in callbacks[..callback_count].iter().copied() {
            unsafe { crate::serial_write(b"[DBG][NTOSKRNL][TLS] entering EXE TLS callback VA="); crate::serial_write_hex(callback); crate::serial_write(b" image="); crate::serial_write_hex(image.base); crate::serial_write(b"\r\n"); }
            unsafe{invoke_user_function(space,callback,image.base,env.teb,USER_STACK_TOP,trampoline_index)?;}
            trampoline_index+=1;
        }
        crate::serial_write(b"PE TLS callbacks executed in ring 3; per-image TLS blocks initialized.\r\n");
        // Provide the return address expected by the Windows x64 thread-entry ABI.
        // A minimal PE entry point that executes RET must terminate the process,
        // not fetch an instruction from address zero above the mapped user stack.
        const PROCESS_RETURN_TRAMPOLINE: u64 = 0x0000_0086_0000_0000;
        let return_page = unsafe { crate::memory::allocate_page() }.map_err(|_| VmError::OutOfMemory)?;
        unsafe { space.map_user_page(PROCESS_RETURN_TRAMPOLINE, return_page, true, false)?; }
        // xor edi,edi; xor eax,eax; int 81h (SYS_EXIT(0)); ud2.
        const RETURN_CODE: [u8; 8] = [0x31, 0xff, 0x31, 0xc0, 0xcd, 0x81, 0x0f, 0x0b];
        unsafe { space.copy_to_user(PROCESS_RETURN_TRAMPOLINE, &RETURN_CODE)?; }
        unsafe { space.protect_user_page(PROCESS_RETURN_TRAMPOLINE, false, true)?; }
        let initial_user_rsp = USER_STACK_TOP - 8;
        unsafe { space.copy_to_user(initial_user_rsp, &PROCESS_RETURN_TRAMPOLINE.to_le_bytes())?; }
        unsafe { crate::serial_write(b"[DBG][NTOSKRNL][USER] entering image entry RIP="); crate::serial_write_hex(image.entry); crate::serial_write(b" RSP="); crate::serial_write_hex(initial_user_rsp); crate::serial_write(b" TEB="); crate::serial_write_hex(env.teb); crate::serial_write(b" return-trampoline="); crate::serial_write_hex(PROCESS_RETURN_TRAMPOLINE); crate::serial_write(b"\r\n"); }
        unsafe{rdotos_process_enter_user(image.entry,initial_user_rsp,env.teb);}
        Ok(())
    })();
    unsafe { asm!("mov cr3, {}", in(reg) kernel_cr3, options(nostack, preserves_flags)); }
    crate::syscall::clear_current_process();
    if let Err(error)=execution_result {
        let _=crate::process::mark_exited(env.process_id,u64::MAX-1);
        let _=cleanup_process(space,image.base,env.process_id);
        return Err(error);
    }
    let snapshot=crate::process::query(env.process_id).ok_or(VmError::InvalidAddress)?;
    let valid=crate::syscall::dispatch_count().saturating_sub(calls_before)>=2
        && crate::syscall::last_exit_code()==0
        && snapshot.state==crate::process::ProcessState::Exited
        && snapshot.exit_code==0;
    let cleanup=cleanup_process(space,image.base,env.process_id);
    cleanup?;
    if !valid {
        unsafe {
            crate::serial_write(b"Ring-3 smoke validation failed; syscall delta=");
            crate::serial_write_hex(crate::syscall::dispatch_count().saturating_sub(calls_before));
            crate::serial_write(b" exit=");
            crate::serial_write_hex(crate::syscall::last_exit_code());
            crate::serial_write(b" state=");
            crate::serial_write_hex(snapshot.state as u32 as u64);
            crate::serial_write(b"\r\n");
        }
        return Err(VmError::PermissionDenied);
    }
    Ok(())
}

const NTDLL_TEST_CODE: u64 = 0x0000_0084_0000_0000;
const NTDLL_TEST_DATA: u64 = 0x0000_0085_0000_0000;

fn resolve_required_ntdll_export(name: &'static [u8]) -> Result<u64, VmError> {
    match crate::winabi::resolve_import(
        b"ntdll.dll",
        crate::pe::ImportSymbol::Name(name),
    ) {
        Some(address) => Ok(address),
        None => {
            unsafe {
                crate::serial_write(b"[DBG][NTDLL] required export missing: ");
                crate::serial_write(name);
                crate::serial_write(b"\r\n");
            }
            Err(VmError::InvalidAddress)
        }
    }
}

/// Load-and-execute smoke test for CognacOS's Rust NTDLL. The mapped PE export
/// table and canonical Windows x64 SYSCALL stubs are the sole user/kernel path.
pub unsafe fn run_native_ntdll_smoke(
    space: &mut AddressSpace,
    ntdll: LoadedImage,
) -> Result<(), VmError> {
    use crate::pe::ImportSymbol;
    let query_process = resolve_required_ntdll_export(b"NtQueryInformationProcess")?;
    let query_thread = resolve_required_ntdll_export(b"NtQueryInformationThread")?;
    let query_system = resolve_required_ntdll_export(b"NtQuerySystemInformation")?;
    let yield_execution = resolve_required_ntdll_export(b"NtYieldExecution")?;
    let rtl_init_unicode = resolve_required_ntdll_export(b"RtlInitUnicodeString")?;
    let rtl_compare_unicode = resolve_required_ntdll_export(b"RtlCompareUnicodeString")?;
    let rtl_copy_unicode = resolve_required_ntdll_export(b"RtlCopyUnicodeString")?;
    let rtl_unicode_to_integer = resolve_required_ntdll_export(b"RtlUnicodeStringToInteger")?;
    let rtl_integer_to_unicode = resolve_required_ntdll_export(b"RtlIntegerToUnicodeString")?;
    let rtl_init_ansi = resolve_required_ntdll_export(b"RtlInitAnsiString")?;
    let rtl_equal_ansi = resolve_required_ntdll_export(b"RtlEqualString")?;
    let rtl_get_nt_version = resolve_required_ntdll_export(b"RtlGetNtVersionNumbers")?;
    let rtl_status_to_dos = resolve_required_ntdll_export(b"RtlNtStatusToDosError")?;
    let rtl_compare_memory = resolve_required_ntdll_export(b"RtlCompareMemory")?;
    let rtl_compute_crc32 = resolve_required_ntdll_export(b"RtlComputeCrc32")?;
    let rtl_is_zero_memory = resolve_required_ntdll_export(b"RtlIsZeroMemory")?;
    let rtl_fill_memory = resolve_required_ntdll_export(b"RtlFillMemory")?;
    let rtl_zero_memory = resolve_required_ntdll_export(b"RtlZeroMemory")?;
    let rtl_move_memory = resolve_required_ntdll_export(b"RtlMoveMemory")?;
    let rtl_copy_memory = resolve_required_ntdll_export(b"RtlCopyMemory")?;
    let rtl_compare_memory_ulong = resolve_required_ntdll_export(b"RtlCompareMemoryUlong")?;
    let rtl_get_version = resolve_required_ntdll_export(b"RtlGetVersion")?;
    let rtl_get_current_peb = resolve_required_ntdll_export(b"RtlGetCurrentPeb")?;
    let ldr_get_procedure_address = resolve_required_ntdll_export(b"LdrGetProcedureAddress")?;
    let ldr_get_dll_handle = resolve_required_ntdll_export(b"LdrGetDllHandle")?;
    let rtl_get_last_error = resolve_required_ntdll_export(b"RtlGetLastWin32Error")?;
    let rtl_set_last_error = resolve_required_ntdll_export(b"RtlSetLastWin32Error")?;
    let rtl_get_last_status = resolve_required_ntdll_export(b"RtlGetLastNtStatus")?;
    let rtl_set_error_from_status = resolve_required_ntdll_export(b"RtlSetLastWin32ErrorAndNtStatusFromNtStatus")?;
    let rtl_allocate_heap = resolve_required_ntdll_export(b"RtlAllocateHeap")?;
    let rtl_free_heap = resolve_required_ntdll_export(b"RtlFreeHeap")?;
    let rtl_reallocate_heap = resolve_required_ntdll_export(b"RtlReAllocateHeap")?;
    let rtl_size_heap = resolve_required_ntdll_export(b"RtlSizeHeap")?;
    let rtl_validate_heap = resolve_required_ntdll_export(b"RtlValidateHeap")?;
    unsafe { crate::serial_write(b"[DBG][NTDLL] all required smoke exports resolved\r\n"); }
    let mut data = [0u8; 4096];
    data[0x3f0] = 66;
    data[0x3f1] = 65;
    for (index, byte) in b"Cognac".iter().copied().enumerate() {
        data[0xA00 + index * 2] = byte;
        data[0xA20 + index * 2] = byte.to_ascii_lowercase();
    }
    for (index, byte) in b"0x2A".iter().copied().enumerate() {
        data[0xA90 + index * 2] = byte;
    }
    data[0xA62..0xA64].copy_from_slice(&16u16.to_le_bytes());
    data[0xA68..0xA70].copy_from_slice(&(NTDLL_TEST_DATA + 0xA80).to_le_bytes());
    data[0xAC2..0xAC4].copy_from_slice(&16u16.to_le_bytes());
    data[0xAC8..0xAD0].copy_from_slice(&(NTDLL_TEST_DATA + 0xAE0).to_le_bytes());
    data[0xB00..0xB06].copy_from_slice(b"Cognac");
    data[0xB10..0xB16].copy_from_slice(b"cognac");
    data[0xB60..0xB69].copy_from_slice(b"123456789");
    data[0xB70..0xB74].copy_from_slice(b"same");
    data[0xB80..0xB84].copy_from_slice(b"same");
    data[0xBA0] = 1;
    data[0xBD0..0xBD6].copy_from_slice(b"abcdef");
    data[0xC00..0xC04].copy_from_slice(&276u32.to_le_bytes()); // RTL_OSVERSIONINFOW size
    data[0xE00..0xE10].copy_from_slice(b"RtlGetCurrentPeb");
    data[0xE20..0xE22].copy_from_slice(&16u16.to_le_bytes());
    data[0xE22..0xE24].copy_from_slice(&17u16.to_le_bytes());
    data[0xE28..0xE30].copy_from_slice(&(NTDLL_TEST_DATA + 0xE00).to_le_bytes());
    for (index, byte) in b"ntdll.dll".iter().copied().enumerate() {
        data[0xE60 + index * 2] = byte;
    }
    data[0xE80..0xE82].copy_from_slice(&18u16.to_le_bytes());
    data[0xE82..0xE84].copy_from_slice(&20u16.to_le_bytes());
    data[0xE88..0xE90].copy_from_slice(&(NTDLL_TEST_DATA + 0xE60).to_le_bytes());
    let data_page = unsafe { crate::memory::allocate_page() }.map_err(|_| VmError::OutOfMemory)?;
    unsafe { space.map_user_page(NTDLL_TEST_DATA, data_page, true, false)?; }
    unsafe { space.copy_to_user(NTDLL_TEST_DATA, &data)?; }
    unsafe { crate::serial_write(b"[DBG][NTDLL] smoke data page mapped and initialized\r\n"); }

    let mut code = [0u8; 4096];
    let mut length = 0usize;
    let mut failures = [0usize; 64];
    let mut failure_count = 0usize;
    macro_rules! emit {
        ($bytes:expr) => {{
            let bytes: &[u8] = $bytes;
            let end = length.checked_add(bytes.len()).ok_or(VmError::InvalidAddress)?;
            if end > code.len() { return Err(VmError::InvalidAddress); }
            code[length..end].copy_from_slice(bytes);
            length = end;
        }};
    }
    macro_rules! imm64 {
        ($prefix:expr, $value:expr) => {{
            emit!($prefix);
            emit!(&($value as u64).to_le_bytes());
        }};
    }
    macro_rules! call_ptr {
        ($value:expr) => {{
            imm64!(&[0x48, 0xb8], $value);
            emit!(&[0xff, 0xd0]);
        }};
    }
    macro_rules! fail_if {
        ($opcode:expr) => {{
            emit!($opcode);
            let at = length;
            emit!(&[0, 0, 0, 0]);
            if failure_count >= failures.len() { return Err(VmError::InvalidAddress); }
            failures[failure_count] = at;
            failure_count += 1;
        }};
    }
    macro_rules! check_dword {
        ($address:expr, $expected:expr) => {{
            imm64!(&[0x48, 0xb8], $address);
            emit!(&[0x8b, 0x00, 0x3d]);
            emit!(&($expected as u32).to_le_bytes());
            fail_if!(&[0x0f, 0x85]);
        }};
    }
    emit!(&[0x48, 0x83, 0xec, 0x38]); // 32-byte shadow space plus 8-byte alignment/fifth-argument slack; RSP is 16-byte aligned before each call.
    // NtQueryInformationProcess(-1, ProcessBasicInformation, info, 48, &returnLength).
    imm64!(&[0x48, 0xb9], u64::MAX);
    emit!(&[0xba, 0, 0, 0, 0]);
    imm64!(&[0x49, 0xb8], NTDLL_TEST_DATA);
    emit!(&[0x41, 0xb9, 48, 0, 0, 0]);
    imm64!(&[0x48, 0xb8], NTDLL_TEST_DATA + 0x80);
    emit!(&[0x48, 0x89, 0x44, 0x24, 0x20]);
    call_ptr!(query_process);
    emit!(&[0x85, 0xc0]);
    fail_if!(&[0x0f, 0x85]);
    check_dword!(NTDLL_TEST_DATA + 0x80, 48);
    imm64!(&[0x48, 0xb8], NTDLL_TEST_DATA + 0x20);
    emit!(&[0x48, 0x8b, 0x00, 0x48, 0x85, 0xc0]);
    fail_if!(&[0x0f, 0x84]); // ProcessId must be nonzero.

    // NtQueryInformationThread(-2, ThreadBasicInformation, info, 48, &returnLength).
    imm64!(&[0x48, 0xb9], u64::MAX - 1);
    emit!(&[0xba, 0, 0, 0, 0]);
    imm64!(&[0x49, 0xb8], NTDLL_TEST_DATA + 0x100);
    emit!(&[0x41, 0xb9, 48, 0, 0, 0]);
    imm64!(&[0x48, 0xb8], NTDLL_TEST_DATA + 0x180);
    emit!(&[0x48, 0x89, 0x44, 0x24, 0x20]);
    call_ptr!(query_thread);
    emit!(&[0x85, 0xc0]);
    fail_if!(&[0x0f, 0x85]);
    check_dword!(NTDLL_TEST_DATA + 0x180, 48);

    // NtQuerySystemInformation(SystemBasicInformation, info, 64, &returnLength).
    emit!(&[0x48, 0xb9, 0, 0, 0, 0, 0, 0, 0, 0]);
    imm64!(&[0x48, 0xba], NTDLL_TEST_DATA + 0x200);
    emit!(&[0x41, 0xb8, 64, 0, 0, 0]);
    imm64!(&[0x49, 0xb9], NTDLL_TEST_DATA + 0x280);
    call_ptr!(query_system);
    emit!(&[0x85, 0xc0]);
    fail_if!(&[0x0f, 0x85]);
    check_dword!(NTDLL_TEST_DATA + 0x280, 64);
    check_dword!(NTDLL_TEST_DATA + 0x208, 4096);

    call_ptr!(yield_execution);
    // Windows NtYieldExecution may legitimately return STATUS_NO_YIELD_PERFORMED
    // (0x40000024) when no peer thread is ready. Accept that informational
    // status as well as STATUS_SUCCESS; reject other nonzero NTSTATUS values.
    emit!(&[0x3d, 0x24, 0x00, 0x00, 0x40]); // cmp eax, STATUS_NO_YIELD_PERFORMED
    emit!(&[0x74, 0x08]); // je past test+jcc when no yield was performed
    emit!(&[0x85, 0xc0]); // test eax, eax
    fail_if!(&[0x0f, 0x85]);
    // Exercise native RTL Unicode/ANSI string routines from ring 3.
    imm64!(&[0x48, 0xb9], NTDLL_TEST_DATA + 0xA40);
    imm64!(&[0x48, 0xba], NTDLL_TEST_DATA + 0xA00);
    call_ptr!(rtl_init_unicode);
    imm64!(&[0x48, 0xb9], NTDLL_TEST_DATA + 0xA50);
    imm64!(&[0x48, 0xba], NTDLL_TEST_DATA + 0xA20);
    call_ptr!(rtl_init_unicode);
    imm64!(&[0x48, 0xb9], NTDLL_TEST_DATA + 0xA40);
    imm64!(&[0x48, 0xba], NTDLL_TEST_DATA + 0xA50);
    imm64!(&[0x49, 0xb8], 1);
    call_ptr!(rtl_compare_unicode);
    emit!(&[0x85, 0xc0]);
    fail_if!(&[0x0f, 0x85]); // case-insensitive comparison must report equality
    imm64!(&[0x48, 0xb9], NTDLL_TEST_DATA + 0xA60);
    imm64!(&[0x48, 0xba], NTDLL_TEST_DATA + 0xA40);
    call_ptr!(rtl_copy_unicode);
    check_dword!(NTDLL_TEST_DATA + 0xA60, 0x0010_000c);
    check_dword!(NTDLL_TEST_DATA + 0xA80, 0x006f0043); // UTF-16 "Co"

    imm64!(&[0x48, 0xb9], NTDLL_TEST_DATA + 0xAA0);
    imm64!(&[0x48, 0xba], NTDLL_TEST_DATA + 0xA90);
    call_ptr!(rtl_init_unicode);
    imm64!(&[0x48, 0xb9], NTDLL_TEST_DATA + 0xAA0);
    emit!(&[0xba, 16, 0, 0, 0]);
    imm64!(&[0x49, 0xb8], NTDLL_TEST_DATA + 0xAB0);
    call_ptr!(rtl_unicode_to_integer);
    emit!(&[0x85, 0xc0]);
    fail_if!(&[0x0f, 0x85]);
    check_dword!(NTDLL_TEST_DATA + 0xAB0, 42);

    emit!(&[0xb9, 42, 0, 0, 0, 0xba, 10, 0, 0, 0]);
    imm64!(&[0x49, 0xb8], NTDLL_TEST_DATA + 0xAC0);
    call_ptr!(rtl_integer_to_unicode);
    emit!(&[0x85, 0xc0]);
    fail_if!(&[0x0f, 0x85]);
    check_dword!(NTDLL_TEST_DATA + 0xAC0, 0x0010_0004);
    check_dword!(NTDLL_TEST_DATA + 0xAE0, 0x00320034); // UTF-16 "42"

    imm64!(&[0x48, 0xb9], NTDLL_TEST_DATA + 0xB20);
    imm64!(&[0x48, 0xba], NTDLL_TEST_DATA + 0xB00);
    call_ptr!(rtl_init_ansi);
    imm64!(&[0x48, 0xb9], NTDLL_TEST_DATA + 0xB30);
    imm64!(&[0x48, 0xba], NTDLL_TEST_DATA + 0xB10);
    call_ptr!(rtl_init_ansi);
    imm64!(&[0x48, 0xb9], NTDLL_TEST_DATA + 0xB20);
    imm64!(&[0x48, 0xba], NTDLL_TEST_DATA + 0xB30);
    imm64!(&[0x49, 0xb8], 1);
    call_ptr!(rtl_equal_ansi);
    emit!(&[0x3c, 1]); // BOOLEAN return is defined in AL, not all of EAX.
    fail_if!(&[0x0f, 0x85]);

    // TEB last-error/status fields must be accessed through the user GS base.
    emit!(&[0xb9, 123, 0, 0, 0]);
    call_ptr!(rtl_set_last_error);
    call_ptr!(rtl_get_last_error);
    emit!(&[0x3d, 123, 0, 0, 0]);
    fail_if!(&[0x0f, 0x85]);
    emit!(&[0xb9, 0x0d, 0, 0, 0xc0]); // STATUS_INVALID_PARAMETER
    call_ptr!(rtl_set_error_from_status);
    call_ptr!(rtl_get_last_error);
    emit!(&[0x3d, 87, 0, 0, 0]); // ERROR_INVALID_PARAMETER
    fail_if!(&[0x0f, 0x85]);
    call_ptr!(rtl_get_last_status);
    emit!(&[0x3d, 0x0d, 0, 0, 0xc0]); // TEB.LastStatusValue
    fail_if!(&[0x0f, 0x85]);

    // Version tuple and NTSTATUS-to-Win32 error mapping.
    imm64!(&[0x48, 0xb9], NTDLL_TEST_DATA + 0xB40);
    imm64!(&[0x48, 0xba], NTDLL_TEST_DATA + 0xB44);
    imm64!(&[0x49, 0xb8], NTDLL_TEST_DATA + 0xB48);
    call_ptr!(rtl_get_nt_version);
    check_dword!(NTDLL_TEST_DATA + 0xB40, 10);
    check_dword!(NTDLL_TEST_DATA + 0xB44, 0);
    check_dword!(NTDLL_TEST_DATA + 0xB48, 26100);

    emit!(&[0xb9, 0x0d, 0, 0, 0xc0]); // STATUS_INVALID_PARAMETER
    call_ptr!(rtl_status_to_dos);
    emit!(&[0x3d, 87, 0, 0, 0]); // ERROR_INVALID_PARAMETER
    fail_if!(&[0x0f, 0x85]);
    emit!(&[0xb9, 0xef, 0xbe, 0xad, 0xde]); // Unknown status
    call_ptr!(rtl_status_to_dos);
    emit!(&[0x3d, 0x3d, 1, 0, 0]); // ERROR_MR_MID_NOT_FOUND
    fail_if!(&[0x0f, 0x85]);

    // Core RTL memory and CRC helpers.
    imm64!(&[0x48, 0xb9], NTDLL_TEST_DATA + 0xB70);
    imm64!(&[0x48, 0xba], NTDLL_TEST_DATA + 0xB80);
    imm64!(&[0x49, 0xb8], 4);
    call_ptr!(rtl_compare_memory);
    emit!(&[0x3d, 4, 0, 0, 0]);
    fail_if!(&[0x0f, 0x85]);

    emit!(&[0xb9, 0, 0, 0, 0]);
    imm64!(&[0x48, 0xba], NTDLL_TEST_DATA + 0xB60);
    imm64!(&[0x49, 0xb8], 9);
    call_ptr!(rtl_compute_crc32);
    emit!(&[0x3d, 0x26, 0x39, 0xf4, 0xcb]); // CRC32("123456789")
    fail_if!(&[0x0f, 0x85]);

    imm64!(&[0x48, 0xb9], NTDLL_TEST_DATA + 0xB90);
    emit!(&[0xba, 16, 0, 0, 0]);
    call_ptr!(rtl_is_zero_memory);
    emit!(&[0x3c, 1]);
    fail_if!(&[0x0f, 0x85]);
    imm64!(&[0x48, 0xb9], NTDLL_TEST_DATA + 0xBA0);
    emit!(&[0xba, 16, 0, 0, 0]);
    call_ptr!(rtl_is_zero_memory);
    emit!(&[0x3c, 0]);
    fail_if!(&[0x0f, 0x85]);

    imm64!(&[0x48, 0xb9], NTDLL_TEST_DATA + 0xBC0);
    emit!(&[0xba, 4, 0, 0, 0]);
    imm64!(&[0x49, 0xb8], 0x41);
    call_ptr!(rtl_fill_memory);
    check_dword!(NTDLL_TEST_DATA + 0xBC0, 0x41414141);
    imm64!(&[0x48, 0xb9], NTDLL_TEST_DATA + 0xBC0);
    emit!(&[0xba, 4, 0, 0, 0]);
    imm64!(&[0x49, 0xb8], 0x41414141);
    call_ptr!(rtl_compare_memory_ulong);
    emit!(&[0x3d, 4, 0, 0, 0]);
    fail_if!(&[0x0f, 0x85]);
    imm64!(&[0x48, 0xb9], NTDLL_TEST_DATA + 0xBC0);
    emit!(&[0xba, 4, 0, 0, 0]);
    call_ptr!(rtl_zero_memory);
    check_dword!(NTDLL_TEST_DATA + 0xBC0, 0);

    imm64!(&[0x48, 0xb9], NTDLL_TEST_DATA + 0xBD1);
    imm64!(&[0x48, 0xba], NTDLL_TEST_DATA + 0xBD0);
    emit!(&[0x41, 0xb8, 6, 0, 0, 0]);
    call_ptr!(rtl_move_memory);
    check_dword!(NTDLL_TEST_DATA + 0xBD1, 0x64636261);
    imm64!(&[0x48, 0xb9], NTDLL_TEST_DATA + 0xBE0);
    imm64!(&[0x48, 0xba], NTDLL_TEST_DATA + 0xB70);
    emit!(&[0x41, 0xb8, 4, 0, 0, 0]);
    call_ptr!(rtl_copy_memory);
    check_dword!(NTDLL_TEST_DATA + 0xBE0, 0x656d6173); // "same"

    // LdrGetProcedureAddress must resolve a real export by its ANSI name.
    imm64!(&[0x48, 0xb9], ntdll.base);
    imm64!(&[0x48, 0xba], NTDLL_TEST_DATA + 0xE20);
    emit!(&[0x45, 0x31, 0xc0]); // ordinal argument unused for name lookup
    imm64!(&[0x49, 0xb9], NTDLL_TEST_DATA + 0xE40);
    call_ptr!(ldr_get_procedure_address);
    emit!(&[0x85, 0xc0]);
    fail_if!(&[0x0f, 0x85]);
    imm64!(&[0x48, 0xb8], NTDLL_TEST_DATA + 0xE40);
    emit!(&[0x48, 0x8b, 0x00]); // RAX = resolved procedure pointer
    imm64!(&[0x48, 0xba], rtl_get_current_peb);
    emit!(&[0x48, 0x39, 0xd0]);
    fail_if!(&[0x0f, 0x85]);

    // LdrGetDllHandle finds the already-loaded native DLL through PEB.Ldr.
    emit!(&[0x48, 0x31, 0xc9, 0x48, 0x31, 0xd2]);
    imm64!(&[0x49, 0xb8], NTDLL_TEST_DATA + 0xE80);
    imm64!(&[0x49, 0xb9], NTDLL_TEST_DATA + 0xE98);
    call_ptr!(ldr_get_dll_handle);
    emit!(&[0x85, 0xc0]);
    fail_if!(&[0x0f, 0x85]);
    imm64!(&[0x48, 0xb8], NTDLL_TEST_DATA + 0xE98);
    emit!(&[0x48, 0x8b, 0x00]);
    imm64!(&[0x48, 0xba], ntdll.base);
    emit!(&[0x48, 0x39, 0xd0]);
    fail_if!(&[0x0f, 0x85]);

    // GS:[0x60] must expose the active process PEB.
    call_ptr!(rtl_get_current_peb);
    imm64!(&[0x48, 0xba], crate::process::PEB_ADDRESS);
    emit!(&[0x48, 0x39, 0xd0]);
    fail_if!(&[0x0f, 0x85]);

    // RtlGetCurrentPeb initializes the default heap lazily and publishes
    // its handle in PEB.ProcessHeap, without requiring a Cognac-only export.
    imm64!(&[0x48, 0xb8], crate::process::PEB_ADDRESS);
    emit!(&[0x48, 0x8b, 0x40, 0x30]); // RAX = PEB.ProcessHeap
    emit!(&[0x48, 0x85, 0xc0]);
    fail_if!(&[0x0f, 0x84]); // the default process heap must be initialized
    imm64!(&[0x48, 0xba], NTDLL_TEST_DATA + 0xD00);
    emit!(&[0x48, 0x89, 0x02]); // save heap handle for the heap ABI tests

    // Default process heap: PEB handle, allocation, size, realloc, free and validation.
    imm64!(&[0x48, 0xba], NTDLL_TEST_DATA + 0xD00);
    emit!(&[0x48, 0x89, 0x02]); // save heap handle

    imm64!(&[0x48, 0xb8], NTDLL_TEST_DATA + 0xD00);
    emit!(&[0x48, 0x8b, 0x08]); // RCX = heap
    emit!(&[0xba, 0, 0, 0, 0]);
    imm64!(&[0x49, 0xb8], 32);
    call_ptr!(rtl_allocate_heap);
    emit!(&[0x48, 0x85, 0xc0]);
    fail_if!(&[0x0f, 0x84]);
    imm64!(&[0x48, 0xba], NTDLL_TEST_DATA + 0xD08);
    emit!(&[0x48, 0x89, 0x02]); // save allocation pointer

    imm64!(&[0x48, 0xb8], NTDLL_TEST_DATA + 0xD08);
    emit!(&[0x48, 0x8b, 0x08]);
    emit!(&[0xba, 4, 0, 0, 0]);
    imm64!(&[0x49, 0xb8], 0x41);
    call_ptr!(rtl_fill_memory);

    imm64!(&[0x48, 0xb8], NTDLL_TEST_DATA + 0xD00);
    emit!(&[0x48, 0x8b, 0x08]);
    emit!(&[0xba, 0, 0, 0, 0]);
    imm64!(&[0x48, 0xb8], NTDLL_TEST_DATA + 0xD08);
    emit!(&[0x4c, 0x8b, 0x00]); // R8 = allocation
    call_ptr!(rtl_size_heap);
    emit!(&[0x3d, 32, 0, 0, 0]);
    fail_if!(&[0x0f, 0x85]);

    imm64!(&[0x48, 0xb8], NTDLL_TEST_DATA + 0xD00);
    emit!(&[0x48, 0x8b, 0x08]);
    emit!(&[0xba, 0, 0, 0, 0]);
    imm64!(&[0x48, 0xb8], NTDLL_TEST_DATA + 0xD08);
    emit!(&[0x4c, 0x8b, 0x00]);
    call_ptr!(rtl_validate_heap);
    emit!(&[0x3c, 1]);
    fail_if!(&[0x0f, 0x85]);

    imm64!(&[0x48, 0xb8], NTDLL_TEST_DATA + 0xD00);
    emit!(&[0x48, 0x8b, 0x08]);
    emit!(&[0xba, 0, 0, 0, 0]);
    imm64!(&[0x48, 0xb8], NTDLL_TEST_DATA + 0xD08);
    emit!(&[0x4c, 0x8b, 0x00]);
    emit!(&[0x41, 0xb9, 64, 0, 0, 0]); // R9D = new size
    call_ptr!(rtl_reallocate_heap);
    emit!(&[0x48, 0x85, 0xc0]);
    fail_if!(&[0x0f, 0x84]);
    imm64!(&[0x48, 0xba], NTDLL_TEST_DATA + 0xD08);
    emit!(&[0x48, 0x89, 0x02]);

    imm64!(&[0x48, 0xb8], NTDLL_TEST_DATA + 0xD00);
    emit!(&[0x48, 0x8b, 0x08]);
    emit!(&[0xba, 0, 0, 0, 0]);
    imm64!(&[0x48, 0xb8], NTDLL_TEST_DATA + 0xD08);
    emit!(&[0x4c, 0x8b, 0x00]);
    call_ptr!(rtl_size_heap);
    emit!(&[0x3d, 64, 0, 0, 0]);
    fail_if!(&[0x0f, 0x85]);
    imm64!(&[0x48, 0xb8], NTDLL_TEST_DATA + 0xD08);
    emit!(&[0x48, 0x8b, 0x00, 0x8b, 0x00]);
    emit!(&[0x3d, 0x41, 0x41, 0x41, 0x41]); // realloc preserves existing bytes
    fail_if!(&[0x0f, 0x85]);

    imm64!(&[0x48, 0xb8], NTDLL_TEST_DATA + 0xD00);
    emit!(&[0x48, 0x8b, 0x08]);
    emit!(&[0xba, 0, 0, 0, 0]);
    imm64!(&[0x48, 0xb8], NTDLL_TEST_DATA + 0xD08);
    emit!(&[0x4c, 0x8b, 0x00]);
    call_ptr!(rtl_free_heap);
    emit!(&[0x3c, 1]);
    fail_if!(&[0x0f, 0x85]);

    imm64!(&[0x48, 0xb8], NTDLL_TEST_DATA + 0xD00);
    emit!(&[0x48, 0x8b, 0x08]);
    emit!(&[0xba, 0, 0, 0, 0]);
    imm64!(&[0x48, 0xb8], NTDLL_TEST_DATA + 0xD08);
    emit!(&[0x4c, 0x8b, 0x00]);
    call_ptr!(rtl_free_heap);
    emit!(&[0x3c, 0]);
    fail_if!(&[0x0f, 0x85]); // double free must be rejected

    imm64!(&[0x48, 0xb8], NTDLL_TEST_DATA + 0xD00);
    emit!(&[0x48, 0x8b, 0x08]);
    emit!(&[0xba, 8, 0, 0, 0]); // HEAP_ZERO_MEMORY
    imm64!(&[0x49, 0xb8], 32);
    call_ptr!(rtl_allocate_heap);
    emit!(&[0x48, 0x85, 0xc0]);
    fail_if!(&[0x0f, 0x84]);
    imm64!(&[0x48, 0xba], NTDLL_TEST_DATA + 0xD08);
    emit!(&[0x48, 0x89, 0x02]);
    imm64!(&[0x48, 0xb8], NTDLL_TEST_DATA + 0xD08);
    emit!(&[0x48, 0x8b, 0x00, 0x8b, 0x00]);
    emit!(&[0x3d, 0, 0, 0, 0]);
    fail_if!(&[0x0f, 0x85]); // zero-memory allocation must clear reused bytes
    imm64!(&[0x48, 0xb8], NTDLL_TEST_DATA + 0xD00);
    emit!(&[0x48, 0x8b, 0x08]);
    emit!(&[0xba, 0, 0, 0, 0]);
    imm64!(&[0x48, 0xb8], NTDLL_TEST_DATA + 0xD08);
    emit!(&[0x4c, 0x8b, 0x00]);
    call_ptr!(rtl_free_heap);
    emit!(&[0x3c, 1]);
    fail_if!(&[0x0f, 0x85]);

    // RtlGetVersion validates the caller's structure and reports the NT tuple.
    imm64!(&[0x48, 0xb9], NTDLL_TEST_DATA + 0xC00);
    call_ptr!(rtl_get_version);
    emit!(&[0x85, 0xc0]);
    fail_if!(&[0x0f, 0x85]);
    check_dword!(NTDLL_TEST_DATA + 0xC04, 10);
    check_dword!(NTDLL_TEST_DATA + 0xC08, 0);
    check_dword!(NTDLL_TEST_DATA + 0xC0C, 26100);
    check_dword!(NTDLL_TEST_DATA + 0xC10, 2);

    emit!(&[0xb8, 2, 0, 0, 0, 0x31, 0xff]);
    imm64!(&[0x48, 0xbe], NTDLL_TEST_DATA + 0x3f0);
    emit!(&[0xba, 1, 0, 0, 0, 0xcd, 0x81]);

    emit!(&[0x31, 0xff, 0x31, 0xc0, 0xcd, 0x81, 0x0f, 0x0b]); // exit(0)
    let mut failure_labels = [0usize; 64];
    for index in 0..failure_count {
        failure_labels[index] = length;
        emit!(&[0xbf]);
        emit!(&((index as u32 + 1).to_le_bytes()));
        emit!(&[0x31, 0xc0, 0xcd, 0x81, 0x0f, 0x0b]); // exit(index + 1)
    }
    for (index, at) in failures[..failure_count].iter().copied().enumerate() {
        let displacement = i32::try_from(failure_labels[index] as isize - (at + 4) as isize)
            .map_err(|_| VmError::InvalidAddress)?;
        code[at..at + 4].copy_from_slice(&displacement.to_le_bytes());
    }

    let code_page = unsafe { crate::memory::allocate_page() }.map_err(|_| VmError::OutOfMemory)?;
    unsafe { space.map_user_page(NTDLL_TEST_CODE, code_page, true, false)?; }
    unsafe { space.copy_to_user(NTDLL_TEST_CODE, &code[..length])?; }
    unsafe { space.protect_user_page(NTDLL_TEST_CODE, false, true)?; }
    let test_image = LoadedImage {
        base: ntdll.base,
        entry: NTDLL_TEST_CODE,
        image_size: ntdll.image_size,
        is_dll: false,
    };
    unsafe {
        crate::serial_write(b"[DBG][NTDLL] smoke code mapped; entering shared process loader\\r\\n");
        // This code is generated at runtime and has no ELF/PDB of its own.
        // Publish its exact address interval so a captured RIP can be classified
        // as a smoke-stub offset rather than incorrectly symbolized as NTDLL.
        crate::serial_write(b"SMOKE STUB: base=");
        crate::serial_write_hex(NTDLL_TEST_CODE);
        crate::serial_write(b" size=");
        crate::serial_write_hex(length as u64);
        crate::serial_write(b" end=");
        crate::serial_write_hex(NTDLL_TEST_CODE.saturating_add(length as u64));
        crate::serial_write(b"\\r\\n");
    }
    unsafe { run_loaded_image(space, test_image)?; }
    Ok(())
}
