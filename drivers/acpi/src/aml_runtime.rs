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
    TypeMismatch,ValueTooLarge,EmptyBuffer,InvalidObjectLength,InvalidIntegerString,Arena(ObjectError),
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

/// Validate the internal extents and references of a public runtime value.
pub fn validate_runtime_value(value:&AmlRuntimeValue,arena:&AmlObjectArena)->Result<(),RuntimeError>{
    match value.kind{
        AmlValueKind::String|AmlValueKind::Buffer=>{checked_byte_length(value)?;}
        AmlValueKind::Package=>{
            let count=checked_element_count(value)?;
            for child in value.elements.iter().take(count){
                if arena.get(*child).is_none(){return Err(RuntimeError::InvalidReference)}
            }
        }
        AmlValueKind::ObjectReference=>{
            match reference_from_value(value)?{
                AmlReference::ArenaValue(value)=>{ if arena.get(value).filter(|object|object.kind!=AmlValueKind::Empty).is_none(){return Err(RuntimeError::InvalidReference)} }
                AmlReference::NamespaceNode(node)=>{
                    if node as usize>=MAX_NAMESPACE_NODES{return Err(RuntimeError::InvalidReference)}
                }
                AmlReference::NamespaceNodeGeneration{node,..}=>{
                    if node as usize>=MAX_NAMESPACE_NODES{return Err(RuntimeError::InvalidReference)}
                }
                AmlReference::PackageElement{package,index}=>{
                    let object=arena.get(package).ok_or(RuntimeError::InvalidReference)?;
                    if object.kind!=AmlValueKind::Package||index as usize>=checked_element_count(object)?{
                        return Err(RuntimeError::InvalidReference)
                    }
                }
                AmlReference::BufferByte{buffer,index}=>{
                    let object=arena.get(buffer).ok_or(RuntimeError::InvalidReference)?;
                    if object.kind!=AmlValueKind::Buffer||index as usize>=checked_byte_length(object)?{
                        return Err(RuntimeError::InvalidReference)
                    }
                }
                AmlReference::BufferField{buffer,bit_offset,bit_width}=>{
                    checked_buffer_field(arena,buffer,bit_offset,bit_width)?;
                }
            }
        }
        _=>{}
    }
    Ok(())
}

impl From<ObjectError> for RuntimeError { fn from(e:ObjectError)->Self{Self::Arena(e)} }
/// Deep-copy an AML runtime value tree into the caller-owned arena. Package
/// elements are copied recursively so a Store/CopyObject does not accidentally
/// make nested package contents alias the source package.
fn clone_value_tree_inner(value_id:u16,arena:&mut AmlObjectArena,
    path:&mut [u16;32],depth:usize)->Result<u16,RuntimeError>{
    if depth>=path.len(){return Err(RuntimeError::InvalidObjectLength)}
    if path[..depth].contains(&value_id){return Err(RuntimeError::InvalidReference)}
    let source=*arena.get(value_id).ok_or(RuntimeError::InvalidReference)?;
    validate_runtime_value(&source,arena)?;
    path[depth]=value_id;
    let mut copy=source;
    if source.kind==AmlValueKind::Package{
        let count=checked_element_count(&source)?;
        for index in 0..count{
            copy.elements[index]=clone_value_tree_inner(source.elements[index],arena,path,depth+1)?;
        }
    }
    arena.allocate(copy).map_err(RuntimeError::from)
}

/// Clone transactionally: a malformed/cyclic package or full arena leaves the
/// arena exactly as it was before the attempted copy.
/// Deep-copy the contents of a Package into an existing Package object,
/// preserving the target object's identity for outstanding PackageElement
/// references. The temporary cloned root is discarded after its children
/// have been installed; failures leave both target and arena unchanged.
fn clone_package_into_existing(source_id:u16,target_id:u16,
    arena:&mut AmlObjectArena)->Result<(),RuntimeError>{
    let target=*arena.get(target_id).ok_or(RuntimeError::InvalidReference)?;
    if target.kind!=AmlValueKind::Package{return Err(RuntimeError::TypeMismatch)}
    let cloned_id=clone_value_tree(source_id,arena)?;
    let cloned=*arena.get(cloned_id).ok_or(RuntimeError::InvalidReference)?;
    if cloned.kind!=AmlValueKind::Package||cloned_id as usize+1!=arena.count{
        return Err(RuntimeError::InvalidReference)
    }
    // clone_value_tree appends the root after every recursively copied child.
    arena.count-=1;
    *arena.get_mut(target_id).ok_or(RuntimeError::InvalidReference)?=cloned;
    Ok(())
}

fn clone_value_tree(value_id:u16,arena:&mut AmlObjectArena)->Result<u16,RuntimeError>{
    let checkpoint=arena.count;
    let mut path=[u16::MAX;32];
    match clone_value_tree_inner(value_id,arena,&mut path,0){
        Ok(id)=>Ok(id),
        Err(error)=>{arena.count=checkpoint;Err(error)}
    }
}


/// Encode a runtime reference into the compact arena-safe object value.
pub fn make_object_reference(reference:AmlReference)->AmlRuntimeValue{
    let mut value=AmlRuntimeValue::EMPTY;
    value.kind=AmlValueKind::ObjectReference;
    match reference{
        AmlReference::NamespaceNode(node)=>{value.reference_kind=0;value.reference_id=node;}
        AmlReference::NamespaceNodeGeneration{node,generation}=>{
            value.reference_kind=5;value.reference_id=node;
            value.reference_index=generation as u16;
            value.reference_bit_width=(generation>>16) as u16;
        }
        AmlReference::ArenaValue(id)=>{value.reference_kind=4;value.reference_id=id;}
        AmlReference::PackageElement{package,index}=>{value.reference_kind=1;value.reference_id=package;value.reference_index=index as u16;}
        AmlReference::BufferByte{buffer,index}=>{value.reference_kind=2;value.reference_id=buffer;value.reference_index=index;}
        AmlReference::BufferField{buffer,bit_offset,bit_width}=>{
            value.reference_kind=3;value.reference_id=buffer;
            value.reference_index=bit_offset;value.reference_bit_width=bit_width;
        }
    }
    value
}

/// Decode a compact ObjectReference value into a checked runtime target.
pub fn reference_from_value(value:&AmlRuntimeValue)->Result<AmlReference,RuntimeError>{
    if value.kind!=AmlValueKind::ObjectReference{return Err(RuntimeError::TypeMismatch)}
    match value.reference_kind{
        0=>Ok(AmlReference::NamespaceNode(value.reference_id)),
        5=>Ok(AmlReference::NamespaceNodeGeneration{node:value.reference_id,
            generation:(value.reference_index as u32)|((value.reference_bit_width as u32)<<16)}),
        4=>Ok(AmlReference::ArenaValue(value.reference_id)),
        1=>{
            if value.reference_index>u8::MAX as u16{return Err(RuntimeError::InvalidReference)}
            Ok(AmlReference::PackageElement{package:value.reference_id,index:value.reference_index as u8})
        }
        2=>Ok(AmlReference::BufferByte{buffer:value.reference_id,index:value.reference_index}),
        3=>Ok(AmlReference::BufferField{buffer:value.reference_id,
            bit_offset:value.reference_index,bit_width:value.reference_bit_width}),
        _=>Err(RuntimeError::InvalidReference),
    }
}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum AmlReference {
    NamespaceNode(u16),
    /// Namespace reference tied to the current node incarnation. The generation
    /// prevents a stale reference from resolving after table unload/reload and
    /// namespace compaction reuses the same 16-bit node index.
    NamespaceNodeGeneration{node:u16,generation:u32},
    /// Stable reference to a caller-owned arena object promoted from Local/Arg.
    ArenaValue(u16),
    PackageElement { package:u16,index:u8 },
    BufferByte { buffer:u16,index:u16 },
    /// A little-endian bit field backed by a runtime Buffer.
    BufferField { buffer:u16,bit_offset:u16,bit_width:u16 },
}

/// Namespace Name objects are bound to arena values independently of the
/// immutable AML parse tree. This table is caller-owned and bounded.
pub struct AmlBindings {
    values:[u16;MAX_NAMESPACE_NODES],
    bound:[bool;MAX_NAMESPACE_NODES],
    node_generations:[u32;MAX_NAMESPACE_NODES],
    integer_width_bits:u8,
}
impl AmlBindings {
    pub const fn new()->Self{Self{values:[0;MAX_NAMESPACE_NODES],bound:[false;MAX_NAMESPACE_NODES],node_generations:[0;MAX_NAMESPACE_NODES],integer_width_bits:64}}
    /// Construct bindings using the Integer width selected by the AML
    /// DefinitionBlock revision (revision 1 is 32-bit; revision 2+ is 64-bit).
    pub const fn with_aml_revision(revision:u8)->Self{
        Self{values:[0;MAX_NAMESPACE_NODES],bound:[false;MAX_NAMESPACE_NODES],node_generations:[0;MAX_NAMESPACE_NODES],
            integer_width_bits:if revision<2{32}else{64}}
    }
    pub const fn integer_width_bits(&self)->u8{self.integer_width_bits}

    /// Invalidate every cached Name binding owned by a DefinitionBlock before
    /// its namespace nodes are removed. Call only after AML execution quiesces.
    pub fn invalidate_table(&mut self, namespace:&AmlNamespace, owner_table:u32)->usize {
        let mut invalidated=0;
        for index in 0..namespace.count {
            if namespace.nodes[index].source_table==owner_table {
                // Invalidate references even when the Name has never been
                // evaluated/bound. Bump before namespace compaction so any
                // saved reference to this node incarnation becomes stale.
                self.node_generations[index]=self.node_generations[index].wrapping_add(1).max(1);
                if self.bound[index] {
                    self.bound[index]=false;
                    self.values[index]=0;
                    invalidated+=1;
                }
            }
        }
        invalidated
    }
    /// Bind only a live, structurally valid runtime value to a Name object.
    pub fn bind_checked(&mut self,namespace:&AmlNamespace,node:u16,value:u16,arena:&AmlObjectArena)->Result<(),RuntimeError>{
        let object=arena.get(value).ok_or(RuntimeError::InvalidReference)?;
        validate_runtime_value(object,arena)?;
        self.bind(namespace,node,value)
    }
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
        Ok(AmlReference::NamespaceNodeGeneration{node:canonical,generation:self.node_generations[canonical as usize]})
    }
    pub fn validate_node_generation(&self,node:u16,generation:u32)->Result<(),RuntimeError>{
        let index=node as usize;
        if index>=MAX_NAMESPACE_NODES||self.node_generations[index]!=generation {
            return Err(RuntimeError::InvalidReference)
        }
        Ok(())
    }
    /// Namespace unload compacts numeric node indices. Since a reference can
    /// outlive the method that created it (for example in a caller-owned Package),
    /// every surviving numeric namespace reference must be retired before compaction.
    /// Clearing all bindings also forces surviving Name objects to be decoded again
    /// at their new indices instead of accidentally inheriting another node's value.
    pub fn invalidate_namespace_compaction(&mut self)->usize {
        let mut invalidated=0usize;
        for index in 0..MAX_NAMESPACE_NODES {
            self.node_generations[index]=self.node_generations[index].wrapping_add(1).max(1);
            if self.bound[index] { self.bound[index]=false; self.values[index]=0; invalidated+=1; }
        }
        invalidated
    }
    pub fn value_id(&self,node:u16)->Result<u16,RuntimeError>{
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

/// Validate a bit-field extent against its backing Buffer and return the
/// backing object plus the field's rounded-up byte length.
fn checked_buffer_field(arena:&AmlObjectArena,buffer:u16,bit_offset:u16,bit_width:u16)
    ->Result<(AmlRuntimeValue,usize),RuntimeError>{
    let target=*arena.get(buffer).ok_or(RuntimeError::InvalidReference)?;
    if target.kind!=AmlValueKind::Buffer||bit_width==0{return Err(RuntimeError::InvalidReference)}
    let bytes=checked_byte_length(&target)?;
    let end=(bit_offset as usize).checked_add(bit_width as usize).ok_or(RuntimeError::InvalidReference)?;
    if end>bytes.checked_mul(8).ok_or(RuntimeError::InvalidReference)?{return Err(RuntimeError::InvalidReference)}
    let field_bytes=(bit_width as usize).checked_add(7).ok_or(RuntimeError::InvalidReference)?/8;
    if field_bytes>MAX_AML_BYTES{return Err(RuntimeError::ValueTooLarge)}
    Ok((target,field_bytes))
}

/// Read a little-endian BufferField. Fields no wider than the current AML
/// Integer are represented as Integer; wider fields are returned as Buffer.
fn read_buffer_field(arena:&AmlObjectArena,buffer:u16,bit_offset:u16,bit_width:u16,
    integer_width_bits:u8)->Result<AmlRuntimeValue,RuntimeError>{
    let (target,field_bytes)=checked_buffer_field(arena,buffer,bit_offset,bit_width)?;
    let mut result=AmlRuntimeValue::EMPTY;
    if bit_width<=integer_width_bits as u16{
        result.kind=AmlValueKind::Integer;
        for bit in 0..bit_width as usize{
            let source_bit=bit_offset as usize+bit;
            if (target.bytes[source_bit/8]>>(source_bit%8))&1!=0{
                result.integer|=1u64<<bit;
            }
        }
    }else{
        result.kind=AmlValueKind::Buffer;result.byte_length=field_bytes as u16;
        for bit in 0..bit_width as usize{
            let source_bit=bit_offset as usize+bit;
            if (target.bytes[source_bit/8]>>(source_bit%8))&1!=0{
                result.bytes[bit/8]|=1u8<<(bit%8);
            }
        }
    }
    Ok(result)
}

/// Write a converted Integer or Buffer into a BufferField, preserving every
/// backing-buffer bit outside the field extent.
fn write_buffer_field(arena:&mut AmlObjectArena,buffer:u16,bit_offset:u16,bit_width:u16,
    source:&AmlRuntimeValue)->Result<(),RuntimeError>{
    let (_,field_bytes)=checked_buffer_field(arena,buffer,bit_offset,bit_width)?;
    if !matches!(source.kind,AmlValueKind::Integer|AmlValueKind::Buffer){return Err(RuntimeError::TypeMismatch)}
    if source.kind==AmlValueKind::Buffer{checked_byte_length(source)?;}
    let target=arena.get_mut(buffer).ok_or(RuntimeError::InvalidReference)?;
    for bit in 0..bit_width as usize{
        let value=if source.kind==AmlValueKind::Integer{
            if bit<64{((source.integer>>bit)&1) as u8}else{0}
        }else if bit/8<field_bytes{
            (source.bytes[bit/8]>>(bit%8))&1
        }else{0};
        let target_bit=bit_offset as usize+bit;
        let mask=1u8<<(target_bit%8);
        if value!=0{target.bytes[target_bit/8]|=mask}else{target.bytes[target_bit/8]&=!mask}
    }
    Ok(())
}

/// Dereference a namespace, package-element, or buffer-byte reference.
pub fn dereference(reference:AmlReference,bindings:&AmlBindings,arena:&AmlObjectArena)
    ->Result<AmlRuntimeValue,RuntimeError>{
    match reference{
        AmlReference::NamespaceNode(node)=>Ok(*arena.get(bindings.value_id(node)?).ok_or(RuntimeError::InvalidReference)?),
        AmlReference::NamespaceNodeGeneration{node,generation}=>{
            bindings.validate_node_generation(node,generation)?;
            Ok(*arena.get(bindings.value_id(node)?).ok_or(RuntimeError::InvalidReference)?)
        }
        AmlReference::ArenaValue(id)=>{let value=*arena.get(id).ok_or(RuntimeError::InvalidReference)?;if value.kind==AmlValueKind::Empty{return Err(RuntimeError::InvalidReference)}Ok(value)},
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
        AmlReference::BufferField{buffer,bit_offset,bit_width}=>{
            read_buffer_field(arena,buffer,bit_offset,bit_width,bindings.integer_width_bits)
        }
    }
}

/// Selects the AML result-store behavior. Store applies the target's fixed
/// data type; CopyObject replaces the target value without implicit conversion.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum StoreMode { Store, CopyObject }

/// AML implicit String-to-Integer conversion is deliberately distinct from
/// the explicit ToInteger operator: it parses hexadecimal digits without a
/// 0x prefix and stops at the first non-hexadecimal character.
fn implicit_string_to_integer(value:&AmlRuntimeValue,width_bits:u8)->Result<u64,RuntimeError>{
    let length=checked_byte_length(value)?;
    if length==0{return Err(RuntimeError::InvalidIntegerString)}
    let mut result=0u64;
    let mut digits=0usize;
    let max_digits=(width_bits/4) as usize;
    for &ch in &value.bytes[..length]{
        let digit=match ch{
            b'0'..=b'9'=>(ch-b'0') as u64,
            b'a'..=b'f'=>(ch-b'a'+10) as u64,
            b'A'..=b'F'=>(ch-b'A'+10) as u64,
            _=>break,
        };
        if digits==max_digits{break}
        result=result.checked_mul(16).and_then(|v|v.checked_add(digit)).ok_or(RuntimeError::ValueTooLarge)?;
        digits+=1;
    }
    if digits==0{return Err(RuntimeError::InvalidIntegerString)}
    Ok(result)
}

fn integer_to_hex_string(value:u64,width_bits:u8)->AmlRuntimeValue{
    const HEX:&[u8;16]=b"0123456789ABCDEF";
    let digits=(width_bits/4) as usize;
    let mut out=AmlRuntimeValue::EMPTY;
    out.kind=AmlValueKind::String;
    for i in 0..digits{out.bytes[i]=HEX[((value >> ((digits-1-i)*4))&0xf) as usize];}
    out.byte_length=digits as u16;
    out
}

/// Convert a source value to the already-established type of a named object.
/// This covers only runtime scalar/container kinds; field-unit and reference
/// conversions require their own execution context and are not guessed here.
fn convert_to_target_type(source:&AmlRuntimeValue,target:&AmlRuntimeValue,arena:&AmlObjectArena,width_bits:u8,target_exists:bool)
    ->Result<AmlRuntimeValue,RuntimeError>{
    let mut converted=AmlRuntimeValue::EMPTY;
    match target.kind{
        AmlValueKind::Integer=>{
            converted.kind=AmlValueKind::Integer;
            converted.integer=match source.kind{
                AmlValueKind::Integer=>source.integer,
                AmlValueKind::Buffer=>{
                    let length=checked_byte_length(source)?;
                    if length==0{return Err(RuntimeError::EmptyBuffer)}
                    let mut integer=0u64;
                    for i in 0..core::cmp::min(length,(width_bits/8) as usize){
                        integer|=(source.bytes[i] as u64)<<(i*8);
                    }
                    integer
                }
                AmlValueKind::String=>implicit_string_to_integer(source,width_bits)?,
                _=>return Err(RuntimeError::TypeMismatch),
            };
        }
        AmlValueKind::Buffer=>{
            converted.kind=AmlValueKind::Buffer;
            match source.kind{
                AmlValueKind::Integer=>{
                    let width_bytes=(width_bits/8) as usize;
                    let target_length=checked_byte_length(target)?;
                    let bytes=if target_exists{target_length}else{width_bytes};
                    converted.byte_length=bytes as u16;
                    for i in 0..core::cmp::min(bytes,width_bytes){
                        converted.bytes[i]=(source.integer>>(i*8)) as u8;
                    }
                }
                AmlValueKind::String=>{
                    let n=checked_byte_length(source)?;
                    let target_length=checked_byte_length(target)?;
                    let required=n.checked_add(1).ok_or(RuntimeError::ValueTooLarge)?;
                    let bytes=if target_exists{target_length}else{required};
                    converted.byte_length=bytes as u16;
                    let copied=core::cmp::min(n,bytes);
                    converted.bytes[..copied].copy_from_slice(&source.bytes[..copied]);
                    // The source string's implicit terminator is copied only
                    // when it fits; any remaining target bytes stay zero.
                    if n<bytes{converted.bytes[n]=0;}
                }
                AmlValueKind::Buffer=>{
                    let n=checked_byte_length(source)?;
                    let target_length=checked_byte_length(target)?;
                    // An existing Buffer retains its declared extent,
                    // including a legitimate zero-length Buffer. Only an
                    // absent target adopts the source extent.
                    let bytes=if target_exists{target_length}else{n};
                    converted.byte_length=bytes as u16;
                    let copied=core::cmp::min(n,bytes);
                    converted.bytes[..copied].copy_from_slice(&source.bytes[..copied]);
                    // EMPTY initialization zero-extends a shorter source.
                }
                _=>return Err(RuntimeError::TypeMismatch),
            }
        }
        AmlValueKind::String=>{
            match source.kind{
                AmlValueKind::Integer=>converted=integer_to_hex_string(source.integer,width_bits),
                AmlValueKind::String=>{
                    let n=checked_byte_length(source)?;
                    converted.kind=AmlValueKind::String;
                    converted.byte_length=n as u16;
                    converted.bytes[..n].copy_from_slice(&source.bytes[..n]);
                }
                AmlValueKind::Buffer=>{
                    let n=checked_byte_length(source)?;
                    // ACPI 6.6 implicit Buffer-to-String conversion
                    // emits two hexadecimal digits per byte, separated by
                    // one space; a zero-length buffer becomes an empty string.
                    let output_len=if n==0{0}else{n.checked_mul(3).and_then(|v|v.checked_sub(1)).ok_or(RuntimeError::ValueTooLarge)?};
                    if output_len>MAX_AML_BYTES{return Err(RuntimeError::ValueTooLarge)}
                    const HEX:&[u8;16]=b"0123456789ABCDEF";
                    converted.kind=AmlValueKind::String;
                    converted.byte_length=output_len as u16;
                    for i in 0..n{
                        if i!=0{converted.bytes[i*3-1]=b' ';}
                        converted.bytes[i*3]=HEX[(source.bytes[i]>>4) as usize];
                        converted.bytes[i*3+1]=HEX[(source.bytes[i]&0xf) as usize];
                    }
                }
                _=>return Err(RuntimeError::TypeMismatch),
            }
        }
        AmlValueKind::Package=>{
            if source.kind!=AmlValueKind::Package{return Err(RuntimeError::TypeMismatch)}
            validate_runtime_value(source,arena)?;
            converted=*source;
        }
        AmlValueKind::Uninitialized=>{
            converted=*source;
        }
        _=>return Err(RuntimeError::TypeMismatch),
    }
    validate_runtime_value(&converted,arena)?;
    Ok(converted)
}

/// Apply implicit source conversion when an AML operator requires Integer.
/// Unlike explicit ToInteger, String operands are hexadecimal without a 0x
/// prefix and terminate at the first non-hexadecimal character.
pub fn to_integer_implicit(value:&AmlRuntimeValue,width_bits:u8)->Result<u64,RuntimeError>{
    if !matches!(width_bits,32|64){return Err(RuntimeError::TypeMismatch)}
    match value.kind{
        AmlValueKind::Integer=>{
            let mask=if width_bits==32{u32::MAX as u64}else{u64::MAX};
            Ok(value.integer&mask)
        }
        AmlValueKind::Buffer=>{
            let length=checked_byte_length(value)?;
            if length==0{return Err(RuntimeError::EmptyBuffer)}
            let mut result=0u64;
            for i in 0..core::cmp::min(length,(width_bits/8) as usize){
                result|=(value.bytes[i] as u64)<<(i*8);
            }
            Ok(result)
        }
        AmlValueKind::String=>implicit_string_to_integer(value,width_bits),
        _=>Err(RuntimeError::TypeMismatch),
    }
}

/// Convert an AML source operand to the data type required by an operator.
/// This is intentionally separate from explicit ToInteger: implicit
/// String-to-Integer conversion uses unprefixed hexadecimal text, while
/// ToInteger follows its explicit decimal-or-0x-prefixed syntax.
pub fn convert_source_operand(value:&AmlRuntimeValue,expected_kind:AmlValueKind,
    integer_width_bits:u8,arena:&AmlObjectArena)->Result<AmlRuntimeValue,RuntimeError>{
    validate_runtime_value(value,arena)?;
    if value.kind==expected_kind{return Ok(*value)}
    if !matches!(integer_width_bits,32|64){return Err(RuntimeError::TypeMismatch)}
    let mut target=AmlRuntimeValue::EMPTY;
    target.kind=expected_kind;
    convert_to_target_type(value,&target,arena,integer_width_bits,false)
}

/// Store an existing arena value through an AML reference. Named objects use
/// implicit result conversion; CopyObject bypasses it. Package elements retain
/// their dynamic value type. Buffer-byte writes truncate to the field width.
pub fn store_reference(reference:AmlReference,value:u16,bindings:&mut AmlBindings,
    namespace:&AmlNamespace,arena:&mut AmlObjectArena)->Result<(),RuntimeError>{
    store_reference_with_mode(reference,value,StoreMode::Store,bindings,namespace,arena)
}

pub fn store_reference_with_mode(reference:AmlReference,value:u16,mode:StoreMode,
    bindings:&mut AmlBindings,namespace:&AmlNamespace,arena:&mut AmlObjectArena)->Result<(),RuntimeError>{
    let source=*arena.get(value).ok_or(RuntimeError::InvalidReference)?;
    validate_runtime_value(&source,arena)?;
    match reference{
        AmlReference::ArenaValue(id)=>{
            let target=*arena.get(id).filter(|object|object.kind!=AmlValueKind::Empty).ok_or(RuntimeError::InvalidReference)?;
            let stored=if mode==StoreMode::CopyObject{source}else{
                convert_to_target_type(&source,&target,arena,bindings.integer_width_bits,true)?
            };
            let stored_id=if stored.kind==AmlValueKind::Package{
                clone_value_tree(value,arena)?
            }else{arena.allocate(stored)?};
            let replacement=*arena.get(stored_id).ok_or(RuntimeError::InvalidReference)?;
            *arena.get_mut(id).ok_or(RuntimeError::InvalidReference)?=replacement;
            Ok(())
        }
        AmlReference::NamespaceNodeGeneration{node,generation}=>{
            bindings.validate_node_generation(node,generation)?;
            store_reference_with_mode(AmlReference::NamespaceNode(node),value,mode,bindings,namespace,arena)
        }
        AmlReference::NamespaceNode(node)=>{
            let canonical=namespace.resolve_alias(node).map_err(|_|RuntimeError::InvalidNode)?;
            let i=canonical as usize;
            if i>=namespace.count||namespace.nodes[i].object_type!=AmlObjectType::Name{return Err(RuntimeError::TypeMismatch)}
            let old_id=if bindings.bound[i]{Some(bindings.values[i])}else{None};
            let old=if let Some(id)=old_id{Some(*arena.get(id).ok_or(RuntimeError::InvalidReference)?)}else{None};
            let stored=if mode==StoreMode::CopyObject{
                source
            }else if let Some(target)=old{
                convert_to_target_type(&source,&target,arena,bindings.integer_width_bits,true)?
            }else{
                // A Name without an evaluated initializer has no established
                // fixed target type yet; first assignment establishes it.
                source
            };
            if mode==StoreMode::Store&&stored.kind==AmlValueKind::Package
                &&old.map(|target|target.kind==AmlValueKind::Package).unwrap_or(false){
                clone_package_into_existing(value,old_id.ok_or(RuntimeError::InvalidReference)?,arena)?;
                return Ok(())
            }
            // Named objects own their stored value; mutable data never aliases
            // the caller's source object.
            let stored_id=if stored.kind==AmlValueKind::Package{
                clone_value_tree(value,arena)?
            }else{arena.allocate(stored)?};
            bindings.values[i]=stored_id;bindings.bound[i]=true;Ok(())
        }
        AmlReference::PackageElement{package,index}=>{
            let p=arena.get(package).ok_or(RuntimeError::InvalidReference)?;
            if p.kind!=AmlValueKind::Package||index as usize>=checked_element_count(p)?{return Err(RuntimeError::InvalidReference)}
            // Package elements are dynamically typed targets. Store and
            // CopyObject both replace the element with an owned value copy.
            let stored_id=clone_value_tree(value,arena)?;
            let p=arena.get_mut(package).ok_or(RuntimeError::InvalidReference)?;
            p.elements[index as usize]=stored_id;Ok(())
        }
        AmlReference::BufferField{buffer,bit_offset,bit_width}=>{
            let (_,field_bytes)=checked_buffer_field(arena,buffer,bit_offset,bit_width)?;
            let converted=if mode==StoreMode::CopyObject{
                // CopyObject does not coerce a field source: only Integer or
                // Buffer objects may be written to a field target.
                if !matches!(source.kind,AmlValueKind::Integer|AmlValueKind::Buffer){
                    return Err(RuntimeError::TypeMismatch)
                }
                source
            }else if bit_width<=bindings.integer_width_bits as u16{
                convert_source_operand(&source,AmlValueKind::Integer,
                    bindings.integer_width_bits,arena)?
            }else{
                let mut target=AmlRuntimeValue::EMPTY;
                target.kind=AmlValueKind::Buffer;target.byte_length=field_bytes as u16;
                convert_to_target_type(&source,&target,arena,bindings.integer_width_bits,true)?
            };
            write_buffer_field(arena,buffer,bit_offset,bit_width,&converted)
        }
        AmlReference::BufferByte{buffer,index}=>{
            // Validate the complete target and source before taking a mutable
            // reference, so every rejected store leaves the destination intact.
            let target=arena.get(buffer).ok_or(RuntimeError::InvalidReference)?;
            if target.kind!=AmlValueKind::Buffer{return Err(RuntimeError::InvalidReference)}
            let length=checked_byte_length(target)?;
            if index as usize>=length{return Err(RuntimeError::InvalidReference)}
            let byte=match (mode,source.kind){
                (StoreMode::Store,AmlValueKind::Integer)|(StoreMode::CopyObject,AmlValueKind::Integer)=>source.integer as u8,
                (StoreMode::Store,AmlValueKind::Buffer)|(StoreMode::CopyObject,AmlValueKind::Buffer)=>{
                    if checked_byte_length(&source)?==0{0}else{source.bytes[0]}
                }
                (StoreMode::Store,AmlValueKind::String)=>{
                    if checked_byte_length(&source)?==0{0}else{source.bytes[0]}
                }
                // CopyObject does not apply String-to-BufferField conversion.
                _=>return Err(RuntimeError::TypeMismatch),
            };
            let target=arena.get_mut(buffer).ok_or(RuntimeError::InvalidReference)?;
            target.bytes[index as usize]=byte;Ok(())
        }
    }
}

/// AML method result target. LocalX and ArgX slots replace their stored
/// object without implicit result conversion; reference targets use the
/// fixed-type Store/CopyObject rules implemented by the runtime.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum AmlTarget { Null,Local(u8),Argument(u8),Reference(AmlReference) }

/// Per-invocation typed Local/Arg storage. Values live in the caller-owned
/// arena, so Package trees can be copied without heap allocation.
pub struct AmlMethodFrame {
    locals:[u16;8],
    arguments:[u16;7],
    local_bound:[bool;8],
    argument_bound:[bool;7],
}
impl AmlMethodFrame {
    /// Create a fresh frame. All LocalX values begin Uninitialized; arguments
    /// are copied into the frame and must be live runtime arena objects.
    pub fn new(arguments:&[u16],arena:&mut AmlObjectArena)->Result<Self,RuntimeError>{
        if arguments.len()>7{return Err(RuntimeError::InvalidReference)}
        // Validate argument handles before reserving frame storage, then roll
        // back every allocation if any deep copy cannot be completed.
        for id in arguments{
            let value=arena.get(*id).ok_or(RuntimeError::InvalidReference)?;
            validate_runtime_value(value,arena)?;
        }
        let checkpoint=arena.count;
        let result=(||{
            let mut uninitialized=AmlRuntimeValue::EMPTY;
            uninitialized.kind=AmlValueKind::Uninitialized;
            let empty=arena.allocate(uninitialized)?;
            let mut frame=Self{
                locals:[empty;8],arguments:[empty;7],
                local_bound:[true;8],argument_bound:[false;7],
            };
            for (index,id) in arguments.iter().copied().enumerate(){
                frame.arguments[index]=clone_value_tree(id,arena)?;
                frame.argument_bound[index]=true;
            }
            Ok(frame)
        })();
        if result.is_err(){arena.count=checkpoint;}
        result
    }

    /// Read a typed LocalX or ArgX object. An unused ArgX is rejected.
    pub fn read(&self,target:AmlTarget,bindings:&AmlBindings,
        namespace:&AmlNamespace,arena:&AmlObjectArena)->Result<AmlRuntimeValue,RuntimeError>{
        let id=match target{
            AmlTarget::Local(index)=>{
                let slot=index as usize;
                if slot>=8||!self.local_bound[slot]{return Err(RuntimeError::InvalidReference)}
                self.locals[slot]
            }
            AmlTarget::Argument(index)=>{
                let slot=index as usize;
                if slot>=7||!self.argument_bound[slot]{return Err(RuntimeError::InvalidReference)}
                let value=*arena.get(self.arguments[slot]).ok_or(RuntimeError::InvalidReference)?;
                if value.kind==AmlValueKind::ObjectReference{
                    return dereference(reference_from_value(&value)?,bindings,arena)
                }
                return Ok(value)
            }
            AmlTarget::Reference(reference)=>return dereference(reference,bindings,arena),
            AmlTarget::Null=>return Err(RuntimeError::InvalidReference),
        };
        Ok(*arena.get(id).ok_or(RuntimeError::InvalidReference)?)
    }

    /// Store a value into a method target. LocalX/ArgX replace the complete
    /// value with no implicit conversion; named/field references use the
    /// reference-specific rules. ArgX object-reference dereference is handled
    /// by the caller before invoking this API.
    pub fn store(&mut self,target:AmlTarget,value:u16,mode:StoreMode,
        bindings:&mut AmlBindings,namespace:&AmlNamespace,
        arena:&mut AmlObjectArena)->Result<(),RuntimeError>{
        let source=*arena.get(value).ok_or(RuntimeError::InvalidReference)?;
        validate_runtime_value(&source,arena)?;
        match target{
            AmlTarget::Null=>Ok(()),
            AmlTarget::Local(index)=>{
                let slot=index as usize;
                if slot>=8{return Err(RuntimeError::InvalidReference)}
                let stored=clone_value_tree(value,arena)?;
                self.locals[slot]=stored;self.local_bound[slot]=true;Ok(())
            }
            AmlTarget::Argument(index)=>{
                let slot=index as usize;
                if slot>=7||!self.argument_bound[slot]{return Err(RuntimeError::InvalidReference)}
                let current=*arena.get(self.arguments[slot]).ok_or(RuntimeError::InvalidReference)?;
                if current.kind==AmlValueKind::ObjectReference{
                    store_reference_with_mode(reference_from_value(&current)?,value,mode,
                        bindings,namespace,arena)
                }else{
                    let stored=clone_value_tree(value,arena)?;
                    self.arguments[slot]=stored;Ok(())
                }
            }
            AmlTarget::Reference(reference)=>
                store_reference_with_mode(reference,value,mode,bindings,namespace,arena),
        }
    }
}

/// AML-style scalar conversion. Integer strings are interpreted as
/// hexadecimal text; buffers are little-endian and limited to eight bytes.
pub fn to_integer(value:&AmlRuntimeValue)->Result<u64,RuntimeError>{
    to_integer_with_width(value,64)
}

/// Explicit ToInteger conversion using the AML DefinitionBlock's integer
/// width. Revision 1 uses 32-bit values; revision 2 and later use 64-bit.
pub fn to_integer_with_width(value:&AmlRuntimeValue,width_bits:u8)->Result<u64,RuntimeError>{
    if !matches!(width_bits,32|64){return Err(RuntimeError::TypeMismatch)}
    let max=if width_bits==32{u32::MAX as u64}else{u64::MAX};
    match value.kind{
        AmlValueKind::Integer=>Ok(value.integer&max),
        AmlValueKind::Buffer=>{
            let length=checked_byte_length(value)?;
            if length==0{return Err(RuntimeError::EmptyBuffer)}
            // Buffer byte zero is the least-significant byte. Excess high
            // bytes are truncated to the AML integer width.
            let mut out=0u64;
            for i in 0..core::cmp::min(length,(width_bits/8) as usize){out|=(value.bytes[i] as u64)<<(i*8)}
            Ok(out)
        }
        AmlValueKind::String=>{
            let length=checked_byte_length(value)?;
            if length==0{return Err(RuntimeError::InvalidIntegerString)}
            let bytes=&value.bytes[..length];
            let (digits,radix)=if bytes.len()>=2&&bytes[0]==b'0'&&(bytes[1]==b'x'||bytes[1]==b'X'){
                (&bytes[2..],16u64)
            }else{(bytes,10u64)};
            if digits.is_empty(){return Err(RuntimeError::InvalidIntegerString)}
            let mut out=0u64;
            for &ch in digits{
                let digit=match ch{
                    b'0'..=b'9'=>(ch-b'0')as u64,
                    b'a'..=b'f' if radix==16=>(ch-b'a'+10)as u64,
                    b'A'..=b'F' if radix==16=>(ch-b'A'+10)as u64,
                    _=>return Err(RuntimeError::InvalidIntegerString),
                };
                if digit>=radix{return Err(RuntimeError::InvalidIntegerString)}
                out=out.checked_mul(radix).and_then(|v|v.checked_add(digit)).ok_or(RuntimeError::ValueTooLarge)?;
                if out>max{return Err(RuntimeError::ValueTooLarge)}
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
    use crate::aml::load_definition_block;
    #[test]fn implicit_integer_source_conversion_uses_hex_and_definition_width(){
        let mut text=AmlRuntimeValue::EMPTY;text.kind=AmlValueKind::String;
        text.byte_length=4;text.bytes[..4].copy_from_slice(b"2aZ9");
        assert_eq!(to_integer_implicit(&text,32),Ok(0x2a));
        text.byte_length=10;text.bytes[..10].copy_from_slice(b"123456789A");
        assert_eq!(to_integer_implicit(&text,32),Ok(0x12345678));
        assert_eq!(to_integer_implicit(&text,64),Ok(0x123456789a));
        let mut buffer=AmlRuntimeValue::EMPTY;buffer.kind=AmlValueKind::Buffer;
        buffer.byte_length=8;buffer.bytes[..8].copy_from_slice(&[1,2,3,4,5,6,7,8]);
        assert_eq!(to_integer_implicit(&buffer,32),Ok(0x04030201));
        assert_eq!(to_integer_implicit(&buffer,64),Ok(0x0807060504030201));
    }

    #[test]fn explicit_to_integer_obeys_definition_block_width(){
        let mut buffer=AmlRuntimeValue::EMPTY;buffer.kind=AmlValueKind::Buffer;
        buffer.byte_length=8;buffer.bytes[..8].copy_from_slice(&[0x78,0x56,0x34,0x12,0xff,0xff,0xff,0xff]);
        assert_eq!(to_integer_with_width(&buffer,32),Ok(0x12345678));
        assert_eq!(to_integer_with_width(&buffer,64),Ok(0xffffffff12345678));
        let mut decimal=AmlRuntimeValue::EMPTY;decimal.kind=AmlValueKind::String;
        decimal.byte_length=10;decimal.bytes[..10].copy_from_slice(b"4294967295");
        assert_eq!(to_integer_with_width(&decimal,32),Ok(u32::MAX as u64));
        decimal.bytes[..10].copy_from_slice(b"4294967296");
        assert_eq!(to_integer_with_width(&decimal,32),Err(RuntimeError::ValueTooLarge));
        let mut hex=decimal;hex.byte_length=11;hex.bytes[..11].copy_from_slice(b"0x100000000");
        assert_eq!(to_integer_with_width(&hex,32),Err(RuntimeError::ValueTooLarge));
        assert_eq!(to_integer_with_width(&hex,16),Err(RuntimeError::TypeMismatch));
    }

    #[test]fn package_store_preserves_named_target_identity_and_copies_contents(){
        let aml=[0x08,b'P',b'K',b'G',b'0',0x01];
        let mut namespace=AmlNamespace::new();load_definition_block(&aml,&mut namespace).unwrap();
        let node=namespace.find_child(0,*b"PKG0").unwrap();
        let mut arena = test_arena();
        let mut one=AmlRuntimeValue::EMPTY;one.kind=AmlValueKind::Integer;one.integer=1;
        let source_child=arena.allocate(one).unwrap();
        let mut source=AmlRuntimeValue::EMPTY;source.kind=AmlValueKind::Package;source.element_count=1;
        source.elements[0]=source_child;
        let source_id=arena.allocate(source).unwrap();
        let mut old_child=one;old_child.integer=9;
        let old_child_id=arena.allocate(old_child).unwrap();
        let mut target=AmlRuntimeValue::EMPTY;target.kind=AmlValueKind::Package;target.element_count=1;
        target.elements[0]=old_child_id;
        let target_id=arena.allocate(target).unwrap();
        let mut bindings=AmlBindings::new();bindings.bind(&namespace,node,target_id).unwrap();
        store_reference(bindings.reference_node(&namespace,node).unwrap(),source_id,
            &mut bindings,&namespace,&mut arena).unwrap();
        assert_eq!(bindings.value_id(node).unwrap(),target_id);
        let updated=*arena.get(target_id).unwrap();
        assert_ne!(updated.elements[0],source_child);
        assert_eq!(arena.get(updated.elements[0]).unwrap().integer,1);
        assert_eq!(arena.get(source_child).unwrap().integer,1);
        assert_eq!(arena.get(old_child_id).unwrap().integer,9);
    }

    #[test]fn package_element_store_owns_non_package_source_values(){
        let mut arena = test_arena();
        let mut original=AmlRuntimeValue::EMPTY;original.kind=AmlValueKind::Buffer;
        original.byte_length=1;original.bytes[0]=0x11;
        let original_id=arena.allocate(original).unwrap();
        let mut package=AmlRuntimeValue::EMPTY;package.kind=AmlValueKind::Package;
        package.element_count=1;package.elements[0]=original_id;
        let package_id=arena.allocate(package).unwrap();
        let mut changed=original;changed.bytes[0]=0x22;
        let changed_id=arena.allocate(changed).unwrap();
        let namespace=AmlNamespace::new();let mut bindings=AmlBindings::new();
        store_reference(AmlReference::PackageElement{package:package_id,index:0},
            changed_id,&mut bindings,&namespace,&mut arena).unwrap();
        let element=arena.get(arena.get(package_id).unwrap().elements[0]).unwrap();
        assert_eq!(element.bytes[0],0x22);
        assert_eq!(arena.get(changed_id).unwrap().bytes[0],0x22);
        assert_ne!(arena.get(package_id).unwrap().elements[0],changed_id);
    }

    #[test]fn package_target_store_deep_copies_nested_values(){
        let aml=[0x08,b'P',b'K',b'G',b'0',0x01];
        let mut namespace=AmlNamespace::new();load_definition_block(&aml,&mut namespace).unwrap();
        let node=namespace.find_child(0,*b"PKG0").unwrap();
        let mut arena = test_arena();
        let mut one=AmlRuntimeValue::EMPTY;one.kind=AmlValueKind::Integer;one.integer=1;
        let source_child=arena.allocate(one).unwrap();
        let mut inner=AmlRuntimeValue::EMPTY;inner.kind=AmlValueKind::Package;inner.element_count=1;
        inner.elements[0]=source_child;
        let source_inner=arena.allocate(inner).unwrap();
        let mut outer=AmlRuntimeValue::EMPTY;outer.kind=AmlValueKind::Package;outer.element_count=1;
        outer.elements[0]=source_inner;
        let source_package=arena.allocate(outer).unwrap();
        let mut old=AmlRuntimeValue::EMPTY;old.kind=AmlValueKind::Package;old.element_count=0;
        let old_id=arena.allocate(old).unwrap();
        let mut bindings=AmlBindings::new();bindings.bind(&namespace,node,old_id).unwrap();
        store_reference(bindings.reference_node(&namespace,node).unwrap(),source_package,
            &mut bindings,&namespace,&mut arena).unwrap();
        let copied_id=bindings.value_id(node).unwrap();
        let copied_outer=*arena.get(copied_id).unwrap();
        let copied_inner_id=copied_outer.elements[0];
        let copied_inner=*arena.get(copied_inner_id).unwrap();
        let copied_child=copied_inner.elements[0];
        assert_ne!(copied_inner_id,source_inner);
        assert_ne!(copied_child,source_child);
        let mut nine=one;nine.integer=9;let nine_id=arena.allocate(nine).unwrap();
        let inner_ref=AmlReference::PackageElement{package:copied_inner_id,index:0};
        store_reference(inner_ref,nine_id,&mut bindings,&namespace,&mut arena).unwrap();
        assert_eq!(arena.get(source_child).unwrap().integer,1);
        assert_eq!(arena.get(copied_child).unwrap().integer,1);
        assert_eq!(arena.get(arena.get(copied_inner_id).unwrap().elements[0]).unwrap().integer,9);
    }

    #[test]fn cyclic_package_copy_rolls_back_arena_allocations(){
        let mut arena = test_arena();
        let mut package=AmlRuntimeValue::EMPTY;package.kind=AmlValueKind::Package;package.element_count=1;
        let package_id=arena.allocate(package).unwrap();
        arena.get_mut(package_id).unwrap().elements[0]=package_id;
        let checkpoint=arena.count;
        assert_eq!(clone_value_tree(package_id,&mut arena),Err(RuntimeError::InvalidReference));
        assert_eq!(arena.count,checkpoint);
    }

    #[test]fn implicit_source_operand_conversions_follow_expected_type(){
        let arena = test_arena();
        let mut text=AmlRuntimeValue::EMPTY;text.kind=AmlValueKind::String;
        text.byte_length=3;text.bytes[..3].copy_from_slice(b"12Z");
        let integer=convert_source_operand(&text,AmlValueKind::Integer,64,&arena).unwrap();
        assert_eq!(integer.integer,0x12);
        let mut bytes=AmlRuntimeValue::EMPTY;bytes.kind=AmlValueKind::Buffer;
        bytes.byte_length=2;bytes.bytes[..2].copy_from_slice(&[0x34,0x12]);
        let integer=convert_source_operand(&bytes,AmlValueKind::Integer,32,&arena).unwrap();
        assert_eq!(integer.integer,0x1234);
        let mut number=AmlRuntimeValue::EMPTY;number.kind=AmlValueKind::Integer;number.integer=0x1234;
        let buffer=convert_source_operand(&number,AmlValueKind::Buffer,32,&arena).unwrap();
        assert_eq!(buffer.byte_length,4);assert_eq!(&buffer.bytes[..4],&[0x34,0x12,0,0]);
        let string=convert_source_operand(&bytes,AmlValueKind::String,64,&arena).unwrap();
        assert_eq!(string.byte_length,5);assert_eq!(&string.bytes[..5],b"34 12");
        let mut package=AmlRuntimeValue::EMPTY;package.kind=AmlValueKind::Package;
        assert_eq!(convert_source_operand(&package,AmlValueKind::Integer,64,&arena),Err(RuntimeError::TypeMismatch));
    }

    #[test]fn named_store_converts_to_existing_integer_buffer_and_string_types(){
        let aml=[0x08,b'I',b'N',b'T',b'0',0x01,0x08,b'B',b'U',b'F',b'0',0x01,0x08,b'S',b'T',b'R',b'0',0x01];
        let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
        let int_node=ns.find_child(0,*b"INT0").unwrap();
        let buf_node=ns.find_child(0,*b"BUF0").unwrap();
        let str_node=ns.find_child(0,*b"STR0").unwrap();
        let mut arena = test_arena();
        let mut initial=AmlRuntimeValue::EMPTY;initial.kind=AmlValueKind::Integer;initial.integer=1;
        let int_id=arena.allocate(initial).unwrap();
        let mut initial_buf=AmlRuntimeValue::EMPTY;initial_buf.kind=AmlValueKind::Buffer;initial_buf.byte_length=1;
        let buf_id=arena.allocate(initial_buf).unwrap();
        let mut initial_str=AmlRuntimeValue::EMPTY;initial_str.kind=AmlValueKind::String;initial_str.byte_length=1;
        let str_id=arena.allocate(initial_str).unwrap();
        let mut bindings=AmlBindings::new();
        bindings.bind(&ns,int_node,int_id).unwrap();bindings.bind(&ns,buf_node,buf_id).unwrap();bindings.bind(&ns,str_node,str_id).unwrap();
        let mut source=AmlRuntimeValue::EMPTY;source.kind=AmlValueKind::String;source.byte_length=3;source.bytes[..3].copy_from_slice(b"2aZ");
        let source_id=arena.allocate(source).unwrap();
        store_reference(bindings.reference_node(&ns,int_node).unwrap(),source_id,&mut bindings,&ns,&mut arena).unwrap();
        let int_value=dereference(bindings.reference_node(&ns,int_node).unwrap(),&bindings,&arena).unwrap();
        assert_eq!(int_value.kind,AmlValueKind::Integer);assert_eq!(int_value.integer,0x2a);
        store_reference(bindings.reference_node(&ns,buf_node).unwrap(),source_id,&mut bindings,&ns,&mut arena).unwrap();
        let buffer=dereference(bindings.reference_node(&ns,buf_node).unwrap(),&bindings,&arena).unwrap();
        assert_eq!(buffer.kind,AmlValueKind::Buffer);assert_eq!(buffer.byte_length,1);assert_eq!(buffer.bytes[0],b'2');
        let mut integer=AmlRuntimeValue::EMPTY;integer.kind=AmlValueKind::Integer;integer.integer=0x2a;
        let integer_id=arena.allocate(integer).unwrap();
        store_reference(bindings.reference_node(&ns,str_node).unwrap(),integer_id,&mut bindings,&ns,&mut arena).unwrap();
        let string=dereference(bindings.reference_node(&ns,str_node).unwrap(),&bindings,&arena).unwrap();
        assert_eq!(string.kind,AmlValueKind::String);assert_eq!(string.byte_length,16);assert_eq!(&string.bytes[..16],b"000000000000002A");
    }

    #[test]fn implicit_named_conversions_follow_definition_block_integer_width(){
        let aml=[0x08,b'I',b'N',b'T',b'0',0x01,0x08,b'B',b'U',b'F',b'0',0x01,0x08,b'S',b'T',b'R',b'0',0x01];
        let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
        let int_node=ns.find_child(0,*b"INT0").unwrap();
        let buf_node=ns.find_child(0,*b"BUF0").unwrap();
        let str_node=ns.find_child(0,*b"STR0").unwrap();
        let mut arena = test_arena();
        let mut integer=AmlRuntimeValue::EMPTY;integer.kind=AmlValueKind::Integer;
        let int_id=arena.allocate(integer).unwrap();
        let mut buffer=AmlRuntimeValue::EMPTY;buffer.kind=AmlValueKind::Buffer;buffer.byte_length=8;
        let buffer_id=arena.allocate(buffer).unwrap();
        let mut string=AmlRuntimeValue::EMPTY;string.kind=AmlValueKind::String;string.byte_length=1;
        let string_id=arena.allocate(string).unwrap();
        let mut bindings=AmlBindings::with_aml_revision(1);
        bindings.bind(&ns,int_node,int_id).unwrap();
        bindings.bind(&ns,buf_node,buffer_id).unwrap();
        bindings.bind(&ns,str_node,string_id).unwrap();
        let mut source=AmlRuntimeValue::EMPTY;source.kind=AmlValueKind::Integer;source.integer=0x12345678;
        let source_id=arena.allocate(source).unwrap();
        store_reference(bindings.reference_node(&ns,buf_node).unwrap(),source_id,&mut bindings,&ns,&mut arena).unwrap();
        let result=dereference(bindings.reference_node(&ns,buf_node).unwrap(),&bindings,&arena).unwrap();
        assert_eq!(result.byte_length,8);
        assert_eq!(&result.bytes[..8],&[0x78,0x56,0x34,0x12,0,0,0,0]);
        store_reference(bindings.reference_node(&ns,str_node).unwrap(),source_id,&mut bindings,&ns,&mut arena).unwrap();
        let result=dereference(bindings.reference_node(&ns,str_node).unwrap(),&bindings,&arena).unwrap();
        assert_eq!(result.byte_length,8);
        assert_eq!(&result.bytes[..8],b"12345678");
        let mut hex=AmlRuntimeValue::EMPTY;hex.kind=AmlValueKind::String;hex.byte_length=10;
        hex.bytes[..10].copy_from_slice(b"123456789A");
        let hex_id=arena.allocate(hex).unwrap();
        store_reference(bindings.reference_node(&ns,int_node).unwrap(),hex_id,&mut bindings,&ns,&mut arena).unwrap();
        let result=dereference(bindings.reference_node(&ns,int_node).unwrap(),&bindings,&arena).unwrap();
        assert_eq!(result.integer,0x12345678);
    }

    #[test]fn copy_object_bypasses_named_target_conversion(){
        let aml=[0x08,b'I',b'N',b'T',b'0',0x01];
        let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
        let node=ns.find_child(0,*b"INT0").unwrap();
        let mut arena = test_arena();let mut initial=AmlRuntimeValue::EMPTY;
        initial.kind=AmlValueKind::Integer;initial.integer=1;let old=arena.allocate(initial).unwrap();
        let mut bindings=AmlBindings::new();bindings.bind(&ns,node,old).unwrap();
        let mut string=AmlRuntimeValue::EMPTY;string.kind=AmlValueKind::String;string.byte_length=2;
        string.bytes[..2].copy_from_slice(b"42");let new=arena.allocate(string).unwrap();
        store_reference_with_mode(bindings.reference_node(&ns,node).unwrap(),new,StoreMode::CopyObject,&mut bindings,&ns,&mut arena).unwrap();
        let stored=dereference(bindings.reference_node(&ns,node).unwrap(),&bindings,&arena).unwrap();
        assert_eq!(stored.kind,AmlValueKind::String);assert_eq!(stored.byte_length,2);
    }

    #[test]fn alias_name_reference_reads_and_stores_canonical_binding(){
        let aml=[0x08,b'V',b'A',b'L',b'0',0x01,0x06,b'V',b'A',b'L',b'0',b'A',b'L',b'I',b'0'];
        let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
        let val=ns.find_child(0,*b"VAL0").unwrap();let alias=ns.find_child(0,*b"ALI0").unwrap();
        let mut arena = test_arena();let mut one=AmlRuntimeValue::EMPTY;one.kind=AmlValueKind::Integer;one.integer=1;
        let one_id=arena.allocate(one).unwrap();let mut bindings=AmlBindings::new();bindings.bind(&ns,val,one_id).unwrap();
        let r=bindings.reference_node(&ns,alias).unwrap();
        assert_eq!(dereference(r,&bindings,&arena).unwrap().integer,1);
        let mut two=one;two.integer=2;let two_id=arena.allocate(two).unwrap();
        store_reference(r,two_id,&mut bindings,&ns,&mut arena).unwrap();
        assert_eq!(dereference(r,&bindings,&arena).unwrap().integer,2);
    }
    #[test]fn package_and_buffer_index_references_support_store(){
        let mut arena = test_arena();let mut p=AmlRuntimeValue::EMPTY;p.kind=AmlValueKind::Package;p.element_count=1;
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
        assert_eq!(to_integer(&buffer),Ok(0xefcd_ab8967452301));
        buffer.byte_length=0;
        assert_eq!(to_integer(&buffer),Err(RuntimeError::EmptyBuffer));
    }
    #[test]fn argument_object_references_auto_dereference_on_read_and_store(){
        let aml=[0x08,b'N',b'A',b'M',b'0',0x01];
        let mut namespace=AmlNamespace::new();load_definition_block(&aml,&mut namespace).unwrap();
        let node=namespace.find_child(0,*b"NAM0").unwrap();
        let mut arena = test_arena();
        let mut original=AmlRuntimeValue::EMPTY;original.kind=AmlValueKind::Integer;original.integer=7;
        let original_id=arena.allocate(original).unwrap();
        let mut bindings=AmlBindings::new();bindings.bind(&namespace,node,original_id).unwrap();
        let reference=make_object_reference(AmlReference::NamespaceNode(node));
        let reference_id=arena.allocate(reference).unwrap();
        let mut frame=AmlMethodFrame::new(&[reference_id],&mut arena).unwrap();
        let read=frame.read(AmlTarget::Argument(0),&bindings,&namespace,&arena).unwrap();
        assert_eq!(read.kind,AmlValueKind::Integer);assert_eq!(read.integer,7);
        let mut replacement=AmlRuntimeValue::EMPTY;replacement.kind=AmlValueKind::Integer;replacement.integer=99;
        let replacement_id=arena.allocate(replacement).unwrap();
        frame.store(AmlTarget::Argument(0),replacement_id,StoreMode::Store,
            &mut bindings,&namespace,&mut arena).unwrap();
        let read=dereference(bindings.reference_node(&namespace,node).unwrap(),&bindings,&arena).unwrap();
        assert_eq!(read.integer,99);
        // LocalX does not auto-dereference: it replaces the reference value.
        frame.store(AmlTarget::Local(0),reference_id,StoreMode::Store,
            &mut bindings,&namespace,&mut arena).unwrap();
        frame.store(AmlTarget::Local(0),replacement_id,StoreMode::Store,
            &mut bindings,&namespace,&mut arena).unwrap();
        assert_eq!(frame.read(AmlTarget::Local(0),&bindings,&namespace,&arena).unwrap().integer,99);
    }

    #[test]fn method_frame_constructor_rolls_back_when_argument_copy_fails(){
        let mut arena = test_arena();
        let mut package=AmlRuntimeValue::EMPTY;package.kind=AmlValueKind::Package;package.element_count=1;
        let package_id=arena.allocate(package).unwrap();
        arena.get_mut(package_id).unwrap().elements[0]=u16::MAX;
        let checkpoint=arena.count;
        assert!(matches!(AmlMethodFrame::new(&[package_id],&mut arena),Err(RuntimeError::InvalidReference)));
        assert_eq!(arena.count,checkpoint);
    }

    #[test]fn method_locals_and_arguments_replace_typed_values_without_conversion(){
        let aml=[0x08,b'N',b'A',b'M',b'0',0x01];
        let mut namespace=AmlNamespace::new();load_definition_block(&aml,&mut namespace).unwrap();
        let node=namespace.find_child(0,*b"NAM0").unwrap();
        let mut arena = test_arena();
        let mut integer=AmlRuntimeValue::EMPTY;integer.kind=AmlValueKind::Integer;integer.integer=7;
        let integer_id=arena.allocate(integer).unwrap();
        let mut frame=AmlMethodFrame::new(&[integer_id],&mut arena).unwrap();
        let mut string=AmlRuntimeValue::EMPTY;string.kind=AmlValueKind::String;
        string.byte_length=2;string.bytes[..2].copy_from_slice(b"OK");
        let string_id=arena.allocate(string).unwrap();
        let mut bindings=AmlBindings::new();bindings.bind(&namespace,node,integer_id).unwrap();
        let mut unused=AmlBindings::new();
        frame.store(AmlTarget::Local(0),string_id,StoreMode::Store,
            &mut unused,&namespace,&mut arena).unwrap();
        frame.store(AmlTarget::Argument(0),string_id,StoreMode::CopyObject,
            &mut unused,&namespace,&mut arena).unwrap();
        assert_eq!(frame.read(AmlTarget::Local(0),&unused,&namespace,&arena).unwrap().kind,AmlValueKind::String);
        assert_eq!(frame.read(AmlTarget::Argument(0),&unused,&namespace,&arena).unwrap().bytes[..2],*b"OK");
        assert_eq!(frame.read(AmlTarget::Local(0),&bindings,&namespace,&arena).unwrap().byte_length,2);
        assert_eq!(dereference(bindings.reference_node(&namespace,node).unwrap(),&bindings,&arena).unwrap().integer,7);
    }

    #[test]fn method_frame_package_store_copies_nested_package_values(){
        let mut arena = test_arena();
        let mut integer=AmlRuntimeValue::EMPTY;integer.kind=AmlValueKind::Integer;integer.integer=1;
        let child=arena.allocate(integer).unwrap();
        let mut package=AmlRuntimeValue::EMPTY;package.kind=AmlValueKind::Package;package.element_count=1;
        package.elements[0]=child;
        let package_id=arena.allocate(package).unwrap();
        let mut frame=AmlMethodFrame::new(&[],&mut arena).unwrap();
        let mut bindings=AmlBindings::new();let namespace=AmlNamespace::new();
        frame.store(AmlTarget::Local(0),package_id,StoreMode::Store,
            &mut bindings,&namespace,&mut arena).unwrap();
        let copied=frame.read(AmlTarget::Local(0),&bindings,&namespace,&arena).unwrap();
        assert_ne!(copied.elements[0],child);
        assert_eq!(arena.get(child).unwrap().integer,1);
    }

    #[test]fn narrow_buffer_field_uses_integer_typing_and_preserves_neighbor_bits(){
        let mut arena = test_arena();
        let mut backing=AmlRuntimeValue::EMPTY;backing.kind=AmlValueKind::Buffer;
        backing.byte_length=2;backing.bytes[..2].copy_from_slice(&[0b1010_0000,0x5a]);
        let buffer=arena.allocate(backing).unwrap();
        let reference=AmlReference::BufferField{buffer,bit_offset:2,bit_width:4};
        let namespace=AmlNamespace::new();let mut bindings=AmlBindings::with_aml_revision(1);
        let mut source=AmlRuntimeValue::EMPTY;source.kind=AmlValueKind::String;
        source.byte_length=2;source.bytes[..2].copy_from_slice(b"0F");
        let source_id=arena.allocate(source).unwrap();
        store_reference(reference,source_id,&mut bindings,&namespace,&mut arena).unwrap();
        let value=dereference(reference,&bindings,&arena).unwrap();
        assert_eq!(value.kind,AmlValueKind::Integer);assert_eq!(value.integer,0x0f);
        assert_eq!(arena.get(buffer).unwrap().bytes[0],0b1011_1100);
        assert_eq!(arena.get(buffer).unwrap().bytes[1],0x5a);
    }

    #[test]fn wide_buffer_field_uses_buffer_typing_and_zero_extends(){
        let mut arena = test_arena();
        let mut backing=AmlRuntimeValue::EMPTY;backing.kind=AmlValueKind::Buffer;
        backing.byte_length=10;backing.bytes[0]=0xaa;
        let buffer=arena.allocate(backing).unwrap();
        let reference=AmlReference::BufferField{buffer,bit_offset:4,bit_width:72};
        let namespace=AmlNamespace::new();let mut bindings=AmlBindings::with_aml_revision(1);
        let mut source=AmlRuntimeValue::EMPTY;source.kind=AmlValueKind::Integer;
        source.integer=0x12345678;
        let source_id=arena.allocate(source).unwrap();
        store_reference(reference,source_id,&mut bindings,&namespace,&mut arena).unwrap();
        let value=dereference(reference,&bindings,&arena).unwrap();
        assert_eq!(value.kind,AmlValueKind::Buffer);assert_eq!(value.byte_length,9);
        assert_eq!(&value.bytes[..4],&[0x78,0x56,0x34,0x12]);
        assert!(value.bytes[4..9].iter().all(|byte|*byte==0));
        assert_eq!(arena.get(buffer).unwrap().bytes[0]&0x0f,0x0a);
    }

    #[test]fn copy_object_field_accepts_integer_or_buffer_but_rejects_string_atomically(){
        let mut arena = test_arena();
        let mut backing=AmlRuntimeValue::EMPTY;backing.kind=AmlValueKind::Buffer;
        backing.byte_length=2;backing.bytes[..2].copy_from_slice(&[0xaa,0x55]);
        let buffer=arena.allocate(backing).unwrap();
        let reference=AmlReference::BufferField{buffer,bit_offset:4,bit_width:8};
        let namespace=AmlNamespace::new();let mut bindings=AmlBindings::new();
        let mut string=AmlRuntimeValue::EMPTY;string.kind=AmlValueKind::String;
        string.byte_length=1;string.bytes[0]=b'F';
        let string_id=arena.allocate(string).unwrap();
        assert_eq!(store_reference_with_mode(reference,string_id,StoreMode::CopyObject,
            &mut bindings,&namespace,&mut arena),Err(RuntimeError::TypeMismatch));
        assert_eq!(&arena.get(buffer).unwrap().bytes[..2],&[0xaa,0x55]);
        let mut integer=AmlRuntimeValue::EMPTY;integer.kind=AmlValueKind::Integer;integer.integer=0x3c;
        let integer_id=arena.allocate(integer).unwrap();
        store_reference_with_mode(reference,integer_id,StoreMode::CopyObject,
            &mut bindings,&namespace,&mut arena).unwrap();
        assert_eq!(&arena.get(buffer).unwrap().bytes[..2],&[0xca,0x53]);
    }

    #[test]fn zero_length_existing_buffer_targets_remain_zero_length(){
        let arena = test_arena();
        let mut target=AmlRuntimeValue::EMPTY;target.kind=AmlValueKind::Buffer;
        let mut source=AmlRuntimeValue::EMPTY;source.kind=AmlValueKind::Buffer;
        source.byte_length=2;source.bytes[..2].copy_from_slice(&[0x12,0x34]);
        let result=convert_to_target_type(&source,&target,&arena,64,true).unwrap();
        assert_eq!(result.kind,AmlValueKind::Buffer);assert_eq!(result.byte_length,0);
        let mut text=AmlRuntimeValue::EMPTY;text.kind=AmlValueKind::String;
        text.byte_length=2;text.bytes[..2].copy_from_slice(b"AB");
        let result=convert_to_target_type(&text,&target,&arena,64,true).unwrap();
        assert_eq!(result.byte_length,0);
        let mut integer=AmlRuntimeValue::EMPTY;integer.kind=AmlValueKind::Integer;
        integer.integer=0x1234;
        let result=convert_to_target_type(&integer,&target,&arena,64,true).unwrap();
        assert_eq!(result.byte_length,0);
    }

    #[test]fn named_buffer_to_buffer_store_preserves_extent_and_zero_extends(){
        let mut arena = test_arena();
        let mut target=AmlRuntimeValue::EMPTY;target.kind=AmlValueKind::Buffer;
        target.byte_length=3;target.bytes[..3].copy_from_slice(&[0xaa,0xbb,0xcc]);
        let target_id=arena.allocate(target).unwrap();
        let mut source=AmlRuntimeValue::EMPTY;source.kind=AmlValueKind::Buffer;
        source.byte_length=1;source.bytes[0]=0x11;
        let source_id=arena.allocate(source).unwrap();
        let mut bindings=AmlBindings::new();
        // Use a valid Name node so Store follows the fixed-type path.
        let aml=[0x08,b'B',b'U',b'F',b'0',0x01];
        let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
        let node=ns.find_child(0,*b"BUF0").unwrap();
        bindings.bind(&ns,node,target_id).unwrap();
        store_reference(bindings.reference_node(&ns,node).unwrap(),source_id,
            &mut bindings,&ns,&mut arena).unwrap();
        let result=arena.get(bindings.value_id(node).unwrap()).unwrap();
        assert_eq!(result.byte_length,3);assert_eq!(&result.bytes[..3],&[0x11,0,0]);

        let mut large=AmlRuntimeValue::EMPTY;large.kind=AmlValueKind::Buffer;
        large.byte_length=4;large.bytes[..4].copy_from_slice(&[1,2,3,4]);
        let large_id=arena.allocate(large).unwrap();
        store_reference(bindings.reference_node(&ns,node).unwrap(),large_id,
            &mut bindings,&ns,&mut arena).unwrap();
        let result=arena.get(bindings.value_id(node).unwrap()).unwrap();
        assert_eq!(result.byte_length,3);assert_eq!(&result.bytes[..3],&[1,2,3]);
    }

    #[test]fn named_buffer_result_conversion_preserves_target_extent(){
        let aml=[0x08,b'B',b'U',b'F',b'0',0x11,0x05,0x0a,2,0,0];
        let mut namespace=AmlNamespace::new();load_definition_block(&aml,&mut namespace).unwrap();
        let node=namespace.find_child(0,*b"BUF0").unwrap();
        let mut arena = test_arena();
        let mut target=AmlRuntimeValue::EMPTY;target.kind=AmlValueKind::Buffer;
        target.byte_length=2;target.bytes[..2].copy_from_slice(&[0xaa,0xbb]);
        let target_id=arena.allocate(target).unwrap();
        let mut bindings=AmlBindings::new();bindings.bind(&namespace,node,target_id).unwrap();
        let mut integer=AmlRuntimeValue::EMPTY;integer.kind=AmlValueKind::Integer;
        integer.integer=0x12345678;
        let integer_id=arena.allocate(integer).unwrap();
        store_reference(bindings.reference_node(&namespace,node).unwrap(),integer_id,
            &mut bindings,&namespace,&mut arena).unwrap();
        let result=arena.get(bindings.value_id(node).unwrap()).unwrap();
        assert_eq!(result.byte_length,2);assert_eq!(&result.bytes[..2],&[0x78,0x56]);
        let mut text=AmlRuntimeValue::EMPTY;text.kind=AmlValueKind::String;
        text.byte_length=4;text.bytes[..4].copy_from_slice(b"ABCD");
        let text_id=arena.allocate(text).unwrap();
        store_reference(bindings.reference_node(&namespace,node).unwrap(),text_id,
            &mut bindings,&namespace,&mut arena).unwrap();
        let result=arena.get(bindings.value_id(node).unwrap()).unwrap();
        assert_eq!(result.byte_length,2);assert_eq!(&result.bytes[..2],b"AB");
    }

    #[test]fn copy_object_detaches_mutable_buffer_and_string_values(){
        let aml=[0x08,b'T',b'A',b'R',b'G',0x11,0x04,0x0a,1,0xaa];
        let mut namespace=AmlNamespace::new();load_definition_block(&aml,&mut namespace).unwrap();
        let node=namespace.find_child(0,*b"TARG").unwrap();
        let mut arena = test_arena();
        let mut target=AmlRuntimeValue::EMPTY;target.kind=AmlValueKind::Buffer;target.byte_length=1;
        let target_id=arena.allocate(target).unwrap();
        let mut source=AmlRuntimeValue::EMPTY;source.kind=AmlValueKind::Buffer;
        source.byte_length=1;source.bytes[0]=0x5a;
        let source_id=arena.allocate(source).unwrap();
        let mut bindings=AmlBindings::new();bindings.bind(&namespace,node,target_id).unwrap();
        store_reference_with_mode(bindings.reference_node(&namespace,node).unwrap(),source_id,
            StoreMode::CopyObject,&mut bindings,&namespace,&mut arena).unwrap();
        let copied_id=bindings.value_id(node).unwrap();
        assert_ne!(copied_id,source_id);
        let mut changed=source;changed.bytes[0]=0x99;
        let changed_id=arena.allocate(changed).unwrap();
        store_reference(AmlReference::BufferByte{buffer:copied_id,index:0},changed_id,
            &mut bindings,&namespace,&mut arena).unwrap();
        assert_eq!(arena.get(source_id).unwrap().bytes[0],0x5a);
        assert_eq!(arena.get(copied_id).unwrap().bytes[0],0x99);
    }

    #[test]fn copy_object_to_buffer_byte_rejects_string_without_mutation(){
        let mut arena = test_arena();
        let mut buffer=AmlRuntimeValue::EMPTY;buffer.kind=AmlValueKind::Buffer;
        buffer.byte_length=1;buffer.bytes[0]=0xaa;
        let buffer_id=arena.allocate(buffer).unwrap();
        let mut string=AmlRuntimeValue::EMPTY;string.kind=AmlValueKind::String;
        string.byte_length=1;string.bytes[0]=b'X';
        let string_id=arena.allocate(string).unwrap();
        let reference=AmlReference::BufferByte{buffer:buffer_id,index:0};
        let mut bindings=AmlBindings::new();let namespace=AmlNamespace::new();
        assert_eq!(store_reference_with_mode(reference,string_id,StoreMode::CopyObject,
            &mut bindings,&namespace,&mut arena),Err(RuntimeError::TypeMismatch));
        assert_eq!(arena.get(buffer_id).unwrap().bytes[0],0xaa);
        assert_eq!(store_reference_with_mode(reference,string_id,StoreMode::Store,
            &mut bindings,&namespace,&mut arena),Ok(()));
        assert_eq!(arena.get(buffer_id).unwrap().bytes[0],b'X');
    }

    #[test]fn buffer_byte_store_rejects_wrong_types_without_mutating_target(){
        let mut arena = test_arena();
        let mut buffer=AmlRuntimeValue::EMPTY;buffer.kind=AmlValueKind::Buffer;buffer.byte_length=1;buffer.bytes[0]=0xaa;
        let buffer_id=arena.allocate(buffer).unwrap();
        let mut string=AmlRuntimeValue::EMPTY;string.kind=AmlValueKind::String;string.byte_length=1;string.bytes[0]=b'1';
        let string_id=arena.allocate(string).unwrap();
        let mut wide=AmlRuntimeValue::EMPTY;wide.kind=AmlValueKind::Integer;wide.integer=0x100;
        let wide_id=arena.allocate(wide).unwrap();
        let reference=AmlReference::BufferByte{buffer:buffer_id,index:0};
        let namespace=AmlNamespace::new();let mut bindings=AmlBindings::new();
        assert_eq!(store_reference(reference,string_id,&mut bindings,&namespace,&mut arena),Ok(()));
        assert_eq!(arena.get(buffer_id).unwrap().bytes[0],b'1');
        assert_eq!(store_reference(reference,wide_id,&mut bindings,&namespace,&mut arena),Ok(()));
        assert_eq!(arena.get(buffer_id).unwrap().bytes[0],0);
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
        let mut s=AmlRuntimeValue::EMPTY;s.kind=AmlValueKind::String;s.bytes[..6].copy_from_slice(b"0x1a2f");s.byte_length=6;
        assert_eq!(to_integer(&s),Ok(0x1a2f));
        let mut decimal=s;decimal.bytes[..3].copy_from_slice(b"123");decimal.byte_length=3;
        assert_eq!(to_integer(&decimal),Ok(123));
        let b=to_buffer(&s,6).unwrap();assert_eq!(&b.bytes[..6],b"0x1a2f");
        assert_eq!(integer_to_string(120).bytes[..3],*b"120");
        let mut bad=s;bad.bytes[2]=b'x';assert_eq!(to_integer(&bad),Err(RuntimeError::InvalidIntegerString));
    }
    #[test]fn malformed_public_byte_lengths_are_rejected_without_panics(){
        let mut malformed=AmlRuntimeValue::EMPTY;
        malformed.kind=AmlValueKind::Buffer;
        malformed.byte_length=(MAX_AML_BYTES+1) as u16;
        assert_eq!(to_integer(&malformed),Err(RuntimeError::InvalidObjectLength));
        assert_eq!(to_buffer(&malformed,4),Err(RuntimeError::InvalidObjectLength));
        let mut arena = test_arena();
        let id=arena.allocate(malformed).unwrap();
        assert_eq!(index_reference(&arena,id,0),Err(RuntimeError::InvalidObjectLength));
    }

    #[test]fn hexadecimal_string_conversion_rejects_empty_and_embedded_nul(){
        let mut string=AmlRuntimeValue::EMPTY;string.kind=AmlValueKind::String;
        assert_eq!(to_integer(&string),Err(RuntimeError::InvalidIntegerString));
        string.byte_length=3;string.bytes[..3].copy_from_slice(&[b'1',0,b'2']);
        assert_eq!(to_integer(&string),Err(RuntimeError::InvalidIntegerString));
        string.byte_length=2;string.bytes[..2].copy_from_slice(b"0x");
        assert_eq!(to_integer(&string),Err(RuntimeError::InvalidIntegerString));
    }

    #[test]fn hexadecimal_string_conversion_detects_u64_overflow(){
        let mut string=AmlRuntimeValue::EMPTY;string.kind=AmlValueKind::String;
        string.byte_length=19;string.bytes[..19].copy_from_slice(b"0xfffffffffffffffff");
        assert_eq!(to_integer(&string),Err(RuntimeError::ValueTooLarge));
    }

    #[test]fn malformed_package_element_counts_are_rejected_without_indexing(){
        let mut package=AmlRuntimeValue::EMPTY;package.kind=AmlValueKind::Package;
        package.element_count=(crate::aml_objects::MAX_PACKAGE_ELEMENTS+1) as u8;
        let mut arena = test_arena();let id=arena.allocate(package).unwrap();
        assert_eq!(index_reference(&arena,id,0),Err(RuntimeError::InvalidObjectLength));
        let reference=AmlReference::PackageElement{package:id,index:0};
        let bindings=AmlBindings::new();
        assert_eq!(dereference(reference,&bindings,&arena),Err(RuntimeError::InvalidObjectLength));
    }

    #[test]fn buffer_byte_store_rejects_corrupt_target_extent_atomically(){
        let mut arena = test_arena();
        let mut buffer=AmlRuntimeValue::EMPTY;buffer.kind=AmlValueKind::Buffer;
        buffer.byte_length=(MAX_AML_BYTES+1) as u16;buffer.bytes[0]=0xaa;
        let buffer_id=arena.allocate(buffer).unwrap();
        let mut replacement=AmlRuntimeValue::EMPTY;replacement.kind=AmlValueKind::Integer;replacement.integer=0x55;
        let replacement_id=arena.allocate(replacement).unwrap();
        let reference=AmlReference::BufferByte{buffer:buffer_id,index:0};
        let mut bindings=AmlBindings::new();let namespace=AmlNamespace::new();
        assert_eq!(store_reference(reference,replacement_id,&mut bindings,&namespace,&mut arena),Err(RuntimeError::InvalidObjectLength));
        assert_eq!(arena.get(buffer_id).unwrap().bytes[0],0xaa);
    }

    #[test]fn package_store_rejects_corrupt_element_count_atomically(){
        let mut arena = test_arena();
        let mut package=AmlRuntimeValue::EMPTY;package.kind=AmlValueKind::Package;
        package.element_count=(crate::aml_objects::MAX_PACKAGE_ELEMENTS+1) as u8;
        package.elements[0]=0;
        let package_id=arena.allocate(package).unwrap();
        let mut value=AmlRuntimeValue::EMPTY;value.kind=AmlValueKind::Integer;value.integer=9;
        let value_id=arena.allocate(value).unwrap();
        let reference=AmlReference::PackageElement{package:package_id,index:0};
        let mut bindings=AmlBindings::new();let namespace=AmlNamespace::new();
        assert_eq!(store_reference(reference,value_id,&mut bindings,&namespace,&mut arena),Err(RuntimeError::InvalidObjectLength));
        assert_eq!(arena.get(package_id).unwrap().elements[0],0);
    }

    #[test]fn checked_namespace_binding_rejects_invalid_arena_values(){
        let aml=[0x08,b'V',b'A',b'L',b'0',0x01];
        let mut namespace=AmlNamespace::new();load_definition_block(&aml,&mut namespace).unwrap();
        let node=namespace.find_child(0,*b"VAL0").unwrap();
        let mut bindings=AmlBindings::new();let arena = test_arena();
        assert_eq!(bindings.bind_checked(&namespace,node,0,&arena),Err(RuntimeError::InvalidReference));
        assert_eq!(bindings.value_id(node),Err(RuntimeError::UnboundNode));
    }

    #[test]fn checked_namespace_binding_rejects_malformed_package_children(){
        let aml=[0x08,b'V',b'A',b'L',b'0',0x01];
        let mut namespace=AmlNamespace::new();load_definition_block(&aml,&mut namespace).unwrap();
        let node=namespace.find_child(0,*b"VAL0").unwrap();
        let mut arena = test_arena();
        let mut package=AmlRuntimeValue::EMPTY;package.kind=AmlValueKind::Package;
        package.element_count=1;package.elements[0]=u16::MAX;
        let id=arena.allocate(package).unwrap();
        let mut bindings=AmlBindings::new();
        assert_eq!(bindings.bind_checked(&namespace,node,id,&arena),Err(RuntimeError::InvalidReference));
        assert_eq!(bindings.value_id(node),Err(RuntimeError::UnboundNode));
    }

    #[test]fn reference_store_rejects_malformed_source_without_mutating_target(){
        let mut arena = test_arena();
        let mut package=AmlRuntimeValue::EMPTY;package.kind=AmlValueKind::Package;package.element_count=1;
        let mut child=AmlRuntimeValue::EMPTY;child.kind=AmlValueKind::Integer;child.integer=1;
        let child_id=arena.allocate(child).unwrap();package.elements[0]=child_id;
        let package_id=arena.allocate(package).unwrap();
        let mut invalid=AmlRuntimeValue::EMPTY;invalid.kind=AmlValueKind::Package;
        invalid.element_count=1;invalid.elements[0]=u16::MAX;
        let invalid_id=arena.allocate(invalid).unwrap();
        let reference=AmlReference::PackageElement{package:package_id,index:0};
        let mut bindings=AmlBindings::new();let namespace=AmlNamespace::new();
        assert_eq!(store_reference(reference,invalid_id,&mut bindings,&namespace,&mut arena),Err(RuntimeError::InvalidReference));
        assert_eq!(arena.get(package_id).unwrap().elements[0],child_id);
    }

    #[test]
    fn invalidates_cached_name_values_owned_by_unloaded_table() {
        let aml=[0x08,b'T',b'E',b'S',b'T',0x0a,0x2a];
        let mut namespace=AmlNamespace::new();
        crate::aml::load_definition_block(&aml,&mut namespace).unwrap();
        let node=namespace.find_child(0,*b"TEST").unwrap();
        let mut arena=test_arena();
        let mut value=AmlRuntimeValue::EMPTY;
        value.kind=AmlValueKind::Integer;
        value.integer=42;
        let id=arena.allocate(value).unwrap();
        let mut bindings=AmlBindings::new();
        bindings.bind_checked(&namespace,node,id,&arena).unwrap();
        assert_eq!(bindings.value_id(node),Ok(id));
        assert_eq!(bindings.invalidate_table(&namespace,0),1);
        assert_eq!(bindings.value_id(node),Err(RuntimeError::UnboundNode));
    }

    #[test]
    fn stale_namespace_reference_cannot_alias_reloaded_node() {
        let first_aml=[0x08,b'T',b'E',b'S',b'T',0x0a,0x2a];
        let mut namespace=AmlNamespace::new();
        crate::aml::load_definition_block(&first_aml,&mut namespace).unwrap();
        let old_node=namespace.find_child(0,*b"TEST").unwrap();
        let mut arena=test_arena();
        let mut bindings=AmlBindings::new();
        let old_reference=bindings.reference_node(&namespace,old_node).unwrap();
        let old_value=arena.allocate({let mut v=AmlRuntimeValue::EMPTY;v.kind=AmlValueKind::Integer;v.integer=42;v}).unwrap();
        bindings.bind_checked(&namespace,old_node,old_value,&arena).unwrap();

        // The table manager calls this while execution is quiesced, before
        // removing the owner's nodes. A later table may reuse the same index.
        assert_eq!(bindings.invalidate_table(&namespace,0),1);
        assert_eq!(crate::aml::unload_definition_block(&mut namespace,0),1);

        let second_aml=[0x08,b'T',b'E',b'S',b'T',0x0a,0x63];
        crate::aml::load_definition_block(&second_aml,&mut namespace).unwrap();
        let new_node=namespace.find_child(0,*b"TEST").unwrap();
        assert_eq!(new_node,old_node);
        let new_value=arena.allocate({let mut v=AmlRuntimeValue::EMPTY;v.kind=AmlValueKind::Integer;v.integer=99;v}).unwrap();
        bindings.bind_checked(&namespace,new_node,new_value,&arena).unwrap();

        assert_eq!(dereference(old_reference,&bindings,&arena),Err(RuntimeError::InvalidReference));
        let new_reference=bindings.reference_node(&namespace,new_node).unwrap();
        assert_eq!(dereference(new_reference,&bindings,&arena).unwrap().integer,99);
    }

}
