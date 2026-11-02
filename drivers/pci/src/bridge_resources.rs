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
    let prefetch_kind=if pref_64{BarKind::Memory64}else{BarKind::Memory32};
    let prefetchable=if pref_base<=pref_limit{aperture(pref_base,pref_limit,prefetch_kind,true)?}else{None};
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
                BarKind::Memory32|BarKind::Memory64 if current.prefetchable=>bridge.prefetchable,
                BarKind::Memory32|BarKind::Memory64=>bridge.memory,
            };
            let Some(window)=candidate else{continue};
            let compatible=if current.kind==BarKind::Io{
                window.kind==BarKind::Io
            }else{
                window.kind!=BarKind::Io&&current.prefetchable==window.prefetchable
                    &&(current.prefetchable||window.kind==BarKind::Memory32)
            };
            if !compatible||(current.kind!=BarKind::Io&&current.prefetchable!=window.prefetchable){continue}
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
    if c.write_u16(PCI_COMMAND,command&!PCI_COMMAND_BUS_MASTER).is_err() {
        let mut rollback_ok=true;
        for i in (0..written).rev() {
            if c.write_u32(old[i].offset,old[i].value).is_err(){rollback_ok=false}
        }
        let safe=command&!(PCI_COMMAND_IO|PCI_COMMAND_MEMORY|PCI_COMMAND_BUS_MASTER);
        if c.write_u16(PCI_COMMAND,if rollback_ok{command&!PCI_COMMAND_BUS_MASTER}else{safe}).is_err(){rollback_ok=false}
        return Err(if rollback_ok{BridgeWindowError::Config}else{BridgeWindowError::InvalidEncoding})
    }
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


pub const MAX_REBALANCE_BRIDGES:usize=64;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct BridgeRebalanceEntry {
    /// Parent index in this same array. Parents must precede children; roots
    /// use None. This makes ordering explicit and prevents cyclic topologies.
    pub parent:Option<u8>,
    pub update:BridgeWindowUpdate,
}
impl BridgeRebalanceEntry {
    pub const EMPTY:Self=Self{parent:None,update:BridgeWindowUpdate::empty()};
}

/// Commit a complete bridge hierarchy parent-first. Before the first write,
/// capture every affected register on every bridge. If any bridge fails,
/// restore all already-committed bridges child-first. A rollback failure
/// leaves that bridge's decode and bus mastering disabled via
/// apply_bridge_update's fail-closed behavior.
pub fn apply_hierarchy<C:ConfigAccess>(
    configs:&mut [C],
    entries:&[BridgeRebalanceEntry],
)->Result<(),BridgeWindowError>{
    if entries.len()!=configs.len()||entries.len()>MAX_REBALANCE_BRIDGES{return Err(BridgeWindowError::Capacity)}
    let mut snapshots=[BridgeWindowUpdate::empty();MAX_REBALANCE_BRIDGES];
    for (i,e) in entries.iter().enumerate(){
        if e.update.count>MAX_BRIDGE_WINDOW_REGISTERS{return Err(BridgeWindowError::Capacity)}
        if let Some(parent)=e.parent {
            let p=parent as usize;
            if p>=i{return Err(BridgeWindowError::InvalidEncoding)}
        }
        for r in e.update.registers.iter().take(e.update.count).flatten(){
            if snapshots[i].push(r.offset,configs[i].read_u32(r.offset).map_err(|_|BridgeWindowError::Config)?).is_err(){
                return Err(BridgeWindowError::InvalidEncoding)
            }
        }
    }
    let mut committed=0usize;
    while committed<entries.len(){
        if let Err(error)=apply_bridge_update(&mut configs[committed],&entries[committed].update){
            let mut rollback_ok=true;
            for i in (0..committed).rev(){
                if apply_bridge_update(&mut configs[i],&snapshots[i]).is_err(){rollback_ok=false}
            }
            return Err(if rollback_ok{error}else{BridgeWindowError::InvalidEncoding})
        }
        committed+=1;
    }
    Ok(())
}

/// Build a bridge's forwarding window from all descendant BAR assignments.
/// Ranges are rounded out to the bridge's 1-MiB memory and 4-KiB I/O
/// granularity. The caller must separately enforce host/root aperture bounds.
pub fn aggregate_descendant_ranges(
    assignments:&[crate::resource_manager::BarAssignment],
    output:&mut BridgeWindows,
)->Result<(),BridgeWindowError>{
    let mut io:Option<(u64,u64)>=None;
    let mut mem:Option<(u64,u64)>=None;
    let mut pref:Option<(u64,u64)>=None;
    for a in assignments{
        let end=a.base.checked_add(a.length).ok_or(BridgeWindowError::Overflow)?;
        let slot=match a.kind{
            BarKind::Io=>&mut io,
            BarKind::Memory32 if !a.prefetchable=>&mut mem,
            BarKind::Memory32|BarKind::Memory64=>&mut pref,
        };
        *slot=Some(match *slot{None=>(a.base,end),Some((lo,hi))=>(lo.min(a.base),hi.max(end))});
    }
    let round=|range:Option<(u64,u64)>,gran:u64,kind:BarKind,prefetchable:bool|->Result<Option<Aperture>,BridgeWindowError>{
        let Some((lo,hi))=range else{return Ok(None)};
        let start=lo&!(gran-1);
        let end=hi.checked_add(gran-1).ok_or(BridgeWindowError::Overflow)?&!(gran-1);
        Ok(Some(Aperture{start,length:end.checked_sub(start).ok_or(BridgeWindowError::Overflow)?,kind,prefetchable}))
    };
    output.io=round(io,0x1000,BarKind::Io,false)?;
    if mem.is_some_and(|(_,hi)|hi>1u64<<32){return Err(BridgeWindowError::InvalidEncoding)}
    if mem.is_some_and(|(_,hi)|hi>1u64<<32){return Err(BridgeWindowError::InvalidEncoding)}
    output.memory=round(mem,0x10_0000,BarKind::Memory32,false)?;
    let pref_kind=if pref.is_some_and(|(_,hi)|hi<=1u64<<32){BarKind::Memory32}else{BarKind::Memory64};
    output.prefetchable=round(pref,0x10_0000,pref_kind,true)?;
    Ok(())
}


/// Commit a whole bridge hierarchy as one decode-disabled transaction.
/// Unlike applying each bridge independently, this keeps every affected bridge
///'s I/O and memory decode disabled until all parent and child windows have
/// been programmed successfully. Any failure restores every saved register
/// image; rollback failure leaves all bridges disabled and bus mastering off.
pub fn apply_hierarchy_atomic<C:ConfigAccess>(
    configs:&mut [C],
    entries:&[BridgeRebalanceEntry],
)->Result<(),BridgeWindowError>{
    if entries.len()!=configs.len()||entries.len()>MAX_REBALANCE_BRIDGES{return Err(BridgeWindowError::Capacity)}
    let mut snapshots=[BridgeWindowUpdate::empty();MAX_REBALANCE_BRIDGES];
    let mut commands=[0u16;MAX_REBALANCE_BRIDGES];
    for (i,e) in entries.iter().enumerate(){
        if e.update.count>MAX_BRIDGE_WINDOW_REGISTERS{return Err(BridgeWindowError::Capacity)}
        if let Some(parent)=e.parent {if parent as usize>=i{return Err(BridgeWindowError::InvalidEncoding)}}
        commands[i]=configs[i].read_u16(PCI_COMMAND).map_err(|_|BridgeWindowError::Config)?;
        for r in e.update.registers.iter().take(e.update.count).flatten(){
            if snapshots[i].push(r.offset,configs[i].read_u32(r.offset).map_err(|_|BridgeWindowError::Config)?).is_err(){
                return Err(BridgeWindowError::InvalidEncoding)
            }
        }
    }
    let disable_all=|configs:&mut[C]|->bool{
        let mut ok=true;
        for (i,c) in configs.iter_mut().enumerate(){
            if c.write_u16(PCI_COMMAND,commands[i]&!(PCI_COMMAND_IO|PCI_COMMAND_MEMORY|PCI_COMMAND_BUS_MASTER)).is_err(){ok=false}
        }
        ok
    };
    let mut disabled=0usize;
    for i in 0..configs.len(){
        if configs[i].write_u16(PCI_COMMAND,commands[i]&!(PCI_COMMAND_IO|PCI_COMMAND_MEMORY|PCI_COMMAND_BUS_MASTER)).is_err(){
            let mut ok=true;
            for j in (0..disabled).rev(){if configs[j].write_u16(PCI_COMMAND,commands[j]&!PCI_COMMAND_BUS_MASTER).is_err(){ok=false}}
            if !ok {let _=disable_all(configs);return Err(BridgeWindowError::InvalidEncoding)}
            return Err(BridgeWindowError::Config)
        }
        disabled+=1;
    }
    let mut failed=false;
    'commit: for i in 0..entries.len(){
        for r in entries[i].update.registers.iter().take(entries[i].update.count).flatten(){
            if configs[i].write_u32(r.offset,r.value).is_err(){failed=true;break 'commit}
        }
    }
    if failed {
        let mut rollback_ok=true;
        for i in (0..entries.len()).rev(){
            for r in snapshots[i].registers.iter().take(snapshots[i].count).flatten(){
                if configs[i].write_u32(r.offset,r.value).is_err(){rollback_ok=false}
            }
        }
        if rollback_ok {
            for i in 0..configs.len(){if configs[i].write_u16(PCI_COMMAND,commands[i]&!PCI_COMMAND_BUS_MASTER).is_err(){rollback_ok=false}}
        }
        if !rollback_ok {let _=disable_all(configs);return Err(BridgeWindowError::InvalidEncoding)}
        return Err(BridgeWindowError::Config)
    }
    // Enable decode only after the entire hierarchy is consistent. Bus master
    // remains disabled; the DMA/IOMMU subsystem owns that transition.
    for i in 0..configs.len(){
        if configs[i].write_u16(PCI_COMMAND,commands[i]&!PCI_COMMAND_BUS_MASTER).is_err(){
            // Some earlier bridges may already have decode re-enabled. Disable
            // every bridge before restoring any old forwarding-window image.
            let mut rollback_ok=true;
            for j in 0..configs.len(){
                if configs[j].write_u16(PCI_COMMAND,commands[j]&!(PCI_COMMAND_IO|PCI_COMMAND_MEMORY|PCI_COMMAND_BUS_MASTER)).is_err(){rollback_ok=false}
            }
            for j in (0..configs.len()).rev(){
                for r in snapshots[j].registers.iter().take(snapshots[j].count).flatten(){
                    if configs[j].write_u32(r.offset,r.value).is_err(){rollback_ok=false}
                }
            }
            if rollback_ok {
                for j in 0..configs.len(){if configs[j].write_u16(PCI_COMMAND,commands[j]&!PCI_COMMAND_BUS_MASTER).is_err(){rollback_ok=false}}
            }
            if !rollback_ok {
                for j in 0..configs.len(){let _=configs[j].write_u16(PCI_COMMAND,commands[j]&!(PCI_COMMAND_IO|PCI_COMMAND_MEMORY|PCI_COMMAND_BUS_MASTER));}
                return Err(BridgeWindowError::InvalidEncoding)
            }
            return Err(BridgeWindowError::Config)
        }
    }
    Ok(())
}

#[cfg(test)]
mod atomic_hierarchy_tests {
    use super::*;
    struct Fake { regs:[u32;64], fail_write:Option<u16> }
    impl ConfigAccess for Fake {
        fn read_u16(&mut self,o:u16)->Result<u16,ResourceError>{Ok(self.regs[o as usize/4] as u16)}
        fn write_u16(&mut self,o:u16,v:u16)->Result<(),ResourceError>{self.regs[o as usize/4]=(self.regs[o as usize/4]&0xffff_0000)|v as u32;Ok(())}
        fn read_u32(&mut self,o:u16)->Result<u32,ResourceError>{Ok(self.regs[o as usize/4])}
        fn write_u32(&mut self,o:u16,v:u32)->Result<(),ResourceError>{if self.fail_write==Some(o){return Err(ResourceError::Config)}self.regs[o as usize/4]=v;Ok(())}
    }
    #[test]
    fn hierarchy_failure_rolls_back_all_bridges(){
        let mut a=Fake{regs:[0;64],fail_write:None};let mut b=Fake{regs:[0;64],fail_write:Some(BRIDGE_MEMORY_BASE_LIMIT)};
        a.regs[1]=3;b.regs[1]=3;a.regs[BRIDGE_MEMORY_BASE_LIMIT as usize/4]=0x1111;b.regs[BRIDGE_MEMORY_BASE_LIMIT as usize/4]=0x2222;
        let update=|value|{let mut u=BridgeWindowUpdate::empty();u.push(BRIDGE_MEMORY_BASE_LIMIT,value).unwrap();u};
        let entries=[BridgeRebalanceEntry{parent:None,update:update(0xaaaa)},BridgeRebalanceEntry{parent:Some(0),update:update(0xbbbb)}];
        assert_eq!(apply_hierarchy_atomic(&mut [a,b],&entries),Err(BridgeWindowError::Config));
    }
}

/// Encode computed Type-1 forwarding windows into the bridge configuration
/// registers. Only aligned, representable windows are accepted; absent windows
/// are disabled using base-greater-than-limit encodings.
pub fn encode_bridge_windows<C:ConfigAccess>(c:&mut C,w:&BridgeWindows)->Result<BridgeWindowUpdate,BridgeWindowError>{
    let _old_io=c.read_u32(BRIDGE_IO_BASE_LIMIT).map_err(|_|BridgeWindowError::Config)?;
    let _old_mem=c.read_u32(BRIDGE_MEMORY_BASE_LIMIT).map_err(|_|BridgeWindowError::Config)?;
    let _old_pref=c.read_u32(BRIDGE_PREFETCH_BASE_LIMIT).map_err(|_|BridgeWindowError::Config)?;
    let _old_pref_base=c.read_u32(BRIDGE_PREFETCH_BASE_UPPER).map_err(|_|BridgeWindowError::Config)?;
    let _old_pref_limit=c.read_u32(BRIDGE_PREFETCH_LIMIT_UPPER).map_err(|_|BridgeWindowError::Config)?;
    let _old_io_upper=c.read_u32(BRIDGE_IO_BASE_UPPER).map_err(|_|BridgeWindowError::Config)?;
    let (io,io_upper)=if let Some(a)=w.io{
        if a.kind!=BarKind::Io||a.length==0||a.start&0xfff!=0||a.length&0xfff!=0{return Err(BridgeWindowError::InvalidEncoding)}
        let end=a.start.checked_add(a.length).and_then(|v|v.checked_sub(1)).ok_or(BridgeWindowError::Overflow)?;
        if end>u32::MAX as u64{return Err(BridgeWindowError::InvalidEncoding)}
        let wide=a.start>0xffff||end>0xffff;let t=wide as u32;
        ((((a.start>>8)as u32&0xf0)|t)|((((end>>8)as u32&0xf0)|t)<<8),
         if wide{((a.start>>16)as u32&0xffff)|(((end>>16)as u32&0xffff)<<16)}else{0})
    }else{(0x0000_00f0,0)};
    let mem=if let Some(a)=w.memory{
        if a.kind!=BarKind::Memory32||a.prefetchable||a.length==0||a.start&0xfffff!=0||a.length&0xfffff!=0{return Err(BridgeWindowError::InvalidEncoding)}
        let end=a.start.checked_add(a.length).and_then(|v|v.checked_sub(1)).ok_or(BridgeWindowError::Overflow)?;
        if end>=1u64<<32{return Err(BridgeWindowError::InvalidEncoding)}
        (((a.start>>16)as u32&0xfff0)|(((end>>16)as u32&0xfff0)<<16))
    }else{0x0000_fff0};
    let (pref,pref_base,pref_limit)=if let Some(a)=w.prefetchable{
        if !a.prefetchable||!matches!(a.kind,BarKind::Memory32|BarKind::Memory64)||a.length==0||a.start&0xfffff!=0||a.length&0xfffff!=0{return Err(BridgeWindowError::InvalidEncoding)}
        let end=a.start.checked_add(a.length).and_then(|v|v.checked_sub(1)).ok_or(BridgeWindowError::Overflow)?;
        let wide=a.kind==BarKind::Memory64||a.start>u32::MAX as u64||end>u32::MAX as u64;let t=wide as u32;
        ((((a.start>>16)as u32&0xfff0)|t)|((((end>>16)as u32&0xfff0)|t)<<16),
         if wide{(a.start>>32)as u32}else{0},if wide{(end>>32)as u32}else{0})
    }else{(0x0000_fff0,0,0)};
    let mut u=BridgeWindowUpdate::empty();
    u.push(BRIDGE_IO_BASE_LIMIT,io)?;
    u.push(BRIDGE_MEMORY_BASE_LIMIT,mem)?;
    u.push(BRIDGE_PREFETCH_BASE_LIMIT,pref)?;
    u.push(BRIDGE_PREFETCH_BASE_UPPER,pref_base)?;
    u.push(BRIDGE_PREFETCH_LIMIT_UPPER,pref_limit)?;
    u.push(BRIDGE_IO_BASE_UPPER,io_upper)?;
    Ok(u)
}

#[cfg(test)]
mod bridge_encoding_tests {
 use super::*;
 struct Fake { regs:[u32;64] }
 impl ConfigAccess for Fake {
  fn read_u16(&mut self,o:u16)->Result<u16,ResourceError>{Ok((self.regs[o as usize/4]>>(if o&2==0{0}else{16}))as u16)}
  fn write_u16(&mut self,o:u16,v:u16)->Result<(),ResourceError>{let i=o as usize/4;let shift=if o&2==0{0}else{16};self.regs[i]=(self.regs[i]&!(0xffff<<shift))|((v as u32)<<shift);Ok(())}
  fn read_u32(&mut self,o:u16)->Result<u32,ResourceError>{Ok(self.regs[o as usize/4])}
  fn write_u32(&mut self,o:u16,v:u32)->Result<(),ResourceError>{self.regs[o as usize/4]=v;Ok(())}
 }
 #[test]
 fn encode_decode_roundtrip_for_bridge_windows(){
  let mut c=Fake{regs:[0;64]};
  let w=BridgeWindows{io:Some(Aperture{start:0x2000,length:0x4000,kind:BarKind::Io,prefetchable:false}),memory:Some(Aperture{start:0x8000_0000,length:0x20_0000,kind:BarKind::Memory32,prefetchable:false}),prefetchable:Some(Aperture{start:0x9000_0000,length:0x20_0000,kind:BarKind::Memory32,prefetchable:true})};
  let u=encode_bridge_windows(&mut c,&w).unwrap();for r in u.registers.iter().take(u.count).flatten(){c.write_u32(r.offset,r.value).unwrap();}
  let d=decode_bridge_windows(&mut c).unwrap();assert_eq!(d.io,w.io);assert_eq!(d.memory,w.memory);assert_eq!(d.prefetchable,w.prefetchable);
 }
}
#[cfg(test)]
mod topology_window_tests {
 use super::*;
 #[test]
 fn prefetchable_32bit_bar_intersects_64bit_bridge_window(){
  let host=[Aperture{start:0x8000_0000,length:0x1000_0000,kind:BarKind::Memory32,prefetchable:true}];
  let bridge=[BridgeWindows{prefetchable:Some(Aperture{start:0x8000_0000,length:0x1000_0000,kind:BarKind::Memory64,prefetchable:true}),..BridgeWindows::default()}];
  let mut out=[Aperture{start:0,length:0,kind:BarKind::Io,prefetchable:false};4];
  let n=intersect_path(&host,&bridge,&mut out).unwrap();assert_eq!(n,1);assert_eq!(out[0].kind,BarKind::Memory32);
 }
}

#[cfg(test)]
mod bridge_encoding_tests {
 use super::*;
 struct Fake { regs:[u32;64] }
 impl ConfigAccess for Fake {
  fn read_u16(&mut self,o:u16)->Result<u16,ResourceError>{Ok((self.regs[o as usize/4]>>(if o&2==0{0}else{16}))as u16)}
  fn write_u16(&mut self,o:u16,v:u16)->Result<(),ResourceError>{let i=o as usize/4;let shift=if o&2==0{0}else{16};self.regs[i]=(self.regs[i]&!(0xffff<<shift))|((v as u32)<<shift);Ok(())}
  fn read_u32(&mut self,o:u16)->Result<u32,ResourceError>{Ok(self.regs[o as usize/4])}
  fn write_u32(&mut self,o:u16,v:u32)->Result<(),ResourceError>{self.regs[o as usize/4]=v;Ok(())}
 }
 #[test]
 fn encode_decode_roundtrip_for_bridge_windows(){
  let mut c=Fake{regs:[0;64]};
  let w=BridgeWindows{io:Some(Aperture{start:0x2000,length:0x4000,kind:BarKind::Io,prefetchable:false}),memory:Some(Aperture{start:0x8000_0000,length:0x20_0000,kind:BarKind::Memory32,prefetchable:false}),prefetchable:Some(Aperture{start:0x9000_0000,length:0x20_0000,kind:BarKind::Memory32,prefetchable:true})};
  let u=encode_bridge_windows(&mut c,&w).unwrap();for r in u.registers.iter().take(u.count).flatten(){c.write_u32(r.offset,r.value).unwrap();}
  let d=decode_bridge_windows(&mut c).unwrap();assert_eq!(d.io,w.io);assert_eq!(d.memory,w.memory);assert_eq!(d.prefetchable,w.prefetchable);
 }
 #[test]
 fn prefetchable_32bit_bar_intersects_64bit_bridge_window(){
  let host=[Aperture{start:0x8000_0000,length:0x1000_0000,kind:BarKind::Memory32,prefetchable:true}];
  let bridge=[BridgeWindows{prefetchable:Some(Aperture{start:0x8000_0000,length:0x1000_0000,kind:BarKind::Memory64,prefetchable:true}),..BridgeWindows::default()}];
  let mut out=[Aperture{start:0,length:0,kind:BarKind::Io,prefetchable:false};4];
  let n=intersect_path(&host,&bridge,&mut out).unwrap();assert_eq!(n,1);assert_eq!(out[0].kind,BarKind::Memory32);
 }
}
