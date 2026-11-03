//! AML-5 field access and synchronization primitives.
//! Field I/O is delegated to the ACPI address-space layer. This module never
//! maps firmware addresses or performs privileged I/O itself.
/*
 * FIXME(ACPI-PRODUCTION) — AML Field and BankField access
 * FIXME(ACPI-PRODUCTION): Complete bit-accurate Field/IndexField/BankField access, access attributes, lock/update rules, bank selection, connection handling, and alignment semantics. Check every bit-range calculation and preserve the Global Lock/interpreter lock ordering contract.
 */

#![no_std]

use core::sync::atomic::{AtomicBool,AtomicU16,AtomicU64,Ordering};
use crate::aml::{AmlNamespace,AmlObjectType};

pub const MAX_FIELD_BITS:u32=4096;
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
    /// Read one access unit through an AML IndexField pair. The backend must
    /// serialize index-register selection with the matching data-register read.
    fn read_indexed(&mut self,_index_register:u16,_data_register:u16,_byte_offset:u64,_width:u8)->Result<u64,()>{Err(())}
    /// Write one access unit through an AML IndexField pair. The backend must
    /// serialize index-register selection with the matching data-register write.
    fn write_indexed(&mut self,_index_register:u16,_data_register:u16,_byte_offset:u64,_width:u8,_value:u64)->Result<(),()>{Err(())}
    /// Select the bank for a BankField before accessing its underlying region.
    fn select_bank(&mut self,_bank_register:u16,_bank_value:u64)->Result<(),()>{Err(())}
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
        let unit_start=unit.checked_mul(8).ok_or(FieldError::AddressOverflow)?;
        let unit_end=unit_start.checked_add(unit_bits).ok_or(FieldError::AddressOverflow)?;
        let overlap_start=start.max(unit_start);
        let overlap_end=end.min(unit_end);
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

/// Read a bounded AML Field through the supplied OperationRegion I/O contract.
pub fn read_field<I:FieldIo>(ns:&AmlNamespace,id:u16,io:&mut I)->Result<u64,FieldError>{
    let n=field_node(ns,id)?;
    if n.field_lock_rule!=0{io.begin_locked_access().map_err(|_|FieldError::LockFailure)?;}
    let result=read_unlocked(n,io);
    if n.field_lock_rule!=0&&io.end_locked_access().is_err(){return Err(FieldError::UnlockFailure)}
    result
}
/// Write a bounded AML Field through the supplied OperationRegion I/O contract.
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
/// Return the AML Mutex synchronization level encoded by the namespace object.
    pub fn sync_level(&self)->u8{self.sync_level}
/// Acquire the AML Mutex while enforcing owner, recursion, sync-level, and bounded-wait rules.
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
/// Release an AML Mutex token and restore the caller's previous synchronization level.
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
/// Set the AML Event signaled state with release ordering.
    pub fn signal(&self){self.signaled.store(true,Ordering::Release);}
/// Clear the AML Event signaled state with release ordering.
    pub fn reset(&self){self.signaled.store(false,Ordering::Release);}
/// Wait for an AML Event using a bounded non-blocking spin policy.
    pub fn wait(&self,spin_limit:u32)->Result<(),SyncError>{
        for _ in 0..spin_limit{
            if self.signaled.compare_exchange(true,false,Ordering::AcqRel,Ordering::Acquire).is_ok(){return Ok(())}
            core::hint::spin_loop();
        }
        Err(SyncError::Timeout)
    }
}
impl Default for AmlEvent{fn default()->Self{Self::new()}}


/// Validate an IndexField declaration and return its namespace metadata.
fn index_field_node(ns:&AmlNamespace,id:u16)->Result<&crate::aml::AmlNode,FieldError>{
    let n=ns.nodes.get(id as usize).filter(|_|(id as usize)<ns.count).ok_or(FieldError::InvalidNode)?;
    if n.object_type!=AmlObjectType::Field{return Err(FieldError::NotField)}
    if n.field_mode!=1||n.field_connection{return Err(FieldError::UnsupportedAccess)}
    if n.field_index_register==u16::MAX||n.field_data_register==u16::MAX{return Err(FieldError::InvalidNode)}
    if n.body_length==0{return Err(FieldError::InvalidLength)}
    if n.body_length>MAX_FIELD_BITS{return Err(FieldError::FieldTooWide)}
    Ok(n)
}
/// Read an IndexField through a backend-serialized index/data register operation.
pub fn read_index_field<I:FieldIo>(ns:&AmlNamespace,id:u16,io:&mut I)->Result<u64,FieldError>{
    let n=index_field_node(ns,id)?;
    let width=access_bytes(n.field_access_type)?;
    if n.field_access_attribute!=0||n.field_access_length!=0{return Err(FieldError::UnsupportedAccess)}
    if n.field_lock_rule!=0{io.begin_locked_access().map_err(|_|FieldError::LockFailure)?;}
    let result=(||{
        let unit_bits=width as u64*8;let start=n.body_start as u64;
        let end=start.checked_add(n.body_length as u64).ok_or(FieldError::AddressOverflow)?;
        let mut value=0u64;let mut cached_unit=u64::MAX;let mut cached=0u64;
        for bit in start..end{
            let unit=(bit/unit_bits)*width as u64;
            if unit!=cached_unit{cached=io.read_indexed(n.field_index_register,n.field_data_register,unit,width)
                .map_err(|_|FieldError::IoFailure)?&unit_mask(width);cached_unit=unit;}
            value|=((cached>>((bit%unit_bits) as u32))&1)<<((bit-start) as u32);
        }
        Ok(value)
    })();
    if n.field_lock_rule!=0&&io.end_locked_access().is_err(){return Err(FieldError::UnlockFailure)}
    result
}
/// Write an IndexField while honoring its update rule and access-unit boundaries.
pub fn write_index_field<I:FieldIo>(ns:&AmlNamespace,id:u16,value:u64,io:&mut I)->Result<(),FieldError>{
    let n=index_field_node(ns,id)?;
    let width=access_bytes(n.field_access_type)?;
    if n.field_access_attribute!=0||n.field_access_length!=0{return Err(FieldError::UnsupportedAccess)}
    if n.field_lock_rule!=0{io.begin_locked_access().map_err(|_|FieldError::LockFailure)?;}
    let result=(||{
        let unit_bits=width as u64*8;let start=n.body_start as u64;
        let end=start.checked_add(n.body_length as u64).ok_or(FieldError::AddressOverflow)?;
        let mut unit=(start/unit_bits)*width as u64;let last=((end-1)/unit_bits)*width as u64;
        while unit<=last{
            let unit_start=unit.checked_mul(8).ok_or(FieldError::AddressOverflow)?;
            let unit_end=unit_start.checked_add(unit_bits).ok_or(FieldError::AddressOverflow)?;
            let a=start.max(unit_start);let b=end.min(unit_end);let shift=(a-unit_start) as u32;
            let count=(b-a) as u32;let source=(a-start) as u32;
            let low=if count==64{u64::MAX}else{(1u64<<count)-1};
            let mask=if count==64{u64::MAX}else{low<<shift};
            let bits=(value>>source)&low;let bits=if count==64{bits}else{bits<<shift};
            let old=match n.field_update_rule{
                0=>io.read_indexed(n.field_index_register,n.field_data_register,unit,width)
                    .map_err(|_|FieldError::IoFailure)?&unit_mask(width),
                1=>unit_mask(width),2=>0,_=>return Err(FieldError::UnsupportedAccess),
            };
            io.write_indexed(n.field_index_register,n.field_data_register,unit,width,
                ((old&!mask)|(bits&mask))&unit_mask(width)).map_err(|_|FieldError::IoFailure)?;
            unit=unit.checked_add(width as u64).ok_or(FieldError::AddressOverflow)?;
        }
        Ok(())
    })();
    if n.field_lock_rule!=0&&io.end_locked_access().is_err(){return Err(FieldError::UnlockFailure)}
    result
}
fn bank_field_node(ns:&AmlNamespace,id:u16)->Result<&crate::aml::AmlNode,FieldError>{
    let n=ns.nodes.get(id as usize).filter(|_|(id as usize)<ns.count).ok_or(FieldError::InvalidNode)?;
    if n.object_type!=AmlObjectType::Field{return Err(FieldError::NotField)}
    if n.field_mode!=2||n.field_connection{return Err(FieldError::UnsupportedAccess)}
    if n.field_region==u16::MAX||n.field_bank_register==u16::MAX{return Err(FieldError::InvalidNode)}
    if n.body_length==0{return Err(FieldError::InvalidLength)}
    if n.body_length>MAX_FIELD_BITS{return Err(FieldError::FieldTooWide)}
    Ok(n)
}
fn decode_bank_value(aml:&[u8],n:&crate::aml::AmlNode)->Result<u64,FieldError>{
    let start=n.field_bank_value_start as usize;
    let end=start.checked_add(n.field_bank_value_length as usize).filter(|end|*end<=aml.len()).ok_or(FieldError::InvalidLength)?;
    if start==end{return Err(FieldError::InvalidLength)}
    let (value,used)=crate::aml_exec::evaluate_term_argument(&aml[start..end]).map_err(|_|FieldError::InvalidLength)?;
    if used!=end-start{return Err(FieldError::InvalidLength)}Ok(value)
}
/// Read a BankField after evaluating its BankFieldValue TermArg and selecting the bank.
pub fn read_bank_field<I:FieldIo>(aml:&[u8],ns:&AmlNamespace,id:u16,io:&mut I)->Result<u64,FieldError>{
    let n=bank_field_node(ns,id)?;let bank=decode_bank_value(aml,n)?;
    if n.field_lock_rule!=0{io.begin_locked_access().map_err(|_|FieldError::LockFailure)?;}
    let result=io.select_bank(n.field_bank_register,bank).map_err(|_|FieldError::IoFailure)
        .and_then(|_|read_unlocked(n,io));
    if n.field_lock_rule!=0&&io.end_locked_access().is_err(){return Err(FieldError::UnlockFailure)}
    result
}
/// Write a BankField after selecting its firmware-defined bank.
pub fn write_bank_field<I:FieldIo>(aml:&[u8],ns:&AmlNamespace,id:u16,value:u64,io:&mut I)->Result<(),FieldError>{
    let n=bank_field_node(ns,id)?;let bank=decode_bank_value(aml,n)?;
    if n.field_lock_rule!=0{io.begin_locked_access().map_err(|_|FieldError::LockFailure)?;}
    let result=io.select_bank(n.field_bank_register,bank).map_err(|_|FieldError::IoFailure)
        .and_then(|_|write_unlocked(n,value,io));
    if n.field_lock_rule!=0&&io.end_locked_access().is_err(){return Err(FieldError::UnlockFailure)}
    result
}
/// Unified mode-aware Field access for Field, IndexField and BankField declarations.
pub fn read_field_with_aml<I:FieldIo>(aml:&[u8],ns:&AmlNamespace,id:u16,io:&mut I)->Result<u64,FieldError>{
    let n=ns.nodes.get(id as usize).filter(|_|(id as usize)<ns.count).ok_or(FieldError::InvalidNode)?;
    match n.field_mode{0=>read_field(ns,id,io),1=>read_index_field(ns,id,io),2=>read_bank_field(aml,ns,id,io),_=>Err(FieldError::UnsupportedAccess)}
}
/// Unified mode-aware Field write dispatch.
pub fn write_field_with_aml<I:FieldIo>(aml:&[u8],ns:&AmlNamespace,id:u16,value:u64,io:&mut I)->Result<(),FieldError>{
    let n=ns.nodes.get(id as usize).filter(|_|(id as usize)<ns.count).ok_or(FieldError::InvalidNode)?;
    match n.field_mode{0=>write_field(ns,id,value,io),1=>write_index_field(ns,id,value,io),2=>write_bank_field(aml,ns,id,value,io),_=>Err(FieldError::UnsupportedAccess)}
}

/// Read a bounded Field of up to MAX_FIELD_BITS into a caller-owned little-endian bit buffer.
/// This supports AML fields wider than an Integer while retaining strict output bounds.
pub fn read_field_buffer<I:FieldIo>(ns:&AmlNamespace,id:u16,io:&mut I,output:&mut [u8])->Result<usize,FieldError>{
    let n=field_node(ns,id)?;
    let required=(n.body_length as usize+7)/8;
    if output.len()<required{return Err(FieldError::InvalidLength)}
    if n.field_access_attribute!=0||n.field_access_length!=0{return Err(FieldError::UnsupportedAccess)}
    let width=access_bytes(n.field_access_type)?;
    let unit_bits=width as u64*8;
    let start=n.body_start as u64;
    let end=start.checked_add(n.body_length as u64).ok_or(FieldError::AddressOverflow)?;
    let operation=||{
        output[..required].fill(0);
        let mut cached_unit=u64::MAX;let mut cached_value=0u64;
        for bit in start..end{
            let unit=(bit/unit_bits)*width as u64;
            if unit!=cached_unit{
                cached_value=io.read(n.field_region,unit,width).map_err(|_|FieldError::IoFailure)?&unit_mask(width);
                cached_unit=unit;
            }
            let value=(cached_value>>((bit%unit_bits) as u32))&1;
            let relative=(bit-start) as usize;
            if value!=0{output[relative/8]|=1<<(relative%8);}
        }
        Ok(required)
    };
    if n.field_lock_rule!=0{io.begin_locked_access().map_err(|_|FieldError::LockFailure)?;}
    let result=operation();
    if n.field_lock_rule!=0&&io.end_locked_access().is_err(){return Err(FieldError::UnlockFailure)}
    result
}

/// Write a bounded little-endian bit buffer into a Field wider than an AML Integer.
/// Uncovered bits in each access unit follow the Field's declared update rule.
pub fn write_field_buffer<I:FieldIo>(ns:&AmlNamespace,id:u16,input:&[u8],io:&mut I)->Result<(),FieldError>{
    let n=field_node(ns,id)?;
    let required=(n.body_length as usize+7)/8;
    if input.len()<required{return Err(FieldError::InvalidLength)}
    if n.field_access_attribute!=0||n.field_access_length!=0{return Err(FieldError::UnsupportedAccess)}
    let width=access_bytes(n.field_access_type)?;
    let unit_bits=width as u64*8;
    let start=n.body_start as u64;
    let end=start.checked_add(n.body_length as u64).ok_or(FieldError::AddressOverflow)?;
    let operation=||{
        let first=(start/unit_bits)*width as u64;
        let last=((end-1)/unit_bits)*width as u64;
        let mut unit=first;
        while unit<=last{
            let unit_start=unit.checked_mul(8).ok_or(FieldError::AddressOverflow)?;
            let unit_end=unit_start.checked_add(unit_bits).ok_or(FieldError::AddressOverflow)?;
            let overlap_start=start.max(unit_start);let overlap_end=end.min(unit_end);
            let shift=(overlap_start-unit_start) as u32;let count=(overlap_end-overlap_start) as u32;
            let field_shift=(overlap_start-start) as usize;
            let low_mask=if count==64{u64::MAX}else{(1u64<<count)-1};
            let mask=if count==64{u64::MAX}else{low_mask<<shift};
            let mut incoming=0u64;
            for bit in 0..count as usize{
                let source=field_shift+bit;
                if input[source/8]&(1<<(source%8))!=0{incoming|=1u64<<bit;}
            }
            let incoming=if count==64{incoming}else{incoming<<shift};
            let old=match n.field_update_rule{
                0=>io.read(n.field_region,unit,width).map_err(|_|FieldError::IoFailure)?&unit_mask(width),
                1=>unit_mask(width),2=>0,_=>return Err(FieldError::UnsupportedAccess),
            };
            io.write(n.field_region,unit,width,((old&!mask)|(incoming&mask))&unit_mask(width))
                .map_err(|_|FieldError::IoFailure)?;
            unit=unit.checked_add(width as u64).ok_or(FieldError::AddressOverflow)?;
        }
        Ok(())
    };
    if n.field_lock_rule!=0{io.begin_locked_access().map_err(|_|FieldError::LockFailure)?;}
    let result=operation();
    if n.field_lock_rule!=0&&io.end_locked_access().is_err(){return Err(FieldError::UnlockFailure)}
    result
}
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
    fn sync_objects_are_created_from_namespace_definitions(){
        let aml=[0x5b,0x01,b'M',b'T',b'X',b'0',5,
                 0x5b,0x02,b'E',b'V',b'T',b'0'];
        let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
        let mutex_id=ns.find_child(0,*b"MTX0").unwrap();
        let event_id=ns.find_child(0,*b"EVT0").unwrap();
        assert_eq!(AmlMutex::from_namespace(&ns,mutex_id).unwrap().sync_level(),5);
        assert!(AmlEvent::from_namespace(&ns,event_id).is_ok());
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
                 0x5b,0x81,0x0d,b'R',b'E',b'G',b'N',0x01,0x00,0x00,b'F',b'O',b'O',b'_',0x05];
        let mut ns=AmlNamespace::new();load_definition_block(&aml,&mut ns).unwrap();
        let id=ns.find_child(0,*b"FOO_").unwrap();
        let mut mem=Memory{bytes:[0b1011_0100;32],locked:false};
        assert_eq!(read_field(&ns,id,&mut mem),Ok(0b10100));
        assert_eq!(write_field(&ns,id,0b00101,&mut mem),Ok(()));
        assert_eq!(mem.bytes[0]&0b11111,0b00101);
        assert_eq!(mem.bytes[0]&0b1110_0000,0b1010_0000);
        assert!(!mem.locked);
    }

    struct CompositeMemory { value:u64, bank:u64, indexed_reads:u32, indexed_writes:u32 }
    impl FieldIo for CompositeMemory {
        fn read(&mut self,_:u16,_:u64,_:u8)->Result<u64,()>{Ok(self.value)}
        fn write(&mut self,_:u16,_:u64,_:u8,value:u64)->Result<(),()>{self.value=value;Ok(())}
        fn read_indexed(&mut self,index:u16,data:u16,offset:u64,_:u8)->Result<u64,()>{
            if index!=2||data!=3||offset!=0{return Err(())} self.indexed_reads+=1;Ok(self.value)
        }
        fn write_indexed(&mut self,index:u16,data:u16,offset:u64,_:u8,value:u64)->Result<(),()>{
            if index!=2||data!=3||offset!=0{return Err(())} self.indexed_writes+=1;self.value=value;Ok(())
        }
        fn select_bank(&mut self,register:u16,value:u64)->Result<(),()>{if register!=4{return Err(())}self.bank=value;Ok(())}
    }

    #[test]
    fn index_field_read_and_write_use_index_data_pair(){
        let mut ns=AmlNamespace::new();
        let mut node=crate::aml::AmlNode::ROOT;
        node.object_type=AmlObjectType::Field;node.field_mode=1;node.field_index_register=2;
        node.field_data_register=3;node.body_length=8;node.field_access_type=0;
        ns.nodes[1]=node;ns.count=2;
        let mut io=CompositeMemory{value:0x5a,bank:0,indexed_reads:0,indexed_writes:0};
        assert_eq!(read_index_field(&ns,1,&mut io),Ok(0x5a));
        assert_eq!(write_index_field(&ns,1,0xa5,&mut io),Ok(()));
        assert_eq!(io.value,0xa5);assert_eq!(io.indexed_reads,1);assert_eq!(io.indexed_writes,1);
    }

    #[test]
    fn bank_field_selects_firmware_bank_before_region_access(){
        let aml=[0x0a,0x02];
        let mut ns=AmlNamespace::new();
        let mut node=crate::aml::AmlNode::ROOT;
        node.object_type=AmlObjectType::Field;node.field_mode=2;node.field_region=5;
        node.field_bank_register=4;node.field_bank_value_start=0;node.field_bank_value_length=2;
        node.body_length=8;node.field_access_type=0;
        ns.nodes[1]=node;ns.count=2;
        let mut io=CompositeMemory{value:0x6c,bank:0,indexed_reads:0,indexed_writes:0};
        assert_eq!(read_field_with_aml(&aml,&ns,1,&mut io),Ok(0x6c));
        assert_eq!(io.bank,2);
        assert_eq!(write_field_with_aml(&aml,&ns,1,0xc6,&mut io),Ok(()));
        assert_eq!(io.bank,2);assert_eq!(io.value,0xc6);
    }





    #[test]
    fn wide_field_buffer_round_trips_more_than_one_aml_integer(){
        let mut ns=AmlNamespace::new();
        let mut node=crate::aml::AmlNode::ROOT;
        node.object_type=AmlObjectType::Field;node.field_region=5;node.body_start=8;
        node.body_length=80;node.field_access_type=0;node.field_update_rule=0;
        ns.nodes[1]=node;ns.count=2;
        let mut mem=Memory{bytes:[0x33;32],locked:false};
        let mut output=[0u8;10];
        assert_eq!(read_field_buffer(&ns,1,&mut mem,&mut output),Ok(10));
        assert_eq!(output,[0x33;10]);
        let input=[0xa5;10];
        assert_eq!(write_field_buffer(&ns,1,&input,&mut mem),Ok(()));
        assert_eq!(&mem.bytes[1..11],&input);
        assert_eq!(mem.bytes[0],0x33);assert_eq!(mem.bytes[11],0x33);
    }

}
