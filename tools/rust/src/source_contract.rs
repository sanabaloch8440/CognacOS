use std::{fs, path::{Path, PathBuf}};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn read(path: &str) -> Result<String, String> {
    let p = root().join(path);
    fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))
}
fn require(source: &str, needle: &str, errors: &mut Vec<String>) {
    if !source.contains(needle) { errors.push(format!("missing source contract: {needle}")); }
}
fn has_raw_bytes_autoref(source: &str, suffix: &str) -> bool {
    source.match_indices("(*").any(|(start, _)| {
        let tail = &source[start + 2..];
        let ident_len = tail.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').map(char::len_utf8).sum::<usize>();
        ident_len > 0 && tail[ident_len..].strip_prefix(')').is_some_and(|rest| rest.starts_with(suffix))
    })
}
fn report(name: &str, errors: Vec<String>) -> Result<(), String> {
    if errors.is_empty() { println!("{name} source-contract checks passed."); Ok(()) }
    else { Err(format!("{name} source-contract checks failed:\n - {}", errors.join("\n - "))) }
}
pub fn check_wdm() -> Result<(), String> {
    let wdm = read("kernel/src/ntoskrnl/io/wdm.rs")?;
    let runtime = read("kernel/src/ntoskrnl/io/runtime.rs")?;
    let exports = read("kernel/src/ntoskrnl/mod.rs")?;
    let manifest = read("kernel/src/ntoskrnl/manifest_26100.json")?;
    let mut errors = Vec::new();
    let lines: Vec<_> = wdm.lines().collect();
    for pair in lines.windows(2) {
        if pair[0].trim_start().starts_with("#[repr(") && pair[1].trim_start().starts_with("const ") {
            errors.push("repr attribute is incorrectly attached to a const item".into());
        }
    }
    require(&wdm, "#[repr(C)]\nstruct IrpSlot", &mut errors);
    for (suffix, label) in [(".bytes.as_mut_ptr()", "mutable"), (".bytes.as_ptr()", "immutable"), (".bytes[", "indexed")] {
        if has_raw_bytes_autoref(&wdm, suffix) { errors.push(format!("implicit raw-pointer autoref pattern remains ({label})")); }
    }
    for forbidden in ["MAX_WDM_IRPS", "MAX_IOCTL_SYSTEM_BUFFER"] {
        if wdm.contains(forbidden) { errors.push(format!("obsolete fixed WDM capacity remains: {forbidden}")); }
    }
    for required in [
        "struct IrpTable{head:*mut IrpSlot,len:usize}", "IRP_SLAB_PAGES:usize=256",
        "system_buffer:*mut u8", "thread_irps:[ThreadIrpQueue;MAX_SYNC_IRP_THREADS]",
        "sync_queue_attach_locked", "pub unsafe extern \"win64\" fn io_csq_initialize",
        "pub unsafe extern \"win64\" fn io_csq_initialize_ex", "pub unsafe extern \"win64\" fn io_csq_insert_irp",
        "pub unsafe extern \"win64\" fn io_csq_insert_irp_ex", "pub unsafe extern \"win64\" fn io_csq_remove_irp",
        "pub unsafe extern \"win64\" fn io_csq_remove_next_irp",
    ] { require(&wdm, required, &mut errors); }
    for symbol in [
        "IoAllocateIrp", "IoFreeIrp", "IoBuildDeviceIoControlRequest", "IoCallDriver",
        "IoCompleteRequest", "IoAcquireCancelSpinLock", "IoReleaseCancelSpinLock",
        "IoCsqInitialize", "IoCsqInitializeEx", "IoCsqInsertIrp", "IoCsqInsertIrpEx",
        "IoCsqRemoveIrp", "IoCsqRemoveNextIrp",
    ] {
        if !exports.contains(symbol) || !manifest.contains(&format!("\"name\": \"{symbol}\"")) {
            errors.push(format!("missing kernel export or manifest entry: {symbol}"));
        }
        let alias_source = if matches!(symbol, "IoAcquireCancelSpinLock" | "IoReleaseCancelSpinLock") { &runtime } else { &wdm };
        if !alias_source.contains(&format!("export_name = \"{symbol}\"")) {
            errors.push(format!("missing PE export alias for {symbol}"));
        }
    }
    report("WDM", errors)
}
pub fn check_xstate() -> Result<(), String> {
    let xstate = read("kernel/src/ntoskrnl/ke/runtime/xstate.rs")?;
    let address_space = read("kernel/src/ntoskrnl/mm/address_space.rs")?;
    let user_range = read("kernel/src/ntoskrnl/mm/user_range.rs")?;
    let process = read("kernel/src/process.rs")?;
    let syscall = read("kernel/src/syscall.rs")?;
    let arch = read("kernel/src/ntoskrnl/ke/arch.rs")?;
    let ntdll = read("ntdll/src/native/exception.rs")?;
    let mut errors = Vec::new();
    for (source, needle) in [
        (&xstate, "const _: [(); 0x348] = [(); core::mem::size_of::<WindowsXStateConfiguration>()]"),
        (&ntdll, "const _: [(); 0x348] = [(); core::mem::size_of::<WindowsXStateConfiguration>()]"),
        (&process, "KUSER_XSTATE_OFFSET: u64 = 0x3d8"),
        (&process, "KUSER_SHARED_DATA_ADDRESS: u64 = crate::ntoskrnl::mm::address_space::KUSER_SHARED_DATA_ADDRESS"),
        (&user_range, "KUSER_SHARED_DATA_ADDRESS: u64 = 0x0000_0000_7ffe_0000"),
        (&address_space, "pub unsafe fn map_kuser_shared_page"),
        (&process, "map_kuser_shared_page(space)?"),
        (&address_space, "record_kuser_tables(self.root, self.kuser_tables)"),
        (&address_space, "self.kuser_tables = [0; 3]"),
        (&xstate, "let enabled = XSTATE_X87 | XSTATE_SSE"),
        (&xstate, "enabled_supervisor_features: 0"),
        (&xstate, "struct XStateSaveRecord"),
        (&xstate, "core::mem::size_of::<XStateSaveRecord>()"),
        (&xstate, "const _: [(); 0x38] = [(); core::mem::size_of::<XStateSaveRecord>()]"),
        (&xstate, "valid_xstate_save_record(save)"),
        (&xstate, "mm_is_non_paged_system_address_writable"),
        (&xstate, "Do not clear this buffer with Rust before XSAVE"),
        (&xstate, "xstate_policy_compatible"),
        (&xstate, "ki_xstate_nested_save_restore_self_test"),
        (&ntdll, "RtlCopyContext"), (&ntdll, "RtlCopyExtendedContext"),
        (&ntdll, "RtlLocateExtendedFeature2"),
        (&ntdll, "config as *const WindowsXStateConfiguration).read_unaligned()"),
    ] { require(source, needle, &mut errors); }
    for (source, entry, dispatch, save, restore, ret) in [
        (&syscall, "cognacos_windows_syscall_entry:", "call cognacos_windows_syscall_dispatch", "call cognacos_save_current_user_xstate_asm", "call cognacos_current_user_xstate_ptr", "pop r15"),
        (&syscall, "rdotos_syscall_entry:", "call rdotos_syscall_dispatch", "call cognacos_save_current_user_xstate_asm", "call cognacos_current_user_xstate_ptr", "pop r15"),
        (&arch, "cognacos_exception_common:", "call cognacos_exception_dispatch", "call cognacos_save_current_user_xstate_asm", "call cognacos_current_user_xstate_ptr", "pop r15"),
        (&arch, "cognacos_interrupt_common:", "call cognacos_external_interrupt_dispatch", "call cognacos_save_current_user_xstate_asm", "call cognacos_current_user_xstate_ptr", "pop r15"),
    ] {
        match source.find(entry).map(|start| {
            let tail = &source[start..];
            let end = tail.find("\"#);").unwrap_or(tail.len());
            &tail[..end]
        }) {
            Some(body) => {
                let positions = [save, dispatch, restore, ret].map(|needle| body.find(needle));
                if positions.iter().any(Option::is_none) ||
                    !(positions[0].unwrap() < positions[1].unwrap() &&
                      positions[1].unwrap() < positions[2].unwrap() &&
                      positions[2].unwrap() < positions[3].unwrap()) {
                    errors.push(format!("{entry}: user XSTATE save/dispatch/restore/return order is invalid"));
                }
            }
            None => errors.push(format!("missing assembly entry {entry}")),
        }
    }
    report("XSTATE", errors)
}
