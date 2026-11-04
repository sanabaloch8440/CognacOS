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
struct ResourceClaim {
 used:bool, owner:*mut DeviceObject, irp:*mut Irp, kind:u8, start:u64, length:u64, committed:bool,
 interface_type:u32, bus_number:u32, bus_level:u32, bus_group:u16, bus_vector:u32,
 system_vector:u32, irql:u32, group:u16, affinity:u64, flags:u16, share:u8,
}
unsafe impl Send for ResourceClaim {}
impl ResourceClaim { const EMPTY:Self=Self{
 used:false,owner:ptr::null_mut(),irp:ptr::null_mut(),kind:0,start:0,length:0,committed:false,
 interface_type:0,bus_number:0,bus_level:0,bus_group:0,bus_vector:0,system_vector:0,irql:0,affinity:0,flags:0,share:0,
}; }
struct ResourceArbiter { claims:[ResourceClaim;MAX_PNP_RESOURCE_CLAIMS] }
impl ResourceArbiter { const fn new()->Self{Self{claims:[ResourceClaim::EMPTY;MAX_PNP_RESOURCE_CLAIMS]}} }
static RESOURCE_ARBITER:IrqSpinLock<ResourceArbiter>=IrqSpinLock::new(ResourceArbiter::new());
#[derive(Clone,Copy)]
struct ParsedResource {
 kind:u8, start:u64, length:u64, interface_type:u32, bus_number:u32,
 level:u32, group:u16, vector:u32, affinity:u64, flags:u16, share:u8,
}
impl ParsedResource { const EMPTY:Self=Self{
 kind:0,start:0,length:0,interface_type:0,bus_number:0,level:0,group:0,vector:0,affinity:0,flags:0,share:0,
}; }

fn read_u16_at(base:*const u8,off:usize)->u16 { unsafe{ptr::read_unaligned(base.add(off).cast::<u16>())} }
fn read_u32_at(base:*const u8,off:usize)->u32 { unsafe{ptr::read_unaligned(base.add(off).cast::<u32>())} }
fn read_u64_at(base:*const u8,off:usize)->u64 { unsafe{ptr::read_unaligned(base.add(off).cast::<u64>())} }

/// Validate a WDK CM_RESOURCE_LIST and extract resources that participate in
/// resource arbitration, including legal shared line interrupts. Structure bounds/counts are capped to avoid
/// unbounded traversal. As with the WDK ABI, the pointer must reference a
/// readable kernel allocation for the duration of this call.
unsafe fn parse_cm_resource_list(list:*const c_void,is_translated:bool,out:&mut[ParsedResource;MAX_RESOURCES_PER_DEVICE])->Result<usize,i32>{
 if list.is_null(){return Ok(0)}
 let base=list.cast::<u8>();
 let full_count=unsafe{read_u32_at(base,0)} as usize;
 if full_count>MAX_PNP_FULL_DESCRIPTORS{return Err(STATUS_INVALID_PARAMETER)}
 let mut cursor=4usize;let mut count=0usize;
 for _ in 0..full_count {
  let interface_type=unsafe{read_u32_at(base,cursor)};
  let bus_number=unsafe{read_u32_at(base,cursor+4)};
  let partial_count=unsafe{read_u32_at(base,cursor+12)} as usize;
  if partial_count>MAX_PNP_PARTIAL_DESCRIPTORS{return Err(STATUS_INVALID_PARAMETER)}
  // WDK CM_PARTIAL_RESOURCE_DESCRIPTOR is 20 bytes on x64 (pack(4)):
  // four-byte header plus a 16-byte union, including 64-bit interrupt affinity.
  const CM_PARTIAL_RESOURCE_DESCRIPTOR_SIZE:usize=24;
  let descriptor_start=cursor.checked_add(16).ok_or(STATUS_INVALID_PARAMETER)?;
  let bytes=partial_count.checked_mul(CM_PARTIAL_RESOURCE_DESCRIPTOR_SIZE).ok_or(STATUS_INVALID_PARAMETER)?;
  let end=descriptor_start.checked_add(bytes).ok_or(STATUS_INVALID_PARAMETER)?;
  for i in 0..partial_count {
   let off=descriptor_start+i*CM_PARTIAL_RESOURCE_DESCRIPTOR_SIZE;
   let kind=unsafe{*base.add(off)};
   let share=unsafe{*base.add(off+1)};
   let flags=unsafe{read_u16_at(base,off+2)};
   match kind {
    0|5|6=>{}, // Null, device-specific, and bus-number resources are not global ranges.
    4=>{
     let channel=unsafe{read_u32_at(base,off+4)} as u64;
     if count==out.len(){return Err(STATUS_INSUFFICIENT_RESOURCES)}
     out[count]=ParsedResource{kind:4,start:channel,length:1,interface_type,bus_number,level:0,group:0,vector:0,affinity:0,flags,share};count+=1;
    }
    CM_RESOURCE_PORT|CM_RESOURCE_MEMORY=>{
     let start=unsafe{read_u64_at(base,off+4)};
     let length=unsafe{read_u32_at(base,off+12)} as u64;
     if length==0||start.checked_add(length).is_none(){return Err(STATUS_INVALID_PARAMETER)}
     if count==out.len(){return Err(STATUS_INSUFFICIENT_RESOURCES)}
     out[count]=ParsedResource{kind,start,length,interface_type,bus_number,level:0,group:0,vector:0,affinity:0,flags,share};count+=1;
    }
    CM_RESOURCE_INTERRUPT=>{
     let (level,group,vector,affinity)=if flags&0x0002!=0&&!is_translated {
      // Raw MSI descriptor: Group, Reserved, MessageCount, Vector, Affinity.
      (0,unsafe{read_u16_at(base,off+4)},unsafe{read_u32_at(base,off+12)},unsafe{read_u64_at(base,off+16)})
     } else {
      // Line-based, or translated MSI descriptor: Level, Group, Vector, Affinity.
      (unsafe{read_u16_at(base,off+4)} as u32,unsafe{read_u16_at(base,off+6)},
       unsafe{read_u32_at(base,off+8)},unsafe{read_u64_at(base,off+12)})
     };
     if is_translated&&(vector==0||affinity==0){return Err(STATUS_INVALID_PARAMETER)}
     if count==out.len(){return Err(STATUS_INSUFFICIENT_RESOURCES)}
     out[count]=ParsedResource{kind,start:vector as u64,length:1,interface_type,bus_number,level,group,vector,affinity,flags,share};count+=1;
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
 if a.kind==CM_RESOURCE_INTERRUPT {
  if a.start!=b.start{return false}
  // Shared line-based resources may overlap only when both assignments are
  // explicitly shareable and level-sensitive. Message-signaled resources may
  // overlap only when both assignments explicitly permit message sharing.
  let a_shared=a.share==3&&((a.flags&0x0002!=0&&a.flags&0x0001!=0)||(a.flags&0x0002==0&&a.flags&0x0001==0));
  let b_shared=b.share==3&&((b.flags&0x0002!=0&&b.flags&0x0001!=0)||(b.flags&0x0002==0&&b.flags&0x0001==0));
  return !(a_shared&&b_shared)
 }
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
 let raw_count=match unsafe{parse_cm_resource_list(allocated,false,&mut raw)}{
  Ok(n)=>n,Err(e)=>{unsafe{io_dereference_device_object(resource_owner)};return e}
 };
 let count=match unsafe{parse_cm_resource_list(translated,true,&mut resources)}{
  Ok(n)=>n,Err(e)=>{unsafe{io_dereference_device_object(resource_owner)};return e}
 };
 if allocated.is_null()!=translated.is_null()||raw_count!=count{
  unsafe{io_dereference_device_object(resource_owner)};return STATUS_INVALID_PARAMETER
 }
 for i in 0..count {
  if raw[i].kind!=resources[i].kind || raw[i].interface_type!=resources[i].interface_type || raw[i].bus_number!=resources[i].bus_number || raw[i].flags!=resources[i].flags || raw[i].share!=resources[i].share{
   unsafe{io_dereference_device_object(resource_owner)};return STATUS_INVALID_PARAMETER
  }
  for j in i+1..count {
   if resources_overlap(&resources[i],&resources[j]){
    unsafe{io_dereference_device_object(resource_owner)};return STATUS_INVALID_PARAMETER
   }
  }
 }
 if count==0{
  unsafe{io_dereference_device_object(resource_owner)};
  return STATUS_SUCCESS
 }
 // Every ResourceClaim owns one object reference. Acquire them before the
 // arbiter lock to preserve the WDM -> resource-arbiter lock order.
 let mut owner_refs=[ptr::null_mut::<DeviceObject>();MAX_RESOURCES_PER_DEVICE];
 owner_refs[0]=resource_owner;
 for i in 1..count{
  let reference=unsafe{io_reference_device_object(resource_owner)};
  if reference.is_null(){
   for held in owner_refs[..i].iter().copied(){if !held.is_null(){unsafe{io_dereference_device_object(held)}}}
   return STATUS_DELETE_PENDING
  }
  owner_refs[i]=reference;
 }
 let mut result=STATUS_SUCCESS;
 let mut transferred=false;
 {
  let mut arbiter=RESOURCE_ARBITER.lock();
  let duplicate=arbiter.claims.iter().any(|c|c.used&&c.owner==resource_owner&&c.irp==irp&&!c.committed);
  if duplicate {
   result=STATUS_SUCCESS;
  }else if arbiter.claims.iter().any(|c|c.used&&c.owner==resource_owner&&!c.committed){
   result=STATUS_DEVICE_BUSY;
  }else{
   for r in resources[..count].iter(){
    for claim in arbiter.claims.iter().filter(|c|c.used&&c.owner!=resource_owner){
     let existing=ParsedResource{kind:claim.kind,start:claim.start,length:claim.length,flags:claim.flags,share:claim.share,..ParsedResource::EMPTY};
     if resources_overlap(r,&existing){result=STATUS_DEVICE_BUSY;break}
    }
    if result!=STATUS_SUCCESS{break}
   }
   if result==STATUS_SUCCESS{
    let free_count=arbiter.claims.iter().filter(|c|!c.used).count();
    if free_count<count{result=STATUS_INSUFFICIENT_RESOURCES}
    else{
     for i in 0..count{
      let slot=arbiter.claims.iter_mut().find(|c|!c.used).unwrap();
      let r=resources[i];
      *slot=ResourceClaim{
       used:true,owner:owner_refs[i],irp,kind:r.kind,start:r.start,length:r.length,committed:false,
       interface_type:raw[i].interface_type,bus_number:raw[i].bus_number,
       bus_level:raw[i].level,bus_group:raw[i].group,bus_vector:raw[i].vector,
       system_vector:r.vector,irql:r.level,group:r.group,affinity:r.affinity,flags:r.flags,share:r.share,
      };
     }
     transferred=true;
    }
   }
  }
 }
 // On duplicate or failure the speculative references were not transferred
 // into the arbiter. Successful insertion transfers one ref per claim.
 if !transferred{
  for held in owner_refs[..count].iter().copied(){if !held.is_null(){unsafe{io_dereference_device_object(held)}}}
 }else{
  // HalGetInterruptVector can be called from the driver's START_DEVICE path,
  // before IoCompleteRequest commits the resource transaction. Publish these
  // tentative mappings now; io_finish_pnp_resources rolls them back on failure.
  for i in 0..count {
   let raw_resource=raw[i];
   let translated_resource=resources[i];
   if translated_resource.kind==CM_RESOURCE_INTERRUPT
       &&translated_resource.flags&0x0002==0
       &&raw_resource.group==0&&translated_resource.group==0
       &&translated_resource.level<=u8::MAX as u32 {
    let _=crate::hal::register_bus_interrupt_translation(
     raw_resource.interface_type,raw_resource.bus_number,raw_resource.level,raw_resource.vector,
     translated_resource.vector,translated_resource.level as u8,translated_resource.affinity,
    );
   }
  }
 }
 result
}

/// Resolve a PnP resource transaction only when IoCompleteRequest reaches the
/// terminal IRP unwind. Failed/cancelled starts drop tentative claims; successful
/// starts replace prior claims. Stop/remove release committed reservations.
pub(crate) fn io_finish_pnp_resources(irp:*mut Irp,device:*mut DeviceObject,minor:u8,status:i32){
 if irp.is_null()||device.is_null(){return}
 if unsafe{io_get_attached_device(device)}!=device{return}
 let resource_owner=unsafe{io_get_device_attachment_base_ref(device)};
 if resource_owner.is_null(){return}
 let device_deleting={let state=WDM.lock();state.devices.iter().find(|s|s.used&&ptr::eq(&s.object,device)).is_none_or(|s|s.deleting)};
 let effective_status=if minor==IRP_MN_START_DEVICE&&device_deleting{STATUS_DELETE_PENDING}else{status};
 let mut released=[ptr::null_mut::<DeviceObject>();MAX_PNP_RESOURCE_CLAIMS];
 let mut released_count=0usize;
 let mut removed_translations=[ResourceClaim::EMPTY;MAX_PNP_RESOURCE_CLAIMS];
 let mut removed_count=0usize;
 {
  let mut arbiter=RESOURCE_ARBITER.lock();
  if minor==IRP_MN_START_DEVICE {
   let has_pending=arbiter.claims.iter().any(|c|c.used&&c.irp==irp&&c.owner==resource_owner&&!c.committed);
   if effective_status==STATUS_SUCCESS {
    for claim in arbiter.claims.iter_mut().filter(|c|c.used&&c.owner==resource_owner&&c.committed){
     released[released_count]=claim.owner;released_count+=1;
     if claim.kind==CM_RESOURCE_INTERRUPT&&claim.flags&0x0002==0&&claim.bus_group==0&&claim.group==0&&claim.irql<=u8::MAX as u32 {
      removed_translations[removed_count]=*claim;removed_count+=1;
     }
     *claim=ResourceClaim::EMPTY;
    }
    for claim in arbiter.claims.iter_mut().filter(|c|c.used&&c.irp==irp&&c.owner==resource_owner&&!c.committed){
     claim.committed=true;claim.irp=ptr::null_mut();
    }
   }else if has_pending{
    for claim in arbiter.claims.iter_mut().filter(|c|c.used&&c.irp==irp&&c.owner==resource_owner&&!c.committed){
     released[released_count]=claim.owner;released_count+=1;
     if claim.kind==CM_RESOURCE_INTERRUPT&&claim.flags&0x0002==0&&claim.bus_group==0&&claim.group==0 {
      removed_translations[removed_count]=*claim;removed_count+=1;
     }
     *claim=ResourceClaim::EMPTY;
    }
   }
  }else if status==STATUS_SUCCESS&&(minor==IRP_MN_STOP_DEVICE||minor==IRP_MN_REMOVE_DEVICE||minor==IRP_MN_SURPRISE_REMOVAL){
   for claim in arbiter.claims.iter_mut().filter(|c|c.used&&c.owner==resource_owner){
    released[released_count]=claim.owner;released_count+=1;
    if claim.kind==CM_RESOURCE_INTERRUPT&&claim.flags&0x0002==0&&claim.bus_group==0&&claim.group==0 {
     removed_translations[removed_count]=*claim;removed_count+=1;
    }
    *claim=ResourceClaim::EMPTY;
   }
  }
 }
 for claim in removed_translations[..removed_count].iter() {
  let _=crate::hal::unregister_bus_interrupt_translation(claim.interface_type,claim.bus_number,claim.bus_level,claim.bus_vector,claim.system_vector,claim.irql as u8,claim.affinity);
 }
 for held in released[..released_count].iter().copied(){if !held.is_null(){unsafe{io_dereference_device_object(held)}}}
 unsafe{io_dereference_device_object(resource_owner)};
}

/// @partial implementation
/// Verify a CONNECT_FULLY_SPECIFIED request against the PDO's pending or committed,
/// translated PnP interrupt resource. A caller cannot connect an arbitrary
/// vector or substitute its own IRQL, affinity, trigger, or sharing policy.
pub(crate) fn io_validate_pnp_interrupt_resource(
 device:*mut DeviceObject, vector:u32, irql:u8, interrupt_mode:u32,
 share_vector:u8, affinity:u64, group:u16,
)->bool{
 if device.is_null()||!(0x20..=0xdf).contains(&vector){return false}
 let owner=unsafe{io_get_device_attachment_base_ref(device)};
 if owner.is_null(){return false}
 let valid={
  let arbiter=RESOURCE_ARBITER.lock();
  let has_pending=arbiter.claims.iter().any(|claim|claim.used&&!claim.committed&&claim.owner==owner);
  arbiter.claims.iter().any(|claim|
   claim.used&&claim.committed!=has_pending&&claim.owner==owner&&claim.kind==CM_RESOURCE_INTERRUPT
    &&claim.system_vector==vector
    &&claim.irql==irql as u32&&claim.group==group
    &&claim.affinity==affinity
    &&(if claim.flags&0x0001!=0{1}else{0})==interrupt_mode
    &&(!(claim.flags&0x0002!=0)||interrupt_mode==1)
    &&(!(claim.flags&0x0002==0&&claim.share==3)||interrupt_mode==0)
    &&(if claim.share==3{1}else{0})==share_vector
  )
 };
 unsafe{io_dereference_device_object(owner)};
 valid
}

/// Translated line-based interrupt resource retained by the PnP arbiter.
/// A single group-zero line interrupt is currently connectable; shared level-sensitive lines are supported.
pub(crate) struct PnpLineInterruptResource {
 pub vector:u32,
 pub irql:u8,
 pub affinity:u64,
 pub interrupt_mode:u32,
 pub share_vector:u8,
}

/// @partial implementation
/// Query the pending START_DEVICE or committed translated interrupt resource.
/// Shared level-sensitive lines are accepted; message-signaled resources are rejected.
pub(crate) fn io_query_pnp_line_interrupt_resource(
 device:*mut DeviceObject,
)->Option<PnpLineInterruptResource>{
 if device.is_null(){return None}
 let owner=unsafe{io_get_device_attachment_base_ref(device)};
 if owner.is_null(){return None}
 let found={
  let arbiter=RESOURCE_ARBITER.lock();
  let has_pending=arbiter.claims.iter().any(|c|c.used&&!c.committed&&c.owner==owner);
  let mut selected=None;
  let mut count=0usize;
  for claim in arbiter.claims.iter().filter(|c|c.used&&c.committed!=has_pending&&c.owner==owner&&c.kind==CM_RESOURCE_INTERRUPT){
   count+=1;
   if count==1{selected=Some(*claim);}
  }
  if count!=1{None}else{selected}
 };
 unsafe{io_dereference_device_object(owner)};
 let claim=found?;
 // CM_RESOURCE_INTERRUPT_MESSAGE = 0x0002; CM_RESOURCE_INTERRUPT_LATCHED = 0x0001.
 if claim.flags&0x0002!=0||claim.group!=0||claim.irql==0
     ||claim.affinity==0||!(0x20..=0xdf).contains(&claim.system_vector){
  return None;
 }
 let interrupt_mode=if claim.flags&0x0001!=0{1}else{0};
 if claim.share==3&&interrupt_mode!=0{return None;}
 Some(PnpLineInterruptResource{
  vector:claim.system_vector,
  irql:claim.irql as u8,
  affinity:claim.affinity,
  interrupt_mode,
  share_vector:if claim.share==3{1}else{0},
 })
}

/// Explicitly release a device's committed resources after a successful
/// STOP_DEVICE/REMOVE_DEVICE completion. Safe to call repeatedly.
pub(crate) fn io_release_pnp_resources(device:*mut DeviceObject){
 if device.is_null(){return}
 let owner=unsafe{io_get_device_attachment_base_ref(device)};
 if owner.is_null(){return}
 let mut released=[ptr::null_mut::<DeviceObject>();MAX_PNP_RESOURCE_CLAIMS];
 let mut released_count=0usize;
 let mut removed_translations=[ResourceClaim::EMPTY;MAX_PNP_RESOURCE_CLAIMS];
 let mut removed_count=0usize;
 {
  let mut arbiter=RESOURCE_ARBITER.lock();
  for claim in arbiter.claims.iter_mut().filter(|c|c.used&&c.owner==owner){
   released[released_count]=claim.owner;released_count+=1;
   if claim.kind==CM_RESOURCE_INTERRUPT&&claim.flags&0x0002==0&&claim.bus_group==0&&claim.group==0 {
    removed_translations[removed_count]=*claim;removed_count+=1;
   }
   *claim=ResourceClaim::EMPTY;
  }
 }
 for claim in removed_translations[..removed_count].iter() {
  let _=crate::hal::unregister_bus_interrupt_translation(claim.interface_type,claim.bus_number,claim.bus_level,claim.bus_vector,claim.system_vector,claim.irql as u8,claim.affinity);
 }
 for held in released[..released_count].iter().copied(){if !held.is_null(){unsafe{io_dereference_device_object(held)}}}
 unsafe{io_dereference_device_object(owner)};
}

/// PnP manager bookkeeping for bus-enumerated device relations. The bus driver
/// remains responsible for constructing a valid DEVICE_RELATIONS array; this
/// manager validates every returned pointer against the I/O manager's live
/// device-object table before publishing the relation.
const MAX_PNP_RELATIONS:usize=64;
#[repr(C)]
pub struct DeviceRelations { pub count:u32, pub objects:[*mut DeviceObject;MAX_PNP_RELATIONS] }
#[derive(Clone,Copy)]
struct PnpRelation { used:bool, bus:*mut DeviceObject, child:*mut DeviceObject, started:bool }
unsafe impl Send for PnpRelation {}
impl PnpRelation { const EMPTY:Self=Self{used:false,bus:ptr::null_mut(),child:ptr::null_mut(),started:false}; }
const MAX_PNP_MANAGER_IRPS:usize=64;
#[derive(Clone,Copy)]
struct ManagedPnpIrp { used:bool, irp:*mut Irp, device:*mut DeviceObject, minor:u8, returned:bool, completed:bool, status:i32, remove_after_surprise:bool, enumerate_bus:bool }
unsafe impl Send for ManagedPnpIrp {}
impl ManagedPnpIrp { const EMPTY:Self=Self{used:false,irp:ptr::null_mut(),device:ptr::null_mut(),minor:0,returned:false,completed:false,status:STATUS_PENDING,remove_after_surprise:false,enumerate_bus:false}; }
struct PnpManagerState { relations:[PnpRelation;MAX_PNP_DEVICES], managed_irps:[ManagedPnpIrp;MAX_PNP_MANAGER_IRPS], enumerations:u64 }
const MAX_PNP_DEVICES:usize=128;
impl PnpManagerState { const fn new()->Self{Self{relations:[PnpRelation::EMPTY;MAX_PNP_DEVICES],managed_irps:[ManagedPnpIrp::EMPTY;MAX_PNP_MANAGER_IRPS],enumerations:0}} }
static PNP_MANAGER:IrqSpinLock<PnpManagerState>=IrqSpinLock::new(PnpManagerState::new());

/// Driver AddDevice registration for bus-enumerated PDOs. This project-owned
/// registration point lets loaded WDM function/filter drivers bind to children
/// discovered after DriverEntry without relying on a private DRIVER_EXTENSION
/// layout. Callbacks always run outside the PnP and WDM locks.
const MAX_PNP_DEVICE_DRIVERS:usize=32;
const MAX_PNP_DRIVER_BINDINGS:usize=MAX_PNP_DEVICES*4;
pub type PnpAddDeviceRoutine=unsafe extern "win64" fn(*mut c_void,*mut DeviceObject)->i32;
#[derive(Clone,Copy)]
struct PnpDeviceDriver { used:bool, driver:*mut c_void, add_device:Option<PnpAddDeviceRoutine> }
unsafe impl Send for PnpDeviceDriver {}
impl PnpDeviceDriver { const EMPTY:Self=Self{used:false,driver:ptr::null_mut(),add_device:None}; }
#[derive(Clone,Copy)]
struct PnpDriverBinding { used:bool, pending:bool, driver:*mut c_void, pdo:*mut DeviceObject }
unsafe impl Send for PnpDriverBinding {}
impl PnpDriverBinding { const EMPTY:Self=Self{used:false,pending:false,driver:ptr::null_mut(),pdo:ptr::null_mut()}; }
static PNP_DEVICE_DRIVERS:IrqSpinLock<[PnpDeviceDriver;MAX_PNP_DEVICE_DRIVERS]>=IrqSpinLock::new([PnpDeviceDriver::EMPTY;MAX_PNP_DEVICE_DRIVERS]);
static PNP_DRIVER_BINDINGS:IrqSpinLock<[PnpDriverBinding;MAX_PNP_DRIVER_BINDINGS]>=IrqSpinLock::new([PnpDriverBinding::EMPTY;MAX_PNP_DRIVER_BINDINGS]);

fn io_pnp_bind_driver(driver:*mut c_void,pdo:*mut DeviceObject,callback:PnpAddDeviceRoutine)->i32 {
 if driver.is_null()||pdo.is_null(){return STATUS_INVALID_PARAMETER}
 {
  let mut bindings=PNP_DRIVER_BINDINGS.lock();
  if bindings.iter().any(|b|b.used&&b.driver==driver&&b.pdo==pdo){return STATUS_SUCCESS}
  let Some(slot)=bindings.iter_mut().find(|b|!b.used) else{return STATUS_INSUFFICIENT_RESOURCES};
  *slot=PnpDriverBinding{used:true,pending:true,driver,pdo};
 }
 let status=unsafe{callback(driver,pdo)};
 let mut bindings=PNP_DRIVER_BINDINGS.lock();
 if let Some(slot)=bindings.iter_mut().find(|b|b.used&&b.driver==driver&&b.pdo==pdo&&b.pending) {
  if status==STATUS_SUCCESS {slot.pending=false;} else {*slot=PnpDriverBinding::EMPTY;}
 }
 status
}

/// Register a WDM-style AddDevice callback for bus-enumerated children.
/// The callback decides whether a PDO belongs to its hardware family and
/// creates/attaches its FDO on a match. Existing children are considered
/// immediately; future BusRelations updates trigger the same callback.
pub unsafe extern "win64" fn IoPnPRegisterDeviceDriver(
 driver:*mut c_void,add_device:Option<PnpAddDeviceRoutine>
)->i32 {
 if driver.is_null()||add_device.is_none(){return STATUS_INVALID_PARAMETER}
 let callback=add_device.unwrap();
 {
  let mut drivers=PNP_DEVICE_DRIVERS.lock();
  if let Some(slot)=drivers.iter_mut().find(|d|d.used&&d.driver==driver){slot.add_device=Some(callback);}
  else if let Some(slot)=drivers.iter_mut().find(|d|!d.used){*slot=PnpDeviceDriver{used:true,driver,add_device:Some(callback)};}
  else{return STATUS_INSUFFICIENT_RESOURCES}
 }
 let children={
  let manager=PNP_MANAGER.lock();
  let mut list=[ptr::null_mut::<DeviceObject>();MAX_PNP_DEVICES];
  let mut n=0usize;
  for relation in manager.relations.iter().filter(|r|r.used) {
   if n<list.len()&&!list[..n].contains(&relation.child){list[n]=relation.child;n+=1;}
  }
  (list,n)
 };
 let mut result=STATUS_SUCCESS;
 for pdo in children.0[..children.1].iter().copied(){
  let status=io_pnp_bind_driver(driver,pdo,callback);
  if status!=STATUS_SUCCESS&&status!=STATUS_NOT_SUPPORTED&&result==STATUS_SUCCESS{result=status;}
 }
 result
}

/// Publish bus relations after a successful IRP_MN_QUERY_DEVICE_RELATIONS
/// completion. The relations object and every child pointer must be kernel-owned
/// and readable; unknown/deleting objects are rejected before any state changes.
pub unsafe extern "win64" fn IoPnPProcessBusRelations(bus:*mut DeviceObject,relations:*const DeviceRelations)->i32{
 if bus.is_null()||relations.is_null(){return STATUS_INVALID_PARAMETER}
 let count=unsafe{ptr::read_unaligned(ptr::addr_of!((*relations).count))} as usize;
 if count>MAX_PNP_RELATIONS{return STATUS_INVALID_PARAMETER}
 // The PnP relation database owns one reference to every published child.
 // Acquire candidate references before taking PNP_MANAGER: WDM reference
 // acquisition takes the WDM lock, and the global lock order is WDM -> PnP.
 let mut children=[ptr::null_mut::<DeviceObject>();MAX_PNP_RELATIONS];
 let mut referenced=0usize;
 for i in 0..count {
  let child=unsafe{ptr::read_unaligned(ptr::addr_of!((*relations).objects[i]))};
  if child.is_null()||child==bus||children[..i].contains(&child){
   for held in children[..referenced].iter().copied(){unsafe{io_dereference_device_object(held)}}
   return STATUS_INVALID_PARAMETER
  }
  let held=unsafe{io_reference_device_object(child)};
  if held.is_null(){
   for prior in children[..referenced].iter().copied(){unsafe{io_dereference_device_object(prior)}}
   return STATUS_DELETE_PENDING
  }
  let valid={let state=WDM.lock();state.devices.iter().any(|s|s.used&&!s.deleting&&ptr::eq(&s.object,held))};
  if !valid{
   unsafe{io_dereference_device_object(held)};
   for prior in children[..referenced].iter().copied(){unsafe{io_dereference_device_object(prior)}}
   return STATUS_INVALID_PARAMETER
  }
  children[i]=held;
  referenced+=1;
 }
 let mut removed=[ptr::null_mut::<DeviceObject>();MAX_PNP_DEVICES];
 let mut removed_count=0usize;
 let mut duplicate_refs=[ptr::null_mut::<DeviceObject>();MAX_PNP_RELATIONS];
 let mut duplicate_count=0usize;
 let mut manager=PNP_MANAGER.lock();
 let free=manager.relations.iter().filter(|r|!r.used||r.bus==bus).count();
 if free<count{
  drop(manager);
  for held in children[..referenced].iter().copied(){unsafe{io_dereference_device_object(held)}}
  return STATUS_INSUFFICIENT_RESOURCES
 }
 for relation in manager.relations.iter().filter(|r|r.used&&r.bus==bus&&!children[..count].contains(&r.child)) {
  if removed_count<removed.len(){removed[removed_count]=relation.child;removed_count+=1;}
 }
 for relation in manager.relations.iter_mut().filter(|r|r.used&&r.bus==bus&&!children[..count].contains(&r.child)){*relation=PnpRelation::EMPTY;}
 for child in children[..count].iter().copied(){
  if manager.relations.iter().any(|r|r.used&&r.bus==bus&&r.child==child){
   duplicate_refs[duplicate_count]=child;duplicate_count+=1;
  }else if let Some(slot)=manager.relations.iter_mut().find(|r|!r.used||r.bus==bus&&!children[..count].contains(&r.child)){
   *slot=PnpRelation{used:true,bus,child,started:false};
  }
 }
 manager.enumerations=manager.enumerations.wrapping_add(1);
 drop(manager);
 // Preserve the first lifecycle failure while still processing every child.
 // This result belongs to bus-relations publication, not the binding routine.
 let mut result=STATUS_SUCCESS;
 // Existing relations retained their original reference; release the
 // speculative reference acquired for this refresh.
 for child in duplicate_refs[..duplicate_count].iter().copied(){unsafe{io_dereference_device_object(child)}}
 // Removed children remain referenced until PnP teardown has completed.
 for child in removed[..removed_count].iter().copied(){
  let status=unsafe{IoPnPRemoveDevice(child)};
  crate::ntoskrnl::pnp::remove_pnp_device_object(child,status);
  let mut bindings=PNP_DRIVER_BINDINGS.lock();
  for binding in bindings.iter_mut().filter(|b|b.used&&b.pdo==child){*binding=PnpDriverBinding::EMPTY;}
  unsafe{io_dereference_device_object(child)};
 }
 // Bind newly published PDOs after releasing the relation and WDM locks.
 let drivers={
  let registered=PNP_DEVICE_DRIVERS.lock();
  let mut list=[PnpDeviceDriver::EMPTY;MAX_PNP_DEVICE_DRIVERS];
  let mut n=0usize;
  for entry in registered.iter().filter(|d|d.used){list[n]=*entry;n+=1;}
  (list,n)
 };
 for child in children[..count].iter().copied(){
  for entry in drivers.0[..drivers.1].iter().copied(){
   if let Some(callback)=entry.add_device {
    let _=io_pnp_bind_driver(entry.driver,child,callback);
   }
  }
 }
 // Disk PDOs are the first storage children in this stack. Start them only
 // after all class/filter AddDevice callbacks have attached their FDOs.
 // Partition PDOs are also FILE_DEVICE_DISK and are started by the same path.
 for child in children[..count].iter().copied(){
  if unsafe{(*child).device_type}!=7{continue}
  let needs_start={
   let manager=PNP_MANAGER.lock();
   manager.relations.iter().any(|r|r.used&&r.bus==bus&&r.child==child&&!r.started)
  };
  if !needs_start{continue}
  let start_status=unsafe{io_pnp_dispatch_lifecycle_internal(child,IRP_MN_START_DEVICE,ptr::null(),ptr::null(),false)};
  if start_status==STATUS_SUCCESS{
   let mut manager=PNP_MANAGER.lock();
   if let Some(relation)=manager.relations.iter_mut().find(|r|r.used&&r.bus==bus&&r.child==child){relation.started=true;}
  }else if start_status!=STATUS_PENDING&&result==STATUS_SUCCESS{
   result=start_status;
  }
 }
 result
}
/// Dispatch one real IRP_MJ_PNP lifecycle request under PnP-manager ownership.
/// START_DEVICE resources are reserved before forwarding and committed only after completion.
/// Pending IRPs remain owned until terminal IoCompleteRequest.
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
 if stack.is_null(){unsafe{io_free_irp(irp);io_dereference_device_object(dispatch_device)};return STATUS_INSUFFICIENT_RESOURCES}
 unsafe{
  (*stack).major_function=0x1b;(*stack).minor_function=minor;
  ptr::write_unaligned(stack.cast::<u8>().add(4).cast::<*mut c_void>(),allocated as *mut c_void);
  ptr::write_unaligned(stack.cast::<u8>().add(12).cast::<*mut c_void>(),translated as *mut c_void);
 }
 // Reserve translated resources before the START_DEVICE IRP reaches the
 // stack. The tentative claim is committed only by terminal completion.
 let resource_status=unsafe{io_prepare_pnp_resources(dispatch_device,irp,minor,allocated,translated)};
 if resource_status!=STATUS_SUCCESS{
  unsafe{io_free_irp(irp);io_dereference_device_object(device);}
  return resource_status
 }
 {
  let mut manager=PNP_MANAGER.lock();
  let Some(slot)=manager.managed_irps.iter_mut().find(|s|!s.used) else {
   drop(manager);
   io_finish_pnp_resources(irp,dispatch_device,minor,STATUS_INSUFFICIENT_RESOURCES);
   unsafe{io_free_irp(irp);io_dereference_device_object(device)};
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
  io_finish_pnp_resources(request.irp,request.device,request.minor,request.status);
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
  io_finish_pnp_resources(request.irp,request.device,request.minor,status);
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
pub unsafe extern "win64" fn IoPnPEnumerateBus(bus:*mut DeviceObject)->i32{
 unsafe{IoPnPDispatchLifecycle(bus,0x07,ptr::null(),ptr::null())}
}

/// Retrieve a child PDO already published by a completed BusRelations request.
/// The caller owns no reference from this query; the PnP relation table retains
/// the PDO until a later relation update or explicit device removal.
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
pub unsafe extern "win64" fn IoPnPStartDevice(device:*mut DeviceObject,allocated:*const c_void,translated:*const c_void)->i32{
 unsafe{IoPnPDispatchLifecycle(device,IRP_MN_START_DEVICE,allocated,translated)}
}
/// Send IRP_MN_QUERY_RESOURCES to the device stack. The bus PDO/FDO owns
/// the returned CM_RESOURCE_LIST; this dispatch does not allocate or mutate it.
pub unsafe extern "win64" fn IoPnPQueryResources(device:*mut DeviceObject)->i32 {
 unsafe{IoPnPDispatchLifecycle(device,0x0a,ptr::null(),ptr::null())}
}

/// Send IRP_MN_QUERY_RESOURCE_REQUIREMENTS to the device stack. The bus driver
/// supplies requirements; the kernel arbiter validates translated claims at START.
pub unsafe extern "win64" fn IoPnPQueryResourceRequirements(device:*mut DeviceObject)->i32 {
 unsafe{IoPnPDispatchLifecycle(device,0x0b,ptr::null(),ptr::null())}
}

/// Rebalance a started device using the caller's newly arbitrated resource
/// lists. The stop must complete before start is issued; STATUS_PENDING is
/// propagated and callers must resume the transaction from completion.
pub unsafe extern "win64" fn IoPnPRebalanceDevice(
 device:*mut DeviceObject, allocated:*const c_void, translated:*const c_void
)->i32 {
 let stop=unsafe{IoPnPStopDevice(device)};
 if stop!=STATUS_SUCCESS{return stop}
 unsafe{IoPnPStartDevice(device,allocated,translated)}
}

/// Stop a started device through its driver's IRP_MN_STOP_DEVICE dispatch.
pub unsafe extern "win64" fn IoPnPStopDevice(device:*mut DeviceObject)->i32{
 unsafe{IoPnPDispatchLifecycle(device,IRP_MN_STOP_DEVICE,ptr::null(),ptr::null())}
}
/// Remove a device using the required surprise-removal-before-remove sequence.
/// If surprise removal completes asynchronously, the manager automatically
/// dispatches REMOVE_DEVICE after successful terminal completion.
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

#[cfg(test)]
mod cm_resource_abi_tests {
 use super::*;

 fn put_u16(bytes:&mut [u8],offset:usize,value:u16){
  bytes[offset..offset+2].copy_from_slice(&value.to_le_bytes());
 }
 fn put_u32(bytes:&mut [u8],offset:usize,value:u32){
  bytes[offset..offset+4].copy_from_slice(&value.to_le_bytes());
 }
 fn put_u64(bytes:&mut [u8],offset:usize,value:u64){
  bytes[offset..offset+8].copy_from_slice(&value.to_le_bytes());
 }

 #[test]
 fn parses_x64_packed_cm_resource_descriptors_at_24_byte_stride(){
  // CM_RESOURCE_LIST header (4), CM_FULL_RESOURCE_DESCRIPTOR header (16),
  // and two 24-byte CM_PARTIAL_RESOURCE_DESCRIPTOR entries.
  let mut translated=[0u8;4+16+48];
  put_u32(&mut translated,0,1);
  put_u32(&mut translated,4,1); // Isa
  put_u32(&mut translated,8,0); // bus zero
  put_u32(&mut translated,16,2); // two partial descriptors
  let irq=20usize;
  translated[irq]=CM_RESOURCE_INTERRUPT;
  translated[irq+1]=1; // device-exclusive
  put_u16(&mut translated,irq+2,0x0001); // latched
  put_u16(&mut translated,irq+4,5); // translated DIRQL
  put_u16(&mut translated,irq+6,0); // processor group
  put_u32(&mut translated,irq+8,0x35); // translated system vector
  put_u64(&mut translated,irq+12,1); // processor affinity
  let mem=irq+24;
  translated[mem]=CM_RESOURCE_MEMORY;
  put_u64(&mut translated,mem+4,0xfee0_0000);
  put_u32(&mut translated,mem+12,0x1000);

  let mut parsed=[ParsedResource::EMPTY;MAX_RESOURCES_PER_DEVICE];
  let count=unsafe{parse_cm_resource_list(translated.as_ptr().cast(),true,&mut parsed)}.unwrap();
  assert_eq!(count,2);
  assert_eq!(parsed[0].vector,0x35);
  assert_eq!(parsed[0].level,5);
  assert_eq!(parsed[0].affinity,1);
  assert_eq!(parsed[1].start,0xfee0_0000);
  assert_eq!(parsed[1].length,0x1000);
 }

 #[test]
 fn raw_interrupt_uses_bus_tuple_and_allows_bus_affinity_placeholder(){
  let mut raw=[0u8;4+16+24];
  put_u32(&mut raw,0,1);
  put_u32(&mut raw,4,1); // Isa
  put_u32(&mut raw,8,0);
  put_u32(&mut raw,16,1);
  let irq=20usize;
  raw[irq]=CM_RESOURCE_INTERRUPT;
  raw[irq+1]=1;
  put_u16(&mut raw,irq+4,9);
  put_u16(&mut raw,irq+6,0);
  put_u32(&mut raw,irq+8,9);
  put_u64(&mut raw,irq+12,0); // raw affinity is not used for system delivery

  let mut parsed=[ParsedResource::EMPTY;MAX_RESOURCES_PER_DEVICE];
  let count=unsafe{parse_cm_resource_list(raw.as_ptr().cast(),false,&mut parsed)}.unwrap();
  assert_eq!(count,1);
  assert_eq!(parsed[0].level,9);
  assert_eq!(parsed[0].vector,9);
  assert_eq!(parsed[0].affinity,0);
 }
 #[test]
 fn only_shared_level_sensitive_interrupts_may_overlap(){
  let shared_a=ParsedResource{kind:CM_RESOURCE_INTERRUPT,start:0x45,length:1,share:3,flags:0,..ParsedResource::EMPTY};
  let shared_b=ParsedResource{kind:CM_RESOURCE_INTERRUPT,start:0x45,length:1,share:3,flags:0,..ParsedResource::EMPTY};
  let exclusive=ParsedResource{kind:CM_RESOURCE_INTERRUPT,start:0x45,length:1,share:1,flags:0,..ParsedResource::EMPTY};
  let latched=ParsedResource{kind:CM_RESOURCE_INTERRUPT,start:0x45,length:1,share:3,flags:0x0001,..ParsedResource::EMPTY};
  assert!(!resources_overlap(&shared_a,&shared_b));
  assert!(resources_overlap(&shared_a,&exclusive));
  assert!(resources_overlap(&shared_a,&latched));
 }

 #[test]
 fn shared_msi_vectors_must_be_latched(){
  let shared_msi_a=ParsedResource{kind:CM_RESOURCE_INTERRUPT,start:0x46,length:1,share:3,flags:0x0003,..ParsedResource::EMPTY};
  let shared_msi_b=ParsedResource{kind:CM_RESOURCE_INTERRUPT,start:0x46,length:1,share:3,flags:0x0003,..ParsedResource::EMPTY};
  let invalid_msi=ParsedResource{kind:CM_RESOURCE_INTERRUPT,start:0x46,length:1,share:3,flags:0x0002,..ParsedResource::EMPTY};
  assert!(!resources_overlap(&shared_msi_a,&shared_msi_b));
  assert!(resources_overlap(&shared_msi_a,&invalid_msi));
 }

 #[test]
 fn parses_raw_and_translated_msi_union_members_at_x64_offsets(){
  let mut raw=[0u8;4+16+24];
  put_u32(&mut raw,0,1);
  put_u32(&mut raw,4,5); // PCI bus
  put_u32(&mut raw,8,2); // bus number
  put_u32(&mut raw,16,1);
  let raw_irq=20usize;
  raw[raw_irq]=CM_RESOURCE_INTERRUPT;
  raw[raw_irq+1]=3; // shared
  put_u16(&mut raw,raw_irq+2,0x0003); // latched + message
  put_u16(&mut raw,raw_irq+4,0); // raw group
  put_u16(&mut raw,raw_irq+8,1); // MessageCount
  put_u32(&mut raw,raw_irq+12,0x21); // raw message vector
  put_u64(&mut raw,raw_irq+16,0);

  let mut translated=[0u8;4+16+24];
  put_u32(&mut translated,0,1);
  put_u32(&mut translated,4,5);
  put_u32(&mut translated,8,2);
  put_u32(&mut translated,16,1);
  let translated_irq=20usize;
  translated[translated_irq]=CM_RESOURCE_INTERRUPT;
  translated[translated_irq+1]=3;
  put_u16(&mut translated,translated_irq+2,0x0003);
  put_u16(&mut translated,translated_irq+4,6); // translated IRQL
  put_u16(&mut translated,translated_irq+6,0); // processor group
  put_u32(&mut translated,translated_irq+8,0x55);
  put_u64(&mut translated,translated_irq+12,1);

  let mut raw_parsed=[ParsedResource::EMPTY;MAX_RESOURCES_PER_DEVICE];
  let mut translated_parsed=[ParsedResource::EMPTY;MAX_RESOURCES_PER_DEVICE];
  assert_eq!(unsafe{parse_cm_resource_list(raw.as_ptr().cast(),false,&mut raw_parsed)}.unwrap(),1);
  assert_eq!(unsafe{parse_cm_resource_list(translated.as_ptr().cast(),true,&mut translated_parsed)}.unwrap(),1);
  assert_eq!(raw_parsed[0].vector,0x21);
  assert_eq!(raw_parsed[0].group,0);
  assert_eq!(translated_parsed[0].vector,0x55);
  assert_eq!(translated_parsed[0].level,6);
  assert_eq!(translated_parsed[0].affinity,1);
 }

}
