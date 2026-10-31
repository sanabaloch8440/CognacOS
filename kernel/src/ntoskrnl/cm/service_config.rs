//! Extended registry metadata for Windows-style driver service configuration.
//! Optional values are validated when present; malformed registry data is never
//! silently converted into a default load-order or dependency policy.
#![allow(dead_code)]
use super::driver_config::{self, Error, ServiceConfig};
use super::hive::{Error as HiveError, Hive, Key};

pub const SERVICE_BOOT_START:u32=0;
pub const SERVICE_SYSTEM_START:u32=1;
pub const SERVICE_AUTO_START:u32=2;
pub const SERVICE_DEMAND_START:u32=3;
pub const SERVICE_DISABLED:u32=4;
pub const SERVICE_KERNEL_DRIVER:u32=1;
pub const SERVICE_FILE_SYSTEM_DRIVER:u32=2;
pub const SERVICE_ADAPTER:u32=4;
pub const SERVICE_RECOGNIZER_DRIVER:u32=8;
pub const SERVICE_TYPE_MASK:u32=0x0f;

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum ExtendedConfigError {
    Base(Error), InvalidStart, InvalidType, InvalidErrorControl, InvalidTag,
    InvalidGroup, InvalidDependencies, BufferTooSmall, InvalidRegistryType,
}
impl From<Error> for ExtendedConfigError { fn from(e:Error)->Self{Self::Base(e)} }
impl From<HiveError> for ExtendedConfigError { fn from(e:HiveError)->Self{Self::Base(Error::Hive(e))} }

/// Registry-backed service metadata. Multi-string fields are returned as
/// validated UTF-16 MULTI_SZ payloads (including the final double NUL).
/// Fixed buffers keep this usable before the kernel heap is available.
#[derive(Clone,Copy,Debug)]
pub struct ServiceConfigDetails<'a> {
    pub base: ServiceConfig<'a>,
    pub tag: Option<u32>,
    pub group: [u16;128],
    pub group_len: usize,
    pub depend_on_service: [u16;512],
    pub depend_on_service_len: usize,
    pub depend_on_group: [u16;512],
    pub depend_on_group_len: usize,
    pub has_parameters: bool,
}
impl<'a> ServiceConfigDetails<'a> {
    pub fn service_dependency_iter(&self)->MultiSzIter<'_>{MultiSzIter::new(self.service_dependencies())}
    pub fn group_dependency_iter(&self)->MultiSzIter<'_>{MultiSzIter::new(self.group_dependencies())}
    pub fn group_name(&self)->&[u16]{&self.group[..self.group_len]}
    pub fn service_dependencies(&self)->&[u16]{&self.depend_on_service[..self.depend_on_service_len]}
    pub fn group_dependencies(&self)->&[u16]{&self.depend_on_group[..self.depend_on_group_len]}
}


/// Allocation-free iterator over a validated registry MULTI_SZ value.
#[derive(Clone,Copy)]
pub struct MultiSzIter<'a>{data:&'a[u16],offset:usize}
impl<'a> MultiSzIter<'a>{
    pub fn new(data:&'a[u16])->Self{Self{data,offset:0}}
}
impl<'a> Iterator for MultiSzIter<'a>{
    type Item=&'a[u16];
    fn next(&mut self)->Option<Self::Item>{
        if self.offset>=self.data.len()||self.data[self.offset]==0{self.offset=self.data.len();return None}
        let start=self.offset;
        let end=self.data[start..].iter().position(|c|*c==0)?+start;
        self.offset=end+1;
        Some(&self.data[start..end])
    }
}

fn ascii(s:&str,out:&mut[u16])->Result<usize,ExtendedConfigError>{
    let mut n=0;for c in s.encode_utf16(){if n==out.len(){return Err(ExtendedConfigError::BufferTooSmall)}out[n]=c;n+=1;}Ok(n)
}
fn optional_dword(key:Key<'_, '_>,name:&[u16])->Result<Option<u32>,ExtendedConfigError>{
    match key.find_value(name){
        Ok(v)=>{if v.kind!=4||v.data_len!=4{return Err(ExtendedConfigError::InvalidRegistryType)}
            let mut b=[0u8;4];v.data(&mut b)?;Ok(Some(u32::from_le_bytes(b)))},
        Err(HiveError::NotFound)=>Ok(None),Err(e)=>Err(e.into())
    }
}
fn optional_string(key:Key<'_, '_>,name:&[u16],out:&mut[u16])->Result<usize,ExtendedConfigError>{
    let v=match key.find_value(name){Ok(v)=>v,Err(HiveError::NotFound)=>return Ok(0),Err(e)=>return Err(e.into())};
    if v.kind!=1&&v.kind!=2{return Err(ExtendedConfigError::InvalidRegistryType)}
    let bytes=v.data_len as usize;
    if bytes<2||bytes%2!=0||bytes/2>out.len()||bytes>1024{return Err(ExtendedConfigError::BufferTooSmall)}
    let mut raw=[0u8;1024];v.data(&mut raw[..bytes])?;
    let n=bytes/2;for i in 0..n{out[i]=u16::from_le_bytes([raw[2*i],raw[2*i+1]])}
    if out[n-1]!=0||out[..n-1].contains(&0){return Err(ExtendedConfigError::InvalidGroup)}
    Ok(n-1)
}
fn optional_multi_sz(key:Key<'_, '_>,name:&[u16],out:&mut[u16])->Result<usize,ExtendedConfigError>{
    let v=match key.find_value(name){Ok(v)=>v,Err(HiveError::NotFound)=>return Ok(0),Err(e)=>return Err(e.into())};
    if v.kind!=7{return Err(ExtendedConfigError::InvalidRegistryType)}
    let bytes=v.data_len as usize;
    if bytes<4||bytes%2!=0||bytes/2>out.len()||bytes>2048{return Err(ExtendedConfigError::BufferTooSmall)}
    let mut raw=[0u8;2048];v.data(&mut raw[..bytes])?;
    let n=bytes/2;for i in 0..n{out[i]=u16::from_le_bytes([raw[2*i],raw[2*i+1]])}
    if out[n-1]!=0||out[n-2]!=0{return Err(ExtendedConfigError::InvalidDependencies)}
    let mut start=0;
    while start<n-1 {
        if out[start]==0 { if start!=n-2{return Err(ExtendedConfigError::InvalidDependencies)} break; }
        let Some(rel)=out[start..n].iter().position(|c|*c==0) else{return Err(ExtendedConfigError::InvalidDependencies)};
        if rel==0{return Err(ExtendedConfigError::InvalidDependencies)}
        let item=&out[start..start+rel];
        if item.iter().any(|c|*c==b'\\' as u16||*c==b'/' as u16){return Err(ExtendedConfigError::InvalidDependencies)}
        start+=rel+1;
    }
    Ok(n)
}
fn service_key<'h,'a>(hive:&'h Hive<'a>,service_name:&[u16])->Result<Key<'h,'a>,ExtendedConfigError>{
    let mut select_path=[0u16;16];let sn=ascii("Select",&mut select_path)?;
    let select=hive.find_path(&select_path[..sn])?;
    let current=optional_dword(select,&[67,117,114,114,101,110,116])?.ok_or(ExtendedConfigError::Base(Error::MissingStart))?;
    if current==0||current>999{return Err(ExtendedConfigError::Base(Error::InvalidImagePath))}
    let mut path=[0u16;256];let prefix=if current<10{format_control(current,&mut path)?}
      else if current<100{format_control(current,&mut path)?}else{format_control(current,&mut path)?};
    let mut n=prefix;
    for c in "\\Services\\".encode_utf16().chain(service_name.iter().copied()){
        if n==path.len(){return Err(ExtendedConfigError::BufferTooSmall)}path[n]=c;n+=1;
    }
    hive.find_path(&path[..n]).map_err(|e|if e==HiveError::NotFound{ExtendedConfigError::Base(Error::MissingService)}else{e.into()})
}
fn format_control(current:u32,out:&mut[u16])->Result<usize,ExtendedConfigError>{
    let s=if current<10{"ControlSet00"}else if current<100{"ControlSet0"}else{"ControlSet"};
    let mut n=ascii(s,out)?;
    if current<10{out[n]=b'0' as u16+current as u16;n+=1;}
    else if current<100{out[n]=b'0' as u16+(current/10) as u16;out[n+1]=b'0' as u16+(current%10) as u16;n+=2;}
    else{out[n]=b'0' as u16+(current/100) as u16;out[n+1]=b'0' as u16+((current/10)%10) as u16;out[n+2]=b'0' as u16+(current%10) as u16;n+=3;}
    Ok(n)
}

/// Read the active ControlSet service key, including Windows load-order and
/// dependency metadata. The base ImagePath parser remains the single authority
/// for path validation and is kept API-compatible with existing callers.
pub fn query_service_details<'h,'b>(
    hive:&'h Hive<'h>,service_name:&[u16],path_buf:&'b mut[u16]
)->Result<ServiceConfigDetails<'b>,ExtendedConfigError>{
    let base=driver_config::query_service(hive,service_name,path_buf)?;
    if base.start>SERVICE_DISABLED{return Err(ExtendedConfigError::InvalidStart)}
    if base.error_control>3{return Err(ExtendedConfigError::InvalidErrorControl)}
    let base_type=base.service_type&SERVICE_TYPE_MASK;
    if !matches!(base_type,SERVICE_KERNEL_DRIVER|SERVICE_FILE_SYSTEM_DRIVER|SERVICE_ADAPTER|SERVICE_RECOGNIZER_DRIVER)\n      ||base.service_type&!(SERVICE_TYPE_MASK|0x100|0x200|0x400|0x800)!=0{
        return Err(ExtendedConfigError::InvalidType)
    }
    let key=service_key(hive,service_name)?;
    let tag=optional_dword(key,&[84,97,103])?;
    let mut group=[0u16;128];let group_len=optional_string(key,&[71,114,111,117,112],&mut group)?;
    let mut depend_on_service=[0u16;512];
    let depend_on_service_len=optional_multi_sz(key,&[68,101,112,101,110,100,79,110,83,101,114,118,105,99,101],&mut depend_on_service)?;
    let mut depend_on_group=[0u16;512];
    let depend_on_group_len=optional_multi_sz(key,&[68,101,112,101,110,100,79,110,71,114,111,117,112],&mut depend_on_group)?;
    let has_parameters=match key.find_subkey(&[80,97,114,97,109,101,116,101,114,115]){
        Ok(_)=>true,Err(HiveError::NotFound)=>false,Err(e)=>return Err(e.into())
    };
    Ok(ServiceConfigDetails{base,tag,group,group_len,depend_on_service,depend_on_service_len,depend_on_group,depend_on_group_len,has_parameters})
}


/// Read an arbitrary driver-specific value from
/// Services\\<name>\\Parameters without interpreting or rewriting its type.
/// Returns (registry type, byte count); None means the key or value is absent.
pub fn query_service_parameter(
    hive:&Hive<'_>,service_name:&[u16],value_name:&[u16],out:&mut[u8]
)->Result<Option<(u32,usize)>,ExtendedConfigError>{
    let service=service_key(hive,service_name)?;
    let parameters=match service.find_subkey(&[80,97,114,97,109,101,116,101,114,115]){
        Ok(key)=>key,Err(HiveError::NotFound)=>return Ok(None),Err(e)=>return Err(e.into())
    };
    let value=match parameters.find_value(value_name){
        Ok(value)=>value,Err(HiveError::NotFound)=>return Ok(None),Err(e)=>return Err(e.into())
    };
    if value.data_len as usize>out.len(){return Err(ExtendedConfigError::BufferTooSmall)}
    let n=value.data(out)?;
    Ok(Some((value.kind,n)))
}


/// Read Control\\ServiceGroupOrder\\List from the active ControlSet.
/// The caller receives the original validated REG_MULTI_SZ UTF-16 sequence.
pub fn query_service_group_order(hive:&Hive<'_>,out:&mut[u16])->Result<usize,ExtendedConfigError>{
    let mut control=[0u16;64];let mut n=format_control(active_control_set(hive)?,&mut control)?;
    for c in "\\Control\\ServiceGroupOrder".encode_utf16(){
        if n==control.len(){return Err(ExtendedConfigError::BufferTooSmall)}control[n]=c;n+=1;
    }
    let key=hive.find_path(&control[..n])?;
    let value=key.find_value(&[76,105,115,116])?;
    if value.kind!=7{return Err(ExtendedConfigError::InvalidRegistryType)}
    let bytes=value.data_len as usize;
    if bytes<4||bytes%2!=0||bytes>out.len()*2{return Err(ExtendedConfigError::BufferTooSmall)}
    let mut raw=[0u8;2048];if bytes>raw.len(){return Err(ExtendedConfigError::BufferTooSmall)}
    value.data(&mut raw[..bytes])?;let units=bytes/2;
    for i in 0..units{out[i]=u16::from_le_bytes([raw[i*2],raw[i*2+1]])}
    if out[units-1]!=0||out[units-2]!=0{return Err(ExtendedConfigError::InvalidDependencies)}
    Ok(units)
}

/// Read a group's REG_BINARY tag-order array from Control\\GroupOrderList.
/// The first DWORD is the number of following DWORD tag IDs.
pub fn query_group_tag_order(hive:&Hive<'_>,group:&[u16],out:&mut[u32])->Result<usize,ExtendedConfigError>{
    if group.is_empty()||group.len()>128||group.iter().any(|c|*c==0||*c==b'\\' as u16){return Err(ExtendedConfigError::InvalidGroup)}
    let mut control=[0u16;64];let mut n=format_control(active_control_set(hive)?,&mut control)?;
    for c in "\\Control\\GroupOrderList".encode_utf16(){
        if n==control.len(){return Err(ExtendedConfigError::BufferTooSmall)}control[n]=c;n+=1;
    }
    let key=hive.find_path(&control[..n])?;
    let value=key.find_value(group)?;
    if value.kind!=3||value.data_len<4||value.data_len as usize>4+out.len()*4{return Err(ExtendedConfigError::InvalidRegistryType)}
    let mut raw=[0u8;1024];let bytes=value.data_len as usize;
    if bytes>raw.len(){return Err(ExtendedConfigError::BufferTooSmall)}
    value.data(&mut raw[..bytes])?;
    let count=u32::from_le_bytes([raw[0],raw[1],raw[2],raw[3]]) as usize;
    if count>out.len()||bytes!=4+count*4{return Err(ExtendedConfigError::InvalidTag)}
    for i in 0..count{out[i]=u32::from_le_bytes([raw[4+i*4],raw[5+i*4],raw[6+i*4],raw[7+i*4]])}
    Ok(count)
}
fn active_control_set(hive:&Hive<'_>)->Result<u32,ExtendedConfigError>{
    let mut select=[0u16;16];let n=ascii("Select",&mut select)?;
    let key=hive.find_path(&select[..n])?;
    optional_dword(key,&[67,117,114,114,101,110,116])?.ok_or(ExtendedConfigError::Base(Error::MissingStart))
}

#[cfg(test)]
mod multisz_tests{
    use super::*;
    #[test]
    fn iterates_registry_multisz_without_empty_tail(){
        let data=[65u16,0,66,67,0,0];
        let mut iter=MultiSzIter::new(&data);
        assert_eq!(iter.next(),Some(&data[0..1]));
        assert_eq!(iter.next(),Some(&data[2..4]));
        assert_eq!(iter.next(),None);
    }
    #[test]
    fn empty_multisz_has_no_items(){
        let data=[0u16,0];
        assert_eq!(MultiSzIter::new(&data).next(),None);
    }
}
