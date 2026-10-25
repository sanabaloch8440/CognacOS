//! Source-first Rust port of core RTL splay-tree routines from Wine's ntdll/rtl.c.
//!
//! This file covers the pointer-based RTL_SPLAY_LINKS operations used by NT's
//! intrusive splay trees. Generic-table allocation/callback handling, debug
//! printing, NTUSER forwarding, compression, and other unrelated rtl.c routines
//! remain separate work. No C export names are emitted here, avoiding symbol
//! collisions while the imported Wine C source remains part of the tree.
//!
//! Upstream source: https://github.com/wine-mirror/wine/blob/master/dlls/ntdll/rtl.c

#![allow(non_camel_case_types, non_snake_case)]

use core::ffi::c_void;

pub type BOOLEAN = u8;
pub type PVOID = *mut c_void;

/// Windows RTL_SPLAY_LINKS ABI: root nodes point their Parent field to themselves.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RTL_SPLAY_LINKS {
    pub Parent: *mut RTL_SPLAY_LINKS,
    pub LeftChild: *mut RTL_SPLAY_LINKS,
    pub RightChild: *mut RTL_SPLAY_LINKS,
}

pub type PRTL_SPLAY_LINKS = *mut RTL_SPLAY_LINKS;

#[inline]
unsafe fn is_root(links: PRTL_SPLAY_LINKS) -> bool {
    // SAFETY: Caller must pass a valid initialized RTL_SPLAY_LINKS node.
    unsafe { !links.is_null() && (*links).Parent == links }
}

#[inline]
unsafe fn is_left_child(links: PRTL_SPLAY_LINKS) -> bool {
    // SAFETY: Caller must pass a valid non-root node and valid parent.
    unsafe { !links.is_null() && !(*links).Parent.is_null() && (*(*links).Parent).LeftChild == links }
}

#[inline]
unsafe fn is_right_child(links: PRTL_SPLAY_LINKS) -> bool {
    // SAFETY: Caller must pass a valid non-root node and valid parent.
    unsafe { !links.is_null() && !(*links).Parent.is_null() && (*(*links).Parent).RightChild == links }
}

/// Initializes a node as a root with no children.
pub unsafe fn RtlInitializeSplayLinks(links: PRTL_SPLAY_LINKS) {
    if links.is_null() { return; }
    // SAFETY: Non-null links points to writable storage by caller contract.
    unsafe {
        (*links).Parent = links;
        (*links).LeftChild = core::ptr::null_mut();
        (*links).RightChild = core::ptr::null_mut();
    }
}

pub unsafe fn RtlParent(links: PRTL_SPLAY_LINKS) -> PRTL_SPLAY_LINKS {
    if links.is_null() { return core::ptr::null_mut(); }
    // SAFETY: Caller supplies a valid node.
    unsafe { (*links).Parent }
}

pub unsafe fn RtlLeftChild(links: PRTL_SPLAY_LINKS) -> PRTL_SPLAY_LINKS {
    if links.is_null() { return core::ptr::null_mut(); }
    // SAFETY: Caller supplies a valid node.
    unsafe { (*links).LeftChild }
}

pub unsafe fn RtlRightChild(links: PRTL_SPLAY_LINKS) -> PRTL_SPLAY_LINKS {
    if links.is_null() { return core::ptr::null_mut(); }
    // SAFETY: Caller supplies a valid node.
    unsafe { (*links).RightChild }
}

pub unsafe fn RtlIsRoot(links: PRTL_SPLAY_LINKS) -> BOOLEAN {
    unsafe { is_root(links) as BOOLEAN }
}

pub unsafe fn RtlIsLeftChild(links: PRTL_SPLAY_LINKS) -> BOOLEAN {
    unsafe { is_left_child(links) as BOOLEAN }
}

pub unsafe fn RtlIsRightChild(links: PRTL_SPLAY_LINKS) -> BOOLEAN {
    unsafe { is_right_child(links) as BOOLEAN }
}

pub unsafe fn RtlInsertAsLeftChild(parent: PRTL_SPLAY_LINKS, child: PRTL_SPLAY_LINKS) {
    if parent.is_null() || child.is_null() { return; }
    // SAFETY: Caller guarantees both nodes are writable and child is detached.
    unsafe {
        (*parent).LeftChild = child;
        (*child).Parent = parent;
    }
}

pub unsafe fn RtlInsertAsRightChild(parent: PRTL_SPLAY_LINKS, child: PRTL_SPLAY_LINKS) {
    if parent.is_null() || child.is_null() { return; }
    // SAFETY: Caller guarantees both nodes are writable and child is detached.
    unsafe {
        (*parent).RightChild = child;
        (*child).Parent = parent;
    }
}

pub unsafe fn RtlSubtreePredecessor(links: PRTL_SPLAY_LINKS) -> PRTL_SPLAY_LINKS {
    if links.is_null() { return core::ptr::null_mut(); }
    // SAFETY: Caller supplies a valid tree.
    unsafe {
        let mut child = (*links).LeftChild;
        if child.is_null() { return child; }
        while !(*child).RightChild.is_null() { child = (*child).RightChild; }
        child
    }
}

pub unsafe fn RtlSubtreeSuccessor(links: PRTL_SPLAY_LINKS) -> PRTL_SPLAY_LINKS {
    if links.is_null() { return core::ptr::null_mut(); }
    // SAFETY: Caller supplies a valid tree.
    unsafe {
        let mut child = (*links).RightChild;
        if child.is_null() { return child; }
        while !(*child).LeftChild.is_null() { child = (*child).LeftChild; }
        child
    }
}

pub unsafe fn RtlRealPredecessor(links: PRTL_SPLAY_LINKS) -> PRTL_SPLAY_LINKS {
    if links.is_null() { return core::ptr::null_mut(); }
    // SAFETY: Caller supplies a valid tree with consistent parent links.
    unsafe {
        let mut child = (*links).LeftChild;
        if !child.is_null() {
            while !(*child).RightChild.is_null() { child = (*child).RightChild; }
            return child;
        }
        child = links;
        while is_left_child(child) { child = (*child).Parent; }
        if is_right_child(child) { (*child).Parent } else { core::ptr::null_mut() }
    }
}

pub unsafe fn RtlRealSuccessor(links: PRTL_SPLAY_LINKS) -> PRTL_SPLAY_LINKS {
    if links.is_null() { return core::ptr::null_mut(); }
    // SAFETY: Caller supplies a valid tree with consistent parent links.
    unsafe {
        let mut child = (*links).RightChild;
        if !child.is_null() {
            while !(*child).LeftChild.is_null() { child = (*child).LeftChild; }
            return child;
        }
        child = links;
        while is_right_child(child) { child = (*child).Parent; }
        if is_left_child(child) { (*child).Parent } else { core::ptr::null_mut() }
    }
}

unsafe fn replace_node(x: PRTL_SPLAY_LINKS, y: PRTL_SPLAY_LINKS, root: *mut PRTL_SPLAY_LINKS) {
    // SAFETY: Caller supplies valid nodes and a writable root pointer.
    unsafe {
        if is_root(x) { *root = y; }
        else if is_left_child(x) { (*(*x).Parent).LeftChild = y; }
        else { (*(*x).Parent).RightChild = y; }
        if !y.is_null() { (*y).Parent = if is_root(x) { y } else { (*x).Parent }; }
    }
}

unsafe fn rotate_left(x: PRTL_SPLAY_LINKS) {
    // SAFETY: Caller supplies a valid node in a valid tree.
    unsafe {
        let y = (*x).RightChild;
        if !y.is_null() {
            (*x).RightChild = (*y).LeftChild;
            if !(*y).LeftChild.is_null() { (*(*y).LeftChild).Parent = x; }
            (*y).Parent = if is_root(x) { y } else { (*x).Parent };
        }
        if !is_root(x) {
            if is_left_child(x) { (*(*x).Parent).LeftChild = y; }
            else { (*(*x).Parent).RightChild = y; }
        }
        if !y.is_null() { (*y).LeftChild = x; }
        (*x).Parent = y;
    }
}

unsafe fn rotate_right(x: PRTL_SPLAY_LINKS) {
    // SAFETY: Caller supplies a valid node in a valid tree.
    unsafe {
        let y = (*x).LeftChild;
        if !y.is_null() {
            (*x).LeftChild = (*y).RightChild;
            if !(*y).RightChild.is_null() { (*(*y).RightChild).Parent = x; }
            (*y).Parent = if is_root(x) { y } else { (*x).Parent };
        }
        if !is_root(x) {
            if is_left_child(x) { (*(*x).Parent).LeftChild = y; }
            else { (*(*x).Parent).RightChild = y; }
        }
        if !y.is_null() { (*y).RightChild = x; }
        (*x).Parent = y;
    }
}

/// Splays a node to the root and returns the new root.
pub unsafe fn RtlSplay(links: PRTL_SPLAY_LINKS) -> PRTL_SPLAY_LINKS {
    if links.is_null() { return links; }
    // SAFETY: Caller supplies a valid tree with consistent links.
    unsafe {
        while !is_root(links) {
            let parent = (*links).Parent;
            if is_root(parent) {
                if is_left_child(links) { rotate_right(parent); }
                else { rotate_left(parent); }
            } else if is_left_child(parent) && is_left_child(links) {
                rotate_right((*parent).Parent);
                rotate_right(parent);
            } else if is_right_child(parent) && is_right_child(links) {
                rotate_left((*parent).Parent);
                rotate_left(parent);
            } else if is_right_child(parent) && is_left_child(links) {
                rotate_right(parent);
                rotate_left((*links).Parent);
            } else {
                rotate_left(parent);
                rotate_right((*links).Parent);
            }
        }
        links
    }
}

/// Deletes a node without splaying and updates the caller's root pointer.
pub unsafe fn RtlDeleteNoSplay(links: PRTL_SPLAY_LINKS, root: *mut PRTL_SPLAY_LINKS) {
    if links.is_null() || root.is_null() { return; }
    // SAFETY: Caller supplies a valid tree and writable root pointer.
    unsafe {
        if is_root(links) && (*links).LeftChild.is_null() && (*links).RightChild.is_null() {
            *root = core::ptr::null_mut();
        } else if (*links).LeftChild.is_null() {
            replace_node(links, (*links).RightChild, root);
        } else if (*links).RightChild.is_null() {
            replace_node(links, (*links).LeftChild, root);
        } else {
            let predecessor = RtlSubtreePredecessor(links);
            if (*predecessor).Parent != links {
                replace_node(predecessor, (*predecessor).LeftChild, root);
                RtlInsertAsLeftChild(predecessor, (*links).LeftChild);
            }
            replace_node(links, predecessor, root);
            RtlInsertAsRightChild(predecessor, (*links).RightChild);
        }
    }
}

/// Deletes a node and splays the affected ancestor; returns the new root.
pub unsafe fn RtlDelete(links: PRTL_SPLAY_LINKS) -> PRTL_SPLAY_LINKS {
    if links.is_null() { return core::ptr::null_mut(); }
    // SAFETY: Caller supplies a valid tree with consistent links.
    unsafe {
        if is_root(links) && (*links).LeftChild.is_null() && (*links).RightChild.is_null() {
            return core::ptr::null_mut();
        }
        let mut root: PRTL_SPLAY_LINKS = core::ptr::null_mut();
        let to_splay;
        if (*links).LeftChild.is_null() {
            replace_node(links, (*links).RightChild, &mut root);
            if is_root(links) { return (*links).RightChild; }
            to_splay = (*links).Parent;
        } else if (*links).RightChild.is_null() {
            replace_node(links, (*links).LeftChild, &mut root);
            if is_root(links) { return (*links).LeftChild; }
            to_splay = (*links).Parent;
        } else {
            let predecessor = RtlSubtreePredecessor(links);
            if (*predecessor).Parent != links {
                replace_node(predecessor, (*predecessor).LeftChild, &mut root);
                RtlInsertAsLeftChild(predecessor, (*links).LeftChild);
                to_splay = (*predecessor).Parent;
            } else {
                to_splay = predecessor;
            }
            replace_node(links, predecessor, &mut root);
            RtlInsertAsRightChild(predecessor, (*links).RightChild);
        }
        RtlSplay(to_splay)
    }
}


/// Computes cumulative reflected CRC-32 using polynomial 0xedb88320.
///
/// # Safety
/// For positive `length`, `data` must point to at least `length` readable bytes.
pub unsafe fn RtlComputeCrc32(initial: u32, data: *const u8, length: i32) -> u32 {
    if length <= 0 { return initial; }
    if data.is_null() { return initial; }
    // SAFETY: The caller guarantees a readable buffer of the requested length.
    let bytes = unsafe { core::slice::from_raw_parts(data, length as usize) };
    let mut crc = !initial;
    for &byte in bytes {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

/// Windows RTL uniform linear-congruential generator.
///
/// # Safety
/// `seed` must be null or point to a writable u32.
pub unsafe fn RtlUniform(seed: *mut u32) -> u32 {
    if seed.is_null() { return 0; }
    // SAFETY: The caller guarantees that a non-null seed is writable.
    unsafe {
        let next = (((*seed as u64) * 0x7fff_ffed_u64 + 0x7fff_ffc3_u64)
            % 0x7fff_ffff_u64) as u32;
        *seed = next;
        next
    }
}

#[inline]
pub fn RtlUshortByteSwap(value: u16) -> u16 { value.swap_bytes() }

#[inline]
pub fn RtlUlongByteSwap(value: u32) -> u32 { value.swap_bytes() }

#[inline]
pub fn RtlUlonglongByteSwap(value: u64) -> u64 { value.swap_bytes() }


// ---------------------------------------------------------------------------
// RTL_GENERIC_TABLE: splay-tree index plus insertion-order list.
// Source-first Rust helpers; exports are deferred until build integration.
// ---------------------------------------------------------------------------
pub type CLONG = i32;
pub type ULONG = u32;
pub type PULONG = *mut ULONG;
pub type PBOOLEAN = *mut BOOLEAN;
pub type PRTL_GENERIC_COMPARE_ROUTINE = Option<unsafe extern "system" fn(*mut RTL_GENERIC_TABLE, PVOID, PVOID) -> i32>;
pub type PRTL_GENERIC_ALLOCATE_ROUTINE = Option<unsafe extern "system" fn(*mut RTL_GENERIC_TABLE, usize) -> PVOID>;
pub type PRTL_GENERIC_FREE_ROUTINE = Option<unsafe extern "system" fn(*mut RTL_GENERIC_TABLE, PVOID)>;

pub const GENERIC_LESS_THAN: i32 = 0;
pub const GENERIC_EQUAL: i32 = 1;
pub const GENERIC_GREATER_THAN: i32 = 2;
const GENERIC_TABLE_DATA_OFFSET: usize = 40;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LIST_ENTRY {
    pub Flink: *mut LIST_ENTRY,
    pub Blink: *mut LIST_ENTRY,
}

#[repr(C)]
pub struct RTL_GENERIC_TABLE {
    pub TableRoot: PRTL_SPLAY_LINKS,
    pub InsertOrderList: LIST_ENTRY,
    pub OrderedPointer: *mut LIST_ENTRY,
    pub NumberGenericTableElements: ULONG,
    pub WhichOrderedElement: ULONG,
    pub CompareRoutine: PRTL_GENERIC_COMPARE_ROUTINE,
    pub AllocateRoutine: PRTL_GENERIC_ALLOCATE_ROUTINE,
    pub FreeRoutine: PRTL_GENERIC_FREE_ROUTINE,
    pub TableContext: PVOID,
}

#[inline]
unsafe fn generic_data(links: PRTL_SPLAY_LINKS) -> PVOID {
    // SAFETY: Generic table allocations reserve the ABI header before element bytes.
    unsafe { (links as *mut u8).add(GENERIC_TABLE_DATA_OFFSET).cast() }
}
#[inline]
unsafe fn generic_list_entry(links: PRTL_SPLAY_LINKS) -> *mut LIST_ENTRY {
    // SAFETY: LIST_ENTRY immediately follows RTL_SPLAY_LINKS in each allocation.
    unsafe { (links as *mut u8).add(core::mem::size_of::<RTL_SPLAY_LINKS>()).cast() }
}
#[inline]
unsafe fn generic_data_from_list(entry: *mut LIST_ENTRY) -> PVOID {
    // SAFETY: entry is the list member of a valid generic table node.
    unsafe { (entry as *mut u8).sub(16).add(GENERIC_TABLE_DATA_OFFSET).cast() }
}
#[inline]
unsafe fn list_insert_tail(head: *mut LIST_ENTRY, entry: *mut LIST_ENTRY) {
    // SAFETY: head is a circular list head and entry is writable detached storage.
    unsafe {
        let tail = (*head).Blink;
        (*entry).Flink = head;
        (*entry).Blink = tail;
        (*tail).Flink = entry;
        (*head).Blink = entry;
    }
}
#[inline]
unsafe fn list_remove(entry: *mut LIST_ENTRY) {
    // SAFETY: entry is linked into a valid circular list.
    unsafe {
        (*(*entry).Blink).Flink = (*entry).Flink;
        (*(*entry).Flink).Blink = (*entry).Blink;
        (*entry).Flink = entry;
        (*entry).Blink = entry;
    }
}
#[inline]
unsafe fn generic_find(table: *mut RTL_GENERIC_TABLE, value: PVOID) -> PRTL_SPLAY_LINKS {
    if table.is_null() || value.is_null() { return core::ptr::null_mut(); }
    // SAFETY: callback and tree nodes belong to an initialized table.
    unsafe {
        let Some(compare) = (*table).CompareRoutine else { return core::ptr::null_mut(); };
        let mut node = (*table).TableRoot;
        while !node.is_null() {
            match compare(table, generic_data(node), value) {
                GENERIC_LESS_THAN => node = (*node).RightChild,
                GENERIC_GREATER_THAN => node = (*node).LeftChild,
                _ => return node,
            }
        }
        core::ptr::null_mut()
    }
}

pub unsafe fn RtlInitializeGenericTable(
    table: *mut RTL_GENERIC_TABLE,
    compare: PRTL_GENERIC_COMPARE_ROUTINE,
    allocate: PRTL_GENERIC_ALLOCATE_ROUTINE,
    free: PRTL_GENERIC_FREE_ROUTINE,
    context: PVOID,
) {
    if table.is_null() { return; }
    // SAFETY: caller supplies writable RTL_GENERIC_TABLE storage.
    unsafe {
        (*table).TableRoot = core::ptr::null_mut();
        let head = &mut (*table).InsertOrderList as *mut LIST_ENTRY;
        (*head).Flink = head;
        (*head).Blink = head;
        (*table).OrderedPointer = head;
        (*table).NumberGenericTableElements = 0;
        (*table).WhichOrderedElement = 0;
        (*table).CompareRoutine = compare;
        (*table).AllocateRoutine = allocate;
        (*table).FreeRoutine = free;
        (*table).TableContext = context;
    }
}

pub unsafe fn RtlIsGenericTableEmpty(table: *const RTL_GENERIC_TABLE) -> BOOLEAN {
    if table.is_null() { return 1; }
    // SAFETY: table points to an initialized RTL_GENERIC_TABLE.
    unsafe { ((*table).TableRoot.is_null()) as BOOLEAN }
}
pub unsafe fn RtlNumberGenericTableElements(table: *const RTL_GENERIC_TABLE) -> ULONG {
    if table.is_null() { return 0; }
    // SAFETY: table points to an initialized RTL_GENERIC_TABLE.
    unsafe { (*table).NumberGenericTableElements }
}

/// Inserts a copy of size bytes, or returns an existing equal element.
///
/// # Safety
/// The table callbacks must follow the Windows callback contract. value must
/// point to size readable bytes and new_element, when non-null, must be writable.
pub unsafe fn RtlInsertElementGenericTable(
    table: *mut RTL_GENERIC_TABLE, value: PVOID, size: CLONG, new_element: PBOOLEAN,
) -> PVOID {
    if !new_element.is_null() { unsafe { *new_element = 0; } }
    if table.is_null() || value.is_null() || size < 0 { return core::ptr::null_mut(); }
    // SAFETY: initialized table and valid callback contracts are caller preconditions.
    unsafe {
        let (Some(compare), Some(allocate)) = ((*table).CompareRoutine, (*table).AllocateRoutine) else {
            return core::ptr::null_mut();
        };
        let mut node = (*table).TableRoot;
        let mut parent: PRTL_SPLAY_LINKS = core::ptr::null_mut();
        let mut relation;
        while !node.is_null() {
            relation = compare(table, generic_data(node), value);
            if relation == GENERIC_EQUAL { return generic_data(node); }
            parent = node;
            node = if relation == GENERIC_LESS_THAN { (*node).RightChild } else { (*node).LeftChild };
        }
        let total = match GENERIC_TABLE_DATA_OFFSET.checked_add(size as usize) {
            Some(n) => n, None => return core::ptr::null_mut(),
        };
        let node = allocate(table, total).cast::<RTL_SPLAY_LINKS>();
        if node.is_null() { return core::ptr::null_mut(); }
        RtlInitializeSplayLinks(node);
        let list = generic_list_entry(node);
        (*list).Flink = list;
        (*list).Blink = list;
        let data = generic_data(node);
        core::ptr::copy_nonoverlapping(value.cast::<u8>(), data.cast::<u8>(), size as usize);
        list_insert_tail(&mut (*table).InsertOrderList, list);
        if !parent.is_null() {
            relation = compare(table, generic_data(parent), value);
            if relation == GENERIC_LESS_THAN { RtlInsertAsRightChild(parent, node); }
            else { RtlInsertAsLeftChild(parent, node); }
        }
        (*table).TableRoot = RtlSplay(node);
        (*table).NumberGenericTableElements = (*table).NumberGenericTableElements.saturating_add(1);
        (*table).OrderedPointer = &mut (*table).InsertOrderList;
        (*table).WhichOrderedElement = 0;
        if !new_element.is_null() { *new_element = 1; }
        data
    }
}

pub unsafe fn RtlLookupElementGenericTable(table: *mut RTL_GENERIC_TABLE, value: PVOID) -> PVOID {
    // SAFETY: generic_find validates null pointers and uses the initialized table callback.
    unsafe {
        let node = generic_find(table, value);
        if node.is_null() { return core::ptr::null_mut(); }
        (*table).TableRoot = RtlSplay(node);
        generic_data(node)
    }
}

pub unsafe fn RtlDeleteElementGenericTable(table: *mut RTL_GENERIC_TABLE, value: PVOID) -> BOOLEAN {
    if table.is_null() { return 0; }
    // SAFETY: table callbacks and allocation lifetime follow the initialized-table contract.
    unsafe {
        let Some(free) = (*table).FreeRoutine else { return 0; };
        let node = generic_find(table, value);
        if node.is_null() { return 0; }
        let list = generic_list_entry(node);
        (*table).TableRoot = RtlDelete(node);
        list_remove(list);
        (*table).NumberGenericTableElements = (*table).NumberGenericTableElements.saturating_sub(1);
        (*table).OrderedPointer = &mut (*table).InsertOrderList;
        (*table).WhichOrderedElement = 0;
        free(table, node.cast());
        1
    }
}

/// Enumerates sorted elements without splaying. previous stores an internal node pointer.
pub unsafe fn RtlEnumerateGenericTableWithoutSplaying(table: *mut RTL_GENERIC_TABLE, previous: *mut PVOID) -> PVOID {
    if table.is_null() || previous.is_null() { return core::ptr::null_mut(); }
    // SAFETY: previous is writable; its value is null or a node from this table.
    unsafe {
        let mut node = if (*previous).is_null() {
            let mut n = (*table).TableRoot;
            while !n.is_null() && !(*n).LeftChild.is_null() { n = (*n).LeftChild; }
            n
        } else {
            RtlRealSuccessor((*previous).cast())
        };
        if node.is_null() { *previous = core::ptr::null_mut(); return core::ptr::null_mut(); }
        *previous = node.cast();
        generic_data(node)
    }
}

/// Enumerates sorted elements while splaying each returned node to the root.
pub unsafe fn RtlEnumerateGenericTable(table: *mut RTL_GENERIC_TABLE, restart: BOOLEAN) -> PVOID {
    if table.is_null() { return core::ptr::null_mut(); }
    // SAFETY: table is initialized and owns its tree.
    unsafe {
        let mut node = (*table).TableRoot;
        if node.is_null() { return core::ptr::null_mut(); }
        if restart != 0 {
            while !(*node).LeftChild.is_null() { node = (*node).LeftChild; }
        } else {
            node = RtlRealSuccessor(node);
            if node.is_null() { return core::ptr::null_mut(); }
        }
        (*table).TableRoot = RtlSplay(node);
        generic_data(node)
    }
}

/// Returns an element by insertion-order index.
pub unsafe fn RtlGetElementGenericTable(table: *mut RTL_GENERIC_TABLE, index: ULONG) -> PVOID {
    if table.is_null() { return core::ptr::null_mut(); }
    // SAFETY: table is initialized and its insertion-order list is valid.
    unsafe {
        let count = (*table).NumberGenericTableElements;
        if index >= count { return core::ptr::null_mut(); }
        let head = &mut (*table).InsertOrderList as *mut LIST_ENTRY;
        let mut entry: *mut LIST_ENTRY;
        if (*table).WhichOrderedElement != 0 && index >= (*table).WhichOrderedElement {
            entry = (*table).OrderedPointer;
            for _ in (*table).WhichOrderedElement..index { entry = (*entry).Flink; }
        } else if (*table).WhichOrderedElement != 0 && index < (*table).WhichOrderedElement {
            entry = (*table).OrderedPointer;
            for _ in index..(*table).WhichOrderedElement { entry = (*entry).Blink; }
        } else if index < count / 2 {
            entry = (*head).Flink;
            for _ in 0..index { entry = (*entry).Flink; }
        } else {
            entry = (*head).Blink;
            for _ in 0..(count - index - 1) { entry = (*entry).Blink; }
        }
        (*table).OrderedPointer = entry;
        (*table).WhichOrderedElement = index + 1;
        generic_data_from_list(entry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traversal_and_splay_preserve_parent_child_links() {
        let mut nodes = [
            RTL_SPLAY_LINKS { Parent: core::ptr::null_mut(), LeftChild: core::ptr::null_mut(), RightChild: core::ptr::null_mut() },
            RTL_SPLAY_LINKS { Parent: core::ptr::null_mut(), LeftChild: core::ptr::null_mut(), RightChild: core::ptr::null_mut() },
            RTL_SPLAY_LINKS { Parent: core::ptr::null_mut(), LeftChild: core::ptr::null_mut(), RightChild: core::ptr::null_mut() },
        ];
        let a = &mut nodes[0] as PRTL_SPLAY_LINKS;
        let b = &mut nodes[1] as PRTL_SPLAY_LINKS;
        let c = &mut nodes[2] as PRTL_SPLAY_LINKS;
        unsafe {
            RtlInitializeSplayLinks(b);
            RtlInitializeSplayLinks(a);
            RtlInitializeSplayLinks(c);
            RtlInsertAsLeftChild(b, a);
            RtlInsertAsRightChild(b, c);
            assert_eq!(RtlSubtreePredecessor(b), a);
            assert_eq!(RtlSubtreeSuccessor(b), c);
            assert_eq!(RtlRealSuccessor(a), b);
            assert_eq!(RtlRealPredecessor(c), b);
            assert_eq!(RtlSplay(a), a);
            assert_eq!((*a).RightChild, b);
            assert_eq!((*b).Parent, a);
            assert_eq!((*b).RightChild, c);
            assert_eq!((*c).Parent, b);
        }
    }

    #[test]
    fn deleting_single_root_returns_null() {
        let mut node = RTL_SPLAY_LINKS { Parent: core::ptr::null_mut(), LeftChild: core::ptr::null_mut(), RightChild: core::ptr::null_mut() };
        unsafe {
            let ptr = &mut node as PRTL_SPLAY_LINKS;
            RtlInitializeSplayLinks(ptr);
            assert!(RtlDelete(ptr).is_null());
        }
    }

    #[test]
    fn delete_no_splay_updates_root() {
        let mut nodes = [
            RTL_SPLAY_LINKS { Parent: core::ptr::null_mut(), LeftChild: core::ptr::null_mut(), RightChild: core::ptr::null_mut() },
            RTL_SPLAY_LINKS { Parent: core::ptr::null_mut(), LeftChild: core::ptr::null_mut(), RightChild: core::ptr::null_mut() },
            RTL_SPLAY_LINKS { Parent: core::ptr::null_mut(), LeftChild: core::ptr::null_mut(), RightChild: core::ptr::null_mut() },
        ];
        unsafe {
            let root = &mut nodes[1] as PRTL_SPLAY_LINKS;
            let left = &mut nodes[0] as PRTL_SPLAY_LINKS;
            let right = &mut nodes[2] as PRTL_SPLAY_LINKS;
            RtlInitializeSplayLinks(root);
            RtlInitializeSplayLinks(left);
            RtlInitializeSplayLinks(right);
            RtlInsertAsLeftChild(root, left);
            RtlInsertAsRightChild(root, right);
            let mut root_ptr = root;
            RtlDeleteNoSplay(root, &mut root_ptr);
            assert_eq!(root_ptr, left);
            assert_eq!((*left).RightChild, right);
            assert_eq!((*right).Parent, left);
        }
    }
    #[test]
    fn crc32_matches_standard_vector_and_supports_chaining() {
        let bytes = b"123456789";
        unsafe {
            assert_eq!(RtlComputeCrc32(0, bytes.as_ptr(), bytes.len() as i32), 0xcbf4_3926);
            let first = RtlComputeCrc32(0, b"1234".as_ptr(), 4);
            assert_eq!(RtlComputeCrc32(first, b"56789".as_ptr(), 5), 0xcbf4_3926);
            assert_eq!(RtlComputeCrc32(0x1234, core::ptr::null(), 0), 0x1234);
        }
    }

    #[test]
    fn uniform_updates_seed_using_windows_constants() {
        let mut seed = 1u32;
        let expected = (((1u64 * 0x7fff_ffed + 0x7fff_ffc3) % 0x7fff_ffff) as u32);
        unsafe { assert_eq!(RtlUniform(&mut seed), expected); }
        assert_eq!(seed, expected);
    }

    #[test]
    fn byte_swap_helpers_reverse_endianness() {
        assert_eq!(RtlUshortByteSwap(0x1234), 0x3412);
        assert_eq!(RtlUlongByteSwap(0x1234_5678), 0x7856_3412);
        assert_eq!(RtlUlonglongByteSwap(0x0123_4567_89ab_cdef), 0xefcd_ab89_6745_2301);
    }


}