//! Windows x64 API-set contract normalization and process-local export bindings.
//! Registered addresses must lie inside a mapped user DLL image.
use crate::pe::ImportSymbol;
const USER_START: u64 = 0x0000_0080_0000_0000;
const USER_END: u64 = 0x0000_8000_0000_0000;
const MODULES: usize = 32;
const EXPORTS: usize = 512;
const NAME: usize = 64;
#[derive(Clone, Copy)]
struct Module { name: [u8; NAME], len: usize, base: u64, size: u32, used: bool }
impl Module { const EMPTY: Self = Self { name: [0; NAME], len: 0, base: 0, size: 0, used: false }; }
#[derive(Clone, Copy)]
struct Export { module: usize, name: [u8; NAME], len: usize, ordinal: u16, address: u64, used: bool }
impl Export { const EMPTY: Self = Self { module: 0, name: [0; NAME], len: 0, ordinal: 0, address: 0, used: false }; }
struct Registry(core::cell::UnsafeCell<([Module; MODULES], [Export; EXPORTS])>);
unsafe impl Sync for Registry {}
static REGISTRY: Registry = Registry(core::cell::UnsafeCell::new(([Module::EMPTY; MODULES], [Export::EMPTY; EXPORTS])));
fn eq_case(a: &[u8], b: &[u8]) -> bool { a.len() == b.len() && a.iter().zip(b).all(|(x,y)| x.eq_ignore_ascii_case(y)) }
fn normalize(input: &[u8], out: &mut [u8; NAME]) -> Option<usize> {
    if input.is_empty() || input.len() >= NAME { return None; }
    let mut n = input.len();
    for (i,b) in input.iter().enumerate() {
        if !b.is_ascii() || *b == b'/' || *b == b'\\' { return None; }
        out[i] = b.to_ascii_lowercase();
    }
    if !out[..n].ends_with(b".dll") && n + 4 < NAME { out[n..n+4].copy_from_slice(b".dll"); n += 4; }
    Some(n)
}
/// Resolve common API-set contracts to known host DLLs; unknown contracts fail closed.
pub fn api_set_host(contract: &[u8]) -> Option<&'static [u8]> {
    let mut name = [0u8; NAME]; let n = normalize(contract, &mut name)?; let s = &name[..n];
    if s.starts_with(b"api-ms-win-core-processthreads-l1-") { Some(b"kernel32.dll") }
    else if s.starts_with(b"api-ms-win-core-file-l1-") { Some(b"kernel32.dll") }
    else if s.starts_with(b"api-ms-win-core-heap-l1-") { Some(b"kernelbase.dll") }
    else if s.starts_with(b"api-ms-win-core-memory-l1-") { Some(b"kernel32.dll") }
    else if s.starts_with(b"api-ms-win-core-libraryloader-l1-") { Some(b"kernel32.dll") }
    else if s.starts_with(b"api-ms-win-core-synch-l1-") { Some(b"kernel32.dll") }
    else if s.starts_with(b"api-ms-win-core-errorhandling-l1-") { Some(b"kernel32.dll") }
    else if s.starts_with(b"api-ms-win-core-string-l1-") { Some(b"kernel32.dll") }
    else if s.starts_with(b"api-ms-win-core-console-l1-") { Some(b"kernel32.dll") }
    else if s.starts_with(b"api-ms-win-core-timezone-l1-") { Some(b"kernel32.dll") }
    else if s.starts_with(b"api-ms-win-core-localization-l1-") { Some(b"kernel32.dll") }
    else if s.starts_with(b"api-ms-win-crt-") { Some(b"ucrtbase.dll") }
    else if s.starts_with(b"api-ms-win-security-") || s.starts_with(b"api-ms-win-registry-") || s.starts_with(b"api-ms-win-service-") { Some(b"advapi32.dll") }
    else if s.starts_with(b"api-ms-win-rtcore-") { Some(b"ntdll.dll") }
    else { None }
}
/// Register a mapped DLL image before its exports are published.
pub unsafe fn register_module(name: &[u8], base: u64, size: u32) -> bool {
    if base < USER_START || size == 0 || base.checked_add(size as u64).is_none_or(|e| e > USER_END) { return false; }
    let mut normalized=[0u8; NAME]; let Some(len)=normalize(name,&mut normalized) else { return false; };
    let state=unsafe { &mut *REGISTRY.0.get() };
    if let Some(slot)=state.0.iter_mut().find(|m| m.used && eq_case(&m.name[..m.len],&normalized[..len])) {
        *slot=Module { name:normalized,len,base,size,used:true }; return true;
    }
    if let Some(slot)=state.0.iter_mut().find(|m| !m.used) {
        *slot=Module { name:normalized,len,base,size,used:true }; true
    } else { false }
}
/// Register an export address after parsing the PE export directory.
pub unsafe fn register_export(module_name: &[u8], name: &[u8], ordinal: u16, address: u64) -> bool {
    let mut normalized=[0u8; NAME]; let Some(len)=normalize(module_name,&mut normalized) else { return false; };
    let state=unsafe { &mut *REGISTRY.0.get() };
    let Some(module)=state.0.iter().position(|m| m.used && eq_case(&m.name[..m.len],&normalized[..len])) else { return false; };
    let m=state.0[module];
    if address < m.base || address >= m.base + m.size as u64 || name.len() >= NAME { return false; }
    let mut exp_name=[0u8; NAME]; exp_name[..name.len()].copy_from_slice(name);
    if let Some(slot)=state.1.iter_mut().find(|e| e.used && e.module==module && e.ordinal==ordinal && e.len==name.len() && e.name[..e.len]==name[..]) {
        *slot=Export { module,name:exp_name,len:name.len(),ordinal,address,used:true }; return true;
    }
    if let Some(slot)=state.1.iter_mut().find(|e| !e.used) {
        *slot=Export { module,name:exp_name,len:name.len(),ordinal,address,used:true }; true
    } else { false }
}
/// Resolve a registered export, canonicalizing known API-set contract names.
pub fn resolve_import(dll: &[u8], symbol: ImportSymbol<'_>) -> Option<u64> {
    let mut normalized=[0u8; NAME]; let len=normalize(dll,&mut normalized)?;
    let canonical=api_set_host(&normalized[..len]).unwrap_or(&normalized[..len]);
    let mut cn=[0u8; NAME]; let cl=normalize(canonical,&mut cn)?;
    let state=unsafe { &*REGISTRY.0.get() };
    let module=state.0.iter().position(|m| m.used && eq_case(&m.name[..m.len],&cn[..cl]))?;
    state.1.iter().find(|e| e.used && e.module==module && match symbol {
        ImportSymbol::Name(n) => e.len==n.len() && eq_case(&e.name[..e.len],n),
        ImportSymbol::Ordinal(o) => e.ordinal==o,
    }).map(|e| e.address)
}
