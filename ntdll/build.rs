use std::{collections::BTreeSet, env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=../root/wine/dlls/ntdll/ntdll.spec");
    println!("cargo:rerun-if-changed=exports/windows-11-x64-exports.txt");
    println!("cargo:rerun-if-changed=src");
    let spec = PathBuf::from("../root/wine/dlls/ntdll/ntdll.spec");
    let mut names = BTreeSet::new();
    if let Ok(source) = fs::read_to_string(&spec) {
        for line in source.lines() {
            let line = line.trim();
            if !line.starts_with('@') || line.starts_with("#") { continue; }
            let mut tokens = line[1..].split_whitespace();
            let Some(kind) = tokens.next() else { continue };
            if !matches!(kind, "stdcall" | "cdecl" | "fastcall" | "thiscall" | "varargs" | "stub") { continue; }
            let mut name = None;
            for token in tokens {
                if token.starts_with('-') { continue; }
                name = Some(token.split('(').next().unwrap_or(token).trim());
                break;
            }
            if let Some(name) = name {
                if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    names.insert(name.to_string());
                }
            }
        }
    }
    // Keep Wine spec names as a compatibility superset for exports not in the committed
    // Windows 11 source surface. The Windows inventory is already implemented in
    // src/windows11_exports.rs and is used here to suppress duplicate build stubs.
    let windows_inventory = PathBuf::from("exports/windows-11-x64-exports.txt");
    if let Ok(source) = fs::read_to_string(&windows_inventory) {
        for line in source.lines() {
            let name = line.trim();
            if name.is_empty() || name.starts_with('#') { continue; }
            if name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                names.insert(name.to_string());
            }
        }
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
        "NtAlpcCreatePort", "NtAlpcDisconnectPort", "NtAlpcConnectPort",
        "NtAlpcAcceptConnectPort", "NtAlpcSendWaitReceivePort", "NtAlpcCreatePortSection",
        "NtAlpcDeletePortSection", "NtAlpcCreateSectionView", "NtAlpcDeleteSectionView",
        "NtAlpcCreateSecurityContext", "NtAlpcDeleteSecurityContext", "NtAlpcRevokeSecurityContext",
        "NtAlpcCreateResourceReserve", "NtAlpcDeleteResourceReserve", "NtAlpcQueryInformation",
        "NtAlpcSetInformation", "NtAlpcQueryInformationMessage", "NtAlpcCancelMessage",
        "NtAlpcOpenSenderProcess", "NtAlpcOpenSenderThread", "NtAlpcImpersonateClientOfPort",
        "NtAlpcImpersonateClientContainerOfPort",
    ] { declared.insert(name.to_string()); }
    let mut output = String::from("// Generated from the committed Windows 11 x64 export inventory plus the in-tree Wine ntdll.spec.\n// Generic ABI placeholders only: no parameter semantics are implemented.\n");
    let mut count = 0usize;
    for name in names {
        // This ABI implementation must never be shadowed by a generated stub.
        if matches!(name.as_str(), "RtlAddFunctionTable" | "RtlGetNonVolatileToken" | "RtlFreeNonVolatileToken") { continue; }
        if declared.contains(&name) { continue; }
        output.push_str(&format!("#[no_mangle]\npub unsafe extern \"system\" fn {name}() -> i32 {{ 0xC0000002u32 as i32 }}\n"));
        count += 1;
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    fs::write(out.join("ntdll_export_stubs.rs"), output).expect("write generated NTDLL stubs");
    println!("cargo:warning=generated {count} missing NTDLL ABI placeholder exports from committed Windows 11 inventory + Wine spec");
}
