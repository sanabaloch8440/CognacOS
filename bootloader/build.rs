use std::{env, fs, path::PathBuf};

fn decode_hex<const N: usize>(name: &str) -> Option<[u8; N]> {
    let value = env::var(name).ok()?;
    if value.len() != N * 2 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut out = [0u8; N];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

fn array_expr(value: Option<&[u8]>) -> String {
    match value {
        Some(bytes) => format!("Some([{}])", bytes.iter().map(|b| format!("0x{b:02x}")).collect::<Vec<_>>().join(",")),
        None => "None".to_string(),
    }
}

fn main() {
    for name in ["COGNACOS_KERNEL_SHA256", "COGNACOS_KERNEL_SIGNATURE", "COGNACOS_SIGNING_PUBLIC_KEY"] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    let digest = decode_hex::<32>("COGNACOS_KERNEL_SHA256");
    let signature = decode_hex::<64>("COGNACOS_KERNEL_SIGNATURE");
    let public_key = decode_hex::<32>("COGNACOS_SIGNING_PUBLIC_KEY");
    let generated = format!(
        "pub const EXPECTED_KERNEL_SHA256: Option<[u8; 32]> = {};\npub const EXPECTED_KERNEL_SIGNATURE: Option<[u8; 64]> = {};\npub const EXPECTED_SIGNING_PUBLIC_KEY: Option<[u8; 32]> = {};\n",
        array_expr(digest.as_ref().map(|v| v.as_slice())),
        array_expr(signature.as_ref().map(|v| v.as_slice())),
        array_expr(public_key.as_ref().map(|v| v.as_slice())),
    );
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR missing"));
    fs::write(out.join("kernel_digest.rs"), generated).expect("write kernel trust anchors");
}
