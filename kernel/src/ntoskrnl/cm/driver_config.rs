//! Registry-backed Windows driver service configuration.
//! Values are read directly from the native SYSTEM hive; the hive is not rewritten.
#![allow(dead_code)]
use super::hive::{Error as HiveError,Hive};
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Error{Hive(HiveError),MissingService,MissingImagePath,InvalidImagePath,MissingStart,MissingType,UnsupportedStart}
impl From<HiveError> for Error{fn from(e:HiveError)->Self{Self::Hive(e)}}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct ServiceConfig<'a>{pub start:u32,pub service_type:u32,pub image_path:&'a[u8],pub error_control:u32}
fn value_u32(key:super::hive::Key<'_>,name:&[u16])->Result<u32,Error>{let v=key.find_value(name)?;if v.kind!=4||v.data_len<4{return Err(Error::InvalidImagePath)}let mut b=[0u8;4];v.data(&mut b)?;Ok(u32::from_le_bytes(b))}
pub fn query_service<'a>(hive:&'a Hive<'a>,service_name:&[u16],path_buf:&mut[u16])->Result<ServiceConfig<'a>,Error>{
 let mut path=[0u16;256];let mut n=0usize;for c in "\\ControlSet001\\Services\\".encode_utf16(){if n>=path.len(){return Err(Error::InvalidImagePath)}path[n]=c;n+=1;}if n+service_name.len()>path.len(){return Err(Error::InvalidImagePath)}path[n..n+service_name.len()].copy_from_slice(service_name);n+=service_name.len();
 let key=hive.find_path(&path[..n])?;
 let start=value_u32(key,&[83,116,97,114,116])?;let service_type=value_u32(key,&[84,121,112,101])?;
 let error_control=value_u32(key,&[69,114,114,111,114,67,111,110,116,114,111,108]).unwrap_or(0);
 let v=key.find_value(&[73,109,97,103,101,80,97,116,104])?;if v.kind!=1&&v.kind!=2{return Err(Error::InvalidImagePath)}let bytes_len=v.data_len as usize;if bytes_len<2||bytes_len%2!=0||bytes_len>path_buf.len()*2{return Err(Error::InvalidImagePath)}
 let mut raw=[0u8;1024];if bytes_len>raw.len(){return Err(Error::InvalidImagePath)}v.data(&mut raw[..bytes_len])?;
 for i in 0..bytes_len/2{path_buf[i]=u16::from_le_bytes([raw[i*2],raw[i*2+1]]);}
 let units=bytes_len/2;if path_buf[units-1]!=0{return Err(Error::InvalidImagePath)}
 // Return the borrowed raw value data from the hive, without any conversion.
 let raw_path=v.raw_data()?;
 Ok(ServiceConfig{start,service_type,image_path:raw_path,error_control})
}
