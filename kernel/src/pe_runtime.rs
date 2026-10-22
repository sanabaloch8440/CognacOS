//! Runtime metadata for loaded PE32+ images: TLS callbacks and x64 .pdata.
//! Callbacks are recorded for user-mode startup; they are never called in ring 0.
use crate::{pe::LoadedImage, vm::AddressSpace};
const MAX_IMAGES: usize = 32;
const MAX_TLS_CALLBACKS: usize = 64;
const MAX_RUNTIME_FUNCTIONS: usize = 2048;
#[derive(Clone, Copy, Debug)]
pub enum RuntimeError { InvalidImage, TooManyImages, TooManyCallbacks, TooManyFunctions, InvalidDirectory, Vm }
#[derive(Clone, Copy, Debug)]
pub struct RuntimeFunction { pub image_base: u64, pub begin: u32, pub end: u32, pub unwind_info: u32 }
#[derive(Clone, Copy, Debug)]
pub struct ImageRuntime { pub base: u64, pub tls_callbacks: [u64; MAX_TLS_CALLBACKS], pub tls_count: usize, pub pdata_rva: u32, pub pdata_size: u32 }
#[derive(Clone, Copy)]
struct ImageRecord { base:u64, size:u32, pdata_rva:u32, pdata_size:u32, callbacks:[u64;MAX_TLS_CALLBACKS], callback_count:usize, used:bool }
impl ImageRecord { const EMPTY:Self=Self { base:0,size:0,pdata_rva:0,pdata_size:0,callbacks:[0;MAX_TLS_CALLBACKS],callback_count:0,used:false }; }
struct Store(core::cell::UnsafeCell<([ImageRecord;MAX_IMAGES],[RuntimeFunction;MAX_RUNTIME_FUNCTIONS],usize)>);
unsafe impl Sync for Store {}
static STORE:Store=Store(core::cell::UnsafeCell::new(([ImageRecord::EMPTY;MAX_IMAGES],[RuntimeFunction {image_base:0,begin:0,end:0,unwind_info:0};MAX_RUNTIME_FUNCTIONS],0)));
fn read(space:&AddressSpace,address:u64,out:&mut [u8])->Result<(),RuntimeError>{unsafe{space.copy_from_user(address,out).map_err(|_|RuntimeError::Vm)}}
fn u32_at(space:&AddressSpace,base:u64,rva:u32)->Result<u32,RuntimeError>{let mut b=[0;4];read(space,base.checked_add(rva as u64).ok_or(RuntimeError::InvalidImage)?,&mut b)?;Ok(u32::from_le_bytes(b))}
fn u64_at(space:&AddressSpace,base:u64,rva:u32)->Result<u64,RuntimeError>{let mut b=[0;8];read(space,base.checked_add(rva as u64).ok_or(RuntimeError::InvalidImage)?,&mut b)?;Ok(u64::from_le_bytes(b))}
/// Register bounded PE TLS and exception directories. Directory RVAs/sizes must
/// have been read from the validated optional header by the loader.
pub unsafe fn register(space:&AddressSpace,image:LoadedImage,tls_rva:u32,tls_size:u32,pdata_rva:u32,pdata_size:u32)->Result<ImageRuntime,RuntimeError>{
    if image.base==0||image.image_size==0{return Err(RuntimeError::InvalidImage);}
    let mut rec=ImageRecord{base:image.base,size:image.image_size,pdata_rva,pdata_size,..ImageRecord::EMPTY};
    if tls_rva!=0||tls_size!=0{
        if tls_rva==0||tls_size<40||tls_rva.checked_add(tls_size).is_none_or(|e|e>image.image_size){return Err(RuntimeError::InvalidDirectory);}
        let callbacks_va=u64_at(space,image.base,tls_rva+24)?;
        if callbacks_va!=0{
            if callbacks_va<image.base||callbacks_va>=image.base+image.image_size as u64{return Err(RuntimeError::InvalidDirectory);}
            let mut slot=callbacks_va;
            loop{
                if rec.callback_count==MAX_TLS_CALLBACKS{return Err(RuntimeError::TooManyCallbacks);}
                let rva=u32::try_from(slot-image.base).map_err(|_|RuntimeError::InvalidDirectory)?;
                let callback=u64_at(space,image.base,rva)?;
                if callback==0{break;}
                if callback<image.base||callback>=image.base+image.image_size as u64{return Err(RuntimeError::InvalidDirectory);}
                rec.callbacks[rec.callback_count]=callback;rec.callback_count+=1;
                slot=slot.checked_add(8).ok_or(RuntimeError::InvalidDirectory)?;
                if slot>=image.base+image.image_size as u64{return Err(RuntimeError::InvalidDirectory);}
            }
        }
    }
    if pdata_rva!=0||pdata_size!=0{
        if pdata_rva==0||pdata_size==0||pdata_rva.checked_add(pdata_size).is_none_or(|e|e>image.image_size)||pdata_size%12!=0{return Err(RuntimeError::InvalidDirectory);}
        let count=pdata_size as usize/12;if count>MAX_RUNTIME_FUNCTIONS{return Err(RuntimeError::TooManyFunctions);}
        let state=unsafe{&mut *STORE.0.get()};
        if state.2.checked_add(count).is_none_or(|n|n>MAX_RUNTIME_FUNCTIONS){return Err(RuntimeError::TooManyFunctions);}
        for i in 0..count{
            let r=pdata_rva+(i*12) as u32;
            let f=RuntimeFunction{image_base:image.base,begin:u32_at(space,image.base,r)?,end:u32_at(space,image.base,r+4)?,unwind_info:u32_at(space,image.base,r+8)?};
            if f.begin>=f.end||f.end>image.image_size||f.unwind_info>=image.image_size{return Err(RuntimeError::InvalidDirectory);}
            state.1[state.2]=f;state.2+=1;
        }
    }
    let state=unsafe{&mut *STORE.0.get()};
    let Some(slot)=state.0.iter_mut().find(|r|!r.used||r.base==image.base)else{return Err(RuntimeError::TooManyImages);};
    *slot=ImageRecord{used:true,..rec};
    Ok(ImageRuntime{base:image.base,tls_callbacks:rec.callbacks,tls_count:rec.callback_count,pdata_rva:rec.pdata_rva,pdata_size:rec.pdata_size})
}
/// Find a function table entry for a control PC. UNWIND_INFO interpretation is separate.
pub fn lookup_function(image_base:u64,control_pc:u64)->Option<RuntimeFunction>{
    let state=unsafe{&*STORE.0.get()};let rva=u32::try_from(control_pc.checked_sub(image_base)?).ok()?;
    state.1[..state.2].iter().copied().find(|f|f.image_base==image_base&&rva>=f.begin&&rva<f.end)
}
pub fn tls_callbacks(image_base:u64,out:&mut[u64])->Result<usize,RuntimeError>{
    let state=unsafe{&*STORE.0.get()};let rec=state.0.iter().find(|r|r.used&&r.base==image_base).ok_or(RuntimeError::InvalidImage)?;
    if out.len()<rec.callback_count{return Err(RuntimeError::TooManyCallbacks);}
    out[..rec.callback_count].copy_from_slice(&rec.callbacks[..rec.callback_count]);Ok(rec.callback_count)
}
