//! Registry-backed Windows driver service configuration.
//! Resolves Select\\Current and reads Services entries from the original SYSTEM hive.
#![allow(dead_code)]
use super::hive::{Error as HiveError,Hive,Key};
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Error{Hive(HiveError),MissingService,MissingImagePath,InvalidImagePath,MissingStart,MissingType,MissingControlSet,InvalidControlSet,UnsupportedStart,BufferTooSmall}
impl From<HiveError> for Error{fn from(e:HiveError)->Self{Self::Hive(e)}}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct ServiceConfig<'a>{pub start:u32,pub service_type:u32,pub image_path:&'a[u16],pub error_control:u32}

/// The four control-set selectors stored under SYSTEM\\Select. `Current` is
/// the control set selected for this boot; `Default` is the preferred next-boot
/// set; `LastKnownGood` is the recovery set; and zero `Failed` means no failed
/// control set is recorded. Optional selectors remain optional for old/minimal
/// hives, but every nonzero selector must name an existing ControlSetNNN key.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct ControlSetSelection{pub current:u32,pub default:Option<u32>,pub last_known_good:Option<u32>,pub failed:Option<u32>}

fn optional_dword(key:Key<'_, '_>,name:&[u16])->Result<Option<u32>,Error>{
    match key.find_value(name){
        Ok(value)=>{if value.kind!=4||value.data_len!=4{return Err(Error::InvalidControlSet)}let mut raw=[0u8;4];value.data(&mut raw)?;Ok(Some(u32::from_le_bytes(raw)))},
        Err(HiveError::NotFound)=>Ok(None),Err(error)=>Err(error.into()),
    }
}
fn control_set_path(number:u32,out:&mut[u16])->Result<usize,Error>{
    if number==0||number>999{return Err(Error::InvalidControlSet)}
    let (prefix,digits)=if number<10{("ControlSet00",1)}else if number<100{("ControlSet0",2)}else{("ControlSet",3)};
    let mut n=utf16_ascii(prefix,out)?;
    if n+digits>out.len(){return Err(Error::BufferTooSmall)}
    match digits {
        1=>{out[n]=b'0' as u16+number as u16;n+=1;},
        2=>{out[n]=b'0' as u16+(number/10) as u16;out[n+1]=b'0' as u16+(number%10) as u16;n+=2;},
        _=>{out[n]=b'0' as u16+(number/100) as u16;out[n+1]=b'0' as u16+((number/10)%10) as u16;out[n+2]=b'0' as u16+(number%10) as u16;n+=3;},
    }
    Ok(n)
}

/// Parse and validate SYSTEM\\Select. A selector is not accepted merely
/// because its DWORD is in range: its referenced ControlSetNNN key must exist.
pub fn query_control_set_selection(hive:&Hive<'_>)->Result<ControlSetSelection,Error>{
    let mut select_path=[0u16;16];let sn=utf16_ascii("Select",&mut select_path)?;
    let select=hive.find_path(&select_path[..sn])?;
    let current=optional_dword(select,&[67,117,114,114,101,110,116])?.ok_or(Error::MissingControlSet)?;
    let default=optional_dword(select,&[68,101,102,97,117,108,116])?;
    let last_known_good=optional_dword(select,&[76,97,115,116,75,110,111,119,110,71,111,111,100])?;
    let failed=optional_dword(select,&[70,97,105,108,101,100])?;
    for value in [Some(current),default,last_known_good,failed].into_iter().flatten(){
        if value==0 {continue;}
        let mut path=[0u16;16];let n=control_set_path(value,&mut path)?;
        hive.find_path(&path[..n]).map_err(|error|if error==HiveError::NotFound{Error::InvalidControlSet}else{Error::Hive(error)})?;
    }
    if current==0{return Err(Error::InvalidControlSet)}
    Ok(ControlSetSelection{current,default,last_known_good,failed})
}

/// Resolve the control set selected for the current boot, never `Default`.
pub fn active_control_set(hive:&Hive<'_>)->Result<u32,Error>{Ok(query_control_set_selection(hive)?.current)}
fn dword(key:Key<'_, '_>,name:&[u16])->Result<u32,Error>{let v=key.find_value(name)?;if v.kind!=4||v.data_len!=4{return Err(Error::InvalidImagePath)}let mut b=[0u8;4];v.data(&mut b)?;Ok(u32::from_le_bytes(b))}
fn utf16_ascii(s:&str,out:&mut[u16])->Result<usize,Error>{let mut n=0;for c in s.encode_utf16(){if n==out.len(){return Err(Error::BufferTooSmall)}out[n]=c;n+=1;}Ok(n)}
pub fn query_service<'h,'b>(hive:&'h Hive<'h>,service_name:&[u16],path_buf:&'b mut[u16])->Result<ServiceConfig<'b>,Error>{
 if service_name.is_empty()||service_name.len()>128||service_name.iter().any(|c|*c==0||*c==b'\\' as u16){return Err(Error::InvalidImagePath)}
 let current=active_control_set(hive)?;
 let mut control_name=[0u16;16];let control_len=control_set_path(current,&mut control_name)?;
 let mut path=[0u16;256];let mut n=0;
 for c in control_name[..control_len].iter().copied().chain("\\Services\\".encode_utf16()).chain(service_name.iter().copied()){if n==path.len(){return Err(Error::InvalidImagePath)}path[n]=c;n+=1;}
 let key=hive.find_path(&path[..n]).map_err(|e|if e==HiveError::NotFound{Error::MissingService}else{Error::Hive(e)})?;
 let start=dword(key,&[83,116,97,114,116]).map_err(|e|if e==Error::Hive(HiveError::NotFound){Error::MissingStart}else{e})?;
 let service_type=dword(key,&[84,121,112,101]).map_err(|e|if e==Error::Hive(HiveError::NotFound){Error::MissingType}else{e})?;
 let error_control=match key.find_value(&[69,114,114,111,114,67,111,110,116,114,111,108]){
  Ok(_)=>dword(key,&[69,114,114,111,114,67,111,110,116,114,111,108])?,
  Err(HiveError::NotFound)=>0,Err(e)=>return Err(Error::Hive(e))
 };
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
