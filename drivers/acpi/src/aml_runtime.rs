//! AML-3 bounded runtime references, namespace bindings and conversions.
//! The runtime owns no heap memory and performs no firmware I/O. Callers own
//! the fixed value arena and binding table for the lifetime of an AML scope.
/*
 * FIXME(ACPI-PRODUCTION) — AML runtime orchestration
 * FIXME(ACPI-PRODUCTION): Complete the interpreter runtime lifecycle: method invocation, namespace coordination, object cleanup, serialization, execution budgets, timeout handling, and safe integration with kernel callbacks. No AML callback should execute while holding unrelated PnP locks or at interrupt level.
 */

#![no_std]

use crate::aml::{AmlNamespace,AmlObjectType,MAX_NAMESPACE_NODES};
use crate::aml_objects::{AmlObjectArena,AmlRuntimeValue,AmlValueKind,ObjectError,MAX_AML_BYTES};

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum RuntimeError {
    InvalidNode,UnboundNode,InvalidReference,NotIndexable,IndexOutOfRange,
    TypeMismatch,ValueTooLarge,InvalidObjectLength,InvalidIntegerString,Arena(ObjectError),
}
/// Validate a caller-constructed runtime object's declared byte extent.
fn checked_element_count(value:&AmlRuntimeValue)->Result<usize,RuntimeError>{
    let count=value.element_count as usize;
    if count>crate::aml_objects::MAX_PACKAGE_ELEMENTS{return Err(RuntimeError::InvalidObjectLength)}
    Ok(count)
}

fn checked_byte_length(value:&AmlRuntimeValue)->Result<usize,RuntimeError>{
    let length=value.byte_length as usize;
    if length>MAX_AML_BYTES{return Err(RuntimeError::InvalidObjectLength)}
    Ok(length)
}

impl From<ObjectError> for RuntimeError { fn from(e:ObjectError)->Self{Self::Arena(e)} }

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum AmlReference {
    NamespaceNode(u16),
    PackageElement { package:u16,index:u8 },
    BufferByte { buffer:u16,index:u16 },
}

/// Namespace Name objects are bound to arena values independently of the
/// immutable AML parse tree. This table is caller-owned and bounded.
pub struct AmlBindings {
    values:[u16;MAX_NAMESPACE_NODES],
    bound:[bool;MAX_NAMESPACE_NODES],
}
impl AmlBindings {
    pub const fn new()->Self{Self{values:[0;MAX_NAMESPACE_NODES],bound:[false;MAX_NAMESPACE_NODES]}}
    pub fn bind(&mut self,namespace:&AmlNamespace,node:u16,value:u16)->Result<(),RuntimeError>{
        let canonical=namespace.resolve_alias(node).map_err(|_|RuntimeError::InvalidNode)?;
        let entry=namespace.nodes.get(canonical as usize).filter(|_|(canonical as usize)<namespace.count)
            .ok_or(RuntimeError::InvalidNode)?;
        if entry.object_type!=AmlObjectType::Name{return Err(RuntimeError::TypeMismatch)}
        if canonical as usize>=MAX_NAMESPACE_NODES{return Err(RuntimeError::InvalidNode)}
        self.values[canonical as usize]=value;self.bound[canonical as usize]=true;Ok(())
    }
    pub fn reference_node(&self,namespace:&AmlNamespace,node:u16)->Result<AmlReference,RuntimeError>{
        let canonical=namespace.resolve_alias(node).map_err(|_|RuntimeError::InvalidNode)?;
        if canonical as usize>=namespace.count{return Err(RuntimeError::InvalidNode)}
        Ok(AmlReference::NamespaceNode(canonical))
    }
    fn value_id(&self,node:u16)->Result<u16,RuntimeError>{
        let i=node as usize;if i>=MAX_NAMESPACE_NODES||!self.bound[i]{return Err(RuntimeError::UnboundNode)}
        Ok(self.values[i])
    }
}
impl Default for AmlBindings{fn default()->Self{Self::new()}}

/// RefOf/Index reference construction. Index is zero-based as required by AML.
pub fn index_reference(arena:&AmlObjectArena,source:u16,index:u16)->Result<AmlReference,RuntimeError>{
    let value=arena.get(source).ok_or(RuntimeError::InvalidReference)?;
    match value.kind{
        AmlValueKind::Package=>{
            let count=checked_element_count(value)?;
            if index as usize>=count{return Err(RuntimeError::IndexOutOfRange)}
            Ok(AmlReference::PackageElement{package:source,index:index as u8})
        }
        AmlValueKind::Buffer=>{
            let length=checked_byte_length(value)?;
            if index as usize>=length{return Err(RuntimeError::IndexOutOfRange)}
            Ok(AmlReference::BufferByte{buffer:source,index})
        }
        _=>Err(RuntimeError::NotIndexable),
    }
}

/// Dereference a namespace, package-element, or buffer-byte reference.
pub fn dereference(reference:AmlReference,bindings:&AmlBindings,arena:&AmlObjectArena)
    ->Result<AmlRuntimeValue,RuntimeError>{
    match reference{
        AmlReference::NamespaceNode(node)=>Ok(*arena.get(bindings.value_id(node)?).ok_or(RuntimeError::InvalidReference)?),
        AmlReference::PackageElement{package,index}=>{
            let p=arena.get(package).ok_or(RuntimeError::InvalidReference)?;
            if p.kind!=AmlValueKind::Package||index as usize>=checked_element_count(p)?{return Err(RuntimeError::InvalidReference)}
            Ok(*arena.get(p.elements[index as usize]).ok_or(RuntimeError::InvalidReference)?)
        }
        AmlReference::BufferByte{buffer,index}=>{
            let b=arena.get(buffer).ok_or(RuntimeError::InvalidReference)?;
            if b.kind!=AmlValueKind::Buffer||index as usize>=checked_byte_length(b)?{return Err(RuntimeError::InvalidReference)}
            let mut v=AmlRuntimeValue::EMPTY;v.kind=AmlValueKind::Integer;v.integer=b.bytes[index as usize] as u64;Ok(v)
        }
    }
}

/// Store an existing arena value through an AML reference. Buffer stores are
/// byte-sized and reject values that cannot be represented without truncation.
pub fn store_reference(reference:AmlReference,value:u16,bindings:&mut AmlBindings,
    namespace:&AmlNamespace,arena:&mut AmlObjectArena)->Result<(),RuntimeError>{
    let source=*arena.get(value).ok_or(RuntimeError::InvalidReference)?;
    match reference{
        AmlReference::NamespaceNode(node)=>{
            let canonical=namespace.resolve_alias(node).map_err(|_|RuntimeError::InvalidNode)?;
            let i=canonical as usize;
            if i>=namespace.count||namespace.nodes[i].object_type!=AmlObjectType::Name{return Err(RuntimeError::TypeMismatch)}
            bindings.values[i]=value;bindings.bound[i]=true;Ok(())
        }
        AmlReference::PackageElement{package,index}=>{
            let p=arena.get_mut(package).ok_or(RuntimeError::InvalidReference)?;
            if p.kind!=AmlValueKind::Package||index as usize>=checked_element_count(p)?{return Err(RuntimeError::InvalidReference)}
            p.elements[index as usize]=value;Ok(())
        }
        AmlReference::BufferByte{buffer,index}=>{
            let b=arena.get_mut(buffer).ok_or(RuntimeError::InvalidReference)?;
            if b.kind!=AmlValueKind::Buffer||index>=b.byte_length{return Err(RuntimeError::InvalidReference)}
            if source.kind!=AmlValueKind::Integer||source.integer>u8::MAX as u64{return Err(RuntimeError::TypeMismatch)}
            b.bytes[index as usize]=source.integer as u8;Ok(())
        }
    }
}

/// AML-style scalar conversion. Integer strings are interpreted as
/// hexadecimal text; buffers are little-endian and limited to eight bytes.
pub fn to_integer(value:&AmlRuntimeValue)->Result<u64,RuntimeError>{
    match value.kind{
        AmlValueKind::Integer=>Ok(value.integer),
        AmlValueKind::Buffer=>{
            let length=checked_byte_length(value)?;
            if length>8{return Err(RuntimeError::ValueTooLarge)}
            let mut out=0u64;
            for i in 0..length{out|=(value.bytes[i] as u64)<<(i*8)}
            Ok(out)
        }
        AmlValueKind::String=>{
            let length=checked_byte_length(value)?;
            if length==0{return Err(RuntimeError::InvalidIntegerString)}
            let mut out=0u64;
            for &ch in &value.bytes[..length]{
                // byte_length excludes the AML terminator; embedded NUL is malformed.
                let nibble=match ch{b'0'..=b'9'=>(ch-b'0')as u64,b'a'..=b'f'=>(ch-b'a'+10)as u64,b'A'..=b'F'=>(ch-b'A'+10)as u64,_=>return Err(RuntimeError::InvalidIntegerString)};
                out=out.checked_mul(16).and_then(|v|v.checked_add(nibble)).ok_or(RuntimeError::ValueTooLarge)?;
            }
            Ok(out)
        }
        _=>Err(RuntimeError::TypeMismatch),
    }
}

/// Convert an integer, string, or buffer to a fixed-length AML Buffer. The
/// output is zero-padded and never exceeds the runtime object's byte capacity.
pub fn to_buffer(value:&AmlRuntimeValue,length:usize)->Result<AmlRuntimeValue,RuntimeError>{
    if length>MAX_AML_BYTES{return Err(RuntimeError::ValueTooLarge)}
    let mut out=AmlRuntimeValue::EMPTY;out.kind=AmlValueKind::Buffer;out.byte_length=length as u16;
    match value.kind{
        AmlValueKind::Integer=>{
            for i in 0..core::cmp::min(length,8){out.bytes[i]=(value.integer>>(i*8)) as u8;}
        }
        AmlValueKind::String|AmlValueKind::Buffer=>{
            let source_length=checked_byte_length(value)?;
            let n=core::cmp::min(length,source_length);
            out.bytes[..n].copy_from_slice(&value.bytes[..n]);
        }
        _=>return Err(RuntimeError::TypeMismatch),
    }
    Ok(out)
}

/// Convert a scalar integer to a NUL-terminated decimal AML String.
pub fn integer_to_string(value:u64)->AmlRuntimeValue{
    let mut out=AmlRuntimeValue::EMPTY;out.kind=AmlValueKind::String;
    let mut digits=[0u8;20];let mut n=0usize;let mut v=value;
    if v==0{digits[0]=b'0';n=1}else{while v!=0{digits[n]=b'0'+(v%10) as u8;n+=1;v/=10;}}
    for i in 0..n{out.bytes[i]=digits[n-1-i];}
    out.byte_length=n as u16;out
}

#[cfg(test)]
mod tests{
    use super::*;
    use crate::aml::load_definition_block;
    #[test]fn alias_name_reference_reads_and_stores_canonical_binding(){
        let aml=[0x08,b'V',b'A',b'L',b'0',0x01,0x06,b'V',b'A',b'L',b'0',b'A',b'L',b'I',b'0'];
        let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
        let val=ns.find_child(0,*b"VAL0").unwrap();let alias=ns.find_child(0,*b"ALI0").unwrap();
        let mut arena=AmlObjectArena::new();let mut one=AmlRuntimeValue::EMPTY;one.kind=AmlValueKind::Integer;one.integer=1;
        let one_id=arena.allocate(one).unwrap();let mut bindings=AmlBindings::new();bindings.bind(&ns,val,one_id).unwrap();
        let r=bindings.reference_node(&ns,alias).unwrap();
        assert_eq!(dereference(r,&bindings,&arena).unwrap().integer,1);
        let mut two=one;two.integer=2;let two_id=arena.allocate(two).unwrap();
        store_reference(r,two_id,&mut bindings,&ns,&mut arena).unwrap();
        assert_eq!(dereference(r,&bindings,&arena).unwrap().integer,2);
    }
    #[test]fn package_and_buffer_index_references_support_store(){
        let mut arena=AmlObjectArena::new();let mut p=AmlRuntimeValue::EMPTY;p.kind=AmlValueKind::Package;p.element_count=1;
        let mut initial=AmlRuntimeValue::EMPTY;initial.kind=AmlValueKind::Integer;initial.integer=3;
        let initial_id=arena.allocate(initial).unwrap();p.elements[0]=initial_id;let package=arena.allocate(p).unwrap();
        let r=index_reference(&arena,package,0).unwrap();
        let mut replacement=initial;replacement.integer=9;let replacement_id=arena.allocate(replacement).unwrap();
        let mut bindings=AmlBindings::new();let ns=AmlNamespace::new();
        store_reference(r,replacement_id,&mut bindings,&ns,&mut arena).unwrap();
        assert_eq!(dereference(r,&bindings,&arena).unwrap().integer,9);
        let mut b=AmlRuntimeValue::EMPTY;b.kind=AmlValueKind::Buffer;b.byte_length=1;b.bytes[0]=0xaa;
        let buffer=arena.allocate(b).unwrap();let br=index_reference(&arena,buffer,0).unwrap();
        let mut byte=AmlRuntimeValue::EMPTY;byte.kind=AmlValueKind::Integer;byte.integer=0x55;
        let byte_id=arena.allocate(byte).unwrap();store_reference(br,byte_id,&mut bindings,&ns,&mut arena).unwrap();
        assert_eq!(dereference(br,&bindings,&arena).unwrap().integer,0x55);
    }
    #[test]fn buffer_integer_conversion_is_little_endian_and_width_bounded(){
        let mut buffer=AmlRuntimeValue::EMPTY;buffer.kind=AmlValueKind::Buffer;
        buffer.byte_length=8;buffer.bytes[..8].copy_from_slice(&[0x01,0x23,0x45,0x67,0x89,0xab,0xcd,0xef]);
        assert_eq!(to_integer(&buffer),Ok(0xefcd_ab8967452301));
        buffer.byte_length=9;
        assert_eq!(to_integer(&buffer),Err(RuntimeError::ValueTooLarge));
    }
    #[test]fn buffer_byte_store_rejects_wrong_types_without_mutating_target(){
        let mut arena=AmlObjectArena::new();
        let mut buffer=AmlRuntimeValue::EMPTY;buffer.kind=AmlValueKind::Buffer;buffer.byte_length=1;buffer.bytes[0]=0xaa;
        let buffer_id=arena.allocate(buffer).unwrap();
        let mut string=AmlRuntimeValue::EMPTY;string.kind=AmlValueKind::String;string.byte_length=1;string.bytes[0]=b'1';
        let string_id=arena.allocate(string).unwrap();
        let mut wide=AmlRuntimeValue::EMPTY;wide.kind=AmlValueKind::Integer;wide.integer=0x100;
        let wide_id=arena.allocate(wide).unwrap();
        let reference=AmlReference::BufferByte{buffer:buffer_id,index:0};
        let namespace=AmlNamespace::new();let mut bindings=AmlBindings::new();
        assert_eq!(store_reference(reference,string_id,&mut bindings,&namespace,&mut arena),Err(RuntimeError::TypeMismatch));
        assert_eq!(arena.get(buffer_id).unwrap().bytes[0],0xaa);
        assert_eq!(store_reference(reference,wide_id,&mut bindings,&namespace,&mut arena),Err(RuntimeError::TypeMismatch));
        assert_eq!(arena.get(buffer_id).unwrap().bytes[0],0xaa);
        let mut byte=AmlRuntimeValue::EMPTY;byte.kind=AmlValueKind::Integer;byte.integer=0x55;
        let byte_id=arena.allocate(byte).unwrap();
        assert_eq!(store_reference(reference,byte_id,&mut bindings,&namespace,&mut arena),Ok(()));
        assert_eq!(arena.get(buffer_id).unwrap().bytes[0],0x55);
    }
    #[test]fn integer_to_buffer_uses_little_endian_and_fixed_zero_filled_length(){
        let mut integer=AmlRuntimeValue::EMPTY;integer.kind=AmlValueKind::Integer;integer.integer=0x1234;
        let bytes=to_buffer(&integer,4).unwrap();
        assert_eq!(bytes.kind,AmlValueKind::Buffer);
        assert_eq!(bytes.byte_length,4);
        assert_eq!(&bytes.bytes[..4],&[0x34,0x12,0,0]);
        assert_eq!(to_buffer(&integer,MAX_AML_BYTES+1),Err(RuntimeError::ValueTooLarge));
    }

    #[test]fn conversions_are_bounded_and_checked(){
        let mut s=AmlRuntimeValue::EMPTY;s.kind=AmlValueKind::String;s.bytes[..4].copy_from_slice(b"1a2f");s.byte_length=4;
        assert_eq!(to_integer(&s),Ok(0x1a2f));
        let b=to_buffer(&s,6).unwrap();assert_eq!(&b.bytes[..6],b"1a2f\0\0");
        assert_eq!(integer_to_string(120).bytes[..3],*b"120");
        let mut bad=s;bad.bytes[1]=b'x';assert_eq!(to_integer(&bad),Err(RuntimeError::InvalidIntegerString));
    }
    #[test]fn malformed_public_byte_lengths_are_rejected_without_panics(){
        let mut malformed=AmlRuntimeValue::EMPTY;
        malformed.kind=AmlValueKind::Buffer;
        malformed.byte_length=(MAX_AML_BYTES+1) as u16;
        assert_eq!(to_integer(&malformed),Err(RuntimeError::InvalidObjectLength));
        assert_eq!(to_buffer(&malformed,4),Err(RuntimeError::InvalidObjectLength));
        let mut arena=AmlObjectArena::new();
        let id=arena.allocate(malformed).unwrap();
        assert_eq!(index_reference(&arena,id,0),Err(RuntimeError::InvalidObjectLength));
    }

    #[test]fn hexadecimal_string_conversion_rejects_empty_and_embedded_nul(){
        let mut string=AmlRuntimeValue::EMPTY;string.kind=AmlValueKind::String;
        assert_eq!(to_integer(&string),Err(RuntimeError::InvalidIntegerString));
        string.byte_length=3;string.bytes[..3].copy_from_slice(&[b'1',0,b'2']);
        assert_eq!(to_integer(&string),Err(RuntimeError::InvalidIntegerString));
    }

    #[test]fn hexadecimal_string_conversion_detects_u64_overflow(){
        let mut string=AmlRuntimeValue::EMPTY;string.kind=AmlValueKind::String;
        string.byte_length=17;string.bytes[..17].fill(b'f');
        assert_eq!(to_integer(&string),Err(RuntimeError::ValueTooLarge));
    }

    #[test]fn malformed_package_element_counts_are_rejected_without_indexing(){
        let mut package=AmlRuntimeValue::EMPTY;package.kind=AmlValueKind::Package;
        package.element_count=(crate::aml_objects::MAX_PACKAGE_ELEMENTS+1) as u8;
        let mut arena=AmlObjectArena::new();let id=arena.allocate(package).unwrap();
        assert_eq!(index_reference(&arena,id,0),Err(RuntimeError::InvalidObjectLength));
        let reference=AmlReference::PackageElement{package:id,index:0};
        let bindings=AmlBindings::new();
        assert_eq!(dereference(reference,&bindings,&arena),Err(RuntimeError::InvalidReference));
    }

}
