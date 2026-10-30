# CognacOS syssetup.dll

Native AMD64 PE32+ DLL target. The initial inventory records 11 named exports and the SetupInfObjectInstallActionW forwarder to SETUPAPI.InstallHinfSectionW. The names and ordinals are corroborated by a published COFF export dump for Windows 10 version 10.0.19041.1. This list has not yet been independently checked against the exact Windows 11 10.0.26100.7920 reference binary; it is therefore an initial compatibility baseline, not a certified exact Windows 11 export match.

The forwarder is emitted by the PE linker. Other export bodies are import-resolution placeholders returning zero. Answer-file/SIF state management, plug-and-play registry restoration, setup display/font configuration, and SAM service coordination are not implemented. Matching names and ordinals does not imply setup compatibility.
