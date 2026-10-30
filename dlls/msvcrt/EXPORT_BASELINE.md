# msvcrt.dll Windows 11 x64 export baseline

The inventory and module-definition file are generated from the WinExports dataset `data/exports/nt100_x64_26200/msvcrt.dll.json` in [learn-more/winexports](https://github.com/learn-more/winexports/tree/master/data/exports/nt100_x64_26200).

The dataset's PE metadata identifies `msvcrt.dll`, machine `AMD64`, binary version `7.0.26100.7623`, and 1,330 named export entries with ordinals 1 through 1,330. The dataset directory key is `26200`; the file's recorded version is 7.0.26100.7623.

CognacOS preserves the names and ordinals in its local TSV and .def. The five low-level memory/string routines `memcpy`, `memmove`, `memset`, `memcmp`, and `strlen` have x64 assembly implementations; other generated Rust entries are temporary zero-return stubs. Export parity is not C runtime behavioral compatibility. The PE export directory does not encode whether an entry is a function or data object, so the current generated layer treats every name as a callable stub; data exports and exact function signatures need a separately verified ABI inventory.
