//! ACPI-1 standard device identification object evaluation.
//! This module deliberately evaluates only the ACPI-1 identity objects. It
//! does not enumerate children, publish PDOs, or interpret resource templates.
#![no_std]

use crate::aml::{AmlNamespace,AmlObjectType,decode_package_length};
use crate::aml_exec::{self,ExecError};

pub const DEVICE_PRESENT:u32=1;
pub const DEVICE_ENABLED:u32=2;
pub const DEVICE_UI:u32=4;
pub const DEVICE_FUNCTIONING:u32=8;
pub const DEVICE_BATTERY:u32=16;
pub const DEVICE_STATUS_DEFAULT:u32=DEVICE_PRESENT|DEVICE_ENABLED|DEVICE_UI|DEVICE_FUNCTIONING;
pub const MAX_COMPATIBLE_IDS:usize=16;
pub const MAX_ID_STRING:usize=64;

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum IdentityError { InvalidDevice,MissingHid,MalformedObject,UnsupportedObject,InvalidEisaId,TooManyCompatibleIds,Method(ExecError),StringTooLong }

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct AcpiId { pub bytes:[u8;MAX_ID_STRING], pub length:u8 }
impl AcpiId {
    const EMPTY:Self=Self{bytes:[0;MAX_ID_STRING],length:0};
    fn from_bytes(value:&[u8])->Result<Self,IdentityError>{
        if value.is_empty()||value.len()>MAX_ID_STRING{return Err(IdentityError::StringTooLong)}
        let mut out=Self::EMPTY;out.bytes[..value.len()].copy_from_slice(value);out.length=value.len() as u8;Ok(out)
    }
    pub fn as_bytes(&self)->&[u8]{&self.bytes[..self.length as usize]}
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct AcpiDeviceIdentity {
    pub hid:AcpiId,
    pub has_hid:bool,
    pub compatible_ids:[AcpiId;MAX_COMPATIBLE_IDS],
    pub compatible_id_count:u8,
    pub uid:Option<u64>,
    pub uid_string:Option<AcpiId>,
    pub adr:Option<u64>,
    pub sta:u32,
}
impl AcpiDeviceIdentity {
    const EMPTY:Self=Self{hid:AcpiId::EMPTY,has_hid:false,compatible_ids:[AcpiId::EMPTY;MAX_COMPATIBLE_IDS],
        compatible_id_count:0,uid:None,uid_string:None,adr:None,sta:DEVICE_STATUS_DEFAULT};
}
/// Method dispatch boundary. The ACPI runtime owns execution gating, method
/// serialization, argument objects and interpreter state; this trait keeps
/// device enumeration independent of firmware execution mechanics.
pub trait AcpiMethodInvoker {
    fn evaluate_integer(&mut self,namespace:&AmlNamespace,aml:&[u8],method:u16)->Result<u64,ExecError>;
}
pub struct IntegerMethodInvoker;
impl AcpiMethodInvoker for IntegerMethodInvoker {
    fn evaluate_integer(&mut self,namespace:&AmlNamespace,aml:&[u8],method:u16)->Result<u64,ExecError>{
        Ok(aml_exec::execute_method(aml,namespace,method,&[])?.integer())
    }
}

enum DataValue<'a>{Integer(u64),String(&'a [u8]),Package(&'a [u8])}
fn read_integer(bytes:&[u8],pos:&mut usize)->Result<u64,IdentityError>{
    let op=*bytes.get(*pos).ok_or(IdentityError::MalformedObject)?;*pos+=1;
    let width=match op{0x00=>return Ok(0),0x01=>return Ok(1),0xff=>return Ok(u64::MAX),0x0a=>1,0x0b=>2,0x0c=>4,0x0e=>8,_=>return Err(IdentityError::UnsupportedObject)};
    let end=(*pos).checked_add(width).ok_or(IdentityError::MalformedObject)?;
    let data=bytes.get(*pos..end).ok_or(IdentityError::MalformedObject)?;
    let mut out=0u64;for (i,b) in data.iter().enumerate(){out|=(*b as u64)<<(i*8);}*pos=end;Ok(out)
}
fn data_value<'a>(aml:&'a [u8],node:&crate::aml::AmlNode)->Result<DataValue<'a>,IdentityError>{
    let start=node.body_start as usize;let end=start.checked_add(node.body_length as usize).ok_or(IdentityError::MalformedObject)?;
    let bytes=aml.get(start..end).ok_or(IdentityError::MalformedObject)?;
    let op=*bytes.first().ok_or(IdentityError::MalformedObject)?;
    match op{
        0x0d=>{let tail=&bytes[1..];let n=tail.iter().position(|b|*b==0).ok_or(IdentityError::MalformedObject)?;if n+2!=bytes.len(){return Err(IdentityError::MalformedObject)}Ok(DataValue::String(&tail[..n]))}
        0x12|0x13=>Ok(DataValue::Package(bytes)),
        _=>{let mut pos=0;let value=read_integer(bytes,&mut pos)?;if pos!=bytes.len(){return Err(IdentityError::MalformedObject)}Ok(DataValue::Integer(value))}
    }
}
fn eisa_id(value:u64)->Result<AcpiId,IdentityError>{
    if value>u32::MAX as u64{return Err(IdentityError::InvalidEisaId)}
    let v=value as u32;
    let compressed=v.swap_bytes();
    let a=((compressed>>26)&0x1f)as u8;
    let b=((compressed>>21)&0x1f)as u8;
    let c=((compressed>>16)&0x1f)as u8;
    if a==0||b==0||c==0{return Err(IdentityError::InvalidEisaId)}
    let digits=compressed&0xffff;
    let hex=|n:u32|if n<10{b'0'+n as u8}else{b'A'+(n-10)as u8};
    let text=[b'A'+a-1,b'A'+b-1,b'A'+c-1,
        hex((digits>>12)&0xf),hex((digits>>8)&0xf),hex((digits>>4)&0xf),hex(digits&0xf)];
    AcpiId::from_bytes(&text)
}
fn id_from_value(value:DataValue<'_>)->Result<AcpiId,IdentityError>{
    match value{DataValue::String(s)=>AcpiId::from_bytes(s),DataValue::Integer(v)=>eisa_id(v),_=>Err(IdentityError::UnsupportedObject)}
}
fn find_object(ns:&AmlNamespace,device:u16,name:[u8;4])->Result<Option<u16>,IdentityError>{
    let node=ns.nodes.get(device as usize).filter(|_|device as usize<ns.count).ok_or(IdentityError::InvalidDevice)?;
    if !matches!(node.object_type,AmlObjectType::Device|AmlObjectType::Processor|AmlObjectType::PowerResource|AmlObjectType::ThermalZone){return Err(IdentityError::InvalidDevice)}
    let Some(found)=ns.find_child(device,name) else{return Ok(None)};
    Ok(Some(ns.resolve_alias(found).map_err(|_|IdentityError::MalformedObject)?))
}
fn evaluate_object<'a>(ns:&AmlNamespace,aml:&'a [u8],node:u16,invoker:&mut impl AcpiMethodInvoker)->Result<DataValue<'a>,IdentityError>{
    let object=ns.nodes.get(node as usize).filter(|_|node as usize<ns.count).ok_or(IdentityError::MalformedObject)?;
    match object.object_type{
        AmlObjectType::Name=>data_value(aml,object),
        AmlObjectType::Method=>{let value=invoker.evaluate_integer(ns,aml,node).map_err(IdentityError::Method)?;Ok(DataValue::Integer(value))},
        _=>Err(IdentityError::UnsupportedObject),
    }
}
fn eval_optional<'a>(ns:&AmlNamespace,aml:&'a [u8],device:u16,name:[u8;4],invoker:&mut impl AcpiMethodInvoker)->Result<Option<DataValue<'a>>,IdentityError>{
    match find_object(ns,device,name)?{Some(id)=>Ok(Some(evaluate_object(ns,aml,id,invoker)?)),None=>Ok(None)}
}
fn package_ids(bytes:&[u8],out:&mut AcpiDeviceIdentity)->Result<(),IdentityError>{
    let (_,end)=decode_package_length(bytes,1,bytes.len()).map_err(|_|IdentityError::MalformedObject)?;
    let plen=1+((bytes[1]>>6)as usize);let mut pos=1+plen;
    let count=*bytes.get(pos).ok_or(IdentityError::MalformedObject)? as usize;pos+=1;
    if count>MAX_COMPATIBLE_IDS{return Err(IdentityError::TooManyCompatibleIds)}
    for _ in 0..count{
        let value=if bytes.get(pos)==Some(&0x0d){
            pos+=1;let tail=bytes.get(pos..end).ok_or(IdentityError::MalformedObject)?;
            let n=tail.iter().position(|b|*b==0).ok_or(IdentityError::MalformedObject)?;
            let id=AcpiId::from_bytes(&tail[..n])?;pos+=n+1;id
        }else{eisa_id(read_integer(bytes,&mut pos)?)?};
        out.compatible_ids[out.compatible_id_count as usize]=value;out.compatible_id_count+=1;
    }
    if pos!=end{return Err(IdentityError::MalformedObject)}Ok(())
}
/// Evaluate _HID, _CID, _UID, _STA and _ADR for one namespace device.
///
/// _HID is mandatory. _STA defaults to 0x0f when absent; _UID defaults to
/// integer zero; _ADR remains absent when not defined. _CID accepts a single
/// ID or a Package of IDs. String IDs are returned verbatim; integer IDs are
/// decoded from ACPI EISAID encoding.
pub fn evaluate_device_identity(ns:&AmlNamespace,aml:&[u8],device:u16,invoker:&mut impl AcpiMethodInvoker)->Result<AcpiDeviceIdentity,IdentityError>{
    let mut out=AcpiDeviceIdentity::EMPTY;
    let hid=eval_optional(ns,aml,device,*b"_HID",invoker)?.ok_or(IdentityError::MissingHid)?;
    out.hid=id_from_value(hid?)?;out.has_hid=true;
    if let Some(cid)=eval_optional(ns,aml,device,*b"_CID",invoker)?{
        match cid?{
            DataValue::Package(bytes)=>package_ids(bytes,&mut out)?,
            value=>{out.compatible_ids[0]=id_from_value(value)?;out.compatible_id_count=1;}
        }
    }
    if let Some(uid)=eval_optional(ns,aml,device,*b"_UID",invoker)?{
        match uid?{DataValue::Integer(v)=>out.uid=Some(v),DataValue::String(s)=>out.uid_string=Some(AcpiId::from_bytes(s)?),_=>return Err(IdentityError::UnsupportedObject)}
    }else{out.uid=Some(0);}
    if let Some(adr)=eval_optional(ns,aml,device,*b"_ADR",invoker)?{
        match adr?{DataValue::Integer(v)=>out.adr=Some(v),_=>return Err(IdentityError::UnsupportedObject)}
    }
    if let Some(sta)=eval_optional(ns,aml,device,*b"_STA",invoker)?{
        match sta?{DataValue::Integer(v)=>out.sta=v as u32,_=>return Err(IdentityError::UnsupportedObject)}
    }
    Ok(out)
}

#[cfg(test)]
mod tests{
    use super::*;
    use crate::aml::load_definition_block;
    fn add_string(aml:&mut std::vec::Vec<u8>,name:[u8;4],value:&[u8]){
        aml.push(0x08);aml.extend_from_slice(&name);aml.push(0x0d);aml.extend_from_slice(value);aml.push(0);
    }
    #[test]fn evaluates_standard_device_identity_and_optional_defaults(){
        let mut aml=std::vec::Vec::new();
        aml.extend_from_slice(&[0x5b,0x82,0,b'D',b'E',b'V',b'0']);
        add_string(&mut aml,*b"_HID",b"ACME0001");
        add_string(&mut aml,*b"_CID",b"ACME0002");
        aml.extend_from_slice(&[0x08,b'_',b'U',b'I',b'D',0x0a,3]);
        aml.extend_from_slice(&[0x08,b'_',b'A',b'D',b'R',0x0c,0x34,0x12,0,0]);
        let package_length=(aml.len()-2) as u8;
        aml[2]=package_length;
        let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
        let dev=ns.find_child(0,*b"DEV0").unwrap();
        let result=evaluate_device_identity(&ns,&aml,dev,&mut IntegerMethodInvoker).unwrap();
        assert_eq!(result.hid.as_bytes(),b"ACME0001");
        assert_eq!(result.compatible_ids[0].as_bytes(),b"ACME0002");
        assert_eq!(result.uid,Some(3));assert_eq!(result.adr,Some(0x1234));
        assert_eq!(result.sta,DEVICE_STATUS_DEFAULT);
    }
    #[test]fn decodes_integer_eisa_hid_and_cid_package(){
        // EISAID("PNP0A03") is encoded as the little-endian integer 0x030ad041.
        let aml=[0x5b,0x82,0x1b,b'D',b'E',b'V',b'0',
            0x08,b'_',b'H',b'I',b'D',0x0c,0x41,0xd0,0x0a,0x03,
            0x08,b'_',b'C',b'I',b'D',0x12,0x08,2,0x0c,0x41,0xd0,0x0a,0x03,
            0x0c,0x42,0xd0,0x0a,0x03];
        let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
        let dev=ns.find_child(0,*b"DEV0").unwrap();
        let result=evaluate_device_identity(&ns,&aml,dev,&mut IntegerMethodInvoker).unwrap();
        assert_eq!(result.hid.as_bytes(),b"PNP0A03");
        assert_eq!(result.compatible_id_count,2);
        assert_eq!(result.compatible_ids[0].as_bytes(),b"PNP0A03");
        assert_eq!(result.compatible_ids[1].as_bytes(),b"PNP0A04");
    }
    struct FixedMethods;
    impl AcpiMethodInvoker for FixedMethods{
        fn evaluate_integer(&mut self,ns:&AmlNamespace,_:&[u8],method:u16)->Result<u64,ExecError>{
            let name=ns.nodes[method as usize].name;
            Ok(if name==*b"_STA"{DEVICE_PRESENT as u64|DEVICE_ENABLED as u64|DEVICE_FUNCTIONING as u64}
                else if name==*b"_ADR"{0x0002_0003}else{0})
        }
    }
    #[test]fn evaluates_sta_and_adr_methods(){
        // Device(DEV0) with _HID string and integer-returning _STA/_ADR methods.
        let aml=[0x5b,0x82,0x1e,b'D',b'E',b'V',b'0',
            0x08,b'_',b'H',b'I',b'D',0x0d,b'A',b'C',b'M',b'E',b'0',b'0',b'0',b'1',0,
            0x14,0x0a,b'_',b'S',b'T',b'A',0,0xa4,0x0a,0x0b,
            0x14,0x0c,b'_',b'A',b'D',b'R',0,0xa4,0x0c,3,0,2,0];
        let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
        let dev=ns.find_child(0,*b"DEV0").unwrap();
        let result=evaluate_device_identity(&ns,&aml,dev,&mut FixedMethods).unwrap();
        assert_eq!(result.sta,0x0b);assert_eq!(result.adr,Some(0x0002_0003));
    }
    #[test]fn rejects_device_without_hid(){
        let aml=[0x5b,0x82,0x05,b'D',b'E',b'V',b'0'];
        let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
        assert_eq!(evaluate_device_identity(&ns,&aml,1,&mut IntegerMethodInvoker),Err(IdentityError::MissingHid));
    }
}
