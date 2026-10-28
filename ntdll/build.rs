use std::{collections::BTreeSet, env, fs, path::PathBuf};

fn prototype_argument_count(source: &str, name: &str) -> Option<usize> {
    let marker = format!("pub fn {name}(");
    let start = source.find(&marker)?.checked_add(marker.len())?;
    let mut depth = 1usize;
    let mut commas = 0usize;
    let mut has_parameter = false;
    for ch in source[start..].chars() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 { return Some(if has_parameter { commas + 1 } else { 0 }); }
            }
            ',' if depth == 1 => commas += 1,
            c if depth == 1 && !c.is_whitespace() => has_parameter = true,
            _ => {}
        }
    }
    None
}

fn main() {
    println!("cargo:rerun-if-changed=exports/windows-11-x64-exports.txt");
    println!("cargo:rerun-if-changed=exports/ntdll-x64.def");
    println!("cargo:rerun-if-changed=exports/ntdll-rust.def");
    println!("cargo:rerun-if-changed=src");
    // The public NTDLL export surface comes exclusively from the pinned Windows 11
    // inventory committed with CognacOS; generated stubs are not sourced from
    // third-party compatibility-layer metadata.
    let mut names = BTreeSet::new();
    let windows_inventory = PathBuf::from("exports/windows-11-x64-exports.txt");
    let source = fs::read_to_string(&windows_inventory)
        .expect("missing pinned Windows 11 x64 NTDLL export inventory");
    for line in source.lines() {
        let name = line.trim();
        if name.is_empty() || name.starts_with('#') { continue; }
        if name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            names.insert(name.to_string());
        } else {
            panic!("invalid name in Windows 11 NTDLL export inventory: {name}");
        }
    }
    // Both PE definition files must remain subsets of the pinned inventory.
    for definition in ["exports/ntdll-x64.def", "exports/ntdll-rust.def"] {
        let text = fs::read_to_string(definition)
            .unwrap_or_else(|error| panic!("cannot read {}: {}", definition, error));
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') || line.starts_with("LIBRARY ") || line == "EXPORTS" {
                continue;
            }
            let name = line.split_whitespace().next().unwrap_or("");
            if !names.contains(name) {
                panic!("{} exports `{}`, which is absent from the pinned Windows 11 inventory", definition, name);
            }
        }
    }

    // Any Rust item explicitly given an unmangled symbol name is a candidate
    // DLL export. Reject helpers and Cognac-only names here, before they can
    // silently widen the Windows ABI surface.
    fn audit_exports(path: &std::path::Path, inventory: &BTreeSet<String>, errors: &mut Vec<String>) {
        let Ok(entries) = fs::read_dir(path) else { return };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                audit_exports(&p, inventory, errors);
                continue;
            }
            if p.extension().and_then(|e| e.to_str()) != Some("rs") { continue; }
            let Ok(src) = fs::read_to_string(&p) else { continue };
            let lines: Vec<&str> = src.lines().collect();
            for (index, line) in lines.iter().enumerate() {
                let Some(after_attribute) = line.split("#[no_mangle]").nth(1) else { continue };
                let mut candidates: Vec<&str> = Vec::new();
                if !after_attribute.trim().is_empty() {
                    candidates.push(after_attribute.trim());
                }
                candidates.extend(lines.iter().skip(index + 1).take(12).copied());
                for candidate in candidates {
                    let candidate = candidate.trim();
                    let declaration = candidate
                        .find("fn ")
                        .map(|pos| &candidate[pos + 3..])
                        .or_else(|| candidate.find("static mut ").map(|pos| &candidate[pos + 11..]))
                        .or_else(|| candidate.find("static ").map(|pos| &candidate[pos + 7..]));
                    if let Some(declaration) = declaration {
                        let name: String = declaration.chars()
                            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                            .collect();
                        if !name.is_empty() && !inventory.contains(&name) {
                            errors.push(format!("{}:{}: non-Windows NTDLL symbol `{}` has #[no_mangle]", p.display(), index + 1, name));
                        }
                        break;
                    }
                }
            }
        }
    }
    let mut export_errors = Vec::new();
    audit_exports(std::path::Path::new("src"), &names, &mut export_errors);
    if !export_errors.is_empty() {
        panic!("NTDLL export inventory audit failed:\n{}", export_errors.join("\n"));
    }

    // Avoid collisions with any Rust declaration already present in this crate.
    let mut declared = BTreeSet::new();
    fn scan(path: &std::path::Path, declared: &mut BTreeSet<String>) {
        let Ok(entries) = fs::read_dir(path) else { return };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() { scan(&p, declared); continue; }
            if p.extension().and_then(|e| e.to_str()) != Some("rs") { continue; }
            let Ok(src) = fs::read_to_string(p) else { continue };
            let lines: Vec<&str> = src.lines().collect();
            let mut pending_no_mangle = 0usize;
            let mut in_extern_block = false;
            for line in lines {
                let line = line.trim();
                if line.contains("#[no_mangle]") {
                    pending_no_mangle = 12;
                }
                if line.starts_with("unsafe extern ") || line.starts_with("extern \"system\" {")
                    || line.starts_with("extern \"C\" {") {
                    in_extern_block = line.contains('{');
                    continue;
                }
                if in_extern_block && line.starts_with('}') {
                    in_extern_block = false;
                    continue;
                }
                if pending_no_mangle > 0 || in_extern_block {
                    for marker in ["fn ", "static mut ", "static "] {
                        if let Some(pos) = line.find(marker) {
                            let rest = &line[pos + marker.len()..];
                            let ident: String = rest.chars()
                                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                                .collect();
                            if !ident.is_empty() { declared.insert(ident); }
                            pending_no_mangle = 0;
                            break;
                        }
                    }
                }
                pending_no_mangle = pending_no_mangle.saturating_sub(1);
            }
        }
    }
    scan(std::path::Path::new("src"), &mut declared);
    // Explicitly protect the unwind-table ABI implementation from generated export stubs.
    declared.insert("RtlAddFunctionTable".to_string());
    // These APIs are implemented in native/memory.rs and must not be shadowed.
    declared.insert("RtlGetNonVolatileToken".to_string());
    declared.insert("RtlFreeNonVolatileToken".to_string());
    declared.insert("NtCreateSection".to_string());
    // ALPC has explicit x64 ABI declarations and assembly entry points in alpc.rs.
    // Keep the generated zero-argument fallback exports from shadowing them.
    for name in [
        "AlpcAdjustCompletionListConcurrencyCount", "AlpcFreeCompletionListMessage",
        "AlpcGetCompletionListLastMessageInformation", "AlpcGetCompletionListMessageAttributes",
        "AlpcGetHeaderSize", "AlpcGetMessageAttribute", "AlpcGetMessageFromCompletionList",
        "AlpcGetOutstandingCompletionListMessageCount", "AlpcInitializeMessageAttribute",
        "AlpcMaxAllowedMessageLength", "AlpcRegisterCompletionList",
        "AlpcRegisterCompletionListWorkerThread", "AlpcRundownCompletionList",
        "AlpcUnregisterCompletionList", "AlpcUnregisterCompletionListWorkerThread",
        "NtAlpcCreatePort", "NtAlpcDisconnectPort", "NtAlpcConnectPort", "NtAlpcConnectPortEx",
        "NtAlpcAcceptConnectPort", "NtAlpcSendWaitReceivePort", "NtAlpcCreatePortSection",
        "NtAlpcDeletePortSection", "NtAlpcCreateSectionView", "NtAlpcDeleteSectionView",
        "NtAlpcCreateSecurityContext", "NtAlpcDeleteSecurityContext", "NtAlpcRevokeSecurityContext",
        "NtAlpcCreateResourceReserve", "NtAlpcDeleteResourceReserve", "NtAlpcQueryInformation",
        "NtAlpcSetInformation", "NtAlpcQueryInformationMessage", "NtAlpcCancelMessage",
        "NtAlpcOpenSenderProcess", "NtAlpcOpenSenderThread", "NtAlpcImpersonateClientOfPort",
        "NtAlpcImpersonateClientContainerOfPort",
    ] { declared.insert(name.to_string()); }
    // The six formatter exports are implemented by x64 assembly entry shims in printf.rs.
    // Prevent generated zero-argument stubs from colliding with those ABI entry points.
    for name in ["_vscprintf", "_vscwprintf", "sprintf", "sprintf_s", "swprintf", "swprintf_s"] {
        declared.insert(name.to_string());
    }
    names.remove("NtCreateSection");
    let mut output = String::from("// Generated exclusively from the committed Windows 11 x64 export inventory.\n// Generic ABI placeholders only: no parameter semantics are implemented.\n");
    let mut count = 0usize;
    for name in names {
        // This ABI implementation must never be shadowed by a generated stub.
        if matches!(name.as_str(), "RtlAddFunctionTable" | "RtlGetNonVolatileToken" | "RtlFreeNonVolatileToken" | "NtCreateSection") { continue; }
        if declared.contains(&name) { continue; }
        output.push_str(&format!("#[no_mangle]\npub unsafe extern \"system\" fn {name}() -> i32 {{ 0xC0000002u32 as i32 }}\n"));
        count += 1;
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    fs::write(out.join("ntdll_export_stubs.rs"), output).expect("write generated NTDLL stubs");
    println!("cargo:warning=generated {count} missing NTDLL ABI placeholder exports from the pinned Windows 11 inventory");

    // ReactOS-inspired, definition-driven syscall stub generation. The manifest
    // is the single source of truth for the currently implemented CognacOS
    // service ordinals from a pinned Windows 11 AMD64 reference image. These
    // ordinals are build-specific; the kernel discovers them from the built PE
    // stubs and translates them to stable internal handlers.
    let syscall_manifest = PathBuf::from("syscalls/x86_64.syscalls");
    println!("cargo:rerun-if-changed={}", syscall_manifest.display());
    let syscall_source = fs::read_to_string(&syscall_manifest)
        .expect("missing x86_64 syscall manifest");
    let mut syscall_names = BTreeSet::new();
    let mut syscall_ids = BTreeSet::new();
    let mut asm = String::from("# Generated by ntdll/build.rs from syscalls/x86_64.syscalls. Do not edit.\n.text\n");
    let declarations = fs::read_to_string("src/native/syscall_stubs.rs")
        .expect("cannot read canonical syscall declarations");
    for (line_index, raw_line) in syscall_source.lines().enumerate() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() { continue; }
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() != 3 {
            panic!("{}:{}: expected NtName, service_id, and argument_count", syscall_manifest.display(), line_index + 1);
        }
        let name = fields[0];
        if !name.starts_with("Nt") || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            panic!("{}:{}: invalid native syscall name {}", syscall_manifest.display(), line_index + 1, name);
        }
        let id = fields[1].strip_prefix("0x")
            .and_then(|v| u32::from_str_radix(v, 16).ok())
            .or_else(|| fields[1].parse::<u32>().ok())
            .unwrap_or_else(|| panic!("{}:{}: invalid service ID {}", syscall_manifest.display(), line_index + 1, fields[1]));
        if !syscall_names.insert(name.to_string()) {
            panic!("{}:{}: duplicate syscall name {}", syscall_manifest.display(), line_index + 1, name);
        }
        if id > 0x0fff {
            panic!("{}:{}: x64 NT service ID {:#x} exceeds the supported native service range", syscall_manifest.display(), line_index + 1, id);
        }
        if !syscall_ids.insert(id) {
            panic!("{}:{}: duplicate syscall service ID {:#x}", syscall_manifest.display(), line_index + 1, id);
        }
        let expected_arguments = fields[2].parse::<usize>()
            .unwrap_or_else(|_| panic!("{}:{}: invalid argument count {}", syscall_manifest.display(), line_index + 1, fields[2]));
        if expected_arguments > 17 {
            panic!("{}:{}: unsupported x64 native argument count {}", syscall_manifest.display(), line_index + 1, expected_arguments);
        }
        let actual_arguments = prototype_argument_count(&declarations, name)
            .unwrap_or_else(|| panic!("{}:{}: manifest entry {} has no parseable ABI declaration", syscall_manifest.display(), line_index + 1, name));
        if actual_arguments != expected_arguments {
            panic!("{}:{}: {} declares {} arguments but manifest records {}", syscall_manifest.display(), line_index + 1, name, actual_arguments, expected_arguments);
        }
        asm.push_str(&format!("\n.p2align 4\n.globl {name}\n{name}:\n    .byte 0x4c, 0x8b, 0xd1\n    movl ${:#x}, %eax\n    syscall\n    ret\n", id));
    }
    if syscall_names.len() != 30 {
        panic!("syscall manifest has {} entries; the kernel currently requires exactly 30 native services", syscall_names.len());
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    fs::write(out.join("ntdll_syscall_stubs.S"), asm)
        .expect("write generated x86_64 NTDLL syscall stubs");
}
