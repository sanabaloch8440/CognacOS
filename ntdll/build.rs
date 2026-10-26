use std::{collections::BTreeSet, env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=exports/windows-11-x64-exports.txt");
    println!("cargo:rerun-if-changed=src");
    // The public NTDLL export surface comes exclusively from the pinned Windows 11
    // inventory. The Wine .spec is metadata for the separate Wine tree, not an
    // authority for CognacOS's native DLL exports.
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
                if line.trim() != "#[no_mangle]" { continue; }
                for candidate in lines.iter().skip(index + 1).take(12) {
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
            for line in src.lines() {
                let line = line.trim();
                for marker in ["fn ", "static mut ", "static "] {
                    if let Some(pos) = line.find(marker) {
                        let rest = &line[pos + marker.len()..];
                        let ident: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
                        if !ident.is_empty() { declared.insert(ident); }
                    }
                }
            }
        }
    }
    scan(std::path::Path::new("src"), &mut declared);
    // Explicitly protect the unwind-table ABI implementation from generated export stubs.
    declared.insert("RtlAddFunctionTable".to_string());
    // These APIs are implemented in windows11_exports/memory.rs and must not be shadowed.
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
}
