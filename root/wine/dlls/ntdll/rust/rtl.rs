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

