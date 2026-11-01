use std::{env,fs,path::PathBuf};

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let root = manifest.parent().expect("workspace root");
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    for (name, required_for_pnp) in [("acpi.sys", true), ("pci.sys", true), ("usbxhci.sys", false), ("ucx01020.sys", false), ("usbhub3.sys", false)] {
        let source = root.join("build/drivers").join(name);
        let out = out_dir.join(name);
        println!("cargo:rerun-if-changed={}", source.display());
        if source.is_file() {
            fs::copy(&source, &out).unwrap_or_else(|error| {
                panic!("copy generated {} into kernel build: {}", name, error)
            });
            println!("cargo:warning=Embedding build/drivers/{} into CognacOS kernel", name);
        } else {
            // Standalone cargo check remains possible without staged PE images.
            // PCI PnP cannot start without pci.sys; xHCI remains optional.
            fs::write(&out, []).expect("write empty driver image placeholder");
            if required_for_pnp {
                println!("cargo:warning=pci.sys was not staged; PCI PnP initialization will be unavailable");
            } else {
                println!("cargo:warning=usbxhci.sys was not staged; xHCI binding will stay disabled");
            }
        }
    }
}
