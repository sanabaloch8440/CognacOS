use std::{collections::BTreeMap, env, fs, path::PathBuf};

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let root = manifest.parent().expect("workspace root");
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let source_dir = root.join("build/drivers");
    let catalog_dir = out_dir.join("drivers");
    fs::create_dir_all(&catalog_dir).expect("create staged driver catalog");
    println!("cargo:rerun-if-changed={}", source_dir.display());

    // Preserve the existing named assets used by platform-specific handoff
    // code while making every staged .sys image available to the generic
    // registry ImagePath resolver.
    let required = [
        ("acpi.sys", true),
        ("pci.sys", true),
        ("usbxhci.sys", false),
        ("ucx01020.sys", false),
        ("usbhub3.sys", false),
    ];
    for (name, required_for_pnp) in required {
        let source = source_dir.join(name);
        let out = out_dir.join(name);
        println!("cargo:rerun-if-changed={}", source.display());
        if source.is_file() {
            fs::copy(&source, &out).unwrap_or_else(|error| {
                panic!("copy generated {} into kernel build: {}", name, error)
            });
        } else {
            fs::write(&out, []).expect("write empty driver image placeholder");
            if required_for_pnp {
                println!("cargo:warning={} was not staged; required platform driver initialization will be unavailable", name);
            } else {
                println!("cargo:warning={} was not staged; its PnP binding will remain unavailable", name);
            }
        }
        fs::copy(&out, catalog_dir.join(name)).expect("copy named driver into generic catalog");
    }

    let mut images = BTreeMap::<String, String>::new();
    for (name, _) in required {
        images.insert(name.to_string(), name.to_string());
    }
    if let Ok(entries) = fs::read_dir(&source_dir) {
        for entry in entries {
            let entry = entry.expect("read staged driver directory entry");
            let source = entry.path();
            if !source.is_file()
                || !source.extension().and_then(|value| value.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("sys"))
            {
                continue;
            }
            let Some(filename) = source.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if !filename.is_ascii()
                || !filename.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
            {
                continue;
            }
            let key = filename.to_ascii_lowercase();
            let destination = catalog_dir.join(filename);
            fs::copy(&source, &destination).unwrap_or_else(|error| {
                panic!("copy staged driver {} into generic catalog: {}", filename, error)
            });
            images.insert(key, filename.to_string());
            println!("cargo:rerun-if-changed={}", source.display());
        }
    }

    let mut generated = String::from(
        "pub static DRIVER_IMAGES: &[(&str, &[u8])] = &[\n"
    );
    for (key, filename) in images {
        generated.push_str(&format!(
            "    ({:?}, &include_bytes!(concat!(env!(\"OUT_DIR\"), \"/drivers/{}\"))[..]),\n",
            key, filename
        ));
    }
    generated.push_str("];\n");
    fs::write(out_dir.join("driver_images.rs"), generated)
        .expect("write generated staged driver catalog");
}
