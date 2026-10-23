use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=RDOTOS_KERNEL_SHA256");
    let value = env::var("RDOTOS_KERNEL_SHA256").unwrap_or_default();
    let generated = if value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit()) {
        let bytes: Vec<String> = (0..32)
            .map(|i| format!("0x{}", &value[i * 2..i * 2 + 2]))
            .collect();
        format!("pub const EXPECTED_KERNEL_SHA256: Option<[u8; 32]> = Some([{}]);\n", bytes.join(","))
    } else {
        "pub const EXPECTED_KERNEL_SHA256: Option<[u8; 32]> = None;\n".to_string()
    };
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR missing"));
    fs::write(out.join("kernel_digest.rs"), generated).expect("write generated kernel trust anchor");
}
