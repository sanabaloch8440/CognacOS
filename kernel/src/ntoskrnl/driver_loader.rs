//! Restricted kernel-mode PE32+ driver loader.
//! The caller must reserve/map the image at its preferred virtual base with W^X
//! section permissions before invoking this routine. Imports and relocations are
//! deliberately rejected until kernel-module dependency binding is implemented.
use core::ffi::c_void;

const IMAGE_FILE_DLL:u16=0x2000;
const IMAGE_SCN_MEM_EXECUTE:u32=0x2000_0000;
const IMAGE_SCN_MEM_WRITE:u32=0x8000_0000;
const IMAGE_SUBSYSTEM_NATIVE:u16=1;
const MAX_SECTIONS:usize=32;
const MAX_IMAGE_SIZE:usize=32*1024*1024;

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum DriverLoadError { Truncated, BadDosHeader, BadPeSignature, WrongMachine, WrongSubsystem, NotDll, InvalidHeader, InvalidSection, ImageTooLarge, WrongBase, ImportsUnsupported, RelocationsUnsupported, WxSection, EntryNotExecutable, EntryFailed(i32) }
#[derive(Clone,Copy,Debug)]
pub struct LoadedDriver { pub image_base:u64,pub image_size:u32,pub entry_point:u64,pub status:i32 }
#[repr(C)]
pub struct DriverObject { pub size:u32,pub flags:u32,pub driver_start:*mut c_void,pub driver_size:u32,pub driver_section:*mut c_void,pub driver_extension:*mut c_void,pub unload:Option<unsafe extern "win64" fn(*mut DriverObject)> }

fn u16at(b:&[u8],o:usize)->Result<u16,DriverLoadError>{Ok(u16::from_le_bytes(b.get(o..o.checked_add(2).ok_or(DriverLoadError::InvalidHeader)?).ok_or(DriverLoadError::Truncated)?.try_into().map_err(|_|DriverLoadError::Truncated)?))}
fn u32at(b:&[u8],o:usize)->Result<u32,DriverLoadError>{Ok(u32::from_le_bytes(b.get(o..o.checked_add(4).ok_or(DriverLoadError::InvalidHeader)?).ok_or(DriverLoadError::Truncated)?.try_into().map_err(|_|DriverLoadError::Truncated)?))}
fn u64at(b:&[u8],o:usize)->Result<u64,DriverLoadError>{Ok(u64::from_le_bytes(b.get(o..o.checked_add(8).ok_or(DriverLoadError::InvalidHeader)?).ok_or(DriverLoadError::Truncated)?.try_into().map_err(|_|DriverLoadError::Truncated)?))}
fn range(total:usize,start:u32,len:u32)->Result<core::ops::Range<usize>,DriverLoadError>{let a=start as usize;let z=a.checked_add(len as usize).ok_or(DriverLoadError::InvalidHeader)?;if z>total{Err(DriverLoadError::Truncated)}else{Ok(a..z)}}

/// Load and invoke a native-subsystem AMD64 driver image already mapped at its
/// preferred base. Caller owns page allocation and final RX/R/RW page protection.
/// This deliberately supports only self-contained images with no imports/relocs.
pub unsafe fn load(file:&[u8],mapped:&mut[u8],base:u64,driver_object:*mut DriverObject,context:*mut c_void)->Result<LoadedDriver,DriverLoadError>{
 if file.len()<0x40{return Err(DriverLoadError::Truncated);}
 if u16at(file,0)?!=0x5a4d{return Err(DriverLoadError::BadDosHeader);}
 let nt=u32at(file,0x3c)? as usize;
 if file.get(nt..nt.checked_add(4).ok_or(DriverLoadError::InvalidHeader)?).ok_or(DriverLoadError::Truncated)?!=b"PE\0\0"{return Err(DriverLoadError::BadPeSignature);}
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
 if preferred!=base{return Err(DriverLoadError::WrongBase);}
 if image_size==0||image_size>MAX_IMAGE_SIZE||mapped.len()<image_size||headers==0||headers>file.len()||headers>image_size||section_align<4096||!section_align.is_power_of_two()||file_align==0||!file_align.is_power_of_two()||entry_rva as usize>=image_size{return Err(DriverLoadError::InvalidHeader);}
 if mapped.as_mut_ptr() as u64!=base{return Err(DriverLoadError::WrongBase);}
 if dirs>1&&(u32at(file,opt+112+8)?!=0||u32at(file,opt+112+12)?!=0){return Err(DriverLoadError::ImportsUnsupported);}
 if dirs>5&&(u32at(file,opt+112+5*8)?!=0||u32at(file,opt+112+5*8+4)?!=0){return Err(DriverLoadError::RelocationsUnsupported);}
 let table=opt.checked_add(opt_size).ok_or(DriverLoadError::InvalidHeader)?;
 if table.checked_add(count*40).is_none_or(|e|e>file.len()){return Err(DriverLoadError::Truncated);}
 mapped[..image_size].fill(0);mapped[..headers].copy_from_slice(file.get(..headers).ok_or(DriverLoadError::Truncated)?);
 let mut entry_executable=false;
 for i in 0..count{
  let o=table+i*40;let virtual_size=u32at(file,o+8)?;let rva=u32at(file,o+12)?;let raw_size=u32at(file,o+16)?;let raw_off=u32at(file,o+20)?;let flags=u32at(file,o+36)?;
  let span=core::cmp::max(virtual_size,raw_size);
  let end=(rva as usize).checked_add(span as usize).ok_or(DriverLoadError::InvalidSection)?;
  if (rva as usize)%section_align!=0||end>image_size||(rva as usize)<headers||(raw_size>0&&raw_off<headers as u32){return Err(DriverLoadError::InvalidSection);}
  if flags&IMAGE_SCN_MEM_EXECUTE!=0&&flags&IMAGE_SCN_MEM_WRITE!=0{return Err(DriverLoadError::WxSection);}
  if flags&IMAGE_SCN_MEM_EXECUTE!=0&&entry_rva>=rva&&entry_rva<rva.saturating_add(span){entry_executable=true;}
  if raw_size!=0{let src=range(file.len(),raw_off,raw_size)?;let dst=(rva as usize)..(rva as usize+raw_size as usize);mapped[dst].copy_from_slice(&file[src]);}
 }
 if !entry_executable{return Err(DriverLoadError::EntryNotExecutable);}
 let entry=base.checked_add(entry_rva as u64).ok_or(DriverLoadError::InvalidHeader)?;
 let routine:unsafe extern "win64" fn(*mut DriverObject,*mut c_void)->i32=unsafe{core::mem::transmute(entry as usize)};
 let status=unsafe{routine(driver_object,context)};
 if status<0{return Err(DriverLoadError::EntryFailed(status));}
 Ok(LoadedDriver{image_base:base,image_size:image_size as u32,entry_point:entry,status})
}
