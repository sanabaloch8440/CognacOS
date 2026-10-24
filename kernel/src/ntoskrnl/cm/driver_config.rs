//! Registry-backed Windows driver service configuration.
//! Resolves Select\\Current and reads Services entries from the original SYSTEM hive.
#![allow(dead_code)]
use super::hive::{Error as HiveError,Hive,Key};
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Error{Hive(HiveError),MissingService,MissingImagePath,InvalidImagePath,MissingStart,MissingType,UnsupportedStart,BufferTooSmall}
impl From<HiveError> for Error{fn from(e:HiveError)->Self{Self::Hive(e)}}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct ServiceConfig<'a>{pub start:u32,pub service_type:u32,pub image_path:&'a[u16],pub error_control:u32}
fn dword(key:Key<'_>,name:&[u16])->Result<u32,Error>{let v=key.find_value(name)?;if v.kind!=4||v.data_len!=4{return Err(Error::InvalidImagePath)}let mut b=[0u8;4];v.data(&mut b)?;Ok(u32::from_le_bytes(b))}
fn utf16_ascii(s:&str,out:&mut[u16])->Result<usize,Error>{let mut n=0;for c in s.encode_utf16(){if n==out.len(){return Err(Error::BufferTooSmall)}out[n]=c;n+=1;}Ok(n)}
pub fn query_service<'h,'b>(hive:&'h Hive<'h>,service_name:&[u16],path_buf:&'b mut[u16])->Result<ServiceConfig<'b>,Error>{
 if service_name.is_empty()||service_name.len()>128||service_name.iter().any(|c|*c==0||*c==b'\\' as u16){return Err(Error::InvalidImagePath)}
 let mut select_path=[0u16;16];let sn=utf16_ascii("Select",&mut select_path)?;
 let select=hive.find_path(&select_path[..sn])?;
 let current=dword(select,&[67,117,114,114,101,110,116])?;
 if current==0||current>999{return Err(Error::InvalidImagePath)}
 let control_name=if current<10{let mut b=[0u16;16];let n=utf16_ascii("ControlSet00",&mut b)?;b[n]=b'0' as u16+current as u16;(b,n+1)}else if current<100{let mut b=[0u16;16];let n=utf16_ascii("ControlSet0",&mut b)?;b[n]=b'0' as u16+(current/10) as u16;b[n+1]=b'0' as u16+(current%10) as u16;(b,n+2)}else{let mut b=[0u16;16];let n=utf16_ascii("ControlSet",&mut b)?;b[n]=b'0' as u16+(current/100) as u16;b[n+1]=b'0' as u16+((current/10)%10) as u16;b[n+2]=b'0' as u16+(current%10) as u16;(b,n+3)};
 let mut path=[0u16;256];let mut n=0;
 for c in control_name.0[..control_name.1].iter().copied().chain("\\Services\\".encode_utf16()).chain(service_name.iter().copied()){if n==path.len(){return Err(Error::InvalidImagePath)}path[n]=c;n+=1;}
 let key=hive.find_path(&path[..n]).map_err(|e|if e==HiveError::NotFound{Error::MissingService}else{Error::Hive(e)})?;
 let start=dword(key,&[83,116,97,114,116]).map_err(|e|if e==Error::Hive(HiveError::NotFound){Error::MissingStart}else{e})?;
 let service_type=dword(key,&[84,121,112,101]).map_err(|e|if e==Error::Hive(HiveError::NotFound){Error::MissingType}else{e})?;
 let error_control=dword(key,&[69,114,114,111,114,67,111,110,116,114,111,108]).unwrap_or(0);
 let value=key.find_value(&[73,109,97,103,101,80,97,116,104]).map_err(|e|if e==HiveError::NotFound{Error::MissingImagePath}else{Error::Hive(e)})?;
 if value.kind!=1&&value.kind!=2{return Err(Error::InvalidImagePath)}
 let bytes_len=value.data_len as usize;if bytes_len<2||bytes_len%2!=0||bytes_len/2>path_buf.len(){return Err(Error::BufferTooSmall)}
 let mut raw=[0u8;1024];if bytes_len>raw.len(){return Err(Error::InvalidImagePath)}value.data(&mut raw[..bytes_len])?;
 let units=bytes_len/2;for i in 0..units{path_buf[i]=u16::from_le_bytes([raw[i*2],raw[i*2+1]]);}
 if path_buf[units-1]!=0||path_buf[..units-1].iter().any(|c|*c==0){return Err(Error::InvalidImagePath)}
 Ok(ServiceConfig{start,service_type,image_path:&path_buf[..units-1],error_control})
}

/// Read the raw BCD element payload from an object key. Object and element
/// identifiers are supplied by the BCD consumer; this routine does not hard-code
/// a particular loader or HAL path.
pub fn query_bcd_element_raw<'a>(hive:&'a Hive<'a>,object_id:&[u16],element_id:&[u16],out:&mut[u8])->Result<usize,Error>{
 if object_id.is_empty()||element_id.is_empty()||object_id.iter().chain(element_id).any(|c|*c==0||*c==b'\\' as u16){return Err(Error::InvalidImagePath)}
 let mut path=[0u16;512];let mut n=0usize;
 for c in "Objects\\".encode_utf16().chain(object_id.iter().copied()).chain("\\Elements\\".encode_utf16()).chain(element_id.iter().copied()){
  if n==path.len(){return Err(Error::InvalidImagePath)}path[n]=c;n+=1;
 }
 let key=hive.find_path(&path[..n])?;
 let value=key.find_value(&[69,108,101,109,101,110,116])?;
 if value.kind!=3{return Err(Error::InvalidImagePath)}
 value.data(out).map_err(Error::Hive)
}
