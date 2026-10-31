use std::{collections::BTreeSet, env, fs, path::PathBuf};
struct Export { ordinal: u32, name: String }
fn main() {
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=exports/windows-11-x64-exports.tsv");
    println!("cargo:rerun-if-changed=exports/bcrypt-x64.def");
    let raw = fs::read_to_string("exports/windows-11-x64-exports.tsv").expect("read bcrypt export manifest");
    let mut exports = Vec::new();
    let mut ordinals = BTreeSet::new();
    let mut names = BTreeSet::new();
    for (line_no, line) in raw.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') { continue; }
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 3, "manifest line {} must have 3 tab-separated fields", line_no + 1);
        let ordinal: u32 = fields[0].parse().expect("valid ordinal");
        assert!(ordinals.insert(ordinal), "duplicate ordinal {ordinal}");
        assert!(fields[2].is_empty(), "unexpected forwarder for bcrypt export {}", fields[1]);
        assert!(names.insert(fields[1].to_owned()), "duplicate export {}", fields[1]);
        exports.push(Export { ordinal, name: fields[1].to_owned() });
    }
    assert_eq!(exports.len(), 60, "pinned Windows 11 bcrypt export count changed");
    let expected = format!("LIBRARY bcrypt.dll\nEXPORTS\n{}", exports.iter().map(|e| format!("    {} @{}\n", e.name, e.ordinal)).collect::<String>());
    let actual = fs::read_to_string("exports/bcrypt-x64.def").expect("read bcrypt .def");
    assert_eq!(actual, expected, "bcrypt .def differs from pinned export manifest");
    let implemented: BTreeSet<&str> = [
        "BCryptOpenAlgorithmProvider", "BCryptCloseAlgorithmProvider", "BCryptGetProperty", "BCryptHash", "BCryptGetFipsAlgorithmMode",
        "BCryptCreateHash", "BCryptHashData", "BCryptFinishHash", "BCryptDestroyHash", "BCryptDuplicateHash", "BCryptGenRandom", "BCryptDeriveKeyPBKDF2",
        "BCryptEnumAlgorithms", "BCryptEnumProviders", "BCryptFreeBuffer", "BCryptSetProperty", "BCryptGenerateSymmetricKey", "BCryptEncrypt", "BCryptDecrypt", "BCryptDestroyKey",
        "BCryptDuplicateKey", "BCryptImportKey", "BCryptExportKey",
        "BCryptCreateMultiHash", "BCryptProcessMultiOperations", "BCryptKeyDerivation", "BCryptDestroySecret", "BCryptDeriveKey", "BCryptImportKeyPair", "BCryptVerifySignature", "BCryptGenerateKeyPair", "BCryptFinalizeKeyPair", "BCryptSignHash", "BCryptDeriveKeyCapi", "BCryptCreateContext", "BCryptDeleteContext", "BCryptConfigureContext", "BCryptQueryContextConfiguration", "BCryptEnumContexts", "BCryptAddContextFunction", "BCryptRemoveContextFunction", "BCryptEnumContextFunctions", "BCryptConfigureContextFunction", "BCryptQueryContextFunctionConfiguration", "BCryptEnumContextFunctionProviders", "BCryptSetContextFunctionProperty", "BCryptQueryContextFunctionProperty", "BCryptAddContextFunctionProvider", "BCryptRemoveContextFunctionProvider",
    ].into_iter().collect();
    let mut generated = String::from("// Unsupported exports fail closed with STATUS_NOT_IMPLEMENTED.\n");
    for e in &exports {
        if !implemented.contains(e.name.as_str()) {
            generated.push_str(&format!("#[export_name = {:?}]\npub extern \"system\" fn cognac_bcrypt_unsupported_{}() -> i32 {{ STATUS_NOT_IMPLEMENTED }}\n", e.name, e.ordinal));
        }
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    fs::write(out.join("bcrypt_unsupported_exports.rs"), generated).expect("write unsupported export wrappers");
    println!("cargo:warning=bcrypt.dll: exact 60-export Windows 11 x64 baseline; {} implemented APIs, {} fail-closed APIs", implemented.len(), exports.len() - implemented.len());
}
