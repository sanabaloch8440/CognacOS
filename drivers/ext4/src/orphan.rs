//! Bounded ext4 orphan-chain recovery planning.
//!
//! Validates s_last_orphan and each orphan inode's i_dtime link, then stages
//! every inode repair and the superblock head update through one transaction.
//! The transaction adapter must truncate linked inodes and reclaim zero-link
//! inodes, including all bitmap, extent, directory and checksum updates.
use super::{Error, update_superblock_checksum};

fn le16(b:&[u8],o:usize)->Result<u16,Error>{let s=b.get(o..o+2).ok_or(Error::Truncated)?;Ok(u16::from_le_bytes([s[0],s[1]]))}
fn le32(b:&[u8],o:usize)->Result<u32,Error>{let s=b.get(o..o+4).ok_or(Error::Truncated)?;Ok(u32::from_le_bytes([s[0],s[1],s[2],s[3]]))}
fn put32(b:&mut[u8],o:usize,v:u32)->Result<(),Error>{b.get_mut(o..o+4).ok_or(Error::Truncated)?.copy_from_slice(&v.to_le_bytes());Ok(())}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct OrphanRecord{pub inode_number:u32,pub next_orphan:u32,pub links_count:u16,pub size_low:u32}

/// Implemented by a mount adapter backed by one JBD2 transaction. stage_orphan
/// must finish truncate for linked inodes and fully reclaim zero-link inodes.
pub trait OrphanRecoveryTransaction{
 fn stage_orphan(&mut self,record:OrphanRecord,inode_image:&[u8])->Result<(),Error>;
 fn stage_superblock(&mut self,superblock_image:&[u8])->Result<(),Error>;
 fn commit(&mut self)->Result<(),Error>;
}

/// Recover the bounded legacy orphan chain atomically. All chain validation and
/// inode reads complete before the transaction is committed. visited is
/// caller-backed cycle-detection storage.
pub fn recover_orphan_chain<R,F>(
 superblock:&mut[u8],inode_count:u32,inode_buffer:&mut[u8],visited:&mut[u32],
 transaction:&mut R,mut read_inode:F,
)->Result<usize,Error>
where R:OrphanRecoveryTransaction,F:FnMut(u32,&mut[u8])->Result<(),Error>{
 if superblock.len()<1024||inode_buffer.len()<128||visited.is_empty(){return Err(Error::InvalidGeometry)}
 let mut current=le32(superblock,232)?;
 let mut count=0usize;
 while current!=0{
  if current>inode_count||current<1||count==visited.len()||visited[..count].contains(&current){return Err(Error::InvalidGeometry)}
  visited[count]=current;count+=1;inode_buffer.fill(0);read_inode(current,inode_buffer)?;
  let mode=le16(inode_buffer,0)?;if mode==0{return Err(Error::InvalidGeometry)}
  let record=OrphanRecord{inode_number:current,next_orphan:le32(inode_buffer,20)?,
   links_count:le16(inode_buffer,26)?,size_low:le32(inode_buffer,4)?};
  if record.next_orphan>inode_count{return Err(Error::InvalidGeometry)}
  transaction.stage_orphan(record,inode_buffer)?;current=record.next_orphan;
 }
 let old_head=le32(superblock,232)?;
 put32(superblock,232,0)?;
 if le32(superblock,100)?&0x0400!=0 {
  if let Err(error)=update_superblock_checksum(superblock) {
   put32(superblock,232,old_head)?;return Err(error)
  }
 }
 if let Err(error)=transaction.stage_superblock(superblock) {
  put32(superblock,232,old_head)?;return Err(error)
 }
 if let Err(error)=transaction.commit() {
  // A commit I/O error is uncertain on disk; the mount must remain offline
  // and run journal recovery. Restore only this caller's staging image.
  put32(superblock,232,old_head)?;return Err(error)
 }
 Ok(count)
}

#[cfg(test)]
mod tests{
 use super::*;
 use std::vec::Vec;
 #[derive(Default)]struct Tx{staged:Vec<u32>,superblock_staged:bool,committed:bool,fail_commit:bool}
 impl OrphanRecoveryTransaction for Tx{
  fn stage_orphan(&mut self,r:OrphanRecord,_:&[u8])->Result<(),Error>{self.staged.push(r.inode_number);Ok(())}
  fn stage_superblock(&mut self,_:&[u8])->Result<(),Error>{self.superblock_staged=true;Ok(())}
  fn commit(&mut self)->Result<(),Error>{if self.fail_commit{return Err(Error::Io)}self.committed=true;Ok(())}
 }
 #[test]fn recovers_chain_in_one_commit_and_clears_head(){
  let mut sb=[0u8;1024];sb[232..236].copy_from_slice(&11u32.to_le_bytes());
  let mut inode=[0u8;128];let mut visited=[0u32;8];let mut tx=Tx::default();
  let n=recover_orphan_chain(&mut sb,32,&mut inode,&mut visited,&mut tx,|ino,out|{
   out[0..2].copy_from_slice(&0x8000u16.to_le_bytes());
   out[20..24].copy_from_slice(&(if ino==11{12u32}else{0u32}).to_le_bytes());
   out[26..28].copy_from_slice(&(if ino==11{1u16}else{0u16}).to_le_bytes());Ok(())
  }).unwrap();
  assert_eq!(n,2);assert_eq!(tx.staged,[11,12]);assert!(tx.superblock_staged&&tx.committed);
  assert_eq!(u32::from_le_bytes(sb[232..236].try_into().unwrap()),0);
 }
 #[test]fn failed_commit_restores_caller_superblock_image(){
  let mut sb=[0u8;1024];sb[232..236].copy_from_slice(&19u32.to_le_bytes());
  let mut inode=[0u8;128];let mut visited=[0u32;4];let mut tx=Tx{fail_commit:true,..Tx::default()};
  let result=recover_orphan_chain(&mut sb,32,&mut inode,&mut visited,&mut tx,|_,out|{
   out[0..2].copy_from_slice(&0x8000u16.to_le_bytes());out[20..24].copy_from_slice(&0u32.to_le_bytes());Ok(())
  });
  assert_eq!(result,Err(Error::Io));
  assert_eq!(u32::from_le_bytes(sb[232..236].try_into().unwrap()),19);
  assert!(!tx.committed);
 }
 #[test]fn cycle_aborts_before_commit(){
  let mut sb=[0u8;1024];sb[232..236].copy_from_slice(&4u32.to_le_bytes());
  let mut inode=[0u8;128];let mut visited=[0u32;8];let mut tx=Tx::default();
  let result=recover_orphan_chain(&mut sb,32,&mut inode,&mut visited,&mut tx,|ino,out|{
   out[0..2].copy_from_slice(&0x8000u16.to_le_bytes());out[20..24].copy_from_slice(&ino.to_le_bytes());Ok(())
  });
  assert_eq!(result,Err(Error::InvalidGeometry));assert!(!tx.committed);
 }
}
