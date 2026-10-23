//! NTOSKRNL import stubs split into module-oriented source files.
//! Source manifest is Windows 10 x64 build 19041, used for stub coverage only.
//! Function signatures and semantics are not implemented by these generic placeholders.
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

mod ex; mod ke; mod mm; mod ob; mod ps; mod io; mod cm; mod se; mod rtl; mod zw; mod other;

fn module_for(name: &str) -> KernelModule {
    if ex::EXPORTS.iter().any(|(n, _)| *n == name) { KernelModule::Ex }
    else if ke::EXPORTS.iter().any(|(n, _)| *n == name) { KernelModule::Ke }
    else if mm::EXPORTS.iter().any(|(n, _)| *n == name) { KernelModule::Mm }
    else if ob::EXPORTS.iter().any(|(n, _)| *n == name) { KernelModule::Ob }
    else if ps::EXPORTS.iter().any(|(n, _)| *n == name) { KernelModule::Ps }
    else if io::EXPORTS.iter().any(|(n, _)| *n == name) { KernelModule::Io }
    else if cm::EXPORTS.iter().any(|(n, _)| *n == name) { KernelModule::Cm }
    else if se::EXPORTS.iter().any(|(n, _)| *n == name) { KernelModule::Se }
    else if rtl::EXPORTS.iter().any(|(n, _)| *n == name) { KernelModule::Rtl }
    else if zw::EXPORTS.iter().any(|(n, _)| *n == name) { KernelModule::Zw }
    else { KernelModule::Other }
}

pub fn exports() -> impl Iterator<Item = Export> {
    ex::EXPORTS.iter().chain(ke::EXPORTS).chain(mm::EXPORTS).chain(ob::EXPORTS)
        .chain(ps::EXPORTS).chain(io::EXPORTS).chain(cm::EXPORTS).chain(se::EXPORTS)
        .chain(rtl::EXPORTS).chain(zw::EXPORTS).chain(other::EXPORTS)
        .map(|(name, ordinal)| Export { name, ordinal: *ordinal, module: module_for(name) })
}

pub fn resolve(dll: &[u8], symbol: ImportSymbol<'_>) -> Option<u64> {
    if !(dll.eq_ignore_ascii_case(b"ntoskrnl.exe") || dll.eq_ignore_ascii_case(b"ntkrnlmp.exe")
        || dll.eq_ignore_ascii_case(b"ntkrnlpa.exe") || dll.eq_ignore_ascii_case(b"ntkrpamp.exe")) { return None; }
    let found = match symbol {
        ImportSymbol::Name(requested) => exports().any(|e| e.name.as_bytes().eq_ignore_ascii_case(requested)),
        ImportSymbol::Ordinal(ordinal) => manifest_ordinals().any(|ord| ord == ordinal),
    };
    if found { Some(not_implemented as StubFn as usize as u64) } else { None }
}

fn manifest_ordinals() -> impl Iterator<Item=u16> {
    ex::EXPORTS.iter().chain(ke::EXPORTS).chain(mm::EXPORTS).chain(ob::EXPORTS)
        .chain(ps::EXPORTS).chain(io::EXPORTS).chain(cm::EXPORTS).chain(se::EXPORTS)
        .chain(rtl::EXPORTS).chain(zw::EXPORTS).chain(other::EXPORTS)
        .map(|(_, ordinal)| *ordinal)
        .chain(manifest_19041::ORDINAL_ONLY.iter().map(|e| e.ordinal))
}

mod manifest_19041;
pub fn named_export_count() -> usize { exports().count() }
pub fn ordinal_only_export_count() -> usize { manifest_19041::ORDINAL_ONLY.len() }
pub fn self_test() -> bool {
    let all = exports().collect::<alloc::vec::Vec<_>>();
    all.len() == 3070 && ordinal_only_export_count() == 23
        && all.iter().all(|e| !e.name.is_empty())
        && resolve(b"ntoskrnl.exe", ImportSymbol::Ordinal(1)).is_some()
        && resolve(b"ntoskrnl.exe", ImportSymbol::Ordinal(3094)).is_none()
        && resolve(b"kernel32.dll", ImportSymbol::Ordinal(1)).is_none()
}
