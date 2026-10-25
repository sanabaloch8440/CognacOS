//! User-mode process launch and return path.
//! The bootstrap uses a dedicated DPL3 software interrupt for process exit;
//! normal syscalls return through IRETQ. IST-based exception stacks remain separate.
use core::arch::{asm, global_asm};
use crate::{pe::LoadedImage, ntoskrnl::mm::address_space::{AddressSpace, VmError}};
const PAGE: u64 = 4096;
const USER_STACK: u64 = 0x0000_0082_0000_0000;
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
    unsafe{rdotos_process_enter_user(va,stack_top,teb);}
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
    let stack_page = unsafe { crate::memory::allocate_page() }.map_err(|_| VmError::OutOfMemory)?;
    unsafe { space.map_user_page(USER_STACK, stack_page, true, false)?; }
    let env = unsafe { crate::process::initialize(space, image.base, image.entry, image.image_size, USER_STACK + PAGE, USER_STACK) }
        .map_err(|_| VmError::OutOfMemory)?;
    // The native NTDLL exports its per-process heap object as data. Point
    // PEB.ProcessHeap at that mapped object before user code begins.
    if let Some(process_heap) = crate::winabi::resolve_import(
        b"ntdll.dll", crate::pe::ImportSymbol::Name(b"CognacNtdllProcessHeapObject"))
    {
        unsafe { space.copy_to_user(env.peb + 0x30, &process_heap.to_le_bytes())?; }
        let mut heap_check = [0u8; 8];
        unsafe { space.copy_from_user(env.peb + 0x30, &mut heap_check)?; }
        if u64::from_le_bytes(heap_check) != process_heap {
            return Err(VmError::PermissionDenied);
        }
    }
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
    let calls_before=crate::syscall::dispatch_count();
    let kernel_cr3: u64;
    unsafe { asm!("mov {}, cr3", out(reg) kernel_cr3, options(nomem, nostack, preserves_flags)); }
    crate::syscall::set_current_process(env.process_id, space as *const _);
    unsafe { space.activate(); }
    // DLL TLS callbacks run before DllMain; the executable TLS callbacks run
    // before the executable entry point. All callbacks execute in ring 3.
    let execution_result = (|| -> Result<(), VmError> {
        let mut trampoline_index=0usize;
        for module in modules[..module_count].iter().copied() {
            let mut dll_callbacks=[0u64;64];
            let n=crate::pe_runtime::tls_callbacks(module.base,&mut dll_callbacks)
                .map_err(|_|VmError::InvalidAddress)?;
            for callback in dll_callbacks[..n].iter().copied() {
                unsafe{invoke_user_function(space,callback,module.base,env.teb,USER_STACK+PAGE,trampoline_index)?;}
                trampoline_index+=1;
            }
            if module.entrypoint!=0 {
                unsafe{invoke_user_function(space,module.entrypoint,module.base,env.teb,USER_STACK+PAGE,trampoline_index)?;}
                trampoline_index+=1;
            }
        }
        let mut callbacks=[0u64;64];
        let callback_count=crate::pe_runtime::tls_callbacks(image.base,&mut callbacks)
            .map_err(|_|VmError::InvalidAddress)?;
        for callback in callbacks[..callback_count].iter().copied() {
            unsafe{invoke_user_function(space,callback,image.base,env.teb,USER_STACK+PAGE,trampoline_index)?;}
            trampoline_index+=1;
        }
        crate::serial_write(b"PE TLS callbacks executed in ring 3; per-image TLS blocks initialized.\r\n");
        unsafe{rdotos_process_enter_user(image.entry,USER_STACK+PAGE,env.teb);}
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

/// Run Windows-numbered NtQueryInformationSystem/Process/Thread calls from
/// actual CPL3 code using the privileged SYSCALL instruction. Used when the
/// staged DLL is stock Windows NTDLL and therefore does not expose CognacOS's
/// custom syscall-table configuration export.
pub unsafe fn run_windows_syscall_abi_smoke(
    space: &mut AddressSpace,
    ntdll: LoadedImage,
) -> Result<(), VmError> {
    let service_query_system = crate::syscall::windows_service_number(b"NtQuerySystemInformation")
        .ok_or(VmError::InvalidAddress)?;
    let service_query_process = crate::syscall::windows_service_number(b"NtQueryInformationProcess")
        .ok_or(VmError::InvalidAddress)?;
    let service_query_thread = crate::syscall::windows_service_number(b"NtQueryInformationThread")
        .ok_or(VmError::InvalidAddress)?;
    let mut data = [0u8; 4096];
    data[0x3f0] = 66;
    data[0x3f1] = 65;
    let data_page = unsafe { crate::memory::allocate_page() }.map_err(|_| VmError::OutOfMemory)?;
    unsafe { space.map_user_page(NTDLL_TEST_DATA, data_page, true, false)?; }
    unsafe { space.copy_to_user(NTDLL_TEST_DATA, &data)?; }

    let mut code = [0u8; 1024];
    let mut length = 0usize;
    let mut failures = [0usize; 16];
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

    emit!(&[0x48, 0x83, 0xec, 0x30]); // Windows shadow space + fifth argument.
    emit!(&[0xb8, 2, 0, 0, 0, 0x31, 0xff]);
    imm64!(&[0x48, 0xbe], NTDLL_TEST_DATA + 0x3f0);
    emit!(&[0xba, 1, 0, 0, 0, 0xcd, 0x81]);
    // NtQuerySystemInformation(0, output, 64, &returnLength), service 0x36.
    imm64!(&[0x48, 0xb9], 0);
    imm64!(&[0x48, 0xba], NTDLL_TEST_DATA + 0x100);
    emit!(&[0x41, 0xb8, 64, 0, 0, 0]);
    imm64!(&[0x49, 0xb9], NTDLL_TEST_DATA + 0x180);
    emit!(&[0x4c, 0x8b, 0xd1, 0xb8]);
    emit!(&service_query_system.to_le_bytes());
    emit!(&[0x0f, 0x05, 0x85, 0xc0]);
    fail_if!(&[0x0f, 0x85]);
    emit!(&[0xb8, 2, 0, 0, 0, 0x31, 0xff]);
    imm64!(&[0x48, 0xbe], NTDLL_TEST_DATA + 0x3f1);
    emit!(&[0xba, 1, 0, 0, 0, 0xcd, 0x81]);
    check_dword!(NTDLL_TEST_DATA + 0x180, 64);
    check_dword!(NTDLL_TEST_DATA + 0x108, 4096);

    // NtQueryInformationProcess(-1, 0, output, 48, &returnLength), service 0x19.
    imm64!(&[0x48, 0xb9], u64::MAX);
    emit!(&[0xba, 0, 0, 0, 0]);
    imm64!(&[0x49, 0xb8], NTDLL_TEST_DATA + 0x200);
    emit!(&[0x41, 0xb9, 48, 0, 0, 0]);
    imm64!(&[0x48, 0xb8], NTDLL_TEST_DATA + 0x280);
    emit!(&[0x48, 0x89, 0x44, 0x24, 0x28]);
    emit!(&[0x4c, 0x8b, 0xd1, 0xb8]);
    emit!(&service_query_process.to_le_bytes());
    emit!(&[0x0f, 0x05, 0x85, 0xc0]);
    fail_if!(&[0x0f, 0x85]);
    check_dword!(NTDLL_TEST_DATA + 0x280, 48);

    // NtQueryInformationThread(-2, 0, output, 48, &returnLength), service 0x25.
    imm64!(&[0x48, 0xb9], u64::MAX - 1);
    emit!(&[0xba, 0, 0, 0, 0]);
    imm64!(&[0x49, 0xb8], NTDLL_TEST_DATA + 0x300);
    emit!(&[0x41, 0xb9, 48, 0, 0, 0]);
    imm64!(&[0x48, 0xb8], NTDLL_TEST_DATA + 0x380);
    emit!(&[0x48, 0x89, 0x44, 0x24, 0x28]);
    emit!(&[0x4c, 0x8b, 0xd1, 0xb8]);
    emit!(&service_query_thread.to_le_bytes());
    emit!(&[0x0f, 0x05, 0x85, 0xc0]);
    fail_if!(&[0x0f, 0x85]);
    check_dword!(NTDLL_TEST_DATA + 0x380, 48);

    // A bad output pointer must return STATUS_ACCESS_VIOLATION, not fault in
    // ring 0. Then an unknown Windows ordinal must fail closed as NOT_IMPLEMENTED.
    imm64!(&[0x48, 0xb9], 0);
    emit!(&[0xba, 1, 0, 0, 0]);
    emit!(&[0x41, 0xb8, 64, 0, 0, 0]);
    imm64!(&[0x49, 0xb9], NTDLL_TEST_DATA + 0x900);
    emit!(&[0x4c, 0x8b, 0xd1, 0xb8]);
    emit!(&service_query_system.to_le_bytes());
    emit!(&[0x0f, 0x05]);
    emit!(&[0x3d, 0x05, 0, 0, 0xc0]);
    fail_if!(&[0x0f, 0x85]);

    emit!(&[0xb8, 0xad, 0xde, 0, 0, 0x0f, 0x05]);
    emit!(&[0x3d, 0x02, 0, 0, 0xc0]);
    fail_if!(&[0x0f, 0x85]);

    emit!(&[0x31, 0xff, 0x31, 0xc0, 0xcd, 0x81, 0x0f, 0x0b]); // exit(0)
    let mut failure_labels = [0usize; 16];
    for index in 0..failure_count {
        failure_labels[index] = length;
        emit!(&[0xbf]);
        emit!(&((index as u32 + 1).to_le_bytes()));
        emit!(&[0x31, 0xc0, 0xcd, 0x81, 0x0f, 0x0b]);
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
    let test_image = LoadedImage { base: ntdll.base, entry: NTDLL_TEST_CODE, image_size: ntdll.image_size, is_dll: false };
    unsafe { run_loaded_image(space, test_image)?; }
    Ok(())
}

/// Load-and-execute smoke test for the freestanding CognacOS NTDLL. The DLL is
/// mapped and its real export table registered by pe::load_dll before this is
/// called. A stock Windows NTDLL lacks CognacNtdllConfigureSyscallTable and is
/// intentionally not executed as a whole image; the focused Windows ABI smoke
/// above exercises supported Windows service numbers in genuine ring-3 code.
pub unsafe fn run_native_ntdll_smoke(
    space: &mut AddressSpace,
    ntdll: LoadedImage,
) -> Result<bool, VmError> {
    use crate::pe::ImportSymbol;
    let configure = crate::winabi::resolve_import(
        b"ntdll.dll", ImportSymbol::Name(b"CognacNtdllConfigureSyscallTable"));
    let Some(configure) = configure else { return Ok(false); };
    let query_process = crate::winabi::resolve_import(
        b"ntdll.dll", ImportSymbol::Name(b"NtQueryInformationProcess"))
        .ok_or(VmError::InvalidAddress)?;
    let query_thread = crate::winabi::resolve_import(
        b"ntdll.dll", ImportSymbol::Name(b"NtQueryInformationThread"))
        .ok_or(VmError::InvalidAddress)?;
    let query_system = crate::winabi::resolve_import(
        b"ntdll.dll", ImportSymbol::Name(b"NtQuerySystemInformation"))
        .ok_or(VmError::InvalidAddress)?;
    let yield_execution = crate::winabi::resolve_import(
        b"ntdll.dll", ImportSymbol::Name(b"NtYieldExecution"))
        .ok_or(VmError::InvalidAddress)?;
    let rtl_init_unicode = crate::winabi::resolve_import(
        b"ntdll.dll", ImportSymbol::Name(b"RtlInitUnicodeString"))
        .ok_or(VmError::InvalidAddress)?;
    let rtl_compare_unicode = crate::winabi::resolve_import(
        b"ntdll.dll", ImportSymbol::Name(b"RtlCompareUnicodeString"))
        .ok_or(VmError::InvalidAddress)?;
    let rtl_copy_unicode = crate::winabi::resolve_import(
        b"ntdll.dll", ImportSymbol::Name(b"RtlCopyUnicodeString"))
        .ok_or(VmError::InvalidAddress)?;
    let rtl_unicode_to_integer = crate::winabi::resolve_import(
        b"ntdll.dll", ImportSymbol::Name(b"RtlUnicodeStringToInteger"))
        .ok_or(VmError::InvalidAddress)?;
    let rtl_integer_to_unicode = crate::winabi::resolve_import(
        b"ntdll.dll", ImportSymbol::Name(b"RtlIntegerToUnicodeString"))
        .ok_or(VmError::InvalidAddress)?;
    let rtl_init_ansi = crate::winabi::resolve_import(
        b"ntdll.dll", ImportSymbol::Name(b"RtlInitAnsiString"))
        .ok_or(VmError::InvalidAddress)?;
    let rtl_equal_ansi = crate::winabi::resolve_import(
        b"ntdll.dll", ImportSymbol::Name(b"RtlEqualString"))
        .ok_or(VmError::InvalidAddress)?;
    let rtl_get_nt_version = crate::winabi::resolve_import(
        b"ntdll.dll", ImportSymbol::Name(b"RtlGetNtVersionNumbers"))
        .ok_or(VmError::InvalidAddress)?;
    let rtl_status_to_dos = crate::winabi::resolve_import(
        b"ntdll.dll", ImportSymbol::Name(b"RtlNtStatusToDosError"))
        .ok_or(VmError::InvalidAddress)?;
    let rtl_compare_memory = crate::winabi::resolve_import(
        b"ntdll.dll", ImportSymbol::Name(b"RtlCompareMemory"))
        .ok_or(VmError::InvalidAddress)?;
    let rtl_compute_crc32 = crate::winabi::resolve_import(
        b"ntdll.dll", ImportSymbol::Name(b"RtlComputeCrc32"))
        .ok_or(VmError::InvalidAddress)?;
    let rtl_is_zero_memory = crate::winabi::resolve_import(
        b"ntdll.dll", ImportSymbol::Name(b"RtlIsZeroMemory"))
        .ok_or(VmError::InvalidAddress)?;
    let rtl_fill_memory = crate::winabi::resolve_import(
        b"ntdll.dll", ImportSymbol::Name(b"RtlFillMemory"))
        .ok_or(VmError::InvalidAddress)?;
    let rtl_zero_memory = crate::winabi::resolve_import(
        b"ntdll.dll", ImportSymbol::Name(b"RtlZeroMemory"))
        .ok_or(VmError::InvalidAddress)?;
    let rtl_move_memory = crate::winabi::resolve_import(
        b"ntdll.dll", ImportSymbol::Name(b"RtlMoveMemory"))
        .ok_or(VmError::InvalidAddress)?;
    let rtl_copy_memory = crate::winabi::resolve_import(
        b"ntdll.dll", ImportSymbol::Name(b"RtlCopyMemory"))
        .ok_or(VmError::InvalidAddress)?;
    let rtl_compare_memory_ulong = crate::winabi::resolve_import(
        b"ntdll.dll", ImportSymbol::Name(b"RtlCompareMemoryUlong"))
        .ok_or(VmError::InvalidAddress)?;
    let rtl_get_version = crate::winabi::resolve_import(
        b"ntdll.dll", ImportSymbol::Name(b"RtlGetVersion"))
        .ok_or(VmError::InvalidAddress)?;
    let rtl_get_process_heap = crate::winabi::resolve_import(
        b"ntdll.dll", ImportSymbol::Name(b"RtlGetProcessHeap"))
        .ok_or(VmError::InvalidAddress)?;
    let rtl_get_current_peb = crate::winabi::resolve_import(
        b"ntdll.dll", ImportSymbol::Name(b"RtlGetCurrentPeb"))
        .ok_or(VmError::InvalidAddress)?;
    let rtl_allocate_heap = crate::winabi::resolve_import(
        b"ntdll.dll", ImportSymbol::Name(b"RtlAllocateHeap"))
        .ok_or(VmError::InvalidAddress)?;
    let rtl_free_heap = crate::winabi::resolve_import(
        b"ntdll.dll", ImportSymbol::Name(b"RtlFreeHeap"))
        .ok_or(VmError::InvalidAddress)?;
    let rtl_reallocate_heap = crate::winabi::resolve_import(
        b"ntdll.dll", ImportSymbol::Name(b"RtlReAllocateHeap"))
        .ok_or(VmError::InvalidAddress)?;
    let rtl_size_heap = crate::winabi::resolve_import(
        b"ntdll.dll", ImportSymbol::Name(b"RtlSizeHeap"))
        .ok_or(VmError::InvalidAddress)?;
    let rtl_validate_heap = crate::winabi::resolve_import(
        b"ntdll.dll", ImportSymbol::Name(b"RtlValidateHeap"))
        .ok_or(VmError::InvalidAddress)?;
    let process_heap_object = crate::winabi::resolve_import(
        b"ntdll.dll", ImportSymbol::Name(b"CognacNtdllProcessHeapObject"))
        .ok_or(VmError::InvalidAddress)?;

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
    let numbers = [0x100u64, 0x101, 0x102, 0x103, 0x104];
    for (index, number) in numbers.iter().enumerate() {
        data[0x300 + index * 8..0x308 + index * 8].copy_from_slice(&number.to_le_bytes());
    }
    let data_page = unsafe { crate::memory::allocate_page() }.map_err(|_| VmError::OutOfMemory)?;
    unsafe { space.map_user_page(NTDLL_TEST_DATA, data_page, true, false)?; }
    unsafe { space.copy_to_user(NTDLL_TEST_DATA, &data)?; }

    let mut code = [0u8; 3072];
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
    emit!(&[0x48, 0x83, 0xec, 0x30]); // Shadow space plus fifth-argument slot; keeps RSP 16-byte aligned.
    imm64!(&[0x48, 0xb9], NTDLL_TEST_DATA + 0x300);
    emit!(&[0xba, 5, 0, 0, 0]);
    call_ptr!(configure);
    emit!(&[0x85, 0xc0]);
    fail_if!(&[0x0f, 0x85]);

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
    emit!(&[0x85, 0xc0]);
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

    // GS:[0x60] must expose the active process PEB and its ProcessHeap field.
    call_ptr!(rtl_get_current_peb);
    imm64!(&[0x48, 0xba], crate::process::PEB_ADDRESS);
    emit!(&[0x48, 0x39, 0xd0]);
    fail_if!(&[0x0f, 0x85]);
    imm64!(&[0x48, 0xb8], crate::process::PEB_ADDRESS);
    emit!(&[0x48, 0x8b, 0x40, 0x30]); // PEB.ProcessHeap
    imm64!(&[0x48, 0xba], process_heap_object);
    emit!(&[0x48, 0x39, 0xd0]);
    fail_if!(&[0x0f, 0x85]);

    // Default process heap: PEB handle, allocation, size, realloc, free and validation.
    call_ptr!(rtl_get_process_heap);
    imm64!(&[0x48, 0xba], process_heap_object);
    emit!(&[0x48, 0x39, 0xd0]); // returned heap handle must equal the exported PEB heap object
    fail_if!(&[0x0f, 0x85]);
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
    unsafe { run_loaded_image(space, test_image)?; }
    Ok(true)
}
