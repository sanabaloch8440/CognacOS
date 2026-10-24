//! Early x64 DMA primitives and Windows DMA ABI layout.
//!
//! Windows x64 bootstrap DMA adapter and DMA_OPERATIONS callbacks.
//! Direct physical mapping and coherent common buffers are supported below 4 GiB;
//! IOMMU domains, map-register scheduling, and MDL-backed scatter/gather remain unavailable.
use core::sync::atomic::{fence, Ordering};

pub const PAGE_SIZE: u64 = 4096;
pub const MAX_COMMON_BUFFERS: usize = 128;
pub const MAX_SG_SEGMENTS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DmaError { InvalidLength, OutOfMemory, AddressMask, RegistryFull, NotFound, TooManySegments, InvalidPageList }

/// Windows 11 x64 DMA_OPERATIONS member order. Each member is an eight-byte
/// function-pointer slot on x64; usize preserves exact offsets while avoiding
/// assigning an incompatible callback ABI to unimplemented operations.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DmaOperations {
    pub size: u32,
    pub put_dma_adapter: usize,
    pub allocate_common_buffer: usize,
    pub free_common_buffer: usize,
    pub allocate_adapter_channel: usize,
    pub flush_adapter_buffers: usize,
    pub free_adapter_channel: usize,
    pub free_map_registers: usize,
    pub map_transfer: usize,
    pub get_dma_alignment: usize,
    pub read_dma_counter: usize,
    pub get_scatter_gather_list: usize,
    pub put_scatter_gather_list: usize,
    pub calculate_scatter_gather_list_size: usize,
    pub build_scatter_gather_list: usize,
    pub build_mdl_from_scatter_gather_list: usize,
    pub get_dma_adapter_info: usize,
    pub get_dma_transfer_info: usize,
    pub initialize_dma_transfer_context: usize,
    pub allocate_common_buffer_ex: usize,
    pub allocate_adapter_channel_ex: usize,
    pub configure_adapter_channel: usize,
    pub cancel_adapter_channel: usize,
    pub map_transfer_ex: usize,
    pub get_scatter_gather_list_ex: usize,
    pub build_scatter_gather_list_ex: usize,
    pub flush_adapter_buffers_ex: usize,
    pub free_adapter_object: usize,
    pub cancel_mapped_transfer: usize,
    pub allocate_domain_common_buffer: usize,
    pub flush_dma_buffer: usize,
    pub join_dma_domain: usize,
    pub leave_dma_domain: usize,
    pub get_dma_domain: usize,
    pub allocate_common_buffer_with_bounds: usize,
    pub allocate_common_buffer_vector: usize,
    pub get_common_buffer_from_vector_by_index: usize,
    pub free_common_buffer_from_vector: usize,
    pub free_common_buffer_vector: usize,
    pub create_common_buffer_from_mdl: usize,
    pub register_fault_callback: usize,
    pub unregister_fault_callback: usize,
}
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

#[derive(Clone, Copy)]
struct BufferRecord { physical: u64, requested: usize, pages: usize }
impl BufferRecord { const EMPTY: Self = Self { physical: 0, requested: 0, pages: 0 }; }
struct BufferStore(core::cell::UnsafeCell<[BufferRecord; MAX_COMMON_BUFFERS]>);
unsafe impl Sync for BufferStore {}
static BUFFERS: BufferStore = BufferStore(core::cell::UnsafeCell::new([BufferRecord::EMPTY; MAX_COMMON_BUFFERS]));
static BUFFER_LOCK: crate::hal::IrqSpinLock<()> = crate::hal::IrqSpinLock::new(());

/// Allocate physically contiguous, zeroed memory for a bus-master device.
/// The returned logical address equals the physical address only for this
/// identity-mapped, no-IOMMU bootstrap mode. maximum_address is inclusive.
pub unsafe fn allocate_common_buffer(length: usize, maximum_address: u64) -> Result<(*mut u8, u64), DmaError> {
    if length == 0 { return Err(DmaError::InvalidLength); }
    let pages = length.checked_add(PAGE_SIZE as usize - 1).ok_or(DmaError::InvalidLength)? / PAGE_SIZE as usize;
    let physical = unsafe { crate::memory::allocate_contiguous_pages(pages) }.map_err(|_| DmaError::OutOfMemory)?;
    let bytes = (pages as u64).checked_mul(PAGE_SIZE).ok_or(DmaError::InvalidLength)?;
    let end = physical.checked_add(bytes - 1).ok_or(DmaError::AddressMask)?;
    if end > maximum_address || end >= 0x1_0000_0000 {
        for i in 0..pages { let _ = unsafe { crate::memory::free_page(physical + i as u64 * PAGE_SIZE) }; }
        return Err(DmaError::AddressMask);
    }
    let _guard = BUFFER_LOCK.lock();
    let records = unsafe { &mut *BUFFERS.0.get() };
    let Some(slot) = records.iter_mut().find(|r| r.physical == 0) else {
        drop(_guard);
        for i in 0..pages { let _ = unsafe { crate::memory::free_page(physical + i as u64 * PAGE_SIZE) }; }
        return Err(DmaError::RegistryFull);
    };
    *slot = BufferRecord { physical, requested: length, pages };
    fence(Ordering::SeqCst);
    Ok((physical as *mut u8, physical))
}

/// Flush ordering for a DMA buffer on the current x86-64 coherent-memory path.
/// An MFENCE orders CPU writes; non-coherent/IOMMU platforms require platform
/// cache-maintenance and DMA mapping services that this early HAL does not yet have.
pub unsafe fn flush_common_buffer(cpu_address: *mut u8, length: usize) -> Result<(), DmaError> {
    if cpu_address.is_null() || length == 0 { return Err(DmaError::InvalidLength); }
    fence(Ordering::SeqCst);
    unsafe { core::arch::asm!("mfence", options(nostack, preserves_flags)); }
    Ok(())
}

/// Free only a buffer returned by allocate_common_buffer, requiring exact length
/// and physical/logical address match to prevent freeing arbitrary driver memory.
pub unsafe fn free_common_buffer(cpu_address: *mut u8, logical_address: u64, length: usize) -> Result<(), DmaError> {
    if cpu_address.is_null() || cpu_address as u64 != logical_address || length == 0 {
        return Err(DmaError::NotFound);
    }
    let _guard = BUFFER_LOCK.lock();
    let records = unsafe { &mut *BUFFERS.0.get() };
    let Some(index) = records.iter().position(|r| r.physical == logical_address && r.requested == length) else {
        return Err(DmaError::NotFound);
    };
    let record = records[index];
    records[index] = BufferRecord::EMPTY;
    fence(Ordering::SeqCst);
    drop(_guard);
    for i in 0..record.pages {
        unsafe { crate::memory::free_page(record.physical + i as u64 * PAGE_SIZE) }.map_err(|_| DmaError::OutOfMemory)?;
    }
    Ok(())
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScatterGatherElement { pub address: u64, pub length: u32, pub reserved: u32 }

/// Build a simple bus-master SG list from physical page fragments. This helper
/// merges adjacent fragments while respecting maximum segment size and an
/// optional power-of-two boundary. It does not accept virtual addresses or MDLs.
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





/// IoGetDmaAdapter for the bootstrap direct-DMA adapter. No IOMMU or
/// adapter-channel scheduler is present yet; callers receive zero map registers.
pub unsafe extern "win64" fn io_get_dma_adapter(
    device_object: *mut core::ffi::c_void,
    device_description: *const core::ffi::c_void,
    number_of_map_registers: *mut u32,
) -> *mut core::ffi::c_void {
    if device_object.is_null() || device_description.is_null() || number_of_map_registers.is_null() {
        return core::ptr::null_mut();
    }
    // DEVICE_DESCRIPTION's stable header is Version, Size.
    let header = unsafe { core::slice::from_raw_parts(device_description.cast::<u8>(), 4) };
    let version = u16::from_le_bytes([header[0], header[1]]);
    let size = u16::from_le_bytes([header[2], header[3]]);
    if version > 1 || size < 4 {
        return core::ptr::null_mut();
    }
    // If firmware advertises Intel VT-d, do not silently hand a driver an
    // identity-mapped adapter: remapping is not yet initialized and bypassing
    // firmware isolation can DMA into arbitrary physical memory.
    let rsdp = crate::acpi::rsdp_address();
    if rsdp != 0 && unsafe { crate::acpi::find_table(rsdp, *b"DMAR", 0) }.is_ok() {
        return core::ptr::null_mut();
    }
    unsafe { number_of_map_registers.write(0); }
    core::ptr::addr_of!(DMA_ADAPTER).cast_mut().cast()
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


/// Windows DMA_OPERATIONS.AllocateCommonBufferEx. The current x64 platform is
/// cache coherent; only zero flags are accepted until the kernel implements
/// DMA domains, encryption attributes, and non-coherent cache maintenance.
pub unsafe extern "win64" fn allocate_common_buffer_ex(
    adapter: *mut core::ffi::c_void,
    length: u32,
    logical_address: *mut i64,
    _cache_type: u32,
    flags: u32,
) -> *mut core::ffi::c_void {
    if adapter != core::ptr::addr_of!(DMA_ADAPTER).cast_mut().cast()
        || logical_address.is_null() || flags != 0 || length == 0 {
        return core::ptr::null_mut();
    }
    unsafe {
        crate::hal::exports::hal_allocate_common_buffer(adapter, length, logical_address, 1)
    }
}

/// Windows DMA_OPERATIONS.MapTransfer. This no-IOMMU implementation returns
/// the physical address corresponding to CurrentVa and shortens Length so the
/// transfer never crosses a page boundary. Map-register tokens are not needed
/// for this direct-mapped mode and must not be treated as IOMMU mappings.
pub unsafe extern "win64" fn map_transfer(
    adapter: *mut core::ffi::c_void,
    mdl: *mut MdlAbi,
    _map_register_base: *mut core::ffi::c_void,
    current_va: *mut core::ffi::c_void,
    length: *mut u32,
    _write_to_device: u8,
) -> i64 {
    if adapter != core::ptr::addr_of!(DMA_ADAPTER).cast_mut().cast()
        || mdl.is_null() || current_va.is_null() || length.is_null() {
        return 0;
    }
    let requested = unsafe { length.read() };
    if requested == 0 { return 0; }
    let m = unsafe { &*mdl };
    if m.byte_count == 0 || m.start_va.is_null() { return 0; }
    let start = m.start_va as u64;
    let va = current_va as u64;
    let data_start = match start.checked_add(m.byte_offset as u64) { Some(v) => v, None => return 0 };
    let end = match data_start.checked_add(m.byte_count as u64) { Some(v) => v, None => return 0 };
    if va < data_start || va >= end { return 0; }
    let available = (end - va).min(PAGE_SIZE - (va & (PAGE_SIZE - 1)));
    let actual = (requested as u64).min(available);
    if actual == 0 { return 0; }
    let physical = unsafe { crate::ntoskrnl::mm::runtime::mm_get_physical_address(current_va) };
    if physical == 0 || physical >= 0x1_0000_0000 { return 0; }
    unsafe { length.write(actual as u32); }
    physical as i64
}

/// Windows DMA_OPERATIONS.FlushAdapterBuffers. x86-64 is cache coherent for
/// ordinary PCI bus mastering; this supplies ordering and validates the MDL range.
pub unsafe extern "win64" fn flush_adapter_buffers(
    adapter: *mut core::ffi::c_void,
    mdl: *mut MdlAbi,
    _map_register_base: *mut core::ffi::c_void,
    current_va: *mut core::ffi::c_void,
    length: u32,
    _write_to_device: u8,
) -> u8 {
    if adapter != core::ptr::addr_of!(DMA_ADAPTER).cast_mut().cast()
        || mdl.is_null() || current_va.is_null() || length == 0 {
        return 0;
    }
    let m = unsafe { &*mdl };
    let start = m.start_va as u64;
    let va = current_va as u64;
    let Some(data_start) = start.checked_add(m.byte_offset as u64) else { return 0; };
    let Some(end) = data_start.checked_add(m.byte_count as u64) else { return 0; };
    if m.byte_count == 0 || va < data_start || va.checked_add(length as u64).map_or(true, |e| e > end) {
        return 0;
    }
    fence(Ordering::SeqCst);
    unsafe { core::arch::asm!("mfence", options(nostack, preserves_flags)); }
    1
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
struct SgOwnership { active: bool, device: usize, list: usize }
impl SgOwnership { const EMPTY: Self = Self { active: false, device: 0, list: 0 }; }
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
    adapter: *mut core::ffi::c_void,
    device_object: *mut core::ffi::c_void,
    mdl: *mut MdlAbi,
    current_va: *mut core::ffi::c_void,
    length: u32,
    execution_routine: Option<ScatterGatherExecuteRoutine>,
    context: *mut core::ffi::c_void,
    _write_to_device: u8,
) -> i32 {
    if adapter != core::ptr::addr_of!(DMA_ADAPTER).cast_mut().cast()
        || device_object.is_null() || mdl.is_null() || current_va.is_null()
        || length == 0 || execution_routine.is_none() {
        return 0xC000_000D_u32 as i32;
    }
    let m = unsafe { &*mdl };
    if validate_mdl_range(m, current_va as u64, length).is_none() {
        return 0xC000_000D_u32 as i32;
    }
    let page_count = (((current_va as u64 & (PAGE_SIZE - 1)) + length as u64 + PAGE_SIZE - 1) / PAGE_SIZE) as usize;
    if page_count == 0 || page_count > MAX_SG_SEGMENTS { return 0xC000_009A_u32 as i32; }
    let _guard = SG_LOCK.lock();
    let owners = unsafe { &mut *SG_REGISTRY.ownership.get() };
    let Some(index) = owners.iter().position(|entry| !entry.active) else {
        return 0xC000_009A_u32 as i32;
    };
    let lists = unsafe { &mut *SG_REGISTRY.lists.get() };
    let list = &mut lists[index];
    let mut va = current_va as u64;
    let mut remaining = length as u64;
    for i in 0..page_count {
        let chunk = remaining.min(PAGE_SIZE - (va & (PAGE_SIZE - 1)));
        let physical = unsafe { crate::ntoskrnl::mm::runtime::mm_get_physical_address(va as *mut core::ffi::c_void) };
        if physical == 0 || physical >= 0x1_0000_0000 { return 0xC000_000D_u32 as i32; }
        list.elements[i] = ScatterGatherElement { address: physical, length: chunk as u32, reserved: 0 };
        va += chunk;
        remaining -= chunk;
    }
    list.number_of_elements = page_count as u32;
    list.reserved = 0;
    let list_ptr = (core::ptr::addr_of_mut!(*list)).cast::<ScatterGatherListAbi>();
    owners[index] = SgOwnership { active: true, device: device_object as usize, list: list_ptr as usize };
    drop(_guard);
    fence(Ordering::SeqCst);
    unsafe { execution_routine.unwrap()(device_object, core::ptr::null_mut(), list_ptr, context); }
    0
}

/// Release only an SG list previously issued by GetScatterGatherList.
pub unsafe extern "win64" fn put_scatter_gather_list(
    adapter: *mut core::ffi::c_void,
    scatter_gather: *mut ScatterGatherListAbi,
    _write_to_device: u8,
) {
    if adapter != core::ptr::addr_of!(DMA_ADAPTER).cast_mut().cast() || scatter_gather.is_null() { return; }
    let _guard = SG_LOCK.lock();
    let owners = unsafe { &mut *SG_REGISTRY.ownership.get() };
    if let Some(slot) = owners.iter_mut().find(|e| e.active && e.list == scatter_gather as usize) {
        *slot = SgOwnership::EMPTY;
        fence(Ordering::SeqCst);
    }
}

/// Validate an MDL transfer range using checked arithmetic.
fn validate_mdl_range(mdl: &MdlAbi, current_va: u64, length: u32) -> Option<u64> {
    if mdl.start_va.is_null() || mdl.byte_count == 0 || length == 0 { return None; }
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
    if adapter != core::ptr::addr_of!(DMA_ADAPTER).cast_mut().cast()
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
    adapter: *mut core::ffi::c_void,
    device_object: *mut core::ffi::c_void,
    mdl: *mut MdlAbi,
    current_va: *mut core::ffi::c_void,
    length: u32,
    execution_routine: Option<ScatterGatherExecuteRoutine>,
    context: *mut core::ffi::c_void,
    _write_to_device: u8,
    scatter_gather_buffer: *mut core::ffi::c_void,
    scatter_gather_buffer_length: u32,
) -> i32 {
    if adapter != core::ptr::addr_of!(DMA_ADAPTER).cast_mut().cast()
        || device_object.is_null() || mdl.is_null() || current_va.is_null()
        || scatter_gather_buffer.is_null() || execution_routine.is_none() {
        return 0xC000_000D_u32 as i32;
    }
    let m = unsafe { &*mdl };
    if validate_mdl_range(m, current_va as u64, length).is_none() {
        return 0xC000_000D_u32 as i32;
    }
    let first_offset = (current_va as u64) & (PAGE_SIZE - 1);
    let pages = (first_offset + length as u64 + PAGE_SIZE - 1) / PAGE_SIZE;
    if pages == 0 || pages > MAX_SG_SEGMENTS as u64 { return 0xC000_009A_u32 as i32; }
    let needed = core::mem::size_of::<ScatterGatherListAbi>()
        + pages as usize * core::mem::size_of::<ScatterGatherElement>();
    if needed > scatter_gather_buffer_length as usize { return 0xC000_0023_u32 as i32; }
    let list = scatter_gather_buffer.cast::<ScatterGatherListAbi>();
    unsafe {
        core::ptr::addr_of_mut!((*list).number_of_elements).write(pages as u32);
        core::ptr::addr_of_mut!((*list).reserved).write(0);
    }
    let mut va = current_va as u64;
    let mut remaining = length as u64;
    for index in 0..pages as usize {
        let chunk = remaining.min(PAGE_SIZE - (va & (PAGE_SIZE - 1)));
        let physical = unsafe { crate::ntoskrnl::mm::runtime::mm_get_physical_address(va as *mut core::ffi::c_void) };
        if physical == 0 || physical >= 0x1_0000_0000 {
            return 0xC000_000D_u32 as i32;
        }
        let element = ScatterGatherElement { address: physical, length: chunk as u32, reserved: 0 };
        unsafe { core::ptr::addr_of_mut!((*list).elements).cast::<ScatterGatherElement>().add(index).write(element); }
        va += chunk;
        remaining -= chunk;
    }
    core::sync::atomic::fence(Ordering::SeqCst);
    unsafe { execution_routine.unwrap()(device_object, core::ptr::null_mut(), list, context); }
    0
}


/// Bounded legacy map-register tokens. Direct-DMA mode uses tokens to enforce
/// adapter ownership/lifetime; these tokens do not create IOMMU mappings.
const MAX_MAP_REGISTER_ALLOCS: usize = 64;
const MAX_MAP_REGISTERS_PER_ALLOCATION: u32 = 256;
#[derive(Clone, Copy)]
struct MapRegisterRecord { active: bool, token: usize, count: u32, device: usize }
impl MapRegisterRecord { const EMPTY: Self = Self { active: false, token: 0, count: 0, device: 0 }; }
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
            *record = MapRegisterRecord::EMPTY;
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
    if adapter != core::ptr::addr_of!(DMA_ADAPTER).cast_mut().cast()
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

fn allocate_map_register_token(device: *mut core::ffi::c_void, count: u32) -> Option<*mut core::ffi::c_void> {
    if count == 0 || count > MAX_MAP_REGISTERS_PER_ALLOCATION { return None; }
    let mut state = MAP_REGISTERS.lock();
    let idx = state.records.iter().position(|r| !r.active)?;
    let id = state.next; state.next = state.next.wrapping_add(1).max(1);
    let token = unsafe { core::ptr::addr_of_mut!(MAP_REGISTER_TOKENS[idx]) };
    unsafe { (*token).id = id as u64; }
    let ptr = token.cast::<core::ffi::c_void>();
    state.records[idx] = MapRegisterRecord { active: true, token: ptr as usize, count, device: device as usize };
    Some(ptr)
}

pub unsafe extern "win64" fn free_map_registers(
    adapter: *mut core::ffi::c_void,
    map_register_base: *mut core::ffi::c_void,
    number_of_map_registers: u32,
) {
    if adapter != core::ptr::addr_of!(DMA_ADAPTER).cast_mut().cast() || map_register_base.is_null() { return; }
    let mut state = MAP_REGISTERS.lock();
    if let Some(r) = state.records.iter_mut().find(|r|
        r.active && r.token == map_register_base as usize && r.count == number_of_map_registers) {
        *r = MapRegisterRecord::EMPTY;
    }
}

/// Release the current channel. If called reentrantly from a driver callback,
/// defer FIFO dispatch until that callback returns, avoiding nested callbacks
/// and stale outer IO_ALLOCATION_ACTION values releasing the next owner.
pub unsafe extern "win64" fn free_adapter_channel(adapter: *mut core::ffi::c_void) {
    if adapter != core::ptr::addr_of!(DMA_ADAPTER).cast_mut().cast() { return; }
    let next = {
        let mut state = CHANNEL.lock();
        if state.owner == 0 { return; }
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

/// Extended variants currently accept only flags that preserve the legacy
/// direct-DMA contract; unsupported remapping/flush semantics fail closed.
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

/// The legacy x64 DMA adapter currently provides real common-buffer allocation
/// only. Mapping, channel ownership and SG callbacks stay null until MDL and
/// device-object lifetimes are implemented; advertising fake callbacks is unsafe.
pub static DMA_OPERATIONS: DmaOperations = DmaOperations {
    size: DmaOperations::ABI_SIZE as u32,
    put_dma_adapter: crate::dma::put_dma_adapter as usize,
    allocate_common_buffer: crate::hal::exports::hal_allocate_common_buffer as usize,
    free_common_buffer: crate::hal::exports::hal_free_common_buffer as usize,
    allocate_adapter_channel: crate::dma::allocate_adapter_channel as usize,
    flush_adapter_buffers: crate::dma::flush_adapter_buffers as usize,
    free_adapter_channel: crate::dma::free_adapter_channel as usize,
    free_map_registers: crate::dma::free_map_registers as usize,
    map_transfer: crate::dma::map_transfer as usize,
    get_dma_alignment: crate::dma::get_dma_alignment as usize,
    read_dma_counter: 0,
    get_scatter_gather_list: crate::dma::get_scatter_gather_list as usize,
    put_scatter_gather_list: crate::dma::put_scatter_gather_list as usize,
    calculate_scatter_gather_list_size: crate::dma::calculate_scatter_gather_list_size as usize,
    build_scatter_gather_list: crate::dma::build_scatter_gather_list as usize,
    build_mdl_from_scatter_gather_list: 0,
    get_dma_adapter_info: 0,
    get_dma_transfer_info: 0,
    initialize_dma_transfer_context: 0,
    allocate_common_buffer_ex: crate::dma::allocate_common_buffer_ex as usize,
    allocate_adapter_channel_ex: 0,
    configure_adapter_channel: 0,
    cancel_adapter_channel: 0,
    map_transfer_ex: 0,
    get_scatter_gather_list_ex: 0,
    build_scatter_gather_list_ex: 0,
    flush_adapter_buffers_ex: 0,
    free_adapter_object: 0,
    cancel_mapped_transfer: 0,
    allocate_domain_common_buffer: 0,
    flush_dma_buffer: 0,
    join_dma_domain: 0,
    leave_dma_domain: 0,
    get_dma_domain: 0,
    allocate_common_buffer_with_bounds: 0,
    allocate_common_buffer_vector: 0,
    get_common_buffer_from_vector_by_index: 0,
    free_common_buffer_from_vector: 0,
    free_common_buffer_vector: 0,
    create_common_buffer_from_mdl: 0,
    register_fault_callback: 0,
    unregister_fault_callback: 0,
};

/// Shared stateless adapter object; DMA_ADAPTER contains Version, Size, and Operations on x64.
pub static DMA_ADAPTER: DmaAdapterAbi = DmaAdapterAbi {
    version: 1,
    size: core::mem::size_of::<DmaAdapterAbi>() as u16,
    operations: &DMA_OPERATIONS as *const DmaOperations,
};
unsafe impl Sync for DmaAdapterAbi {}

pub extern "win64" fn put_dma_adapter(_adapter: *mut core::ffi::c_void) {}
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
            owners[0] = SgOwnership { active: true, device: 0x55, list: ptr as usize };
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
            next: core::ptr::null_mut(), size: 48, flags: 0,
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
    fn scatter_gather_splits_at_boundaries_and_limits() {
        let fragments = [(0x1ff0, 0x40)];
        let mut out = [ScatterGatherElement::default(); 8];
        let n = build_scatter_gather(&fragments, 0x30, 0x1000, &mut out).unwrap();
        assert_eq!(n, 2);
        assert_eq!((out[0].address, out[0].length), (0x1ff0, 0x10));
        assert_eq!((out[1].address, out[1].length), (0x2000, 0x30));
    }
}
