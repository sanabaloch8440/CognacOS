//! ACPI-1 standard device identification object evaluation.
//! This module deliberately evaluates only the ACPI-1 identity objects. It
//! does not enumerate children, publish PDOs, or interpret resource templates.
/*
 * FIXME(ACPI-PRODUCTION) — ACPI device identity and status
 * FIXME(ACPI-PRODUCTION): Complete typed evaluation and conversion for _HID/_CID/_UID/_STA/_ADR and related device methods, including ACPI defaults, malformed-result handling, and Windows-compatible ID ordering. Distinguish absent methods from execution failure and do not silently suppress errors without a testable policy.
 */

#![no_std]

use crate::aml::{AmlNamespace,AmlObjectType,decode_package_length};
use crate::aml_exec::{self,ExecError};
use crate::aml_objects::MAX_AML_BYTES;

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
/// Return the validated, length-bounded bytes of this normalized ACPI identifier.
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
    pub const EMPTY:Self=Self{hid:AcpiId::EMPTY,has_hid:false,compatible_ids:[AcpiId::EMPTY;MAX_COMPATIBLE_IDS],
        compatible_id_count:0,uid:None,uid_string:None,adr:None,sta:DEVICE_STATUS_DEFAULT};
}
/// Method dispatch boundary. The ACPI runtime owns execution gating, method
/// serialization, argument objects and interpreter state; this trait keeps
/// device enumeration independent of firmware execution mechanics.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum AcpiMethodValue {
    Integer(u64),
    String(AcpiId),
    Buffer{bytes:[u8;MAX_AML_BYTES],length:u16},
    CompatibleIds{ids:[AcpiId;MAX_COMPATIBLE_IDS],count:u8}
}
pub trait AcpiMethodInvoker {
    fn evaluate_value(&mut self,namespace:&AmlNamespace,aml:&[u8],method:u16)->Result<AcpiMethodValue,ExecError>;
}
pub struct IntegerMethodInvoker;
/// ACPI method invoker backed by the kernel's dispatcher-object mutex API.
/// The synchronizer is supplied by the WDM adapter; the parser remains
/// independent of kernel ABI and can still be exercised with host test doubles.
pub struct SynchronizedMethodInvoker<'a>{pub synchronizer:&'a dyn aml_exec::AmlMethodSynchronizer}

fn evaluate_method_value(
 namespace:&AmlNamespace,aml:&[u8],method:u16,
 sync:Option<&dyn aml_exec::AmlMethodSynchronizer>,
)->Result<AcpiMethodValue,ExecError>{
 let node=namespace.nodes.get(method as usize).filter(|_|(method as usize)<namespace.count).ok_or(ExecError::InvalidMethod)?;
 let start=node.body_start as usize;
 let end=start.checked_add(node.body_length as usize).filter(|end|*end<=aml.len()).ok_or(ExecError::InvalidMethod)?;
 let body=&aml[start..end];
 // Direct data returns avoid allocating interpreter objects, but serialized
 // methods still acquire their namespace method mutex around this fast path.
 let direct=||->Result<AcpiMethodValue,ExecError>{
  if body.first()==Some(&0xa4)&&body.get(1)==Some(&0x0d){
   let text=&body[2..];let n=text.iter().position(|b|*b==0).ok_or(ExecError::Truncated)?;
   if n+3!=body.len(){return Err(ExecError::InvalidMethod)}
   return Ok(AcpiMethodValue::String(AcpiId::from_bytes(&text[..n]).map_err(|_|ExecError::InvalidMethod)?));
  }
  if body.first()==Some(&0xa4)&&body.get(1)==Some(&0x12){
   let (ids,count)=decode_package_ids(&body[1..]).map_err(|_|ExecError::InvalidMethod)?;
   return Ok(AcpiMethodValue::CompatibleIds{ids,count});
  }
  if body.first()==Some(&0xa4)&&body.get(1)==Some(&0x11){
   let mut bytes=[0u8;MAX_AML_BYTES];
   let length=crate::aml_buffer::decode_returned_buffer(body,&mut bytes).map_err(|_|ExecError::InvalidMethod)?;
   return Ok(AcpiMethodValue::Buffer{bytes,length:length as u16});
  }
  match sync{
   Some(provider)=>Ok(AcpiMethodValue::Integer(aml_exec::execute_method_with_sync(aml,namespace,method,&[],64,provider)?.integer())),
   None=>Ok(AcpiMethodValue::Integer(aml_exec::execute_method(aml,namespace,method,&[])?.integer())),
  }
 };
 // The interpreter manages its own lock for scalar/control-flow methods.
 // Only direct-return fast paths need the external guard here.
 let is_direct=body.first()==Some(&0xa4)&&matches!(body.get(1),Some(0x0d|0x11|0x12));
 if is_direct {
  if let Some(provider)=sync {
   return aml_exec::with_method_synchronization(namespace,method,provider,direct);
  }
 }
 direct()
}
impl AcpiMethodInvoker for IntegerMethodInvoker{
 fn evaluate_value(&mut self,namespace:&AmlNamespace,aml:&[u8],method:u16)->Result<AcpiMethodValue,ExecError>{
  evaluate_method_value(namespace,aml,method,None)
 }
}
impl AcpiMethodInvoker for SynchronizedMethodInvoker<'_>{
 fn evaluate_value(&mut self,namespace:&AmlNamespace,aml:&[u8],method:u16)->Result<AcpiMethodValue,ExecError>{
  evaluate_method_value(namespace,aml,method,Some(self.synchronizer))
 }
}

enum DataValue<'a>{Integer(u64),String(&'a [u8]),Package(&'a [u8])}
#[derive(Clone,Copy)]
enum IdentityValue{Integer(u64),String(AcpiId),CompatibleIds{ids:[AcpiId;MAX_COMPATIBLE_IDS],count:u8}}
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
        0x12=>Ok(DataValue::Package(bytes)),
        0x13=>Err(IdentityError::UnsupportedObject),
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
    if a==0||b==0||c==0||a>26||b>26||c>26{return Err(IdentityError::InvalidEisaId)}
    let digits=compressed&0xffff;
    let hex=|n:u32|if n<10{b'0'+n as u8}else{b'A'+(n-10)as u8};
    let text=[b'A'+a-1,b'A'+b-1,b'A'+c-1,
        hex((digits>>12)&0xf),hex((digits>>8)&0xf),hex((digits>>4)&0xf),hex(digits&0xf)];
    AcpiId::from_bytes(&text)
}
fn id_from_value(value:IdentityValue)->Result<AcpiId,IdentityError>{
    match value{IdentityValue::String(s)=>Ok(s),IdentityValue::Integer(v)=>eisa_id(v),_=>Err(IdentityError::UnsupportedObject)}
}
fn decode_package_ids(bytes:&[u8])->Result<([AcpiId;MAX_COMPATIBLE_IDS],u8),IdentityError>{
    let (_,end)=decode_package_length(bytes,1,bytes.len()).map_err(|_|IdentityError::MalformedObject)?;
    let plen=1+((bytes[1]>>6)as usize);let mut pos=1+plen;
    let count=*bytes.get(pos).ok_or(IdentityError::MalformedObject)? as usize;pos+=1;
    if count>MAX_COMPATIBLE_IDS{return Err(IdentityError::TooManyCompatibleIds)}
    let mut ids=[AcpiId::EMPTY;MAX_COMPATIBLE_IDS];
    for index in 0..count{
        ids[index]=if bytes.get(pos)==Some(&0x0d){
            pos+=1;let tail=bytes.get(pos..end).ok_or(IdentityError::MalformedObject)?;
            let n=tail.iter().position(|b|*b==0).ok_or(IdentityError::MalformedObject)?;
            let id=AcpiId::from_bytes(&tail[..n])?;pos+=n+1;id
        }else{eisa_id(read_integer(bytes,&mut pos)?)?};
    }
    if pos!=end{return Err(IdentityError::MalformedObject)}
    Ok((ids,count as u8))
}
fn convert_data(value:DataValue<'_>)->Result<IdentityValue,IdentityError>{
    match value{
        DataValue::Integer(v)=>Ok(IdentityValue::Integer(v)),
        DataValue::String(s)=>Ok(IdentityValue::String(AcpiId::from_bytes(s)?)),
        DataValue::Package(bytes)=>{let(ids,count)=decode_package_ids(bytes)?;Ok(IdentityValue::CompatibleIds{ids,count})}
    }
}
fn find_object(ns:&AmlNamespace,device:u16,name:[u8;4])->Result<Option<u16>,IdentityError>{
    let node=ns.nodes.get(device as usize).filter(|_|(device as usize)<ns.count).ok_or(IdentityError::InvalidDevice)?;
    if !matches!(node.object_type,AmlObjectType::Device|AmlObjectType::Processor|AmlObjectType::PowerResource|AmlObjectType::ThermalZone){return Err(IdentityError::InvalidDevice)}
    let Some(found)=ns.find_child(device,name) else{return Ok(None)};
    Ok(Some(ns.resolve_alias(found).map_err(|_|IdentityError::MalformedObject)?))
}
fn evaluate_object(ns:&AmlNamespace,aml_tables:&[&[u8]],node:u16,invoker:&mut impl AcpiMethodInvoker)->Result<IdentityValue,IdentityError>{
    let object=ns.nodes.get(node as usize).filter(|_|(node as usize)<ns.count).ok_or(IdentityError::MalformedObject)?;
    let aml=aml_tables.get(object.source_table as usize).copied().ok_or(IdentityError::MalformedObject)?;
    match object.object_type{
        AmlObjectType::Name=>convert_data(data_value(aml,object)?),
        AmlObjectType::Method=>{match invoker.evaluate_value(ns,aml,node).map_err(IdentityError::Method)?{
            AcpiMethodValue::Integer(v)=>Ok(IdentityValue::Integer(v)),
            AcpiMethodValue::String(v)=>Ok(IdentityValue::String(v)),
            AcpiMethodValue::Buffer{..}=>Err(IdentityError::UnsupportedObject),
            AcpiMethodValue::CompatibleIds{ids,count}=>{if count as usize>MAX_COMPATIBLE_IDS{return Err(IdentityError::TooManyCompatibleIds)}Ok(IdentityValue::CompatibleIds{ids,count})}
        }},
        _=>Err(IdentityError::UnsupportedObject),
    }
}
fn eval_optional(ns:&AmlNamespace,aml_tables:&[&[u8]],device:u16,name:[u8;4],invoker:&mut impl AcpiMethodInvoker)->Result<Option<IdentityValue>,IdentityError>{
    match find_object(ns,device,name)?{Some(id)=>Ok(Some(evaluate_object(ns,aml_tables,id,invoker)?)),None=>Ok(None)}
}
/// Evaluate _HID, _CID, _UID, _STA and _ADR for one namespace device.
///
/// _HID identifies non-addressed devices; _ADR alone is sufficient for a
/// PCI-enumerated child. _STA defaults to 0x0f when absent; _UID defaults to
/// integer zero; _ADR remains absent when not defined. _CID accepts a single
/// ID or a Package of IDs. String IDs are returned verbatim; integer IDs are
/// decoded from ACPI EISAID encoding.
pub fn evaluate_device_identity(ns:&AmlNamespace,aml_tables:&[&[u8]],device:u16,invoker:&mut impl AcpiMethodInvoker)->Result<AcpiDeviceIdentity,IdentityError>{
    let mut out=AcpiDeviceIdentity::EMPTY;
    if let Some(hid)=eval_optional(ns,aml_tables,device,*b"_HID",invoker)?{
        out.hid=id_from_value(hid)?;out.has_hid=true;
    }
    if let Some(cid)=eval_optional(ns,aml_tables,device,*b"_CID",invoker)?{
        match cid{
            IdentityValue::CompatibleIds{ids,count}=>{out.compatible_ids=ids;out.compatible_id_count=count;},
            value=>{out.compatible_ids[0]=id_from_value(value)?;out.compatible_id_count=1;}
        }
    }
    if let Some(uid)=eval_optional(ns,aml_tables,device,*b"_UID",invoker)?{
        match uid{IdentityValue::Integer(v)=>out.uid=Some(v),IdentityValue::String(s)=>out.uid_string=Some(s),_=>return Err(IdentityError::UnsupportedObject)}
    }else{out.uid=Some(0);}
    if let Some(adr)=eval_optional(ns,aml_tables,device,*b"_ADR",invoker)?{
        match adr{IdentityValue::Integer(v)=>out.adr=Some(v),_=>return Err(IdentityError::UnsupportedObject)}
    }
    if let Some(sta)=eval_optional(ns,aml_tables,device,*b"_STA",invoker)?{
        match sta{IdentityValue::Integer(v) if v<=u32::MAX as u64=>out.sta=v as u32,_=>return Err(IdentityError::UnsupportedObject)}
    }
    // PCI-enumerated children commonly expose _ADR without _HID. A device
    // with neither identifier cannot be matched or addressed by its parent.
    if !out.has_hid&&out.adr.is_none(){return Err(IdentityError::MissingHid)}
    Ok(out)
}

#[cfg(test)]
mod tests{
    use super::*;
    use crate::aml::load_definition_block;
    fn one_table<'a>(aml:&'a [u8])->[&'a [u8];1]{[aml]}
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
        let result=evaluate_device_identity(&ns,&one_table(&aml),dev,&mut IntegerMethodInvoker).unwrap();
        assert_eq!(result.hid.as_bytes(),b"ACME0001");
        assert_eq!(result.compatible_ids[0].as_bytes(),b"ACME0002");
        assert_eq!(result.uid,Some(3));assert_eq!(result.adr,Some(0x1234));
        assert_eq!(result.sta,DEVICE_STATUS_DEFAULT);
    }
    #[test]fn decodes_integer_eisa_hid_and_cid_package(){
        // EISAID("PNP0A03") is encoded as the little-endian integer 0x030ad041.
        let aml=[0x5b,0x82,0x21,b'D',b'E',b'V',b'0',
            0x08,b'_',b'H',b'I',b'D',0x0c,0x41,0xd0,0x0a,0x03,
            0x08,b'_',b'C',b'I',b'D',0x12,0x0c,2,0x0c,0x41,0xd0,0x0a,0x03,
            0x0c,0x41,0xd0,0x0a,0x04];
        let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
        let dev=ns.find_child(0,*b"DEV0").unwrap();
        let result=evaluate_device_identity(&ns,&one_table(&aml),dev,&mut IntegerMethodInvoker).unwrap();
        assert_eq!(result.hid.as_bytes(),b"PNP0A03");
        assert_eq!(result.compatible_id_count,2);
        assert_eq!(result.compatible_ids[0].as_bytes(),b"PNP0A03");
        assert_eq!(result.compatible_ids[1].as_bytes(),b"PNP0A04");
    }
    struct FixedMethods;
    impl AcpiMethodInvoker for FixedMethods{
        fn evaluate_value(&mut self,ns:&AmlNamespace,_:&[u8],method:u16)->Result<AcpiMethodValue,ExecError>{
            let name=ns.nodes[method as usize].name;
            Ok(AcpiMethodValue::Integer(if name==*b"_STA"{DEVICE_PRESENT as u64|DEVICE_ENABLED as u64|DEVICE_FUNCTIONING as u64}
                else if name==*b"_ADR"{0x0002_0003}else{0}))
        }
    }
    #[test]fn evaluates_sta_and_adr_methods(){
        // Device(DEV0) with _HID string and integer-returning _STA/_ADR methods.
        let aml=[0x5b,0x82,0x2b,b'D',b'E',b'V',b'0',
            0x08,b'_',b'H',b'I',b'D',0x0d,b'A',b'C',b'M',b'E',b'0',b'0',b'0',b'1',0,
            0x14,0x09,b'_',b'S',b'T',b'A',0,0xa4,0x0a,0x0b,
            0x14,0x0c,b'_',b'A',b'D',b'R',0,0xa4,0x0c,3,0,2,0];
        let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
        let dev=ns.find_child(0,*b"DEV0").unwrap();
        let result=evaluate_device_identity(&ns,&one_table(&aml),dev,&mut FixedMethods).unwrap();
        assert_eq!(result.sta,0x0b);assert_eq!(result.adr,Some(0x0002_0003));
    }
    #[test]fn accepts_pci_address_identity_without_hid(){
        let aml=[0x5b,0x82,0x0f,b'D',b'E',b'V',b'0',
            0x08,b'_',b'A',b'D',b'R',0x0c,0x34,0x12,0,0];
        let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
        let dev=ns.find_child(0,*b"DEV0").unwrap();
        let result=evaluate_device_identity(&ns,&one_table(&aml),dev,&mut IntegerMethodInvoker).unwrap();
        assert!(!result.has_hid);assert_eq!(result.adr,Some(0x1234));
    }
    struct TypedMethods;
    impl AcpiMethodInvoker for TypedMethods{
        fn evaluate_value(&mut self,ns:&AmlNamespace,_:&[u8],method:u16)->Result<AcpiMethodValue,ExecError>{
            Ok(match ns.nodes[method as usize].name{
                n if n==*b"_HID"=>AcpiMethodValue::String(AcpiId::from_bytes(b"ACME0001").unwrap()),
                n if n==*b"_CID"=>{let mut ids=[AcpiId::EMPTY;MAX_COMPATIBLE_IDS];ids[0]=AcpiId::from_bytes(b"ACME0002").unwrap();AcpiMethodValue::CompatibleIds{ids,count:1}},
                n if n==*b"_UID"=>AcpiMethodValue::String(AcpiId::from_bytes(b"SERIAL-7").unwrap()),
                n if n==*b"_STA"=>AcpiMethodValue::Integer(DEVICE_STATUS_DEFAULT as u64),
                _=>AcpiMethodValue::Integer(0x10002),
            })
        }
    }
    #[test]fn evaluates_string_and_package_results_from_identity_methods(){
        let mut aml=std::vec::Vec::new();aml.extend_from_slice(&[0x5b,0x82,0,b'D',b'E',b'V',b'0']);
        for name in [*b"_HID",*b"_CID",*b"_UID",*b"_STA",*b"_ADR"]{
            aml.extend_from_slice(&[0x14,0x08]);aml.extend_from_slice(&name);aml.extend_from_slice(&[0,0xa4,0]);
        }
        aml[2]=(aml.len()-2) as u8;
        let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
        let dev=ns.find_child(0,*b"DEV0").unwrap();
        let result=evaluate_device_identity(&ns,&one_table(&aml),dev,&mut TypedMethods).unwrap();
        assert_eq!(result.hid.as_bytes(),b"ACME0001");
        assert_eq!(result.compatible_ids[0].as_bytes(),b"ACME0002");
        assert_eq!(result.uid_string.unwrap().as_bytes(),b"SERIAL-7");
        assert_eq!(result.sta,DEVICE_STATUS_DEFAULT);
        assert_eq!(result.adr,Some(0x10002));
    }
    fn add_method(aml:&mut std::vec::Vec<u8>,name:[u8;4],body:&[u8]){
        aml.push(0x14);aml.push((1+4+1+body.len()) as u8);aml.extend_from_slice(&name);aml.push(0);aml.extend_from_slice(body);
    }
    #[test]fn integer_invoker_evaluates_direct_string_and_package_methods(){
        let mut aml=std::vec::Vec::new();aml.extend_from_slice(&[0x5b,0x82,0,b'D',b'E',b'V',b'0']);
        add_method(&mut aml,*b"_HID",&[0xa4,0x0d,b'A',b'C',b'M',b'E',b'0',b'0',b'0',b'1',0]);
        add_method(&mut aml,*b"_CID",&[0xa4,0x12,0x16,2,
            0x0d,b'A',b'C',b'M',b'E',b'0',b'0',b'0',b'2',0,
            0x0d,b'A',b'C',b'M',b'E',b'0',b'0',b'0',b'3',0]);
        add_method(&mut aml,*b"_UID",&[0xa4,0x0d,b'S',b'E',b'R',b'I',b'A',b'L',b'-',b'7',0]);
        add_method(&mut aml,*b"_STA",&[0xa4,0x0a,0x0f]);
        add_method(&mut aml,*b"_ADR",&[0xa4,0x0c,0x34,0x12,0,0]);
        let package_length=aml.len()-2+1; // Include the second PkgLength byte when extended encoding is needed.
        if package_length<0x40 {
            aml[2]=package_length as u8;
        } else {
            aml[2]=0x40|((package_length&0x0f) as u8);
            aml.insert(3,(package_length>>4) as u8);
        }
        let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
        let dev=ns.find_child(0,*b"DEV0").unwrap();
        let result=evaluate_device_identity(&ns,&one_table(&aml),dev,&mut IntegerMethodInvoker).unwrap();
        assert_eq!(result.hid.as_bytes(),b"ACME0001");
        assert_eq!(result.compatible_id_count,2);
        assert_eq!(result.compatible_ids[0].as_bytes(),b"ACME0002");
        assert_eq!(result.compatible_ids[1].as_bytes(),b"ACME0003");
        assert_eq!(result.uid_string.unwrap().as_bytes(),b"SERIAL-7");
        assert_eq!(result.sta,DEVICE_STATUS_DEFAULT);
        assert_eq!(result.adr,Some(0x1234));
    }
    #[test]fn rejects_device_without_hid(){
        let aml=[0x5b,0x82,0x05,b'D',b'E',b'V',b'0'];
        let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
        assert_eq!(evaluate_device_identity(&ns,&one_table(&aml),1,&mut IntegerMethodInvoker),Err(IdentityError::MissingHid));
    }
    struct SyncRecorder{
        acquired:core::sync::atomic::AtomicUsize,
        released:core::sync::atomic::AtomicUsize,
        fail:bool,
    }
    impl SyncRecorder{fn new()->Self{Self{acquired:core::sync::atomic::AtomicUsize::new(0),released:core::sync::atomic::AtomicUsize::new(0),fail:false}}}
    impl aml_exec::AmlMethodSynchronizer for SyncRecorder{
        fn acquire(&self,_method:u16,_level:u8)->Result<(),ExecError>{
            if self.fail{return Err(ExecError::ExecutionBusy)}
            self.acquired.fetch_add(1,core::sync::atomic::Ordering::SeqCst);Ok(())
        }
        fn release(&self,_method:u16){self.released.fetch_add(1,core::sync::atomic::Ordering::SeqCst);}
    }
    #[test]fn synchronized_invoker_locks_serialized_direct_string_results(){
        let mut aml=std::vec::Vec::new();
        aml.extend_from_slice(&[0x5b,0x82,0,b'D',b'E',b'V',b'0']);
        add_method(&mut aml,*b"_HID",&[0xa4,0x0d,b'A',b'C',b'M',b'E',b'0',b'0',b'0',b'1',0]);
        let flags_offset=aml.len()-&[0xa4,0x0d,b'A',b'C',b'M',b'E',b'0',b'0',b'0',b'1',0].len()-1;
        aml[flags_offset]=0x08; // Serialized method, SyncLevel 0.
        aml[2]=(aml.len()-2) as u8;
        let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
        let method=ns.find_child(ns.find_child(0,*b"DEV0").unwrap(),*b"_HID").unwrap();
        assert_ne!(ns.nodes[method as usize].method_flags&0x08,0);
        let sync=SyncRecorder::new();
        let mut invoker=SynchronizedMethodInvoker{synchronizer:&sync};
        assert_eq!(invoker.evaluate_value(&ns,&aml,method).unwrap(),AcpiMethodValue::String(AcpiId::from_bytes(b"ACME0001").unwrap()));
        assert_eq!(sync.acquired.load(core::sync::atomic::Ordering::SeqCst),1);
        assert_eq!(sync.released.load(core::sync::atomic::Ordering::SeqCst),1);
    }
    #[test]fn synchronized_invoker_fails_closed_when_serialized_method_lock_fails(){
        let mut aml=std::vec::Vec::new();
        aml.extend_from_slice(&[0x5b,0x82,0,b'D',b'E',b'V',b'0']);
        add_method(&mut aml,*b"_HID",&[0xa4,0x0d,b'A',b'C',b'M',b'E',b'0',b'0',b'0',b'1',0]);
        aml[2]=(aml.len()-2) as u8;
        let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
        let method=ns.find_child(ns.find_child(0,*b"DEV0").unwrap(),*b"_HID").unwrap();
        let sync=SyncRecorder{fail:true,..SyncRecorder::new()};
        let mut invoker=SynchronizedMethodInvoker{synchronizer:&sync};
        assert_eq!(invoker.evaluate_value(&ns,&aml,method),Err(ExecError::ExecutionBusy));
        assert_eq!(sync.acquired.load(core::sync::atomic::Ordering::SeqCst),0);
        assert_eq!(sync.released.load(core::sync::atomic::Ordering::SeqCst),0);
    }

}
