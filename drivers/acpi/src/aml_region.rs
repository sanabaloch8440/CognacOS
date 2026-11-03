//! AML-4 OperationRegion decoding and bounded address-space dispatch.
//! This layer never dereferences firmware-provided addresses. All accesses
//! are delegated to an explicitly installed platform handler (ACPI.sys layer).
#![no_std]

use crate::aml::{AmlNamespace,AmlObjectType};

pub const MAX_OPERATION_REGIONS:usize=128;

pub const SPACE_SYSTEM_MEMORY:u8=0x00;
pub const SPACE_SYSTEM_IO:u8=0x01;
pub const SPACE_PCI_CONFIG:u8=0x02;
pub const SPACE_EMBEDDED_CONTROLLER:u8=0x03;
pub const SPACE_SMBUS:u8=0x04;
pub const SPACE_SYSTEM_CMOS:u8=0x05;
pub const SPACE_PCI_BAR_TARGET:u8=0x06;
pub const SPACE_IPMI:u8=0x07;
pub const SPACE_GPIO:u8=0x08;
pub const SPACE_GENERIC_SERIAL_BUS:u8=0x09;
pub const SPACE_PLATFORM_COMMUNICATION_CHANNEL:u8=0x0a;
pub const SPACE_FUNCTIONAL_FIXED_HARDWARE:u8=0x7f;

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum RegionError {
    InvalidNode,NotOperationRegion,MalformedTermArg,UnsupportedTermArg,
    ZeroLength,AddressOverflow,RegionTableFull,DuplicateRegion,
    RegionNotFound,NoHandler,AccessWidth,AccessOutOfBounds,HandlerFailure,
}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct OperationRegion {
    pub node:u16,
    pub space_id:u8,
    pub base:u64,
    pub length:u64,
}
impl OperationRegion {
    pub const EMPTY:Self=Self{node:u16::MAX,space_id:0,base:0,length:0};
/// Compute the exclusive end of an OperationRegion with overflow checking.
    pub fn end_exclusive(&self)->Result<u64,RegionError>{
        self.base.checked_add(self.length).ok_or(RegionError::AddressOverflow)
    }
}

/// Decode the constant integer subset of AML TermArg. Dynamic NameString,
/// method-call and arithmetic operands are deliberately deferred until the
/// AML interpreter can evaluate them in the correct namespace/method scope.
fn decode_integer(bytes:&[u8],pos:&mut usize,end:usize)->Result<u64,RegionError>{
    let op=*bytes.get(*pos).filter(|_|*pos<end).ok_or(RegionError::MalformedTermArg)?;
    *pos+=1;
    match op {
        0x00=>Ok(0),
        0x01=>Ok(1),
        0xff=>Ok(u64::MAX),
        0x0a=>{
            let b=*bytes.get(*pos).filter(|_|*pos<end).ok_or(RegionError::MalformedTermArg)?;
            *pos+=1;Ok(b as u64)
        }
        0x0b=>{
            let next=(*pos).checked_add(2).ok_or(RegionError::MalformedTermArg)?;
            let s=bytes.get(*pos..next).filter(|_|next<=end).ok_or(RegionError::MalformedTermArg)?;
            *pos=next;Ok(u16::from_le_bytes([s[0],s[1]]) as u64)
        }
        0x0c=>{
            let next=(*pos).checked_add(4).ok_or(RegionError::MalformedTermArg)?;
            let s=bytes.get(*pos..next).filter(|_|next<=end).ok_or(RegionError::MalformedTermArg)?;
            *pos=next;Ok(u32::from_le_bytes([s[0],s[1],s[2],s[3]]) as u64)
        }
        0x0e=>{
            let next=(*pos).checked_add(8).ok_or(RegionError::MalformedTermArg)?;
            let s=bytes.get(*pos..next).filter(|_|next<=end).ok_or(RegionError::MalformedTermArg)?;
            *pos=next;Ok(u64::from_le_bytes(s.try_into().map_err(|_|RegionError::MalformedTermArg)?))
        }
        _=>Err(RegionError::UnsupportedTermArg),
    }
}

/// Resolve one OperationRegion declaration from its retained AML byte range.
/// The AML parser stores the address-space ID in method_flags and the two
/// TermArgs (base and length) in body_start/body_length.
pub fn decode_operation_region(aml:&[u8],namespace:&AmlNamespace,node:u16)
    ->Result<OperationRegion,RegionError>{
    let n=namespace.nodes.get(node as usize).filter(|_|(node as usize)<namespace.count)
        .ok_or(RegionError::InvalidNode)?;
    if n.object_type!=AmlObjectType::OperationRegion{return Err(RegionError::NotOperationRegion)}
    let start=n.body_start as usize;
    let end=start.checked_add(n.body_length as usize).ok_or(RegionError::MalformedTermArg)?;
    if end>aml.len(){return Err(RegionError::MalformedTermArg)}
    let mut pos=start;
    let base=decode_integer(aml,&mut pos,end)?;
    let length=decode_integer(aml,&mut pos,end)?;
    if pos!=end{return Err(RegionError::MalformedTermArg)}
    if length==0{return Err(RegionError::ZeroLength)}
    let region=OperationRegion{node,space_id:n.method_flags,base,length};
    let _=region.end_exclusive()?;
    Ok(region)
}

#[derive(Clone,Copy)]
pub struct OperationRegionTable {
    entries:[OperationRegion;MAX_OPERATION_REGIONS],
    count:usize,
}
impl OperationRegionTable {
    pub const fn new()->Self{Self{entries:[OperationRegion::EMPTY;MAX_OPERATION_REGIONS],count:0}}
/// Return the number of registered OperationRegion objects.
    pub fn count(&self)->usize{self.count}
/// Find an OperationRegion by its AML namespace node identifier.
    pub fn get(&self,node:u16)->Option<&OperationRegion>{
        self.entries.iter().take(self.count).find(|r|r.node==node)
    }
/// Register one validated region while rejecting duplicate nodes and table exhaustion.
    pub fn register(&mut self,region:OperationRegion)->Result<(),RegionError>{
        if self.get(region.node).is_some(){return Err(RegionError::DuplicateRegion)}
        if self.count==MAX_OPERATION_REGIONS{return Err(RegionError::RegionTableFull)}
        if region.length==0{return Err(RegionError::ZeroLength)}
        let _=region.end_exclusive()?;
        self.entries[self.count]=region;self.count+=1;Ok(())
    }
/// Decode and register one namespace OperationRegion declaration.
    pub fn load_node(&mut self,aml:&[u8],namespace:&AmlNamespace,node:u16)
        ->Result<(),RegionError>{
        let region=decode_operation_region(aml,namespace,node)?;
        self.register(region)
    }
    /// Load every OperationRegion in a namespace atomically. A malformed or
    /// unsupported declaration rolls back the table to its prior count.
    pub fn load_namespace(&mut self,aml:&[u8],namespace:&AmlNamespace)
        ->Result<(),RegionError>{
        let original=self.count;
        for index in 1..namespace.count {
            if namespace.nodes[index].object_type==AmlObjectType::OperationRegion {
                if let Err(error)=self.load_node(aml,namespace,index as u16) {
                    self.count=original;
                    return Err(error);
                }
            }
        }
        Ok(())
    }
/// Find a registered region that fully contains an access in the requested address space.
    pub fn find_containing(&self,space_id:u8,address:u64,width:u8)
        ->Result<&OperationRegion,RegionError>{
        validate_width(width)?;
        let access_end=address.checked_add(width as u64).ok_or(RegionError::AddressOverflow)?;
        self.entries.iter().take(self.count)
            .find(|r|r.space_id==space_id&&address>=r.base&&r.base.checked_add(r.length).is_some_and(|end|access_end<=end))
            .ok_or(RegionError::RegionNotFound)
    }
}
impl Default for OperationRegionTable{fn default()->Self{Self::new()}}

fn width_mask(width:u8)->u64{if width==8{u64::MAX}else{(1u64<<(width as u32*8))-1}}

fn validate_width(width:u8)->Result<(),RegionError>{
    match width{1|2|4|8=>Ok(()),_=>Err(RegionError::AccessWidth)}
}

/// Platform-specific address-space implementation. Implementations must
/// validate their own hardware aperture and mapping policy. In particular,
/// SystemMemory must use the kernel's mapped-I/O/MMIO manager, SystemIO must
/// use privileged port-I/O primitives, and PCI_Config must route through the
/// PCI configuration manager rather than treating its address as a pointer.
pub trait AddressSpaceHandler {
    fn supports(&self,space_id:u8)->bool;
    fn read(&mut self,space_id:u8,address:u64,width:u8)->Result<u64,()>;
    fn write(&mut self,space_id:u8,address:u64,width:u8,value:u64)->Result<(),()>;
}

pub fn read_region<H:AddressSpaceHandler>(table:&OperationRegionTable,node:u16,
    offset:u64,width:u8,handler:&mut H)->Result<u64,RegionError>{
    validate_width(width)?;
    let region=table.get(node).ok_or(RegionError::RegionNotFound)?;
    let relative_end=offset.checked_add(width as u64).ok_or(RegionError::AddressOverflow)?;
    if relative_end>region.length{return Err(RegionError::AccessOutOfBounds)}
    let address=region.base.checked_add(offset).ok_or(RegionError::AddressOverflow)?;
    if !handler.supports(region.space_id){return Err(RegionError::NoHandler)}
    handler.read(region.space_id,address,width).map(|value|value & width_mask(width)).map_err(|_|RegionError::HandlerFailure)
}

pub fn write_region<H:AddressSpaceHandler>(table:&OperationRegionTable,node:u16,
    offset:u64,width:u8,value:u64,handler:&mut H)->Result<(),RegionError>{
    validate_width(width)?;
    let region=table.get(node).ok_or(RegionError::RegionNotFound)?;
    let relative_end=offset.checked_add(width as u64).ok_or(RegionError::AddressOverflow)?;
    if relative_end>region.length{return Err(RegionError::AccessOutOfBounds)}
    let address=region.base.checked_add(offset).ok_or(RegionError::AddressOverflow)?;
    if !handler.supports(region.space_id){return Err(RegionError::NoHandler)}
    handler.write(region.space_id,address,width,value & width_mask(width)).map_err(|_|RegionError::HandlerFailure)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aml::load_definition_block;

    struct MockHandler { space:u8, address:u64, value:u64, width:u8 }
    impl AddressSpaceHandler for MockHandler {
        fn supports(&self,space_id:u8)->bool{space_id==self.space}
        fn read(&mut self,_:u8,address:u64,width:u8)->Result<u64,()>{self.address=address;self.width=width;Ok(self.value)}
        fn write(&mut self,_:u8,address:u64,width:u8,value:u64)->Result<(),()>{self.address=address;self.width=width;self.value=value;Ok(())}
    }

    #[test]
    fn decodes_system_memory_region_and_bounds_accesses(){
        // OperationRegion(REGN, SystemMemory, 0x1000, 0x20)
        let aml=[0x5b,0x80,b'R',b'E',b'G',b'N',0x00,0x0c,0x00,0x10,0,0,0,0x0a,0x20];
        let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
        let id=ns.find_child(0,*b"REGN").unwrap();
        let mut table=OperationRegionTable::new();table.load_node(&aml,&ns,id).unwrap();
        let mut h=MockHandler{space:0,address:0,value:0x1234,width:0};
        assert_eq!(read_region(&table,id,4,4,&mut h),Ok(0x1234));
        assert_eq!(h.address,0x1004);assert_eq!(h.width,4);
        assert_eq!(write_region(&table,id,8,2,0xabcd,&mut h),Ok(()));
        assert_eq!(h.address,0x1008);assert_eq!(h.value,0xabcd);
        assert_eq!(write_region(&table,id,0,1,0x1234,&mut h),Ok(()));
        assert_eq!(h.value,0x34);
        assert_eq!(read_region(&table,id,0x1f,2,&mut h),Err(RegionError::AccessOutOfBounds));
    }

    #[test]
    fn supports_system_io_only_through_installed_handler(){
        // OperationRegion(IO__, SystemIO, 0x3f8, 8)
        let aml=[0x5b,0x80,b'I',b'O',b'_',b'_',0x01,0x0b,0xf8,0x03,0x0a,8];
        let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
        let id=ns.find_child(0,*b"IO__").unwrap();
        let mut table=OperationRegionTable::new();table.load_node(&aml,&ns,id).unwrap();
        let mut h=MockHandler{space:1,address:0,value:0,width:0};
        assert_eq!(read_region(&table,id,0,1,&mut h),Ok(0));
        let mut wrong=MockHandler{space:0,address:0,value:0,width:0};
        assert_eq!(read_region(&table,id,0,1,&mut wrong),Err(RegionError::NoHandler));
    }

    #[test]
    fn rejects_dynamic_operands_until_interpreter_can_evaluate_them(){
        // A NameString operand is syntactically valid AML but is not a
        // constant and must not be mistaken for an address.
        let aml=[0x5b,0x80,b'R',b'E',b'G',b'N',0x00,b'B',b'A',b'S',b'E',0x0a,8];
        let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
        let id=ns.find_child(0,*b"REGN").unwrap();
        assert_eq!(decode_operation_region(&aml,&ns,id),Err(RegionError::UnsupportedTermArg));
    }

    #[test]
    fn namespace_load_is_transactional_when_later_region_is_invalid(){
        let aml=[
            0x5b,0x80,b'R',b'E',b'G',b'1',0x00,0x0a,0x10,0x0a,8,
            0x5b,0x80,b'R',b'E',b'G',b'2',0x00,0x0a,0x20,0x00,0x00
        ];
        let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
        let mut table=OperationRegionTable::new();
        assert_eq!(table.load_namespace(&aml,&ns),Err(RegionError::ZeroLength));
        assert_eq!(table.count(),0);
    }

    #[test]
    fn rejects_zero_length_overflow_and_unsupported_width(){
        let mut table=OperationRegionTable::new();
        assert_eq!(table.register(OperationRegion{node:1,space_id:0,base:0,length:0}),Err(RegionError::ZeroLength));
        assert_eq!(table.register(OperationRegion{node:1,space_id:0,base:u64::MAX,length:2}),Err(RegionError::AddressOverflow));
        assert_eq!(validate_width(3),Err(RegionError::AccessWidth));
    }
}
