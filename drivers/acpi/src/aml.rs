//! Bounded AML definition-block parser and namespace loader.
//! This module deliberately separates AML parsing from AML execution. Method
//! bodies are retained as validated byte ranges; they are never executed here.
/*
 * FIXME(ACPI-PRODUCTION) — AML parser and namespace
 * FIXME(ACPI-PRODUCTION): This module currently parses a bounded AML subset and stores namespace metadata; it is not a complete ACPI AML implementation. Intended completion: full NameString/namespace semantics, table ownership and unload, forward references, complete declaration/Field parsing, comprehensive opcode coverage, robust error offsets, conformance fixtures, and malformed-input fuzzing. Keep all cursor arithmetic checked and preserve the invariant that parser code never executes AML or performs firmware I/O. Unsupported constructs must fail closed and be reported distinctly from malformed input.
 */

#![no_std]

pub const MAX_NAMESPACE_NODES:usize=2048;
pub const MAX_NAME_SEGMENTS:usize=8;

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum AmlError { Truncated, BadPackageLength, BadName, UnsupportedOpcode, NamespaceFull, NamespaceCapacity { capacity:usize, requested:usize }, DuplicateName, InvalidParent, BadDefinitionBlock }

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct AmlName { pub segments:[[u8;4];MAX_NAME_SEGMENTS], pub count:u8, pub rooted:bool, pub parents:u8 }
impl AmlName { pub const EMPTY:Self=Self{segments:[[0;4];MAX_NAME_SEGMENTS],count:0,rooted:false,parents:0}; }

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
#[repr(u8)]
pub enum AmlObjectType { Root=0, Scope=1, Device=2, Method=3, Name=4, Processor=5, PowerResource=6, ThermalZone=7, Alias=8, External=9, OperationRegion=10, Field=11, IndexField=12, BankField=13, Mutex=14, Event=15, DataRegion=16 }
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct AmlNode {
    pub parent:u16, pub name:[u8;4], pub object_type:AmlObjectType,
    /// AML definition block slot that supplied this namespace object.
    pub source_table:u32,
    pub method_flags:u8, pub body_start:u32, pub body_length:u32,
    /// Canonical target for Alias objects; u16::MAX for non-alias nodes.
    pub alias_target:u16,
    pub field_region:u16,
    pub field_mode:u8,
    pub field_connection:bool,
    pub field_access_type:u8,
    pub field_access_attribute:u8,
    pub field_access_length:u8,
    pub field_lock_rule:u8,
    pub field_update_rule:u8,
    pub field_index_register:u16, pub field_data_register:u16, pub field_bank_register:u16,
    pub field_bank_value_start:u32, pub field_bank_value_length:u32,
    pub field_connection_start:u32, pub field_connection_length:u32,
    pub processor_id:u8, pub processor_pblk_address:u32, pub processor_pblk_length:u8,
    pub power_resource_system_level:u8, pub power_resource_order:u16,
    pub external_placeholder:bool, pub external_owner:u32, pub external_object_type:u8, pub external_method_flags:u8,
}
impl AmlNode {
    pub const ROOT:Self=Self{parent:0,name:*b"____",object_type:AmlObjectType::Root,source_table:0,method_flags:0,body_start:0,body_length:0,alias_target:u16::MAX,field_region:u16::MAX,field_mode:0,field_connection:false,field_access_type:0,field_access_attribute:0,field_access_length:0,field_lock_rule:0,field_update_rule:0,field_index_register:u16::MAX,field_data_register:u16::MAX,field_bank_register:u16::MAX,field_bank_value_start:0,field_bank_value_length:0,field_connection_start:0,field_connection_length:0,processor_id:0,processor_pblk_address:0,processor_pblk_length:0,power_resource_system_level:0,power_resource_order:0,external_placeholder:false,external_owner:0,external_object_type:0,external_method_flags:0};
}
pub struct AmlNamespace { pub nodes:[AmlNode;MAX_NAMESPACE_NODES], pub count:usize, loading_table:u32 }
impl AmlNamespace {
    pub const fn new()->Self { Self{nodes:[AmlNode::ROOT;MAX_NAMESPACE_NODES],count:1,loading_table:0} }
/// Find a direct namespace child by its four-byte AML NameSeg without resolving aliases.
    pub fn find_child(&self,parent:u16,name:[u8;4])->Option<u16>{
        self.nodes.iter().take(self.count).enumerate().skip(1)
            .find(|(_,n)|n.parent==parent&&n.name==name).map(|(i,_)|i as u16)
    }

    /// Follow Alias objects to their canonical namespace object. Cycles and
    /// invalid targets are rejected rather than recursing indefinitely.
    pub fn resolve_alias(&self,mut index:u16)->Result<u16,AmlError>{
        for _ in 0..self.count {
            let node=self.nodes.get(index as usize).filter(|_|(index as usize)<self.count).ok_or(AmlError::InvalidParent)?;
            if node.object_type!=AmlObjectType::Alias{return Ok(index)}
            if node.alias_target==u16::MAX{return Err(AmlError::InvalidParent)}
            index=node.alias_target;
        }
        Err(AmlError::BadDefinitionBlock)
    }
    fn insert(&mut self,parent:u16,name:[u8;4],kind:AmlObjectType,flags:u8,start:usize,len:usize)->Result<u16,AmlError>{
        if parent as usize>=self.count{return Err(AmlError::InvalidParent)}
        if self.find_child(parent,name).is_some(){return Err(AmlError::DuplicateName)}
        if self.count==MAX_NAMESPACE_NODES{return Err(AmlError::NamespaceCapacity{capacity:MAX_NAMESPACE_NODES,requested:self.count+1})}
        let i=self.count;self.nodes[i]=AmlNode{parent,name,object_type:kind,source_table:self.loading_table,method_flags:flags,body_start:u32::try_from(start).map_err(|_|AmlError::BadDefinitionBlock)?,body_length:u32::try_from(len).map_err(|_|AmlError::BadDefinitionBlock)?,alias_target:u16::MAX,field_region:u16::MAX,field_mode:0,field_connection:false,field_access_type:0,field_access_attribute:0,field_access_length:0,field_lock_rule:0,field_update_rule:0,field_index_register:u16::MAX,field_data_register:u16::MAX,field_bank_register:u16::MAX,field_bank_value_start:0,field_bank_value_length:0,field_connection_start:0,field_connection_length:0,processor_id:0,processor_pblk_address:0,processor_pblk_length:0,power_resource_system_level:0,power_resource_order:0,external_placeholder:false,external_owner:0,external_object_type:0,external_method_flags:0};
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
/// Decode AML PkgLength and return the package end while enforcing the caller's byte limit.
pub fn decode_package_length(bytes:&[u8],offset:usize,limit:usize)->Result<(usize,usize),AmlError>{
    if limit>bytes.len()||offset>limit{return Err(AmlError::Truncated)}
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
/// Parse one AML NameString at `offset`, returning its decoded path and the
/// first byte after it. This is shared by namespace lookup and method execution.
pub fn parse_name_string(bytes:&[u8],offset:usize,limit:usize)->Result<(AmlName,usize),AmlError>{
    if offset>limit||limit>bytes.len(){return Err(AmlError::Truncated)}
    let mut cursor=Cursor{bytes,pos:offset,end:limit};
    let name=parse_name(&mut cursor)?;
    Ok((name,cursor.pos))
}

/// AML IntegerData decoded according to the DefinitionBlock AML revision.
/// Revisions below 2 use a 32-bit Integer object; revision 2 and later use
/// 64 bits. QWordConst is rejected for legacy AML revisions.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct AmlInteger { pub value:u64, pub width_bits:u8 }

pub fn decode_integer_constant(bytes:&[u8],offset:usize,limit:usize,aml_revision:u8)->Result<(AmlInteger,usize),AmlError>{
    if offset>limit||limit>bytes.len(){return Err(AmlError::Truncated)}
    let mut c=Cursor{bytes,pos:offset,end:limit};
    let opcode=c.byte()?;
    let width=if aml_revision<2{32}else{64};
    let value=match opcode{
        0x00=>0,
        0x01=>1,
        0xff=>if width==32{u32::MAX as u64}else{u64::MAX},
        0x0a=>c.byte()? as u64,
        0x0b=>u16::from_le_bytes(c.take(2)?.try_into().map_err(|_|AmlError::Truncated)?) as u64,
        0x0c=>u32::from_le_bytes(c.take(4)?.try_into().map_err(|_|AmlError::Truncated)?) as u64,
        0x0e if width==64=>u64::from_le_bytes(c.take(8)?.try_into().map_err(|_|AmlError::Truncated)?),
        0x0e=>return Err(AmlError::UnsupportedOpcode),
        _=>return Err(AmlError::UnsupportedOpcode),
    };
    if width==32 && value>u32::MAX as u64{return Err(AmlError::BadDefinitionBlock)}
    Ok((AmlInteger{value,width_bits:width},c.pos))
}

fn resolve_parent(ns:&AmlNamespace,current:u16,n:&AmlName)->Result<u16,AmlError>{
    let mut p=if n.rooted{0}else{current};
    for _ in 0..n.parents{
        if p==0||p as usize>=ns.count{return Err(AmlError::InvalidParent)}
        p=ns.nodes[p as usize].parent;
    }
    Ok(p)
}

/// Resolve a NameString using AML namespace search rules. Rooted and explicit
/// parent paths are exact; unprefixed paths search successive ancestor scopes.
pub fn resolve_name_string(ns:&AmlNamespace,current:u16,n:&AmlName)->Result<u16,AmlError>{
    if current as usize>=ns.count{return Err(AmlError::InvalidParent)}
    if n.count==0{return Ok(resolve_parent(ns,current,n)?)}
    let mut candidate=resolve_parent(ns,current,n)?;
    let explicit=n.rooted||n.parents!=0;
    loop {
        if let Some(mut found)=ns.find_child(candidate,n.segments[0]) {
            for segment in n.segments.iter().take(n.count as usize).skip(1){
                found=ns.find_child(found,*segment).ok_or(AmlError::InvalidParent)?;
            }
            return Ok(found);
        }
        if explicit||candidate==0{return Err(AmlError::InvalidParent)}
        candidate=ns.nodes.get(candidate as usize).ok_or(AmlError::InvalidParent)?.parent;
    }
}

fn lookup_path(ns:&AmlNamespace,current:u16,n:&AmlName)->Result<u16,AmlError>{
    if n.count==0{return Ok(if n.rooted{0}else{current})}
    let start=resolve_parent(ns,current,n)?;
    let mut p=start;
    for i in 0..n.count as usize{
        p=ns.find_child(p,n.segments[i]).ok_or(AmlError::InvalidParent)?;
    }
    Ok(p)
}

fn external_type_for(kind:AmlObjectType)->Option<u8>{
    match kind{
        AmlObjectType::Device=>Some(6),AmlObjectType::Event=>Some(7),
        AmlObjectType::Method=>Some(8),AmlObjectType::Mutex=>Some(9),
        AmlObjectType::OperationRegion=>Some(10),AmlObjectType::PowerResource=>Some(11),
        AmlObjectType::Processor=>Some(12),AmlObjectType::ThermalZone=>Some(13),
        _=>None,
    }
}
fn resolve_path(ns:&mut AmlNamespace,current:u16,n:&AmlName,create_intermediate:bool,kind:AmlObjectType,body_start:usize,body_len:usize,flags:u8)->Result<u16,AmlError>{
    if n.count==0{return Ok(if n.rooted{0}else{current})}
    let mut p=resolve_parent(ns,current,n)?;
    for i in 0..n.count as usize{
        let name=n.segments[i];
        if let Some(found)=ns.find_child(p,name){
            if i+1==n.count as usize {
                let existing=ns.nodes[found as usize].object_type;
                if kind==AmlObjectType::Scope {
                    if !matches!(existing,AmlObjectType::Root|AmlObjectType::Scope|AmlObjectType::Device|AmlObjectType::Processor|AmlObjectType::PowerResource|AmlObjectType::ThermalZone){return Err(AmlError::InvalidParent)}
                    
                } else if kind==AmlObjectType::External && existing==AmlObjectType::External {
                    // The External parser validates repeated declaration
                    // compatibility after it has decoded the declared type.
                } else if existing==AmlObjectType::External && ns.nodes[found as usize].external_placeholder {
                    let declared=ns.nodes[found as usize].external_object_type;
                    if declared!=0 && external_type_for(kind).is_some_and(|expected|expected!=declared){return Err(AmlError::DuplicateName)}
                    ns.nodes[found as usize].object_type=kind;
                    ns.nodes[found as usize].method_flags=flags;
                    ns.nodes[found as usize].body_start=u32::try_from(body_start).map_err(|_|AmlError::BadDefinitionBlock)?;
                    ns.nodes[found as usize].body_length=u32::try_from(body_len).map_err(|_|AmlError::BadDefinitionBlock)?;
                    ns.nodes[found as usize].source_table=ns.loading_table;
                } else {return Err(AmlError::DuplicateName)}
            }
            p=found;continue
        }
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
        0x60..=0x6e=>Ok(()), // Local0..Local7 and Arg0..Arg6
        // Integer and logical TermArgs used by BufferSize and package
        // expressions. Parsing validates operand and target boundaries; the
        // executor independently decides which operations it can evaluate.
        0x72|0x74|0x77|0x79|0x7a|0x7b|0x7c|0x7d|0x7e|0x7f|0x85
        |0x90|0x91|0x93|0x94|0x95=>{
            skip_data_object_depth(c,depth+1)?;
            skip_data_object_depth(c,depth+1)?;
            if matches!(op,0x90|0x91|0x93|0x94|0x95){Ok(())}
            else{skip_target(c)}
        }
        0x78=>{ // Divide has quotient and remainder targets.
            skip_data_object_depth(c,depth+1)?;
            skip_data_object_depth(c,depth+1)?;
            skip_target(c)?;skip_target(c)
        }
        0x75|0x76=>skip_target(c),
        0x81|0x82=>{skip_data_object_depth(c,depth+1)?;skip_target(c)},
        0x80=>{skip_data_object_depth(c,depth+1)?;skip_target(c)},
        0x83|0x92=>skip_data_object_depth(c,depth+1),
        0x88=>{ // Index(Source, Index, Target)
            skip_data_object_depth(c,depth+1)?;
            skip_data_object_depth(c,depth+1)?;
            skip_target(c)
        }
        0x5c|0x5e|0x2e|0x2f|b'_'|b'A'..=b'Z'=>{
            c.pos-=1;let _=parse_name(c)?;Ok(())
        }
        _=>Err(AmlError::UnsupportedOpcode)
    }
}

fn skip_target(c:&mut Cursor<'_>)->Result<(),AmlError>{
    let op=c.byte()?;
    match op{
        0x00|0x60..=0x6e=>Ok(()),
        0x5c|0x5e|0x2e|0x2f|b'_'|b'A'..=b'Z'=>{
            c.pos-=1;let _=parse_name(c)?;Ok(())
        }
        _=>Err(AmlError::UnsupportedOpcode),
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
    // FieldLength reuses the PkgLength byte encoding but represents a bit
    // count, so zero remains a valid ReservedField width.
    Ok(value)
}

fn parse_field_list(c:&mut Cursor<'_>,ns:&mut AmlNamespace,container:u16,region:u16,mode:u8,flags:u8,index_register:u16,data_register:u16,bank_register:u16,bank_value_start:u32,bank_value_length:u32)->Result<(),AmlError>{
    let mut bit_offset=0usize;
    let mut access_type=0u8;
    let mut access_attribute=0u8;
    let mut access_length=0u8;
    let mut connection=false;
    let mut connection_start=0u32;
    let mut connection_length=0u32;
    while c.pos<c.end{
        let op=c.byte()?;
        match op{
            0x00=>{ // ReservedField: advance by encoded bit length.
                bit_offset=bit_offset.checked_add(read_pkg_value(c)?).ok_or(AmlError::BadPackageLength)?;
            }
            0x01=>{access_type=c.byte()?;access_attribute=c.byte()?;access_length=0;if access_type>5||access_attribute>0x0f{return Err(AmlError::UnsupportedOpcode)}} // AccessField
            0x02=>{let start=c.pos;skip_data_object(c)?;connection=true;connection_start=u32::try_from(start).map_err(|_|AmlError::BadDefinitionBlock)?;connection_length=u32::try_from(c.pos-start).map_err(|_|AmlError::BadDefinitionBlock)?;} // ConnectField
            0x03=>{access_type=c.byte()?;access_attribute=c.byte()?;access_length=c.byte()?;if access_type>5{return Err(AmlError::UnsupportedOpcode)}} // ExtendedAccessField
            _=>{
                c.pos-=1;
                let seg=c.take(4)?;
                if !valid_name_seg(seg){return Err(AmlError::BadName)}
                let mut name=[0;4];name.copy_from_slice(seg);
                let bit_length=read_pkg_value(c)?;
                if bit_length==0{return Err(AmlError::BadPackageLength)}
                u32::try_from(bit_offset).map_err(|_|AmlError::BadPackageLength)?;
                u32::try_from(bit_length).map_err(|_|AmlError::BadPackageLength)?;
                let id=ns.insert(container,name,AmlObjectType::Field,0,bit_offset,bit_length)?;
                ns.nodes[id as usize].field_region=region;
                ns.nodes[id as usize].field_mode=mode;
                ns.nodes[id as usize].field_index_register=index_register;
                ns.nodes[id as usize].field_data_register=data_register;
                ns.nodes[id as usize].field_bank_register=bank_register;
                ns.nodes[id as usize].field_bank_value_start=bank_value_start;
                ns.nodes[id as usize].field_bank_value_length=bank_value_length;
                ns.nodes[id as usize].field_connection_start=connection_start;
                ns.nodes[id as usize].field_connection_length=connection_length;
                ns.nodes[id as usize].field_connection=connection;
                ns.nodes[id as usize].field_access_type=access_type;
                ns.nodes[id as usize].field_access_attribute=access_attribute;
                ns.nodes[id as usize].field_access_length=access_length;
                ns.nodes[id as usize].field_lock_rule=flags&1;
                ns.nodes[id as usize].field_update_rule=(flags>>1)&3;
                connection=false;connection_start=0;connection_length=0;
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
                // Alias source must already resolve; never manufacture a
                // placeholder object for a misspelled source NameString.
                let target=lookup_path(ns,parent,&source)?;
                let alias_index=resolve_path(ns,parent,&alias,true,AmlObjectType::Alias,0,0,0)?;
                ns.nodes[alias_index as usize].alias_target=target;
            }
            0x15=>{let name=parse_name(c)?;let object_type=c.byte()?;let flags=if object_type==8{c.byte()?}else{0};let id=resolve_path(ns,parent,&name,true,AmlObjectType::External,0,0,flags)?;let node=&mut ns.nodes[id as usize];if node.external_placeholder{if node.external_object_type!=object_type||node.external_method_flags!=flags{return Err(AmlError::DuplicateName)}}else{node.external_placeholder=true;node.external_owner=ns.loading_table;node.external_object_type=object_type;node.external_method_flags=flags;}}
            0x5b=>{
                let ext=c.byte()?;
                match ext{
                    0x01=>{let name=parse_name(c)?;let sync_level=c.byte()?;if sync_level>15{return Err(AmlError::BadDefinitionBlock)}resolve_path(ns,parent,&name,true,AmlObjectType::Mutex,0,0,sync_level)?;}
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
                        let mut index_register=u16::MAX;let mut data_register=u16::MAX;let mut bank_register=u16::MAX;
                        let mut bank_value_start=0u32;let mut bank_value_length=0u32;
                        if ext==0x86{
                            let index=lookup_path(ns,parent,&region).map_err(|_|AmlError::InvalidParent)?;
                            let data_name=parse_name(&mut sub)?;
                            let data=lookup_path(ns,parent,&data_name).map_err(|_|AmlError::InvalidParent)?;
                            index_register=ns.resolve_alias(index)?;data_register=ns.resolve_alias(data)?;
                        }
                        if ext==0x87{
                            let bank_name=parse_name(&mut sub)?;
                            let bank=lookup_path(ns,parent,&bank_name).map_err(|_|AmlError::InvalidParent)?;
                            bank_register=ns.resolve_alias(bank)?;
                            let start=sub.pos;skip_data_object(&mut sub)?;
                            bank_value_start=u32::try_from(start).map_err(|_|AmlError::BadDefinitionBlock)?;
                            bank_value_length=u32::try_from(sub.pos-start).map_err(|_|AmlError::BadDefinitionBlock)?;
                        }
                        let flags=sub.byte()?;
                        if flags&0xf8!=0||((flags>>1)&3)==3{return Err(AmlError::UnsupportedOpcode)}
                        let (region_node,mode)=if ext==0x81{
                            let found=lookup_path(ns,parent,&region).map_err(|_|AmlError::InvalidParent)?;
                            let id=ns.resolve_alias(found)?;
                            if ns.nodes[id as usize].object_type!=AmlObjectType::OperationRegion{return Err(AmlError::InvalidParent)}
                            (id,0)
                        }else if ext==0x87{
                            let found=lookup_path(ns,parent,&region).map_err(|_|AmlError::InvalidParent)?;
                            let id=ns.resolve_alias(found)?;
                            if ns.nodes[id as usize].object_type!=AmlObjectType::OperationRegion{return Err(AmlError::InvalidParent)}
                            (id,2)
                        }else{(u16::MAX,1)};
                        parse_field_list(&mut sub,ns,parent,region_node,mode,flags,index_register,data_register,bank_register,bank_value_start,bank_value_length)?;
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
                        match ext{
                            0x83=>{
                                ns.nodes[node as usize].processor_id=sub.byte()?;
                                ns.nodes[node as usize].processor_pblk_address=u32::from_le_bytes(sub.take(4)?.try_into().map_err(|_|AmlError::Truncated)?);
                                ns.nodes[node as usize].processor_pblk_length=sub.byte()?;
                            },
                            0x84=>{
                                ns.nodes[node as usize].power_resource_system_level=sub.byte()?;
                                ns.nodes[node as usize].power_resource_order=u16::from_le_bytes(sub.take(2)?.try_into().map_err(|_|AmlError::Truncated)?);
                            },
                            _=>{}
                        }
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
    load_definition_block_owned(aml,namespace,0)
}

/// Load one DSDT/SSDT with a stable table-owner ID. All namespace objects
/// introduced by this block carry the ID and can be removed independently.
pub fn load_definition_block_owned(aml:&[u8],namespace:&mut AmlNamespace,table_id:u32)->Result<(),AmlError>{
    // Namespace insertion is append-only. Restoring the original count on
    // failure makes each definition-block load atomic without copying the
    // large fixed-capacity namespace onto the kernel stack.
    let original_count=namespace.count;
    let original_owner=namespace.loading_table;
    namespace.loading_table=table_id;
    let mut c=Cursor{bytes:aml,pos:0,end:aml.len()};
    match parse_terms(&mut c,namespace,0,0){
        Ok(())=>{namespace.loading_table=original_owner;Ok(())},
        Err(error)=>{
            // Revert promotions of External placeholders in pre-existing
            // namespace nodes so a failed table load is truly atomic.
            for index in 1..original_count {
                let node=&mut namespace.nodes[index];
                if node.source_table==table_id && node.external_placeholder && node.external_owner!=table_id {
                    node.object_type=AmlObjectType::External;
                    node.source_table=node.external_owner;
                    node.method_flags=node.external_method_flags;
                    node.body_start=0;
                    node.body_length=0;
                }
            }
            namespace.count=original_count;namespace.loading_table=original_owner;Err(error)
        }
    }
}

/// Remove only objects owned by one definition block. Descendants whose
/// parent disappears are also removed; surviving aliases/fields are invalidated
/// if their referenced target belonged to the unloaded table.
pub fn unload_definition_block(namespace:&mut AmlNamespace,table_id:u32)->usize{
    let old_count=namespace.count;
    // A real declaration may temporarily satisfy an External placeholder
    // from an earlier table. When its owning table unloads, restore that
    // placeholder so the earlier table's namespace remains coherent.
    for index in 1..old_count {
        let node=&mut namespace.nodes[index];
        if node.source_table==table_id && node.external_placeholder && node.external_owner!=table_id {
            node.object_type=AmlObjectType::External;
            node.source_table=node.external_owner;
            node.method_flags=node.external_method_flags;
            node.body_start=0;
            node.body_length=0;
        }
    }
    let mut needed_as_scope=[false;MAX_NAMESPACE_NODES];
    // Any surviving object needs its Scope ancestors as namespace containers.
    for index in 1..old_count {
        if namespace.nodes[index].source_table!=table_id {
            let mut parent=namespace.nodes[index].parent as usize;
            while parent>0 && parent<old_count {
                needed_as_scope[parent]=true;
                parent=namespace.nodes[parent].parent as usize;
            }
        }
    }
    let mut remap=[u16::MAX;MAX_NAMESPACE_NODES];
    remap[0]=0;
    let mut write=1usize;
    for read in 1..old_count {
        let node=namespace.nodes[read];
        let parent=node.parent as usize;
        if parent>=old_count || remap[parent]==u16::MAX {continue}
        if node.source_table==table_id && !(node.object_type==AmlObjectType::Scope && needed_as_scope[read]) {continue}
        let mut kept=node;
        if kept.source_table==table_id {kept.source_table=0;}
        kept.parent=remap[parent];
        remap[read]=write as u16;
        namespace.nodes[write]=kept;
        write+=1;
    }
    for i in 1..write {
        let node=&mut namespace.nodes[i];
        if node.alias_target!=u16::MAX {
            let target=node.alias_target as usize;
            node.alias_target=if target<old_count {remap[target]} else {u16::MAX};
        }
        if node.field_region!=u16::MAX {
            let target=node.field_region as usize;
            node.field_region=if target<old_count {remap[target]} else {u16::MAX};
        }
    }
    namespace.count=write;
    old_count-write
}

/// Report the fixed namespace capacity and current free slots to callers so
/// firmware with unusually large namespaces can be diagnosed without logging.
pub const fn namespace_capacity()->usize{MAX_NAMESPACE_NODES}
pub fn namespace_remaining(namespace:&AmlNamespace)->usize{MAX_NAMESPACE_NODES.saturating_sub(namespace.count)}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn package_length_checks_all_width_boundaries_and_input_limit(){
        let mut one=[0u8;4];one[0]=0x04;
        assert_eq!(decode_package_length(&one,0,4),Ok((4,4)));
        let mut two=[0u8;17];two[0]=0x41;two[1]=1;
        assert_eq!(decode_package_length(&two,0,17),Ok((17,17)));
        let mut three=[0u8;16];three[0]=0x80;three[1]=1;
        assert_eq!(decode_package_length(&three,0,16),Ok((16,16)));
        let mut four=[0u8;16];four[0]=0xc0;four[1]=1;
        assert_eq!(decode_package_length(&four,0,16),Ok((16,16)));
        assert_eq!(decode_package_length(&[0x40],0,1),Err(AmlError::Truncated));
        assert_eq!(decode_package_length(&[0x41,0x01],0,1),Err(AmlError::Truncated));
        assert_eq!(decode_package_length(&[0x3f],0,1),Err(AmlError::BadPackageLength));
        assert_eq!(decode_package_length(&[0xc0,0xff,0xff,0x0f],0,4),Err(AmlError::BadPackageLength));
        assert_eq!(decode_package_length(&[0x41,0x01],0,usize::MAX),Err(AmlError::Truncated));
    }
    #[test]fn namestring_accepts_nullname_and_rejects_malformed_segments(){
        let (null,end)=parse_name_string(&[0x00],0,1).unwrap();assert_eq!(null.count,0);assert_eq!(end,1);
        assert!(parse_name_string(&[0x2e,b'A',b'B',b'C',b'D',b'E',b'F',b'G',b'H'],0,9).is_ok());
        assert!(parse_name_string(&[0x2f,1,b'A',b'B',b'C',b'D'],0,6).is_ok());
        assert_eq!(parse_name_string(&[0x2f,0],0,2),Err(AmlError::BadName));
        assert_eq!(parse_name_string(&[0x2f,9],0,2),Err(AmlError::BadName));
        assert_eq!(parse_name_string(&[0x5c,0x5e,b'A',b'B',b'C',b'D'],0,6),Err(AmlError::BadName));
        assert_eq!(parse_name_string(&[b'1',b'A',b'B',b'C'],0,4),Err(AmlError::BadName));
        assert_eq!(parse_name_string(&[b'A',b'B',b'?',b'D'],0,4),Err(AmlError::BadName));
    }
    #[test]fn external_forward_declaration_is_promoted_by_later_definition(){
        let aml=[0x15,b'M',b'E',b'T',b'0',8,0,0x14,0x06,b'M',b'E',b'T',b'0',0];
        let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
        let id=ns.find_child(0,*b"MET0").unwrap() as usize;
        assert_eq!(ns.nodes[id].object_type,AmlObjectType::Method);
        assert_eq!(ns.nodes[id].method_flags,0);
    }
    #[test]fn external_method_definition_is_owned_by_its_table_and_reverts_on_unload(){
        let mut ns=AmlNamespace::new();
        let external=[0x15,b'M',b'E',b'T',b'0',8,0];
        load_definition_block_owned(&external,&mut ns,1).unwrap();
        let id=ns.find_child(0,*b"MET0").unwrap() as usize;
        assert_eq!(ns.nodes[id].object_type,AmlObjectType::External);
        let method=[0x14,0x06,b'M',b'E',b'T',b'0',0];
        load_definition_block_owned(&method,&mut ns,2).unwrap();
        assert_eq!(ns.nodes[id].object_type,AmlObjectType::Method);
        assert_eq!(ns.nodes[id].source_table,2);
        assert_eq!(unload_definition_block(&mut ns,2),0);
        assert_eq!(ns.nodes[id].object_type,AmlObjectType::External);
        assert_eq!(ns.nodes[id].source_table,1);
        assert_eq!(unload_definition_block(&mut ns,1),1);
        assert!(ns.find_child(0,*b"MET0").is_none());
    }
    #[test]fn external_object_type_must_match_the_later_declaration(){
        let mut ns=AmlNamespace::new();
        load_definition_block_owned(&[0x15,b'M',b'E',b'T',b'0',8,0],&mut ns,1).unwrap();
        let device=[0x5b,0x82,0x0b,b'M',b'E',b'T',b'0',0x08,b'_',b'S',b'T',b'A',0x01];
        assert_eq!(load_definition_block_owned(&device,&mut ns,2),Err(AmlError::DuplicateName));
        let id=ns.find_child(0,*b"MET0").unwrap() as usize;
        assert_eq!(ns.nodes[id].object_type,AmlObjectType::External);
        assert_eq!(ns.nodes[id].source_table,1);
    }
    #[test]fn repeated_external_declarations_must_match_and_keep_original_owner(){
        let mut ns=AmlNamespace::new();
        load_definition_block_owned(&[0x15,b'M',b'E',b'T',b'0',8,1],&mut ns,1).unwrap();
        load_definition_block_owned(&[0x15,b'M',b'E',b'T',b'0',8,1],&mut ns,2).unwrap();
        let id=ns.find_child(0,*b"MET0").unwrap() as usize;
        assert_eq!(ns.nodes[id].external_owner,1);
        assert_eq!(load_definition_block_owned(&[0x15,b'M',b'E',b'T',b'0',8,2],&mut ns,3),Err(AmlError::DuplicateName));
        assert_eq!(ns.nodes[id].external_owner,1);
    }
    #[test]fn cross_table_duplicate_definitions_are_rejected_atomically(){
        let mut ns=AmlNamespace::new();
        load_definition_block_owned(&[0x08,b'V',b'A',b'L',b'0',0x01],&mut ns,1).unwrap();
        assert_eq!(load_definition_block_owned(&[0x08,b'V',b'A',b'L',b'0',0x00],&mut ns,2),Err(AmlError::DuplicateName));
        assert_eq!(ns.count,2);assert_eq!(ns.nodes[1].source_table,1);
    }
    #[test]fn failed_cross_table_definition_rolls_back_external_promotion(){
        let mut ns=AmlNamespace::new();
        load_definition_block_owned(&[0x15,b'M',b'E',b'T',b'0',8,0],&mut ns,1).unwrap();
        let malformed=[0x14,0x06,b'M',b'E',b'T',b'0',0,0x99];
        assert_eq!(load_definition_block_owned(&malformed,&mut ns,2),Err(AmlError::UnsupportedOpcode));
        let id=ns.find_child(0,*b"MET0").unwrap() as usize;
        assert_eq!(ns.nodes[id].object_type,AmlObjectType::External);
        assert_eq!(ns.nodes[id].source_table,1);
        assert_eq!(ns.count,2);
    }
    #[test]fn table_owner_ids_support_values_above_one_byte(){
        let mut ns=AmlNamespace::new();
        load_definition_block_owned(&[0x08,b'H',b'I',b'G',b'H',0x01],&mut ns,300).unwrap();
        assert_eq!(ns.nodes[1].source_table,300);
        assert_eq!(unload_definition_block(&mut ns,300),1);
        assert_eq!(ns.count,1);
    }
    #[test]fn scope_reopen_is_shared_across_table_owners(){
        let mut ns=AmlNamespace::new();
        load_definition_block_owned(&[0x10,0x0b,b'P',b'A',b'R',b'0',0x08,b'V',b'A',b'L',b'0',0x01],&mut ns,1).unwrap();
        load_definition_block_owned(&[0x10,0x0b,b'P',b'A',b'R',b'0',0x08,b'V',b'A',b'L',b'1',0x01],&mut ns,2).unwrap();
        let parent=ns.find_child(0,*b"PAR0").unwrap() as usize;
        assert_eq!(ns.nodes[parent].source_table,1);
        unload_definition_block(&mut ns,1);
        let parent=ns.find_child(0,*b"PAR0").unwrap();
        assert!(ns.find_child(parent,*b"VAL1").is_some());
        assert!(ns.find_child(parent,*b"VAL0").is_none());
    }
    #[test]fn unloading_one_table_preserves_unrelated_objects(){
        let mut ns=AmlNamespace::new();
        load_definition_block_owned(&[0x08,b'A',b'A',b'A',b'A',0x01,0x06,b'A',b'A',b'A',b'A',b'A',b'L',b'I',b'1'],&mut ns,1).unwrap();
        load_definition_block_owned(&[0x08,b'B',b'B',b'B',b'B',0x01],&mut ns,2).unwrap();
        assert_eq!(unload_definition_block(&mut ns,1),2);
        assert!(ns.find_child(0,*b"AAAA").is_none());assert!(ns.find_child(0,*b"ALI1").is_none());assert!(ns.find_child(0,*b"BBBB").is_some());
    }
    #[test]fn reports_namespace_capacity_explicitly(){
        let mut ns=AmlNamespace::new();ns.count=MAX_NAMESPACE_NODES;
        assert_eq!(ns.insert(0,*b"FULL",AmlObjectType::Name,0,0,0),Err(AmlError::NamespaceCapacity{capacity:MAX_NAMESPACE_NODES,requested:MAX_NAMESPACE_NODES+1}));
        assert_eq!(namespace_capacity(),MAX_NAMESPACE_NODES);assert_eq!(namespace_remaining(&ns),0);
    }
    #[test]fn integer_constants_follow_definition_block_revision_width(){
        assert_eq!(decode_integer_constant(&[0x00],0,1,1),Ok((AmlInteger{value:0,width_bits:32},1)));
        assert_eq!(decode_integer_constant(&[0x01],0,1,1),Ok((AmlInteger{value:1,width_bits:32},1)));
        assert_eq!(decode_integer_constant(&[0xff],0,1,1),Ok((AmlInteger{value:u32::MAX as u64,width_bits:32},1)));
        assert_eq!(decode_integer_constant(&[0xff],0,1,2),Ok((AmlInteger{value:u64::MAX,width_bits:64},1)));
        assert_eq!(decode_integer_constant(&[0x0a,0xfe],0,2,2),Ok((AmlInteger{value:254,width_bits:64},2)));
        assert_eq!(decode_integer_constant(&[0x0b,0x34,0x12],0,3,2),Ok((AmlInteger{value:0x1234,width_bits:64},3)));
        assert_eq!(decode_integer_constant(&[0x0c,1,2,3,4],0,5,1),Ok((AmlInteger{value:0x04030201,width_bits:32},5)));
        assert_eq!(decode_integer_constant(&[0x0e,1,2,3,4,5,6,7,8],0,9,2),Ok((AmlInteger{value:0x0807060504030201,width_bits:64},9)));
        assert_eq!(decode_integer_constant(&[0x0e,1,2,3,4,5,6,7,8],0,9,1),Err(AmlError::UnsupportedOpcode));
        assert_eq!(decode_integer_constant(&[0x0c,1,2],0,3,2),Err(AmlError::Truncated));
        assert_eq!(decode_integer_constant(&[0xff],0,2,2),Err(AmlError::Truncated));
    }
    #[test]fn package_length_decodes_all_widths(){
        assert_eq!(decode_package_length(&[0x3f;63],0,63),Ok((63,63)));
        assert_eq!(decode_package_length(&[0x41,0x01,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],0,17),Ok((17,17)));
        assert_eq!(decode_package_length(&[0x80,1,0,0,0,0,0,0,0,0,0,0,0,0,0,0],0,16),Ok((16,16)));
    }
    #[test]fn parses_root_parent_dual_and_multi_name_strings(){
        let (rooted,end)=parse_name_string(&[0x5c,0x2e,b'_',b'S',b'B',b'_',b'P',b'C',b'I',b'0'],0,10).unwrap();
        assert!(rooted.rooted);assert_eq!(rooted.count,2);assert_eq!(end,10);
        let (parent,end)=parse_name_string(&[0x5e,b'P',b'C',b'I',b'0'],0,5).unwrap();
        assert_eq!(parent.parents,1);assert_eq!(parent.segments[0],*b"PCI0");assert_eq!(end,5);
        let (dual,end)=parse_name_string(&[0x2e,b'P',b'C',b'I',b'0',b'D',b'E',b'V',b'0'],0,9).unwrap();
        assert_eq!(dual.count,2);assert_eq!(end,9);
        let (multi,end)=parse_name_string(&[0x2f,2,b'P',b'C',b'I',b'0',b'D',b'E',b'V',b'0'],0,10).unwrap();
        assert_eq!(multi.count,2);assert_eq!(end,10);
    }
    #[test]fn loads_device_and_name_namespace(){
        // Device(PCI0) { Name(_STA, One) } -- a simple integer-valued namespace object.
        let aml=[0x5b,0x82,0x0b,b'P',b'C',b'I',b'0',0x08,b'_',b'S',b'T',b'A',0x01];
        let mut ns=AmlNamespace::new();
        assert_eq!(load_definition_block(&aml,&mut ns),Ok(()));
        assert_eq!(ns.find_child(0,*b"PCI0"),Some(1));
        assert_eq!(ns.find_child(1,*b"_STA"),Some(2));
    }
    #[test]fn resolves_unprefixed_names_through_ancestor_scopes(){
        let aml=[0x10,0x17,b'P',b'A',b'R',b'0',0x08,b'V',b'A',b'L',b'0',0x01,
                 0x10,0x0b,b'C',b'H',b'D',b'0',0x08,b'V',b'A',b'L',b'1',0x01];
        let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
        let parent=ns.find_child(0,*b"PAR0").unwrap();let child=ns.find_child(parent,*b"CHD0").unwrap();
        let name=AmlName{segments:[*b"VAL0",[0;4],[0;4],[0;4],[0;4],[0;4],[0;4],[0;4]],count:1,rooted:false,parents:0};
        assert_eq!(resolve_name_string(&ns,child,&name),Ok(ns.find_child(parent,*b"VAL0").unwrap()));
    }
    #[test]fn parses_processor_powerresource_and_thermalzone_fixed_prefixes(){
        let aml=[
            0x5b,0x83,0x0b,b'C',b'P',b'U',b'0',0x42,0x78,0x56,0x34,0x12,0x06,
            0x5b,0x84,0x08,b'P',b'W',b'R',b'0',3,0x67,0x45,
            0x5b,0x85,0x05,b'T',b'Z',b'0',b'_'
        ];
        let mut ns=AmlNamespace::new();assert!(load_definition_block(&aml,&mut ns).is_ok());
        assert_eq!(ns.nodes[1].object_type,AmlObjectType::Processor);assert_eq!(ns.nodes[1].processor_id,0x42);assert_eq!(ns.nodes[1].processor_pblk_address,0x12345678);assert_eq!(ns.nodes[1].processor_pblk_length,6);
        assert_eq!(ns.nodes[2].object_type,AmlObjectType::PowerResource);assert_eq!(ns.nodes[2].power_resource_system_level,3);assert_eq!(ns.nodes[2].power_resource_order,0x4567);
        assert_eq!(ns.nodes[3].object_type,AmlObjectType::ThermalZone);
    }
    #[test]fn zero_width_reserved_field_does_not_corrupt_following_offsets(){
        let aml=[0x5b,0x80,b'R',b'E',b'G',b'N',0,0x0a,0,0x0a,16,
            0x5b,0x81,0x0d,b'R',b'E',b'G',b'N',0,0x00,0x00,b'F',b'L',b'D',b'0',8];
        let mut ns=AmlNamespace::new();assert_eq!(load_definition_block(&aml,&mut ns),Ok(()));
        let field=ns.find_child(0,*b"FLD0").unwrap() as usize;
        assert_eq!(ns.nodes[field].body_start,0);assert_eq!(ns.nodes[field].body_length,8);
    }
    #[test]fn rejects_zero_length_and_malformed_access_fields(){
        let mut ns=AmlNamespace::new();
        // OperationRegion(REGN, SystemMemory, 0, 16); Field(REGN, flags, FLD0, 0)
        let zero=[0x5b,0x80,b'R',b'E',b'G',b'N',0,0x0a,0,0x0a,16,
            0x5b,0x81,0x0b,b'R',b'E',b'G',b'N',0,b'F',b'L',b'D',b'0',0];
        assert_eq!(load_definition_block(&zero,&mut ns),Err(AmlError::BadPackageLength));
        assert_eq!(ns.count,1);
        // AccessField with an out-of-range access attribute.
        let access=[0x5b,0x80,b'R',b'E',b'G',b'N',0,0x0a,0,0x0a,16,
            0x5b,0x81,0x0e,b'R',b'E',b'G',b'N',0,0x01,0x00,0xff,b'F',b'L',b'D',b'0',8];
        assert_eq!(load_definition_block(&access,&mut ns),Err(AmlError::UnsupportedOpcode));
    }
    #[test]fn parses_index_and_bank_field_declarations(){
        let aml=[
            0x5b,0x80,b'R',b'E',b'G',b'N',0,0x0a,0,0x0a,16,
            0x5b,0x80,b'I',b'N',b'D',b'X',1,0x0a,0,0x0a,1,
            0x5b,0x80,b'D',b'A',b'T',b'A',1,0x0a,0,0x0a,1,
            0x5b,0x86,0x18,b'I',b'N',b'D',b'X',b'D',b'A',b'T',b'A',0,0x03,0x00,0x0c,0x01,0x02,b'I',b'N',b'D',b'X',b'F',b'L',b'D',b'0',8,
            0x5b,0x87,0x11,b'R',b'E',b'G',b'N',b'I',b'N',b'D',b'X',0x0a,1,0,b'B',b'F',b'L',b'D',8
        ];
        let mut ns=AmlNamespace::new();assert_eq!(load_definition_block(&aml,&mut ns),Ok(()));
        assert_eq!(ns.nodes[4].object_type,AmlObjectType::Field);assert_eq!(ns.nodes[4].field_mode,1);assert_eq!(ns.nodes[4].field_index_register,2);assert_eq!(ns.nodes[4].field_data_register,3);assert!(ns.nodes[4].field_connection);assert_eq!(ns.nodes[4].field_connection_length,4);assert_eq!(ns.nodes[4].field_access_attribute,0x0c);assert_eq!(ns.nodes[4].field_access_length,1);
        assert_eq!(ns.nodes[5].object_type,AmlObjectType::Field);assert_eq!(ns.nodes[5].field_mode,2);assert_eq!(ns.nodes[5].field_region,1);assert_eq!(ns.nodes[5].field_bank_register,2);assert_eq!(ns.nodes[5].field_bank_value_length,2);
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
        assert_eq!(ns.nodes[2].field_region,1);
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
    #[test]fn rejects_truncated_unary_terms_and_restores_namespace(){
        // Not has an operand and a target; the target is missing.
        let not_missing_target=[0x08,b'B',b'A',b'D',b'2',0x11,0x04,
            0x80,0x0a,1];
        let mut ns=AmlNamespace::new();
        assert_eq!(load_definition_block(&not_missing_target,&mut ns),Err(AmlError::Truncated));
        assert_eq!(ns.count,1);

        // FindSetLeftBit is also a unary value plus target operation.
        let bit_scan_missing_target=[0x08,b'B',b'A',b'D',b'3',0x11,0x04,
            0x81,0x0a,1];
        assert_eq!(load_definition_block(&bit_scan_missing_target,&mut ns),Err(AmlError::Truncated));
        assert_eq!(ns.count,1);
    }

    #[test]fn rejects_malformed_package_counts_and_truncated_buffer_size(){
        // Package(2) has only one element: its payload must end exactly at
        // the package boundary and cannot silently accept missing elements.
        let package_count=[0x08,b'B',b'A',b'D',b'4',0x12,0x03,2,0x01];
        let mut ns=AmlNamespace::new();
        assert_eq!(load_definition_block(&package_count,&mut ns),Err(AmlError::Truncated));
        assert_eq!(ns.count,1);
        // Buffer package with a truncated DWord BufferSize expression.
        let buffer=[0x08,b'B',b'A',b'D',b'5',0x11,0x04,0x0c,1,2];
        assert_eq!(load_definition_block(&buffer,&mut ns),Err(AmlError::Truncated));
        assert_eq!(ns.count,1);
        // VarPackage requires a count TermArg within its own package.
        let varpackage=[0x08,b'B',b'A',b'D',b'6',0x13,0x02];
        assert_eq!(load_definition_block(&varpackage,&mut ns),Err(AmlError::Truncated));
        assert_eq!(ns.count,1);
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


    #[test]fn rejects_parent_prefixes_that_walk_above_namespace_root(){
        let ns=AmlNamespace::new();
        let name=AmlName{segments:[[0;4];MAX_NAME_SEGMENTS],count:0,rooted:false,parents:1};
        assert_eq!(resolve_name_string(&ns,0,&name),Err(AmlError::InvalidParent));
    }
    #[test]fn explicit_root_and_parent_paths_do_not_search_other_ancestors(){
        let aml=[0x10,0x17,b'P',b'A',b'R',b'0',0x08,b'V',b'A',b'L',b'0',0x01,
                 0x10,0x0b,b'C',b'H',b'D',b'0',0x08,b'V',b'A',b'L',b'1',0x01];
        let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
        let child=ns.find_child(ns.find_child(0,*b"PAR0").unwrap(),*b"CHD0").unwrap();
        let rooted=AmlName{segments:[*b"PAR0",*b"VAL0",[0;4],[0;4],[0;4],[0;4],[0;4],[0;4]],count:2,rooted:true,parents:0};
        assert_eq!(resolve_name_string(&ns,child,&rooted),Ok(ns.find_child(ns.find_child(0,*b"PAR0").unwrap(),*b"VAL0").unwrap()));
        let missing=AmlName{segments:[*b"MISS",[0;4],[0;4],[0;4],[0;4],[0;4],[0;4],[0;4]],count:1,rooted:false,parents:1};
        assert_eq!(resolve_name_string(&ns,child,&missing),Err(AmlError::InvalidParent));
    }
    #[test]fn alias_resolves_to_existing_canonical_object(){
        // Name(VAL0, One); Alias(VAL0, ALI0)
        let aml=[0x08,b'V',b'A',b'L',b'0',0x01,
                 0x06,b'V',b'A',b'L',b'0',b'A',b'L',b'I',b'0'];
        let mut ns=AmlNamespace::new();
        load_definition_block(&aml,&mut ns).unwrap();
        let alias=ns.find_child(0,*b"ALI0").unwrap();
        let target=ns.find_child(0,*b"VAL0").unwrap();
        assert_eq!(ns.resolve_alias(alias),Ok(target));
        assert_eq!(ns.nodes[alias as usize].alias_target,target);
    }
    #[test]fn alias_rejects_missing_source_without_mutating_namespace(){
        let aml=[0x06,b'M',b'I',b'S',b'S',b'A',b'L',b'I',b'0'];
        let mut ns=AmlNamespace::new();
        assert_eq!(load_definition_block(&aml,&mut ns),Err(AmlError::InvalidParent));
        assert_eq!(ns.count,1);
    }
    #[test]fn alias_cycle_is_rejected(){
        let mut ns=AmlNamespace::new();
        let a=ns.insert(0,*b"ALIA",AmlObjectType::Alias,0,0,0).unwrap();
        let b=ns.insert(0,*b"ALIB",AmlObjectType::Alias,0,0,0).unwrap();
        ns.nodes[a as usize].alias_target=b;ns.nodes[b as usize].alias_target=a;
        assert_eq!(ns.resolve_alias(a),Err(AmlError::BadDefinitionBlock));
    }


    #[test]fn validates_not_and_dereference_terms_inside_buffer_sizes(){
        // Buffer(Not(ByteConst(0), NullName)) { 0xaa }
        let aml=[0x08,b'B',b'U',b'F',b'2',0x11,0x06,
            0x80,0x0a,0,0x00,0xaa];
        let mut ns=AmlNamespace::new();
        assert!(load_definition_block(&aml,&mut ns).is_ok());
        assert_eq!(ns.find_child(0,*b"BUF2"),Some(1));

        // DerefOf(NullName) is structurally parsed but remains unsupported
        // by the scalar executor until reference semantics are implemented.
        let deref=[0x08,b'B',b'U',b'F',b'3',0x11,0x06,
            0x83,0x00,0xaa,0,0];
        let mut ns=AmlNamespace::new();
        assert!(load_definition_block(&deref,&mut ns).is_ok());
    }

    #[test]fn validates_modulo_and_find_set_bit_inside_buffer_size_terms(){
        // Name(BUF0, Buffer(Mod(23, 5, NullName)) { 0xaa })
        let modulo=[0x08,b'B',b'U',b'F',b'0',0x11,0x08,
            0x85,0x0a,23,0x0a,5,0x00,0xaa];
        let mut ns=AmlNamespace::new();
        assert!(load_definition_block(&modulo,&mut ns).is_ok());

        // Name(BUF1, Buffer(FindSetLeftBit(DWordConst(0x8000), NullName)) { 0xaa })
        let bit_scan=[0x08,b'B',b'U',b'F',b'1',0x11,0x09,
            0x81,0x0c,0x00,0x80,0x00,0x00,0x00,0xaa];
        let mut ns=AmlNamespace::new();
        assert!(load_definition_block(&bit_scan,&mut ns).is_ok());
    }

}
