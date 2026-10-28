//! Restricted kernel-mode PE32+ driver loader.
//! Validates AMD64 native PE32+ images, applies DIR64 relocations, binds imports
//! through a pluggable resolver, and applies section-derived W^X permissions.
//! Third-party support is bounded by the exports/dependencies the resolver provides.
use core::ffi::c_void;
use crate::pe::ImportSymbol;

const IMAGE_FILE_DLL:u16=0x2000;
const IMAGE_SCN_MEM_EXECUTE:u32=0x2000_0000;
const IMAGE_SCN_MEM_WRITE:u32=0x8000_0000;
const IMAGE_SUBSYSTEM_NATIVE:u16=1;
const MAX_SECTIONS:usize=32;
const MAX_IMAGE_SIZE:usize=32*1024*1024;

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum DriverLoadError { Truncated, BadDosHeader, BadPeSignature, WrongMachine, WrongSubsystem, NotDll, InvalidHeader, InvalidSection, ImageTooLarge, WrongBase, ImportsUnsupported, ImportNotFound, InvalidImport, RelocationsUnsupported, RelocationUnsupported, WxSection, EntryNotExecutable, MappingFailed, NoMemory, EntryFailed(i32) }
#[derive(Clone,Copy,Debug)]
pub struct LoadedDriver { pub image_base:u64,pub image_size:u32,pub entry_point:u64,pub status:i32 }
const MAX_LOADED_DRIVER_IMAGES:usize=256;
#[derive(Clone,Copy)]
struct DriverImageRecord { used:bool, driver:usize, base:u64, size:u32 }
impl DriverImageRecord { const EMPTY:Self=Self{used:false,driver:0,base:0,size:0}; }
static LOADED_DRIVER_IMAGES:crate::hal::IrqSpinLock<[DriverImageRecord;MAX_LOADED_DRIVER_IMAGES]>=
 crate::hal::IrqSpinLock::new([DriverImageRecord::EMPTY;MAX_LOADED_DRIVER_IMAGES]);
fn remember_loaded_driver(driver:*mut DriverObject,base:u64,size:u32)->bool {
 if driver.is_null()||base==0||size==0{return false;}
 let mut records=LOADED_DRIVER_IMAGES.lock();
 if records.iter().any(|r|r.used&&r.driver==driver as usize){return false;}
 let Some(slot)=records.iter_mut().find(|r|!r.used) else{return false;};
 *slot=DriverImageRecord{used:true,driver:driver as usize,base,size};true
}
fn forget_loaded_driver(driver:*mut DriverObject,base:u64) {
 let mut records=LOADED_DRIVER_IMAGES.lock();
 if let Some(slot)=records.iter_mut().find(|r|r.used&&r.driver==driver as usize&&r.base==base){*slot=DriverImageRecord::EMPTY;}
}
/// Release the image associated with a driver after IoUnloadDriver has completed
/// device rundown and invoked DriverUnload. Unknown/non-loader-owned drivers are ignored.
pub unsafe fn unload_driver_image(driver:*mut c_void)->bool {
 if driver.is_null(){return false;}
 let record={let mut records=LOADED_DRIVER_IMAGES.lock();let Some(index)=records.iter().position(|r|r.used&&r.driver==driver as usize) else{return false;};let record=records[index];records[index]=DriverImageRecord::EMPTY;record};
 unsafe{unload_image(record.base,record.size)}.is_ok()
}

/// Windows x64 DRIVER_OBJECT layout passed to a native driver's DriverEntry.
/// Keep this layout in lockstep with the shared WDM ABI; a shortened host-side
/// record is not safe because real drivers index MajorFunction at offset 0x70.
#[repr(C)]
pub struct DriverObject {
 pub object_type:i16,
 pub size:i16,
 pub device_object:*mut c_void,
 pub flags:u32,
 pub driver_start:*mut c_void,
 pub driver_size:u32,
 pub driver_section:*mut c_void,
 pub driver_extension:*mut c_void,
 pub driver_name:crate::nt_abi::object::UNICODE_STRING,
 pub hardware_database:*mut crate::nt_abi::object::UNICODE_STRING,
 pub fast_io_dispatch:*mut c_void,
 pub driver_init:Option<unsafe extern "win64" fn(*mut DriverObject,*mut c_void)->i32>,
 pub driver_start_io:Option<unsafe extern "win64" fn(*mut c_void,*mut c_void)>,
 pub driver_unload:Option<unsafe extern "win64" fn(*mut DriverObject)>,
 pub major_function:[Option<unsafe extern "win64" fn(*mut c_void,*mut c_void)->i32>;28],
}

fn u16at(b:&[u8],o:usize)->Result<u16,DriverLoadError>{Ok(u16::from_le_bytes(b.get(o..o.checked_add(2).ok_or(DriverLoadError::InvalidHeader)?).ok_or(DriverLoadError::Truncated)?.try_into().map_err(|_|DriverLoadError::Truncated)?))}
/// Validate the canonical DOS/PE signatures and return the PE header offset.
/// Shared by both allocation and mapping paths so malformed signatures cannot
/// diverge between the two entry points.
fn pe_header_offset(file:&[u8])->Result<usize,DriverLoadError>{
 if file.len()<0x40{return Err(DriverLoadError::Truncated);}
 if u16at(file,0)?!=0x5a4d{return Err(DriverLoadError::BadDosHeader);}
 let nt=u32at(file,0x3c)? as usize;
 if file.get(nt..nt.checked_add(4).ok_or(DriverLoadError::InvalidHeader)?).ok_or(DriverLoadError::Truncated)?!=b"PE\0\0"{return Err(DriverLoadError::BadPeSignature);}
 Ok(nt)
}
fn u32at(b:&[u8],o:usize)->Result<u32,DriverLoadError>{Ok(u32::from_le_bytes(b.get(o..o.checked_add(4).ok_or(DriverLoadError::InvalidHeader)?).ok_or(DriverLoadError::Truncated)?.try_into().map_err(|_|DriverLoadError::Truncated)?))}
fn u64at(b:&[u8],o:usize)->Result<u64,DriverLoadError>{Ok(u64::from_le_bytes(b.get(o..o.checked_add(8).ok_or(DriverLoadError::InvalidHeader)?).ok_or(DriverLoadError::Truncated)?.try_into().map_err(|_|DriverLoadError::Truncated)?))}
fn range(total:usize,start:u32,len:u32)->Result<core::ops::Range<usize>,DriverLoadError>{let a=start as usize;let z=a.checked_add(len as usize).ok_or(DriverLoadError::InvalidHeader)?;if z>total{Err(DriverLoadError::Truncated)}else{Ok(a..z)}}

/// Load and invoke a native-subsystem AMD64 image using the kernel's current
/// implemented-export resolver. The mapped image is writable/NX staging memory.
pub unsafe fn load(file:&[u8],mapped:&mut[u8],base:u64,driver_object:*mut DriverObject,context:*mut c_void)->Result<LoadedDriver,DriverLoadError>{
 unsafe{load_with_resolver(file,mapped,base,driver_object,context,crate::ntoskrnl::resolve)}
}
/// Loader variant accepting the dependency manager's symbol resolver.
pub unsafe fn load_with_resolver(file:&[u8],mapped:&mut[u8],base:u64,driver_object:*mut DriverObject,context:*mut c_void,resolver:crate::pe::ImportResolver)->Result<LoadedDriver,DriverLoadError>{
 let nt=pe_header_offset(file)?;
 let coff=nt+4;
 if u16at(file,coff)?!=0x8664{return Err(DriverLoadError::WrongMachine);}
 let count=u16at(file,coff+2)? as usize;
 let characteristics=u16at(file,coff+18)?;
 if characteristics&IMAGE_FILE_DLL==0{return Err(DriverLoadError::NotDll);}
 let opt_size=u16at(file,coff+16)? as usize;let opt=coff+20;
 if count==0||count>MAX_SECTIONS||opt_size<240||u16at(file,opt)?!=0x20b{return Err(DriverLoadError::InvalidHeader);}
 if u16at(file,opt+68)?!=IMAGE_SUBSYSTEM_NATIVE{return Err(DriverLoadError::WrongSubsystem);}
 let entry_rva=u32at(file,opt+16)?;let preferred=u64at(file,opt+24)?;
 let section_align=u32at(file,opt+32)? as usize;let file_align=u32at(file,opt+36)? as usize;
 let image_size=u32at(file,opt+56)? as usize;let headers=u32at(file,opt+60)? as usize;
 let dirs=u32at(file,opt+108)? as usize;
 if dirs>16{return Err(DriverLoadError::InvalidHeader);}
 let import_rva=if dirs>1{u32at(file,opt+112+8)?}else{0};
 let import_size=if dirs>1{u32at(file,opt+112+12)?}else{0};
 let reloc_rva=if dirs>5{u32at(file,opt+112+5*8)?}else{0};
 let reloc_size=if dirs>5{u32at(file,opt+112+5*8+4)?}else{0};
 if mapped.as_mut_ptr() as u64!=base{return Err(DriverLoadError::WrongBase);}
 if (import_rva==0)!=(import_size==0)||(reloc_rva==0)!=(reloc_size==0){return Err(DriverLoadError::InvalidHeader);}
 if image_size==0||image_size>MAX_IMAGE_SIZE||mapped.len()<image_size||headers==0||headers>file.len()||headers>image_size||section_align<4096||!section_align.is_power_of_two()||file_align==0||!file_align.is_power_of_two()||entry_rva as usize>=image_size{return Err(DriverLoadError::InvalidHeader);}
 if import_rva!=0{let _=range(image_size,import_rva,import_size)?;}
 if reloc_rva!=0{let _=range(image_size,reloc_rva,reloc_size)?;}
 if base!=preferred&&reloc_rva==0{return Err(DriverLoadError::RelocationsUnsupported);}
 let table=opt.checked_add(opt_size).ok_or(DriverLoadError::InvalidHeader)?;
 if table.checked_add(count*40).is_none_or(|e|e>file.len()){return Err(DriverLoadError::Truncated);}
 mapped[..image_size].fill(0);mapped[..headers].copy_from_slice(file.get(..headers).ok_or(DriverLoadError::Truncated)?);
 let mut entry_executable=false;
 let mut section_ranges=[(0usize,0usize);MAX_SECTIONS];let mut section_range_count=0usize;
 for i in 0..count{
  let o=table+i*40;let virtual_size=u32at(file,o+8)?;let rva=u32at(file,o+12)?;let raw_size=u32at(file,o+16)?;let raw_off=u32at(file,o+20)?;let flags=u32at(file,o+36)?;
  let span=core::cmp::max(virtual_size,raw_size);
  let end=(rva as usize).checked_add(span as usize).ok_or(DriverLoadError::InvalidSection)?;
  if (rva as usize)%section_align!=0||end>image_size||(rva as usize)<headers||(raw_size>0&&raw_off<headers as u32){return Err(DriverLoadError::InvalidSection);}
  if span!=0&&section_ranges[..section_range_count].iter().any(|(a,b)|(rva as usize)<*b&&*a<end){return Err(DriverLoadError::InvalidSection);}
  if span!=0{section_ranges[section_range_count]= (rva as usize,end);section_range_count+=1;}
  if flags&IMAGE_SCN_MEM_EXECUTE!=0&&flags&IMAGE_SCN_MEM_WRITE!=0{return Err(DriverLoadError::WxSection);}
  if flags&IMAGE_SCN_MEM_EXECUTE!=0&&entry_rva>=rva&&entry_rva<rva.saturating_add(span){entry_executable=true;}
  if raw_size!=0{let src=range(file.len(),raw_off,raw_size)?;let dst=(rva as usize)..(rva as usize+raw_size as usize);mapped[dst].copy_from_slice(&file[src]);}
 }
 if !entry_executable{return Err(DriverLoadError::EntryNotExecutable);}
 if reloc_rva!=0{apply_relocations(mapped,base,preferred,reloc_rva,reloc_size)?;}
 if import_rva!=0{apply_imports(mapped,import_rva,import_size,resolver)?;}
 // Default the whole image to read-only/NX, then grant only the section
 // permissions declared by validated section characteristics.
 let image_pages=(image_size+4095)/4096;
 crate::kernel_vm::protect_pages(base,image_pages,false,false).map_err(|_|DriverLoadError::MappingFailed)?;
 for i in 0..count{
  let o=table+i*40;let virtual_size=u32at(file,o+8)?;let rva=u32at(file,o+12)?;let raw_size=u32at(file,o+16)?;let flags=u32at(file,o+36)?;
  let span=core::cmp::max(virtual_size,raw_size) as usize;
  if span==0{continue;}
  let pages=(span+4095)/4096;
  crate::kernel_vm::protect_pages(base+rva as u64,pages,flags&IMAGE_SCN_MEM_WRITE!=0,flags&IMAGE_SCN_MEM_EXECUTE!=0).map_err(|_|DriverLoadError::MappingFailed)?;
 }
 // Register exact section ranges only after mapping and final protection checks.
 // PAGE/PAGEDATA sections are evictable; all other sections receive lifetime
 // PFN pins so executable code and ordinary data cannot be reclaimed.
 for i in 0..count {
  let o=table+i*40;
  let virtual_size=u32at(file,o+8)?;
  let rva=u32at(file,o+12)?;
  let raw_size=u32at(file,o+16)?;
  let span=core::cmp::max(virtual_size,raw_size) as u64;
  if span==0 { continue; }
  let name=file.get(o..o+8).ok_or(DriverLoadError::Truncated)?;
  let name_len=name.iter().position(|b|*b==0).unwrap_or(name.len());
  let name=&name[..name_len];
  let pageable=name.starts_with(b"PAGE") || name.starts_with(b".PAGE");
  if !crate::ntoskrnl::mm::runtime::register_driver_image_section(base,base+rva as u64,span,pageable) {
   crate::ntoskrnl::mm::runtime::unregister_driver_image_sections(base);
   return Err(DriverLoadError::MappingFailed);
  }
 }
 let entry=base.checked_add(entry_rva as u64).ok_or(DriverLoadError::InvalidHeader)?;
 let routine:unsafe extern "win64" fn(*mut DriverObject,*mut c_void)->i32=unsafe{core::mem::transmute(entry as usize)};
 let status=unsafe{routine(driver_object,context)};
 if status<0 {
  crate::ntoskrnl::mm::runtime::unregister_driver_image_sections(base);
  return Err(DriverLoadError::EntryFailed(status));
 }
 Ok(LoadedDriver{image_base:base,image_size:image_size as u32,entry_point:entry,status})
}

/// Allocate an image at its preferred reserved slot when available, otherwise
/// allocate another kernel-VM extent and apply AMD64 base relocations.
pub unsafe fn load_allocated(file:&[u8],driver_object:*mut DriverObject,context:*mut c_void)->Result<LoadedDriver,DriverLoadError>{
 unsafe{load_allocated_with_resolver(file,driver_object,context,crate::ntoskrnl::resolve)}
}
pub unsafe fn load_allocated_with_resolver(file:&[u8],driver_object:*mut DriverObject,context:*mut c_void,resolver:crate::pe::ImportResolver)->Result<LoadedDriver,DriverLoadError>{
 let nt=pe_header_offset(file)?;
 let opt=nt.checked_add(4+20).ok_or(DriverLoadError::InvalidHeader)?;
 if u16at(file,nt+4)?!=0x8664||u16at(file,opt)?!=0x20b{return Err(DriverLoadError::WrongMachine);}
 let preferred=u64at(file,opt+24)?;let size=u32at(file,opt+56)? as usize;
 if size==0||size>MAX_IMAGE_SIZE{return Err(DriverLoadError::ImageTooLarge);}
 let pages=size.checked_add(4095).ok_or(DriverLoadError::InvalidHeader)?/4096;
 // Driver images use pageable kernel mappings. Preserve an available reserved
 // preferred slot (important for images without relocation data); otherwise the
 // loader relocates into a demand-paged extent. Section metadata pins non-PAGE
 // sections permanently and MmLockPagableDataSection pins PAGE sections on demand.
 let mapped=if crate::kernel_vm::is_driver_image_base(preferred) {
  crate::kernel_vm::allocate_pageable_pages_at(preferred,pages).or_else(|_|crate::kernel_vm::allocate_pageable_pages(pages))
 } else { crate::kernel_vm::allocate_pageable_pages(pages) }.map_err(|_|DriverLoadError::NoMemory)?;
 let base=mapped as u64;
 let image=unsafe{core::slice::from_raw_parts_mut(mapped,size)};
 // Reserve unload bookkeeping before DriverEntry can publish devices or callbacks.
 if !remember_loaded_driver(driver_object,base,size as u32) {
  let _=crate::kernel_vm::free_pages(mapped,pages);
  return Err(DriverLoadError::NoMemory);
 }
 match unsafe{load_with_resolver(file,image,base,driver_object,context,resolver)}{
  Ok(driver)=>Ok(driver),
  Err(error)=>{forget_loaded_driver(driver_object,base);crate::ntoskrnl::mm::runtime::unregister_driver_image_sections(base);let _=crate::kernel_vm::free_pages(mapped,pages);Err(error)}
 }
}

/// Release a driver image after DriverUnload has returned and all image
/// callbacks/IRPs have drained. Callers must not unload while any CPU can execute
/// inside the image.
pub unsafe fn unload_image(image_base:u64,image_size:u32)->Result<(),DriverLoadError>{
 if image_base<0xffff_8000_0000_0000||image_size==0{return Err(DriverLoadError::InvalidHeader)}
 let pages=(image_size as usize).checked_add(4095).ok_or(DriverLoadError::InvalidHeader)?/4096;
 crate::ntoskrnl::mm::runtime::unregister_driver_image_sections(image_base);
 crate::kernel_vm::free_pages(image_base as *mut u8,pages).map_err(|_|DriverLoadError::MappingFailed)
}

fn put_u64(bytes:&mut[u8],offset:usize,value:u64)->Result<(),DriverLoadError>{
 let end=offset.checked_add(8).ok_or(DriverLoadError::InvalidHeader)?;
 bytes.get_mut(offset..end).ok_or(DriverLoadError::Truncated)?.copy_from_slice(&value.to_le_bytes());Ok(())
}
fn c_string(bytes:&[u8],rva:u32,maximum:usize)->Result<&[u8],DriverLoadError>{
 let tail=bytes.get(rva as usize..).ok_or(DriverLoadError::InvalidImport)?;
 let len=tail.iter().take(maximum).position(|b|*b==0).ok_or(DriverLoadError::InvalidImport)?;
 if len==0{return Err(DriverLoadError::InvalidImport)}Ok(&tail[..len])
}
fn apply_relocations(image:&mut[u8],base:u64,preferred:u64,rva:u32,size:u32)->Result<(),DriverLoadError>{
 if base==preferred{return Ok(())}
 let delta=base as i128-preferred as i128;let start=rva as usize;let end=start.checked_add(size as usize).ok_or(DriverLoadError::InvalidHeader)?;
 if end>image.len(){return Err(DriverLoadError::Truncated)}
 let mut cursor=start;
 while cursor<end{
  if cursor+8>end{return Err(DriverLoadError::InvalidHeader)}
  let page=u32at(image,cursor)?;let block=u32at(image,cursor+4)? as usize;
  if block<8||block%2!=0||cursor.checked_add(block).is_none_or(|e|e>end){return Err(DriverLoadError::InvalidHeader)}
  let count=(block-8)/2;
  for i in 0..count{
   let entry=u16at(image,cursor+8+i*2)?;let kind=entry>>12;let offset=(entry&0x0fff) as usize;
   if kind==0{continue;}if kind!=10{return Err(DriverLoadError::RelocationUnsupported);}
   let target=(page as usize).checked_add(offset).ok_or(DriverLoadError::InvalidHeader)?;
   let old=u64at(image,target)? as i128;let value=old+delta;
   if value<0||value>u64::MAX as i128{return Err(DriverLoadError::InvalidHeader);}
   put_u64(image,target,value as u64)?;
  }
  cursor+=block;
 }
 if cursor!=end{return Err(DriverLoadError::InvalidHeader)}Ok(())
}
fn apply_imports(image:&mut[u8],rva:u32,size:u32,resolver:crate::pe::ImportResolver)->Result<(),DriverLoadError>{
 let start=rva as usize;let end=start.checked_add(size as usize).ok_or(DriverLoadError::InvalidHeader)?;
 if end>image.len()||size<20{return Err(DriverLoadError::InvalidImport)}
 let mut desc=start;let mut terminated=false;
 while desc.checked_add(20).is_some_and(|e|e<=end){
  let original=u32at(image,desc)?;let timestamp=u32at(image,desc+4)?;let forwarder=u32at(image,desc+8)?;let name_rva=u32at(image,desc+12)?;let first=u32at(image,desc+16)?;
  if original==0&&timestamp==0&&forwarder==0&&name_rva==0&&first==0{terminated=true;break;}
  if name_rva==0||first==0{return Err(DriverLoadError::InvalidImport)}
  let dll_src=c_string(image,name_rva,260)?;let dll_len=dll_src.len();let mut dll_buf=[0u8;260];dll_buf[..dll_len].copy_from_slice(dll_src);let dll=&dll_buf[..dll_len];
  let source=if original==0{first}else{original};
  let max_thunks=image.len()/8;let mut thunk_index=0usize;let mut thunk_terminated=false;
  while thunk_index<max_thunks{
   let src=(source as usize).checked_add(thunk_index.checked_mul(8).ok_or(DriverLoadError::InvalidImport)?).ok_or(DriverLoadError::InvalidImport)?;
   let dst=(first as usize).checked_add(thunk_index.checked_mul(8).ok_or(DriverLoadError::InvalidImport)?).ok_or(DriverLoadError::InvalidImport)?;
   let thunk=u64at(image,src)?;if thunk==0{thunk_terminated=true;break;}
   let mut symbol_buf=[0u8;512];let symbol=if thunk&(1u64<<63)!=0{
    if thunk&0x7fff_ffff_ffff_0000!=0{return Err(DriverLoadError::InvalidImport)}
    ImportSymbol::Ordinal((thunk&0xffff) as u16)
   }else{
    if thunk>u32::MAX as u64{return Err(DriverLoadError::InvalidImport)}
    let name_offset=(thunk as usize).checked_add(2).ok_or(DriverLoadError::InvalidImport)?;
    let name_rva=u32::try_from(name_offset).map_err(|_|DriverLoadError::InvalidImport)?;
    let symbol_src=c_string(image,name_rva,512)?;let symbol_len=symbol_src.len();symbol_buf[..symbol_len].copy_from_slice(symbol_src);ImportSymbol::Name(&symbol_buf[..symbol_len])
   };
   let address=resolver(dll,symbol).ok_or(DriverLoadError::ImportNotFound)?;
   put_u64(image,dst,address)?;thunk_index+=1;
  }
  if !thunk_terminated{return Err(DriverLoadError::InvalidImport)}
  desc+=20;
 }
 if !terminated{return Err(DriverLoadError::InvalidImport)}Ok(())
}

/// Errors from resolving and loading a service selected by the native SYSTEM hive.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum RegistryDriverLoadError{
 ServiceConfig(crate::ntoskrnl::cm::driver_config::Error),
 StartTypeUnsupported,NotKernelDriver,ImageResolutionFailed,ImageTooLarge,
 Load(DriverLoadError),
}
/// Resolve a service under the active ControlSet in SYSTEM, read its ImagePath,
/// obtain the image through the caller's filesystem resolver, then load it.
/// No driver filename or service name is baked into the loader.
pub unsafe fn load_registered_service<'h,'p,F>(
 hive:&'h crate::ntoskrnl::cm::hive::Hive<'h>,service_name:&[u16],path_buffer:&'p mut[u16],
 image_buffer:&mut[u8],driver_object:*mut DriverObject,context:*mut c_void,resolve_image:F,
)->Result<LoadedDriver,RegistryDriverLoadError>
where F:FnMut(&[u16],&mut[u8])->Option<usize>{
 unsafe{load_registered_service_with_resolver(hive,service_name,path_buffer,image_buffer,driver_object,context,resolve_image,crate::ntoskrnl::resolve)}
}
pub unsafe fn load_registered_service_with_resolver<'h,'p,F>(
 hive:&'h crate::ntoskrnl::cm::hive::Hive<'h>,service_name:&[u16],path_buffer:&'p mut[u16],
 image_buffer:&mut[u8],driver_object:*mut DriverObject,context:*mut c_void,mut resolve_image:F,
 symbol_resolver:crate::pe::ImportResolver,
)->Result<LoadedDriver,RegistryDriverLoadError>
where F:FnMut(&[u16],&mut[u8])->Option<usize>{
 let config=crate::ntoskrnl::cm::driver_config::query_service(hive,service_name,path_buffer)
  .map_err(RegistryDriverLoadError::ServiceConfig)?;
 if config.start>1{return Err(RegistryDriverLoadError::StartTypeUnsupported);}
 if config.service_type!=1&&config.service_type!=2{return Err(RegistryDriverLoadError::NotKernelDriver);}
 let len=resolve_image(config.image_path,image_buffer).ok_or(RegistryDriverLoadError::ImageResolutionFailed)?;
 if len==0||len>image_buffer.len(){return Err(RegistryDriverLoadError::ImageTooLarge);}
 unsafe{load_allocated_with_resolver(&image_buffer[..len],driver_object,context,symbol_resolver)}.map_err(RegistryDriverLoadError::Load)
}
#[cfg(test)]
mod pe_signature_tests {
 use super::*;
 #[test]
 fn driver_object_matches_windows_x64_size_and_dispatch_offset() {
  assert_eq!(core::mem::offset_of!(DriverObject, device_object), 0x08);
  assert_eq!(core::mem::offset_of!(DriverObject, driver_name), 0x38);
  assert_eq!(core::mem::offset_of!(DriverObject, driver_unload), 0x68);
  assert_eq!(core::mem::offset_of!(DriverObject, major_function), 0x70);
  assert_eq!(core::mem::size_of::<DriverObject>(), 0x150);
 }
 #[test]
 fn accepts_canonical_pe_signature() {
  let mut file=vec![0u8;0x44];
  file[0..2].copy_from_slice(&0x5a4du16.to_le_bytes());
  file[0x3c..0x40].copy_from_slice(&0x40u32.to_le_bytes());
  file[0x40..0x44].copy_from_slice(b"PE\0\0");
  assert_eq!(pe_header_offset(&file),Ok(0x40));
 }
 #[test]
 fn rejects_backslash_zero_signature() {
  let mut file=vec![0u8;0x46];
  file[0..2].copy_from_slice(&0x5a4du16.to_le_bytes());
  file[0x3c..0x40].copy_from_slice(&0x40u32.to_le_bytes());
  file[0x40..0x46].copy_from_slice(b"PE\\\\0\\\\0");
  assert_eq!(pe_header_offset(&file),Err(DriverLoadError::BadPeSignature));
 }
 #[test]
 fn rejects_out_of_bounds_pe_header() {
  let mut file=vec![0u8;0x40];
  file[0..2].copy_from_slice(&0x5a4du16.to_le_bytes());
  file[0x3c..0x40].copy_from_slice(&0xfffffff0u32.to_le_bytes());
  assert_eq!(pe_header_offset(&file),Err(DriverLoadError::Truncated));
 }
}
