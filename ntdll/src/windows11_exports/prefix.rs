//! Native ANSI prefix-table routines (RTL Pfx*).
//! This ABI-compatible linked-list implementation preserves caller-owned table/entry
//! storage and returns the longest component-boundary prefix. It is not the Windows
//! splay-tree implementation, but keeps the public structures and semantics usable.
#![allow(non_snake_case, clippy::missing_safety_doc)]
use core::{ffi::c_void, ptr};
#[repr(C)] struct AnsiString { length:u16, maximum_length:u16, buffer:*mut i8 }
#[repr(C)] struct PrefixTable { node_type_code:i16, name_length:i16, next_prefix_tree:*mut PrefixTableEntry }
#[repr(C)] struct SplayLinks { parent:*mut SplayLinks, left:*mut SplayLinks, right:*mut SplayLinks }
#[repr(C)] struct PrefixTableEntry { node_type_code:i16, name_length:i16, next_prefix_tree:*mut PrefixTableEntry, links:SplayLinks, prefix:*mut AnsiString }
#[no_mangle] pub unsafe extern "system" fn PfxInitialize(table:*mut PrefixTable) { if table.is_null(){return;} unsafe{(*table).node_type_code=0;(*table).name_length=0;(*table).next_prefix_tree=ptr::null_mut();} }
#[no_mangle] pub unsafe extern "system" fn PfxInsertPrefix(table:*mut PrefixTable,prefix:*mut AnsiString,entry:*mut PrefixTableEntry)->u8 {
 if table.is_null()||prefix.is_null()||entry.is_null(){return 0;} let p=unsafe{&*prefix}; if p.buffer.is_null()||p.length==0||p.length>p.maximum_length||p.length>i16::MAX as u16{return 0;}
 let mut cur=unsafe{(*table).next_prefix_tree}; while !cur.is_null(){let old=unsafe{(*cur).prefix};if !old.is_null(){let o=unsafe{&*old};if o.length==p.length&&o.length!=0&&!o.buffer.is_null()&&unsafe{core::slice::from_raw_parts(o.buffer.cast::<u8>(),o.length as usize)==core::slice::from_raw_parts(p.buffer.cast::<u8>(),p.length as usize)}{return 0;}}cur=unsafe{(*cur).next_prefix_tree};}
 unsafe{(*entry).node_type_code=0;(*entry).name_length=p.length as i16;(*entry).prefix=prefix;(*entry).links=SplayLinks{parent:ptr::null_mut(),left:ptr::null_mut(),right:ptr::null_mut()};(*entry).next_prefix_tree=(*table).next_prefix_tree;(*table).next_prefix_tree=entry;} 1
}
#[no_mangle] pub unsafe extern "system" fn PfxRemovePrefix(table:*mut PrefixTable,entry:*mut PrefixTableEntry){if table.is_null()||entry.is_null(){return;}let mut link=ptr::addr_of_mut!((*table).next_prefix_tree);unsafe{while !(*link).is_null(){if *link==entry{*link=(*entry).next_prefix_tree;(*entry).next_prefix_tree=ptr::null_mut();return;}link=ptr::addr_of_mut!((**link).next_prefix_tree);}}}
#[no_mangle] pub unsafe extern "system" fn PfxFindPrefix(table:*mut PrefixTable,full_name:*mut AnsiString)->*mut PrefixTableEntry {
 if table.is_null()||full_name.is_null(){return ptr::null_mut();}let full=unsafe{&*full_name};if full.buffer.is_null()||full.length==0||full.length>full.maximum_length{return ptr::null_mut();}let bytes=unsafe{core::slice::from_raw_parts(full.buffer.cast::<u8>(),full.length as usize)};let mut best=ptr::null_mut();let mut best_len=0usize;let mut cur=unsafe{(*table).next_prefix_tree};while !cur.is_null(){let p=unsafe{(*cur).prefix};if !p.is_null(){let v=unsafe{&*p};let n=v.length as usize;if n>0&&n<=bytes.len()&&n>best_len&&!v.buffer.is_null(){let prefix=unsafe{core::slice::from_raw_parts(v.buffer.cast::<u8>(),n)};let boundary=n==bytes.len()||bytes[n]==b'\\'||bytes[n]==b'/';if boundary&&prefix==&bytes[..n]{best=cur;best_len=n;}}}cur=unsafe{(*cur).next_prefix_tree};}best
}
