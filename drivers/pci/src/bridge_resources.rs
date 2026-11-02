//! PCI-to-PCI bridge resource windows and transactional rebalance primitives.
//! These routines decode firmware-programmed Type-1 windows, intersect them
//! with host apertures, and stage bridge register changes with rollback.
#![no_std]

use crate::resources::{BarKind, ConfigAccess, ResourceError, PCI_COMMAND, PCI_COMMAND_IO, PCI_COMMAND_MEMORY, PCI_COMMAND_BUS_MASTER};
use crate::resource_manager::{Aperture, OccupiedRange};

pub const BRIDGE_IO_BASE_LIMIT:u16=0x1c;
pub const BRIDGE_MEMORY_BASE_LIMIT:u16=0x20;
pub const BRIDGE_PREFETCH_BASE_LIMIT:u16=0x24;
pub const BRIDGE_PREFETCH_BASE_UPPER:u16=0x28;
pub const BRIDGE_PREFETCH_LIMIT_UPPER:u16=0x2c;
pub const BRIDGE_IO_BASE_UPPER:u16=0x30;
pub const BRIDGE_IO_LIMIT_UPPER:u16=0x32;

#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub struct BridgeWindows {
    pub io:Option<Aperture>,
    pub memory:Option<Aperture>,
    pub prefetchable:Option<Aperture>,
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum BridgeWindowError { Config, InvalidEncoding, Overflow, NoIntersection, Capacity }

fn aperture(start:u64,end_inclusive:u64,kind:BarKind,prefetchable:bool)->Result<Option<Aperture>,BridgeWindowError>{
    if end_inclusive<start{return Ok(None)}
    let length=end_inclusive.checked_sub(start).and_then(|v|v.checked_add(1)).ok_or(BridgeWindowError::Overflow)?;
    Ok(Some(Aperture{start,length,kind,prefetchable}))
}

/// Decode the standard Type-1 I/O, non-prefetchable memory and prefetchable
/// memory windows. Disabled windows are returned as None; malformed encodings
/// are rejected instead of being treated as address zero.
pub fn decode_bridge_windows<C:ConfigAccess>(c:&mut C)->Result<BridgeWindows,BridgeWindowError>{
    let io=c.read_u32(BRIDGE_IO_BASE_LIMIT).map_err(|_|BridgeWindowError::Config)?;
    let io_upper=c.read_u32(BRIDGE_IO_BASE_UPPER).map_err(|_|BridgeWindowError::Config)?;
    let io_type=io&0x0101;
    if (io_type&1)!=((io_type>>8)&1){return Err(BridgeWindowError::InvalidEncoding)}
    let io_32=io_type&1!=0;
    let io_base=((io&0xf0) as u64)<<8 | if io_32{((io_upper&0xffff) as u64)<<16}else{0};
    let io_limit=(((io>>8)&0xf0) as u64)<<8 | if io_32{((io_upper>>16) as u64)<<16}else{0} | 0xfff;
    let io_window=if io_base<=io_limit{aperture(io_base,io_limit,BarKind::Io,false)?}else{None};

    let mem=c.read_u32(BRIDGE_MEMORY_BASE_LIMIT).map_err(|_|BridgeWindowError::Config)?;
    let mem_base=((mem&0x0000fff0) as u64)<<16;
    let mem_limit=(((mem>>16)&0x0000fff0) as u64)<<16 | 0xfffff;
    let memory=if mem_base<=mem_limit{aperture(mem_base,mem_limit,BarKind::Memory32,false)?}else{None};

    let pref=c.read_u32(BRIDGE_PREFETCH_BASE_LIMIT).map_err(|_|BridgeWindowError::Config)?;
    let pref_type=pref&0x0001_0001;
    if (pref_type&1)!=((pref_type>>16)&1){return Err(BridgeWindowError::InvalidEncoding)}
    let pref_64=pref_type&1!=0;
    let pref_upper_base=c.read_u32(BRIDGE_PREFETCH_BASE_UPPER).map_err(|_|BridgeWindowError::Config)? as u64;
    let pref_upper_limit=c.read_u32(BRIDGE_PREFETCH_LIMIT_UPPER).map_err(|_|BridgeWindowError::Config)? as u64;
    let pref_base=(((pref&0x0000fff0) as u64)<<16) | if pref_64{pref_upper_base<<32}else{0};
    let pref_limit=((((pref>>16)&0x0000fff0) as u64)<<16) | if pref_64{pref_upper_limit<<32}else{0} | 0xfffff;
    let prefetchable=if pref_base<=pref_limit{aperture(pref_base,pref_limit,BarKind::Memory64,true)?}else{None};
    Ok(BridgeWindows{io:io_window,memory,prefetchable})
}

/// Intersect every host aperture with each ancestor bridge's corresponding
/// window. No aperture is invented, and empty intersections are discarded.
pub fn intersect_path(host:&[Aperture],ancestors:&[BridgeWindows],out:&mut[Aperture])->Result<usize,BridgeWindowError>{
    let mut staged=[Aperture{start:0,length:0,kind:BarKind::Io,prefetchable:false};64];
    if host.len()>staged.len(){return Err(BridgeWindowError::Capacity)}
    staged[..host.len()].copy_from_slice(host);
    let mut count=host.len();
    for bridge in ancestors {
        let mut next=[Aperture{start:0,length:0,kind:BarKind::Io,prefetchable:false};64];
        let mut n=0;
        for current in staged[..count].iter().copied(){
            let candidate=match current.kind{
                BarKind::Io=>bridge.io,
                BarKind::Memory32=>bridge.memory,
                BarKind::Memory64=>bridge.prefetchable,
            };
            let Some(window)=candidate else{continue};
            if current.kind!=window.kind || (current.kind!=BarKind::Io&&current.prefetchable!=window.prefetchable){continue}
            let a_end=current.start.checked_add(current.length).ok_or(BridgeWindowError::Overflow)?;
            let b_end=window.start.checked_add(window.length).ok_or(BridgeWindowError::Overflow)?;
            let start=current.start.max(window.start);let end=a_end.min(b_end);
            if start<end {
                if n==next.len(){return Err(BridgeWindowError::Capacity)}
                next[n]=Aperture{start,length:end-start,kind:current.kind,prefetchable:current.prefetchable};n+=1;
            }
        }
        staged=next;count=n;
        if count==0{return Err(BridgeWindowError::NoIntersection)}
    }
    if out.len()<count{return Err(BridgeWindowError::Capacity)}
    out[..count].copy_from_slice(&staged[..count]);Ok(count)
}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct BridgeWindowRegister { pub offset:u16,pub value:u32 }
pub const MAX_BRIDGE_WINDOW_REGISTERS:usize=7;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct BridgeWindowUpdate { pub registers:[Option<BridgeWindowRegister>;MAX_BRIDGE_WINDOW_REGISTERS],pub count:usize }
impl BridgeWindowUpdate {
    pub const fn empty()->Self{Self{registers:[None;MAX_BRIDGE_WINDOW_REGISTERS],count:0}}
    pub fn push(&mut self,offset:u16,value:u32)->Result<(),BridgeWindowError>{
        if self.count==MAX_BRIDGE_WINDOW_REGISTERS{return Err(BridgeWindowError::Capacity)}
        if self.registers[..self.count].iter().flatten().any(|r|r.offset==offset){return Err(BridgeWindowError::InvalidEncoding)}
        self.registers[self.count]=Some(BridgeWindowRegister{offset,value});self.count+=1;Ok(())
    }
}

/// Apply a precomputed bridge-window register update atomically. On any write
/// failure the previous image is restored in reverse order; if rollback fails,
/// decode and bus mastering remain disabled so a partially rebased bridge
/// cannot expose conflicting ranges.
pub fn apply_bridge_update<C:ConfigAccess>(c:&mut C,update:&BridgeWindowUpdate)->Result<(),BridgeWindowError>{
    if update.count>MAX_BRIDGE_WINDOW_REGISTERS{return Err(BridgeWindowError::Capacity)}
    let command=c.read_u16(PCI_COMMAND).map_err(|_|BridgeWindowError::Config)?;
    let mut old=[BridgeWindowRegister{offset:0,value:0};MAX_BRIDGE_WINDOW_REGISTERS];
    for(i,r)in update.registers[..update.count].iter().flatten().enumerate(){
        old[i]=BridgeWindowRegister{offset:r.offset,value:c.read_u32(r.offset).map_err(|_|BridgeWindowError::Config)?};
    }
    c.write_u16(PCI_COMMAND,command&!(PCI_COMMAND_IO|PCI_COMMAND_MEMORY|PCI_COMMAND_BUS_MASTER)).map_err(|_|BridgeWindowError::Config)?;
    let mut written=0usize;
    for r in update.registers[..update.count].iter().flatten(){
        if c.write_u32(r.offset,r.value).is_err(){
            let mut rollback_ok=true;
            for i in (0..written).rev(){if c.write_u32(old[i].offset,old[i].value).is_err(){rollback_ok=false}}
            let safe=command&!(PCI_COMMAND_IO|PCI_COMMAND_MEMORY|PCI_COMMAND_BUS_MASTER);
            let restore=if rollback_ok{command&!PCI_COMMAND_BUS_MASTER}else{safe};
            if c.write_u16(PCI_COMMAND,restore).is_err(){rollback_ok=false}
            return Err(if rollback_ok{BridgeWindowError::Config}else{BridgeWindowError::InvalidEncoding})
        }
        written+=1;
    }
    // Re-enable the prior I/O/memory decode state but never bus mastering.
    c.write_u16(PCI_COMMAND,command&!PCI_COMMAND_BUS_MASTER).map_err(|_|BridgeWindowError::Config)?;
    Ok(())
}

/// Reserve existing bridge windows in the global occupied-range set before
/// endpoint allocation. Caller merges this with host/firmware reservations.
pub fn occupied_windows(windows:&BridgeWindows,out:&mut[OccupiedRange])->Result<usize,BridgeWindowError>{
    let mut n=0;
    for w in [windows.io,windows.memory,windows.prefetchable].into_iter().flatten(){
        if n==out.len(){return Err(BridgeWindowError::Capacity)}
        out[n]=OccupiedRange{start:w.start,length:w.length,kind:w.kind};n+=1;
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn path_intersection_never_expands_host_aperture(){
        let host=[Aperture{start:0x8000_0000,length:0x1000_0000,kind:BarKind::Memory32,prefetchable:false}];
        let bridge=BridgeWindows{io:None,memory:Some(Aperture{start:0x8800_0000,length:0x1000_0000,kind:BarKind::Memory32,prefetchable:false}),prefetchable:None};
        let mut out=[Aperture{start:0,length:0,kind:BarKind::Io,prefetchable:false};4];
        assert_eq!(intersect_path(&host,&[bridge],&mut out),Ok(1));
        assert_eq!(out[0].start,0x8800_0000);assert_eq!(out[0].length,0x0800_0000);
    }
    #[test] fn empty_path_intersection_fails_closed(){
        let host=[Aperture{start:0x8000_0000,length:0x1000,kind:BarKind::Memory32,prefetchable:false}];
        let bridge=BridgeWindows{io:None,memory:None,prefetchable:None};
        let mut out=[Aperture{start:0,length:0,kind:BarKind::Io,prefetchable:false};4];
        assert_eq!(intersect_path(&host,&[bridge],&mut out),Err(BridgeWindowError::NoIntersection));
    }
}
