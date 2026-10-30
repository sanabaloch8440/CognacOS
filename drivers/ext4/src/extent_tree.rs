//! Bounded validation of ext4 inline extent roots.
use crate::Error;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TreeExtent { pub logical_start:u32, pub physical_start:u64, pub length:u32, pub unwritten:bool }
fn u16at(b:&[u8],o:usize)->Result<u16,Error>{let s=b.get(o..o+2).ok_or(Error::Truncated)?;Ok(u16::from_le_bytes([s[0],s[1]]))}
fn u32at(b:&[u8],o:usize)->Result<u32,Error>{let s=b.get(o..o+4).ok_or(Error::Truncated)?;Ok(u32::from_le_bytes([s[0],s[1],s[2],s[3]]))}
/// Validate and visit each depth-zero extent in logical order.
pub fn visit_inline_root(root:&[u8],blocks:u64,mut visit:impl FnMut(TreeExtent)->Result<(),Error>)->Result<usize,Error>{
if root.len()!=60||u16at(root,0)?!=0xf30a||u16at(root,6)?!=0{return Err(Error::InvalidExtentTree)}
let n=u16at(root,2)? as usize;let max=u16at(root,4)? as usize;if n==0||n>max||max>4{return Err(Error::InvalidExtentTree)}
let mut end=0u64;for i in 0..n{let o=12+i*12;let l=u32at(root,o)?;let raw=u16at(root,o+4)?;if raw==0{return Err(Error::InvalidExtentTree)}let unwritten=raw<=0x8000;let len=if unwritten{raw as u32}else{(raw-0x8000) as u32};let p=((u16at(root,o+8)? as u64)<<32)|u32at(root,o+10)? as u64;let pe=p.checked_add(len as u64).ok_or(Error::OutOfRange)?;let le=(l as u64).checked_add(len as u64).ok_or(Error::OutOfRange)?;if p==0||pe>blocks||(l as u64)<end||le>(u32::MAX as u64)+1{return Err(Error::InvalidExtentTree)}visit(TreeExtent{logical_start:l,physical_start:p,length:len,unwritten})?;end=le;}Ok(n)
}
