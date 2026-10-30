use std::{collections::BTreeSet, env, fs, path::PathBuf};

#[derive(Debug)] struct Export { ordinal: u32, name: String }

fn read_exports(path: &str) -> Vec<Export> {
    println!("cargo:rerun-if-changed={path}");
    let raw = fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    let mut exports = Vec::new(); let mut ordinals = BTreeSet::new(); let mut names = BTreeSet::new();
    for (line_no, line) in raw.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') { continue; }
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 3, "{path}:{} expected ordinal<TAB>name<TAB>forwarder", line_no + 1);
        let ordinal: u32 = fields[0].parse().unwrap_or_else(|_| panic!("{path}:{} invalid ordinal", line_no + 1));
        let name = fields[1].to_owned();
        assert!(fields[2].is_empty(), "{path}:{} unexpected forwarder", line_no + 1);
        assert!(ordinals.insert(ordinal), "{path}: duplicate ordinal {ordinal}");
        assert!(!name.is_empty(), "{path}:{} missing export name", line_no + 1);
        assert!(names.insert(name.clone()), "{path}: duplicate export name {name}");
        exports.push(Export { ordinal, name });
    }
    assert_eq!(exports.len(), 1330, "{path}: unexpected Windows 11 msvcrt export count");
    for (index, export) in exports.iter().enumerate() { assert_eq!(export.ordinal, index as u32 + 1, "{path}: non-contiguous ordinal table"); }
    exports
}

fn expected_def(exports: &[Export]) -> String {
    let mut out = String::from("LIBRARY msvcrt.dll\nEXPORTS\n");
    for export in exports { out.push_str(&format!("    {} @{}\n", export.name, export.ordinal)); }
    out
}

fn main() {
    println!("cargo:rerun-if-changed=src/lib.rs"); println!("cargo:rerun-if-changed=exports/msvcrt-x64.def");
    let exports = read_exports("exports/windows-11-x64-exports.tsv");
    let actual = fs::read_to_string("exports/msvcrt-x64.def").expect("missing msvcrt .def");
    assert_eq!(actual, expected_def(&exports), "msvcrt .def diverges from pinned Windows 11 export manifest");
    let mut generated = String::from("// Generated from the pinned Windows 11 x64 msvcrt export manifest.\n");
    for export in &exports { generated.push_str(&format!("#[cfg(target_os = \"windows\")]\n#[export_name = {:?}]\npub extern \"system\" fn cognac_msvcrt_export_{}() -> usize {{ 0 }}\n", export.name, export.ordinal)); }
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    fs::write(out_dir.join("msvcrt_exports.rs"), generated).expect("write generated msvcrt exports");
    println!("cargo:warning=msvcrt: {} exact Windows 11 x64 named exports; all entry bodies are compatibility stubs", exports.len());
}
