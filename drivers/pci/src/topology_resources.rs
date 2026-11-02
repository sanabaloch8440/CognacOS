//! End-to-end planning for PCI endpoint BARs across a complete bridge hierarchy.
//!
//! The planner consumes only firmware/bridge windows already discovered by ACPI
//! and PCI. It allocates endpoints against the intersection of every ancestor,
//! shares one occupied-range ledger across the whole bus tree, and stages the
//! forwarding windows required by each bridge. Hardware writes are a separate
//! atomic commit performed by bridge_resources::apply_hierarchy_atomic.
#![no_std]

use crate::resources::{BarKind, BarRequirement, ConfigAccess, ResourceError};
use crate::resource_manager::{self, AllocationPlan, Aperture, OccupiedRange};
use crate::bridge_resources::{self, BridgeWindows, BridgeWindowError, BridgeRebalanceEntry};

pub const MAX_HIERARCHY_ENDPOINTS: usize = 64;
pub const MAX_HIERARCHY_BRIDGES: usize = bridge_resources::MAX_REBALANCE_BRIDGES;
pub const MAX_HIERARCHY_OCCUPIED: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EndpointRequest {
    /// Bridge indices in root-to-leaf order. The last bridge is the immediate
    /// parent of the endpoint. A zero-length path denotes a root-bus endpoint.
    pub bridge_path: [u8; 8],
    pub bridge_path_len: u8,
    pub requirements: [Option<BarRequirement>; resource_manager::MAX_ASSIGNMENTS],
    pub requirement_count: usize,
}
impl EndpointRequest {
    pub const EMPTY: Self = Self {
        bridge_path: [0; 8], bridge_path_len: 0,
        requirements: [None; resource_manager::MAX_ASSIGNMENTS], requirement_count: 0,
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EndpointAllocation {
    pub plan: AllocationPlan,
}
impl EndpointAllocation { pub const EMPTY: Self = Self { plan: AllocationPlan::empty() }; }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HierarchyPlan {
    pub endpoints: [Option<EndpointAllocation>; MAX_HIERARCHY_ENDPOINTS],
    pub endpoint_count: usize,
    /// Minimal forwarding apertures required by each bridge's descendants.
    pub bridge_windows: [Option<BridgeWindows>; MAX_HIERARCHY_BRIDGES],
    pub bridge_count: usize,
}
impl HierarchyPlan {
    pub const fn empty() -> Self {
        Self { endpoints: [None; MAX_HIERARCHY_ENDPOINTS], endpoint_count: 0,
            bridge_windows: [None; MAX_HIERARCHY_BRIDGES], bridge_count: 0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HierarchyError {
    Capacity, InvalidPath, InvalidRequirement, Bridge(BridgeWindowError), Resource(ResourceError),
}
impl From<ResourceError> for HierarchyError { fn from(e: ResourceError) -> Self { Self::Resource(e) } }
impl From<BridgeWindowError> for HierarchyError { fn from(e: BridgeWindowError) -> Self { Self::Bridge(e) } }

/// Allocate all endpoint BARs as one scratch transaction. No caller-visible
/// plan changes on failure. Existing ranges include firmware reservations and
/// resources owned by devices outside this rebalance operation.
pub fn plan_hierarchy(
    host_apertures: &[Aperture],
    bridge_windows: &[BridgeWindows],
    requests: &[EndpointRequest],
    existing: &[OccupiedRange],
    output: &mut HierarchyPlan,
) -> Result<(), HierarchyError> {
    if bridge_windows.len() > MAX_HIERARCHY_BRIDGES
        || requests.len() > MAX_HIERARCHY_ENDPOINTS
        || existing.len() > MAX_HIERARCHY_OCCUPIED {
        return Err(HierarchyError::Capacity);
    }
    let mut staged = HierarchyPlan::empty();
    staged.bridge_count = bridge_windows.len();
    let mut occupied = [OccupiedRange { start: 0, length: 0, kind: BarKind::Io }; MAX_HIERARCHY_OCCUPIED];
    let mut occupied_count = existing.len();
    occupied[..existing.len()].copy_from_slice(existing);

    for (endpoint_index, request) in requests.iter().enumerate() {
        let path_len = request.bridge_path_len as usize;
        if path_len > request.bridge_path.len() || request.requirement_count > resource_manager::MAX_ASSIGNMENTS {
            return Err(HierarchyError::InvalidPath);
        }
        let mut ancestors = [BridgeWindows::default(); 8];
        for (depth, bridge_index) in request.bridge_path[..path_len].iter().copied().enumerate() {
            let index = bridge_index as usize;
            if index >= bridge_windows.len() { return Err(HierarchyError::InvalidPath); }
            // A path cannot revisit a bridge.
            if request.bridge_path[..depth].contains(&bridge_index) { return Err(HierarchyError::InvalidPath); }
            ancestors[depth] = bridge_windows[index];
        }
        // Rebalance plans from the root apertures, not from the currently
        // programmed leaf windows. Those leaf windows may be precisely what
        // is too small; the newly aggregated windows are validated below.
        let mut allowed = [Aperture { start: 0, length: 0, kind: BarKind::Io, prefetchable: false }; 64];
        let allowed_count = bridge_resources::intersect_path(host_apertures, &[], &mut allowed)?;
        let mut requirements = request.requirements;
        let mut plan = AllocationPlan::empty();
        resource_manager::allocate_all(
            &requirements, request.requirement_count,
            &allowed[..allowed_count], &occupied[..occupied_count], &mut plan,
        )?;
        for assignment in plan.assignments[..plan.count].iter().flatten() {
            if occupied_count == occupied.len() { return Err(HierarchyError::Capacity); }
            occupied[occupied_count] = OccupiedRange {
                start: assignment.base, length: assignment.length, kind: assignment.kind,
            };
            occupied_count += 1;
        }
        staged.endpoints[endpoint_index] = Some(EndpointAllocation { plan });
        staged.endpoint_count += 1;
    }

    // Derive each bridge's forwarding windows from every descendant BAR. This
    // avoids independently allocating overlapping windows for sibling devices.
    for bridge_index in 0..bridge_windows.len() {
        let mut descendants = [resource_manager::BarAssignment {
            index: 0, kind: BarKind::Io, base: 0, length: 0, prefetchable: false,
        }; MAX_HIERARCHY_ENDPOINTS * resource_manager::MAX_ASSIGNMENTS];
        let mut n = 0usize;
        for (request, allocation) in requests.iter().zip(staged.endpoints.iter()).take(staged.endpoint_count) {
            if !request.bridge_path[..request.bridge_path_len as usize].contains(&(bridge_index as u8)) { continue; }
            let Some(allocation) = allocation else { return Err(HierarchyError::InvalidPath); };
            for a in allocation.plan.assignments[..allocation.plan.count].iter().flatten() {
                if n == descendants.len() { return Err(HierarchyError::Capacity); }
                descendants[n] = *a; n += 1;
            }
        }
        let mut windows = BridgeWindows::default();
        bridge_resources::aggregate_descendant_ranges(&descendants[..n], &mut windows)?;
        staged.bridge_windows[bridge_index] = Some(windows);
    }
    // Every new bridge window must fit inside the newly computed windows of
    // its ancestors (or the firmware host aperture for a root bridge).
    for request in requests {
        let path_len=request.bridge_path_len as usize;
        for depth in 0..path_len {
            let bridge_index=request.bridge_path[depth] as usize;
            let required=staged.bridge_windows[bridge_index].ok_or(HierarchyError::InvalidPath)?;
            let mut parents=[BridgeWindows::default();8];
            for parent_depth in 0..depth {
                let parent_index=request.bridge_path[parent_depth] as usize;
                parents[parent_depth]=staged.bridge_windows[parent_index].ok_or(HierarchyError::InvalidPath)?;
            }
            validate_forwarding_path(host_apertures,&parents[..depth],&required)?;
        }
    }
    *output = staged;
    Ok(())
}

/// Ensure the computed forwarding windows fit inside the supplied host and
/// parent-bridge windows before any bridge register is changed.
pub fn validate_forwarding_path(
    host_apertures: &[Aperture],
    ancestors: &[BridgeWindows],
    required: &BridgeWindows,
) -> Result<(), HierarchyError> {
    let mut allowed = [Aperture { start: 0, length: 0, kind: BarKind::Io, prefetchable: false }; 64];
    let n = bridge_resources::intersect_path(host_apertures, ancestors, &mut allowed)?;
    for need in [required.io, required.memory, required.prefetchable].into_iter().flatten() {
        let end = need.start.checked_add(need.length).ok_or(HierarchyError::InvalidRequirement)?;
        if !allowed[..n].iter().any(|a| {
            (a.kind == need.kind
                || (need.kind == BarKind::Memory32 && need.prefetchable
                    && a.kind == BarKind::Memory64 && a.prefetchable))
                && a.prefetchable == need.prefetchable
                && a.start <= need.start
                && a.start.checked_add(a.length).is_some_and(|limit| end <= limit)
        }) { return Err(HierarchyError::InvalidRequirement); }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn req(index:u8, kind:BarKind, length:u64)->Option<BarRequirement>{
        Some(BarRequirement{index,kind,length,alignment:length,prefetchable:false,assigned:0})
    }
    #[test]
    fn siblings_share_one_global_allocation_ledger_and_bridge_window(){
        let host=[Aperture{start:0x8000_0000,length:0x0100_0000,kind:BarKind::Memory32,prefetchable:false}];
        let bridge=[BridgeWindows{memory:Some(host[0]),..BridgeWindows::default()}];
        let mut a=EndpointRequest::EMPTY;a.bridge_path_len=1;a.bridge_path[0]=0;a.requirement_count=1;a.requirements[0]=req(0,BarKind::Memory32,0x1000);
        let mut b=a;b.requirements[0]=req(0,BarKind::Memory32,0x2000);
        let mut out=HierarchyPlan::empty();
        plan_hierarchy(&host,&bridge,&[a,b],&[],&mut out).unwrap();
        let first=out.endpoints[0].unwrap().plan.assignments[0].unwrap();
        let second=out.endpoints[1].unwrap().plan.assignments[0].unwrap();
        assert_ne!(first.base,second.base);
        let window=out.bridge_windows[0].unwrap().memory.unwrap();
        assert!(window.start<=first.base && window.start<=second.base);
        assert!(window.start+window.length>=second.base+second.length);
    }
    #[test]
    fn failed_endpoint_does_not_modify_previous_output(){
        let host=[Aperture{start:0x8000_0000,length:0x1000,kind:BarKind::Memory32,prefetchable:false}];
        let mut request=EndpointRequest::EMPTY;request.requirement_count=1;request.requirements[0]=req(0,BarKind::Memory32,0x2000);
        let mut out=HierarchyPlan::empty();out.endpoint_count=17;let old=out;
        assert!(plan_hierarchy(&host,&[],&[request],&[],&mut out).is_err());
        assert_eq!(out,old);
    }
    #[test]
    fn rejects_bad_or_repeated_bridge_paths(){
        let host=[Aperture{start:0x8000_0000,length:0x1000,kind:BarKind::Memory32,prefetchable:false}];
        let bridges=[BridgeWindows{memory:Some(host[0]),..BridgeWindows::default()}];
        let mut request=EndpointRequest::EMPTY;request.bridge_path_len=2;request.bridge_path[..2].copy_from_slice(&[0,0]);
        let mut out=HierarchyPlan::empty();
        assert_eq!(plan_hierarchy(&host,&bridges,&[request],&[],&mut out),Err(HierarchyError::InvalidPath));
    }
}


/// Plan a full rebalance without relying on the currently programmed bridge
/// windows. Endpoints are allocated against ACPI-provided host apertures using
/// one global occupied-range ledger. New bridge windows are derived bottom-up
/// and checked against their parent before any hardware is changed.
pub fn plan_hierarchy_rebalance(
    host_apertures:&[Aperture],
    requests:&[EndpointRequest],
    existing:&[OccupiedRange],
    output:&mut HierarchyPlan,
)->Result<[Option<u8>;MAX_HIERARCHY_BRIDGES],HierarchyError>{
    if host_apertures.is_empty()||host_apertures.len()>64||requests.len()>MAX_HIERARCHY_ENDPOINTS||existing.len()>MAX_HIERARCHY_OCCUPIED {
        return Err(HierarchyError::Capacity);
    }
    let mut staged=HierarchyPlan::empty();
    let bridge_count=requests.iter().flat_map(|r|r.bridge_path[..(r.bridge_path_len as usize).min(8)].iter()).map(|v|*v as usize+1).max().unwrap_or(0);
    if bridge_count>MAX_HIERARCHY_BRIDGES{return Err(HierarchyError::Capacity)}
    staged.bridge_count=bridge_count;
    let mut parents=[None;MAX_HIERARCHY_BRIDGES];let mut seen=[false;MAX_HIERARCHY_BRIDGES];
    let mut occupied=[OccupiedRange{start:0,length:0,kind:BarKind::Io};MAX_HIERARCHY_OCCUPIED];
    let mut occupied_count=existing.len();occupied[..existing.len()].copy_from_slice(existing);
    for (ei,request) in requests.iter().enumerate(){
        let depth=request.bridge_path_len as usize;
        if depth>request.bridge_path.len()||request.requirement_count>resource_manager::MAX_ASSIGNMENTS{return Err(HierarchyError::InvalidPath)}
        for d in 0..depth{
            let index=request.bridge_path[d] as usize;
            if index>=bridge_count||request.bridge_path[..d].contains(&request.bridge_path[d]){return Err(HierarchyError::InvalidPath)}
            let parent=if d==0{None}else{Some(request.bridge_path[d-1])};
            if seen[index]&&parents[index]!=parent{return Err(HierarchyError::InvalidPath)}
            parents[index]=parent;seen[index]=true;
            if let Some(p)=parent{if p as usize>=index{return Err(HierarchyError::InvalidPath)}}
        }
        let mut allowed=[Aperture{start:0,length:0,kind:BarKind::Io,prefetchable:false};64];let mut allowed_count=0;
        for a in host_apertures.iter().copied(){
            if depth>0&&a.kind==BarKind::Memory64&&!a.prefetchable{
                if a.start>=1u64<<32{continue}
                let end=a.start.checked_add(a.length).ok_or(HierarchyError::InvalidRequirement)?.min(1u64<<32);
                if end>a.start{allowed[allowed_count]=Aperture{start:a.start,length:end-a.start,kind:BarKind::Memory32,prefetchable:false};allowed_count+=1;}
            }else{allowed[allowed_count]=a;allowed_count+=1;}
        }
        let mut plan=AllocationPlan::empty();
        resource_manager::allocate_all(&request.requirements,request.requirement_count,&allowed[..allowed_count],&occupied[..occupied_count],&mut plan)?;
        for a in plan.assignments[..plan.count].iter().flatten(){
            if occupied_count==occupied.len(){return Err(HierarchyError::Capacity)}
            occupied[occupied_count]=OccupiedRange{start:a.base,length:a.length,kind:a.kind};occupied_count+=1;
        }
        staged.endpoints[ei]=Some(EndpointAllocation{plan});staged.endpoint_count+=1;
    }
    for bi in 0..bridge_count{
        let mut descendants=[resource_manager::BarAssignment{index:0,kind:BarKind::Io,base:0,length:0,prefetchable:false};MAX_HIERARCHY_ENDPOINTS*resource_manager::MAX_ASSIGNMENTS];let mut n=0;
        for (request,allocation) in requests.iter().zip(staged.endpoints.iter()).take(staged.endpoint_count){
            let depth=request.bridge_path_len as usize;if !request.bridge_path[..depth].contains(&(bi as u8)){continue}
            let Some(allocation)=allocation else{return Err(HierarchyError::InvalidPath)};
            for a in allocation.plan.assignments[..allocation.plan.count].iter().flatten(){if n==descendants.len(){return Err(HierarchyError::Capacity)}descendants[n]=*a;n+=1;}
        }
        let mut w=BridgeWindows::default();bridge_resources::aggregate_descendant_ranges(&descendants[..n],&mut w)?;staged.bridge_windows[bi]=Some(w);
    }
    for bi in 0..bridge_count{
        let required=staged.bridge_windows[bi].ok_or(HierarchyError::InvalidPath)?;
        if let Some(parent)=parents[bi]{
            let pw=staged.bridge_windows[parent as usize].ok_or(HierarchyError::InvalidPath)?;
            let mut allowed=[Aperture{start:0,length:0,kind:BarKind::Io,prefetchable:false};3];let mut n=0;
            for a in [pw.io,pw.memory,pw.prefetchable].into_iter().flatten(){allowed[n]=a;n+=1;}
            bridge_resources::validate_forwarding_path(&allowed[..n],&[],&required)?;
        }else{
            bridge_resources::validate_forwarding_path(host_apertures,&[],&required)?;
        }
    }
    *output=staged;Ok(parents)
}

/// Calculate, encode and atomically commit every bridge in one rebalance.
/// Config accessors are ordered parent-before-child and indexed like paths.
pub fn rebalance_hierarchy_atomic<C:ConfigAccess>(
    host_apertures:&[Aperture],requests:&[EndpointRequest],existing:&[OccupiedRange],
    configs:&mut[C],output:&mut HierarchyPlan,
)->Result<(),HierarchyError>{
    if configs.len()>MAX_HIERARCHY_BRIDGES{return Err(HierarchyError::Capacity)}
    let mut staged=HierarchyPlan::empty();let parents=plan_hierarchy_rebalance(host_apertures,requests,existing,&mut staged)?;
    if configs.len()!=staged.bridge_count{return Err(HierarchyError::InvalidPath)}
    let mut entries=[BridgeRebalanceEntry::EMPTY;MAX_HIERARCHY_BRIDGES];
    for i in 0..staged.bridge_count{
        let w=staged.bridge_windows[i].ok_or(HierarchyError::InvalidPath)?;
        entries[i]=BridgeRebalanceEntry{parent:parents[i],update:bridge_resources::encode_bridge_windows(&mut configs[i],&w)?};
    }
    bridge_resources::apply_hierarchy_atomic(configs,&entries[..staged.bridge_count])?;
    *output=staged;Ok(())
}

#[cfg(test)]
mod full_rebalance_tests {
 use super::*;
 fn request(path:&[u8],index:u8,length:u64)->EndpointRequest{
  let mut r=EndpointRequest::EMPTY;r.bridge_path_len=path.len() as u8;r.bridge_path[..path.len()].copy_from_slice(path);
  r.requirement_count=1;r.requirements[0]=Some(BarRequirement{index,kind:BarKind::Memory32,length,alignment:length,prefetchable:false,assigned:0});r
 }
 #[test]
 fn nested_bridge_windows_are_derived_and_parent_contains_child(){
  let host=[Aperture{start:0x8000_0000,length:0x0200_0000,kind:BarKind::Memory32,prefetchable:false}];
  let requests=[request(&[0],0,0x1000),request(&[0,1],0,0x2000)];
  let mut out=HierarchyPlan::empty();let parents=plan_hierarchy_rebalance(&host,&requests,&[],&mut out).unwrap();
  assert_eq!(parents[0],None);assert_eq!(parents[1],Some(0));
  let root=out.bridge_windows[0].unwrap().memory.unwrap();let child=out.bridge_windows[1].unwrap().memory.unwrap();
  assert!(root.start<=child.start);assert!(root.start+root.length>=child.start+child.length);
 }
 #[test]
 fn failed_rebalance_preserves_previous_plan(){
  let host=[Aperture{start:0x8000_0000,length:0x1000,kind:BarKind::Memory32,prefetchable:false}];
  let requests=[request(&[0],0,0x2000)];let mut out=HierarchyPlan::empty();out.endpoint_count=9;let old=out;
  assert!(plan_hierarchy_rebalance(&host,&requests,&[],&mut out).is_err());assert_eq!(out,old);
 }
}
