const MAX_SYMBOLIC_LINKS:usize=32;
const MAX_SYMBOLIC_LINK_CHARS:usize=260;
#[derive(Clone,Copy)]
struct SymbolicLink { used:bool, name_len:usize, target_len:usize, name:[u16;MAX_SYMBOLIC_LINK_CHARS], target:[u16;MAX_SYMBOLIC_LINK_CHARS] }
impl SymbolicLink { const EMPTY:Self=Self{used:false,name_len:0,target_len:0,name:[0;MAX_SYMBOLIC_LINK_CHARS],target:[0;MAX_SYMBOLIC_LINK_CHARS]}; }
struct NamespaceState { links:[SymbolicLink;MAX_SYMBOLIC_LINKS] }
impl NamespaceState { const fn new()->Self{Self{links:[SymbolicLink::EMPTY;MAX_SYMBOLIC_LINKS]}} }
static NAMESPACE:IrqSpinLock<NamespaceState>=IrqSpinLock::new(NamespaceState::new());

fn valid_unicode_name(name:*mut UnicodeString)->Option<(usize,*const u16)> {
 if name.is_null(){return None}
 let value=unsafe{&*name};
 let len=value.length as usize;
 if len==0||len%2!=0||len>MAX_SYMBOLIC_LINK_CHARS*2||len>value.maximum_length as usize||value.buffer.is_null(){return None}
 Some((len/2,value.buffer.cast_const()))
}
fn unicode_equal_folded(a:&[u16],b:&[u16])->bool {
 a.len()==b.len()&&a.iter().zip(b).all(|(x,y)| {
  let fold=|c:u16| if (b'A' as u16..=b'Z' as u16).contains(&c){c+32}else{c};
  fold(*x)==fold(*y)
 })
}

/// Create a bounded object-manager symbolic-link mapping. Resolution through
/// this namespace is intentionally separate from this storage primitive.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn io_create_symbolic_link(link_name:*mut UnicodeString,target_name:*mut UnicodeString)->i32 {
 let Some((name_len,name_ptr))=valid_unicode_name(link_name) else{return STATUS_INVALID_PARAMETER};
 let Some((target_len,target_ptr))=valid_unicode_name(target_name) else{return STATUS_INVALID_PARAMETER};
 let mut name=[0u16;MAX_SYMBOLIC_LINK_CHARS];
 let mut target=[0u16;MAX_SYMBOLIC_LINK_CHARS];
 unsafe{ptr::copy_nonoverlapping(name_ptr,name.as_mut_ptr(),name_len);ptr::copy_nonoverlapping(target_ptr,target.as_mut_ptr(),target_len);}
 let mut state=NAMESPACE.lock();
 if state.links.iter().any(|l|l.used&&unicode_equal_folded(&l.name[..l.name_len],&name[..name_len])) {return STATUS_OBJECT_NAME_COLLISION}
 let Some(slot)=state.links.iter_mut().find(|l|!l.used) else{return STATUS_INSUFFICIENT_RESOURCES};
 *slot=SymbolicLink::EMPTY;slot.used=true;slot.name_len=name_len;slot.target_len=target_len;
 slot.name[..name_len].copy_from_slice(&name[..name_len]);slot.target[..target_len].copy_from_slice(&target[..target_len]);
 STATUS_SUCCESS
}

/// Delete a previously created symbolic-link mapping by its counted name.
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn io_delete_symbolic_link(link_name:*mut UnicodeString)->i32 {
 let Some((name_len,name_ptr))=valid_unicode_name(link_name) else{return STATUS_INVALID_PARAMETER};
 let mut name=[0u16;MAX_SYMBOLIC_LINK_CHARS];
 unsafe{ptr::copy_nonoverlapping(name_ptr,name.as_mut_ptr(),name_len);}
 let mut state=NAMESPACE.lock();
 let Some(slot)=state.links.iter_mut().find(|l|l.used&&unicode_equal_folded(&l.name[..l.name_len],&name[..name_len])) else{return STATUS_OBJECT_NAME_NOT_FOUND};
 *slot=SymbolicLink::EMPTY;
 STATUS_SUCCESS
}


/// Windows x64 IO_CSQ public storage layout. The queue remains driver-owned;
/// all operations are routed through its callbacks as required by WDM.
#[repr(C)]