use std::{env,fs,path::PathBuf};

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let root = manifest.parent().expect("workspace root");
    let source = root.join("build/drivers/usbxhci.sys");
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR")).join("usbxhci.sys");
    println!("cargo:rerun-if-changed={}", source.display());
    if source.is_file() {
        fs::copy(&source, &out).expect("copy generated usbxhci.sys into kernel build");
        println!("cargo:warning=Embedding build/drivers/usbxhci.sys into CognacOS kernel");
    } else {
        // Keep standalone cargo check/build usable outside the full CI pipeline.
        // Runtime binding explicitly rejects this empty image.
        fs::write(&out, []).expect("write empty xHCI image placeholder");
        println!("cargo:warning=usbxhci.sys was not staged; boot-time xHCI binding will stay disabled");
    }
}
