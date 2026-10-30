# msvcrt.dll (Rust)

CognacOS builds a native AMD64 PE32+ `msvcrt.dll` from Rust and exports the complete 1,330-entry named export inventory in the pinned Windows 11 x64 reference snapshot.

- Reference dataset: `learn-more/winexports/data/exports/nt100_x64_26200/msvcrt.dll.json`
- Binary version: `7.0.26100.7623-AMD64`
- Inventory: `exports/windows-11-x64-exports.tsv`
- Linker definition: `exports/msvcrt-x64.def`
- Build-time generation: `build.rs`

Most exports currently have zero-return placeholder bodies. The core `memcpy`, `memmove`, `memset`, `memcmp`, and `strlen` exports have x64 ABI assembly implementations so Rust's own runtime/linker memory operations do not resolve to inert stubs. The remaining functions do **not** implement their documented C runtime semantics. Real signatures, floating-point return paths, variadic functions, CRT global data, FILE/locale/thread state, heap ownership, startup/termination, and exception/unwind integration remain future work. Do not treat arbitrary applications as runnable merely because their imports resolve.

The source inventory is vendored locally; builds and CI do not fetch export data from the network.
