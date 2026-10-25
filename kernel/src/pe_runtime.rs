//! Runtime metadata for loaded PE32+ images: TLS callbacks and x64 .pdata.
//! Callbacks are recorded for user-mode startup; they are never called in ring 0.
use crate::{pe::LoadedImage, ntoskrnl::mm::address_space::AddressSpace};
use core::sync::atomic::{AtomicBool, Ordering};
static RUNTIME_LOCK: AtomicBool = AtomicBool::new(false);
struct RuntimeGuard;
impl RuntimeGuard { fn acquire() -> Self { while RUNTIME_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() { core::hint::spin_loop(); } Self } }
impl Drop for RuntimeGuard { fn drop(&mut self) { RUNTIME_LOCK.store(false, Ordering::Release); } }
const MAX_IMAGES: usize = 32;
const MAX_TLS_CALLBACKS: usize = 64;
const MAX_RUNTIME_FUNCTIONS: usize = 8192;
#[derive(Clone, Copy, Debug)]
pub enum RuntimeError { InvalidImage, TooManyImages, TooManyCallbacks, TooManyFunctions, InvalidDirectory, Vm }
#[derive(Clone, Copy, Debug)]
pub struct RuntimeFunction { pub image_base: u64, pub begin: u32, pub end: u32, pub unwind_info: u32 }
#[derive(Clone, Copy, Debug)]
pub struct ImageRuntime { pub base: u64, pub tls_callbacks: [u64; MAX_TLS_CALLBACKS], pub tls_count: usize, pub tls_template_start: u64, pub tls_template_size: u32, pub tls_index_address: u64, pub tls_zero_fill: u32, pub pdata_rva: u32, pub pdata_size: u32 }
#[derive(Clone, Copy)]
struct ImageRecord { base:u64, size:u32, pdata_rva:u32, pdata_size:u32, tls_template_start:u64, tls_template_size:u32, tls_index_address:u64, tls_zero_fill:u32, callbacks:[u64;MAX_TLS_CALLBACKS], callback_count:usize, used:bool }
impl ImageRecord { const EMPTY:Self=Self { base:0,size:0,pdata_rva:0,pdata_size:0,tls_template_start:0,tls_template_size:0,tls_index_address:0,tls_zero_fill:0,callbacks:[0;MAX_TLS_CALLBACKS],callback_count:0,used:false }; }
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
        let start_va=u64_at(space,image.base,tls_rva)?;
        let end_va=u64_at(space,image.base,tls_rva+8)?;
        let index_va=u64_at(space,image.base,tls_rva+16)?;
        let template_size=if start_va==0&&end_va==0 { 0 } else {
            if start_va<image.base||end_va<start_va||end_va>image.base+image.image_size as u64{return Err(RuntimeError::InvalidDirectory);}
            u32::try_from(end_va-start_va).map_err(|_|RuntimeError::InvalidDirectory)?
        };
        if index_va!=0&&(index_va<image.base||index_va.checked_add(4).is_none_or(|e|e>image.base+image.image_size as u64)){return Err(RuntimeError::InvalidDirectory);}
        rec.tls_template_start=start_va;
        rec.tls_template_size=template_size;
        rec.tls_index_address=index_va;
        rec.tls_zero_fill=u32_at(space,image.base,tls_rva+32)?;
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
    let function_count = if pdata_rva!=0||pdata_size!=0 {
        if pdata_rva==0||pdata_size==0||pdata_rva.checked_add(pdata_size).is_none_or(|e|e>image.image_size)||pdata_size%12!=0{return Err(RuntimeError::InvalidDirectory);}
        let count=pdata_size as usize/12;if count>MAX_RUNTIME_FUNCTIONS{return Err(RuntimeError::TooManyFunctions);}
        // Validate the complete directory before mutating global state. A malformed
        // late entry must not leave a partially registered table behind.
        let mut previous_end=0u32;
        for i in 0..count{
            let r=pdata_rva+(i*12) as u32;
            let f=RuntimeFunction{image_base:image.base,begin:u32_at(space,image.base,r)?,end:u32_at(space,image.base,r+4)?,unwind_info:u32_at(space,image.base,r+8)?};
            if f.begin>=f.end||f.begin<previous_end||f.end>image.image_size
                ||f.unwind_info>=image.image_size||f.unwind_info&3!=0{return Err(RuntimeError::InvalidDirectory);}
            previous_end=f.end;
        }
        count
    } else { 0 };
    let _guard=RuntimeGuard::acquire();
    let state=unsafe{&mut *STORE.0.get()};
    if state.2.checked_add(function_count).is_none_or(|n|n>MAX_RUNTIME_FUNCTIONS){return Err(RuntimeError::TooManyFunctions);}
    let Some(slot_index)=state.0.iter().position(|r|!r.used||r.base==image.base)else{return Err(RuntimeError::TooManyImages);};
    // Append only after all entries and destination capacity are validated. Roll
    // back the visible count if the mapped user image changes during the copy.
    let old_count=state.2;
    for i in 0..function_count {
        let r=pdata_rva+(i*12) as u32;
        let entry=RuntimeFunction{image_base:image.base,begin:u32_at(space,image.base,r)?,end:u32_at(space,image.base,r+4)?,unwind_info:u32_at(space,image.base,r+8)?};
        if entry.begin>=entry.end||entry.end>image.image_size||entry.unwind_info>=image.image_size||entry.unwind_info&3!=0 {
            state.2=old_count;
            return Err(RuntimeError::InvalidDirectory);
        }
        state.1[state.2]=entry;state.2+=1;
    }
    state.0[slot_index]=ImageRecord{used:true,..rec};
    Ok(ImageRuntime{base:image.base,tls_callbacks:rec.callbacks,tls_count:rec.callback_count,tls_template_start:rec.tls_template_start,tls_template_size:rec.tls_template_size,tls_index_address:rec.tls_index_address,tls_zero_fill:rec.tls_zero_fill,pdata_rva:rec.pdata_rva,pdata_size:rec.pdata_size})
}
/// Find a function table entry for a control PC. UNWIND_INFO interpretation is separate.
pub fn lookup_function(image_base:u64,control_pc:u64)->Option<RuntimeFunction>{
    let _guard=RuntimeGuard::acquire();
    let state=unsafe{&*STORE.0.get()};let rva=u32::try_from(control_pc.checked_sub(image_base)?).ok()?;
    state.1[..state.2].iter().copied().find(|f|f.image_base==image_base&&rva>=f.begin&&rva<f.end)
}
/// Resolve a PC against all loaded images' validated .pdata registrations.
pub fn lookup_any_function(control_pc:u64)->Option<RuntimeFunction>{
    let _guard=RuntimeGuard::acquire();
    let state=unsafe{&*STORE.0.get()};
    state.1[..state.2].iter().copied().find(|f|{
        control_pc.checked_sub(f.image_base).and_then(|r|u32::try_from(r).ok())
            .is_some_and(|rva|rva>=f.begin&&rva<f.end)
    })
}
pub fn tls_callbacks(image_base:u64,out:&mut[u64])->Result<usize,RuntimeError>{
    let _guard=RuntimeGuard::acquire();
    let state=unsafe{&*STORE.0.get()};let rec=state.0.iter().find(|r|r.used&&r.base==image_base).ok_or(RuntimeError::InvalidImage)?;
    if out.len()<rec.callback_count{return Err(RuntimeError::TooManyCallbacks);}
    out[..rec.callback_count].copy_from_slice(&rec.callbacks[..rec.callback_count]);Ok(rec.callback_count)
}

/// Remove an image's runtime registration before its address space is reclaimed.
pub fn unregister_image(image_base:u64) {
    let _guard=RuntimeGuard::acquire();
    let state=unsafe{&mut *STORE.0.get()};
    for rec in state.0.iter_mut() {
        if rec.used && rec.base==image_base { *rec=ImageRecord::EMPTY; }
    }
    let mut write=0usize;
    for read in 0..state.2 {
        if state.1[read].image_base!=image_base {
            if write!=read { state.1[write]=state.1[read]; }
            write+=1;
        }
    }
    for i in write..state.2 { state.1[i]=RuntimeFunction{image_base:0,begin:0,end:0,unwind_info:0}; }
    state.2=write;
}

#[derive(Clone, Copy, Debug)]
pub struct TlsLayout { pub present: bool, pub template_start: u64, pub template_size: u32, pub index_address: u64, pub zero_fill: u32 }
pub fn tls_layout(image_base:u64)->Result<TlsLayout,RuntimeError>{
    let _guard=RuntimeGuard::acquire();
    let state=unsafe{&*STORE.0.get()};
    let r=state.0.iter().find(|r|r.used&&r.base==image_base).ok_or(RuntimeError::InvalidImage)?;
    Ok(TlsLayout{present:r.tls_template_start!=0||r.tls_template_size!=0||r.tls_index_address!=0||r.tls_zero_fill!=0||r.callback_count!=0,
        template_start:r.tls_template_start,template_size:r.tls_template_size,index_address:r.tls_index_address,zero_fill:r.tls_zero_fill})
}
