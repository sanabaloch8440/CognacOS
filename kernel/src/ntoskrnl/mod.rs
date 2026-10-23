//! NTOSKRNL import stubs with broad name coverage from a public Windows x64 export dataset.
//! The checked-in manifest is Windows 10 x64 build 19041 (WinExports); it is used only
//! to provide names/ordinals for stub coverage, not as an ABI claim for Windows 11 26100.
//! Every entry resolves to a generic placeholder; signatures and semantics are NOT implemented.
use crate::pe::ImportSymbol;

pub const STATUS_NOT_IMPLEMENTED: u32 = 0xC000_0002;
pub const STATUS_PROCEDURE_NOT_FOUND: u32 = 0xC000_007A;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KernelModule { Ex, Ke, Mm, Ob, Ps, Io, Cm, Se, Rtl, Zw, Other }

#[derive(Clone, Copy, Debug)]
pub struct Export { pub name: &'static str, pub ordinal: u16, pub module: KernelModule }

pub type StubFn = unsafe extern "win64" fn(u64, u64, u64, u64, u64, u64) -> u64;

/// ABI placeholder only. Do not call as if it implemented the target API's signature.
pub unsafe extern "win64" fn not_implemented(
    _a1: u64, _a2: u64, _a3: u64, _a4: u64, _a5: u64, _a6: u64,
) -> u64 { STATUS_NOT_IMPLEMENTED as u64 }

mod manifest_19041;

fn module_for(name: &str) -> KernelModule {
    if name.starts_with("Ex") { KernelModule::Ex }
    else if name.starts_with("Ke") { KernelModule::Ke }
    else if name.starts_with("Mm") { KernelModule::Mm }
    else if name.starts_with("Ob") { KernelModule::Ob }
    else if name.starts_with("Ps") { KernelModule::Ps }
    else if name.starts_with("Io") { KernelModule::Io }
    else if name.starts_with("Cm") { KernelModule::Cm }
    else if name.starts_with("Se") { KernelModule::Se }
    else if name.starts_with("Rtl") { KernelModule::Rtl }
    else if name.starts_with("Zw") { KernelModule::Zw }
    else { KernelModule::Other }
}

pub fn exports() -> impl Iterator<Item = Export> {
    manifest_19041::ENTRIES.iter().filter_map(|entry| entry.name.map(|name| Export {
        name, ordinal: entry.ordinal, module: module_for(name),
    }))
}

pub fn resolve(dll: &[u8], symbol: ImportSymbol<'_>) -> Option<u64> {
    if !(dll.eq_ignore_ascii_case(b"ntoskrnl.exe") || dll.eq_ignore_ascii_case(b"ntkrnlmp.exe")
        || dll.eq_ignore_ascii_case(b"ntkrnlpa.exe") || dll.eq_ignore_ascii_case(b"ntkrpamp.exe")) {
        return None;
    }
    match symbol {
        ImportSymbol::Name(requested) => {
            if !exports().any(|e| e.name.as_bytes().eq_ignore_ascii_case(requested)) { return None; }
        }
        ImportSymbol::Ordinal(ordinal) => {
            if !manifest_19041::ENTRIES.iter().any(|e| e.ordinal == ordinal) { return None; }
        }
    }
    Some(not_implemented as StubFn as usize as u64)
}

pub fn named_export_count() -> usize { exports().count() }
pub fn ordinal_only_export_count() -> usize {
    manifest_19041::ENTRIES.iter().filter(|e| e.name.is_none()).count()
}

pub fn self_test() -> bool {
    let entries = manifest_19041::ENTRIES;
    entries.len() == 3093
        && named_export_count() == 3070
        && ordinal_only_export_count() == 23
        && entries.iter().enumerate().all(|(i, e)| e.ordinal as usize == i + 1)
        && exports().all(|e| !e.name.is_empty())
        && resolve(b"ntoskrnl.exe", ImportSymbol::Name(b"KeYieldExecution")).is_none()
            == !exports().any(|e| e.name == "KeYieldExecution")
        && resolve(b"ntoskrnl.exe", ImportSymbol::Ordinal(1)).is_some()
        && resolve(b"ntoskrnl.exe", ImportSymbol::Ordinal(3094)).is_none()
        && resolve(b"kernel32.dll", ImportSymbol::Ordinal(1)).is_none()
}
