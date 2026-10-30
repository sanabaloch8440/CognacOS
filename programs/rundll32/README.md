# CognacOS rundll32.exe

Native freestanding Windows x64 command-line host for DLL entry points. It parses the conventional module.dll,EntryPoint [arguments] form and the separate DLL/entry form, loads the DLL with LoadLibraryW, resolves named or #ordinal entry points with GetProcAddress, invokes the four-argument x64 CALLBACK shape, and releases the module.

For unsuffixed names it probes the W export first, then A, then the unsuffixed export. W callbacks receive the original UTF-16 command-line tail. A callbacks receive an ASCII-only conversion; non-ASCII arguments fail closed instead of being silently corrupted. The command-line tokenizer handles basic quote grouping but is not yet the complete Windows backslash/quote algorithm.

Remaining compatibility work includes exact Windows command-line tokenization, full active-code-page conversion, GUI error reporting, 32-bit/WOW64 host selection, activation-context behavior, and exhaustive comparison with Windows 11 rundll32.exe. This host must only invoke exports designed for the rundll32 callback ABI; arbitrary DLL exports are unsafe to call.

The Windows CI builds a freestanding AMD64 PE32+ executable and links only the five required Kernel32 imports. Host-target tests are deliberately not treated as a substitute for running it on CognacOS.
