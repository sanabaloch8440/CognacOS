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
    let runtime_symbols = ["memcmp", "memcpy", "memmove", "memset", "strlen"];
    let mut generated = String::from("// Generated from the pinned Windows 11 x64 msvcrt export manifest.\\n");
    for export in &exports {
        if runtime_symbols.contains(&export.name.as_str()) { continue; }
        generated.push_str(&format!("#[cfg(target_os = \\"windows\\")]\\n#[export_name = {:?}]\\npub extern \\"system\\" fn cognac_msvcrt_export_{}() -> usize {{ 0 }}\\n", export.name, export.ordinal));
    }
    let runtime_asm = r#"
.text
.p2align 4
.globl memcpy
memcpy:
    pushq %rdi
    pushq %rsi
    movq %rcx, %rax
    movq %rcx, %rdi
    movq %rdx, %rsi
    movq %r8, %rcx
    cld
    rep movsb
    popq %rsi
    popq %rdi
    ret

.p2align 4
.globl memmove
memmove:
    pushq %rdi
    pushq %rsi
    movq %rcx, %rax
    testq %r8, %r8
    jz .Lmemmove_done
    cmpq %rdx, %rcx
    jbe .Lmemmove_forward
    leaq -1(%rdx,%r8), %rsi
    leaq -1(%rcx,%r8), %rdi
    movq %r8, %rcx
    std
    rep movsb
    cld
    jmp .Lmemmove_done
.Lmemmove_forward:
    movq %rcx, %rdi
    movq %rdx, %rsi
    movq %r8, %rcx
    cld
    rep movsb
.Lmemmove_done:
    popq %rsi
    popq %rdi
    ret

.p2align 4
.globl memset
memset:
    pushq %rdi
    movq %rcx, %rax
    movq %rcx, %rdi
    movq %r8, %rcx
    movb %dl, %al
    cld
    rep stosb
    popq %rdi
    ret

.p2align 4
.globl memcmp
memcmp:
    pushq %rsi
    pushq %rdi
    movq %rcx, %rsi
    movq %rdx, %rdi
    movq %r8, %rcx
    cld
    repe cmpsb
    je .Lmemcmp_equal
    movzbl -1(%rsi), %eax
    movzbl -1(%rdi), %edx
    subl %edx, %eax
    jmp .Lmemcmp_done
.Lmemcmp_equal:
    xorl %eax, %eax
.Lmemcmp_done:
    popq %rdi
    popq %rsi
    ret

.p2align 4
.globl strlen
strlen:
    pushq %rdi
    movq %rcx, %rdi
    movq $-1, %rcx
    xorl %eax, %eax
    cld
    repne scasb
    notq %rcx
    decq %rcx
    movq %rcx, %rax
    popq %rdi
    ret
"#;
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    fs::write(out_dir.join("msvcrt_exports.rs"), generated).expect("write generated msvcrt exports");
    fs::write(out_dir.join("msvcrt_runtime.S"), runtime_asm).expect("write CRT runtime assembly");
    println!("cargo:warning=msvcrt: {} Windows 11 x64 exports; core memory/string primitives have ABI implementations and remaining exports are stubs", exports.len());
}
