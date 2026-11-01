const MAX_PNP_RESOURCE_CLAIMS:usize=256;
const MAX_RESOURCES_PER_DEVICE:usize=32;
const MAX_PNP_FULL_DESCRIPTORS:usize=16;
const MAX_PNP_PARTIAL_DESCRIPTORS:usize=64;
const IRP_MN_START_DEVICE:u8=0x00;
const IRP_MN_STOP_DEVICE:u8=0x04;
const IRP_MN_REMOVE_DEVICE:u8=0x02;
const IRP_MN_SURPRISE_REMOVAL:u8=0x17;
const CM_RESOURCE_PORT:u8=1;
const CM_RESOURCE_INTERRUPT:u8=2;
const CM_RESOURCE_MEMORY:u8=3;
#[derive(Clone,Copy)]
struct ResourceClaim { used:bool, owner:*mut DeviceObject, irp:*mut Irp, kind:u8, start:u64, length:u64, committed:bool }
unsafe impl Send for ResourceClaim {}
impl ResourceClaim { const EMPTY:Self=Self{used:false,owner:ptr::null_mut(),irp:ptr::null_mut(),kind:0,start:0,length:0,committed:false}; }
struct ResourceArbiter { claims:[ResourceClaim;MAX_PNP_RESOURCE_CLAIMS] }
impl ResourceArbiter { const fn new()->Self{Self{claims:[ResourceClaim::EMPTY;MAX_PNP_RESOURCE_CLAIMS]}} }
static RESOURCE_ARBITER:IrqSpinLock<ResourceArbiter>=IrqSpinLock::new(ResourceArbiter::new());
#[derive(Clone,Copy)]
struct ParsedResource { kind:u8, start:u64, length:u64 }
impl ParsedResource { const EMPTY:Self=Self{kind:0,start:0,length:0}; }

fn read_u16_at(base:*const u8,off:usize)->u16 { unsafe{ptr::read_unaligned(base.add(off).cast::<u16>())} }
fn read_u32_at(base:*const u8,off:usize)->u32 { unsafe{ptr::read_unaligned(base.add(off).cast::<u32>())} }
fn read_u64_at(base:*const u8,off:usize)->u64 { unsafe{ptr::read_unaligned(base.add(off).cast::<u64>())} }

/// Validate a WDK CM_RESOURCE_LIST and extract resources that participate in
/// exclusive range arbitration. Structure bounds/counts are capped to avoid
/// unbounded traversal. As with the WDK ABI, the pointer must reference a
/// readable kernel allocation for the duration of this call.
unsafe fn parse_cm_resource_list(list:*const c_void,out:&mut[ParsedResource;MAX_RESOURCES_PER_DEVICE])->Result<usize,i32>{
 if list.is_null(){return Ok(0)}
 let base=list.cast::<u8>();
 let full_count=unsafe{read_u32_at(base,0)} as usize;
 if full_count>MAX_PNP_FULL_DESCRIPTORS{return Err(STATUS_INVALID_PARAMETER)}
 let mut cursor=4usize;let mut count=0usize;
 for _ in 0..full_count {
  let partial_count=unsafe{read_u32_at(base,cursor+12)} as usize;
  if partial_count>MAX_PNP_PARTIAL_DESCRIPTORS{return Err(STATUS_INVALID_PARAMETER)}
  let descriptor_start=cursor.checked_add(16).ok_or(STATUS_INVALID_PARAMETER)?;
  let bytes=partial_count.checked_mul(16).ok_or(STATUS_INVALID_PARAMETER)?;
  let end=descriptor_start.checked_add(bytes).ok_or(STATUS_INVALID_PARAMETER)?;
  for i in 0..partial_count {
   let off=descriptor_start+i*16;
   let kind=unsafe{*base.add(off)};
   match kind {
    0|5|6=>{}, // Null, device-specific, and bus-number resources are not global ranges.
    4=>{
     let channel=unsafe{read_u32_at(base,off+4)} as u64;
     if count==out.len(){return Err(STATUS_INSUFFICIENT_RESOURCES)}
     out[count]=ParsedResource{kind:4,start:channel,length:1};count+=1;
    }
    CM_RESOURCE_PORT|CM_RESOURCE_MEMORY=>{
     let start=unsafe{read_u64_at(base,off+4)};
     let length=unsafe{read_u32_at(base,off+12)} as u64;
     if length==0||start.checked_add(length).is_none(){return Err(STATUS_INVALID_PARAMETER)}
     if count==out.len(){return Err(STATUS_INSUFFICIENT_RESOURCES)}
     out[count]=ParsedResource{kind,start,length};count+=1;
    }
    CM_RESOURCE_INTERRUPT=>{
     let vector=unsafe{read_u32_at(base,off+8)} as u64;
     if vector==0{return Err(STATUS_INVALID_PARAMETER)}
     if count==out.len(){return Err(STATUS_INSUFFICIENT_RESOURCES)}
     out[count]=ParsedResource{kind,start:vector,length:1};count+=1;
    }
    7=>return Err(STATUS_INVALID_DEVICE_REQUEST), // Large-memory encoding needs architecture-specific decoding.
    _=>return Err(STATUS_INVALID_PARAMETER),
   }
  }
  cursor=end;
 }
 Ok(count)
}
fn resources_overlap(a:&ParsedResource,b:&ParsedResource)->bool{
 if a.kind!=b.kind{return false}
 if a.kind==CM_RESOURCE_INTERRUPT{return a.start==b.start}
 let a_end=a.start.saturating_add(a.length);
 let b_end=b.start.saturating_add(b.length);
 a.start<b_end&&b.start<a_end
}

/// Reserve translated resources for an IRP_MN_START_DEVICE transaction.
/// Reservations are tentative until the IRP's terminal completion succeeds.
pub(crate) unsafe fn io_prepare_pnp_resources(device:*mut DeviceObject,irp:*mut Irp,minor:u8,allocated:*const c_void,translated:*const c_void)->i32{
 if minor!=IRP_MN_START_DEVICE{return STATUS_SUCCESS}
 if device.is_null()||irp.is_null(){return STATUS_INVALID_PARAMETER}
 // Resource arbitration is performed once at the top of the stack and claims
 // belong to the attachment base (PDO), even when a filter receives START first.
 if unsafe{io_get_attached_device(device)}!=device{return STATUS_SUCCESS}
 let resource_owner=unsafe{io_get_device_attachment_base_ref(device)};
 if resource_owner.is_null(){return STATUS_INVALID_PARAMETER}
 let mut raw=[ParsedResource::EMPTY;MAX_RESOURCES_PER_DEVICE];
 let mut resources=[ParsedResource::EMPTY;MAX_RESOURCES_PER_DEVICE];
 let raw_count=match unsafe{parse_cm_resource_list(allocated,&mut raw)}{Ok(n)=>n,Err(e)=>return e};
 let count=match unsafe{parse_cm_resource_list(translated,&mut resources)}{Ok(n)=>n,Err(e)=>return e};
 if allocated.is_null()!=translated.is_null()||raw_count!=count{return STATUS_INVALID_PARAMETER}
 // Translation may relocate a resource but cannot change its resource class.
 // In particular, a bus must not turn a DMA channel into an IRQ or MMIO claim.
 for i in 0..count {
  if raw[i].kind!=resources[i].kind{return STATUS_INVALID_PARAMETER}
  for j in i+1..count {
   if resources_overlap(&resources[i],&resources[j]) { return STATUS_INVALID_PARAMETER; }
  }
 }
 let mut arbiter=RESOURCE_ARBITER.lock();
 if arbiter.claims.iter().any(|c|c.used&&c.owner==resource_owner&&c.irp==irp&&!c.committed){return STATUS_SUCCESS}
 if arbiter.claims.iter().any(|c|c.used&&c.owner==resource_owner&&!c.committed){return STATUS_DEVICE_BUSY}
 for r in resources[..count].iter() {
  for claim in arbiter.claims.iter().filter(|c|c.used&&c.owner!=resource_owner) {
   let existing=ParsedResource{kind:claim.kind,start:claim.start,length:claim.length};
   if claim.owner!=resource_owner && resources_overlap(r,&existing){return STATUS_DEVICE_BUSY}
  }
 }
 let free_count=arbiter.claims.iter().filter(|c|!c.used).count();
 if free_count<count{return STATUS_INSUFFICIENT_RESOURCES}
 for r in resources[..count].iter() {
  let Some(slot)=arbiter.claims.iter_mut().find(|c|!c.used) else{return STATUS_INSUFFICIENT_RESOURCES};
  *slot=ResourceClaim{used:true,owner:resource_owner,irp,kind:r.kind,start:r.start,length:r.length,committed:false};
 }
 STATUS_SUCCESS
}

/// Resolve a PnP resource transaction only when IoCompleteRequest reaches the
/// terminal IRP unwind. Failed/cancelled starts drop tentative claims; successful
/// starts replace prior claims. Stop/remove release committed reservations.
pub(crate) fn io_finish_pnp_resources(irp:*mut Irp,device:*mut DeviceObject,minor:u8,status:i32){
 if irp.is_null()||device.is_null(){return}
 // Only the top-of-stack completion owns the transaction. Claims are attached
 // to the stack base so filters cannot double-reserve or prematurely release them.
 if unsafe{io_get_attached_device(device)}!=device{return}
 let resource_owner=unsafe{io_get_device_attachment_base_ref(device)};
 if resource_owner.is_null(){return}
 let device_deleting={let state=WDM.lock();state.devices.iter().find(|s|s.used&&ptr::eq(&s.object,device)).is_none_or(|s|s.deleting)};
 let effective_status=if minor==IRP_MN_START_DEVICE&&device_deleting{STATUS_DELETE_PENDING}else{status};
 let mut arbiter=RESOURCE_ARBITER.lock();
 if minor==IRP_MN_START_DEVICE {
  let has_pending=arbiter.claims.iter().any(|c|c.used&&c.irp==irp&&c.owner==resource_owner&&!c.committed);
  if effective_status==STATUS_SUCCESS {
   for c in arbiter.claims.iter_mut().filter(|c|c.used&&c.owner==resource_owner&&c.committed){*c=ResourceClaim::EMPTY;}
   for c in arbiter.claims.iter_mut().filter(|c|c.used&&c.irp==irp&&c.owner==resource_owner&&!c.committed){c.committed=true;c.irp=ptr::null_mut();}
  } else if has_pending {
   for c in arbiter.claims.iter_mut().filter(|c|c.used&&c.irp==irp&&c.owner==resource_owner&&!c.committed){*c=ResourceClaim::EMPTY;}
  }
 } else if status==STATUS_SUCCESS&&(minor==IRP_MN_STOP_DEVICE||minor==IRP_MN_REMOVE_DEVICE||minor==IRP_MN_SURPRISE_REMOVAL) {
  for c in arbiter.claims.iter_mut().filter(|c|c.used&&c.owner==resource_owner){*c=ResourceClaim::EMPTY;}
 }
}

/// Explicitly release a device's committed resources after a successful
/// STOP_DEVICE/REMOVE_DEVICE completion. Safe to call repeatedly.
pub(crate) fn io_release_pnp_resources(device:*mut DeviceObject){
 if device.is_null(){return}
 // Resolve the stack base before taking the arbiter lock. START takes the
 // WDM lock first and the resource lock second; preserve that global order.
 let owner=unsafe{io_get_device_attachment_base_ref(device)};
 if owner.is_null(){return}
 let mut arbiter=RESOURCE_ARBITER.lock();
 for c in arbiter.claims.iter_mut().filter(|c|c.used&&c.owner==owner){*c=ResourceClaim::EMPTY;}
}

/// PnP manager bookkeeping for bus-enumerated device relations. The bus driver
/// remains responsible for constructing a valid DEVICE_RELATIONS array; this
/// manager validates every returned pointer against the I/O manager's live
/// device-object table before publishing the relation.
const MAX_PNP_RELATIONS:usize=64;
#[repr(C)]
pub struct DeviceRelations { pub count:u32, pub objects:[*mut DeviceObject;MAX_PNP_RELATIONS] }
#[derive(Clone,Copy)]
struct PnpRelation { used:bool, bus:*mut DeviceObject, child:*mut DeviceObject }
unsafe impl Send for PnpRelation {}
impl PnpRelation { const EMPTY:Self=Self{used:false,bus:ptr::null_mut(),child:ptr::null_mut()}; }
const MAX_PNP_MANAGER_IRPS:usize=64;
#[derive(Clone,Copy)]
struct ManagedPnpIrp { used:bool, irp:*mut Irp, device:*mut DeviceObject, minor:u8, returned:bool, completed:bool, status:i32, remove_after_surprise:bool, enumerate_bus:bool }
unsafe impl Send for ManagedPnpIrp {}
impl ManagedPnpIrp { const EMPTY:Self=Self{used:false,irp:ptr::null_mut(),device:ptr::null_mut(),minor:0,returned:false,completed:false,status:STATUS_PENDING,remove_after_surprise:false,enumerate_bus:false}; }
struct PnpManagerState { relations:[PnpRelation;MAX_PNP_DEVICES], managed_irps:[ManagedPnpIrp;MAX_PNP_MANAGER_IRPS], enumerations:u64 }
const MAX_PNP_DEVICES:usize=128;
impl PnpManagerState { const fn new()->Self{Self{relations:[PnpRelation::EMPTY;MAX_PNP_DEVICES],managed_irps:[ManagedPnpIrp::EMPTY;MAX_PNP_MANAGER_IRPS],enumerations:0}} }
static PNP_MANAGER:IrqSpinLock<PnpManagerState>=IrqSpinLock::new(PnpManagerState::new());

/// Publish bus relations after a successful IRP_MN_QUERY_DEVICE_RELATIONS
/// completion. The relations object and every child pointer must be kernel-owned
/// and readable; unknown/deleting objects are rejected before any state changes.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn IoPnPProcessBusRelations(bus:*mut DeviceObject,relations:*const DeviceRelations)->i32{
 if bus.is_null()||relations.is_null(){return STATUS_INVALID_PARAMETER}
 let count=unsafe{ptr::read_unaligned(ptr::addr_of!((*relations).count))} as usize;
 if count>MAX_PNP_RELATIONS{return STATUS_INVALID_PARAMETER}
 let mut children=[ptr::null_mut::<DeviceObject>();MAX_PNP_RELATIONS];
 for i in 0..count {
  let child=unsafe{ptr::read_unaligned(ptr::addr_of!((*relations).objects[i]))};
  if child.is_null()||child==bus{return STATUS_INVALID_PARAMETER}
  let valid={let state=WDM.lock();state.devices.iter().any(|s|s.used&&!s.deleting&&ptr::eq(&s.object,child))};
  if !valid{return STATUS_INVALID_PARAMETER}
  if children[..i].contains(&child){return STATUS_INVALID_PARAMETER}
  children[i]=child;
 }
 let mut removed=[ptr::null_mut::<DeviceObject>();MAX_PNP_DEVICES];
 let mut removed_count=0usize;
 let mut manager=PNP_MANAGER.lock();
 let free=manager.relations.iter().filter(|r|!r.used||r.bus==bus).count();
 if free<count{return STATUS_INSUFFICIENT_RESOURCES}
 for relation in manager.relations.iter().filter(|r|r.used&&r.bus==bus&&!children[..count].contains(&r.child)) {
  if removed_count<removed.len(){removed[removed_count]=relation.child;removed_count+=1;}
 }
 for relation in manager.relations.iter_mut().filter(|r|r.used&&r.bus==bus&&!children[..count].contains(&r.child)){*relation=PnpRelation::EMPTY;}
 for child in children[..count].iter().copied(){
  if !manager.relations.iter().any(|r|r.used&&r.bus==bus&&r.child==child){
   if let Some(slot)=manager.relations.iter_mut().find(|r|!r.used||r.bus==bus&&!children[..count].contains(&r.child)){
    *slot=PnpRelation{used:true,bus,child};
   }
  }
 }
 manager.enumerations=manager.enumerations.wrapping_add(1);
 drop(manager);
 // Children removed from BusRelations must go through real PnP teardown rather
 // than simply disappearing from the enumeration table.
 for child in removed[..removed_count].iter().copied(){
  let status=unsafe{IoPnPRemoveDevice(child)};
  crate::ntoskrnl::pnp::remove_pnp_device_object(child,status);
 }
 STATUS_SUCCESS
}

/// Dispatch one real IRP_MJ_PNP lifecycle request under PnP-manager ownership.
/// START_DEVICE is validated/resourced by the normal Po/I/O dispatch path.
/// Pending IRPs remain owned until terminal IoCompleteRequest.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn IoPnPDispatchLifecycle(device:*mut DeviceObject,minor:u8,allocated:*const c_void,translated:*const c_void)->i32 {
 unsafe{io_pnp_dispatch_lifecycle_internal(device,minor,allocated,translated,false)}
}
unsafe fn io_pnp_dispatch_lifecycle_internal(device:*mut DeviceObject,minor:u8,allocated:*const c_void,translated:*const c_void,remove_after_surprise:bool)->i32 {
 if device.is_null()||!matches!(minor,0x00|0x01|0x02|0x03|0x04|0x05|0x06|0x07|0x09|0x0a|0x0b|0x17){return STATUS_INVALID_PARAMETER}
 let dispatch_device=unsafe{io_get_attached_device(device)};
 if dispatch_device.is_null(){return STATUS_DELETE_PENDING}
 let referenced=unsafe{io_reference_device_object(dispatch_device)};
 if referenced.is_null(){return STATUS_DELETE_PENDING}
 let stack_size=unsafe{(*dispatch_device).stack_size.max(1)};
 let irp=unsafe{io_allocate_irp(stack_size,0)};
 if irp.is_null(){unsafe{io_dereference_device_object(dispatch_device)};return STATUS_INSUFFICIENT_RESOURCES}
 let stack=unsafe{io_get_next_irp_stack_location(irp)};
 if stack.is_null(){unsafe{io_free_irp(irp);io_dereference_device_object(device)};return STATUS_INSUFFICIENT_RESOURCES}
 unsafe{
  (*stack).major_function=0x1b;(*stack).minor_function=minor;
  ptr::write_unaligned(stack.cast::<u8>().add(4).cast::<*mut c_void>(),allocated as *mut c_void);
  ptr::write_unaligned(stack.cast::<u8>().add(12).cast::<*mut c_void>(),translated as *mut c_void);
 }
 {
  let mut manager=PNP_MANAGER.lock();
  let Some(slot)=manager.managed_irps.iter_mut().find(|s|!s.used) else {
   drop(manager);unsafe{io_free_irp(irp);io_dereference_device_object(device)};
   return STATUS_INSUFFICIENT_RESOURCES
  };
  *slot=ManagedPnpIrp{used:true,irp,device:dispatch_device,minor,returned:false,completed:false,status:STATUS_PENDING,remove_after_surprise,enumerate_bus:minor==0x07};
 }
 let status=unsafe{io_call_driver(dispatch_device,irp)};
 let terminal={let state=WDM.lock();state.irps.iter().find(|s|s.used&&ptr::eq(&s.object,irp)).is_some_and(|s|s.completion_state==2)};
 if status!=STATUS_PENDING&&!terminal {
  unsafe{
   let status_bytes=status.to_le_bytes();
   ptr::copy_nonoverlapping(status_bytes.as_ptr(),ptr::addr_of_mut!((*irp).bytes).cast::<u8>().add(IRP_STATUS_OFFSET),status_bytes.len());
   io_complete_request(irp,0);
  }
 }
 let completed={
  let mut manager=PNP_MANAGER.lock();
  if let Some(slot)=manager.managed_irps.iter_mut().find(|s|s.used&&s.irp==irp){
   slot.returned=true;
   if slot.completed{let result=*slot;*slot=ManagedPnpIrp::EMPTY;Some(result)}else{None}
  }else{None}
 };
 let mut final_status=status;
 if let Some(request)=completed {
  if request.enumerate_bus&&request.status==STATUS_SUCCESS {
   let relations=unsafe{read_ptr((*request.irp).bytes.as_ptr(),IRP_INFORMATION_OFFSET)};
   let _=unsafe{IoPnPProcessBusRelations(request.device,relations.cast::<DeviceRelations>())};
  }
  if request.minor==IRP_MN_REMOVE_DEVICE&&request.status==STATUS_SUCCESS {
   crate::ntoskrnl::pnp::remove_pnp_device_object(request.device,request.status);
  }
  unsafe{io_free_irp(request.irp);}
  if request.remove_after_surprise&&request.status==STATUS_SUCCESS {
   let remove_status=unsafe{io_pnp_dispatch_lifecycle_internal(request.device,IRP_MN_REMOVE_DEVICE,ptr::null(),ptr::null(),false)};
   if status!=STATUS_PENDING {final_status=remove_status;}
  }
  unsafe{io_dereference_device_object(request.device);}
 }
 final_status
}
fn io_pnp_manager_irp_completed(irp:*mut Irp){
 if irp.is_null(){return}
 let status=unsafe{ptr::read_unaligned(ptr::addr_of!((*irp).bytes).cast::<u8>().add(IRP_STATUS_OFFSET).cast::<i32>())};
 let completed={
  let mut manager=PNP_MANAGER.lock();
  let Some(slot)=manager.managed_irps.iter_mut().find(|s|s.used&&s.irp==irp) else{return};
  slot.completed=true;slot.status=status;
  if slot.returned{let result=*slot;*slot=ManagedPnpIrp::EMPTY;Some(result)}else{None}
 };
 if let Some(request)=completed {
  if request.enumerate_bus&&status==STATUS_SUCCESS {
   let relations=unsafe{read_ptr((*request.irp).bytes.as_ptr(),IRP_INFORMATION_OFFSET)};
   let _=unsafe{IoPnPProcessBusRelations(request.device,relations.cast::<DeviceRelations>())};
  }
  if request.minor==IRP_MN_REMOVE_DEVICE&&status==STATUS_SUCCESS {
   crate::ntoskrnl::pnp::remove_pnp_device_object(request.device,status);
  }
  unsafe{io_free_irp(request.irp);}
  if request.remove_after_surprise&&status==STATUS_SUCCESS {
   let _=unsafe{io_pnp_dispatch_lifecycle_internal(request.device,IRP_MN_REMOVE_DEVICE,ptr::null(),ptr::null(),false)};
  }
  unsafe{io_dereference_device_object(request.device);}
 }
}

/// Query a synchronous IRP_MN_QUERY_INTERFACE on a bus PDO/FDO. The
/// returned interface is copied into caller-owned storage; the provider retains
/// ownership of its context and callback targets for the device lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn IoPnPQueryBusInterface(
 bus:*mut DeviceObject,
 interface_guid:*const crate::nt_abi::bus::GUID,
 interface_size:u16,
 interface_version:u16,
 output:*mut c_void,
)->i32 {
 if bus.is_null()||interface_guid.is_null()||output.is_null()||interface_size==0 {
  return STATUS_INVALID_PARAMETER;
 }
 let dispatch_device=unsafe{io_get_attached_device(bus)};
 if dispatch_device.is_null(){return STATUS_DELETE_PENDING}
 let referenced=unsafe{io_reference_device_object(dispatch_device)};
 if referenced.is_null(){return STATUS_DELETE_PENDING}
 let stack_size=unsafe{(*dispatch_device).stack_size.max(1)};
 let irp=unsafe{io_allocate_irp(stack_size,0)};
 if irp.is_null(){unsafe{io_dereference_device_object(dispatch_device)};return STATUS_INSUFFICIENT_RESOURCES}
 let stack=unsafe{io_get_next_irp_stack_location(irp)};
 if stack.is_null(){unsafe{io_free_irp(irp);io_dereference_device_object(dispatch_device)};return STATUS_INSUFFICIENT_RESOURCES}
 unsafe{
  (*stack).major_function=0x1b;
  (*stack).minor_function=0x08;
  ptr::write_unaligned((*stack).parameters.as_mut_ptr().cast::<*const crate::nt_abi::bus::GUID>(),interface_guid);
  ptr::write_unaligned((*stack).parameters.as_mut_ptr().add(8).cast::<u16>(),interface_size);
  ptr::write_unaligned((*stack).parameters.as_mut_ptr().add(10).cast::<u16>(),interface_version);
  ptr::write_unaligned((*stack).parameters.as_mut_ptr().add(16).cast::<*mut c_void>(),output);
 }
 let status=unsafe{io_call_driver(dispatch_device,irp)};
 // Bus interface queries are synchronous by contract. If a provider violates
 // that contract, retain the IRP and device reference rather than freeing
 // memory that a pending completion could still access.
 if status==STATUS_PENDING{return STATUS_PENDING}
 let io_status=unsafe{
  ptr::read_unaligned(ptr::addr_of!((*irp).bytes).cast::<u8>().add(IRP_STATUS_OFFSET).cast::<i32>())
 };
 unsafe{io_free_irp(irp);io_dereference_device_object(dispatch_device);}
 if status==STATUS_SUCCESS{io_status}else{status}
}

/// Ask a bus driver for IRP_MN_QUERY_DEVICE_RELATIONS / BusRelations and
/// publish the returned children when the IRP completes. STATUS_PENDING is
/// supported; the manager retains the IRP and processes relations on completion.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn IoPnPEnumerateBus(bus:*mut DeviceObject)->i32{
 unsafe{IoPnPDispatchLifecycle(bus,0x07,ptr::null(),ptr::null())}
}

/// Retrieve a child PDO already published by a completed BusRelations request.
/// The caller owns no reference from this query; the PnP relation table retains
/// the PDO until a later relation update or explicit device removal.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn IoPnPQueryBusChild(
 bus:*mut DeviceObject,index:u32,output:*mut *mut DeviceObject,
)->i32 {
 if bus.is_null()||output.is_null(){return STATUS_INVALID_PARAMETER}
 unsafe{output.write(ptr::null_mut());}
 let manager=PNP_MANAGER.lock();
 let mut ordinal=0u32;
 for relation in manager.relations.iter().filter(|r|r.used&&r.bus==bus){
  if ordinal==index{
   unsafe{output.write(relation.child);}
   return STATUS_SUCCESS;
  }
  ordinal=ordinal.saturating_add(1);
 }
 STATUS_OBJECT_NAME_NOT_FOUND
}
/// Start a PnP device by sending its real START_DEVICE IRP with the
/// allocated and translated resource lists supplied by the bus/resource arbiter.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn IoPnPStartDevice(device:*mut DeviceObject,allocated:*const c_void,translated:*const c_void)->i32{
 unsafe{IoPnPDispatchLifecycle(device,IRP_MN_START_DEVICE,allocated,translated)}
}
/// Send IRP_MN_QUERY_RESOURCES to the device stack. The bus PDO/FDO owns
/// the returned CM_RESOURCE_LIST; this dispatch does not allocate or mutate it.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn IoPnPQueryResources(device:*mut DeviceObject)->i32 {
 unsafe{IoPnPDispatchLifecycle(device,0x0a,ptr::null(),ptr::null())}
}

/// Send IRP_MN_QUERY_RESOURCE_REQUIREMENTS to the device stack. The bus driver
/// supplies requirements; the kernel arbiter validates translated claims at START.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn IoPnPQueryResourceRequirements(device:*mut DeviceObject)->i32 {
 unsafe{IoPnPDispatchLifecycle(device,0x0b,ptr::null(),ptr::null())}
}

/// Rebalance a started device using the caller's newly arbitrated resource
/// lists. The stop must complete before start is issued; STATUS_PENDING is
/// propagated and callers must resume the transaction from completion.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn IoPnPRebalanceDevice(
 device:*mut DeviceObject, allocated:*const c_void, translated:*const c_void
)->i32 {
 let stop=unsafe{IoPnPStopDevice(device)};
 if stop!=STATUS_SUCCESS{return stop}
 unsafe{IoPnPStartDevice(device,allocated,translated)}
}

/// Stop a started device through its driver's IRP_MN_STOP_DEVICE dispatch.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn IoPnPStopDevice(device:*mut DeviceObject)->i32{
 unsafe{IoPnPDispatchLifecycle(device,IRP_MN_STOP_DEVICE,ptr::null(),ptr::null())}
}
/// Remove a device using the required surprise-removal-before-remove sequence.
/// If surprise removal completes asynchronously, the manager automatically
/// dispatches REMOVE_DEVICE after successful terminal completion.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn IoPnPRemoveDevice(device:*mut DeviceObject)->i32{
 if device.is_null(){return STATUS_INVALID_PARAMETER}
 let state=io_pnp_state(device);
 if state<0{return STATUS_INVALID_PARAMETER}
 if state==PNP_STATE_REMOVED as i32{return STATUS_SUCCESS}
 if state==PNP_STATE_STARTED as i32 {
  // The manager owns the continuation: successful surprise-removal completion
  // automatically queues REMOVE_DEVICE, including when the first IRP pends.
  return unsafe{io_pnp_dispatch_lifecycle_internal(device,IRP_MN_SURPRISE_REMOVAL,ptr::null(),ptr::null(),true)}
 }
 if state!=PNP_STATE_SURPRISE_REMOVED as i32&&state!=PNP_STATE_STOPPED as i32&&state!=PNP_STATE_NOT_STARTED as i32{return STATUS_DEVICE_BUSY}
 unsafe{IoPnPDispatchLifecycle(device,IRP_MN_REMOVE_DEVICE,ptr::null(),ptr::null())}
}

/// Return the number of currently published children for a bus. This is an
/// enumeration snapshot count, not a claim that those children have started.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn IoPnPQueryChildCount(bus:*mut DeviceObject)->u32{
 if bus.is_null(){return 0}
 let manager=PNP_MANAGER.lock();
 manager.relations.iter().filter(|r|r.used&&r.bus==bus).count() as u32
}

pub(crate) fn io_pnp_state(device:*mut DeviceObject)->i32 {
 if device.is_null(){return -1}
 let state=WDM.lock();state.devices.iter().find(|s|s.used&&ptr::eq(&s.object,device)).map(|s|s.pnp_state as i32).unwrap_or(-1)
}
/// Atomically publish a committed D-state while the device is still live.
/// The Po layer owns the WDK ABI; this helper only protects I/O device metadata.
pub(crate) fn io_set_device_power_state(device:*mut DeviceObject,next:u8)->Option<u8> {
 if device.is_null() || next>DEVICE_POWER_D3 { return None; }
 let mut state=WDM.lock();
 let slot=state.devices.iter_mut().find(|s|s.used&&ptr::eq(&s.object,device)&&!s.deleting)?;
 let old=slot.power_state;
 slot.power_state=next;
 Some(old)
}

pub fn device_power_state(device:*mut DeviceObject)->i32 {
 if device.is_null(){return -1}
 let state=WDM.lock();
 state.devices.iter().find(|s|s.used&&ptr::eq(&s.object,device)).map(|s|s.power_state as i32).unwrap_or(-1)
}
