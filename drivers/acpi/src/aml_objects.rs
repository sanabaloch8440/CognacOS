//! Bounded AML runtime data-object decoder.
//! Values live in a caller-owned fixed arena; no heap allocation or firmware I/O
//! occurs here. Package elements reference other arena entries by index.
/*
 * FIXME(ACPI-PRODUCTION) — AML runtime objects
 * FIXME(ACPI-PRODUCTION): Runtime objects use bounded caller-owned storage; complete ACPI AML value/reference behavior and lifetime rules remain a production gate. Intended completion: exact Integer/String/Buffer/Package/VarPackage conversions, reference and index semantics, ownership/aliasing rules, dynamic package evaluation, capacity/size limits, and complete malformed-object handling. Every decoder must enforce enclosing boundaries and checked arithmetic, and failures must leave the arena in a consistent state.
 */

#![no_std]
use crate::aml::{decode_package_length,AmlError,AmlNamespace,AmlObjectType};
use crate::aml_exec::evaluate_term_argument_in_context;
use crate::aml_runtime::AmlBindings;

/// A 128-entry _PRT may require one outer Package, 128 route Packages,
/// and four or more leaf values per route. Keep headroom for method temporaries.
pub const MAX_AML_VALUES:usize=768;
pub const MAX_AML_BYTES:usize=256;
/// ACPI _PRT packages may contain up to 128 routes; each route has four fields.
pub const MAX_PACKAGE_ELEMENTS:usize=128;

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
#[repr(u8)]
pub enum AmlValueKind { Empty=0,Uninitialized=1,Integer=2,String=3,Buffer=4,Package=5,ObjectReference=6 }
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct AmlRuntimeValue {
    pub kind:AmlValueKind,
    pub integer:u64,
    pub bytes:[u8;MAX_AML_BYTES],
    pub byte_length:u16,
    pub elements:[u16;MAX_PACKAGE_ELEMENTS],
    pub element_count:u8,
    /// Encoded target kind and identity for AML ObjectReference values.
    pub reference_kind:u8,
    pub reference_id:u16,
    pub reference_index:u16,
    pub reference_bit_width:u16,
}
impl AmlRuntimeValue {
    pub const EMPTY:Self=Self{kind:AmlValueKind::Empty,integer:0,bytes:[0;MAX_AML_BYTES],byte_length:0,elements:[0;MAX_PACKAGE_ELEMENTS],element_count:0,reference_kind:0,reference_id:0,reference_index:0,reference_bit_width:0};
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum ObjectError { Truncated,BadPackageLength,UnsupportedOpcode,InvalidTermArgument,ValueArenaFull,ValueTooLarge,TooManyElements,MalformedObject }
impl From<AmlError> for ObjectError { fn from(e:AmlError)->Self { match e { AmlError::Truncated=>Self::Truncated,_=>Self::BadPackageLength } } }

pub struct AmlObjectArena { pub values:[AmlRuntimeValue;MAX_AML_VALUES],pub count:usize }
impl AmlObjectArena {
    pub const fn new()->Self { Self{values:[AmlRuntimeValue::EMPTY;MAX_AML_VALUES],count:0} }
    /// Initialize a pool-backed arena without constructing its large fixed array
    /// as a temporary on the caller's kernel stack. All-zero is the exact EMPTY
    /// representation (AmlValueKind::Empty has discriminant zero).
    ///
    /// # Safety
    /// destination must be non-null, correctly aligned, and writable for
    /// size_of::<AmlObjectArena>() bytes, with no live arena value at that address.
    pub unsafe fn initialize_in_place(destination:*mut Self) {
        unsafe { core::ptr::write_bytes(destination.cast::<u8>(), 0, core::mem::size_of::<Self>()); }
    }
    fn push(&mut self,value:AmlRuntimeValue)->Result<u16,ObjectError>{
        if self.count==MAX_AML_VALUES{return Err(ObjectError::ValueArenaFull)}
        let id=self.count as u16;self.values[self.count]=value;self.count+=1;Ok(id)
    }
/// Return an immutable runtime object by arena index when that index is allocated.
    pub fn get(&self,id:u16)->Option<&AmlRuntimeValue>{self.values.get(id as usize).filter(|_|(id as usize)<self.count)}
/// Return a mutable runtime object by arena index when that index is allocated.
    pub fn get_mut(&mut self,id:u16)->Option<&mut AmlRuntimeValue>{if id as usize>=self.count{return None}self.values.get_mut(id as usize)}
    /// Allocate a caller-constructed value in this bounded arena.
    pub fn allocate(&mut self,value:AmlRuntimeValue)->Result<u16,ObjectError>{self.push(value)}
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

/// Maximum nested Buffer/Package depth accepted by the fixed-stack decoder.
pub const MAX_DATA_OBJECT_DEPTH:u8=32;

/// Decode one AML DataRefObject transactionally. If any nested initializer is
/// malformed, arena allocations made by this decode are rolled back.
pub fn decode_data_object(bytes:&[u8],offset:usize,limit:usize,arena:&mut AmlObjectArena)->Result<(u16,usize),ObjectError>{
    let checkpoint=arena.count;
    let mut bindings = AmlBindings::with_aml_revision(2);
    match decode_data_object_depth(bytes,offset,limit,arena,&mut bindings,0){
        Ok(result)=>Ok(result),
        Err(error)=>{arena.count=checkpoint;Err(error)}
    }
}

fn decode_data_object_depth(bytes:&[u8],offset:usize,limit:usize,arena:&mut AmlObjectArena,bindings:&mut AmlBindings,depth:u8)->Result<(u16,usize),ObjectError>{
    if depth>MAX_DATA_OBJECT_DEPTH{return Err(ObjectError::MalformedObject)}
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
            let (declared,consumed)=evaluate_term_argument_in_context(&bytes[pos..end],arena,bindings).map_err(|_|ObjectError::InvalidTermArgument)?;
            pos=pos.checked_add(consumed).ok_or(ObjectError::Truncated)?;
            let declared=usize::try_from(declared).map_err(|_|ObjectError::ValueTooLarge)?;
            if declared>MAX_AML_BYTES{return Err(ObjectError::ValueTooLarge)}
            let available=end.saturating_sub(pos);
            if available>declared{return Err(ObjectError::MalformedObject)}
            let copy=available;
            value.kind=AmlValueKind::Buffer;value.bytes[..copy].copy_from_slice(&bytes[pos..pos+copy]);
            value.byte_length=declared as u16;pos=end;
        }
        0x12|0x13=>{
            let (body,end)=package_end(bytes,pos,limit)?;pos=body;
            let count=if opcode==0x12{byte(bytes,&mut pos,end)? as usize}else{
                let (count,consumed)=evaluate_term_argument_in_context(&bytes[pos..end],arena,bindings).map_err(|_|ObjectError::InvalidTermArgument)?;
                pos=pos.checked_add(consumed).ok_or(ObjectError::Truncated)?;
                usize::try_from(count).map_err(|_|ObjectError::TooManyElements)?
            };
            if count>MAX_PACKAGE_ELEMENTS{return Err(ObjectError::TooManyElements)}
            value.kind=AmlValueKind::Package;
            for _ in 0..count {
                if pos>=end { // Missing package initializers remain Uninitialized.
                    let missing=arena.push({let mut z=AmlRuntimeValue::EMPTY;z.kind=AmlValueKind::Uninitialized;z})?;
                    value.elements[value.element_count as usize]=missing;value.element_count+=1;continue
                }
                let (child,next)=decode_data_object_depth(bytes,pos,end,arena,bindings,depth+1)?;
                value.elements[value.element_count as usize]=child;value.element_count+=1;pos=next;
            }
            if pos!=end{return Err(ObjectError::MalformedObject)}
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
    fn test_arena() -> std::boxed::Box<AmlObjectArena> {
        let mut storage = std::boxed::Box::<AmlObjectArena>::new_uninit();
        unsafe {
            core::ptr::write_bytes(
                storage.as_mut_ptr().cast::<u8>(), 0,
                core::mem::size_of::<AmlObjectArena>(),
            );
            storage.assume_init()
        }
    }

    use super::*;
    #[test] fn resolves_namespace_name_initializer(){
        let aml=[0x08,b'_',b'H',b'I',b'D',0x0d,b'P',b'N',b'P',b'0',b'C',0];
        let mut namespace=AmlNamespace::new();
        crate::aml::load_definition_block(&aml,&mut namespace).unwrap();
        let id=namespace.find_child(0,*b"_HID").unwrap();
        let mut arena = test_arena();
        let value=decode_namespace_name(&aml,&namespace,id,&mut arena).unwrap();
        let object=arena.get(value).unwrap();
        assert_eq!(object.kind,AmlValueKind::String);
        assert_eq!(&object.bytes[..5],b"PNP0C");
    }
    #[test] fn decodes_integer_and_string_objects(){
        let mut arena = test_arena();
        let (n,p)=decode_data_object(&[0x0a,42],0,2,&mut arena).unwrap();
        assert_eq!(p,2);assert_eq!(arena.get(n).unwrap().integer,42);
        let (s,p)=decode_data_object(&[0x0d,b'O',b'K',0],0,4,&mut arena).unwrap();
        assert_eq!(p,4);let v=arena.get(s).unwrap();assert_eq!(v.kind,AmlValueKind::String);assert_eq!(&v.bytes[..2],b"OK");
    }
    #[test] fn decodes_buffer_with_zero_filled_tail(){
        // Buffer(PkgLength=5, ByteConst(4), bytes 0xaa,0xbb)
        let mut arena = test_arena();
        let (id,end)=decode_data_object(&[0x11,0x05,0x0a,4,0xaa,0xbb],0,6,&mut arena).unwrap();
        let v=arena.get(id).unwrap();assert_eq!(end,6);assert_eq!(v.kind,AmlValueKind::Buffer);
        assert_eq!(v.byte_length,4);assert_eq!(&v.bytes[..4],&[0xaa,0xbb,0,0]);
    }
    #[test] fn evaluates_computed_buffer_size_term_arguments(){
        // Buffer(Add(ByteConst(2), ByteConst(2), NullTarget), { 0xaa, 0xbb }).
        let bytes=[0x11,0x09,0x72,0x0a,2,0x0a,2,0x00,0xaa,0xbb];
        let mut arena = test_arena();
        let (id,end)=decode_data_object(&bytes,0,bytes.len(),&mut arena).unwrap();
        let value=arena.get(id).unwrap();
        assert_eq!(end,bytes.len());assert_eq!(value.kind,AmlValueKind::Buffer);
        assert_eq!(value.byte_length,4);assert_eq!(&value.bytes[..4],&[0xaa,0xbb,0,0]);
    }
    #[test] fn rejects_buffer_initializer_bytes_beyond_declared_size(){
        let bytes=[0x11,0x05,0x0a,1,0xaa,0xbb];
        let mut arena = test_arena();
        assert_eq!(decode_data_object(&bytes,0,bytes.len(),&mut arena),Err(ObjectError::MalformedObject));
    }
    #[test] fn decodes_nested_packages_by_arena_reference(){
        // Package(2) { One, Package(1) { ByteConst(9) } }
        let bytes=[0x12,0x08,2,0x01,0x12,0x04,1,0x0a,9];
        let mut arena = test_arena();
        let (id,end)=decode_data_object(&bytes,0,bytes.len(),&mut arena).unwrap();
        assert_eq!(end,bytes.len());let outer=arena.get(id).unwrap();assert_eq!(outer.element_count,2);
        let inner=arena.get(outer.elements[1]).unwrap();assert_eq!(inner.kind,AmlValueKind::Package);
        let nine=arena.get(inner.elements[0]).unwrap();assert_eq!(nine.integer,9);
    }
    #[test] fn evaluates_varpackage_element_count_expression(){
        // VarPackage(Add(One, One, NullTarget), { One, ByteConst(9) }).
        let bytes=[0x13,0x08,0x72,0x01,0x01,0x00,0x01,0x0a,9];
        let mut arena = test_arena();
        let (id,end)=decode_data_object(&bytes,0,bytes.len(),&mut arena).unwrap();
        let package=arena.get(id).unwrap();assert_eq!(end,bytes.len());
        assert_eq!(package.kind,AmlValueKind::Package);assert_eq!(package.element_count,2);
        assert_eq!(arena.get(package.elements[0]).unwrap().integer,1);
        assert_eq!(arena.get(package.elements[1]).unwrap().integer,9);
    }
    #[test] fn decodes_full_128_entry_dynamic_prt_package_with_arena_headroom() {
        // Outer Package(128), each child is Package(4) { 1, 1, 1, 1 }.
        // The outer PkgLength is 899 bytes (two length bytes plus 897 body bytes).
        let mut bytes = std::vec![0x12, 0x43, 0x38, 128];
        for _ in 0..128 {
            bytes.extend_from_slice(&[0x12, 0x06, 4, 0x01, 0x01, 0x01, 0x01]);
        }
        assert_eq!(bytes.len(), 900);
        let mut arena = test_arena();
        let (root, end) = decode_data_object(&bytes, 0, bytes.len(), &mut arena).unwrap();
        assert_eq!(end, bytes.len());
        let package = arena.get(root).unwrap();
        assert_eq!(package.kind, AmlValueKind::Package);
        assert_eq!(package.element_count, 128);
        assert_eq!(arena.count, 641);
        for child in package.elements.iter().take(128) {
            let route = arena.get(*child).unwrap();
            assert_eq!(route.kind, AmlValueKind::Package);
            assert_eq!(route.element_count, 4);
        }
    }
    #[test] fn leaves_omitted_package_slots_uninitialized(){
        let mut arena = test_arena();
        let (id,end)=decode_data_object(&[0x12,0x02,2],0,3,&mut arena).unwrap();
        assert_eq!(end,3);
        let package=arena.get(id).unwrap();
        assert_eq!(package.element_count,2);
        for child in package.elements.iter().take(2) {
            assert_eq!(arena.get(*child).unwrap().kind,AmlValueKind::Uninitialized);
        }
    }
    #[test] fn rejects_excessive_package_depth_without_leaking_arena_entries(){
        let mut bytes=std::vec![0x01];
        for _ in 0..(MAX_DATA_OBJECT_DEPTH as usize+2){
            let body_length=1+bytes.len();
            let short_length=1+body_length;
            let mut package=std::vec![0x12];
            if short_length<64{package.push(short_length as u8);}
            else{let length=2+body_length;package.push(0x40|((length&0x0f)as u8));package.push((length>>4)as u8);}
            package.push(1);package.extend_from_slice(&bytes);bytes=package;
        }
        let mut arena = test_arena();
        assert_eq!(decode_data_object(&bytes,0,bytes.len(),&mut arena),Err(ObjectError::MalformedObject));
        assert_eq!(arena.count,0);
    }
    #[test] fn rejects_package_payload_after_declared_initializer_count(){
        let bytes=[0x12,0x04,1,0x01,0x01];
        let mut arena = test_arena();
        assert_eq!(decode_data_object(&bytes,0,bytes.len(),&mut arena),Err(ObjectError::MalformedObject));
    }
    #[test] fn varpackage_fills_declared_runtime_count_with_uninitialized_tail(){
        // VarPackage(ByteConst(3), { One }) has one explicit initializer and
        // two implicit Uninitialized elements.
        let bytes=[0x13,0x04,0x0a,3,0x01];
        let mut arena=test_arena();
        let (id,end)=decode_data_object(&bytes,0,bytes.len(),&mut arena).unwrap();
        assert_eq!(end,bytes.len());
        let package=arena.get(id).unwrap();
        assert_eq!(package.kind,AmlValueKind::Package);
        assert_eq!(package.element_count,3);
        assert_eq!(arena.get(package.elements[0]).unwrap().integer,1);
        assert_eq!(arena.get(package.elements[1]).unwrap().kind,AmlValueKind::Uninitialized);
        assert_eq!(arena.get(package.elements[2]).unwrap().kind,AmlValueKind::Uninitialized);
    }
    #[test] fn malformed_nested_package_rolls_back_every_partial_allocation(){
        // The nested String is unterminated; the outer package and any
        // successfully decoded children must not remain visible in the arena.
        let bytes=[0x12,0x04,1,0x0d,b'x'];
        let mut arena=test_arena();
        assert_eq!(decode_data_object(&bytes,0,bytes.len(),&mut arena),Err(ObjectError::Truncated));
        assert_eq!(arena.count,0);
        assert!(arena.values.iter().all(|value|value.kind==AmlValueKind::Empty));
    }
    #[test] fn varpackage_accepts_full_bounded_count_with_no_initializers(){
        // VarPackage(ByteConst(128), {}) materializes 128 Uninitialized slots.
        let bytes=[0x13,0x03,0x0a,128];
        let mut arena=test_arena();
        let (id,end)=decode_data_object(&bytes,0,bytes.len(),&mut arena).unwrap();
        assert_eq!(end,bytes.len());
        let package=arena.get(id).unwrap();
        assert_eq!(package.kind,AmlValueKind::Package);
        assert_eq!(package.element_count as usize,MAX_PACKAGE_ELEMENTS);
        for child in package.elements.iter().take(MAX_PACKAGE_ELEMENTS){
            assert_eq!(arena.get(*child).unwrap().kind,AmlValueKind::Uninitialized);
        }
    }
    #[test] fn package_decode_rolls_back_when_arena_budget_is_exhausted(){
        let mut arena=test_arena();
        let mut existing=AmlRuntimeValue::EMPTY;
        existing.kind=AmlValueKind::Integer;
        existing.integer=0xfeed;
        for _ in 0..(MAX_AML_VALUES-1){arena.allocate(existing).unwrap();}
        let before=arena.count;
        // The child fits in the final slot, but the enclosing Package does not.
        let bytes=[0x12,0x02,1];
        assert_eq!(decode_data_object(&bytes,0,bytes.len(),&mut arena),Err(ObjectError::ValueArenaFull));
        assert_eq!(arena.count,before);
        assert_eq!(arena.get((before-1) as u16).unwrap().integer,0xfeed);
        assert!(arena.get(before as u16).is_none());
    }
    #[test] fn rejects_truncated_strings_and_oversized_packages(){
        let mut arena = test_arena();
        assert_eq!(decode_data_object(&[0x0d,b'x'],0,2,&mut arena),Err(ObjectError::Truncated));
        assert_eq!(decode_data_object(&[0x12,0x02,(MAX_PACKAGE_ELEMENTS as u8)+1],0,3,&mut arena),Err(ObjectError::TooManyElements));
    }
}
