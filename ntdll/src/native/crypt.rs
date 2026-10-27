//! Windows 11 x64 NTDLL hash-table and random helpers.
//!
//! Dynamic-hash-table structures and signatures follow the public phnt/NTAPI
//! declarations. Caller-owned entries remain caller-owned; only table headers
//! and bucket directories created by this implementation are allocated here.
#![allow(non_snake_case, clippy::missing_safety_doc)]

use alloc::{boxed::Box, vec::Vec};

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
            assert_eq!(RtlLookupEntryHashTable(table, 0x1234, &mut context), &mut entry as *mut _);
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


// Legacy-compatible cryptographic context APIs share this authoritative
// source file, isolated to preserve their ABI types without colliding with
// the native RTL hash-table and random helper implementation.
mod legacy_compat {
//! Rust source port of Wine's \`dlls/ntdll/crypt.c\`.
//!
//! Source-only port of the legacy NTDLL SHA-1, MD4, and MD5 helpers. The
//! context layouts and exported entry points are kept C-compatible. This file
//! deliberately does not add a dependency on a host crypto library: Wine's
//! routines are implemented directly so CognacOS can provide them natively.
//!
//! Compatibility/security note: MD4 and MD5 are legacy algorithms and must
//! not be selected for new security-sensitive designs.

#![allow(non_snake_case, non_camel_case_types)]

use core::ptr;

type ULONG = u32;
type UINT = u32;
type UCHAR = u8;
type INT = i32;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SHA_CTX {
    pub Unknown: [ULONG; 6],
    pub State: [ULONG; 5],
    pub Count: [ULONG; 2],
    pub Buffer: [UCHAR; 64],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MD4_CTX {
    pub buf: [ULONG; 4],
    pub i: [ULONG; 2],
    pub input: [UCHAR; 64],
    pub digest: [UCHAR; 16],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MD5_CTX {
    pub i: [ULONG; 2],
    pub buf: [ULONG; 4],
    pub input: [UCHAR; 64],
    pub digest: [UCHAR; 16],
}

#[inline]
fn rol(x: u32, n: u32) -> u32 {
    x.rotate_left(n)
}

#[inline]
unsafe fn copy_bytes(dst: *mut u8, src: *const u8, len: usize) {
    if len != 0 {
        unsafe { ptr::copy_nonoverlapping(src, dst, len) };
    }
}

#[inline]
unsafe fn zero_bytes(dst: *mut u8, len: usize) {
    if len != 0 {
        unsafe { ptr::write_bytes(dst, 0, len) };
    }
}

/* ------------------------------- SHA-1 ---------------------------------- */

fn sha1_transform(state: &mut [u32; 5], block: &[u8; 64]) {
    let mut w = [0u32; 80];
    for i in 0..16 {
        let p = i * 4;
        w[i] = u32::from_le_bytes([block[p], block[p + 1], block[p + 2], block[p + 3]]);
        w[i] = w[i].rotate_left(24) & 0xff00_ff00 | w[i].rotate_left(8) & 0x00ff_00ff;
    }
    for i in 16..80 {
        w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
    }

    let [mut a, mut b, mut c, mut d, mut e] = *state;
    for i in 0..80 {
        let (f, k) = match i {
            0..=19 => ((b & c) | (!b & d), 0x5a82_7999),
            20..=39 => (b ^ c ^ d, 0x6ed9_eba1),
            40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1b_bcdc),
            _ => (b ^ c ^ d, 0xca62_c1d6),
        };
        let temp = a.rotate_left(5).wrapping_add(f).wrapping_add(e)
            .wrapping_add(k).wrapping_add(w[i]);
        e = d;
        d = c;
        c = b.rotate_left(30);
        b = a;
        a = temp;
    }
    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
    state[4] = state[4].wrapping_add(e);
}

#[no_mangle]
pub unsafe extern "system" fn A_SHAInit(context: *mut SHA_CTX) {
    if context.is_null() { return; }
    unsafe {
        (*context).State = [
            0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476, 0xc3d2_e1f0,
        ];
        (*context).Count = [0, 0];
    }
}

#[no_mangle]
pub unsafe extern "system" fn A_SHAUpdate(
    context: *mut SHA_CTX,
    mut buffer: *const UCHAR,
    mut buffer_size: UINT,
) {
    if context.is_null() || (buffer.is_null() && buffer_size != 0) { return; }
    let ctx = unsafe { &mut *context };
    let mut buffered = (ctx.Count[1] & 63) as usize;
    let old_low = ctx.Count[1];
    ctx.Count[1] = ctx.Count[1].wrapping_add(buffer_size);
    if ctx.Count[1] < old_low { ctx.Count[0] = ctx.Count[0].wrapping_add(1); }
    ctx.Count[0] = ctx.Count[0].wrapping_add(buffer_size >> 29);

    if buffered + (buffer_size as usize) < 64 {
        unsafe { copy_bytes(ctx.Buffer.as_mut_ptr().add(buffered), buffer, buffer_size as usize); }
        return;
    }

    while buffered + buffer_size as usize >= 64 {
        let take = 64 - buffered;
        unsafe { copy_bytes(ctx.Buffer.as_mut_ptr().add(buffered), buffer, take); }
        buffer = unsafe { buffer.add(take) };
        buffer_size -= take as UINT;
        sha1_transform(&mut ctx.State, &ctx.Buffer);
        buffered = 0;
    }
    unsafe { copy_bytes(ctx.Buffer.as_mut_ptr(), buffer, buffer_size as usize); }
}

#[no_mangle]
pub unsafe extern "system" fn A_SHAFinal(context: *mut SHA_CTX, result: *mut ULONG) {
    if context.is_null() || result.is_null() { return; }
    let ctx = unsafe { &mut *context };
    let buffered = (ctx.Count[1] & 63) as usize;
    let pad = if buffered >= 56 { 56 + 64 - buffered } else { 56 - buffered };
    let bit_hi = (ctx.Count[0] << 3) | (ctx.Count[1] >> 29);
    let bit_lo = ctx.Count[1] << 3;
    let mut tail = [0u8; 72];
    tail[0] = 0x80;
    tail[pad..pad + 4].copy_from_slice(&bit_hi.to_be_bytes());
    tail[pad + 4..pad + 8].copy_from_slice(&bit_lo.to_be_bytes());
    unsafe { A_SHAUpdate(context, tail.as_ptr(), (pad + 8) as UINT); }
    for i in 0..5 {
        unsafe { *result.add(i) = ctx.State[i].to_be(); }
    }
    unsafe { A_SHAInit(context); }
}

/* -------------------------------- MD4 ------------------------------------ */

#[inline]
fn md4_f(x: u32, y: u32, z: u32) -> u32 { (x & y) | (!x & z) }
#[inline]
fn md4_g(x: u32, y: u32, z: u32) -> u32 { (x & y) | (x & z) | (y & z) }
#[inline]
fn md4_h(x: u32, y: u32, z: u32) -> u32 { x ^ y ^ z }

fn md4_transform(state: &mut [u32; 4], words: &[u32; 16]) {
    let [mut a, mut b, mut c, mut d] = *state;
    macro_rules! ff { ($a:ident,$b:ident,$c:ident,$d:ident,$k:expr,$s:expr) => {
        $a = $a.wrapping_add(md4_f($b,$c,$d)).wrapping_add(words[$k]).rotate_left($s);
    }; }
    macro_rules! gg { ($a:ident,$b:ident,$c:ident,$d:ident,$k:expr,$s:expr) => {
        $a = $a.wrapping_add(md4_g($b,$c,$d)).wrapping_add(words[$k]).wrapping_add(0x5a82_7999).rotate_left($s);
    }; }
    macro_rules! hh { ($a:ident,$b:ident,$c:ident,$d:ident,$k:expr,$s:expr) => {
        $a = $a.wrapping_add(md4_h($b,$c,$d)).wrapping_add(words[$k]).wrapping_add(0x6ed9_eba1).rotate_left($s);
    }; }

    ff!(a,b,c,d,0,3); ff!(d,a,b,c,1,7); ff!(c,d,a,b,2,11); ff!(b,c,d,a,3,19);
    ff!(a,b,c,d,4,3); ff!(d,a,b,c,5,7); ff!(c,d,a,b,6,11); ff!(b,c,d,a,7,19);
    ff!(a,b,c,d,8,3); ff!(d,a,b,c,9,7); ff!(c,d,a,b,10,11); ff!(b,c,d,a,11,19);
    ff!(a,b,c,d,12,3); ff!(d,a,b,c,13,7); ff!(c,d,a,b,14,11); ff!(b,c,d,a,15,19);

    gg!(a,b,c,d,0,3); gg!(d,a,b,c,4,5); gg!(c,d,a,b,8,9); gg!(b,c,d,a,12,13);
    gg!(a,b,c,d,1,3); gg!(d,a,b,c,5,5); gg!(c,d,a,b,9,9); gg!(b,c,d,a,13,13);
    gg!(a,b,c,d,2,3); gg!(d,a,b,c,6,5); gg!(c,d,a,b,10,9); gg!(b,c,d,a,14,13);
    gg!(a,b,c,d,3,3); gg!(d,a,b,c,7,5); gg!(c,d,a,b,11,9); gg!(b,c,d,a,15,13);

    hh!(a,b,c,d,0,3); hh!(d,a,b,c,8,9); hh!(c,d,a,b,4,11); hh!(b,c,d,a,12,15);
    hh!(a,b,c,d,2,3); hh!(d,a,b,c,10,9); hh!(c,d,a,b,6,11); hh!(b,c,d,a,14,15);
    hh!(a,b,c,d,1,3); hh!(d,a,b,c,9,9); hh!(c,d,a,b,5,11); hh!(b,c,d,a,13,15);
    hh!(a,b,c,d,3,3); hh!(d,a,b,c,11,9); hh!(c,d,a,b,7,11); hh!(b,c,d,a,15,15);

    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
}

fn little_endian_words(block: &[u8; 64]) -> [u32; 16] {
    let mut words = [0u32; 16];
    for i in 0..16 {
        words[i] = u32::from_le_bytes([
            block[i * 4], block[i * 4 + 1], block[i * 4 + 2], block[i * 4 + 3],
        ]);
    }
    words
}

fn md4_process_block(state: &mut [u32; 4], block: &[u8; 64]) {
    md4_transform(state, &little_endian_words(block));
}

#[no_mangle]
pub unsafe extern "system" fn MD4Init(context: *mut MD4_CTX) {
    if context.is_null() { return; }
    unsafe {
        (*context).buf = [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476];
        (*context).i = [0, 0];
        (*context).input = [0; 64];
        (*context).digest = [0; 16];
    }
}

#[no_mangle]
pub unsafe extern "system" fn MD4Update(
    context: *mut MD4_CTX,
    mut buffer: *const UCHAR,
    mut length: UINT,
) {
    if context.is_null() || (buffer.is_null() && length != 0) { return; }
    let ctx = unsafe { &mut *context };
    let old_low = ctx.i[0];
    ctx.i[0] = ctx.i[0].wrapping_add(length << 3);
    if ctx.i[0] < old_low { ctx.i[1] = ctx.i[1].wrapping_add(1); }
    ctx.i[1] = ctx.i[1].wrapping_add(length >> 29);
    let mut buffered = ((old_low >> 3) & 0x3f) as usize;

    if buffered != 0 {
        let take = 64 - buffered;
        if (length as usize) < take {
            unsafe { copy_bytes(ctx.input.as_mut_ptr().add(buffered), buffer, length as usize); }
            return;
        }
        unsafe { copy_bytes(ctx.input.as_mut_ptr().add(buffered), buffer, take); }
        md4_process_block(&mut ctx.buf, &ctx.input);
        buffer = unsafe { buffer.add(take) };
        length -= take as UINT;
    }
    while length >= 64 {
        let mut block = [0u8; 64];
        unsafe { copy_bytes(block.as_mut_ptr(), buffer, 64); }
        md4_process_block(&mut ctx.buf, &block);
        buffer = unsafe { buffer.add(64) };
        length -= 64;
    }
    buffered = 0;
    unsafe { copy_bytes(ctx.input.as_mut_ptr().add(buffered), buffer, length as usize); }
}

#[no_mangle]
pub unsafe extern "system" fn MD4Final(context: *mut MD4_CTX) {
    if context.is_null() { return; }
    let ctx = unsafe { &mut *context };
    let bit_count = [ctx.i[0], ctx.i[1]];
    let mut buffered = ((ctx.i[0] >> 3) & 0x3f) as usize;
    ctx.input[buffered] = 0x80;
    buffered += 1;
    if buffered > 56 {
        ctx.input[buffered..].fill(0);
        md4_process_block(&mut ctx.buf, &ctx.input);
        ctx.input = [0; 64];
    } else {
        ctx.input[buffered..56].fill(0);
    }
    ctx.input[56..60].copy_from_slice(&bit_count[0].to_le_bytes());
    ctx.input[60..64].copy_from_slice(&bit_count[1].to_le_bytes());
    md4_process_block(&mut ctx.buf, &ctx.input);
    for i in 0..4 {
        ctx.digest[i * 4..i * 4 + 4].copy_from_slice(&ctx.buf[i].to_le_bytes());
    }
}

/* -------------------------------- MD5 ------------------------------------ */

fn md5_transform(state: &mut [u32; 4], words: &[u32; 16]) {
    const S: [u32; 64] = [
        7,12,17,22,7,12,17,22,7,12,17,22,7,12,17,22,
        5,9,14,20,5,9,14,20,5,9,14,20,5,9,14,20,
        4,11,16,23,4,11,16,23,4,11,16,23,4,11,16,23,
        6,10,15,21,6,10,15,21,6,10,15,21,6,10,15,21,
    ];
    const K: [u32; 64] = [
        0xd76a_a478,0xe8c7_b756,0x2420_70db,0xc1bd_ceee,0xf57c_0faf,0x4787_c62a,0xa830_4613,0xfd46_9501,
        0x6980_98d8,0x8b44_f7af,0xffff_5bb1,0x895c_d7be,0x6b90_1122,0xfd98_7193,0xa679_438e,0x49b4_0821,
        0xf61e_2562,0xc040_b340,0x265e_5a51,0xe9b6_c7aa,0xd62f_105d,0x0244_1453,0xd8a1_e681,0xe7d3_fbc8,
        0x21e1_cde6,0xc337_07d6,0xf4d5_0d87,0x455a_14ed,0xa9e3_e905,0xfcefa3f8,0x676f_02d9,0x8d2a_4c8a,
        0xfffa_3942,0x8771_f681,0x6d9d_6122,0xfde5_380c,0xa4be_ea44,0x4bde_cfa9,0xf6bb_4b60,0xbebf_bc70,
        0x289b_7ec6,0xeaa1_27fa,0xd4ef_3085,0x0488_1d05,0xd9d4_d039,0xe6db_99e5,0x1fa2_7cf8,0xc4ac_5665,
        0xf429_2244,0x432a_ff97,0xab94_23a7,0xfc93_a039,0x655b_59c3,0x8f0c_cc92,0xffef_f47d,0x8584_5dd1,
        0x6fa8_7e4f,0xfe2c_e6e0,0xa301_4314,0x4e08_11a1,0xf753_7e82,0xbd3a_f235,0x2ad7_d2bb,0xeb86_d391,
    ];
    let [mut a, mut b, mut c, mut d] = *state;
    for i in 0..64 {
        let (f, g) = match i {
            0..=15 => ((b & c) | (!b & d), i),
            16..=31 => ((d & b) | (!d & c), (5 * i + 1) & 15),
            32..=47 => (b ^ c ^ d, (3 * i + 5) & 15),
            _ => (c ^ (b | !d), (7 * i) & 15),
        };
        let next = a.wrapping_add(f).wrapping_add(K[i]).wrapping_add(words[g]);
        let next = b.wrapping_add(rol(next, S[i]));
        a = d;
        d = c;
        c = b;
        b = next;
    }
    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
}

fn md5_process_block(state: &mut [u32; 4], block: &[u8; 64]) {
    md5_transform(state, &little_endian_words(block));
}

#[no_mangle]
pub unsafe extern "system" fn MD5Init(context: *mut MD5_CTX) {
    if context.is_null() { return; }
    unsafe {
        (*context).i = [0, 0];
        (*context).buf = [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476];
        (*context).input = [0; 64];
        (*context).digest = [0; 16];
    }
}

#[no_mangle]
pub unsafe extern "system" fn MD5Update(
    context: *mut MD5_CTX,
    mut buffer: *const UCHAR,
    mut length: UINT,
) {
    if context.is_null() || (buffer.is_null() && length != 0) { return; }
    let ctx = unsafe { &mut *context };
    let old_low = ctx.i[0];
    ctx.i[0] = ctx.i[0].wrapping_add(length << 3);
    if ctx.i[0] < old_low { ctx.i[1] = ctx.i[1].wrapping_add(1); }
    ctx.i[1] = ctx.i[1].wrapping_add(length >> 29);
    let mut buffered = ((old_low >> 3) & 0x3f) as usize;

    if buffered != 0 {
        let take = 64 - buffered;
        if (length as usize) < take {
            unsafe { copy_bytes(ctx.input.as_mut_ptr().add(buffered), buffer, length as usize); }
            return;
        }
        unsafe { copy_bytes(ctx.input.as_mut_ptr().add(buffered), buffer, take); }
        md5_process_block(&mut ctx.buf, &ctx.input);
        buffer = unsafe { buffer.add(take) };
        length -= take as UINT;
    }
    while length >= 64 {
        let mut block = [0u8; 64];
        unsafe { copy_bytes(block.as_mut_ptr(), buffer, 64); }
        md5_process_block(&mut ctx.buf, &block);
        buffer = unsafe { buffer.add(64) };
        length -= 64;
    }
    buffered = 0;
    unsafe { copy_bytes(ctx.input.as_mut_ptr().add(buffered), buffer, length as usize); }
}

#[no_mangle]
pub unsafe extern "system" fn MD5Final(context: *mut MD5_CTX) {
    if context.is_null() { return; }
    let ctx = unsafe { &mut *context };
    let bit_count = [ctx.i[0], ctx.i[1]];
    let mut buffered = ((ctx.i[0] >> 3) & 0x3f) as usize;
    ctx.input[buffered] = 0x80;
    buffered += 1;
    if buffered > 56 {
        ctx.input[buffered..].fill(0);
        md5_process_block(&mut ctx.buf, &ctx.input);
        ctx.input = [0; 64];
    } else {
        ctx.input[buffered..56].fill(0);
    }
    ctx.input[56..60].copy_from_slice(&bit_count[0].to_le_bytes());
    ctx.input[60..64].copy_from_slice(&bit_count[1].to_le_bytes());
    md5_process_block(&mut ctx.buf, &ctx.input);
    for i in 0..4 {
        ctx.digest[i * 4..i * 4 + 4].copy_from_slice(&ctx.buf[i].to_le_bytes());
    }
}

}

pub use legacy_compat::*;
