//! Kernel-owned dynamic AMD64 runtime-function table registry.
//!
//! Registration is process-scoped. Every caller-owned .pdata read is copied
//! through the active address-space checker, and table removal waits for active
//! lookups so loader unload cannot race an unwind lookup.

use core::sync::atomic::{AtomicBool, Ordering};

const STATUS_SUCCESS: u64 = 0;
const STATUS_INVALID_PARAMETER: u64 = 0xC000_000D;
const STATUS_INVALID_HANDLE: u64 = 0xC000_0008;
const STATUS_ACCESS_VIOLATION: u64 = 0xC000_0005;
const STATUS_INSUFFICIENT_RESOURCES: u64 = 0xC000_009A;
const MAX_TABLES: usize = 64;
const MAX_ENTRIES: u32 = 8192;
const ENTRY_SIZE: u64 = 12;

#[derive(Clone, Copy)]
struct Table {
    active: bool,
    removing: bool,
    readers: u32,
    owner: u64,
    address: u64,
    count: u32,
    image_base: u64,
}
impl Table {
    const EMPTY: Self = Self { active: false, removing: false, readers: 0, owner: 0, address: 0, count: 0, image_base: 0 };
}
static LOCK: AtomicBool = AtomicBool::new(false);
static mut TABLES: [Table; MAX_TABLES] = [Table::EMPTY; MAX_TABLES];
struct Guard;
impl Guard {
    fn acquire() -> Self {
        while LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }
        Self
    }
}
impl Drop for Guard {
    fn drop(&mut self) { LOCK.store(false, Ordering::Release); }
}

#[derive(Clone, Copy)]
struct RuntimeFunction { begin: u32, end: u32, unwind: u32 }
fn read_entry(address: u64) -> Result<RuntimeFunction, u64> {
    let mut bytes = [0u8; ENTRY_SIZE as usize];
    if !crate::syscall::copy_current_user_from(address, &mut bytes) { return Err(STATUS_ACCESS_VIOLATION); }
    Ok(RuntimeFunction {
        begin: u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
        end: u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
        unwind: u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
    })
}

/// args = [user table pointer, entry count, image base, ...]
pub fn register(args: &[u64; crate::syscall::NT_MAX_ARGUMENTS]) -> u64 {
    let address = args[0];
    let count = args[1];
    let image_base = args[2];
    if address == 0 || count == 0 || count > MAX_ENTRIES || image_base == 0
        || address.checked_add(count.saturating_mul(ENTRY_SIZE)).is_none()
        || image_base.checked_add(u32::MAX as u64).is_none() {
        return STATUS_INVALID_PARAMETER;
    }
    let owner = crate::syscall::current_process_id();
    if owner == 0 { return STATUS_INVALID_PARAMETER; }

    let mut previous_end = 0u32;
    for i in 0..count {
        let Some(entry_address) = address.checked_add(i * ENTRY_SIZE) else { return STATUS_INVALID_PARAMETER; };
        let entry = match read_entry(entry_address) { Ok(v) => v, Err(s) => return s };
        if entry.begin >= entry.end || entry.begin < previous_end || entry.unwind & 3 != 0
            || image_base.checked_add(entry.end as u64).is_none() {
            return STATUS_INVALID_PARAMETER;
        }
        previous_end = entry.end;
    }

    let _guard = Guard::acquire();
    let tables = unsafe { &mut *core::ptr::addr_of_mut!(TABLES) };
    if tables.iter().any(|t| t.active && t.owner == owner && t.address == address && t.image_base == image_base) {
        return STATUS_INVALID_PARAMETER;
    }
    let Some(slot) = tables.iter_mut().find(|t| !t.active) else { return STATUS_INSUFFICIENT_RESOURCES; };
    *slot = Table { active: true, removing: false, readers: 0, owner, address, count: count as u32, image_base };
    STATUS_SUCCESS
}

/// args = [user table pointer, ...]; returns BOOLEAN in the syscall result.
pub fn delete(args: &[u64; crate::syscall::NT_MAX_ARGUMENTS]) -> u64 {
    let address = args[0];
    if address == 0 { return 0; }
    let owner = crate::syscall::current_process_id();
    let index = {
        let _guard = Guard::acquire();
        let tables = unsafe { &mut *core::ptr::addr_of_mut!(TABLES) };
        let Some(i) = tables.iter().position(|t| t.active && !t.removing && t.owner == owner && t.address == address) else { return 0; };
        tables[i].removing = true;
        i
    };
    loop {
        let done = {
            let _guard = Guard::acquire();
            let tables = unsafe { &mut *core::ptr::addr_of_mut!(TABLES) };
            if tables[index].readers == 0 {
                tables[index] = Table::EMPTY;
                true
            } else { false }
        };
        if done { return 1; }
        core::hint::spin_loop();
    }
}

/// args = [ControlPc, optional ImageBase output pointer, ...]; returns a user
/// RUNTIME_FUNCTION pointer or zero. The table is pinned until scanning ends.
pub fn lookup(args: &[u64; crate::syscall::NT_MAX_ARGUMENTS]) -> u64 {
    let pc = args[0];
    if pc == 0 { return 0; }
    let owner = crate::syscall::current_process_id();
    let candidate = {
        let _guard = Guard::acquire();
        let tables = unsafe { &mut *core::ptr::addr_of_mut!(TABLES) };
        let mut selected = None;
        for (index, table) in tables.iter_mut().enumerate() {
            if table.active && !table.removing && table.owner == owner
                && pc >= table.image_base && pc - table.image_base <= u32::MAX as u64 {
                table.readers = table.readers.saturating_add(1);
                selected = Some((index, table.address, table.count, table.image_base, (pc - table.image_base) as u32));
                break;
            }
        }
        selected
    };
    let Some((index, address, count, image_base, rva)) = candidate else {
        if args[1] != 0 { let _ = crate::syscall::copy_current_user_to(args[1], &0u64.to_le_bytes()); }
        return 0;
    };

    let mut lo = 0u32;
    let mut hi = count;
    let mut found = 0u64;
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let Some(entry_address) = address.checked_add(mid as u64 * ENTRY_SIZE) else { break; };
        let entry = match read_entry(entry_address) { Ok(v) => v, Err(_) => break };
        if entry.begin >= entry.end || entry.unwind & 3 != 0 { break; }
        if rva < entry.begin { hi = mid; }
        else if rva >= entry.end { lo = mid + 1; }
        else { found = entry_address; break; }
    }
    if args[1] != 0 && crate::syscall::copy_current_user_to(args[1], &image_base.to_le_bytes()).is_err() {
        found = 0;
    }
    {
        let _guard = Guard::acquire();
        let tables = unsafe { &mut *core::ptr::addr_of_mut!(TABLES) };
        if tables[index].active && tables[index].readers > 0 { tables[index].readers -= 1; }
    }
    found
}

/// Called by process exit before the address space can be reaped.
pub fn remove_owner(owner: u64) {
    if owner == 0 { return; }
    let mut indices = [usize::MAX; MAX_TABLES];
    let mut n = 0usize;
    {
        let _guard = Guard::acquire();
        let tables = unsafe { &mut *core::ptr::addr_of_mut!(TABLES) };
        for (i, table) in tables.iter_mut().enumerate() {
            if table.active && table.owner == owner {
                table.removing = true;
                indices[n] = i;
                n += 1;
            }
        }
    }
    for index in indices[..n].iter().copied() {
        loop {
            let done = {
                let _guard = Guard::acquire();
                let tables = unsafe { &mut *core::ptr::addr_of_mut!(TABLES) };
                if tables[index].readers == 0 { tables[index] = Table::EMPTY; true } else { false }
            };
            if done { break; }
            core::hint::spin_loop();
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn registry_capacity_is_bounded() {
        assert!(super::MAX_TABLES > 0);
        assert!(super::MAX_ENTRIES <= 8192);
        assert_eq!(core::mem::size_of::<super::RuntimeFunction>(), 12);
    }
}
