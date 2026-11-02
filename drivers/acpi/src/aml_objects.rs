//! Bounded AML runtime data-object decoder.
//! Values live in a caller-owned fixed arena; no heap allocation or firmware I/O
//! occurs here. Package elements reference other arena entries by index.
#![no_std]
use crate::aml::{decode_package_length,AmlError,AmlNamespace,AmlObjectType};

pub const MAX_AML_VALUES:usize=128;
pub const MAX_AML_BYTES:usize=256;
pub const MAX_PACKAGE_ELEMENTS:usize=16;

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
#[repr(u8)]
pub enum AmlValueKind { Empty=0,Uninitialized=1,Integer=2,String=3,Buffer=4,Package=5 }
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct AmlRuntimeValue {
    pub kind:AmlValueKind,
    pub integer:u64,
    pub bytes:[u8;MAX_AML_BYTES],
    pub byte_length:u16,
    pub elements:[u16;MAX_PACKAGE_ELEMENTS],
    pub element_count:u8,
}
impl AmlRuntimeValue {
    pub const EMPTY:Self=Self{kind:AmlValueKind::Empty,integer:0,bytes:[0;MAX_AML_BYTES],byte_length:0,elements:[0;MAX_PACKAGE_ELEMENTS],element_count:0};
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum ObjectError { Truncated,BadPackageLength,UnsupportedOpcode,ValueArenaFull,ValueTooLarge,TooManyElements,MalformedObject }
impl From<AmlError> for ObjectError { fn from(e:AmlError)->Self { match e { AmlError::Truncated=>Self::Truncated,_=>Self::BadPackageLength } } }

pub struct AmlObjectArena { pub values:[AmlRuntimeValue;MAX_AML_VALUES],pub count:usize }
impl AmlObjectArena {
    pub const fn new()->Self { Self{values:[AmlRuntimeValue::EMPTY;MAX_AML_VALUES],count:0} }
    fn push(&mut self,value:AmlRuntimeValue)->Result<u16,ObjectError>{
        if self.count==MAX_AML_VALUES{return Err(ObjectError::ValueArenaFull)}
        let id=self.count as u16;self.values[self.count]=value;self.count+=1;Ok(id)
    }
    pub fn get(&self,id:u16)->Option<&AmlRuntimeValue>{self.values.get(id as usize).filter(|_|(id as usize)<self.count)}
}
impl Default for AmlObjectArena { fn default()->Self { Self::new() } }

fn byte(bytes:&[u8],pos:&mut usize,end:usize)->Result<u8,ObjectError>{
    if *pos>=end{return Err(ObjectError::Truncated)}let b=bytes[*pos];*pos+=1;Ok(b)
}
fn take<'a>(bytes:&'a [u8],pos:&mut usize,n:usize,end:usize)->Result<&'a [u8],ObjectError>{
    let next=pos.checked_add(n).ok_or(ObjectError::Truncated)?;
    if next>end{return Err(ObjectError::Truncated)}
    let out=&bytes[*pos..next];*pos=next;Ok(out)
}
fn integer_term(bytes:&[u8],pos:&mut usize,end:usize)->Result<u64,ObjectError>{
    match byte(bytes,pos,end)? {
        0x00=>Ok(0),0x01=>Ok(1),0xff=>Ok(u64::MAX),
        0x0a=>Ok(byte(bytes,pos,end)? as u64),
        0x0b=>{let b=take(bytes,pos,2,end)?;Ok(u16::from_le_bytes([b[0],b[1]]) as u64)}
        0x0c=>{let b=take(bytes,pos,4,end)?;Ok(u32::from_le_bytes([b[0],b[1],b[2],b[3]]) as u64)}
        0x0e=>{let b=take(bytes,pos,8,end)?;Ok(u64::from_le_bytes(b.try_into().map_err(|_|ObjectError::Truncated)?))}
        _=>Err(ObjectError::UnsupportedOpcode)
    }
}
fn package_end(bytes:&[u8],pos:usize,end:usize)->Result<(usize,usize),ObjectError>{
    let (_,finish)=decode_package_length(bytes,pos,end).map_err(ObjectError::from)?;
    let lead=*bytes.get(pos).ok_or(ObjectError::Truncated)?;
    Ok((pos+1+((lead>>6) as usize),finish))
}

/// Decode one AML DataRefObject and return its arena index and byte offset after
/// the object. Nested packages are bounded by both the package end and arena.
pub fn decode_data_object(bytes:&[u8],offset:usize,limit:usize,arena:&mut AmlObjectArena)->Result<(u16,usize),ObjectError>{
    if offset>limit||limit>bytes.len(){return Err(ObjectError::Truncated)}
    let mut pos=offset;
    let opcode=byte(bytes,&mut pos,limit)?;
    let mut value=AmlRuntimeValue::EMPTY;
    match opcode {
        0x00|0x01|0xff=>{value.kind=AmlValueKind::Integer;value.integer=match opcode{0x00=>0,0x01=>1,_=>u64::MAX};}
        0x0a|0x0b|0x0c|0x0e=>{
            pos-=1;value.kind=AmlValueKind::Integer;value.integer=integer_term(bytes,&mut pos,limit)?;
        }
        0x0d=>{
            value.kind=AmlValueKind::String;
            let start=pos;
            while pos<limit&&bytes[pos]!=0{pos+=1;}
            if pos==limit{return Err(ObjectError::Truncated)}
            let len=pos-start;if len>MAX_AML_BYTES{return Err(ObjectError::ValueTooLarge)}
            value.bytes[..len].copy_from_slice(&bytes[start..pos]);value.byte_length=len as u16;pos+=1;
        }
        0x11=>{
            let (body,end)=package_end(bytes,pos,limit)?;pos=body;
            let declared=integer_term(bytes,&mut pos,end)? as usize;
            if declared>MAX_AML_BYTES{return Err(ObjectError::ValueTooLarge)}
            let available=end.saturating_sub(pos);let copy=core::cmp::min(declared,available);
            value.kind=AmlValueKind::Buffer;value.bytes[..copy].copy_from_slice(&bytes[pos..pos+copy]);
            value.byte_length=declared as u16;pos=end;
        }
        0x12|0x13=>{
            let (body,end)=package_end(bytes,pos,limit)?;pos=body;
            let count=if opcode==0x12{byte(bytes,&mut pos,end)? as usize}else{integer_term(bytes,&mut pos,end)? as usize};
            if count>MAX_PACKAGE_ELEMENTS{return Err(ObjectError::TooManyElements)}
            value.kind=AmlValueKind::Package;
            for _ in 0..count {
                if pos>=end { // AML packages may omit trailing elements; absent slots are integer zero.
                    let zero=arena.push({let mut z=AmlRuntimeValue::EMPTY;z.kind=AmlValueKind::Integer;z})?;
                    value.elements[value.element_count as usize]=zero;value.element_count+=1;continue
                }
                let (child,next)=decode_data_object(bytes,pos,end,arena)?;
                value.elements[value.element_count as usize]=child;value.element_count+=1;pos=next;
            }
            pos=end;
        }
        _=>return Err(ObjectError::UnsupportedOpcode)
    }
    let id=arena.push(value)?;
    Ok((id,pos))
}

/// Decode a namespace NameOp's stored initializer into the supplied arena.
/// The namespace loader stores the exact initializer byte range on Name nodes.
pub fn decode_namespace_name(aml:&[u8],namespace:&AmlNamespace,node_index:u16,arena:&mut AmlObjectArena)->Result<u16,ObjectError>{
    let node=namespace.nodes.get(node_index as usize).filter(|_|(node_index as usize)<namespace.count).ok_or(ObjectError::MalformedObject)?;
    if node.object_type!=AmlObjectType::Name{return Err(ObjectError::MalformedObject)}
    let start=node.body_start as usize;
    let end=start.checked_add(node.body_length as usize).ok_or(ObjectError::Truncated)?;
    if end>aml.len(){return Err(ObjectError::Truncated)}
    let (value,next)=decode_data_object(aml,start,end,arena)?;
    if next!=end{return Err(ObjectError::MalformedObject)}
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn resolves_namespace_name_initializer(){
        let aml=[0x08,b'_',b'H',b'I',b'D',0x0d,b'P',b'N',b'P',b'0',b'C',0];
        let mut namespace=AmlNamespace::new();
        crate::aml::load_definition_block(&aml,&mut namespace).unwrap();
        let id=namespace.find_child(0,*b"_HID").unwrap();
        let mut arena=AmlObjectArena::new();
        let value=decode_namespace_name(&aml,&namespace,id,&mut arena).unwrap();
        let object=arena.get(value).unwrap();
        assert_eq!(object.kind,AmlValueKind::String);
        assert_eq!(&object.bytes[..5],b"PNP0C");
    }
    #[test] fn decodes_integer_and_string_objects(){
        let mut arena=AmlObjectArena::new();
        let (n,p)=decode_data_object(&[0x0a,42],0,2,&mut arena).unwrap();
        assert_eq!(p,2);assert_eq!(arena.get(n).unwrap().integer,42);
        let (s,p)=decode_data_object(&[0x0d,b'O',b'K',0],0,4,&mut arena).unwrap();
        assert_eq!(p,4);let v=arena.get(s).unwrap();assert_eq!(v.kind,AmlValueKind::String);assert_eq!(&v.bytes[..2],b"OK");
    }
    #[test] fn decodes_buffer_with_zero_filled_tail(){
        // Buffer(PkgLength=5, ByteConst(4), bytes 0xaa,0xbb)
        let mut arena=AmlObjectArena::new();
        let (id,end)=decode_data_object(&[0x11,0x05,0x0a,4,0xaa,0xbb],0,6,&mut arena).unwrap();
        let v=arena.get(id).unwrap();assert_eq!(end,6);assert_eq!(v.kind,AmlValueKind::Buffer);
        assert_eq!(v.byte_length,4);assert_eq!(&v.bytes[..4],&[0xaa,0xbb,0,0]);
    }
    #[test] fn decodes_nested_packages_by_arena_reference(){
        // Package(2) { One, Package(1) { ByteConst(9) } }
        let bytes=[0x12,0x08,2,0x01,0x12,0x04,1,0x0a,9];
        let mut arena=AmlObjectArena::new();
        let (id,end)=decode_data_object(&bytes,0,bytes.len(),&mut arena).unwrap();
        assert_eq!(end,bytes.len());let outer=arena.get(id).unwrap();assert_eq!(outer.element_count,2);
        let inner=arena.get(outer.elements[1]).unwrap();assert_eq!(inner.kind,AmlValueKind::Package);
        let nine=arena.get(inner.elements[0]).unwrap();assert_eq!(nine.integer,9);
    }
    #[test] fn rejects_truncated_strings_and_oversized_packages(){
        let mut arena=AmlObjectArena::new();
        assert_eq!(decode_data_object(&[0x0d,b'x'],0,2,&mut arena),Err(ObjectError::Truncated));
        assert_eq!(decode_data_object(&[0x12,0x12,17],0,3,&mut arena),Err(ObjectError::TooManyElements));
    }
}
