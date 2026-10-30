//! Bounded HTree split and index-entry mutation primitives.
//! These operate on caller-owned buffers only; callers must stage every
//! changed leaf, index block, inode and allocation bitmap in one JBD2 commit.
use crate::{checksum::crc32c_update, hash::{hash_directory_name,DirectoryHash}, Error};

#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub struct DirectoryMapEntry { pub offset:u32, pub record_len:u32, pub major:u32, pub minor:u32 }

fn u16at(b:&[u8],o:usize)->Result<u16,Error>{let s=b.get(o..o+2).ok_or(Error::Truncated)?;Ok(u16::from_le_bytes([s[0],s[1]]))}
fn u32at(b:&[u8],o:usize)->Result<u32,Error>{let s=b.get(o..o+4).ok_or(Error::Truncated)?;Ok(u32::from_le_bytes([s[0],s[1],s[2],s[3]]))}
fn put16(b:&mut[u8],o:usize,v:u16)->Result<(),Error>{b.get_mut(o..o+2).ok_or(Error::Truncated)?.copy_from_slice(&v.to_le_bytes());Ok(())}
fn put32(b:&mut[u8],o:usize,v:u32)->Result<(),Error>{b.get_mut(o..o+4).ok_or(Error::Truncated)?.copy_from_slice(&v.to_le_bytes());Ok(())}
fn rec_len(raw:u16,bs:usize)->usize{if bs>=65536{if raw==0||raw==u16::MAX{bs}else{((raw as usize)&0xfffc)|(((raw as usize)&3)<<16)}}else{raw as usize}}
fn rec_disk(n:usize,bs:usize)->Result<u16,Error>{if n==bs&&bs==65536{Ok(u16::MAX)}else if n<=u16::MAX as usize{Ok(n as u16)}else{Err(Error::OutOfRange)}}
fn ideal(n:usize)->Result<usize,Error>{8usize.checked_add(n).and_then(|x|x.checked_add(3)).map(|x|x&!3).ok_or(Error::OutOfRange)}
fn has_csum(sb:&[u8])->Result<bool,Error>{if sb.len()<1024{return Err(Error::Truncated)}Ok(u32at(sb,100)?&0x400!=0)}
fn write_leaf_tail(block:&mut[u8],seed:u32)->Result<(),Error>{if block.len()<12{return Err(Error::Truncated)}let t=block.len()-12;put32(block,t,0)?;put16(block,t+4,12)?;block[t+6]=0;block[t+7]=0xde;let sum=crc32c_update(seed,&block[..block.len()-4]);put32(block,block.len()-4,sum)}
fn dx_limit(block:&[u8],off:usize,csum:bool)->Result<(usize,usize),Error>{let limit=u16at(block,off)? as usize;let count=u16at(block,off+2)? as usize;let tail=if csum{8}else{0};let expected=block.len().checked_sub(off+tail).ok_or(Error::InvalidDirectoryEntry)?/8;if limit==0||limit!=expected||count==0||count>limit{return Err(Error::InvalidDirectoryEntry)}Ok((limit,count))}
fn dx_checksum(block:&[u8],off:usize,limit:usize,count:usize,seed:u32)->Result<u32,Error>{let used=off.checked_add(count.checked_mul(8).ok_or(Error::OutOfRange)?).ok_or(Error::OutOfRange)?;let tail=off.checked_add(limit.checked_mul(8).ok_or(Error::OutOfRange)?).ok_or(Error::OutOfRange)?;if tail+8!=block.len()||used>tail||u32at(block,tail)?!=0{return Err(Error::InvalidDirectoryEntry)}let mut crc=crc32c_update(seed,&block[..used]);crc=crc32c_update(crc,&block[tail..tail+4]);Ok(crc32c_update(crc,&[0;4]))}
fn update_dx(block:&mut[u8],off:usize,limit:usize,count:usize,csum:bool,seed:u32)->Result<(),Error>{if csum{let sum=dx_checksum(block,off,limit,count,seed)?;put32(block,off+limit*8+4,sum)?}Ok(())}

/// Split a full leaf into two compacted leaves, ordered by Linux major/minor
/// hash. `map` must hold one element per live entry and `source_copy` must be
/// a block-sized scratch buffer. The returned separator has bit zero set when
/// equal-major-hash names straddle the split.
pub fn split_directory_leaf(
 sb:&[u8],inode:u32,generation:u32,old_leaf:&mut[u8],new_leaf:&mut[u8],
 version:u8,hash_seed:[u32;4],siphash_key:Option<[u64;2]>,
 map:&mut[DirectoryMapEntry],source_copy:&mut[u8],
)->Result<u32,Error>{
 if old_leaf.len()!=new_leaf.len()||old_leaf.len()!=source_copy.len()||old_leaf.len()<1024{return Err(Error::InvalidGeometry)}
 let csum=has_csum(sb)?;let seed=if csum{crate::ext4_inode_checksum_seed(sb,inode,generation)?}else{0};
 if csum{crate::validate_directory_block_checksum(sb,inode,generation,old_leaf)?;}
 let limit=old_leaf.len()-if csum{12}else{0};let mut off=0usize;let mut count=0usize;
 while off<limit{
  if limit-off<8{return Err(Error::InvalidDirectoryEntry)}
  let ino=u32at(old_leaf,off)?;let raw=u16at(old_leaf,off+4)?;let r=rec_len(raw,old_leaf.len());let n=old_leaf[off+6] as usize;
  if r<8||r&3!=0||off.checked_add(r).ok_or(Error::OutOfRange)?>limit||n>r-8{return Err(Error::InvalidDirectoryEntry)}
  if ino!=0&&n!=0{
   if count>=map.len(){return Err(Error::TransactionTooLarge)}
   let h=hash_directory_name(version,hash_seed,&old_leaf[off+8..off+8+n],siphash_key)?;
   map[count]=DirectoryMapEntry{offset:off as u32,record_len:r as u32,major:h.major,minor:h.minor};count+=1;
  }
  off+=r;
 }
 if off!=limit||count<2{return Err(Error::InvalidDirectoryEntry)}
 source_copy.copy_from_slice(old_leaf);
 map[..count].sort_unstable_by(|a,b|a.major.cmp(&b.major).then(a.minor.cmp(&b.minor)).then(a.offset.cmp(&b.offset)));
 let mut total=0usize;for e in map.iter().take(count){let n=source_copy[e.offset as usize+6] as usize;total=total.checked_add(ideal(n)?).ok_or(Error::OutOfRange)?;}
 if total>limit.checked_mul(2).ok_or(Error::OutOfRange)?{return Err(Error::OutOfRange)}
 let mut accumulated=0usize;let mut split=1usize;
 for i in 0..count-1{let n=source_copy[map[i].offset as usize+6] as usize;accumulated+=ideal(n)?;split=i+1;if accumulated>=total/2{break}}
 let mut left_need=0usize;for e in map.iter().take(split){left_need+=ideal(source_copy[e.offset as usize+6] as usize)?;}
 let right_need=total-left_need;if left_need>limit||right_need>limit{return Err(Error::OutOfRange)}
 fn rebuild(src:&[u8],dst:&mut[u8],entries:&[DirectoryMapEntry],limit:usize)->Result<(),Error>{
  dst.fill(0);let mut out=0usize;
  for (i,e) in entries.iter().enumerate(){let at=e.offset as usize;let n=src[at+6] as usize;let need=ideal(n)?;let remain=limit-out;let r=if i+1==entries.len(){remain}else{need};if r<need||r>remain{return Err(Error::OutOfRange)}
   dst[out..out+8+n].copy_from_slice(&src[at..at+8+n]);put16(dst,out+4,rec_disk(r,dst.len())?)?;out+=r;
  }Ok(())
 }
 rebuild(source_copy,old_leaf,&map[..split],limit)?;rebuild(source_copy,new_leaf,&map[split..count],limit)?;
 if csum{write_leaf_tail(old_leaf,seed)?;write_leaf_tail(new_leaf,seed)?;}
 let mut separator=map[split].major;if map[split-1].major==separator{separator|=1;}Ok(separator)
}

/// Insert a separator after the last entry not greater than `hash`. Returns
/// OutOfRange when the node is full; callers must split the node and retry in
/// the appropriate half rather than dropping an index entry.
pub fn insert_dx_entry(block:&mut[u8],count_offset:usize,hash:u32,child:u32,metadata_csum:bool,inode_seed:u32)->Result<(),Error>{
 if child==0||child>0x0fff_ffff{return Err(Error::OutOfRange)}
 let(limit,count)=dx_limit(block,count_offset,metadata_csum)?;
 if metadata_csum{let stored=u32at(block,count_offset+limit*8+4)?;if dx_checksum(block,count_offset,limit,count,inode_seed)?!=stored{return Err(Error::InvalidChecksum)}}
 if count>=limit{return Err(Error::OutOfRange)}
 let mut at=1usize;
 while at<count{let h=u32at(block,count_offset+at*8)?;if h>hash{break}at+=1;}
 let start=count_offset+at*8;let end=count_offset+count*8;
 block.copy_within(start..end,start+8);put32(block,start,hash)?;put32(block,start+4,child)?;put16(block,count_offset+2,(count+1) as u16)?;
 update_dx(block,count_offset,limit,count+1,metadata_csum,inode_seed)
}

/// Split a full interior dx_node into two valid nodes. The caller inserts the
/// returned separator/new-node block into the parent in the same transaction.
/// This primitive deliberately does not mutate the parent; a full parent must
/// itself be split recursively before committing.
pub fn split_dx_node(node:&mut[u8],new_node:&mut[u8],source_copy:&mut[u8],metadata_csum:bool,inode_seed:u32)->Result<(u32,u32),Error>{
 const OFF:usize=8;
 if node.len()!=new_node.len()||node.len()!=source_copy.len()||node.len()<1024{return Err(Error::InvalidGeometry)}
 let(limit,count)=dx_limit(node,OFF,metadata_csum)?;
 if metadata_csum{let stored=u32at(node,OFF+limit*8+4)?;if dx_checksum(node,OFF,limit,count,inode_seed)?!=stored{return Err(Error::InvalidChecksum)}}
 if count<2{return Err(Error::InvalidDirectoryEntry)}
 source_copy.copy_from_slice(node);
 let split=count/2;let separator=u32at(source_copy,OFF+split*8)?;
 let new_count=count-split;
 node.fill(0);new_node.fill(0);
 node[..8].copy_from_slice(&source_copy[..8]);new_node[..8].copy_from_slice(&source_copy[..8]);
 put16(node,OFF,limit as u16)?;put16(node,OFF+2,split as u16)?;
 put16(new_node,OFF,limit as u16)?;put16(new_node,OFF+2,new_count as u16)?;
 node[OFF+4..OFF+8].copy_from_slice(&source_copy[OFF+4..OFF+8]);
 for i in 1..split{let s=OFF+i*8;node[s..s+8].copy_from_slice(&source_copy[s..s+8]);}
 let first_src=OFF+split*8;
 new_node[OFF+4..OFF+8].copy_from_slice(&source_copy[first_src+4..first_src+8]);
 for i in 1..new_count{let s=first_src+i*8;let d=OFF+i*8;new_node[d..d+8].copy_from_slice(&source_copy[s..s+8]);}
 update_dx(node,OFF,limit,split,metadata_csum,inode_seed)?;
 update_dx(new_node,OFF,limit,new_count,metadata_csum,inode_seed)?;
 Ok((separator,new_count as u32))
}

#[cfg(test)]
mod tests{
 use super::*;
 #[test]fn dx_insert_preserves_sorted_separators_and_checksum(){
  let mut b=[0u8;1024];let off=8;let limit=(1024-8-8)/8;
  put16(&mut b,off,limit as u16).unwrap();put16(&mut b,off+2,2).unwrap();
  put32(&mut b,off+4,2).unwrap();put32(&mut b,off+8,0x4000).unwrap();put32(&mut b,off+12,3).unwrap();
  let seed=0x12345678;update_dx(&mut b,off,limit,2,true,seed).unwrap();
  insert_dx_entry(&mut b,off,0x2000,4,true,seed).unwrap();
  assert_eq!(u16at(&b,off+2).unwrap(),3);assert_eq!(u32at(&b,off+8).unwrap(),0x2000);
  assert_eq!(dx_checksum(&b,off,limit,3,seed).unwrap(),u32at(&b,off+limit*8+4).unwrap());
 }
 #[test]fn dx_node_split_keeps_implicit_first_hash_and_checksums(){
  let mut a=[0u8;1024];let mut b=[0u8;1024];let mut scratch=[0u8;1024];let off=8;let limit=(1024-16)/8;
  put32(&mut a,0,0).unwrap();put16(&mut a,4,1024).unwrap();put16(&mut a,off,limit as u16).unwrap();put16(&mut a,off+2,4).unwrap();
  for i in 0..4{put32(&mut a,off+i*8+4,(i+2) as u32).unwrap();if i>0{put32(&mut a,off+i*8,(i as u32)*0x1000).unwrap();}}
  let seed=77;update_dx(&mut a,off,limit,4,true,seed).unwrap();
  let (separator,_)=split_dx_node(&mut a,&mut b,&mut scratch,true,seed).unwrap();
  assert_eq!(separator,0x2000);assert_eq!(u16at(&a,off+2).unwrap(),2);assert_eq!(u16at(&b,off+2).unwrap(),2);
  assert_eq!(dx_checksum(&a,off,limit,2,seed).unwrap(),u32at(&a,off+limit*8+4).unwrap());
  assert_eq!(dx_checksum(&b,off,limit,2,seed).unwrap(),u32at(&b,off+limit*8+4).unwrap());
 }
}
