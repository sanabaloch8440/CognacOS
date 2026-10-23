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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_dma_operations_layout_matches_x64_slots() {
        assert_eq!(core::mem::offset_of!(DmaOperations, put_dma_adapter), 8);
        assert_eq!(DmaOperations::POINTER_COUNT, 41);
        assert_eq!(DmaOperations::ABI_SIZE, 336);
        assert_eq!(core::mem::size_of::<DmaAdapterAbi>(), 16);
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
