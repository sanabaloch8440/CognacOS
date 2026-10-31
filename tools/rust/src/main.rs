use serde_json::Value;
use std::{collections::{BTreeMap, BTreeSet}, env, fs, path::{Path, PathBuf}, process};

macro_rules! fail {
    ($($arg:tt)*) => {{
        eprintln!("cognacos-tools: {}", format!($($arg)*));
        process::exit(1)
    }};
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| fail!("{}: {e}", path.display()))
}

fn validate_manifest(path: &Path) {
    let value: Value = serde_json::from_str(&read(path)).unwrap_or_else(|e| fail!("{}: invalid JSON: {e}", path.display()));
    let exports = value["exports"].as_array().unwrap_or_else(|| fail!("manifest exports must be an array"));
    let counts = &value["counts"];
    let eat = counts["eat_entries"].as_u64().unwrap_or_else(|| fail!("counts.eat_entries missing"));
    let named = counts["named_exports"].as_u64().unwrap_or_else(|| fail!("counts.named_exports missing"));
    let ordinal_only = counts["ordinal_only_exports"].as_u64().unwrap_or_else(|| fail!("counts.ordinal_only_exports missing"));
    if exports.len() as u64 != eat { fail!("EAT count mismatch: actual {}, expected {eat}", exports.len()); }
    let mut names = BTreeSet::new();
    let mut named_count = 0u64;
    let mut ordinal_count = 0u64;
    for (index, entry) in exports.iter().enumerate() {
        if entry["ordinal"].as_u64() != Some(index as u64 + 1) {
            fail!("EAT slot {} has non-contiguous ordinal", index + 1);
        }
        match entry["name"].as_str() {
            Some(name) => {
                named_count += 1;
                if !names.insert(name) { fail!("duplicate named export: {name}"); }
            }
            None => ordinal_count += 1,
        }
    }
    if named_count != named || ordinal_count != ordinal_only {
        fail!("manifest count mismatch: named {named_count}/{named}, ordinal-only {ordinal_count}/{ordinal_only}");
    }
    let version = value["source"]["version"].as_str().unwrap_or("unknown");
    println!("validated {} EAT slots: {} named, {} ordinal-only (source {})", exports.len(), named_count, ordinal_count, version);
}

fn check_ntdll_def(path: &Path) {
    let source = read(path);
    let mut library = false;
    let mut exports_section = false;
    let mut names = BTreeSet::new();
    let mut count = 0usize;
    for (line_no, raw) in source.lines().enumerate() {
        let line = raw.split(';').next().unwrap_or("").trim();
        if line.eq_ignore_ascii_case("LIBRARY ntdll") { library = true; continue; }
        if line.eq_ignore_ascii_case("EXPORTS") { exports_section = true; continue; }
        if line.is_empty() || !exports_section { continue; }
        let name = line.split_whitespace().next().unwrap();
        if name.starts_with('@') { continue; }
        if !names.insert(name.to_owned()) { fail!("{}:{} duplicate export {name}", path.display(), line_no + 1); }
        count += 1;
    }
    if !library { fail!("{}: missing LIBRARY ntdll", path.display()); }
    if !exports_section { fail!("{}: missing EXPORTS section", path.display()); }
    let required = ["NtClose", "NtOpenKey", "NtQueryValueKey", "NtYieldExecution",
        "NtQueryInformationProcess", "NtQueryInformationThread", "NtQuerySystemInformation",
        "RtlGetVersion", "RtlInitUnicodeString", "RtlAllocateHeap", "LdrGetProcedureAddress"];
    let missing: Vec<_> = required.iter().filter(|name| !names.contains(**name)).copied().collect();
    if !missing.is_empty() { fail!("missing required named exports: {}", missing.join(", ")); }
    println!("validated {count} unique named NTDLL exports");
}

fn check_csrsrv_def(path: &Path) {
    let source = read(path);
    let mut exports = BTreeMap::<String, u32>::new();
    for (line_no, raw) in source.lines().enumerate() {
        let line = raw.split(';').next().unwrap_or("").trim();
        if line.is_empty() || line.to_ascii_uppercase().starts_with("LIBRARY ") || line.eq_ignore_ascii_case("EXPORTS") { continue; }
        let mut fields = line.split_whitespace();
        let name = fields.next().unwrap().to_owned();
        let ordinal = fields.filter_map(|field| field.strip_prefix('@')).find_map(|v| v.parse::<u32>().ok())
            .unwrap_or_else(|| fail!("{}:{} missing ordinal for {name}", path.display(), line_no + 1));
        if exports.insert(name.clone(), ordinal).is_some() { fail!("{}:{} duplicate export {name}", path.display(), line_no + 1); }
    }
    if exports.is_empty() { fail!("{}: no exports found", path.display()); }
    println!("validated {} unique named CSRSRV exports (inventory-only)", exports.len());
}

mod dll_exports;
mod elf_pe;
mod pe_exports;
mod source_contract;

fn main() {
    let mut args = env::args_os().skip(1);
    let command = args.next().unwrap_or_else(|| fail!("usage: cognacos-tools <validate-ntoskrnl-manifest|check-ntdll-def|check-csrsrv-def|elf-to-pe32plus|validate-pe-exports|check-wdm-source-contracts|check-xstate-source-contracts|verify-dll-exports> [arguments]"));
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    match command.to_string_lossy().as_ref() {
        "validate-ntoskrnl-manifest" => validate_manifest(&args.next().map(PathBuf::from).unwrap_or(root.join("kernel/src/ntoskrnl/manifest_19041.json"))),
        "check-ntdll-def" => check_ntdll_def(&args.next().map(PathBuf::from).unwrap_or(root.join("ntdll/exports/ntdll-x64.def"))),
        "check-csrsrv-def" => check_csrsrv_def(&args.next().map(PathBuf::from).unwrap_or(root.join("csrss/exports/csrsrv-x64.def"))),
        "check-wdm-source-contracts" => source_contract::check_wdm().unwrap_or_else(|e| fail!("{e}")),
        "check-xstate-source-contracts" => source_contract::check_xstate().unwrap_or_else(|e| fail!("{e}")),
        "verify-dll-exports" => {\n            let specs: Vec<String> = args.collect::<Vec<_>>().into_iter().map(|x| x.to_string_lossy().into_owned()).collect();\n            dll_exports::run(&specs).unwrap_or_else(|e| fail!("DLL export verification failed: {e}"));\n        }\n        "validate-pe-exports" => {
            let pe = args.next().map(PathBuf::from).unwrap_or_else(|| fail!("missing PE image"));
            let manifest = args.next().map(PathBuf::from).unwrap_or_else(|| fail!("missing export manifest"));
            let required: Vec<String> = args.map(|x| x.to_string_lossy().into_owned()).collect();
            pe_exports::validate(&pe, &manifest, &required).unwrap_or_else(|e| fail!("PE export validation failed: {e}"));
        }
        "elf-to-pe32plus" => {
            let input = args.next().map(PathBuf::from).unwrap_or_else(|| fail!("missing ELF input"));
            let output = args.next().map(PathBuf::from).unwrap_or_else(|| fail!("missing PE output"));
            let mut image_base = elf_pe::BASE;
            let mut dll = false;
            for arg in args {
                let value = arg.to_string_lossy();
                if value == "--dll" { dll = true; }
                else if let Some(hex) = value.strip_prefix("--image-base=") {
                    image_base = u64::from_str_radix(hex.trim_start_matches("0x"), 16).unwrap_or_else(|_| fail!("invalid image base"));
                } else { fail!("unknown converter option {value}"); }
            }
            elf_pe::convert_file(&input, &output, image_base, dll).unwrap_or_else(|e| fail!("ELF-to-PE conversion failed: {e}"));
            println!("PE32+ image written: {}", output.display());
        }
        other => fail!("unknown command {other}"),
    }
}
