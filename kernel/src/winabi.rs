//! Windows x64 API-set contract normalization and process-local export bindings.
//! Registered addresses must lie inside a mapped user DLL image.
use crate::pe::ImportSymbol;
const USER_START: u64 = 0x0000_0080_0000_0000;
const USER_END: u64 = 0x0000_8000_0000_0000;
const MODULES: usize = 32;
const EXPORTS: usize = 512;
const NAME: usize = 64;
#[derive(Clone, Copy)]
pub struct ModuleInfo { pub name: [u8; NAME], pub len: usize, pub base: u64, pub size: u32, pub entrypoint: u64 }
impl ModuleInfo { pub const EMPTY: Self = Self { name:[0;NAME],len:0,base:0,size:0,entrypoint:0 }; }
#[derive(Clone, Copy)]
struct Module { name: [u8; NAME], len: usize, base: u64, size: u32, entrypoint: u64, used: bool }
impl Module { const EMPTY: Self = Self { name: [0; NAME], len: 0, base: 0, size: 0, entrypoint: 0, used: false }; }
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
pub unsafe fn register_module(name: &[u8], base: u64, size: u32, entrypoint: u64) -> bool {
    if base < USER_START || size == 0 || base.checked_add(size as u64).is_none_or(|e| e > USER_END) { return false; }
    let mut normalized=[0u8; NAME]; let Some(len)=normalize(name,&mut normalized) else { return false; };
    let state=unsafe { &mut *REGISTRY.0.get() };
    if let Some(slot)=state.0.iter_mut().find(|m| m.used && eq_case(&m.name[..m.len],&normalized[..len])) {
        *slot=Module { name:normalized,len,base,size,entrypoint,used:true }; return true;
    }
    if let Some(slot)=state.0.iter_mut().find(|m| !m.used) {
        *slot=Module { name:normalized,len,base,size,used:true }; true
    } else { false }
}
/// Snapshot registered modules for constructing a process-local PEB loader list.
pub fn module_infos(out:&mut[ModuleInfo])->usize {
    let state=unsafe{&*REGISTRY.0.get()};
    let mut count=0;
    for m in state.0.iter().filter(|m|m.used).take(out.len()) {
        out[count]=ModuleInfo{name:m.name,len:m.len,base:m.base,size:m.size,entrypoint:m.entrypoint};
        count+=1;
    }
    count
}
/// Remove one DLL and every export associated with it during process teardown.
pub fn unregister_module(name:&[u8]) {
    let mut normalized=[0u8;NAME];
    let Some(len)=normalize(name,&mut normalized) else{return;};
    let state=unsafe{&mut *REGISTRY.0.get()};
    let Some(index)=state.0.iter().position(|m|m.used&&eq_case(&m.name[..m.len],&normalized[..len])) else{return;};
    state.0[index]=Module::EMPTY;
    for e in state.1.iter_mut() { if e.used&&e.module==index { *e=Export::EMPTY; } }
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

fn read_user(space:&crate::vm::AddressSpace,address:u64,out:&mut[u8])->Result<(),()> {
    unsafe { space.copy_from_user(address,out).map_err(|_|()) }
}
fn read_u16(space:&crate::vm::AddressSpace,address:u64)->Result<u16,()> {
    let mut b=[0;2];read_user(space,address,&mut b)?;Ok(u16::from_le_bytes(b))
}
fn read_u32(space:&crate::vm::AddressSpace,address:u64)->Result<u32,()> {
    let mut b=[0;4];read_user(space,address,&mut b)?;Ok(u32::from_le_bytes(b))
}
fn read_export_name(space:&crate::vm::AddressSpace,base:u64,rva:u32)->Result<([u8;NAME],usize),()> {
    let mut out=[0u8;NAME];
    for i in 0..NAME {
        out[i]=read_u32_byte(space,base.checked_add(rva as u64).and_then(|x|x.checked_add(i as u64)).ok_or(())?)?;
        if out[i]==0{return Ok((out,i));}
        if !out[i].is_ascii(){return Err(());}
    }
    Err(())
}
fn read_u32_byte(space:&crate::vm::AddressSpace,address:u64)->Result<u8,()> {
    let mut b=[0];read_user(space,address,&mut b)?;Ok(b[0])
}
/// Parse a loaded PE32+ DLL export directory and publish named and ordinal
/// exports into the bounded binding registry. Forwarded exports are rejected
/// until their target module has been resolved explicitly.
pub unsafe fn register_mapped_exports(space:&crate::vm::AddressSpace,module_name:&[u8],image:crate::pe::LoadedImage)->Result<usize,()> {
    let base=image.base;
    let mut dos=[0u8;0x40];read_user(space,base,&mut dos)?;
    if dos[0]!=b'M'||dos[1]!=b'Z'{return Err(());}
    let nt=u32::from_le_bytes(dos[0x3c..0x40].try_into().map_err(|_|())?) as u64;
    let mut sig=[0u8;4];read_user(space,base.checked_add(nt).ok_or(())?,&mut sig)?;
    if &sig!=b"PE\0\0"{return Err(());}
    let opt_size=read_u16(space,base.checked_add(nt+20).ok_or(())?)? as u64;
    let opt=base.checked_add(nt+24).ok_or(())?;
    if read_u16(space,opt)?!=0x20b{return Err(());}
    if read_u32(space,opt+108)?==0{return Err(());}
    let export_rva=read_u32(space,opt+112)?;
    let export_size=read_u32(space,opt+116)?;
    if export_rva==0||export_size<40||export_rva.checked_add(export_size).is_none_or(|e|e>image.image_size){return Err(());}
    let dir=base.checked_add(export_rva as u64).ok_or(())?;
    let ordinal_base=read_u32(space,dir+16)?;
    let function_count=read_u32(space,dir+20)? as usize;
    let name_count=read_u32(space,dir+24)? as usize;
    let functions=read_u32(space,dir+28)?;
    let names=read_u32(space,dir+32)?;
    let ordinals=read_u32(space,dir+36)?;
    if function_count>EXPORTS||name_count>EXPORTS||name_count>function_count{return Err(());}
    if !unsafe{register_module(module_name,base,image.image_size,image.entry)}{return Err(());}
    let mut named=[false;EXPORTS];
    for i in 0..name_count {
        let name_rva=read_u32(space,base+names as u64+(i*4) as u64)?;
        let ordinal_index=read_u16(space,base+ordinals as u64+(i*2) as u64)? as usize;
        if ordinal_index>=function_count{return Err(());}
        let function_rva=read_u32(space,base+functions as u64+(ordinal_index*4) as u64)?;
        if function_rva==0||function_rva>=image.image_size{return Err(());}
        if function_rva>=export_rva&&function_rva<export_rva+export_size{return Err(());}
        let (name,len)=read_export_name(space,base,name_rva)?;
        if len==0{return Err(());}
        let ordinal=ordinal_base.checked_add(ordinal_index as u32).filter(|&o|o<=u16::MAX as u32).ok_or(())? as u16;
        if !unsafe{register_export(module_name,&name[..len],ordinal,base+function_rva as u64)}{return Err(());}
        named[ordinal_index]=true;
    }
    let mut registered=name_count;
    for i in 0..function_count {
        let function_rva=read_u32(space,base+functions as u64+(i*4) as u64)?;
        if function_rva==0||named[i]{continue;}
        if function_rva>=image.image_size{return Err(());}
        if function_rva>=export_rva&&function_rva<export_rva+export_size{continue;}
        let ordinal=ordinal_base.checked_add(i as u32).filter(|&o|o<=u16::MAX as u32).ok_or(())? as u16;
        if !unsafe{register_export(module_name,&[],ordinal,base+function_rva as u64)}{return Err(());}
        registered+=1;
    }
    let _=opt_size;
    Ok(registered)
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
