//! NTOSKRNL export catalogue and import stubs.
//! Export metadata is pinned to Windows 11 x64 10.0.26100.8036-AMD64; most entries remain unimplemented.
use crate::pe::ImportSymbol;

pub const STATUS_NOT_IMPLEMENTED: u32 = 0xC000_0002;
pub const STATUS_PROCEDURE_NOT_FOUND: u32 = 0xC000_007A;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KernelModule { Ex, Ke, Mm, Ob, Ps, Io, Cm, Se, Rtl, Zw, Other }
#[derive(Clone, Copy, Debug)]
pub struct Export { pub name: &'static str, pub ordinal: u16, pub module: KernelModule }
pub type StubFn = unsafe extern "win64" fn(u64, u64, u64, u64, u64, u64) -> u64;

/// ABI placeholder only. Do not call as if it implemented the target API's signature.
pub unsafe extern "win64" fn not_implemented(_a1: u64, _a2: u64, _a3: u64, _a4: u64, _a5: u64, _a6: u64) -> u64 { STATUS_NOT_IMPLEMENTED as u64 }

mod ex; mod ke; mod mm; mod ob; mod ps; mod io; mod cm; mod se; mod rtl; mod zw; mod other;
mod manifest_26100;
pub mod abi_signatures;

pub fn exports() -> impl Iterator<Item = Export> {
    ex::EXPORTS.iter().map(|&(name, ordinal)| Export { name, ordinal, module: KernelModule::Ex })
        .chain(ke::EXPORTS.iter().map(|&(name, ordinal)| Export { name, ordinal, module: KernelModule::Ke }))
        .chain(mm::EXPORTS.iter().map(|&(name, ordinal)| Export { name, ordinal, module: KernelModule::Mm }))
        .chain(ob::EXPORTS.iter().map(|&(name, ordinal)| Export { name, ordinal, module: KernelModule::Ob }))
        .chain(ps::EXPORTS.iter().map(|&(name, ordinal)| Export { name, ordinal, module: KernelModule::Ps }))
        .chain(io::EXPORTS.iter().map(|&(name, ordinal)| Export { name, ordinal, module: KernelModule::Io }))
        .chain(cm::EXPORTS.iter().map(|&(name, ordinal)| Export { name, ordinal, module: KernelModule::Cm }))
        .chain(se::EXPORTS.iter().map(|&(name, ordinal)| Export { name, ordinal, module: KernelModule::Se }))
        .chain(rtl::EXPORTS.iter().map(|&(name, ordinal)| Export { name, ordinal, module: KernelModule::Rtl }))
        .chain(zw::EXPORTS.iter().map(|&(name, ordinal)| Export { name, ordinal, module: KernelModule::Zw }))
        .chain(other::EXPORTS.iter().map(|&(name, ordinal)| Export { name, ordinal, module: KernelModule::Other }))
}

pub fn resolve(dll: &[u8], symbol: ImportSymbol<'_>) -> Option<u64> {
    if !(dll.eq_ignore_ascii_case(b"ntoskrnl.exe") || dll.eq_ignore_ascii_case(b"ntkrnlmp.exe") || dll.eq_ignore_ascii_case(b"ntkrnlpa.exe") || dll.eq_ignore_ascii_case(b"ntkrpamp.exe")) { return None; }
    let found = match symbol {
        ImportSymbol::Name(requested) => exports().any(|e| e.name.as_bytes().eq_ignore_ascii_case(requested)),
        // Ordinal-only exports are not assumed to be callable functions.
        ImportSymbol::Ordinal(ordinal) => manifest_26100::ENTRIES.iter().any(|e| e.ordinal == ordinal && e.name.is_some()),
    };
    if found { Some(not_implemented as StubFn as usize as u64) } else { None }
}

pub fn named_export_count() -> usize { exports().count() }
pub fn ordinal_only_export_count() -> usize { manifest_26100::ORDINAL_ONLY.len() }
pub fn self_test() -> bool {
    named_export_count() == 3352 && ordinal_only_export_count() == 37
        && exports().all(|e| !e.name.is_empty())
        && resolve(b"ntoskrnl.exe", ImportSymbol::Ordinal(1)).is_none()
        && resolve(b"kernel32.dll", ImportSymbol::Ordinal(1)).is_none()
        && abi_signatures::documented_signature("KeGetCurrentIrql").is_some()
}
