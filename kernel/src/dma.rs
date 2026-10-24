//! Early x64 DMA primitives and Windows DMA ABI layout.
//!
//! Windows x64 bootstrap DMA adapter and DMA_OPERATIONS callbacks.
//! DMA is fail-closed unless a real platform IOMMU backend is registered. Logical
//! addresses are IOMMU outputs, not physical-address aliases.
use core::sync::atomic::{fence, Ordering};
#[path = "dma/iommu.rs"] pub(crate) mod iommu;

pub const PAGE_SIZE: u64 = 4096;
pub const MAX_COMMON_BUFFERS: usize = 128;
pub const MAX_SG_SEGMENTS: usize = 256;
const MDL_PAGES_LOCKED: i16 = 0x0002;
const MDL_SOURCE_IS_NONPAGED_POOL: i16 = 0x0004;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DmaError { InvalidLength, OutOfMemory, AddressMask, RegistryFull, NotFound, TooManySegments, InvalidPageList }

/// Windows 11 x64 DMA_OPERATIONS member order. Each member is an eight-byte
/// function-pointer slot on x64; usize preserves exact offsets while avoiding
/// assigning an incompatible callback ABI to unimplemented operations.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DmaOperations {
    pub size: u32,
    pub put_dma_adapter: *const (),
    pub allocate_common_buffer: *const (),
    pub free_common_buffer: *const (),
    pub allocate_adapter_channel: *const (),
    pub flush_adapter_buffers: *const (),
    pub free_adapter_channel: *const (),
    pub free_map_registers: *const (),
    pub map_transfer: *const (),
    pub get_dma_alignment: *const (),
    pub read_dma_counter: *const (),
    pub get_scatter_gather_list: *const (),
    pub put_scatter_gather_list: *const (),
    pub calculate_scatter_gather_list_size: *const (),
    pub build_scatter_gather_list: *const (),
    pub build_mdl_from_scatter_gather_list: *const (),
    pub get_dma_adapter_info: *const (),
    pub get_dma_transfer_info: *const (),
    pub initialize_dma_transfer_context: *const (),
    pub allocate_common_buffer_ex: *const (),
    pub allocate_adapter_channel_ex: *const (),
    pub configure_adapter_channel: *const (),
    pub cancel_adapter_channel: *const (),
    pub map_transfer_ex: *const (),
    pub get_scatter_gather_list_ex: *const (),
    pub build_scatter_gather_list_ex: *const (),
    pub flush_adapter_buffers_ex: *const (),
    pub free_adapter_object: *const (),
    pub cancel_mapped_transfer: *const (),
    pub allocate_domain_common_buffer: *const (),
    pub flush_dma_buffer: *const (),
    pub join_dma_domain: *const (),
    pub leave_dma_domain: *const (),
    pub get_dma_domain: *const (),
    pub allocate_common_buffer_with_bounds: *const (),
    pub allocate_common_buffer_vector: *const (),
    pub get_common_buffer_from_vector_by_index: *const (),
    pub free_common_buffer_from_vector: *const (),
    pub free_common_buffer_vector: *const (),
    pub create_common_buffer_from_mdl: *const (),
    pub register_fault_callback: *const (),
    pub unregister_fault_callback: *const (),
}
unsafe impl Sync for DmaOperations {}

impl DmaOperations {
    pub const ABI_SIZE: usize = core::mem::size_of::<Self>();
    pub const POINTER_COUNT: usize = 41;
}

/// Leading Windows DMA_ADAPTER object layout (Version, Size, Operations).
#[repr(C)]
pub struct DmaAdapterAbi {
    pub version: u16,
    pub size: u16,
    pub operations: *const DmaOperations,
}

const MAX_ADAPTERS:usize=32;
struct AdapterSlot { used:bool,device:usize,version:u32,maximum_address:u64,abi:DmaAdapterAbi }
unsafe impl Send for AdapterSlot {}
impl AdapterSlot { const EMPTY:Self=Self{used:false,device:0,version:0,maximum_address:0,abi:DmaAdapterAbi{version:1,size:core::mem::size_of::<DmaAdapterAbi>() as u16,operations:core::ptr::null()}}; }
static ADAPTERS:crate::hal::IrqSpinLock<[AdapterSlot;MAX_ADAPTERS]>=crate::hal::IrqSpinLock::new([const{AdapterSlot::EMPTY};MAX_ADAPTERS]);
fn create_device_adapter(device:*mut core::ffi::c_void,version:u32,maximum_address:u64)->*mut core::ffi::c_void{
 if device.is_null()||!iommu::available(){return core::ptr::null_mut()}
 let mut s=ADAPTERS.lock();
 if let Some(slot)=s.iter_mut().find(|a|a.used&&a.device==device as usize){return core::ptr::addr_of_mut!(slot.abi).cast()}
 let Some(slot)=s.iter_mut().find(|a|!a.used)else{return core::ptr::null_mut()};
 let operations=match version{0|1=>core::ptr::addr_of!(DMA_OPERATIONS_V1).cast(),2=>core::ptr::addr_of!(DMA_OPERATIONS_V2).cast(),3=>core::ptr::addr_of!(DMA_OPERATIONS).cast(),_=>return core::ptr::null_mut()};
 slot.used=true;slot.device=device as usize;slot.version=version;slot.maximum_address=maximum_address;
 slot.abi=DmaAdapterAbi{version:1,size:core::mem::size_of::<DmaAdapterAbi>() as u16,operations};
 core::ptr::addr_of_mut!(slot.abi).cast()
}
pub(crate) fn adapter_device(adapter:*mut core::ffi::c_void)->Option<usize>{
 if adapter.is_null(){return None}
 let s=ADAPTERS.lock();s.iter().find(|a|a.used&&core::ptr::eq(&a.abi,adapter.cast::<DmaAdapterAbi>())).map(|a|a.device)
}
pub(crate) fn adapter_maximum_address(adapter:*mut core::ffi::c_void)->u64{let s=ADAPTERS.lock();s.iter().find(|a|a.used&&core::ptr::eq(&a.abi,adapter.cast::<DmaAdapterAbi>())).map(|a|a.maximum_address).unwrap_or(0)}
pub(crate) fn release_adapter(adapter:*mut core::ffi::c_void){
 let device={let s=ADAPTERS.lock();s.iter().find(|a|a.used&&core::ptr::eq(&a.abi,adapter.cast::<DmaAdapterAbi>())).map(|a|a.device)};
 let Some(device)=device else{return};
 if iommu::device_busy(device)||MAP_REGISTERS.lock().records.iter().any(|r|r.active&&r.device==device){return}
 let mut s=ADAPTERS.lock();if let Some(a)=s.iter_mut().find(|a|a.used&&a.device==device&&core::ptr::eq(&a.abi,adapter.cast::<DmaAdapterAbi>())){a.used=false;a.device=0;a.version=0;a.maximum_address=0;a.abi.operations=core::ptr::null();}
}

#[derive(Clone, Copy)]
struct BufferRecord { physical: u64, requested: usize, pages: usize }
impl BufferRecord { const EMPTY: Self = Self { physical: 0, requested: 0, pages: 0 }; }
struct BufferStore(core::cell::UnsafeCell<[BufferRecord; MAX_COMMON_BUFFERS]>);
unsafe impl Sync for BufferStore {}
static BUFFERS: BufferStore = BufferStore(core::cell::UnsafeCell::new([BufferRecord::EMPTY; MAX_COMMON_BUFFERS]));
static BUFFER_LOCK: crate::hal::IrqSpinLock<()> = crate::hal::IrqSpinLock::new(());

/// Allocate DMA-safe common memory and map it into the requesting device's IOMMU domain.
pub unsafe fn allocate_common_buffer(_length:usize,_maximum_address:u64)->Result<(*mut u8,u64),DmaError>{ Err(DmaError::AddressMask) }
pub unsafe fn flush_common_buffer_for_adapter(adapter:*mut core::ffi::c_void,cpu_address:*mut u8,length:usize)->Result<(),DmaError>{iommu::flush_common(adapter,cpu_address,length)}
pub unsafe fn free_common_buffer_for_adapter(adapter:*mut core::ffi::c_void,cpu_address:*mut u8,logical_address:u64,length:usize)->Result<(),DmaError>{unsafe{iommu::free_common(adapter,cpu_address,logical_address,length)}}
pub(crate) unsafe fn iommu_allocate_common(adapter:*mut core::ffi::c_void,length:usize,maximum_address:u64)->Result<(*mut u8,u64),DmaError>{unsafe{iommu::allocate_common(adapter,length,maximum_address)}}


#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScatterGatherElement { pub address: u64, pub length: u32, pub reserved: u32 }

/// Pure SG shape helper for tests/planning only. Its output addresses remain
/// physical inputs and MUST NOT be submitted to hardware; hardware-facing WDM
/// SG APIs below use IOMMU logical mappings exclusively.
pub fn build_scatter_gather(
    fragments: &[(u64, u32)], maximum_segment: u32, boundary: u64,
    out: &mut [ScatterGatherElement],
) -> Result<usize, DmaError> {
    if maximum_segment == 0 || (boundary != 0 && !boundary.is_power_of_two()) {
        return Err(DmaError::InvalidLength);
    }
    let mut count = 0usize;
    for &(mut address, mut remaining) in fragments {
        if remaining == 0 || address.checked_add(remaining as u64).is_none() {
            return Err(DmaError::InvalidPageList);
        }
        while remaining != 0 {
            let mut chunk = remaining.min(maximum_segment);
            if boundary != 0 {
                let until_boundary = boundary - (address & (boundary - 1));
                chunk = chunk.min(until_boundary.min(u32::MAX as u64) as u32);
            }
            if count != 0 {
                let prev = &mut out[count - 1];
                let contiguous = prev.address.checked_add(prev.length as u64) == Some(address);
                let same_boundary = boundary == 0 || (prev.address / boundary) == ((address + chunk as u64 - 1) / boundary);
                if contiguous && same_boundary && prev.length.checked_add(chunk).is_some_and(|n| n <= maximum_segment) {
                    prev.length += chunk;
                    address += chunk as u64;
                    remaining -= chunk;
                    continue;
                }
            }
            if count == out.len() || count == MAX_SG_SEGMENTS { return Err(DmaError::TooManySegments); }
            out[count] = ScatterGatherElement { address, length: chunk, reserved: 0 };
            count += 1;
            address += chunk as u64;
            remaining -= chunk;
        }
    }
    Ok(count)
}





fn parse_device_description_prefix(bytes:&[u8])->Option<u32> {
    if bytes.len()<12 { return None; }
    let version=u32::from_le_bytes(bytes[0..4].try_into().ok()?);
    let master=bytes[4]!=0;
    let reserved1=bytes[10];
    if version>3 || !master || reserved1!=0 || bytes[11]>1 { return None; }
    Some(version)
}

/// Return a per-device adapter only when a real IOMMU backend is registered.
pub unsafe extern "win64" fn io_get_dma_adapter(device_object:*mut core::ffi::c_void,device_description:*const core::ffi::c_void,number_of_map_registers:*mut u32)->*mut core::ffi::c_void{
 if device_object.is_null()||device_description.is_null()||number_of_map_registers.is_null()||!iommu::available(){return core::ptr::null_mut()}
 let header=unsafe{core::slice::from_raw_parts(device_description.cast::<u8>(),12)};
 let Some(version)=parse_device_description_prefix(header)else{return core::ptr::null_mut()};
 let maximum=if header[11]!=0{u64::MAX}else{0xffff_ffff};
 let adapter=create_device_adapter(device_object,version,maximum);if adapter.is_null(){return adapter}
 unsafe{number_of_map_registers.write(MAX_MAP_REGISTERS_PER_ALLOCATION)}adapter
}

/// The stable prefix of the Windows x64 MDL. PFN entries follow this header;
/// this implementation translates CurrentVa through the active kernel page tables
/// and deliberately limits a transfer to the current physically contiguous page.
#[repr(C)]
pub struct MdlAbi {
    pub next: *mut MdlAbi,
    pub size: i16,
    pub flags: i16,
    pub process: *mut core::ffi::c_void,
    pub mapped_system_va: *mut core::ffi::c_void,
    pub start_va: *mut core::ffi::c_void,
    pub byte_count: u32,
    pub byte_offset: u32,
}


/// AllocateCommonBufferEx maps backing pages into this adapter's IOMMU domain.
pub unsafe extern "win64" fn allocate_common_buffer_ex(adapter:*mut core::ffi::c_void,length:u32,logical_address:*mut i64,_cache_type:u32,flags:u32)->*mut core::ffi::c_void{
 if !is_dma_adapter(adapter)||logical_address.is_null()||flags!=0||length==0{return core::ptr::null_mut()}
 match unsafe{iommu::allocate_common(adapter,length as usize,u64::MAX)}{Ok((cpu,logical))=>{unsafe{logical_address.write(logical as i64)}cpu.cast()},Err(_)=>core::ptr::null_mut()}
}
/// MapTransfer returns only an IOMMU logical address. A valid allocated map-register
/// token is mandatory; no physical-address fallback exists.
pub unsafe extern "win64" fn map_transfer(adapter:*mut core::ffi::c_void,mdl:*mut MdlAbi,map_register_base:*mut core::ffi::c_void,current_va:*mut core::ffi::c_void,length:*mut u32,write_to_device:u8)->i64{
 if !is_dma_adapter(adapter)||mdl.is_null()||map_register_base.is_null()||current_va.is_null()||length.is_null(){return 0}
 let requested=unsafe{length.read()};if requested==0{return 0}let m=unsafe{&*mdl};
 if validate_mdl_range(m,current_va as u64,requested).is_none(){return 0}
 let physical=unsafe{crate::ntoskrnl::mm::mm_get_physical_address(current_va)};if physical==0{return 0}
 let available=(PAGE_SIZE-(physical&(PAGE_SIZE-1))).min(u32::MAX as u64) as u32;
 let actual=requested.min(available);if actual==0{return 0}
 match iommu::map_transfer(adapter,map_register_base,physical,actual,write_to_device){Ok(logical)=>{unsafe{length.write(actual)}logical as i64},Err(_)=>0}
}
/// Flushes device-visible mapping and unmaps it only after the transfer is done.
pub unsafe extern "win64" fn flush_adapter_buffers(adapter:*mut core::ffi::c_void,mdl:*mut MdlAbi,map_register_base:*mut core::ffi::c_void,current_va:*mut core::ffi::c_void,length:u32,write_to_device:u8)->u8{
 if !is_dma_adapter(adapter)||mdl.is_null()||map_register_base.is_null()||current_va.is_null()||length==0{return 0}
 let physical=unsafe{crate::ntoskrnl::mm::mm_get_physical_address(current_va)};if physical==0{return 0}
 if validate_mdl_range(unsafe{&*mdl},current_va as u64,length).is_none(){return 0}
 if iommu::sync_transfer(adapter,map_register_base,physical,length,write_to_device){1}else{0}
}

/// Windows SCATTER_GATHER_LIST header followed by NumberOfElements elements.
#[repr(C)]
pub struct ScatterGatherListAbi {
    pub number_of_elements: u32,
    pub reserved: u32,
    pub elements: [ScatterGatherElement; 0],
}

pub type ScatterGatherExecuteRoutine = unsafe extern "win64" fn(
    device_object: *mut core::ffi::c_void,
    irp: *mut core::ffi::c_void,
    scatter_gather: *mut ScatterGatherListAbi,
    context: *mut core::ffi::c_void,
);


const MAX_OWNED_SG_LISTS: usize = 32;

#[repr(C)]
struct ScatterGatherListStorage {
    number_of_elements: u32,
    reserved: u32,
    elements: [ScatterGatherElement; MAX_SG_SEGMENTS],
}
#[derive(Clone, Copy)]
struct SgOwnership { active: bool, busy:bool, device: usize, list: usize, token:usize, count:u32 }
impl SgOwnership { const EMPTY: Self = Self { active: false, busy:false, device: 0, list: 0, token:0, count:0 }; }
struct SgRegistry {
    ownership: core::cell::UnsafeCell<[SgOwnership; MAX_OWNED_SG_LISTS]>,
    lists: core::cell::UnsafeCell<[ScatterGatherListStorage; MAX_OWNED_SG_LISTS]>,
}
unsafe impl Sync for SgRegistry {}
static SG_REGISTRY: SgRegistry = SgRegistry {
    ownership: core::cell::UnsafeCell::new([SgOwnership::EMPTY; MAX_OWNED_SG_LISTS]),
    lists: core::cell::UnsafeCell::new([const { ScatterGatherListStorage {
        number_of_elements: 0, reserved: 0,
        elements: [ScatterGatherElement { address: 0, length: 0, reserved: 0 }; MAX_SG_SEGMENTS],
    }}; MAX_OWNED_SG_LISTS]),
};
static SG_LOCK: crate::hal::IrqSpinLock<()> = crate::hal::IrqSpinLock::new(());

/// Obtain an owned SG list. The callback is invoked after the registry lock is
/// released; the list remains valid until PutScatterGatherList releases it.
/// This bootstrap path invokes the callback synchronously (no channel queue yet).
pub unsafe extern "win64" fn get_scatter_gather_list(
 adapter:*mut core::ffi::c_void,device_object:*mut core::ffi::c_void,mdl:*mut MdlAbi,current_va:*mut core::ffi::c_void,length:u32,
 execution_routine:Option<ScatterGatherExecuteRoutine>,context:*mut core::ffi::c_void,write_to_device:u8
)->i32{
 if !is_dma_adapter(adapter)||device_object.is_null()||mdl.is_null()||current_va.is_null()||length==0||execution_routine.is_none(){return 0xC000_000Du32 as i32}
 let m=unsafe{&*mdl};if validate_mdl_range(m,current_va as u64,length).is_none(){return 0xC000_000Du32 as i32}
 let pages=(((current_va as u64&(PAGE_SIZE-1))+length as u64+PAGE_SIZE-1)/PAGE_SIZE) as usize;
 if pages==0||pages>MAX_SG_SEGMENTS{return 0xC000_009Au32 as i32}
 let _guard=SG_LOCK.lock();let owners=unsafe{&mut *SG_REGISTRY.ownership.get()};
 let Some(index)=owners.iter().position(|e|!e.active)else{return 0xC000_009Au32 as i32};
 let lists=unsafe{&mut *SG_REGISTRY.lists.get()};let list=&mut lists[index];let device=match adapter_device(adapter){Some(v)=>v,None=>return 0xC000_000Du32 as i32};
 let token=match allocate_map_register_token(device as *mut core::ffi::c_void,pages as u32){Some(t)=>t,None=>return 0xC000_009Au32 as i32};
 let mut va=current_va as u64;let mut remaining=length as u64;let mut mapped=0usize;
 for i in 0..pages{
  let chunk=remaining.min(PAGE_SIZE-(va&(PAGE_SIZE-1))) as u32;
  let physical=unsafe{crate::ntoskrnl::mm::mm_get_physical_address(va as *mut core::ffi::c_void)};
  if physical==0{for j in 0..mapped{let _=iommu::unmap_logical(device,list.elements[j].address);}unsafe{free_map_registers(adapter,token,pages as u32)}return 0xC000_000Du32 as i32}
  match iommu::map_fragment(adapter,token,physical,chunk,write_to_device){
   Ok(logical)=>{list.elements[i]=ScatterGatherElement{address:logical,length:chunk,reserved:0};mapped+=1;}
   Err(_)=>{for j in 0..mapped{let _=iommu::unmap_logical(device,list.elements[j].address);}unsafe{free_map_registers(adapter,token,pages as u32)}return 0xC000_009Au32 as i32}
  }
  va+=chunk as u64;remaining-=chunk as u64;
 }
 list.number_of_elements=pages as u32;list.reserved=0;
 let list_ptr=(core::ptr::addr_of_mut!(*list)).cast::<ScatterGatherListAbi>();
 owners[index]=SgOwnership{active:true,busy:false,device:device_object as usize,list:list_ptr as usize,token:token as usize,count:pages as u32};
 drop(_guard);fence(Ordering::SeqCst);
 unsafe{execution_routine.unwrap()(device_object,core::ptr::null_mut(),list_ptr,context)};0
}
pub unsafe extern "win64" fn put_scatter_gather_list(adapter:*mut core::ffi::c_void,scatter_gather:*mut ScatterGatherListAbi,write_to_device:u8){
 if !is_dma_adapter(adapter)||scatter_gather.is_null(){return}
 let owner={let _guard=SG_LOCK.lock();let owners=unsafe{&mut *SG_REGISTRY.ownership.get()};let Some(i)=owners.iter().position(|e|e.active&&e.list==scatter_gather as usize&&!e.busy)else{return};owners[i].busy=true;owners[i]};
 let domain=adapter_device(adapter).unwrap_or(owner.device);
 let count=unsafe{(*scatter_gather).number_of_elements as usize};
 if count>MAX_SG_SEGMENTS{let _g=SG_LOCK.lock();let owners=unsafe{&mut *SG_REGISTRY.ownership.get()};if let Some(o)=owners.iter_mut().find(|e|e.active&&e.list==scatter_gather as usize){o.busy=false;}return}
 let elems=unsafe{core::ptr::addr_of_mut!((*scatter_gather).elements).cast::<ScatterGatherElement>()};let mut all_released=true;
 for i in 0..count{let e=unsafe{elems.add(i).read()};if e.address==0{continue}if iommu::unmap_logical(domain,e.address){unsafe{elems.add(i).write(ScatterGatherElement{address:0,length:0,reserved:0});}}else{all_released=false;}}
 if all_released{
  let _g=SG_LOCK.lock();let owners=unsafe{&mut *SG_REGISTRY.ownership.get()};if let Some(o)=owners.iter_mut().find(|e|e.active&&e.list==scatter_gather as usize){*o=SgOwnership::EMPTY;}
  if owner.token!=0{unsafe{free_map_registers(adapter,owner.token as *mut core::ffi::c_void,owner.count)}}
 }else{let _g=SG_LOCK.lock();let owners=unsafe{&mut *SG_REGISTRY.ownership.get()};if let Some(o)=owners.iter_mut().find(|e|e.active&&e.list==scatter_gather as usize){o.busy=false;}}
 let _=write_to_device;fence(Ordering::SeqCst)
}
/// Validate an MDL transfer range using checked arithmetic.
fn validate_mdl_range(mdl: &MdlAbi, current_va: u64, length: u32) -> Option<u64> {
    if mdl.start_va.is_null() || mdl.byte_count == 0 || length == 0
        || mdl.flags & (MDL_PAGES_LOCKED | MDL_SOURCE_IS_NONPAGED_POOL) == 0 { return None; }
    let start = (mdl.start_va as u64).checked_add(mdl.byte_offset as u64)?;
    let end = start.checked_add(mdl.byte_count as u64)?;
    let transfer_end = current_va.checked_add(length as u64)?;
    if current_va < start || transfer_end > end { return None; }
    Some(start)
}

/// Calculate the caller-provided storage size for a worst-case page-fragmented
/// SCATTER_GATHER_LIST. This does not allocate memory or retain an MDL.
pub unsafe extern "win64" fn calculate_scatter_gather_list_size(
    adapter: *mut core::ffi::c_void,
    mdl: *mut MdlAbi,
    current_va: *mut core::ffi::c_void,
    length: u32,
    list_size: *mut u32,
    flags: u32,
) -> i32 {
    if !is_dma_adapter(adapter)
        || mdl.is_null() || current_va.is_null() || list_size.is_null() || flags != 0 {
        return 0xC000_000D_u32 as i32; // STATUS_INVALID_PARAMETER
    }
    let m = unsafe { &*mdl };
    if validate_mdl_range(m, current_va as u64, length).is_none() {
        return 0xC000_000D_u32 as i32;
    }
    let first_offset = (current_va as u64) & (PAGE_SIZE - 1);
    let span = match first_offset.checked_add(length as u64) { Some(v) => v, None => return 0xC000_000D_u32 as i32 };
    let pages = (span + PAGE_SIZE - 1) / PAGE_SIZE;
    let bytes = match core::mem::size_of::<ScatterGatherListAbi>()
        .checked_add((pages as usize).saturating_mul(core::mem::size_of::<ScatterGatherElement>())) {
        Some(v) if v <= u32::MAX as usize => v as u32,
        _ => return 0xC000_009A_u32 as i32, // STATUS_INSUFFICIENT_RESOURCES
    };
    unsafe { list_size.write(bytes); }
    0
}

/// Build an SG list in caller-owned nonpaged storage and invoke the driver's
/// execution callback synchronously. Each entry covers one physically contiguous
/// page fragment; this early implementation does not create IOMMU mappings.
pub unsafe extern "win64" fn build_scatter_gather_list(
 adapter:*mut core::ffi::c_void,device_object:*mut core::ffi::c_void,mdl:*mut MdlAbi,current_va:*mut core::ffi::c_void,length:u32,
 execution_routine:Option<ScatterGatherExecuteRoutine>,context:*mut core::ffi::c_void,write_to_device:u8,
 scatter_gather_buffer:*mut core::ffi::c_void,scatter_gather_buffer_length:u32
)->i32{
 if !is_dma_adapter(adapter)||device_object.is_null()||mdl.is_null()||current_va.is_null()||scatter_gather_buffer.is_null()||execution_routine.is_none(){return 0xC000_000Du32 as i32}
 let m=unsafe{&*mdl};if validate_mdl_range(m,current_va as u64,length).is_none(){return 0xC000_000Du32 as i32}
 let first=(current_va as u64)&(PAGE_SIZE-1);let pages=(first+length as u64+PAGE_SIZE-1)/PAGE_SIZE;
 if pages==0||pages>MAX_SG_SEGMENTS as u64{return 0xC000_009Au32 as i32}
 let needed=core::mem::size_of::<ScatterGatherListAbi>()+pages as usize*core::mem::size_of::<ScatterGatherElement>();
 if needed>scatter_gather_buffer_length as usize{return 0xC000_0023u32 as i32}
 {let _g=SG_LOCK.lock();let owners=unsafe{&*SG_REGISTRY.ownership.get()};if owners.iter().any(|o|o.active&&o.list==scatter_gather_buffer as usize){return 0xC000_000Du32 as i32}}
 let list=scatter_gather_buffer.cast::<ScatterGatherListAbi>();unsafe{core::ptr::addr_of_mut!((*list).number_of_elements).write(pages as u32);core::ptr::addr_of_mut!((*list).reserved).write(0);}
 let domain=match adapter_device(adapter){Some(v)=>v,None=>return 0xC000_000Du32 as i32};
 let token=match allocate_map_register_token(domain as *mut core::ffi::c_void,pages as u32){Some(t)=>t,None=>return 0xC000_009Au32 as i32};
 let mut va=current_va as u64;let mut remaining=length as u64;let mut mapped=0usize;
 for i in 0..pages as usize{
  let chunk=remaining.min(PAGE_SIZE-(va&(PAGE_SIZE-1))) as u32;let physical=unsafe{crate::ntoskrnl::mm::mm_get_physical_address(va as *mut core::ffi::c_void)};
  if physical==0{for j in 0..mapped{let e=unsafe{core::ptr::addr_of!((*list).elements).cast::<ScatterGatherElement>().add(j).read()};let _=iommu::unmap_logical(domain,e.address);}unsafe{free_map_registers(adapter,token,pages as u32)}return 0xC000_000Du32 as i32}
  match iommu::map_fragment(adapter,token,physical,chunk,write_to_device){
   Ok(logical)=>{unsafe{core::ptr::addr_of_mut!((*list).elements).cast::<ScatterGatherElement>().add(i).write(ScatterGatherElement{address:logical,length:chunk,reserved:0});}mapped+=1;}
   Err(_)=>{for j in 0..mapped{let e=unsafe{core::ptr::addr_of!((*list).elements).cast::<ScatterGatherElement>().add(j).read()};let _=iommu::unmap_logical(domain,e.address);}unsafe{free_map_registers(adapter,token,pages as u32)}return 0xC000_009Au32 as i32}
  }
  va+=chunk as u64;remaining-=chunk as u64;
 }
 let owner_index={let _g=SG_LOCK.lock();let owners=unsafe{&mut *SG_REGISTRY.ownership.get()};owners.iter().position(|o|!o.active)};
 let Some(owner_index)=owner_index else{for j in 0..mapped{let e=unsafe{core::ptr::addr_of!((*list).elements).cast::<ScatterGatherElement>().add(j).read()};let _=iommu::unmap_logical(domain,e.address);}unsafe{free_map_registers(adapter,token,pages as u32)}return 0xC000_009Au32 as i32};
 {let _g=SG_LOCK.lock();let owners=unsafe{&mut *SG_REGISTRY.ownership.get()};if owners[owner_index].active{for j in 0..mapped{let e=unsafe{core::ptr::addr_of!((*list).elements).cast::<ScatterGatherElement>().add(j).read()};let _=iommu::unmap_logical(domain,e.address);}unsafe{free_map_registers(adapter,token,pages as u32)}return 0xC000_009Au32 as i32}owners[owner_index]=SgOwnership{active:true,busy:false,device:device_object as usize,list:list as usize,token:token as usize,count:pages as u32};}
 fence(Ordering::SeqCst);unsafe{execution_routine.unwrap()(device_object,core::ptr::null_mut(),list,context)};0
}



/// Bounded legacy map-register tokens. Direct-DMA mode uses tokens to enforce
/// adapter ownership/lifetime; these tokens do not create IOMMU mappings.
const MAX_MAP_REGISTER_ALLOCS: usize = 64;
const MAX_MAP_REGISTERS_PER_ALLOCATION: u32 = 256;
#[derive(Clone, Copy)]
struct MapRegisterRecord { active: bool, token: usize, count: u32, device: usize, in_use:u32, free_pending:bool }
impl MapRegisterRecord { const EMPTY: Self = Self { active: false, token: 0, count: 0, device: 0, in_use:0, free_pending:false }; }
struct MapRegisterState { records: [MapRegisterRecord; MAX_MAP_REGISTER_ALLOCS], next: usize }
static MAP_REGISTERS: crate::hal::IrqSpinLock<MapRegisterState> =
    crate::hal::IrqSpinLock::new(MapRegisterState { records: [MapRegisterRecord::EMPTY; MAX_MAP_REGISTER_ALLOCS], next: 1 });
#[repr(C, align(16))]
struct MapRegisterToken { id: u64 }
static mut MAP_REGISTER_TOKENS: [MapRegisterToken; MAX_MAP_REGISTER_ALLOCS] =
    [const { MapRegisterToken { id: 0 } }; MAX_MAP_REGISTER_ALLOCS];

/// IO_ALLOCATION_ACTION returned by an adapter execution callback.
type AdapterExecutionRoutine = unsafe extern "win64" fn(
    device_object: *mut core::ffi::c_void,
    irp: *mut core::ffi::c_void,
    map_register_base: *mut core::ffi::c_void,
    context: *mut core::ffi::c_void,
) -> u32;
const IO_DEALLOCATE_OBJECT: u32 = 2;
const IO_DEALLOCATE_OBJECT_KEEP_REGISTERS: u32 = 3;
const STATUS_PENDING: i32 = 0x0000_0103;
const STATUS_INVALID_PARAMETER: i32 = 0xC000_000D_u32 as i32;
const STATUS_INSUFFICIENT_RESOURCES: i32 = 0xC000_009A_u32 as i32;

#[derive(Clone, Copy)]
struct ChannelRequest {
    active: bool,
    device: usize,
    count: u32,
    routine: usize,
    context: usize,
    token: usize,
}
impl ChannelRequest {
    const EMPTY: Self = Self { active: false, device: 0, count: 0, routine: 0, context: 0, token: 0 };
}
struct ChannelState {
    owner: usize,
    owner_generation: u64,
    next_generation: u64,
    callback_active: bool,
    dispatch_pending: bool,
    queue: [ChannelRequest; 32],
    head: usize,
    len: usize,
}
static CHANNEL: crate::hal::IrqSpinLock<ChannelState> = crate::hal::IrqSpinLock::new(
    ChannelState {
        owner: 0, owner_generation: 0, next_generation: 1,
        callback_active: false, dispatch_pending: false,
        queue: [ChannelRequest::EMPTY; 32], head: 0, len: 0,
    });

fn release_token(device: usize, token: usize, count: u32) {
    if token != 0 {
        // Internal cleanup uses the same exact-token/count validation as the
        // exported callback. Device ownership is checked by the registry too.
        let mut state = MAP_REGISTERS.lock();
        if let Some(record) = state.records.iter_mut().find(|r|
            r.active && r.token == token && r.count == count && r.device == device) {
            if record.in_use==0{*record=MapRegisterRecord::EMPTY;}else{record.free_pending=true;}
        }
    }
}

fn take_next_channel_locked(state: &mut ChannelState) -> Option<(ChannelRequest, u64)> {
    while state.len != 0 {
        let req = state.queue[state.head];
        state.queue[state.head] = ChannelRequest::EMPTY;
        state.head = (state.head + 1) % state.queue.len();
        state.len -= 1;
        if !req.active { continue; }
        let generation = state.next_generation;
        state.next_generation = state.next_generation.wrapping_add(1).max(1);
        state.owner = req.device;
        state.owner_generation = generation;
        state.callback_active = true;
        state.dispatch_pending = false;
        return Some((req, generation));
    }
    state.owner = 0;
    state.owner_generation = 0;
    state.dispatch_pending = false;
    None
}

/// Run callbacks outside locks. Generation checks prevent an old/reentrant
/// callback's return value from releasing a newer owner's channel. Queued
/// requests reserve their bounded map-register token before enqueue, so a later
/// dispatch cannot strand the FIFO due to token-allocation failure.
fn dispatch_channel_callbacks(mut current: Option<(ChannelRequest, u64)>) {
    while let Some((req, generation)) = current {
        let routine: AdapterExecutionRoutine = unsafe { core::mem::transmute(req.routine) };
        let token = req.token as *mut core::ffi::c_void;
        let action = unsafe {
            routine(req.device as *mut _, core::ptr::null_mut(), token, req.context as *mut _)
        };
        let auto_release = action == IO_DEALLOCATE_OBJECT || action == IO_DEALLOCATE_OBJECT_KEEP_REGISTERS;
        if action == IO_DEALLOCATE_OBJECT {
            release_token(req.device, req.token, req.count);
        }

        let mut state = CHANNEL.lock();
        state.callback_active = false;
        let still_owner = state.owner != 0 && state.owner_generation == generation;
        if auto_release && still_owner {
            state.owner = 0;
            state.owner_generation = 0;
        }
        // A callback may call FreeAdapterChannel itself. In that case owner
        // has already changed; never apply its stale action to the next owner.
        if state.owner == 0 && (auto_release || state.dispatch_pending) {
            current = take_next_channel_locked(&mut state);
        } else {
            current = None;
        }
    }
}

/// Queue requests fairly and reserve all bookkeeping resources before returning
/// STATUS_PENDING. This is bookkeeping only, not hardware IOMMU/map-register
/// programming; the adapter remains unavailable when remapping is required.
pub unsafe extern "win64" fn allocate_adapter_channel(
    adapter: *mut core::ffi::c_void,
    device: *mut core::ffi::c_void,
    number_of_map_registers: u32,
    routine: Option<AdapterExecutionRoutine>,
    context: *mut core::ffi::c_void,
) -> i32 {
    if !is_dma_adapter(adapter)
        || adapter_device(adapter)!=Some(device as usize)
        || device.is_null()
        || number_of_map_registers > MAX_MAP_REGISTERS_PER_ALLOCATION
        || routine.is_none() {
        return STATUS_INVALID_PARAMETER;
    }

    let token = if number_of_map_registers == 0 { 0 } else {
        match allocate_map_register_token(device, number_of_map_registers) {
            Some(p) => p as usize,
            None => return STATUS_INSUFFICIENT_RESOURCES,
        }
    };
    let req = ChannelRequest {
        active: true, device: device as usize, count: number_of_map_registers,
        routine: routine.unwrap() as usize, context: context as usize, token,
    };

    let immediate = {
        let mut state = CHANNEL.lock();
        // WDM permits only one pending legacy AllocateAdapterChannel request
        // per DEVICE_OBJECT. Reject duplicates instead of creating ambiguous
        // cancellation/ownership semantics.
        if state.owner == req.device
            || state.queue.iter().any(|queued| queued.active && queued.device == req.device) {
            drop(state);
            release_token(req.device, req.token, req.count);
            return STATUS_INVALID_PARAMETER;
        }
        if state.owner == 0 && !state.callback_active {
            let generation = state.next_generation;
            state.next_generation = state.next_generation.wrapping_add(1).max(1);
            state.owner = req.device;
            state.owner_generation = generation;
            state.callback_active = true;
            state.dispatch_pending = false;
            Some((req, generation))
        } else if state.len == state.queue.len() {
            drop(state);
            release_token(req.device, req.token, req.count);
            return STATUS_INSUFFICIENT_RESOURCES;
        } else {
            let tail = (state.head + state.len) % state.queue.len();
            state.queue[tail] = req;
            state.len += 1;
            return STATUS_PENDING;
        }
    };
    dispatch_channel_callbacks(immediate);
    0
}

pub(crate) fn consume_map_register(token:*mut core::ffi::c_void,device:usize)->bool{
 if token.is_null(){return false}let mut s=MAP_REGISTERS.lock();
 if let Some(r)=s.records.iter_mut().find(|r|r.active&&r.token==token as usize&&r.device==device&&r.in_use<r.count){r.in_use+=1;true}else{false}
}
pub(crate) fn return_map_register(token:*mut core::ffi::c_void,device:usize){
 if token.is_null(){return}let mut s=MAP_REGISTERS.lock();
 if let Some(r)=s.records.iter_mut().find(|r|r.active&&r.token==token as usize&&r.device==device&&r.in_use>0){
  r.in_use-=1;if r.in_use==0&&r.free_pending{*r=MapRegisterRecord::EMPTY;}
 }
}
fn allocate_map_register_token(device: *mut core::ffi::c_void, count: u32) -> Option<*mut core::ffi::c_void> {
    if count == 0 || count > MAX_MAP_REGISTERS_PER_ALLOCATION { return None; }
    let mut state = MAP_REGISTERS.lock();
    let idx = state.records.iter().position(|r| !r.active)?;
    let id = state.next; state.next = state.next.wrapping_add(1).max(1);
    let token = unsafe { core::ptr::addr_of_mut!(MAP_REGISTER_TOKENS[idx]) };
    unsafe { (*token).id = id as u64; }
    let ptr = token.cast::<core::ffi::c_void>();
    state.records[idx] = MapRegisterRecord { active: true, token: ptr as usize, count, device: device as usize, in_use:0, free_pending:false };
    Some(ptr)
}

pub unsafe extern "win64" fn free_map_registers(
    adapter: *mut core::ffi::c_void,
    map_register_base: *mut core::ffi::c_void,
    number_of_map_registers: u32,
) {
    if !is_dma_adapter(adapter) || map_register_base.is_null() { return; }
    let mut state = MAP_REGISTERS.lock();
    if let Some(r) = state.records.iter_mut().find(|r|
        r.active && r.token == map_register_base as usize && r.count == number_of_map_registers
        && adapter_device(adapter).is_none_or(|d|d==r.device)) {
        if r.in_use==0{*r=MapRegisterRecord::EMPTY;}else{r.free_pending=true;}
    }
}

/// Release the current channel. If called reentrantly from a driver callback,
/// defer FIFO dispatch until that callback returns, avoiding nested callbacks
/// and stale outer IO_ALLOCATION_ACTION values releasing the next owner.
pub unsafe extern "win64" fn free_adapter_channel(adapter: *mut core::ffi::c_void) {
    if !is_dma_adapter(adapter) { return; }
    let Some(domain)=adapter_device(adapter) else { return; };
    let next = {
        let mut state = CHANNEL.lock();
        if state.owner == 0 || state.owner!=domain { return; }
        state.owner = 0;
        state.owner_generation = 0;
        if state.callback_active {
            state.dispatch_pending = true;
            return;
        }
        take_next_channel_locked(&mut state)
    };
    dispatch_channel_callbacks(next);
}

/// Extended map-transfer ABI is not advertised in the operation table until its
/// complete WDK context and scatter/gather output semantics are implemented.
pub unsafe extern "win64" fn map_transfer_ex(
    adapter: *mut core::ffi::c_void, _context: *mut core::ffi::c_void,
    mdl: *mut MdlAbi, map_register_base: *mut core::ffi::c_void,
    current_va: *mut core::ffi::c_void, length: *mut u32, direction: u32,
    _scatter_gather: *mut core::ffi::c_void,
) -> i32 {
    if direction > 2 || map_register_base.is_null() { return 0xC000000D_u32 as i32; }
    let address = unsafe { map_transfer(adapter, mdl, map_register_base, current_va, length, (direction == 1) as u8) };
    if address == 0 { 0xC000000D_u32 as i32 } else { 0 }
}

/// Windows x64 DMA operation table. Hardware-facing callbacks require a registered
/// IOMMU backend and return failure rather than exposing physical addresses.
pub static DMA_OPERATIONS: DmaOperations = DmaOperations {
    size: DmaOperations::ABI_SIZE as u32,
    put_dma_adapter: crate::dma::put_dma_adapter as *const (),
    allocate_common_buffer: crate::hal::exports::hal_allocate_common_buffer as *const (),
    free_common_buffer: crate::hal::exports::hal_free_common_buffer as *const (),
    allocate_adapter_channel: crate::dma::allocate_adapter_channel as *const (),
    flush_adapter_buffers: crate::dma::flush_adapter_buffers as *const (),
    free_adapter_channel: crate::dma::free_adapter_channel as *const (),
    free_map_registers: crate::dma::free_map_registers as *const (),
    map_transfer: crate::dma::map_transfer as *const (),
    get_dma_alignment: crate::dma::get_dma_alignment as *const (),
    read_dma_counter: core::ptr::null(),
    get_scatter_gather_list: crate::dma::get_scatter_gather_list as *const (),
    put_scatter_gather_list: crate::dma::put_scatter_gather_list as *const (),
    calculate_scatter_gather_list_size: crate::dma::calculate_scatter_gather_list_size as *const (),
    build_scatter_gather_list: crate::dma::build_scatter_gather_list as *const (),
    build_mdl_from_scatter_gather_list: core::ptr::null(),
    get_dma_adapter_info: core::ptr::null(),
    get_dma_transfer_info: core::ptr::null(),
    initialize_dma_transfer_context: core::ptr::null(),
    allocate_common_buffer_ex: crate::dma::allocate_common_buffer_ex as *const (),
    allocate_adapter_channel_ex: core::ptr::null(),
    configure_adapter_channel: core::ptr::null(),
    cancel_adapter_channel: core::ptr::null(),
    map_transfer_ex: core::ptr::null(),
    get_scatter_gather_list_ex: core::ptr::null(),
    build_scatter_gather_list_ex: core::ptr::null(),
    flush_adapter_buffers_ex: core::ptr::null(),
    free_adapter_object: core::ptr::null(),
    cancel_mapped_transfer: core::ptr::null(),
    allocate_domain_common_buffer: core::ptr::null(),
    flush_dma_buffer: core::ptr::null(),
    join_dma_domain: core::ptr::null(),
    leave_dma_domain: core::ptr::null(),
    get_dma_domain: core::ptr::null(),
    allocate_common_buffer_with_bounds: core::ptr::null(),
    allocate_common_buffer_vector: core::ptr::null(),
    get_common_buffer_from_vector_by_index: core::ptr::null(),
    free_common_buffer_from_vector: core::ptr::null(),
    free_common_buffer_vector: core::ptr::null(),
    create_common_buffer_from_mdl: core::ptr::null(),
    register_fault_callback: core::ptr::null(),
    unregister_fault_callback: core::ptr::null(),
};

#[repr(C)]
pub struct DmaOperationsV1 {
    pub size:u32,
    pub put_dma_adapter:*const (), pub allocate_common_buffer:*const (), pub free_common_buffer:*const (),
    pub allocate_adapter_channel:*const (), pub flush_adapter_buffers:*const (), pub free_adapter_channel:*const (),
    pub free_map_registers:*const (), pub map_transfer:*const (), pub get_dma_alignment:*const (),
    pub read_dma_counter:*const (), pub get_scatter_gather_list:*const (), pub put_scatter_gather_list:*const (),
}
unsafe impl Sync for DmaOperationsV1 {}
#[repr(C)]
pub struct DmaOperationsV2 {
    pub size:u32,
    pub put_dma_adapter:*const (), pub allocate_common_buffer:*const (), pub free_common_buffer:*const (),
    pub allocate_adapter_channel:*const (), pub flush_adapter_buffers:*const (), pub free_adapter_channel:*const (),
    pub free_map_registers:*const (), pub map_transfer:*const (), pub get_dma_alignment:*const (),
    pub read_dma_counter:*const (), pub get_scatter_gather_list:*const (), pub put_scatter_gather_list:*const (),
    pub calculate_scatter_gather_list_size:*const (), pub build_scatter_gather_list:*const (),
    pub build_mdl_from_scatter_gather_list:*const (),
}
unsafe impl Sync for DmaOperationsV2 {}
pub static DMA_OPERATIONS_V1:DmaOperationsV1=DmaOperationsV1 {
 size:104, put_dma_adapter:put_dma_adapter as *const (),
 allocate_common_buffer:crate::hal::exports::hal_allocate_common_buffer as *const (),
 free_common_buffer:crate::hal::exports::hal_free_common_buffer as *const (),
 allocate_adapter_channel:allocate_adapter_channel as *const (),flush_adapter_buffers:flush_adapter_buffers as *const (),
 free_adapter_channel:free_adapter_channel as *const (),free_map_registers:free_map_registers as *const (),
 map_transfer:map_transfer as *const (),get_dma_alignment:get_dma_alignment as *const (),read_dma_counter:read_dma_counter as *const (),
 get_scatter_gather_list:get_scatter_gather_list as *const (),put_scatter_gather_list:put_scatter_gather_list as *const (),
};
pub static DMA_OPERATIONS_V2:DmaOperationsV2=DmaOperationsV2 {
 size:128, put_dma_adapter:put_dma_adapter as *const (),
 allocate_common_buffer:crate::hal::exports::hal_allocate_common_buffer as *const (),
 free_common_buffer:crate::hal::exports::hal_free_common_buffer as *const (),
 allocate_adapter_channel:allocate_adapter_channel as *const (),flush_adapter_buffers:flush_adapter_buffers as *const (),
 free_adapter_channel:free_adapter_channel as *const (),free_map_registers:free_map_registers as *const (),
 map_transfer:map_transfer as *const (),get_dma_alignment:get_dma_alignment as *const (),read_dma_counter:read_dma_counter as *const (),
 get_scatter_gather_list:get_scatter_gather_list as *const (),put_scatter_gather_list:put_scatter_gather_list as *const (),
 calculate_scatter_gather_list_size:calculate_scatter_gather_list_size as *const (),
 build_scatter_gather_list:build_scatter_gather_list as *const (),build_mdl_from_scatter_gather_list:core::ptr::null(),
};
fn is_dma_adapter(adapter:*mut core::ffi::c_void)->bool {
 !adapter.is_null() && (adapter_device(adapter).is_some()
 || adapter==core::ptr::addr_of!(DMA_ADAPTER_V1).cast_mut().cast()
 || adapter==core::ptr::addr_of!(DMA_ADAPTER_V2).cast_mut().cast()
 || adapter==core::ptr::addr_of!(DMA_ADAPTER_V3).cast_mut().cast())
}

/// Shared stateless adapter object; DMA_ADAPTER contains Version, Size, and Operations on x64.
pub static DMA_ADAPTER_V3: DmaAdapterAbi = DmaAdapterAbi {
    version: 1,
    size: core::mem::size_of::<DmaAdapterAbi>() as u16,
    operations: &DMA_OPERATIONS as *const DmaOperations,
};
pub static DMA_ADAPTER_V1: DmaAdapterAbi = DmaAdapterAbi { version:1, size:core::mem::size_of::<DmaAdapterAbi>() as u16, operations: core::ptr::addr_of!(DMA_OPERATIONS_V1).cast() };
pub static DMA_ADAPTER_V2: DmaAdapterAbi = DmaAdapterAbi { version:1, size:core::mem::size_of::<DmaAdapterAbi>() as u16, operations: core::ptr::addr_of!(DMA_OPERATIONS_V2).cast() };
pub use DMA_ADAPTER_V3 as DMA_ADAPTER;
unsafe impl Sync for DmaAdapterAbi {}

pub extern "win64" fn put_dma_adapter(adapter: *mut core::ffi::c_void) { release_adapter(adapter); }
pub extern "win64" fn get_dma_alignment(_adapter: *mut core::ffi::c_void) -> u32 { 1 }
pub extern "win64" fn read_dma_counter(_adapter: *mut core::ffi::c_void) -> u32 { 0 }


#[cfg(test)]
mod dma_lifecycle_tests {
    use super::*;

    #[test]
    fn map_register_tokens_require_exact_owner_token_and_count() {
        let device = 0x1234usize as *mut core::ffi::c_void;
        let token = allocate_map_register_token(device, 3).expect("token");
        {
            let state = MAP_REGISTERS.lock();
            let rec = state.records.iter().find(|r| r.active && r.token == token as usize).unwrap();
            assert_eq!(rec.device, device as usize);
            assert_eq!(rec.count, 3);
        }
        unsafe { free_map_registers(core::ptr::addr_of!(DMA_ADAPTER).cast_mut().cast(), token, 2); }
        assert!(MAP_REGISTERS.lock().records.iter().any(|r| r.active && r.token == token as usize));
        unsafe { free_map_registers(core::ptr::addr_of!(DMA_ADAPTER).cast_mut().cast(), token, 3); }
        assert!(!MAP_REGISTERS.lock().records.iter().any(|r| r.active && r.token == token as usize));
    }

    #[test]
    fn map_register_release_waits_for_live_mapping_references() {
        let device = 0x2345usize as *mut core::ffi::c_void;
        let adapter = core::ptr::addr_of!(DMA_ADAPTER).cast_mut().cast();
        let token = allocate_map_register_token(device, 2).expect("map-register token");
        assert!(consume_map_register(token, device as usize));
        unsafe { free_map_registers(adapter, token, 2); }
        {
            let state = MAP_REGISTERS.lock();
            let record = state.records.iter().find(|r| r.active && r.token == token as usize).unwrap();
            assert_eq!(record.in_use, 1);
            assert!(record.free_pending);
        }
        return_map_register(token, device as usize);
        assert!(!MAP_REGISTERS.lock().records.iter().any(|r| r.active && r.token == token as usize));
    }

    #[test]
    fn channel_queue_is_fifo_and_bounded() {
        let mut state = ChannelState { owner: 1, owner_generation: 1, next_generation: 2, callback_active: false, dispatch_pending: false, queue: [ChannelRequest::EMPTY; 32], head: 0, len: 0 };
        for n in 0..32usize {
            let tail = (state.head + state.len) % state.queue.len();
            state.queue[tail] = ChannelRequest { active: true, device: n + 2, count: 0, routine: 1, context: 0, token: 0 };
            state.len += 1;
        }
        assert_eq!(state.len, state.queue.len());
        assert_eq!((state.head + state.len) % state.queue.len(), state.head);
        let first = state.queue[state.head];
        assert_eq!(first.device, 2);
        state.queue[state.head] = ChannelRequest::EMPTY;
        state.head = (state.head + 1) % state.queue.len();
        state.len -= 1;
        let tail = (state.head + state.len) % state.queue.len();
        state.queue[tail] = ChannelRequest { active: true, device: 99, count: 0, routine: 1, context: 0, token: 0 };
        state.len += 1;
        assert_eq!(state.queue[state.head].device, 3);
        assert_eq!(state.queue[(state.head + state.len - 1) % state.queue.len()].device, 99);
    }

    #[test]
    fn dequeue_assigns_a_fresh_generation_and_marks_callback_active() {
        let mut state = ChannelState {
            owner: 0, owner_generation: 0, next_generation: 41,
            callback_active: false, dispatch_pending: false,
            queue: [ChannelRequest::EMPTY; 32], head: 7, len: 1,
        };
        state.queue[7] = ChannelRequest {
            active: true, device: 0x77, count: 0, routine: 1, context: 0, token: 0,
        };
        let (req, generation) = take_next_channel_locked(&mut state).expect("queued request");
        assert_eq!(req.device, 0x77);
        assert_eq!(generation, 41);
        assert_eq!(state.owner, 0x77);
        assert_eq!(state.owner_generation, 41);
        assert!(state.callback_active);
        assert_eq!(state.len, 0);
        assert_eq!(state.next_generation, 42);
    }

    #[test]
    fn sg_release_rejects_unknown_and_double_release() {
        let bogus = 0x1234usize as *mut ScatterGatherListAbi;
        unsafe { put_scatter_gather_list(core::ptr::addr_of!(DMA_ADAPTER).cast_mut().cast(), bogus, 0); }
        {
            let _guard = SG_LOCK.lock();
            assert!(!unsafe { &*SG_REGISTRY.ownership.get() }.iter().any(|e| e.active && e.list == bogus as usize));
        }

        // Exercise both release of a live registry entry and idempotent
        // rejection of the same pointer after ownership has been relinquished.
        let list = unsafe { &mut *SG_REGISTRY.lists.get() };
        let ptr = core::ptr::addr_of_mut!(list[0]).cast::<ScatterGatherListAbi>();
        {
            let _guard = SG_LOCK.lock();
            let owners = unsafe { &mut *SG_REGISTRY.ownership.get() };
            owners[0] = SgOwnership { active: true, busy:false, device: 0x55, list: ptr as usize, token:0, count:0 };
        }
        let adapter = core::ptr::addr_of!(DMA_ADAPTER).cast_mut().cast();
        unsafe { put_scatter_gather_list(adapter, ptr, 0); }
        unsafe { put_scatter_gather_list(adapter, ptr, 0); }
        let _guard = SG_LOCK.lock();
        assert!(!unsafe { &*SG_REGISTRY.ownership.get() }.iter().any(|e| e.active && e.list == ptr as usize));
    }

    #[test]
    fn concurrent_map_register_token_stress() {
        // This is a genuine host-side multithreaded test of the shared registry
        // lock and token reuse. It does not claim to emulate interrupt context
        // or a multi-CPU kernel scheduler.
        const THREADS: usize = 8;
        const ITERATIONS: usize = 1000;
        let start = std::sync::Arc::new(std::sync::Barrier::new(THREADS));
        let adapter = core::ptr::addr_of!(DMA_ADAPTER).cast_mut().cast::<core::ffi::c_void>() as usize;
        let mut workers = std::vec::Vec::new();
        for worker in 0..THREADS {
            let gate = start.clone();
            workers.push(std::thread::spawn(move || {
                let device = (0x1000usize + worker) as *mut core::ffi::c_void;
                gate.wait();
                for _ in 0..ITERATIONS {
                    let token = loop {
                        if let Some(token) = allocate_map_register_token(device, 1) { break token; }
                        std::thread::yield_now();
                    };
                    unsafe { free_map_registers(adapter as *mut core::ffi::c_void, token, 1); }
                }
            }));
        }
        for worker in workers { worker.join().expect("stress worker panicked"); }
        let state = MAP_REGISTERS.lock();
        assert!(state.records.iter().all(|r| !r.active));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dma_adapter_operation_tables_match_requested_wdk_version() {
        assert_eq!(core::mem::size_of::<DmaOperationsV1>(),104);
        assert_eq!(core::mem::size_of::<DmaOperationsV2>(),128);
        assert_eq!(DMA_OPERATIONS_V1.size,104);
        assert_eq!(DMA_OPERATIONS_V2.size,128);
        assert_eq!(DMA_OPERATIONS.size as usize,DmaOperations::ABI_SIZE);
    }

    #[test]
    fn device_description_uses_ulong_version_and_validates_master_flags() {
        let mut desc=[0u8;64];
        desc[0..4].copy_from_slice(&3u32.to_le_bytes());
        desc[4]=1; // Master
        desc[5]=1; // ScatterGather
        desc[11]=1; // Dma64BitAddresses
        assert_eq!(parse_device_description_prefix(&desc),Some(3));
        desc[4]=0;
        assert_eq!(parse_device_description_prefix(&desc),None);
        desc[4]=1;desc[10]=1; // Reserved1 must remain zero.
        assert_eq!(parse_device_description_prefix(&desc),None);
        desc[10]=0;desc[0..4].copy_from_slice(&4u32.to_le_bytes());
        assert_eq!(parse_device_description_prefix(&desc),None);
        assert_eq!(parse_device_description_prefix(&desc[..8]),None);
    }

    #[test]
    fn windows_dma_operations_layout_matches_x64_slots() {
        assert_eq!(core::mem::offset_of!(DmaOperations, put_dma_adapter), 8);
        assert_eq!(DmaOperations::POINTER_COUNT, 41);
        assert_eq!(DmaOperations::ABI_SIZE, 336);
        assert_eq!(core::mem::size_of::<DmaAdapterAbi>(), 16);
        assert_eq!(core::mem::size_of::<MdlAbi>(), 48);
        assert_eq!(core::mem::offset_of!(MdlAbi, byte_count), 40);
    }
    #[test]
    fn calculate_sg_size_accounts_for_unaligned_page_span() {
        let mut mdl = MdlAbi {
            next: core::ptr::null_mut(), size: 48, flags: MDL_PAGES_LOCKED,
            process: core::ptr::null_mut(), mapped_system_va: core::ptr::null_mut(),
            start_va: 0x1000usize as *mut core::ffi::c_void,
            byte_count: 0x2000, byte_offset: 0,
        };
        let mut size = 0u32;
        let status = unsafe {
            calculate_scatter_gather_list_size(
                core::ptr::addr_of!(DMA_ADAPTER).cast_mut().cast(),
                &mut mdl, 0x1003usize as *mut core::ffi::c_void, 4096, &mut size, 0,
            )
        };
        assert_eq!(status, 0);
        assert_eq!(size as usize, core::mem::size_of::<ScatterGatherListAbi>()
            + 2 * core::mem::size_of::<ScatterGatherElement>());
    }

    #[test]
    fn dma_rejects_unlocked_mdl_before_exposing_physical_addresses() {
        let mut mdl = MdlAbi {
            next: core::ptr::null_mut(), size: 48, flags: 0,
            process: core::ptr::null_mut(), mapped_system_va: core::ptr::null_mut(),
            start_va: 0x1000usize as *mut core::ffi::c_void,
            byte_count: 0x2000, byte_offset: 0,
        };
        assert!(validate_mdl_range(&mdl, 0x1000, 512).is_none());
        mdl.flags = MDL_SOURCE_IS_NONPAGED_POOL;
        assert!(validate_mdl_range(&mdl, 0x1000, 512).is_some());
        assert!(validate_mdl_range(&mdl, 0x2ff0, 32).is_none());
    }

    #[test]
    fn scatter_gather_splits_at_boundaries_and_limits() {
        let fragments = [(0x1ff0, 0x40)];
        let mut out = [ScatterGatherElement::default(); 8];
        let n = build_scatter_gather(&fragments, 0x30, 0x1000, &mut out).unwrap();
        assert_eq!(n, 2);
        assert_eq!((out[0].address, out[0].length), (0x1ff0, 0x10));
        assert_eq!((out[1].address, out[1].length), (0x2000, 0x30));
    }
}

/// Release every IOMMU mapping explicitly associated with this IRP.
pub(crate) fn release_irp_mappings(irp:*mut core::ffi::c_void){iommu::release_irp_mappings(irp)}
