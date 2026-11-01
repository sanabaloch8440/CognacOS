//! CognacOS UCX 1.2.0-compatible host-controller extension core.
//!
//! This crate models the documented UCX object and request boundaries without
//! depending on KMDF. The native kernel adapter must provide PnP, IRP/URB,
//! DMA, interrupt, work-queue and object-manager integration. It is not a
//! drop-in Microsoft Ucx01000.sys binary and does not claim WDF ABI compatibility.
#![no_std]
#[cfg(test)] extern crate std;

pub const UCX_INTERFACE_MAJOR:u16=1;
pub const UCX_INTERFACE_MINOR:u16=2;
pub const MAX_ROOT_HUB_PORTS:usize=255;

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum UcxStatus {
    Success, InvalidParameter, InvalidHandle, InvalidState, QueueFull,
    AlreadyCompleted, StaleGeneration, DmaNotReady, Busy, Unsupported,
    HardwareFailure, Cancelled,
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum ControllerState { Created, Started, Resetting, Failed, Removing, Removed }
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum DeviceState { Created, Enabled, Addressed, Configured, Disabled, Removing, Removed }
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum EndpointState { Created, Configured, Running, Paused, Aborting, Purging, Disabled, Removed }
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum EndpointType { Control, Isochronous, Bulk, Interrupt }
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum RequestKind { Control, Bulk, Interrupt, Isochronous, RootHubInterrupt, RootHubControl }
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct ControllerHandle { pub id:u32, pub generation:u32 }
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct RootHubHandle { pub controller:ControllerHandle, pub generation:u32 }
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct DeviceHandle { pub id:u32, pub generation:u32, pub controller:ControllerHandle }
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct EndpointHandle { pub id:u32, pub generation:u32, pub device:DeviceHandle }
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct StreamHandle { pub id:u32, pub generation:u32, pub endpoint:EndpointHandle }

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct DmaBuffer { pub device_address:u64, pub length:u32 }
impl DmaBuffer {
    pub fn validate(self, alignment:u64, max_address:u64)->Result<Self,UcxStatus>{
        if self.device_address==0 || self.length==0 || alignment==0 ||
            !alignment.is_power_of_two() || self.device_address&(alignment-1)!=0 {
            return Err(UcxStatus::DmaNotReady);
        }
        let end=self.device_address.checked_add(self.length as u64-1).ok_or(UcxStatus::DmaNotReady)?;
        if end>max_address { return Err(UcxStatus::DmaNotReady); }
        Ok(self)
    }
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct Request {
    pub id:u64,
    pub endpoint:EndpointHandle,
    pub kind:RequestKind,
    pub buffer:DmaBuffer,
    pub requested_length:u32,
    pub generation:u32,
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct Completion {
    pub request_id:u64,
    pub status:UcxStatus,
    pub bytes_transferred:u32,
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct PortChange { pub port:u8, pub connected:bool, pub enabled:bool, pub over_current:bool, pub generation:u32 }

#[derive(Clone,Copy)]
struct EndpointRecord {
    used:bool, handle:EndpointHandle, kind:EndpointType, state:EndpointState,
    generation:u32, outstanding:u16, max_transfer:u32,
}
impl EndpointRecord {
    const EMPTY:Self=Self{used:false,handle:EndpointHandle{id:0,generation:0,device:DeviceHandle{id:0,generation:0,controller:ControllerHandle{id:0,generation:0}}},kind:EndpointType::Control,state:EndpointState::Removed,generation:0,outstanding:0,max_transfer:0};
}
#[derive(Clone,Copy)]
struct RequestRecord { used:bool, request:Request, cancelling:bool }
impl RequestRecord { const EMPTY:Self=Self{used:false,request:Request{id:0,endpoint:EndpointHandle{id:0,generation:0,device:DeviceHandle{id:0,generation:0,controller:ControllerHandle{id:0,generation:0}}},kind:RequestKind::Bulk,buffer:DmaBuffer{device_address:0,length:0},requested_length:0,generation:0},cancelling:false}; }

/// Evidence that hardware has stopped consuming descriptors for the endpoint.
/// Construct only after the controller's Stop Endpoint/Disable Slot sequence,
/// event/DPC synchronization, and requester-scoped DMA unmap/invalidation finish.
pub struct EndpointQuiesced { _private:() }
impl EndpointQuiesced {
    /// # Safety
    /// Caller must prove hardware and deferred event processing can no longer
    /// access any request buffer belonging to this endpoint.
    pub unsafe fn after_hardware_stop_and_dma_fence()->Self { Self{_private:()} }
}

/// Evidence that the entire controller is halted and no ISR/DPC or DMA
/// requester can still access controller-owned descriptors or buffers.
pub struct ControllerQuiesced { _private:() }
impl ControllerQuiesced {
    /// # Safety
    /// Caller must prove controller halt, bus-master disable, all requester
    /// DMA unmaps/IOMMU invalidations, and interrupt/DPC rundown have completed.
    pub unsafe fn after_controller_halt_and_dma_fence()->Self { Self{_private:()} }
}

/// Fixed-capacity UCX object/request core. The kernel adapter owns actual
/// PnP objects, IRPs/URBs and DMA mappings; this core owns their UCX lifetimes.
pub struct UcxCore<const E:usize,const R:usize> {
    controller:ControllerHandle, controller_state:ControllerState,
    root_hub:RootHubHandle, port_count:u8, port_generation:[u32;MAX_ROOT_HUB_PORTS],
    endpoints:[EndpointRecord;E], requests:[RequestRecord;R], next_request:u64,
}
impl<const E:usize,const R:usize> UcxCore<E,R> {
    pub const fn new(controller_id:u32,port_count:u8)->Self {
        Self{controller:ControllerHandle{id:controller_id,generation:1},controller_state:ControllerState::Created,
            root_hub:RootHubHandle{controller:ControllerHandle{id:controller_id,generation:1},generation:1},
            port_count,port_generation:[0;MAX_ROOT_HUB_PORTS],endpoints:[EndpointRecord::EMPTY;E],
            requests:[RequestRecord::EMPTY;R],next_request:1}
    }
    pub const fn controller_state(&self)->ControllerState { self.controller_state }
    pub const fn controller(&self)->ControllerHandle { self.controller }
    pub const fn root_hub(&self)->RootHubHandle { self.root_hub }
    pub fn start_controller(&mut self)->Result<(),UcxStatus>{
        if self.controller_state!=ControllerState::Created { return Err(UcxStatus::InvalidState); }
        self.controller_state=ControllerState::Started; Ok(())
    }
    pub fn fail_controller(&mut self)->Result<(),UcxStatus>{
        if self.controller_state==ControllerState::Removed { return Err(UcxStatus::InvalidState); }
        self.controller_state=ControllerState::Failed; Ok(())
    }
    pub fn begin_reset(&mut self)->Result<(),UcxStatus>{
        if self.controller_state!=ControllerState::Started { return Err(UcxStatus::InvalidState); }
        self.controller_state=ControllerState::Resetting; Ok(())
    }
    pub fn reset_complete(&mut self,success:bool,_q:&ControllerQuiesced)->Result<(),UcxStatus>{
        if self.controller_state!=ControllerState::Resetting { return Err(UcxStatus::InvalidState); }
        if self.requests.iter().any(|r|r.used){return Err(UcxStatus::Busy)}
        // Reset completion is published only after controller/DMA/DPC quiescence.
        self.endpoints=[EndpointRecord::EMPTY;E];
        self.controller.generation=self.controller.generation.wrapping_add(1).max(1);
        self.root_hub.generation=self.root_hub.generation.wrapping_add(1).max(1);
        self.root_hub.controller=self.controller;
        if success { self.controller_state=ControllerState::Started; }
        else { self.controller_state=ControllerState::Failed; }
        Ok(())
    }
    pub fn create_device(&self,id:u32,generation:u32)->Result<DeviceHandle,UcxStatus>{
        if self.controller_state!=ControllerState::Started || id==0 || generation==0 { return Err(UcxStatus::InvalidState); }
        Ok(DeviceHandle{id,generation,controller:self.controller})
    }
    pub fn validate_device(&self,d:DeviceHandle)->Result<(),UcxStatus>{
        if d.controller!=self.controller { return Err(UcxStatus::StaleGeneration); }
        if self.controller_state!=ControllerState::Started { return Err(UcxStatus::InvalidState); }
        Ok(())
    }
    pub fn create_endpoint(&mut self,device:DeviceHandle,id:u32,kind:EndpointType,max_transfer:u32)->Result<EndpointHandle,UcxStatus>{
        self.validate_device(device)?;
        if id==0 || max_transfer==0 { return Err(UcxStatus::InvalidParameter); }
        let slot=self.endpoints.iter().position(|e|!e.used).ok_or(UcxStatus::QueueFull)?;
        let h=EndpointHandle{id,generation:1,device};
        self.endpoints[slot]=EndpointRecord{used:true,handle:h,kind,state:EndpointState::Configured,generation:1,outstanding:0,max_transfer};
        Ok(h)
    }
    fn endpoint_index(&self,h:EndpointHandle)->Result<usize,UcxStatus>{
        // Endpoint handles embed their owning device/controller generation. A
        // reset invalidates every old endpoint even if its slot has not yet been
        // recycled in the fixed-capacity table.
        if h.device.controller!=self.controller { return Err(UcxStatus::StaleGeneration); }
        self.endpoints.iter().position(|e|e.used&&e.handle==h&&e.generation==h.generation)
            .ok_or(UcxStatus::InvalidHandle)
    }
    pub fn start_endpoint(&mut self,h:EndpointHandle)->Result<(),UcxStatus>{
        let i=self.endpoint_index(h)?;let e=&mut self.endpoints[i];
        if e.state!=EndpointState::Configured&&e.state!=EndpointState::Paused {return Err(UcxStatus::InvalidState)}
        e.state=EndpointState::Running;Ok(())
    }
    pub fn pause_endpoint(&mut self,h:EndpointHandle)->Result<(),UcxStatus>{
        let i=self.endpoint_index(h)?;let e=&mut self.endpoints[i];
        if e.state!=EndpointState::Running{return Err(UcxStatus::InvalidState)}
        e.state=EndpointState::Paused;Ok(())
    }
    pub fn submit(&mut self,h:EndpointHandle,kind:RequestKind,buffer:DmaBuffer,requested:u32,max_dma:u64)->Result<u64,UcxStatus>{
        let i=self.endpoint_index(h)?;let e=self.endpoints[i];
        if self.controller_state!=ControllerState::Started || e.state!=EndpointState::Running {return Err(UcxStatus::InvalidState)}
        if requested==0 || requested>e.max_transfer || requested>buffer.length {return Err(UcxStatus::InvalidParameter)}
        let kind_matches=matches!((e.kind,kind),
            (EndpointType::Control,RequestKind::Control) |
            (EndpointType::Bulk,RequestKind::Bulk) |
            (EndpointType::Interrupt,RequestKind::Interrupt) |
            (EndpointType::Isochronous,RequestKind::Isochronous));
        if !kind_matches { return Err(UcxStatus::InvalidParameter); }
        buffer.validate(1,max_dma)?;
        let slot=self.requests.iter().position(|r|!r.used).ok_or(UcxStatus::QueueFull)?;
        let id=self.next_request;self.next_request=self.next_request.wrapping_add(1).max(1);
        let req=Request{id,endpoint:h,kind,buffer,requested_length:requested,generation:e.generation};
        self.requests[slot]=RequestRecord{used:true,request:req,cancelling:false};
        self.endpoints[i].outstanding=self.endpoints[i].outstanding.checked_add(1).ok_or(UcxStatus::QueueFull)?;
        Ok(id)
    }
    /// A hardware completion is accepted exactly once and only for the matching
    /// endpoint generation. Completion must be called from the deferred event path,
    /// not directly from an ISR.
    pub fn complete(&mut self,id:u64,h:EndpointHandle,status:UcxStatus,bytes:u32)->Result<Completion,UcxStatus>{
        let ri=self.requests.iter().position(|r|r.used&&r.request.id==id).ok_or(UcxStatus::AlreadyCompleted)?;
        let record=self.requests[ri];
        let req=record.request;
        if req.endpoint!=h || req.generation!=h.generation {return Err(UcxStatus::StaleGeneration)}
        if bytes>req.requested_length {return Err(UcxStatus::InvalidParameter)}
        let completion_status=if record.cancelling{UcxStatus::Cancelled}else{status};
        let ei=self.endpoint_index(h)?;
        self.requests[ri]=RequestRecord::EMPTY;
        self.endpoints[ei].outstanding=self.endpoints[ei].outstanding.saturating_sub(1);
        Ok(Completion{request_id:id,status:completion_status,bytes_transferred:bytes})
    }
    pub fn begin_abort_endpoint(&mut self,h:EndpointHandle)->Result<usize,UcxStatus>{
        let ei=self.endpoint_index(h)?;
        if self.endpoints[ei].state!=EndpointState::Running&&self.endpoints[ei].state!=EndpointState::Paused{return Err(UcxStatus::InvalidState)}
        self.endpoints[ei].state=EndpointState::Aborting;
        let mut n=0;
        for r in self.requests.iter_mut(){if r.used&&r.request.endpoint==h{r.cancelling=true;n+=1;}}
        Ok(n)
    }
    /// Once hardware is quiesced, return cancelled requests to their original
    /// submitters one at a time. No request is silently discarded during purge.
    pub fn complete_cancelled_after_quiescence(&mut self,h:EndpointHandle,_q:&EndpointQuiesced)->Result<Option<Completion>,UcxStatus>{
        self.endpoint_index(h)?;
        let Some(ri)=self.requests.iter().position(|r|r.used&&r.request.endpoint==h&&r.cancelling) else {return Ok(None)};
        let req=self.requests[ri].request;
        self.requests[ri]=RequestRecord::EMPTY;
        let ei=self.endpoint_index(h)?;
        self.endpoints[ei].outstanding=self.endpoints[ei].outstanding.saturating_sub(1);
        Ok(Some(Completion{request_id:req.id,status:UcxStatus::Cancelled,bytes_transferred:0}))
    }
    pub fn finish_abort_endpoint(&mut self,h:EndpointHandle,_q:&EndpointQuiesced)->Result<(),UcxStatus>{
        let ei=self.endpoint_index(h)?;
        if self.endpoints[ei].state!=EndpointState::Aborting{return Err(UcxStatus::InvalidState)}
        if self.endpoints[ei].outstanding!=0{return Err(UcxStatus::Busy)}
        self.endpoints[ei].state=EndpointState::Configured;Ok(())
    }
    pub fn remove_endpoint(&mut self,h:EndpointHandle,_q:&EndpointQuiesced)->Result<(),UcxStatus>{
        let ei=self.endpoint_index(h)?;
        if self.endpoints[ei].outstanding!=0{return Err(UcxStatus::Busy)}
        self.endpoints[ei].state=EndpointState::Removed;self.endpoints[ei].generation=self.endpoints[ei].generation.wrapping_add(1);
        self.endpoints[ei].used=false;Ok(())
    }
    /// Record a physical port-change indication and return a generation token.
    /// The root-hub interrupt request is completed by the adapter after it has
    /// translated this into the pending hub notification.
    pub fn root_hub_port_changed(&mut self,port:u8)->Result<PortChange,UcxStatus>{
        self.root_hub_port_changed_with_status(port,false,false,false)
    }
    /// Publish the hardware-observed root-port state with a generation token.
    /// The xHCI adapter must read PORTSC before calling this method; this core
    /// never invents connection or enable state from the event type alone.
    pub fn root_hub_port_changed_with_status(&mut self,port:u8,connected:bool,enabled:bool,over_current:bool)->Result<PortChange,UcxStatus>{
        if port==0||port>self.port_count{return Err(UcxStatus::InvalidParameter)}
        let g=&mut self.port_generation[(port-1) as usize];*g=g.wrapping_add(1).max(1);
        Ok(PortChange{port,connected,enabled,over_current,generation:*g})
    }
    pub fn validate_port_generation(&self,change:PortChange)->Result<(),UcxStatus>{
        if change.port==0||change.port>self.port_count{return Err(UcxStatus::InvalidParameter)}
        if self.port_generation[(change.port-1) as usize]!=change.generation{return Err(UcxStatus::StaleGeneration)}
        Ok(())
    }
    pub fn begin_remove_controller(&mut self)->Result<(),UcxStatus>{
        if self.controller_state==ControllerState::Removed||self.controller_state==ControllerState::Removing{return Err(UcxStatus::InvalidState)}
        self.controller_state=ControllerState::Removing;Ok(())
    }
    pub fn finish_remove_controller(&mut self,_q:&ControllerQuiesced)->Result<(),UcxStatus>{
        if self.controller_state!=ControllerState::Removing{return Err(UcxStatus::InvalidState)}
        if self.requests.iter().any(|r|r.used){return Err(UcxStatus::Busy)}
        self.endpoints=[EndpointRecord::EMPTY;E];
        self.controller_state=ControllerState::Removed;Ok(())
    }
    pub fn outstanding_requests(&self)->usize { self.requests.iter().filter(|r|r.used).count() }
    pub fn endpoint_outstanding(&self,h:EndpointHandle)->Result<u16,UcxStatus>{Ok(self.endpoints[self.endpoint_index(h)?].outstanding)}
}
impl<const E:usize,const R:usize> Default for UcxCore<E,R>{fn default()->Self{Self::new(1,1)}}

#[cfg(test)]
mod tests {
 use super::*;
 fn started()->UcxCore<8,16>{
  let mut c=UcxCore::new(7,4);c.start_controller().unwrap();c
 }
 #[test]fn endpoint_requests_complete_once_and_account(){
  let mut c=started();let d=c.create_device(1,1).unwrap();
  let e=c.create_endpoint(d,2,EndpointType::Interrupt,64).unwrap();c.start_endpoint(e).unwrap();
  let id=c.submit(e,RequestKind::Interrupt,DmaBuffer{device_address:0x8000,length:8},8,0xffff_ffff).unwrap();
  assert_eq!(c.endpoint_outstanding(e),Ok(1));
  assert_eq!(c.complete(id,e,UcxStatus::Success,8),Ok(Completion{request_id:id,status:UcxStatus::Success,bytes_transferred:8}));
  assert_eq!(c.complete(id,e,UcxStatus::Success,8),Err(UcxStatus::AlreadyCompleted));
  assert_eq!(c.outstanding_requests(),0);
 }
 #[test]fn abort_does_not_release_dma_until_hardware_quiesces(){
  let mut c=started();let d=c.create_device(1,1).unwrap();let e=c.create_endpoint(d,2,EndpointType::Bulk,512).unwrap();c.start_endpoint(e).unwrap();
  c.submit(e,RequestKind::Bulk,DmaBuffer{device_address:0x9000,length:512},512,0xffff_ffff).unwrap();
  assert_eq!(c.begin_abort_endpoint(e),Ok(1));assert_eq!(c.outstanding_requests(),1);
  let q=unsafe{EndpointQuiesced::after_hardware_stop_and_dma_fence()};
  assert_eq!(c.finish_abort_endpoint(e,&q),Err(UcxStatus::Busy));
  assert_eq!(c.complete_cancelled_after_quiescence(e,&q).unwrap().unwrap().status,UcxStatus::Cancelled);
  assert_eq!(c.complete_cancelled_after_quiescence(e,&q),Ok(None));
  assert_eq!(c.finish_abort_endpoint(e,&q),Ok(()));assert_eq!(c.outstanding_requests(),0);
 }
 #[test]fn stale_controller_generation_rejects_old_device(){
  let mut c=started();let d=c.create_device(1,1).unwrap();c.begin_reset().unwrap();let q=unsafe{ControllerQuiesced::after_controller_halt_and_dma_fence()};c.reset_complete(true,&q).unwrap();
  assert_eq!(c.validate_device(d),Err(UcxStatus::StaleGeneration));
 }
 #[test]fn root_hub_port_change_generation_invalidates_old_event(){
  let mut c=started();let a=c.root_hub_port_changed_with_status(2,true,true,false).unwrap();let b=c.root_hub_port_changed_with_status(2,false,false,true).unwrap();
  assert!(a.connected&&a.enabled&&!a.over_current);
  assert_eq!(c.validate_port_generation(a),Err(UcxStatus::StaleGeneration));assert_eq!(c.validate_port_generation(b),Ok(()));
 }
 #[test]fn endpoint_rejects_wrong_request_type_and_stale_controller_generation(){
  let mut c=started();let d=c.create_device(1,1).unwrap();
  let e=c.create_endpoint(d,2,EndpointType::Interrupt,64).unwrap();c.start_endpoint(e).unwrap();
  let b=DmaBuffer{device_address:0x8000,length:8};
  assert_eq!(c.submit(e,RequestKind::Bulk,b,8,0xffff_ffff),Err(UcxStatus::InvalidParameter));
  c.begin_reset().unwrap();let q=unsafe{ControllerQuiesced::after_controller_halt_and_dma_fence()};c.reset_complete(true,&q).unwrap();
  assert_eq!(c.submit(e,RequestKind::Interrupt,b,8,0xffff_ffff),Err(UcxStatus::StaleGeneration));
 }
 #[test]fn rejects_bad_transfer_spans_and_lengths(){
  let mut c=started();let d=c.create_device(1,1).unwrap();let e=c.create_endpoint(d,3,EndpointType::Bulk,128).unwrap();c.start_endpoint(e).unwrap();
  assert_eq!(c.submit(e,RequestKind::Bulk,DmaBuffer{device_address:u64::MAX-3,length:8},8,u64::MAX),Err(UcxStatus::DmaNotReady));
  assert_eq!(c.submit(e,RequestKind::Bulk,DmaBuffer{device_address:0x1000,length:8},9,u64::MAX),Err(UcxStatus::InvalidParameter));
 }
}

pub mod wdm;

pub mod root_hub;
pub mod streams;
