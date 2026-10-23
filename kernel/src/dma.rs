//! Early x64 DMA primitives and Windows DMA ABI layout.
//!
//! This is not yet a published Windows DMA adapter: the WDK callback signatures
//! and MDL integration must be wired before exposing a DMA_OPERATIONS pointer to
//! a third-party driver. Helpers here are usable internally and fail closed when
//! the allocator cannot satisfy a physical-contiguity or address-mask constraint.
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

/// The legacy x64 DMA adapter currently provides real common-buffer allocation
/// only. Mapping, channel ownership and SG callbacks stay null until MDL and
/// device-object lifetimes are implemented; advertising fake callbacks is unsafe.
pub static DMA_OPERATIONS: DmaOperations = DmaOperations {
    size: DmaOperations::ABI_SIZE as u32,
    put_dma_adapter: crate::dma::put_dma_adapter as usize,
    allocate_common_buffer: crate::hal::exports::hal_allocate_common_buffer as usize,
    free_common_buffer: crate::hal::exports::hal_free_common_buffer as usize,
    allocate_adapter_channel: 0,
    flush_adapter_buffers: crate::dma::flush_adapter_buffers as usize,
    free_adapter_channel: 0,
    free_map_registers: 0,
    map_transfer: crate::dma::map_transfer as usize,
    get_dma_alignment: crate::dma::get_dma_alignment as usize,
    read_dma_counter: 0,
    get_scatter_gather_list: 0,
    put_scatter_gather_list: 0,
    calculate_scatter_gather_list_size: 0,
    build_scatter_gather_list: 0,
    build_mdl_from_scatter_gather_list: 0,
    get_dma_adapter_info: 0,
    get_dma_transfer_info: 0,
    initialize_dma_transfer_context: 0,
    allocate_common_buffer_ex: 0,
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

/// Shared stateless adapter object; DMA_ADAPTER is a leading Version/Size/
—Operations structure on x64.
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
mod tests {
    use super::*;
    #[test]
    fn windows_dma_operations_layout_matches_x64_slots() {
        assert_eq!(core::mem::offset_of!(DmaOperations, put_dma_adapter), 8);
        assert_eq!(DmaOperations::POINTER_COUNT, 41);
        assert_eq!(DmaOperations::ABI_SIZE, 336);
        assert_eq!(core::mem::size_of::<DmaAdapterAbi>(), 16);\n        assert_eq!(core::mem::size_of::<MdlAbi>(), 48);\n        assert_eq!(core::mem::offset_of!(MdlAbi, byte_count), 40);
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
