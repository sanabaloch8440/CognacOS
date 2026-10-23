//! ABI-correct, bounded core NT memory-manager routines.
//!
//! These helpers deliberately validate page-table translations before touching
//! memory. They are not a substitute for pageable process address spaces or a
//! complete Windows Memory Manager.
use core::ffi::c_void;

const PAGE: u64 = 4096;
const PRESENT: u64 = 1;
const WRITE: u64 = 1 << 1;
const LARGE: u64 = 1 << 7;
const ADDRESS_MASK: u64 = 0x000f_ffff_ffff_f000;
const LARGE_2M_MASK: u64 = 0x000f_ffff_ffe0_0000;
const LARGE_1G_MASK: u64 = 0x000f_ffff_c000_0000;
const STATUS_SUCCESS: i32 = 0;
const STATUS_ACCESS_VIOLATION: i32 = 0xC000_0005u32 as i32;
const STATUS_INVALID_PARAMETER: i32 = 0xC000_000Du32 as i32;
const STATUS_PARTIAL_COPY: i32 = 0x8000_000Du32 as i32;

/// Translate a canonical virtual address through the active x86-64 page tables.
/// The bootstrap maps the physical pages used by page tables into the low 4 GiB.
fn translate(va: u64, require_write: bool) -> Option<u64> {
    // Accept only canonical 48-bit virtual addresses.
    let high = va >> 48;
    let sign = (va >> 47) & 1;
    if (sign == 0 && high != 0) || (sign == 1 && high != 0xffff) { return None; }
    let cr3: u64;
    unsafe { core::arch::asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack, preserves_flags)); }
    let root = (cr3 & ADDRESS_MASK) as *const u64;
    if root.is_null() { return None; }
    let i4 = ((va >> 39) & 0x1ff) as usize;
    let i3 = ((va >> 30) & 0x1ff) as usize;
    let i2 = ((va >> 21) & 0x1ff) as usize;
    let i1 = ((va >> 12) & 0x1ff) as usize;
    let e4 = unsafe { root.add(i4).read_volatile() };
    if e4 & PRESENT == 0 || (require_write && e4 & WRITE == 0) { return None; }
    let pdpt = (e4 & ADDRESS_MASK) as *const u64;
    let e3 = unsafe { pdpt.add(i3).read_volatile() };
    if e3 & PRESENT == 0 || (require_write && e3 & WRITE == 0) { return None; }
    if e3 & LARGE != 0 {
        return Some((e3 & LARGE_1G_MASK) | (va & ((1u64 << 30) - 1)));
    }
    let pd = (e3 & ADDRESS_MASK) as *const u64;
    let e2 = unsafe { pd.add(i2).read_volatile() };
    if e2 & PRESENT == 0 || (require_write && e2 & WRITE == 0) { return None; }
    if e2 & LARGE != 0 {
        return Some((e2 & LARGE_2M_MASK) | (va & ((1u64 << 21) - 1)));
    }
    let pt = (e2 & ADDRESS_MASK) as *const u64;
    let e1 = unsafe { pt.add(i1).read_volatile() };
    if e1 & PRESENT == 0 || (require_write && e1 & WRITE == 0) { return None; }
    Some((e1 & ADDRESS_MASK) | (va & (PAGE - 1)))
}

#[inline]
pub unsafe extern "win64" fn mm_get_physical_address(address: *const c_void) -> u64 {
    translate(address as u64, false).unwrap_or(0)
}

#[inline]
pub unsafe extern "win64" fn mm_get_virtual_for_physical(address: u64) -> *mut c_void {
    // The current kernel bootstrap provides a low-4-GiB identity map only.
    if address >= 0x1_0000_0000 { return core::ptr::null_mut(); }
    if translate(address, false).is_some() { address as *mut c_void } else { core::ptr::null_mut() }
}

#[inline]
pub unsafe extern "win64" fn mm_is_address_valid(address: *const c_void) -> u8 {
    (translate(address as u64, false).is_some()) as u8
}

#[inline]
pub unsafe extern "win64" fn mm_is_non_paged_system_address_valid(address: *const c_void) -> u8 {
    // Until paging can evict and restore arbitrary system mappings, a present
    // translation is the strongest honest nonpaged-address guarantee.
    (translate(address as u64, false).is_some()) as u8
}

/// MmCopyMemory(MM_COPY_ADDRESS is an 8-byte union): flags 0 = virtual source,
/// flags 1 = physical source. Destination must be a currently writable mapping.
/// Copies stop at the first invalid translation and report the completed prefix.
pub unsafe extern "win64" fn mm_copy_memory(
    destination: *mut c_void,
    source_address: u64,
    length: u64,
    flags: u32,
    bytes_transferred: *mut u64,
) -> i32 {
    if !bytes_transferred.is_null() { unsafe { bytes_transferred.write(0); } }
    if destination.is_null() || length == 0 || (flags != 0 && flags != 1) {
        return if length == 0 && !destination.is_null() && (flags == 0 || flags == 1) {
            STATUS_SUCCESS
        } else { STATUS_INVALID_PARAMETER };
    }
    let Some(end) = source_address.checked_add(length) else { return STATUS_INVALID_PARAMETER; };
    if flags == 1 && end > 0x1_0000_0000 { return STATUS_INVALID_PARAMETER; }
    let Some(dest_end) = (destination as u64).checked_add(length) else { return STATUS_INVALID_PARAMETER; };
    let mut done = 0u64;
    while done < length {
        let src_va = match source_address.checked_add(done) { Some(v) => v, None => break };
        let dst_va = match (destination as u64).checked_add(done) { Some(v) => v, None => break };
        let src_pa = if flags == 1 {
            // Physical source accesses use the kernel's identity map.
            if src_va >= 0x1_0000_0000 { break; }
            src_va
        } else {
            match translate(src_va, false) { Some(p) => p, None => break }
        };
        let dst_pa = match translate(dst_va, true) { Some(p) => p, None => break };
        let count = core::cmp::min(
            length - done,
            core::cmp::min(PAGE - (src_va & (PAGE - 1)), PAGE - (dst_va & (PAGE - 1))),
        ) as usize;
        if count == 0 { break; }
        unsafe {
            core::ptr::copy(
                src_pa as *const u8,
                dst_pa as *mut u8,
                count,
            );
        }
        done += count as u64;
    }
    if !bytes_transferred.is_null() { unsafe { bytes_transferred.write(done); } }
    if done == length { STATUS_SUCCESS } else { let _ = dest_end; STATUS_PARTIAL_COPY }
}

/// Boot-time validation uses two real kernel VM pages, checks their translations,
/// exercises the MM copy path, and always releases the test extents.
pub fn self_test() -> bool {
    let source = match crate::kernel_vm::allocate_pages(1) { Ok(p) => p, Err(_) => return false };
    let destination = match crate::kernel_vm::allocate_pages(1) {
        Ok(p) => p,
        Err(_) => { let _ = crate::kernel_vm::free_pages(source, 1); return false; }
    };
    unsafe {
        source.write(0x39);
        source.add(127).write(0xA6);
        destination.write(0);
        destination.add(127).write(0);
    }
    let physical = unsafe { mm_get_physical_address(source.cast()) };
    let mut copied = 0u64;
    let status = unsafe { mm_copy_memory(destination.cast(), source as u64, 128, 0, &mut copied) };
    let valid = physical != 0
        && unsafe { mm_is_address_valid(source.cast()) } != 0
        && unsafe { mm_is_non_paged_system_address_valid(source.cast()) } != 0
        && unsafe { mm_get_virtual_for_physical(physical) } as u64 == physical
        && status == STATUS_SUCCESS && copied == 128
        && unsafe { destination.read() } == 0x39
        && unsafe { destination.add(127).read() } == 0xA6;
    let freed = crate::kernel_vm::free_pages(destination, 1).is_ok()
        && crate::kernel_vm::free_pages(source, 1).is_ok();
    valid && freed
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn status_values_match_nt_contract() {
        assert_eq!(STATUS_SUCCESS, 0);
        assert_eq!(STATUS_ACCESS_VIOLATION as u32, 0xC000_0005);
        assert_eq!(STATUS_INVALID_PARAMETER as u32, 0xC000_000D);
        assert_eq!(STATUS_PARTIAL_COPY as u32, 0x8000_000D);
    }
}
