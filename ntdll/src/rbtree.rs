//! Rust port of RTL red-black tree primitives from Wine's ntdll/rtl.c.
//!
//! This is source-first and is not yet wired into the native NTDLL export table.
//! RTL_BALANCED_NODE stores the parent pointer and color in ParentValue; nodes
//! must be naturally aligned so the low two pointer bits are available as tags.
//!
//! Reference: https://github.com/wine-mirror/wine/blob/master/dlls/ntdll/rtl.c

#![allow(non_camel_case_types, non_snake_case)]

use core::ffi::c_void;

pub type BOOLEAN = u8;
pub type PVOID = *mut c_void;

const RED: usize = 1;
const TAG_MASK: usize = 3;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RTL_BALANCED_NODE {
    pub Left: *mut RTL_BALANCED_NODE,
    pub Right: *mut RTL_BALANCED_NODE,
    /// Parent pointer with the low bit used as the red flag.
    pub ParentValue: usize,
}

pub type PRTL_BALANCED_NODE = *mut RTL_BALANCED_NODE;

#[repr(C)]
pub struct RTL_RB_TREE {
    pub Root: PRTL_BALANCED_NODE,
    pub Min: PRTL_BALANCED_NODE,
}

#[inline]
unsafe fn parent(node: PRTL_BALANCED_NODE) -> PRTL_BALANCED_NODE {
    if node.is_null() { return core::ptr::null_mut(); }
    // SAFETY: caller supplies an aligned, initialized node.
    unsafe { ((*node).ParentValue & !TAG_MASK) as PRTL_BALANCED_NODE }
}
#[inline]
unsafe fn is_red(node: PRTL_BALANCED_NODE) -> bool {
    !node.is_null() && unsafe { (*node).ParentValue & RED != 0 }
}
#[inline]
unsafe fn set_parent(node: PRTL_BALANCED_NODE, p: PRTL_BALANCED_NODE) {
    if node.is_null() { return; }
    // Preserve the color bit while replacing the parent pointer.
    unsafe { (*node).ParentValue = (p as usize & !TAG_MASK) | ((*node).ParentValue & TAG_MASK); }
}
#[inline]
unsafe fn set_red(node: PRTL_BALANCED_NODE, red: bool) {
    if node.is_null() { return; }
    unsafe {
        if red { (*node).ParentValue |= RED; }
        else { (*node).ParentValue &= !RED; }
    }
}
#[inline]
unsafe fn set_color(node: PRTL_BALANCED_NODE, p: PRTL_BALANCED_NODE, red: bool) {
    if node.is_null() { return; }
    unsafe { (*node).ParentValue = (p as usize & !TAG_MASK) | if red { RED } else { 0 }; }
}
#[inline]
unsafe fn minimum(mut node: PRTL_BALANCED_NODE) -> PRTL_BALANCED_NODE {
    if node.is_null() { return node; }
    unsafe { while !(*node).Left.is_null() { node = (*node).Left; } }
    node
}

unsafe fn rotate_left(tree: *mut RTL_RB_TREE, x: PRTL_BALANCED_NODE) {
    // SAFETY: tree and x are valid and x has a right child.
    unsafe {
        let y = (*x).Right;
        (*x).Right = (*y).Left;
        if !(*y).Left.is_null() { set_parent((*y).Left, x); }
        let xp = parent(x);
        set_parent(y, xp);
        if xp.is_null() { (*tree).Root = y; }
        else if x == (*xp).Left { (*xp).Left = y; }
        else { (*xp).Right = y; }
        (*y).Left = x;
        set_parent(x, y);
    }
}
unsafe fn rotate_right(tree: *mut RTL_RB_TREE, x: PRTL_BALANCED_NODE) {
    // SAFETY: tree and x are valid and x has a left child.
    unsafe {
        let y = (*x).Left;
        (*x).Left = (*y).Right;
        if !(*y).Right.is_null() { set_parent((*y).Right, x); }
        let xp = parent(x);
        set_parent(y, xp);
        if xp.is_null() { (*tree).Root = y; }
        else if x == (*xp).Right { (*xp).Right = y; }
        else { (*xp).Left = y; }
        (*y).Right = x;
        set_parent(x, y);
    }
}

pub unsafe fn RtlRbInsertNodeEx(
    tree: *mut RTL_RB_TREE,
    parent_node: PRTL_BALANCED_NODE,
    right: BOOLEAN,
    node: PRTL_BALANCED_NODE,
) {
    if tree.is_null() || node.is_null() { return; }
    // SAFETY: caller provides a valid tree, detached writable node, and parent
    // either null (empty root insertion) or an existing node in this tree.
    unsafe {
        (*node).Left = core::ptr::null_mut();
        (*node).Right = core::ptr::null_mut();
        set_color(node, parent_node, true);
        if parent_node.is_null() {
            (*tree).Root = node;
        } else if right != 0 {
            (*parent_node).Right = node;
        } else {
            (*parent_node).Left = node;
        }

        let mut z = node;
        while !parent(z).is_null() && is_red(parent(z)) {
            let p = parent(z);
            let g = parent(p);
            if p == (*g).Left {
                let uncle = (*g).Right;
                if is_red(uncle) {
                    set_red(p, false);
                    set_red(uncle, false);
                    set_red(g, true);
                    z = g;
                } else {
                    if z == (*p).Right {
                        z = p;
                        rotate_left(tree, z);
                    }
                    let p2 = parent(z);
                    let g2 = parent(p2);
                    set_red(p2, false);
                    set_red(g2, true);
                    rotate_right(tree, g2);
                }
            } else {
                let uncle = (*g).Left;
                if is_red(uncle) {
                    set_red(p, false);
                    set_red(uncle, false);
                    set_red(g, true);
                    z = g;
                } else {
                    if z == (*p).Left {
                        z = p;
                        rotate_right(tree, z);
                    }
                    let p2 = parent(z);
                    let g2 = parent(p2);
                    set_red(p2, false);
                    set_red(g2, true);
                    rotate_left(tree, g2);
                }
            }
        }
        set_red((*tree).Root, false);
        (*tree).Min = minimum((*tree).Root);
    }
}

unsafe fn transplant(tree: *mut RTL_RB_TREE, u: PRTL_BALANCED_NODE, v: PRTL_BALANCED_NODE) {
    // SAFETY: u is in tree and v may be null or a valid node.
    unsafe {
        let up = parent(u);
        if up.is_null() { (*tree).Root = v; }
        else if u == (*up).Left { (*up).Left = v; }
        else { (*up).Right = v; }
        if !v.is_null() { set_parent(v, up); }
    }
}

unsafe fn delete_fixup(tree: *mut RTL_RB_TREE, mut x: PRTL_BALANCED_NODE, mut xp: PRTL_BALANCED_NODE) {
    // SAFETY: x and xp are nodes from tree; xp tracks parent when x is null.
    unsafe {
        while x != (*tree).Root && !is_red(x) {
            if xp.is_null() { break; }
            if x == (*xp).Left {
                let mut w = (*xp).Right;
                if is_red(w) {
                    set_red(w, false);
                    set_red(xp, true);
                    rotate_left(tree, xp);
                    w = (*xp).Right;
                }
                let wl = if w.is_null() { core::ptr::null_mut() } else { (*w).Left };
                let wr = if w.is_null() { core::ptr::null_mut() } else { (*w).Right };
                if !is_red(wl) && !is_red(wr) {
                    set_red(w, true);
                    x = xp;
                    xp = parent(x);
                } else {
                    if !is_red(wr) {
                        set_red(wl, false);
                        set_red(w, true);
                        if !w.is_null() { rotate_right(tree, w); }
                        w = (*xp).Right;
                    }
                    if !w.is_null() {
                        set_red(w, is_red(xp));
                        set_red((*w).Right, false);
                    }
                    set_red(xp, false);
                    if !w.is_null() { rotate_left(tree, xp); }
                    x = (*tree).Root;
                    xp = core::ptr::null_mut();
                }
            } else {
                let mut w = (*xp).Left;
                if is_red(w) {
                    set_red(w, false);
                    set_red(xp, true);
                    rotate_right(tree, xp);
                    w = (*xp).Left;
                }
                let wl = if w.is_null() { core::ptr::null_mut() } else { (*w).Left };
                let wr = if w.is_null() { core::ptr::null_mut() } else { (*w).Right };
                if !is_red(wl) && !is_red(wr) {
                    set_red(w, true);
                    x = xp;
                    xp = parent(x);
                } else {
                    if !is_red(wl) {
                        set_red(wr, false);
                        set_red(w, true);
                        if !w.is_null() { rotate_left(tree, w); }
                        w = (*xp).Left;
                    }
                    if !w.is_null() {
                        set_red(w, is_red(xp));
                        set_red((*w).Left, false);
                    }
                    set_red(xp, false);
                    if !w.is_null() { rotate_right(tree, xp); }
                    x = (*tree).Root;
                    xp = core::ptr::null_mut();
                }
            }
        }
        set_red(x, false);
    }
}

pub unsafe fn RtlRbRemoveNode(tree: *mut RTL_RB_TREE, z: PRTL_BALANCED_NODE) {
    if tree.is_null() || z.is_null() { return; }
    // SAFETY: z is a live node in tree. As with the native API, invalid or
    // duplicate removals are caller errors; the routine does not own node memory.
    unsafe {
        let mut y = z;
        let mut y_was_red = is_red(y);
        let x: PRTL_BALANCED_NODE;
        let xp: PRTL_BALANCED_NODE;
        if (*z).Left.is_null() {
            x = (*z).Right;
            xp = parent(z);
            transplant(tree, z, (*z).Right);
        } else if (*z).Right.is_null() {
            x = (*z).Left;
            xp = parent(z);
            transplant(tree, z, (*z).Left);
        } else {
            y = minimum((*z).Right);
            y_was_red = is_red(y);
            x = (*y).Right;
            if parent(y) == z {
                xp = y;
                if !x.is_null() { set_parent(x, y); }
            } else {
                xp = parent(y);
                transplant(tree, y, (*y).Right);
                (*y).Right = (*z).Right;
                set_parent((*y).Right, y);
            }
            transplant(tree, z, y);
            (*y).Left = (*z).Left;
            set_parent((*y).Left, y);
            set_red(y, is_red(z));
        }
        if !y_was_red { delete_fixup(tree, x, xp); }
        (*z).Left = core::ptr::null_mut();
        (*z).Right = core::ptr::null_mut();
        set_color(z, core::ptr::null_mut(), false);
        (*tree).Min = minimum((*tree).Root);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node() -> RTL_BALANCED_NODE {
        RTL_BALANCED_NODE { Left: core::ptr::null_mut(), Right: core::ptr::null_mut(), ParentValue: 0 }
    }

    #[test]
    fn insert_keeps_root_black_and_minimum_current() {
        let mut tree = RTL_RB_TREE { Root: core::ptr::null_mut(), Min: core::ptr::null_mut() };
        let mut n = [node(), node(), node(), node(), node()];
        unsafe {
            RtlRbInsertNodeEx(&mut tree, core::ptr::null_mut(), 0, &mut n[0]);
            RtlRbInsertNodeEx(&mut tree, tree.Root, 0, &mut n[1]);
            let root = tree.Root;
            RtlRbInsertNodeEx(&mut tree, root, 1, &mut n[2]);
            RtlRbInsertNodeEx(&mut tree, &mut n[1], 0, &mut n[3]);
            RtlRbInsertNodeEx(&mut tree, &mut n[2], 1, &mut n[4]);
            assert!(!is_red(tree.Root));
            assert_eq!(tree.Min, &mut n[3] as PRTL_BALANCED_NODE);
        }
    }

    #[test]
    fn remove_root_and_leaf_updates_tree() {
        let mut tree = RTL_RB_TREE { Root: core::ptr::null_mut(), Min: core::ptr::null_mut() };
        let mut n = [node(), node(), node()];
        unsafe {
            RtlRbInsertNodeEx(&mut tree, core::ptr::null_mut(), 0, &mut n[1]);
            let root = tree.Root;
            RtlRbInsertNodeEx(&mut tree, root, 0, &mut n[0]);
            let root = tree.Root;
            RtlRbInsertNodeEx(&mut tree, root, 1, &mut n[2]);
            RtlRbRemoveNode(&mut tree, &mut n[1]);
            assert!(!tree.Root.is_null());
            assert!(!is_red(tree.Root));
            RtlRbRemoveNode(&mut tree, &mut n[0]);
            RtlRbRemoveNode(&mut tree, &mut n[2]);
            assert!(tree.Root.is_null());
            assert!(tree.Min.is_null());
        }
    }
}
