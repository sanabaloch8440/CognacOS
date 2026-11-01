//! Native Windows NLS file catalogue and opaque binary access.
//! NLS tables are consumed as their original bytes; this module does not convert
//! them into a RdotOS-specific format.
#![allow(dead_code)]
pub const MAX_NLS_FILE_SIZE:usize=16*1024*1024;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum NlsKind{CodePage037,CodePage437,CodePage1252,Locale,SortKey,SortTables,CType,Unicode}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Error{UnknownName,Empty,TooLarge,Truncated,InvalidArgument}
#[derive(Clone,Copy,Debug)]
pub struct NlsDescriptor{pub name:&'static str,pub kind:NlsKind,pub min_size:usize}
pub const FILES:[NlsDescriptor;8]=[
 NlsDescriptor{name:"c_037.nls",kind:NlsKind::CodePage037,min_size:256},
 NlsDescriptor{name:"c_437.nls",kind:NlsKind::CodePage437,min_size:256},
 NlsDescriptor{name:"c_1252.nls",kind:NlsKind::CodePage1252,min_size:256},
 NlsDescriptor{name:"locale.nls",kind:NlsKind::Locale,min_size:16},
 NlsDescriptor{name:"sortkey.nls",kind:NlsKind::SortKey,min_size:16},
 NlsDescriptor{name:"sorttbls.nls",kind:NlsKind::SortTables,min_size:16},
 NlsDescriptor{name:"ctype.nls",kind:NlsKind::CType,min_size:16},
 NlsDescriptor{name:"unicode.nls",kind:NlsKind::Unicode,min_size:16},
];
pub fn descriptor(name:&str)->Result<NlsDescriptor,Error>{FILES.iter().copied().find(|d|d.name.eq_ignore_ascii_case(name)).ok_or(Error::UnknownName)}
#[derive(Clone,Copy)]
pub struct NlsFile<'a>{pub descriptor:NlsDescriptor,bytes:&'a[u8]}
impl<'a>NlsFile<'a>{
 pub fn open(name:&str,bytes:&'a[u8])->Result<Self,Error>{let descriptor=descriptor(name)?;if bytes.is_empty(){return Err(Error::Empty)}if bytes.len()>MAX_NLS_FILE_SIZE{return Err(Error::TooLarge)}if bytes.len()<descriptor.min_size{return Err(Error::Truncated)}Ok(Self{descriptor,bytes})}
 pub fn bytes(&self)->&'a[u8]{self.bytes}
 pub fn len(&self)->usize{self.bytes.len()}
 pub fn is_empty(&self)->bool{self.bytes.is_empty()}
 pub fn copy_raw_to(&self,out:&mut[u8])->Result<usize,Error>{if out.len()<self.bytes.len(){return Err(Error::Truncated)}out[..self.bytes.len()].copy_from_slice(self.bytes);Ok(self.bytes.len())}
}
#[cfg(test)]mod tests{use super::*;#[test]fn catalogues_windows_names(){for f in FILES{assert_eq!(descriptor(f.name).unwrap().kind,f.kind);}}#[test]fn preserves_bytes_without_conversion(){let b=[0x81u8;256];let f=NlsFile::open("C_1252.NLS",&b).unwrap();assert_eq!(f.bytes(),&b);}}
