# CognacOS rundll32.exe

Native Windows x64 command-line host for DLL entry points. It parses the conventional module.dll,EntryPoint [arguments] form (and a separate DLL/entry form), loads the DLL with LoadLibraryW, resolves named or #ordinal entry points with GetProcAddress, invokes the x64 callback shape, and releases the module.

This is an initial host, not a complete Windows implementation. It currently reconstructs Unicode arguments rather than preserving the exact original command-line tail, does not implement the full Windows quoting/ANSI-vs-Unicode dispatch rules, GUI error presentation, 32-bit/WOW64 host selection, shell activation contexts, or all Windows process lifetime semantics. DLL entry points must have the expected rundll32 callback ABI; arbitrary exports are not safe to call.

Host tests cover argument selection and malformed invocation. Build the actual PE32+ executable for x86_64-pc-windows-msvc.
