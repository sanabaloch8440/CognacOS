//! AML-5 field access and synchronization primitives.
//! Field I/O is delegated to the ACPI address-space layer. This module never
//! maps firmware addresses or performs privileged I/O itself.
#![no_std]

use core::sync::atomic::{AtomicBool,AtomicU16,AtomicU64,Ordering};
use crate::aml::{AmlNamespace,AmlObjectType};

pub const MAX_FIELD_BITS:u32=64;
pub const DEFAULT_SPIN_LIMIT:u32=100_000;

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum FieldError {
    InvalidNode,NotField,UnsupportedAccess,FieldTooWide,InvalidLength,
    AddressOverflow,IoFailure,LockFailure,UnlockFailure,
}
fn access_bytes(access:u8)->Result<u8,FieldError>{
    match access {0|1=>Ok(1),2=>Ok(2),3=>Ok(4),4=>Ok(8),_=>Err(FieldError::UnsupportedAccess)}
}
fn unit_mask(width:u8)->u64{if width==8{u64::MAX}else{(1u64<<(width as u32*8))-1}}

/// A region-scoped interface implemented by the ACPI platform address-space
/// dispatcher. Width is in bytes and must be one of 1, 2, 4, or 8.
pub trait FieldIo {
    fn read(&mut self,region_node:u16,byte_offset:u64,width:u8)->Result<u64,()>;
    fn write(&mut self,region_node:u16,byte_offset:u64,width:u8,value:u64)->Result<(),()>;
    /// Implementations may acquire/release the ACPI Global Lock here.
    fn begin_locked_access(&mut self)->Result<(),()>{Err(())}
    fn end_locked_access(&mut self)->Result<(),()>{Err(())}
}

fn field_node(ns:&AmlNamespace,id:u16)->Result<&crate::aml::AmlNode,FieldError>{
    let n=ns.nodes.get(id as usize).filter(|_|(id as usize)<ns.count).ok_or(FieldError::InvalidNode)?;
    if n.object_type!=AmlObjectType::Field{return Err(FieldError::NotField)}
    if n.field_mode!=0||n.field_connection{return Err(FieldError::UnsupportedAccess)}
    if n.field_region==u16::MAX{return Err(FieldError::InvalidNode)}
    if n.body_length==0{return Err(FieldError::InvalidLength)}
    if n.body_length>MAX_FIELD_BITS{return Err(FieldError::FieldTooWide)}
    Ok(n)
}

fn read_unlocked<I:FieldIo>(n:&crate::aml::AmlNode,io:&mut I)->Result<u64,FieldError>{
    if n.field_access_attribute!=0||n.field_access_length!=0{return Err(FieldError::UnsupportedAccess)}
    let width=access_bytes(n.field_access_type)?;
    let unit_bits=width as u64*8;
    let start=n.body_start as u64;
    let end=start.checked_add(n.body_length as u64).ok_or(FieldError::AddressOverflow)?;
    let mut result=0u64;
    let mut cached_unit=u64::MAX;
    let mut cached_value=0u64;
    for bit in start..end{
        let unit=(bit/unit_bits)*width as u64;
        if unit!=cached_unit{
            cached_value=io.read(n.field_region,unit,width).map_err(|_|FieldError::IoFailure)? & unit_mask(width);
            cached_unit=unit;
        }
        let within=(bit%unit_bits) as u32;
        let value=(cached_value>>within)&1;
        result|=value<<((bit-start) as u32);
    }
    Ok(result)
}

fn write_unlocked<I:FieldIo>(n:&crate::aml::AmlNode,value:u64,io:&mut I)->Result<(),FieldError>{
    if n.field_access_attribute!=0||n.field_access_length!=0{return Err(FieldError::UnsupportedAccess)}
    let width=access_bytes(n.field_access_type)?;
    let unit_bits=width as u64*8;
    let start=n.body_start as u64;
    let end=start.checked_add(n.body_length as u64).ok_or(FieldError::AddressOverflow)?;
    let first=(start/unit_bits)*width as u64;
    let last=((end-1)/unit_bits)*width as u64;
    let mut unit=first;
    while unit<=last{
        let unit_start=unit;
        let overlap_start=start.max(unit_start);
        let overlap_end=end.min(unit_start+unit_bits);
        let shift=(overlap_start-unit_start) as u32;
        let count=(overlap_end-overlap_start) as u32;
        let field_shift=(overlap_start-start) as u32;
        let low_mask=if count==64{u64::MAX}else{(1u64<<count)-1};
        let mask=if count==64{u64::MAX}else{low_mask<<shift};
        let incoming=(value>>field_shift)&low_mask;
        let incoming=if count==64{incoming}else{incoming<<shift};
        let old=match n.field_update_rule{
            0=>io.read(n.field_region,unit,width).map_err(|_|FieldError::IoFailure)? & unit_mask(width),
            1=>unit_mask(width),
            2=>0,
            _=>return Err(FieldError::UnsupportedAccess),
        };
        let new_value=((old&!mask)|(incoming&mask))&unit_mask(width);
        io.write(n.field_region,unit,width,new_value).map_err(|_|FieldError::IoFailure)?;
        unit=unit.checked_add(width as u64).ok_or(FieldError::AddressOverflow)?;
    }
    Ok(())
}

pub fn read_field<I:FieldIo>(ns:&AmlNamespace,id:u16,io:&mut I)->Result<u64,FieldError>{
    let n=field_node(ns,id)?;
    if n.field_lock_rule!=0{io.begin_locked_access().map_err(|_|FieldError::LockFailure)?;}
    let result=read_unlocked(n,io);
    if n.field_lock_rule!=0&&io.end_locked_access().is_err(){return Err(FieldError::UnlockFailure)}
    result
}
pub fn write_field<I:FieldIo>(ns:&AmlNamespace,id:u16,value:u64,io:&mut I)->Result<(),FieldError>{
    let n=field_node(ns,id)?;
    if n.field_lock_rule!=0{io.begin_locked_access().map_err(|_|FieldError::LockFailure)?;}
    let result=write_unlocked(n,value,io);
    if n.field_lock_rule!=0&&io.end_locked_access().is_err(){return Err(FieldError::UnlockFailure)}
    result
}

/// AML Mutex with owner identity, recursive acquisition, sync-level checking,
/// and bounded spinning. Thread IDs must be nonzero and stable for a thread.
pub struct AmlMutex {
    owner:AtomicU64,
    recursion:AtomicU16,
    sync_level:u8,
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum SyncError { InvalidOwner,InvalidObject,Timeout,NotOwner,SyncLevelViolation,RecursionOverflow }
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct MutexToken { previous_sync_level:u8 }
impl AmlMutex {
    pub const fn new(sync_level:u8)->Self{Self{owner:AtomicU64::new(0),recursion:AtomicU16::new(0),sync_level}}
    pub fn from_namespace(ns:&AmlNamespace,node:u16)->Result<Self,SyncError>{
        let n=ns.nodes.get(node as usize).filter(|_|(node as usize)<ns.count).ok_or(SyncError::InvalidObject)?;
        if n.object_type!=AmlObjectType::Mutex||n.method_flags>15{return Err(SyncError::InvalidObject)}
        Ok(Self::new(n.method_flags))
    }
    pub fn sync_level(&self)->u8{self.sync_level}
    pub fn acquire(&self,thread_id:u64,current_sync_level:&mut u8,spin_limit:u32)->Result<MutexToken,SyncError>{
        if thread_id==0{return Err(SyncError::InvalidOwner)}
        if self.sync_level<*current_sync_level{return Err(SyncError::SyncLevelViolation)}
        let previous=*current_sync_level;
        if self.owner.load(Ordering::Acquire)==thread_id{
            let old=self.recursion.load(Ordering::Relaxed);
            if old==u16::MAX{return Err(SyncError::RecursionOverflow)}
            self.recursion.store(old+1,Ordering::Relaxed);
            *current_sync_level=self.sync_level;
            return Ok(MutexToken{previous_sync_level:previous});
        }
        for _ in 0..spin_limit{
            if self.owner.compare_exchange(0,thread_id,Ordering::Acquire,Ordering::Relaxed).is_ok(){
                self.recursion.store(1,Ordering::Relaxed);
                *current_sync_level=self.sync_level;
                return Ok(MutexToken{previous_sync_level:previous});
            }
            core::hint::spin_loop();
        }
        Err(SyncError::Timeout)
    }
    pub fn release(&self,thread_id:u64,current_sync_level:&mut u8,token:MutexToken)->Result<(),SyncError>{
        if thread_id==0||self.owner.load(Ordering::Acquire)!=thread_id{return Err(SyncError::NotOwner)}
        let depth=self.recursion.load(Ordering::Relaxed);
        if depth==0{return Err(SyncError::NotOwner)}
        if depth>1{self.recursion.store(depth-1,Ordering::Relaxed);}
        else{self.recursion.store(0,Ordering::Relaxed);self.owner.store(0,Ordering::Release);}
        *current_sync_level=token.previous_sync_level;
        Ok(())
    }
}

/// AML Event is a binary synchronization event. Signal coalesces repeated
/// signals; Wait consumes one pending signal or times out after a bounded spin.
pub struct AmlEvent { signaled:AtomicBool }
impl AmlEvent {
    pub const fn new()->Self{Self{signaled:AtomicBool::new(false)}}
    pub fn from_namespace(ns:&AmlNamespace,node:u16)->Result<Self,SyncError>{
        let n=ns.nodes.get(node as usize).filter(|_|(node as usize)<ns.count).ok_or(SyncError::InvalidObject)?;
        if n.object_type!=AmlObjectType::Event{return Err(SyncError::InvalidObject)}
        Ok(Self::new())
    }
    pub fn signal(&self){self.signaled.store(true,Ordering::Release);}
    pub fn reset(&self){self.signaled.store(false,Ordering::Release);}
    pub fn wait(&self,spin_limit:u32)->Result<(),SyncError>{
        for _ in 0..spin_limit{
            if self.signaled.compare_exchange(true,false,Ordering::AcqRel,Ordering::Acquire).is_ok(){return Ok(())}
            core::hint::spin_loop();
        }
        Err(SyncError::Timeout)
    }
}
impl Default for AmlEvent{fn default()->Self{Self::new()}}

#[cfg(test)]
mod tests{
    use super::*;
    use crate::aml::load_definition_block;

    struct Memory{bytes:[u8;32],locked:bool}
    impl FieldIo for Memory{
        fn read(&mut self,_:u16,offset:u64,width:u8)->Result<u64,()>{let mut v=0u64;for i in 0..width as usize{v|=(self.bytes[offset as usize+i] as u64)<<(i*8)}Ok(v)}
        fn write(&mut self,_:u16,offset:u64,width:u8,value:u64)->Result<(),()>{for i in 0..width as usize{self.bytes[offset as usize+i]=(value>>(i*8))as u8}Ok(())}
        fn begin_locked_access(&mut self)->Result<(),()>{if self.locked{return Err(())}self.locked=true;Ok(())}
        fn end_locked_access(&mut self)->Result<(),()>{self.locked=false;Ok(())}
    }

    #[test]
    fn field_read_write_crosses_access_units_without_touching_neighbors(){
        let aml=[
            0x5b,0x80,b'R',b'E',b'G',b'N',0,0x0a,0,0x0a,16,
            0x5b,0x81,0x0d,b'R',b'E',b'G',b'N',0,0x00,0x07,b'W',b'I',b'D',b'E',0x0a
        ];
        let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
        let id=ns.find_child(0,*b"WIDE").unwrap();
        let mut mem=Memory{bytes:[0;32],locked:false};
        mem.bytes[0]=0x80;mem.bytes[1]=0x55;mem.bytes[2]=0x81;
        assert_eq!(read_field(&ns,id,&mut mem),Ok(0x2ab));
        assert_eq!(write_field(&ns,id,0x155,&mut mem),Ok(()));
        assert_eq!(mem.bytes[0],0x80);
        assert_eq!(mem.bytes[1],0xaa);
        assert_eq!(mem.bytes[2],0x80);
    }

    #[test]
    fn mutex_enforces_owner_recursion_and_sync_level(){
        let m=AmlMutex::new(3);let mut level=1;
        let a=m.acquire(11,&mut level,8).unwrap();assert_eq!(level,3);
        let b=m.acquire(11,&mut level,8).unwrap();
        assert_eq!(m.release(12,&mut level,b),Err(SyncError::NotOwner));
        m.release(11,&mut level,b).unwrap();assert_eq!(level,3);
        m.release(11,&mut level,a).unwrap();assert_eq!(level,1);
        assert_eq!(m.acquire(12,&mut 4,1),Err(SyncError::SyncLevelViolation));
    }
    #[test]
    fn event_is_binary_and_wait_consumes_signal(){
        let e=AmlEvent::new();assert_eq!(e.wait(1),Err(SyncError::Timeout));
        e.signal();e.signal();assert_eq!(e.wait(1),Ok(()));assert_eq!(e.wait(1),Err(SyncError::Timeout));
    }
    #[test]
    fn field_read_write_preserves_adjacent_bits(){
        // OperationRegion(REGN,SystemMemory,0,8); Field(REGN,Preserve){Offset? FOO:5}
        let aml=[0x5b,0x80,b'R',b'E',b'G',b'N',0,0x0a,0,0x0a,8,
                 0x5b,0x81,0x0e,b'R',b'E',b'G',b'N',0x01,0x01,0x00,b'F',b'O',b'O',b'_',0x05];
        let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
        let id=ns.find_child(0,*b"FOO_").unwrap();
        let mut mem=Memory{bytes:[0b1011_0100;32],locked:false};
        assert_eq!(read_field(&ns,id,&mut mem),Ok(0b10100));
        assert_eq!(write_field(&ns,id,0b00101,&mut mem),Ok(()));
        assert_eq!(mem.bytes[0]&0b11111,0b00101);
        assert_eq!(mem.bytes[0]&0b1110_0000,0b1010_0000);
        assert!(!mem.locked);
    }
}
