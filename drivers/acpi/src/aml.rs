//! Bounded AML definition-block parser and namespace loader.
//! This module deliberately separates AML parsing from AML execution. Method
//! bodies are retained as validated byte ranges; they are never executed here.
#![no_std]

pub const MAX_NAMESPACE_NODES:usize=2048;
pub const MAX_NAME_SEGMENTS:usize=8;

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum AmlError { Truncated, BadPackageLength, BadName, UnsupportedOpcode, NamespaceFull, DuplicateName, InvalidParent, BadDefinitionBlock }

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct AmlName { pub segments:[[u8;4];MAX_NAME_SEGMENTS], pub count:u8, pub rooted:bool, pub parents:u8 }
impl AmlName { pub const EMPTY:Self=Self{segments:[[0;4];MAX_NAME_SEGMENTS],count:0,rooted:false,parents:0}; }

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
#[repr(u8)]
pub enum AmlObjectType { Root=0, Scope=1, Device=2, Method=3, Name=4, Processor=5, PowerResource=6, ThermalZone=7, Alias=8, External=9, OperationRegion=10, Field=11, IndexField=12, BankField=13, Mutex=14, Event=15, DataRegion=16 }
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct AmlNode {
    pub parent:u16, pub name:[u8;4], pub object_type:AmlObjectType,
    pub method_flags:u8, pub body_start:u32, pub body_length:u32,
}
impl AmlNode {
    pub const ROOT:Self=Self{parent:0,name:*b"____",object_type:AmlObjectType::Root,method_flags:0,body_start:0,body_length:0};
}
pub struct AmlNamespace { pub nodes:[AmlNode;MAX_NAMESPACE_NODES], pub count:usize }
impl AmlNamespace {
    pub const fn new()->Self { Self{nodes:[AmlNode::ROOT;MAX_NAMESPACE_NODES],count:1} }
    pub fn find_child(&self,parent:u16,name:[u8;4])->Option<u16>{
        self.nodes.iter().take(self.count).enumerate().skip(1)
            .find(|(_,n)|n.parent==parent&&n.name==name).map(|(i,_)|i as u16)
    }
    fn insert(&mut self,parent:u16,name:[u8;4],kind:AmlObjectType,flags:u8,start:usize,len:usize)->Result<u16,AmlError>{
        if parent as usize>=self.count{return Err(AmlError::InvalidParent)}
        if self.find_child(parent,name).is_some(){return Err(AmlError::DuplicateName)}
        if self.count==MAX_NAMESPACE_NODES{return Err(AmlError::NamespaceFull)}
        let i=self.count;self.nodes[i]=AmlNode{parent,name,object_type:kind,method_flags:flags,body_start:u32::try_from(start).map_err(|_|AmlError::BadDefinitionBlock)?,body_length:u32::try_from(len).map_err(|_|AmlError::BadDefinitionBlock)?};
        self.count+=1;Ok(i as u16)
    }
}
impl Default for AmlNamespace { fn default()->Self{Self::new()} }

#[derive(Clone,Copy)]
struct Cursor<'a>{ bytes:&'a [u8], pos:usize, end:usize }
impl<'a> Cursor<'a>{
    fn byte(&mut self)->Result<u8,AmlError>{if self.pos>=self.end{return Err(AmlError::Truncated)}let b=self.bytes[self.pos];self.pos+=1;Ok(b)}
    fn take(&mut self,n:usize)->Result<&'a [u8],AmlError>{let e=self.pos.checked_add(n).ok_or(AmlError::Truncated)?;if e>self.end{return Err(AmlError::Truncated)}let s=&self.bytes[self.pos..e];self.pos=e;Ok(s)}
}
pub fn decode_package_length(bytes:&[u8],offset:usize,limit:usize)->Result<(usize,usize),AmlError>{
    let lead=*bytes.get(offset).filter(|_|offset<limit).ok_or(AmlError::Truncated)?;
    let follow=(lead>>6) as usize;
    let encoded=1+follow;
    if offset.checked_add(encoded).filter(|e|*e<=limit).is_none(){return Err(AmlError::Truncated)}
    // Bits 5:4 are reserved when the package length uses continuation
    // bytes. Accepting them silently can make malformed AML shift every
    // enclosing package boundary.
    if follow!=0 && lead&0x30!=0{return Err(AmlError::BadPackageLength)}
    let value=if follow==0{(lead&0x3f) as usize}else{
        let mut v=(lead&0x0f) as usize;
        for i in 0..follow{v|=(bytes[offset+1+i] as usize)<<(4+8*i);}
        v
    };
    if value<encoded{return Err(AmlError::BadPackageLength)}
    let end=offset.checked_add(value).filter(|e|*e<=limit).ok_or(AmlError::BadPackageLength)?;
    Ok((value,end))
}
fn valid_name_seg(s:&[u8])->bool{
    if s.len()!=4{return false}
    let first=s[0];(first==b'_'||first.is_ascii_uppercase())&&s[1..].iter().all(|c|*c==b'_'||c.is_ascii_uppercase()||c.is_ascii_digit())
}
fn parse_name(c:&mut Cursor<'_>)->Result<AmlName,AmlError>{
    let mut n=AmlName::EMPTY;
    let mut b=c.byte()?;
    if b==0x5c{n.rooted=true;b=c.byte()?;}
    while b==0x5e{n.parents=n.parents.checked_add(1).ok_or(AmlError::BadName)?;b=c.byte()?;}
    if n.rooted&&n.parents!=0{return Err(AmlError::BadName)}
    let prefixed=b==0x2e||b==0x2f;
    let count=match b{0x00=>0,0x2e=>2,0x2f=>{let n=c.byte()? as usize;if n==0{return Err(AmlError::BadName)}n},_=>1};
    if count>MAX_NAME_SEGMENTS{return Err(AmlError::BadName)}
    for i in 0..count{
        let mut segment=[0u8;4];
        if i==0&&!prefixed {
            segment[0]=b;
            segment[1..].copy_from_slice(c.take(3)?);
        }else{
            segment.copy_from_slice(c.take(4)?);
        }
        if !valid_name_seg(&segment){return Err(AmlError::BadName)}
        n.segments[i]=segment;
    }
    n.count=count as u8;Ok(n)
}
fn resolve_parent(ns:&AmlNamespace,current:u16,n:&AmlName)->Result<u16,AmlError>{
    let mut p=if n.rooted{0}else{current};
    for _ in 0..n.parents{p=ns.nodes.get(p as usize).ok_or(AmlError::InvalidParent)?.parent;}
    Ok(p)
}
fn resolve_path(ns:&mut AmlNamespace,current:u16,n:&AmlName,create_intermediate:bool,kind:AmlObjectType,body_start:usize,body_len:usize,flags:u8)->Result<u16,AmlError>{
    if n.count==0{return Ok(if n.rooted{0}else{current})}
    let mut p=resolve_parent(ns,current,n)?;
    for i in 0..n.count as usize{
        let name=n.segments[i];
        if let Some(found)=ns.find_child(p,name){p=found;continue}
        if i+1<n.count as usize&&!create_intermediate{return Err(AmlError::InvalidParent)}
        let k=if i+1==n.count as usize{kind}else{AmlObjectType::Scope};
        p=ns.insert(p,name,k,flags,body_start,body_len)?;
    }
    Ok(p)
}
// Parse AML DataRefObject values rather than merely jumping over package
// payloads. This validates nested package boundaries and prevents malformed
// data objects from hiding truncated objects inside otherwise valid tables.
const MAX_DATA_OBJECT_DEPTH:u8=32;
fn skip_data_object(c:&mut Cursor<'_>)->Result<(),AmlError>{
    skip_data_object_depth(c,0)
}
fn skip_data_object_depth(c:&mut Cursor<'_>,depth:u8)->Result<(),AmlError>{
    if depth>MAX_DATA_OBJECT_DEPTH{return Err(AmlError::BadDefinitionBlock)}
    let op=c.byte()?;
    match op{
        0x00|0x01|0xff=>Ok(()),
        0x0a=>{c.take(1)?;Ok(())},
        0x0b=>{c.take(2)?;Ok(())},
        0x0c=>{c.take(4)?;Ok(())},
        0x0e=>{c.take(8)?;Ok(())},
        0x0d=>{while c.byte()?!=0{}Ok(())},
        0x11|0x12|0x13=>{
            let (_,end)=decode_package_length(c.bytes,c.pos,c.end)?;
            let length_bytes=package_length_bytes(c.bytes,c.pos,c.end)?;
            let payload_start=c.pos.checked_add(length_bytes).ok_or(AmlError::BadPackageLength)?;
            if payload_start>end{return Err(AmlError::BadPackageLength)}
            let mut payload=Cursor{bytes:c.bytes,pos:payload_start,end};
            match op{
                0x11=>{
                    // BufferSize is a TermArg; its byte-list is opaque AML
                    // data and occupies the remainder of the package.
                    skip_data_object_depth(&mut payload,depth+1)?;
                    payload.pos=end;
                }
                0x12=>{
                    let count=payload.byte()? as usize;
                    for _ in 0..count{skip_data_object_depth(&mut payload,depth+1)?;}
                    if payload.pos!=end{return Err(AmlError::BadPackageLength)}
                }
                0x13=>{
                    // VarPackage's element count is a TermArg. The count can
                    // be dynamic, so validate each encoded element to the
                    // package boundary rather than trusting a runtime value.
                    skip_data_object_depth(&mut payload,depth+1)?;
                    while payload.pos<payload.end{
                        skip_data_object_depth(&mut payload,depth+1)?;
                    }
                }
                _=>unreachable!(),
            }
            c.pos=end;
            Ok(())
        }
        0x5c|0x5e|0x2e|0x2f|b'_'|b'A'..=b'Z'=>{
            c.pos-=1;let _=parse_name(c)?;Ok(())
        }
        _=>Err(AmlError::UnsupportedOpcode)
    }
}

fn package_length_bytes(bytes:&[u8],offset:usize,limit:usize)->Result<usize,AmlError>{
    let lead=*bytes.get(offset).filter(|_|offset<limit).ok_or(AmlError::Truncated)?;Ok(1+(lead>>6) as usize)
}

fn read_pkg_value(c:&mut Cursor<'_>)->Result<usize,AmlError>{
    let lead=c.byte()?;let follow=(lead>>6)as usize;
    if follow!=0&&lead&0x30!=0{return Err(AmlError::BadPackageLength)}
    let mut value=if follow==0{(lead&0x3f)as usize}else{(lead&0x0f)as usize};
    for i in 0..follow{value|=(c.byte()? as usize)<<(4+8*i);}
    Ok(value)
}

fn parse_field_list(c:&mut Cursor<'_>,ns:&mut AmlNamespace,parent:u16)->Result<(),AmlError>{
    let mut bit_offset=0usize;
    while c.pos<c.end{
        let op=c.byte()?;
        match op{
            0x00=>{ // ReservedField: advance by encoded bit length.
                bit_offset=bit_offset.checked_add(read_pkg_value(c)?).ok_or(AmlError::BadPackageLength)?;
            }
            0x01=>{c.take(2)?;} // AccessField
            0x02=>{skip_data_object(c)?;} // ConnectField
            0x03=>{c.take(3)?;} // ExtendedAccessField
            _=>{
                c.pos-=1;
                let seg=c.take(4)?;
                if !valid_name_seg(seg){return Err(AmlError::BadName)}
                let mut name=[0;4];name.copy_from_slice(seg);
                let bit_length=read_pkg_value(c)?;
                ns.insert(parent,name,AmlObjectType::Field,0,bit_offset,bit_length)?;
                bit_offset=bit_offset.checked_add(bit_length).ok_or(AmlError::BadPackageLength)?;
            }
        }
    }
    Ok(())
}

fn parse_terms(c:&mut Cursor<'_>,ns:&mut AmlNamespace,parent:u16,depth:u8)->Result<(),AmlError>{
    if depth>32{return Err(AmlError::BadDefinitionBlock)}
    while c.pos<c.end{
        let op=c.byte()?;
        match op{
            0x10|0x14=>{
                let (_,end)=decode_package_length(c.bytes,c.pos,c.end)?;
                let pkg_len_bytes=package_length_bytes(c.bytes,c.pos,c.end)?;
                let pkg_start=c.pos+pkg_len_bytes;
                let mut sub=Cursor{bytes:c.bytes,pos:pkg_start,end};
                let name=parse_name(&mut sub)?;
                if op==0x10{
                    let scope=resolve_path(ns,parent,&name,true,AmlObjectType::Scope,0,0,0)?;
                    parse_terms(&mut sub,ns,scope,depth+1)?;
                }else{
                    let flags=sub.byte()?;
                    let body_start=sub.pos;let body_len=sub.end-sub.pos;
                    resolve_path(ns,parent,&name,true,AmlObjectType::Method,body_start,body_len,flags)?;
                }
                c.pos=end;
            }
            0x08=>{
                let name=parse_name(c)?;let value_start=c.pos;skip_data_object(c)?;
                resolve_path(ns,parent,&name,true,AmlObjectType::Name,value_start,c.pos-value_start,0)?;
            }
            0x06=>{
                let source=parse_name(c)?;let alias=parse_name(c)?;
                let _=resolve_path(ns,parent,&source,true,AmlObjectType::Alias,0,0,0)?;
                resolve_path(ns,parent,&alias,true,AmlObjectType::Alias,0,0,0)?;
            }
            0x15=>{let name=parse_name(c)?;let object_type=c.byte()?;let flags=if object_type==8{c.byte()?}else{0};resolve_path(ns,parent,&name,true,AmlObjectType::External,0,0,flags)?;}
            0x5b=>{
                let ext=c.byte()?;
                match ext{
                    0x01=>{let name=parse_name(c)?;let _sync=c.byte()?;resolve_path(ns,parent,&name,true,AmlObjectType::Mutex,0,0,0)?;}
                    0x02=>{let name=parse_name(c)?;resolve_path(ns,parent,&name,true,AmlObjectType::Event,0,0,0)?;}
                    0x80=>{
                        let name=parse_name(c)?;let space=c.byte()?;let start=c.pos;
                        skip_data_object(c)?;skip_data_object(c)?;
                        resolve_path(ns,parent,&name,true,AmlObjectType::OperationRegion,start,c.pos-start,space)?;
                    }
                    0x81|0x86|0x87=>{
                        let (_,end)=decode_package_length(c.bytes,c.pos,c.end)?;
                        let plen=package_length_bytes(c.bytes,c.pos,c.end)?;
                        let mut sub=Cursor{bytes:c.bytes,pos:c.pos+plen,end};
                        let region=parse_name(&mut sub)?;
                        if ext==0x86{let _index=parse_name(&mut sub)?;let _data=parse_name(&mut sub)?;}
                        if ext==0x87{let _=parse_name(&mut sub)?;skip_data_object(&mut sub)?;}
                        let _flags=sub.byte()?;
                        let node=resolve_path(ns,parent,&region,true,if ext==0x81{AmlObjectType::Field}else if ext==0x86{AmlObjectType::IndexField}else{AmlObjectType::BankField},0,0,0)?;
                        parse_field_list(&mut sub,ns,parent)?;
                        c.pos=end;
                    }
                    0x88=>{
                        // DataRegion(Name, SignatureString, OEMIDString,
                        // OEMTableIDString). Preserve the three data objects
                        // for later namespace evaluation; do not interpret
                        // firmware-provided table identifiers in the parser.
                        let name=parse_name(c)?;
                        let start=c.pos;
                        skip_data_object(c)?;skip_data_object(c)?;skip_data_object(c)?;
                        resolve_path(ns,parent,&name,true,AmlObjectType::DataRegion,start,c.pos-start,0)?;
                    }
                    0x82|0x83|0x84|0x85=>{
                        let (_,end)=decode_package_length(c.bytes,c.pos,c.end)?;
                        let plen=package_length_bytes(c.bytes,c.pos,c.end)?;
                        let mut sub=Cursor{bytes:c.bytes,pos:c.pos+plen,end};
                        let name=parse_name(&mut sub)?;
                        let kind=match ext{0x82=>AmlObjectType::Device,0x83=>AmlObjectType::Processor,0x84=>AmlObjectType::PowerResource,_=>AmlObjectType::ThermalZone};
                        let node=resolve_path(ns,parent,&name,true,kind,0,0,0)?;
                        match ext{0x83=>{sub.take(6)?;},0x84=>{sub.take(3)?;},_=>{}}
                        parse_terms(&mut sub,ns,node,depth+1)?;
                        c.pos=end;
                    }
                    _=>return Err(AmlError::UnsupportedOpcode)
                }
            }
            _=>return Err(AmlError::UnsupportedOpcode)
        }
    }
    Ok(())
}

/// Load a validated DSDT/SSDT definition block body into a bounded namespace.
/// The caller must validate the SDT signature, length and checksum first.
pub fn load_definition_block(aml:&[u8],namespace:&mut AmlNamespace)->Result<(),AmlError>{
    // Namespace insertion is append-only. Restoring the original count on
    // failure makes each definition-block load atomic without copying the
    // large fixed-capacity namespace onto the kernel stack.
    let original_count=namespace.count;
    let mut c=Cursor{bytes:aml,pos:0,end:aml.len()};
    match parse_terms(&mut c,namespace,0,0){
        Ok(())=>Ok(()),
        Err(error)=>{namespace.count=original_count;Err(error)}
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn package_length_decodes_all_widths(){
        assert_eq!(decode_package_length(&[0x3f;63],0,63),Ok((63,63)));
        assert_eq!(decode_package_length(&[0x41,0x01,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],0,17),Ok((17,17)));
        assert_eq!(decode_package_length(&[0x80,1,0,0,0,0,0,0,0,0,0,0,0,0,0,0],0,16),Ok((16,16)));
    }
    #[test]fn loads_device_and_name_namespace(){
        // Device(PCI0) { Name(_HID, EisaId("PNP0A08")) } -- the name value
        // is deliberately a simple AML string in this initial parser slice.
        let aml=[0x5b,0x82,0x13,0x50,b'P',b'C',b'I',b'0',0x08,b'_',
            b'H',b'I',b'D',0x0d,b'P',b'N',b'P',b'0',b'A',b'0',b'8',0];
        let mut ns=AmlNamespace::new();
        assert!(load_definition_block(&aml,&mut ns).is_ok());
        assert_eq!(ns.find_child(0,*b"PCI0"),Some(1));
        assert_eq!(ns.find_child(1,*b"_HID"),Some(2));
    }
    #[test]fn loads_operation_region_and_named_field(){
        let aml=[
            0x5b,0x80,b'R',b'E',b'G',b'N',0x00,0x0a,0x00,0x0a,0x10,
            0x5b,0x81,0x0b,b'R',b'E',b'G',b'N',0x00,b'D',b'A',b'T',b'0',0x08
        ];
        let mut ns=AmlNamespace::new();
        assert!(load_definition_block(&aml,&mut ns).is_ok());
        assert_eq!(ns.find_child(0,*b"REGN"),Some(1));
        assert_eq!(ns.nodes[1].object_type,AmlObjectType::OperationRegion);
        assert_eq!(ns.find_child(0,*b"DAT0"),Some(2));
        assert_eq!(ns.nodes[2].body_start,0);
        assert_eq!(ns.nodes[2].body_length,8);
        assert_eq!(ns.nodes[2].object_type,AmlObjectType::Field);
    }
    #[test]fn loads_mutex_event_and_external(){
        let aml=[0x5b,0x01,b'M',b'T',b'X',b'0',0,
                 0x5b,0x02,b'E',b'V',b'T',b'0',
                 0x15,b'M',b'E',b'T',b'0',8,2];
        let mut ns=AmlNamespace::new();
        assert!(load_definition_block(&aml,&mut ns).is_ok());
        assert_eq!(ns.nodes[1].object_type,AmlObjectType::Mutex);
        assert_eq!(ns.nodes[2].object_type,AmlObjectType::Event);
        assert_eq!(ns.nodes[3].object_type,AmlObjectType::External);
        assert_eq!(ns.nodes[3].method_flags,2);
    }
    #[test]fn rejects_truncated_package(){
        let mut ns=AmlNamespace::new();
        assert_eq!(load_definition_block(&[0x10,0x40,0x5c],&mut ns),Err(AmlError::BadPackageLength));
    }
    #[test]fn rejects_reserved_package_length_bits(){
        let mut ns=AmlNamespace::new();
        assert_eq!(decode_package_length(&[0x61,0x00],0,2),Err(AmlError::BadPackageLength));
        assert_eq!(load_definition_block(&[0x10,0x61,0x00],&mut ns),Err(AmlError::BadPackageLength));
    }
    #[test]fn validates_nested_package_data_objects(){
        // Name(PKG0, Package(2) { ByteConst(1), Package(1) { Ones } })
        let aml=[0x08,b'P',b'K',b'G',b'0',0x12,0x08,2,
                 0x0a,1,0x12,0x03,1,0xff];
        let mut ns=AmlNamespace::new();
        assert!(load_definition_block(&aml,&mut ns).is_ok());
        assert_eq!(ns.find_child(0,*b"PKG0"),Some(1));
    }
    #[test]fn rejects_truncated_nested_package_without_mutating_namespace(){
        // The outer Package declares one element, whose DWord payload is
        // truncated at the end of the definition block.
        let aml=[0x08,b'B',b'A',b'D',b'0',0x12,0x05,1,0x0c,0x11,0x22];
        let mut ns=AmlNamespace::new();
        assert_eq!(load_definition_block(&aml,&mut ns),Err(AmlError::Truncated));
        assert_eq!(ns.count,1);
        assert_eq!(ns.find_child(0,*b"BAD0"),None);
    }
    #[test]fn rejects_unterminated_string_inside_package(){
        let aml=[0x08,b'B',b'A',b'D',b'1',0x12,0x05,1,0x0d,b'x',b'y'];
        let mut ns=AmlNamespace::new();
        assert_eq!(load_definition_block(&aml,&mut ns),Err(AmlError::Truncated));
        assert_eq!(ns.count,1);
    }

    #[test]fn loads_data_region_declaration(){
        // DataRegion(DREG, "SSDT", "OEMID", "TABLEID")
        let aml=[0x5b,0x88,b'D',b'R',b'E',b'G',
            0x0d,b'S',b'S',b'D',b'T',0,
            0x0d,b'O',b'E',b'M',b'I',b'D',0,
            0x0d,b'T',b'A',b'B',b'L',b'E',b'I',b'D',0];
        let mut ns=AmlNamespace::new();
        assert!(load_definition_block(&aml,&mut ns).is_ok());
        let id=ns.find_child(0,*b"DREG").unwrap() as usize;
        assert_eq!(ns.nodes[id].object_type,AmlObjectType::DataRegion);
        assert_eq!(ns.nodes[id].body_length,22);
    }
    #[test]fn rejects_invalid_name_prefix_forms(){
        let mut ns=AmlNamespace::new();
        // Root prefix followed by a parent prefix is not a valid NameString.
        assert_eq!(load_definition_block(&[0x08,0x5c,0x5e,b'A',b'B',b'C',b'D'],&mut ns),Err(AmlError::BadName));
        assert_eq!(ns.count,1);
        // MultiNamePrefix must carry at least one NameSeg.
        assert_eq!(load_definition_block(&[0x08,0x2f,0],&mut ns),Err(AmlError::BadName));
    }

}
