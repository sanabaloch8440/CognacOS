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

fn update_windows11_exports(check: bool) {
    const URL: &str = "https://raw.githubusercontent.com/learn-more/winexports/master/data/exports/nt100_x64_26200/ntdll.dll.json";
    let output = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ntdll/exports/windows-11-x64-exports.txt");
    let fetched = std::process::Command::new("curl").args(["-fsSL", "-A", "CognacOS-export-audit/1.0", URL]).output()
        .unwrap_or_else(|e| fail!("could not run curl to refresh Windows export inventory: {e}"));
    if !fetched.status.success() { fail!("download of Windows export reference failed: {}", String::from_utf8_lossy(&fetched.stderr)); }
    let data: Value = serde_json::from_slice(&fetched.stdout).unwrap_or_else(|e| fail!("invalid export reference JSON: {e}"));
    let mut names = BTreeSet::new();
    for entry in data["exports"].as_array().unwrap_or_else(|| fail!("reference JSON has no exports array")) {
        if let Some(name) = entry["name"].as_str().filter(|s| !s.is_empty()) { names.insert(name.to_owned()); }
    }
    if names.len() < 2000 { fail!("refusing suspiciously small Windows export inventory ({} names)", names.len()); }
    let mut sorted: Vec<_> = names.into_iter().collect();
    sorted.sort_by(|a,b| a.to_lowercase().cmp(&b.to_lowercase()).then_with(|| a.cmp(b)));
    let version = data["FileVersion"].as_str().unwrap_or("unknown");
    let mut rendered = format!("# Windows 11 x64 ntdll.dll named exports\n# Reference: {URL}\n# Reference dataset: nt100_x64_26200; PE file version {version} (AMD64).\n# Named exports only: {} entries. Ordinal-only slots are not represented.\n# Inventory only; this is not an ABI-signature database.\n", sorted.len());
    for name in &sorted { rendered.push_str(name); rendered.push('\n'); }
    if check {
        let existing = fs::read_to_string(&output).unwrap_or_default();
        if existing != rendered { fail!("Windows 11 export inventory is stale; run cognacos-tools update-windows11-exports"); }
        println!("Windows 11 NTDLL inventory is current ({} named exports)", sorted.len());
    } else {
        fs::write(&output, rendered).unwrap_or_else(|e| fail!("{}: {e}", output.display()));
        println!("Wrote {} named exports to {}", sorted.len(), output.display());
    }
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

fn check_csrsrv_def(path: &Path, reference: Option<&Path>) {
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
    if let Some(reference) = reference {
        let actual = dll_exports::named_exports(reference).unwrap_or_else(|e| fail!("{}: {e}", reference.display()));
        let missing: Vec<_> = actual.keys().filter(|n| !exports.contains_key(*n)).cloned().collect();
        let extra: Vec<_> = exports.keys().filter(|n| !actual.contains_key(*n)).cloned().collect();
        let ordinal: Vec<_> = exports.iter().filter_map(|(n,want)| actual.get(n).filter(|got| *got != want).map(|got| format!("{n}: definition @{want}, reference @{got}"))).collect();
        if !missing.is_empty() || !extra.is_empty() || !ordinal.is_empty() {
            fail!("CSRSRV export mismatch; missing={missing:?}, extra={extra:?}, ordinal={ordinal:?}");
        }
        println!("exact CSRSRV PE export name/ordinal match: {} exports", exports.len());
    } else {
        println!("validated {} unique named CSRSRV exports (inventory-only; pass --reference for Windows-build parity)", exports.len());
    }
}

mod dll_exports;
mod elf_pe;
mod pe_exports;
mod qemu_trace;
mod qemu_ps2;
mod source_contract;
mod windows_exports;
mod reg_hive;

fn main() {
    let mut args = env::args_os().skip(1);
    let command = args.next().unwrap_or_else(|| fail!("usage: cognacos-tools <validate-ntoskrnl-manifest|check-ntdll-def|check-csrsrv-def|elf-to-pe32plus|validate-pe-exports|check-wdm-source-contracts|check-xstate-source-contracts|verify-dll-exports|verify-windows-exports|resolve-qemu-trace|qemu-ps2-smoke|reg-hive> [arguments]"));
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    match command.to_string_lossy().as_ref() {
        "validate-ntoskrnl-manifest" => validate_manifest(&args.next().map(PathBuf::from).unwrap_or(root.join("kernel/src/ntoskrnl/manifest_19041.json"))),
        "reg-hive" => {
            let input = args.next().map(PathBuf::from).unwrap_or_else(|| root.join("registry/drivers/CognacOS-Drivers.reg"));
            let output = args.next().map(PathBuf::from).unwrap_or_else(|| root.join("build/registry/SYSTEM"));
            if args.next().is_some() { fail!("usage: cognacos-tools reg-hive [input.reg] [output-hive]"); }
            reg_hive::convert_file(&input, &output).unwrap_or_else(|e| fail!("{e}"));
        },
        "update-windows11-exports" => { let check = args.any(|x| x == "--check"); update_windows11_exports(check); },
        "check-ntdll-def" => check_ntdll_def(&args.next().map(PathBuf::from).unwrap_or(root.join("ntdll/exports/ntdll-x64.def"))),
        "check-csrsrv-def" => {
            let definition = args.next().map(PathBuf::from).unwrap_or(root.join("csrss/exports/csrsrv-x64.def"));
            let mut reference = None;
            while let Some(arg) = args.next() {
                if arg == "--reference" { reference = Some(args.next().map(PathBuf::from).unwrap_or_else(|| fail!("--reference requires a PE path"))); }
                else { fail!("unknown CSRSRV option {}", arg.to_string_lossy()); }
            }
            check_csrsrv_def(&definition, reference.as_deref());
        },
        "check-wdm-source-contracts" => source_contract::check_wdm().unwrap_or_else(|e| fail!("{e}")),
        "check-xstate-source-contracts" => source_contract::check_xstate().unwrap_or_else(|e| fail!("{e}")),
        "qemu-ps2-smoke" => {
            let values: Vec<String> = args.map(|x| x.to_string_lossy().into_owned()).collect();
            qemu_ps2::run(&values).unwrap_or_else(|e| fail!("PS/2 QEMU smoke failed: {e}"));
        }
        "resolve-qemu-trace" => {
            let values: Vec<String> = args.map(|x| x.to_string_lossy().into_owned()).collect();
            qemu_trace::run(&values).unwrap_or_else(|e| fail!("QEMU trace resolution failed: {e}"));
        }
        "verify-windows-exports" => windows_exports::run().unwrap_or_else(|e| fail!("Windows export baseline verification failed: {e}")),
        "verify-dll-exports" => {
            let specs: Vec<String> = args.collect::<Vec<_>>().into_iter().map(|x| x.to_string_lossy().into_owned()).collect();
            dll_exports::run(&specs).unwrap_or_else(|e| fail!("DLL export verification failed: {e}"));
        }
        "validate-pe-exports" => {
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
            let mut export_manifest: Option<PathBuf> = None;
            while let Some(arg) = args.next() {
                let value = arg.to_string_lossy();
                if value == "--dll" { dll = true; }
                else if let Some(hex) = value.strip_prefix("--image-base=") {
                    image_base = u64::from_str_radix(hex.trim_start_matches("0x"), 16).unwrap_or_else(|_| fail!("invalid image base"));
                } else if value == "--export-manifest" {
                    export_manifest = Some(args.next().map(PathBuf::from).unwrap_or_else(|| fail!("--export-manifest requires a path")));
                } else { fail!("unknown converter option {value}"); }
            }
            elf_pe::convert_file(&input, &output, image_base, dll, export_manifest.as_deref()).unwrap_or_else(|e| fail!("ELF-to-PE conversion failed: {e}"));
            println!("PE32+ image written: {}", output.display());
        }
        other => fail!("unknown command {other}"),
    }
}
