//! Build-pinned NT kernel import compatibility surface for RdotOS.
//!
//! This is an original compatibility shim, not Microsoft's kernel code.
//! Named entries are a curated first tranche, not a complete Windows 11 export
//! dump. Only ordinal-only exports listed in ordinal_exports() have verified
//! ordinal assignments here; never infer a named export's ordinal from this list.
use crate::pe::ImportSymbol;

pub const STATUS_NOT_IMPLEMENTED: u32 = 0xC000_0002;
pub const STATUS_PROCEDURE_NOT_FOUND: u32 = 0xC000_007A;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KernelModule { Ex, Ke, Mm, Ob, Ps, Io, Cm, Se, Rtl, Zw }

#[derive(Clone, Copy, Debug)]
pub struct Export { pub name: &'static str, pub module: KernelModule }

pub type StubFn = unsafe extern "win64" fn(u64, u64, u64, u64, u64, u64) -> u64;

/// ABI placeholder for unresolved kernel services. This does not implement the
/// API's contract and is not safe for arbitrary callers to rely on.
pub unsafe extern "win64" fn not_implemented(
    _a1: u64, _a2: u64, _a3: u64, _a4: u64, _a5: u64, _a6: u64,
) -> u64 { STATUS_NOT_IMPLEMENTED as u64 }

mod ex;
mod ke;
mod mm;
mod ob;
mod ps;
mod io;
mod cm;
mod se;
mod rtl;
mod zw;

pub fn exports() -> impl Iterator<Item = Export> {
    ex::NAMES.iter().copied().map(|name| Export { name, module: KernelModule::Ex })
        .chain(ke::NAMES.iter().copied().map(|name| Export { name, module: KernelModule::Ke }))
        .chain(mm::NAMES.iter().copied().map(|name| Export { name, module: KernelModule::Mm }))
        .chain(ob::NAMES.iter().copied().map(|name| Export { name, module: KernelModule::Ob }))
        .chain(ps::NAMES.iter().copied().map(|name| Export { name, module: KernelModule::Ps }))
        .chain(io::NAMES.iter().copied().map(|name| Export { name, module: KernelModule::Io }))
        .chain(cm::NAMES.iter().copied().map(|name| Export { name, module: KernelModule::Cm }))
        .chain(se::NAMES.iter().copied().map(|name| Export { name, module: KernelModule::Se }))
        .chain(rtl::NAMES.iter().copied().map(|name| Export { name, module: KernelModule::Rtl }))
        .chain(zw::NAMES.iter().copied().map(|name| Export { name, module: KernelModule::Zw }))
}

#[derive(Clone, Copy)]
struct OrdinalExport { ordinal: u16, name: &'static str }

// Ordinal-only exports documented for Windows 10 version 2004 and later.
// These are ordinal slots, not ordinal assignments for the named exports above.
const ORDINAL_ONLY: &[OrdinalExport] = &[
    OrdinalExport { ordinal: 4, name: "ExAllocateTimerInternal2" },
    OrdinalExport { ordinal: 5, name: "VslTestRoutine" },
    OrdinalExport { ordinal: 6, name: "KeYieldExecution" },
    OrdinalExport { ordinal: 7, name: "KeExpandKernelStackAndCalloutInternal" },
    OrdinalExport { ordinal: 9, name: "VslCapturePgoData" },
    OrdinalExport { ordinal: 12, name: "MmGetNumberOfPhysicalPages" },
    OrdinalExport { ordinal: 13, name: "PnpFreeSystemPdoList" },
    OrdinalExport { ordinal: 14, name: "PnpGetDeviceInstancePropertyData" },
    OrdinalExport { ordinal: 15, name: "PnpGetDeviceInstanceRegistryValue" },
    OrdinalExport { ordinal: 16, name: "PnpGetSystemPdoList" },
    OrdinalExport { ordinal: 17, name: "RtlGetHostNtSystemRoot" },
    OrdinalExport { ordinal: 19, name: "ZwPlugPlayControl" },
    OrdinalExport { ordinal: 20, name: "ExpSetKernelDataProtection" },
    OrdinalExport { ordinal: 21, name: "ExpGetKernelDataProtection" },
    OrdinalExport { ordinal: 22, name: "ExpReducedLicenseData" },
    OrdinalExport { ordinal: 23, name: "SeCodeIntegrityGetBuildExpiryTime" },
    OrdinalExport { ordinal: 24, name: "ExpEtwTraceLicensingCacheChange" },
    OrdinalExport { ordinal: 25, name: "ClipInitHandles" },
    OrdinalExport { ordinal: 28, name: "ExUpdateOsPfnInRegistry" },
];

fn eq_ascii(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.eq_ignore_ascii_case(y))
}

fn is_kernel_dll(dll: &[u8]) -> bool {
    eq_ascii(dll, b"ntoskrnl.exe") || eq_ascii(dll, b"ntkrnlmp.exe")
        || eq_ascii(dll, b"ntkrnlpa.exe") || eq_ascii(dll, b"ntkrpamp.exe")
}

pub fn resolve(dll: &[u8], symbol: ImportSymbol<'_>) -> Option<u64> {
    if !is_kernel_dll(dll) { return None; }
    let name = match symbol {
        ImportSymbol::Name(requested) => {
            exports().find(|e| eq_ascii(e.name.as_bytes(), requested)).map(|e| e.name)
                .or_else(|| ORDINAL_ONLY.iter().find(|e| eq_ascii(e.name.as_bytes(), requested)).map(|e| e.name))?
        }
        ImportSymbol::Ordinal(ordinal) => ORDINAL_ONLY.iter()
            .find(|e| e.ordinal == ordinal).map(|e| e.name)?,
    };
    let _ = name;
    Some(not_implemented as StubFn as usize as u64)
}

pub fn named_export_count() -> usize { exports().count() }
pub fn ordinal_only_export_count() -> usize { ORDINAL_ONLY.len() }

/// Self-check the generated registry without invoking a stub.
pub fn self_test() -> bool {
    let count = named_export_count();
    count >= 100
        && exports().all(|e| !e.name.is_empty())
        && ORDINAL_ONLY.iter().all(|e| !e.name.is_empty() && e.ordinal != 0)
        && resolve(b"ntoskrnl.exe", ImportSymbol::Name(b"KeYieldExecution")).is_some()
        && resolve(b"ntoskrnl.exe", ImportSymbol::Ordinal(6)).is_some()
        && resolve(b"ntoskrnl.exe", ImportSymbol::Ordinal(8)).is_none()
        && resolve(b"kernel32.dll", ImportSymbol::Name(b"KeYieldExecution")).is_none()
}
