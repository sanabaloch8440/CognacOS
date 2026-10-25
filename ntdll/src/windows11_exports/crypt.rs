//! Windows 11 x64 NTDLL hash-table and random helpers.
//!
//! Dynamic-hash-table structures and signatures follow the public phnt/NTAPI
//! declarations. Caller-owned entries remain caller-owned; only table headers
//! and bucket directories created by this implementation are allocated here.
#![allow(non_snake_case, clippy::missing_safety_doc)]

use core::ffi::c_void;
use core::mem::{size_of, MaybeUninit};
use core::ptr;

type ULONG = u32;
type BOOLEAN = u8;
type ULONG_PTR = usize;
type PVOID = *mut c_void;
type NTSTATUS = i32;

const STATUS_SUCCESS: NTSTATUS = 0;
const STATUS_INVALID_PARAMETER: NTSTATUS = 0xC000_000Du32 as i32;
const STATUS_NOT_IMPLEMENTED: NTSTATUS = 0xC000_0002u32 as i32;
const RTL_HASH_ALLOCATED_HEADER: ULONG = 1;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LIST_ENTRY {
    pub Flink: *mut LIST_ENTRY,
    pub Blink: *mut LIST_ENTRY,
}

#[repr(C)]
#[derive(Debug)]
pub struct RTL_DYNAMIC_HASH_TABLE_ENTRY {
    pub Linkage: LIST_ENTRY,
    pub Signature: ULONG_PTR,
}

#[repr(C)]
pub struct RTL_DYNAMIC_HASH_TABLE_CONTEXT {
    pub ChainHead: *mut LIST_ENTRY,
    pub PrevLinkage: *mut LIST_ENTRY,
    pub Signature: ULONG_PTR,
}

#[repr(C)]
pub struct RTL_DYNAMIC_HASH_TABLE_ENUMERATOR {
    pub HashEntry: RTL_DYNAMIC_HASH_TABLE_ENTRY,
    pub ChainHead: *mut LIST_ENTRY,
    pub BucketIndex: ULONG,
}

#[repr(C)]
pub struct RTL_DYNAMIC_HASH_TABLE {
    pub Flags: ULONG,
    pub Shift: ULONG,
    pub TableSize: ULONG,
    pub Pivot: ULONG,
    pub DivisorMask: ULONG,
    pub NumEntries: ULONG,
    pub NonEmptyBuckets: ULONG,
    pub NumEnumerators: ULONG,
    pub Directory: PVOID,
}

#[inline]
unsafe fn bucket_heads(table: *mut RTL_DYNAMIC_HASH_TABLE) -> *mut LIST_ENTRY {
    unsafe { (*table).Directory.cast::<LIST_ENTRY>() }
}
#[inline]
unsafe fn bucket_index(table: *const RTL_DYNAMIC_HASH_TABLE, signature: ULONG_PTR) -> usize {
    (signature as usize) & unsafe { (*table).DivisorMask as usize }
}
#[inline]
unsafe fn init_list(head: *mut LIST_ENTRY) {
    unsafe { (*head).Flink = head; (*head).Blink = head; }
}
#[inline]
unsafe fn insert_tail(head: *mut LIST_ENTRY, entry: *mut LIST_ENTRY) {
    let tail = unsafe { (*head).Blink };
    unsafe {
        (*entry).Flink = head;
        (*entry).Blink = tail;
        (*tail).Flink = entry;
        (*head).Blink = entry;
    }
}
#[inline]
unsafe fn unlink(entry: *mut LIST_ENTRY) {
    let f = unsafe { (*entry).Flink };
    let b = unsafe { (*entry).Blink };
    unsafe { (*b).Flink = f; (*f).Blink = b; (*entry).Flink = entry; (*entry).Blink = entry; }
}
#[inline]
unsafe fn entry_from_link(link: *mut LIST_ENTRY) -> *mut RTL_DYNAMIC_HASH_TABLE_ENTRY {
    link.cast::<u8>().wrapping_sub(core::mem::offset_of!(RTL_DYNAMIC_HASH_TABLE_ENTRY, Linkage))
        .cast::<RTL_DYNAMIC_HASH_TABLE_ENTRY>()
}
unsafe fn alloc_directory(count: usize) -> Option<*mut LIST_ENTRY> {
    if count == 0 || count > (1usize << 20) { return None; }
    let mut v = Vec::with_capacity(count);
    for _ in 0..count {
        let mut head = MaybeUninit::<LIST_ENTRY>::uninit();
        let p = head.as_mut_ptr();
        unsafe { (*p).Flink = p; (*p).Blink = p; v.push(head.assume_init()); }
    }
    let boxed = v.into_boxed_slice();
    Some(Box::into_raw(boxed) as *mut LIST_ENTRY)
}
unsafe fn free_directory(ptr: *mut LIST_ENTRY, count: usize) {
    if !ptr.is_null() && count != 0 {
        let slice = ptr::slice_from_raw_parts_mut(ptr, count);
        drop(unsafe { Box::from_raw(slice) });
    }
}
unsafe fn initialize_table(table: *mut RTL_DYNAMIC_HASH_TABLE, shift: ULONG, flags: ULONG) -> bool {
    if table.is_null() || shift > 20 { return false; }
    let count = 1usize << shift;
    let Some(directory) = (unsafe { alloc_directory(count) }) else { return false; };
    unsafe {
        (*table).Flags = flags;
        (*table).Shift = shift;
        (*table).TableSize = count as ULONG;
        (*table).Pivot = 0;
        (*table).DivisorMask = count as ULONG - 1;
        (*table).NumEntries = 0;
        (*table).NonEmptyBuckets = 0;
        (*table).NumEnumerators = 0;
        (*table).Directory = directory.cast();
    }
    true
}

#[no_mangle]
pub unsafe extern "system" fn RtlCreateHashTable(
    hash_table: *mut *mut RTL_DYNAMIC_HASH_TABLE, shift: ULONG, flags: ULONG,
) -> BOOLEAN {
    if hash_table.is_null() || shift > 20 { return 0; }
    let mut table = unsafe { *hash_table };
    let allocated = table.is_null();
    if allocated {
        table = Box::into_raw(Box::new(unsafe { MaybeUninit::<RTL_DYNAMIC_HASH_TABLE>::zeroed().assume_init() }));
    }
    if !unsafe { initialize_table(table, shift, if allocated { flags | RTL_HASH_ALLOCATED_HEADER } else { flags & !RTL_HASH_ALLOCATED_HEADER }) } {
        if allocated { drop(unsafe { Box::from_raw(table) }); }
        return 0;
    }
    unsafe { *hash_table = table; }
    1
}

#[no_mangle]
pub unsafe extern "system" fn RtlCreateHashTableEx(
    hash_table: *mut *mut RTL_DYNAMIC_HASH_TABLE, shift: ULONG, table_size: ULONG,
) -> BOOLEAN {
    if table_size == 0 { return 0; }
    let wanted = table_size.next_power_of_two();
    if wanted > (1 << 20) { return 0; }
    let actual_shift = wanted.trailing_zeros().max(1);
    unsafe { RtlCreateHashTable(hash_table, actual_shift.max(shift.min(20)), RTL_HASH_ALLOCATED_HEADER) }
}

#[no_mangle]
pub unsafe extern "system" fn RtlDeleteHashTable(table: *mut RTL_DYNAMIC_HASH_TABLE) {
    if table.is_null() { return; }
    unsafe {
        let directory = bucket_heads(table);
        let size = (*table).TableSize as usize;
        if !directory.is_null() {
            for i in 0..size {
                let head = directory.add(i);
                let mut link = (*head).Flink;
                while link != head {
                    let next = (*link).Flink;
                    unlink(link);
                    link = next;
                }
            }
            free_directory(directory, size);
        }
        let allocated = ((*table).Flags & RTL_HASH_ALLOCATED_HEADER) != 0;
        (*table).Directory = ptr::null_mut();
        (*table).TableSize = 0;
        (*table).NumEntries = 0;
        if allocated { drop(Box::from_raw(table)); }
    }
}

#[no_mangle]
pub unsafe extern "system" fn RtlInsertEntryHashTable(
    table: *mut RTL_DYNAMIC_HASH_TABLE, entry: *mut RTL_DYNAMIC_HASH_TABLE_ENTRY,
    signature: ULONG_PTR, context: *mut RTL_DYNAMIC_HASH_TABLE_CONTEXT,
) -> BOOLEAN {
    if table.is_null() || entry.is_null() || unsafe { (*table).Directory.is_null() || (*table).TableSize == 0 } { return 0; }
    // Caller supplies an unlinked entry; initialize its links as part of insertion.
    let index = unsafe { bucket_index(table, signature) };
    let head = unsafe { bucket_heads(table).add(index) };
    let was_empty = unsafe { (*head).Flink == head };
    unsafe {
        (*entry).Signature = signature;
        insert_tail(head, ptr::addr_of_mut!((*entry).Linkage));
        (*table).NumEntries = (*table).NumEntries.saturating_add(1);
        if was_empty { (*table).NonEmptyBuckets = (*table).NonEmptyBuckets.saturating_add(1); }
        if !context.is_null() {
            (*context).ChainHead = head;
            (*context).PrevLinkage = ptr::addr_of_mut!((*entry).Linkage);
            (*context).Signature = signature;
        }
    }
    1
}

#[no_mangle]
pub unsafe extern "system" fn RtlLookupEntryHashTable(
    table: *mut RTL_DYNAMIC_HASH_TABLE, signature: ULONG_PTR,
    context: *mut RTL_DYNAMIC_HASH_TABLE_CONTEXT,
) -> *mut RTL_DYNAMIC_HASH_TABLE_ENTRY {
    if table.is_null() || unsafe { (*table).Directory.is_null() || (*table).TableSize == 0 } { return ptr::null_mut(); }
    let head = unsafe { bucket_heads(table).add(bucket_index(table, signature)) };
    let mut link = unsafe { (*head).Flink };
    while link != head {
        let entry = unsafe { entry_from_link(link) };
        if unsafe { (*entry).Signature == signature } {
            if !context.is_null() {
                unsafe { (*context).ChainHead = head; (*context).PrevLinkage = link; (*context).Signature = signature; }
            }
            return entry;
        }
        link = unsafe { (*link).Flink };
    }
    if !context.is_null() { unsafe { (*context).ChainHead = head; (*context).PrevLinkage = head; (*context).Signature = signature; } }
    ptr::null_mut()
}

#[no_mangle]
pub unsafe extern "system" fn RtlRemoveEntryHashTable(
    table: *mut RTL_DYNAMIC_HASH_TABLE, entry: *mut RTL_DYNAMIC_HASH_TABLE_ENTRY,
    _context: *mut RTL_DYNAMIC_HASH_TABLE_CONTEXT,
) -> BOOLEAN {
    if table.is_null() || entry.is_null() || unsafe { (*table).Directory.is_null() || (*entry).Linkage.Flink.is_null() || (*entry).Linkage.Blink.is_null() } { return 0; }
    let head = unsafe { bucket_heads(table).add(bucket_index(table, (*entry).Signature)) };
    let empty_after = unsafe { (*entry).Linkage.Flink == head && (*entry).Linkage.Blink == head };
    unsafe {
        unlink(ptr::addr_of_mut!((*entry).Linkage));
        (*table).NumEntries = (*table).NumEntries.saturating_sub(1);
        if empty_after { (*table).NonEmptyBuckets = (*table).NonEmptyBuckets.saturating_sub(1); }
    }
    1
}

#[no_mangle]
pub unsafe extern "system" fn RtlGetNextEntryHashTable(
    table: *mut RTL_DYNAMIC_HASH_TABLE, context: *mut RTL_DYNAMIC_HASH_TABLE_CONTEXT,
) -> *mut RTL_DYNAMIC_HASH_TABLE_ENTRY {
    if table.is_null() || context.is_null() || unsafe { (*table).Directory.is_null() } { return ptr::null_mut(); }
    let head = unsafe { (*context).ChainHead };
    if !head.is_null() {
        let next = unsafe { if (*context).PrevLinkage.is_null() { (*head).Flink } else { (*(*context).PrevLinkage).Flink } };
        if next != head {
            let entry = unsafe { entry_from_link(next) };
            unsafe { (*context).PrevLinkage = next; (*context).Signature = (*entry).Signature; }
            return entry;
        }
    }
    ptr::null_mut()
}

#[no_mangle]
pub unsafe extern "system" fn RtlInitEnumerationHashTable(
    table: *mut RTL_DYNAMIC_HASH_TABLE, enumerator: *mut RTL_DYNAMIC_HASH_TABLE_ENUMERATOR,
) -> BOOLEAN {
    unsafe { init_enumerator(table, enumerator) }
}
#[no_mangle]
pub unsafe extern "system" fn RtlInitWeakEnumerationHashTable(
    table: *mut RTL_DYNAMIC_HASH_TABLE, enumerator: *mut RTL_DYNAMIC_HASH_TABLE_ENUMERATOR,
) -> BOOLEAN { unsafe { init_enumerator(table, enumerator) } }
#[no_mangle]
pub unsafe extern "system" fn RtlInitStrongEnumerationHashTable(
    table: *mut RTL_DYNAMIC_HASH_TABLE, enumerator: *mut RTL_DYNAMIC_HASH_TABLE_ENUMERATOR,
) -> BOOLEAN { unsafe { init_enumerator(table, enumerator) } }

unsafe fn init_enumerator(table: *mut RTL_DYNAMIC_HASH_TABLE, en: *mut RTL_DYNAMIC_HASH_TABLE_ENUMERATOR) -> BOOLEAN {
    if table.is_null() || en.is_null() || unsafe { (*table).Directory.is_null() } { return 0; }
    unsafe {
        (*en).HashEntry.Linkage.Flink = ptr::null_mut();
        (*en).HashEntry.Linkage.Blink = ptr::null_mut();
        (*en).HashEntry.Signature = 0;
        (*en).ChainHead = ptr::null_mut();
        (*en).BucketIndex = 0;
        (*table).NumEnumerators = (*table).NumEnumerators.saturating_add(1);
    }
    1
}

unsafe fn enumerate_next(table: *mut RTL_DYNAMIC_HASH_TABLE, en: *mut RTL_DYNAMIC_HASH_TABLE_ENUMERATOR) -> *mut RTL_DYNAMIC_HASH_TABLE_ENTRY {
    if table.is_null() || en.is_null() || unsafe { (*table).Directory.is_null() } { return ptr::null_mut(); }
    let size = unsafe { (*table).TableSize };
    let mut index = unsafe { (*en).BucketIndex };
    if !unsafe { (*en).ChainHead.is_null() } {
        let head = unsafe { (*en).ChainHead };
        let current = unsafe { (*en).HashEntry.Linkage.Blink };
        let next = if current.is_null() { unsafe { (*head).Flink } } else { unsafe { (*current).Flink } };
        if next != head {
            let entry = unsafe { entry_from_link(next) };
            unsafe { (*en).HashEntry.Linkage.Blink = next; (*en).HashEntry.Signature = (*entry).Signature; }
            return entry;
        }
        index = unsafe { (*en).BucketIndex.saturating_add(1) };
    }
    while index < size {
        let head = unsafe { bucket_heads(table).add(index as usize) };
        if unsafe { (*head).Flink != head } {
            let link = unsafe { (*head).Flink };
            let entry = unsafe { entry_from_link(link) };
            unsafe { (*en).ChainHead = head; (*en).BucketIndex = index; (*en).HashEntry.Linkage.Blink = link; (*en).HashEntry.Signature = (*entry).Signature; }
            return entry;
        }
        index += 1;
    }
    unsafe { (*en).ChainHead = ptr::null_mut(); (*en).HashEntry.Linkage.Blink = ptr::null_mut(); }
    ptr::null_mut()
}

#[no_mangle]
pub unsafe extern "system" fn RtlEnumerateEntryHashTable(table: *mut RTL_DYNAMIC_HASH_TABLE, en: *mut RTL_DYNAMIC_HASH_TABLE_ENUMERATOR) -> *mut RTL_DYNAMIC_HASH_TABLE_ENTRY {
    unsafe { enumerate_next(table, en) }
}
#[no_mangle]
pub unsafe extern "system" fn RtlWeaklyEnumerateEntryHashTable(table: *mut RTL_DYNAMIC_HASH_TABLE, en: *mut RTL_DYNAMIC_HASH_TABLE_ENUMERATOR) -> *mut RTL_DYNAMIC_HASH_TABLE_ENTRY {
    unsafe { enumerate_next(table, en) }
}
#[no_mangle]
pub unsafe extern "system" fn RtlStronglyEnumerateEntryHashTable(table: *mut RTL_DYNAMIC_HASH_TABLE, en: *mut RTL_DYNAMIC_HASH_TABLE_ENUMERATOR) -> *mut RTL_DYNAMIC_HASH_TABLE_ENTRY {
    unsafe { enumerate_next(table, en) }
}

unsafe fn end_enumerator(table: *mut RTL_DYNAMIC_HASH_TABLE, en: *mut RTL_DYNAMIC_HASH_TABLE_ENUMERATOR) {
    if table.is_null() || en.is_null() { return; }
    unsafe {
        (*table).NumEnumerators = (*table).NumEnumerators.saturating_sub(1);
        (*en).ChainHead = ptr::null_mut();
        (*en).HashEntry.Linkage.Blink = ptr::null_mut();
    }
}
#[no_mangle]
pub unsafe extern "system" fn RtlEndEnumerationHashTable(table: *mut RTL_DYNAMIC_HASH_TABLE, en: *mut RTL_DYNAMIC_HASH_TABLE_ENUMERATOR) { unsafe { end_enumerator(table, en); } }
#[no_mangle]
pub unsafe extern "system" fn RtlEndWeakEnumerationHashTable(table: *mut RTL_DYNAMIC_HASH_TABLE, en: *mut RTL_DYNAMIC_HASH_TABLE_ENUMERATOR) { unsafe { end_enumerator(table, en); } }
#[no_mangle]
pub unsafe extern "system" fn RtlEndStrongEnumerationHashTable(table: *mut RTL_DYNAMIC_HASH_TABLE, en: *mut RTL_DYNAMIC_HASH_TABLE_ENUMERATOR) { unsafe { end_enumerator(table, en); } }

#[no_mangle]
pub unsafe extern "system" fn RtlExpandHashTable(table: *mut RTL_DYNAMIC_HASH_TABLE) -> BOOLEAN {
    if table.is_null() || unsafe { (*table).Directory.is_null() || (*table).Shift >= 20 } { return 0; }
    let old_size = unsafe { (*table).TableSize as usize };
    let Some(new_dir) = (unsafe { alloc_directory(old_size * 2) }) else { return 0; };
    unsafe {
        let old_dir = bucket_heads(table);
        for i in 0..old_size {
            let head = old_dir.add(i);
            let mut link = (*head).Flink;
            while link != head {
                let next = (*link).Flink;
                let entry = entry_from_link(link);
                let index = ((*entry).Signature as usize) & (old_size * 2 - 1);
                insert_tail(new_dir.add(index), link);
                link = next;
            }
        }
        free_directory(old_dir, old_size);
        (*table).Directory = new_dir.cast();
        (*table).Shift += 1;
        (*table).TableSize = (old_size * 2) as ULONG;
        (*table).DivisorMask = (*table).TableSize - 1;
        (*table).NonEmptyBuckets = 0;
        for i in 0..(*table).TableSize as usize {
            if (*new_dir.add(i)).Flink != new_dir.add(i) { (*table).NonEmptyBuckets += 1; }
        }
    }
    1
}

#[no_mangle]
pub unsafe extern "system" fn RtlContractHashTable(table: *mut RTL_DYNAMIC_HASH_TABLE) -> BOOLEAN {
    if table.is_null() || unsafe { (*table).Directory.is_null() || (*table).Shift <= 1 } { return 0; }
    let old_size = unsafe { (*table).TableSize as usize };
    let new_size = old_size / 2;
    let Some(new_dir) = (unsafe { alloc_directory(new_size) }) else { return 0; };
    unsafe {
        let old_dir = bucket_heads(table);
        for i in 0..old_size {
            let head = old_dir.add(i);
            let mut link = (*head).Flink;
            while link != head {
                let next = (*link).Flink;
                let entry = entry_from_link(link);
                let index = ((*entry).Signature as usize) & (new_size - 1);
                insert_tail(new_dir.add(index), link);
                link = next;
            }
        }
        free_directory(old_dir, old_size);
        (*table).Directory = new_dir.cast();
        (*table).Shift -= 1;
        (*table).TableSize = new_size as ULONG;
        (*table).DivisorMask = (*table).TableSize - 1;
        (*table).NonEmptyBuckets = 0;
        for i in 0..new_size {
            if (*new_dir.add(i)).Flink != new_dir.add(i) { (*table).NonEmptyBuckets += 1; }
        }
    }
    1
}

#[no_mangle]
pub unsafe extern "system" fn RtlRandomEx(seed: *mut ULONG) -> ULONG {
    if seed.is_null() { return 0; }
    let next = unsafe { (*seed).wrapping_mul(0x7fff_ffed).wrapping_add(0x7fff_ffc3) };
    unsafe { *seed = next; }
    next % 0x7fff_ffff
}
#[no_mangle]
pub unsafe extern "system" fn RtlRandom(seed: *mut ULONG) -> ULONG {
    // Stable deterministic fallback; Windows RtlRandom additionally maintains
    // a per-process shuffle table, which will be connected to process state later.
    unsafe { RtlRandomEx(seed) }
}

#[no_mangle]
pub unsafe extern "system" fn RtlComputeImportTableHash(
    _base_address: PVOID, _hash: *mut u8, _hash_size: ULONG,
) -> ULONG {
    STATUS_NOT_IMPLEMENTED as ULONG
}

#[repr(C)]
pub struct SID_AND_ATTRIBUTES {
    pub Sid: PVOID,
    pub Attributes: ULONG,
}
#[repr(C)]
pub struct SID_AND_ATTRIBUTES_HASH {
    pub SidCount: ULONG,
    pub SidAttr: *mut SID_AND_ATTRIBUTES,
    pub Hash: [ULONG; 32],
}

#[inline]
unsafe fn sid_length(sid: PVOID) -> Option<usize> {
    if sid.is_null() { return None; }
    let bytes = sid.cast::<u8>();
    let revision = unsafe { *bytes };
    let count = unsafe { *bytes.add(1) } as usize;
    if revision != 1 || count > 15 { return None; }
    Some(8 + count * 4)
}

#[no_mangle]
pub unsafe extern "system" fn RtlSidHashInitialize(
    sid_attr: *mut SID_AND_ATTRIBUTES, sid_count: ULONG, sid_attr_hash: *mut SID_AND_ATTRIBUTES_HASH,
) -> NTSTATUS {
    if sid_attr_hash.is_null() || (sid_count != 0 && sid_attr.is_null()) {
        return STATUS_INVALID_PARAMETER;
    }
    unsafe {
        (*sid_attr_hash).SidCount = sid_count;
        (*sid_attr_hash).SidAttr = sid_attr;
        (*sid_attr_hash).Hash = [0; 32];
    }
    for i in 0..sid_count as usize {
        let sid = unsafe { (*sid_attr.add(i)).Sid };
        let Some(length) = (unsafe { sid_length(sid) }) else {
            unsafe { (*sid_attr_hash).SidCount = 0; (*sid_attr_hash).SidAttr = ptr::null_mut(); }
            return STATUS_INVALID_PARAMETER;
        };
        let bytes = unsafe { core::slice::from_raw_parts(sid.cast::<u8>(), length) };
        let mut hash = 2166136261u32;
        for byte in bytes { hash = (hash ^ (*byte as u32)).wrapping_mul(16777619); }
        unsafe { (*sid_attr_hash).Hash[(hash as usize) & 31] |= 1u32 << ((hash >> 5) & 31); }
    }
    STATUS_SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn RtlSidHashLookup(
    sid_attr_hash: *mut SID_AND_ATTRIBUTES_HASH, sid: PVOID,
) -> *mut SID_AND_ATTRIBUTES {
    if sid_attr_hash.is_null() || sid.is_null() { return ptr::null_mut(); }
    let Some(wanted_len) = (unsafe { sid_length(sid) }) else { return ptr::null_mut(); };
    let wanted = unsafe { core::slice::from_raw_parts(sid.cast::<u8>(), wanted_len) };
    let count = unsafe { (*sid_attr_hash).SidCount as usize };
    let items = unsafe { (*sid_attr_hash).SidAttr };
    if count != 0 && items.is_null() { return ptr::null_mut(); }
    for i in 0..count {
        let item = unsafe { items.add(i) };
        let candidate = unsafe { (*item).Sid };
        let Some(length) = (unsafe { sid_length(candidate) }) else { continue; };
        if length == wanted_len && unsafe { core::slice::from_raw_parts(candidate.cast::<u8>(), length) } == wanted {
            return item;
        }
    }
    ptr::null_mut()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dynamic_hash_table_inserts_looks_up_removes_and_expands() {
        unsafe {
            let mut table: *mut RTL_DYNAMIC_HASH_TABLE = ptr::null_mut();
            assert_eq!(RtlCreateHashTable(&mut table, 2, RTL_HASH_ALLOCATED_HEADER), 1);
            let mut entry = RTL_DYNAMIC_HASH_TABLE_ENTRY {
                Linkage: LIST_ENTRY { Flink: ptr::null_mut(), Blink: ptr::null_mut() }, Signature: 0
            };
            entry.Linkage.Flink = ptr::addr_of_mut!(entry.Linkage);
            entry.Linkage.Blink = ptr::addr_of_mut!(entry.Linkage);
            let mut context = RTL_DYNAMIC_HASH_TABLE_CONTEXT { ChainHead: ptr::null_mut(), PrevLinkage: ptr::null_mut(), Signature: 0 };
            assert_eq!(RtlInsertEntryHashTable(table, &mut entry, 0x1234, &mut context), 1);
            assert_eq!(RtlLookupEntryHashTable(table, 0x1234, &mut context), &mut entry as *mut _);
            assert_eq!((*table).NumEntries, 1);
            assert_eq!(RtlExpandHashTable(table), 1);
            assert_eq!(RtlLookupEntryHashTable(table, 0x1234, &mut context), &mut entry);
            assert_eq!(RtlRemoveEntryHashTable(table, &mut entry, &mut context), 1);
            assert_eq!((*table).NumEntries, 0);
            RtlDeleteHashTable(table);
        }
    }

    #[test]
    fn random_ex_updates_seed_and_stays_nonnegative() {
        let mut seed = 1;
        let value = unsafe { RtlRandomEx(&mut seed) };
        assert_eq!(seed, value);
        assert!(value <= 0x7fff_ffff);
    }
}
