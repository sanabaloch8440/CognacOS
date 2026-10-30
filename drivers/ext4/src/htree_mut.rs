//! Bounded HTree split and index-entry mutation primitives.
//! These operate on caller-owned buffers only; callers must stage every
//! changed leaf, index block, inode and allocation bitmap in one JBD2 commit.
use crate::{checksum::crc32c_update, hash::hash_directory_name, Error};

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
 let version=crate::hash::effective_hash_version(version,crate::hash::ext4_hash_unsigned(sb)?)?;
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

/// Expand a full depth-zero root into a single-child root plus a new dx_node.
/// The caller allocates new_node_logical and journals both blocks atomically.
pub fn grow_dx_root(root:&mut[u8],new_node:&mut[u8],new_node_logical:u32,metadata_csum:bool,inode_seed:u32)->Result<(),Error>{
 const RO:usize=32;const NO:usize=8;
 if root.len()!=new_node.len()||root.len()<1024||new_node_logical==0||new_node_logical>0x0fff_ffff{return Err(Error::InvalidGeometry)}
 if root[30]!=0{return Err(Error::UnsupportedFormat)}
 if u32at(root,0)?==0||u16at(root,4)?!=12||root[6]!=1||root[8]!=b'.'||u32at(root,12)?==0||rec_len(u16at(root,16)?,root.len())!=root.len()-12||root[18]!=2||&root[20..22]!=b".."||u32at(root,24)?!=0||root[29]!=8||root[31]!=0{return Err(Error::InvalidDirectoryEntry)}
 if root[28]!=0&&root[28]!=1&&root[28]!=2&&root[28]!=6{return Err(Error::UnsupportedFormat)}
 let(limit,count)=dx_limit(root,RO,metadata_csum)?;
 if metadata_csum{let stored=u32at(root,RO+limit*8+4)?;if dx_checksum(root,RO,limit,count,inode_seed)?!=stored{return Err(Error::InvalidChecksum)}}
 let node_limit=root.len().checked_sub(NO+if metadata_csum{8}else{0}).ok_or(Error::InvalidGeometry)?/8;
 if count>node_limit||node_limit>u16::MAX as usize{return Err(Error::OutOfRange)}
 new_node.fill(0);put32(new_node,0,0)?;put16(new_node,4,rec_disk(new_node.len(),new_node.len())?)?;new_node[6]=0;new_node[7]=0;
 put16(new_node,NO,node_limit as u16)?;put16(new_node,NO+2,count as u16)?;
 new_node[NO+4..NO+8].copy_from_slice(&root[RO+4..RO+8]);
 for i in 1..count{let s=RO+i*8;let d=NO+i*8;new_node[d..d+8].copy_from_slice(&root[s..s+8]);}
 update_dx(new_node,NO,node_limit,count,metadata_csum,inode_seed)?;
 root[30]=1;root[RO..RO+limit*8].fill(0);
 put16(root,RO,limit as u16)?;put16(root,RO+2,1)?;put32(root,RO+4,new_node_logical)?;
 update_dx(root,RO,limit,1,metadata_csum,inode_seed)
}

/// Insert a dirent into a checked leaf using ext4's 64-KiB record-length
/// encoding. The caller has already checked the directory's other HTree leaves.
fn insert_leaf_dirent(sb:&[u8],inode:u32,generation:u32,block:&mut[u8],total_inodes:u32,
 child:u32,name:&[u8],file_type:u8)->Result<(),Error>{
 if child==0||child>total_inodes||name.is_empty()||name.len()>255||name.iter().any(|b|*b==0||*b==b'/'){return Err(Error::InvalidDirectoryEntry)}
 crate::validate_superblock_checksum(sb)?;let csum=has_csum(sb)?;
 if csum{crate::validate_directory_block_checksum(sb,inode,generation,block)?;}
 let limit=block.len()-if csum{12}else{0};let need=ideal(name.len())?;let mut off=0usize;let mut placed=false;
 while off<limit{
  if limit-off<8{return Err(Error::InvalidDirectoryEntry)}
  let ino=u32at(block,off)?;let rec=rec_len(u16at(block,off+4)?,block.len());let n=block[off+6] as usize;
  if rec<8||rec&3!=0||off.checked_add(rec).ok_or(Error::OutOfRange)?>limit||n>rec-8{return Err(Error::InvalidDirectoryEntry)}
  if ino!=0&&n==name.len()&&&block[off+8..off+8+n]==name{return Err(Error::InvalidDirectoryEntry)}
  if ino==0&&rec>=need{
   let used=if rec-need>=8{need}else{rec};put32(block,off,child)?;put16(block,off+4,rec_disk(used,block.len())?)?;block[off+6]=name.len()as u8;block[off+7]=file_type;block[off+8..off+8+name.len()].copy_from_slice(name);
   if rec>used{let free=off+used;put32(block,free,0)?;put16(block,free+4,rec_disk(rec-used,block.len())?)?;block[free+6]=0;block[free+7]=0;}placed=true;break;
  }
  if ino!=0{let used=ideal(n)?;if rec>=used&&rec-used>=need{
   put16(block,off+4,rec_disk(used,block.len())?)?;let new=off+used;let leftover=rec-used;let used_new=if leftover-need>=8{need}else{leftover};
   put32(block,new,child)?;put16(block,new+4,rec_disk(used_new,block.len())?)?;block[new+6]=name.len()as u8;block[new+7]=file_type;block[new+8..new+8+name.len()].copy_from_slice(name);
   if leftover>used_new{let free=new+used_new;put32(block,free,0)?;put16(block,free+4,rec_disk(leftover-used_new,block.len())?)?;block[free+6]=0;block[free+7]=0;}placed=true;break;
  }}
  off+=rec;
 }
 if !placed{return Err(Error::OutOfRange)}
 if csum{crate::update_directory_block_checksum(sb,inode,generation,block)?;}Ok(())
}

/// Split the selected full HTree leaf, insert a new name into the correct half,
/// and add the new leaf separator to its parent index block. The caller must
/// stage old_leaf, new_leaf and parent_index together in one JBD2 transaction.
/// If this function errors after the split, discard all three staged buffers.
pub fn split_and_insert_htree_entry<R:crate::htree::DirectoryBlockReader>(
 sb:&[u8],directory_inode:u32,generation:u32,root:&[u8],reader:&mut R,
 directory_blocks:u32,total_inodes:u32,old_leaf_logical:u32,old_leaf:&mut[u8],
 new_leaf_logical:u32,new_leaf:&mut[u8],parent_index_logical:u32,parent_index:&mut[u8],
 parent_count_offset:usize,child_inode:u32,name:&[u8],file_type:u8,
 siphash_key:Option<[u64;2]>,map:&mut[DirectoryMapEntry],source_copy:&mut[u8],
 scratch:&mut[u8],
)->Result<u32,Error>{
 if new_leaf_logical==0||new_leaf_logical==old_leaf_logical||new_leaf_logical>0x0fff_ffff{return Err(Error::OutOfRange)}
 let csum=has_csum(sb)?;let inode_seed=if csum{crate::ext4_inode_checksum_seed(sb,directory_inode,generation)?}else{0};
 let hash_seed=crate::hash::ext4_directory_hash_seed(sb)?;
 let unsigned=crate::hash::ext4_hash_unsigned(sb)?;
 let version=crate::hash::effective_hash_version(*root.get(28).ok_or(Error::Truncated)?,unsigned)?;
 let hash=hash_directory_name(version,hash_seed,name,siphash_key)?;
 match crate::htree::lookup_htree_name_from_superblock(reader,sb,directory_inode,generation,root,
  directory_blocks,total_inodes,name,siphash_key,scratch){
  Ok(_) => return Err(Error::InvalidDirectoryEntry),
  Err(Error::OutOfRange) => {},
  Err(e) => return Err(e),
 }
 let path=crate::htree::lookup_htree_path(reader,root,directory_blocks,hash.major,csum,inode_seed,scratch)?;
 if path.leaf_block!=old_leaf_logical{return Err(Error::InvalidGeometry)}
 let parent_level=path.depth as usize;
 let expected_parent=if parent_level==0{0}else{path.blocks[parent_level-1]};
 let expected_offset=if parent_level==0{32}else{8};
 if parent_index_logical!=expected_parent||parent_count_offset!=expected_offset||parent_index.len()!=root.len(){return Err(Error::InvalidGeometry)}
 let(limit,count)=dx_limit(parent_index,parent_count_offset,csum)?;
 if csum{let stored=u32at(parent_index,parent_count_offset+limit*8+4)?;if dx_checksum(parent_index,parent_count_offset,limit,count,inode_seed)?!=stored{return Err(Error::InvalidChecksum)}}
 if count>=limit{return Err(Error::OutOfRange)}
 let separator=split_directory_leaf(sb,directory_inode,generation,old_leaf,new_leaf,
  *root.get(28).ok_or(Error::Truncated)?,hash_seed,siphash_key,map,source_copy)?;
 let (target,target_logical)=if hash.major>=separator{(new_leaf,new_leaf_logical)}else{(old_leaf,old_leaf_logical)};
 insert_leaf_dirent(sb,directory_inode,generation,target,total_inodes,child_inode,name,file_type)?;
 insert_dx_entry(parent_index,parent_count_offset,separator,new_leaf_logical,csum,inode_seed)?;
 Ok(target_logical)
}

/// Split a full interior dx_node into two valid nodes. The caller inserts the
/// returned separator/new-node block into the parent in the same transaction.
/// This primitive deliberately does not mutate the parent; a full parent must
/// itself be split recursively before committing.
pub fn split_dx_node(node:&mut[u8],new_node:&mut[u8],source_copy:&mut[u8],metadata_csum:bool,inode_seed:u32)->Result<(u32,u32),Error>{
 const OFF:usize=8;
 if node.len()!=new_node.len()||node.len()!=source_copy.len()||node.len()<1024{return Err(Error::InvalidGeometry)}
 if u32at(node,0)?!=0||rec_len(u16at(node,4)?,node.len())!=node.len()||node[6]!=0||node[7]!=0{return Err(Error::InvalidDirectoryEntry)}
 let(limit,count)=dx_limit(node,OFF,metadata_csum)?;
 if metadata_csum{let stored=u32at(node,OFF+limit*8+4)?;if dx_checksum(node,OFF,limit,count,inode_seed)?!=stored{return Err(Error::InvalidChecksum)}}
 if count<2{return Err(Error::InvalidDirectoryEntry)}
 source_copy.copy_from_slice(node);
 let split=count/2;let mut separator=u32at(source_copy,OFF+split*8)?;let previous=if split==1{0}else{u32at(source_copy,OFF+(split-1)*8)?};if (previous&!1)==(separator&!1){separator|=1;}
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
 #[test]fn collision_lookup_crosses_leaf_and_readdir_enumerates_all_indexed_leaves(){
  struct Reader{blocks:[[u8;1024];4]}
  impl crate::htree::DirectoryBlockReader for Reader{fn read_directory_block(&mut self,n:u32,out:&mut[u8])->Result<(),Error>{let src=self.blocks.get(n as usize).ok_or(Error::OutOfRange)?;if out.len()!=1024{return Err(Error::InvalidGeometry)}out.copy_from_slice(src);Ok(())}}
  let mut sb=[0u8;1024];put32(&mut sb,100,0x0400).unwrap();sb[373]=1;sb[104..120].copy_from_slice(&[0x22;16]);
  let hash_seed=[0x1111_1111,0x2222_2222,0x3333_3333,0x4444_4444];
  for(i,v)in hash_seed.iter().enumerate(){put32(&mut sb,236+i*4,*v).unwrap();}
  put32(&mut sb,352,2).unwrap();crate::update_superblock_checksum(&mut sb).unwrap();
  let fs_seed=crate::ext4_metadata_checksum_seed(&sb).unwrap();let inode_seed=crate::checksum::ext4_inode_csum_seed(fs_seed,2,9);
  let wanted=b"collision-target";let h=hash_directory_name(4,hash_seed,wanted,None).unwrap();let mut root=[0u8;1024];
  put32(&mut root,0,2).unwrap();put16(&mut root,4,12).unwrap();root[6]=1;root[8]=b'.';
  put32(&mut root,12,2).unwrap();put16(&mut root,16,1012).unwrap();root[18]=2;root[20..22].copy_from_slice(b"..");
  root[28]=1;root[29]=8;let limit=(1024-40)/8;put16(&mut root,32,limit as u16).unwrap();put16(&mut root,34,2).unwrap();
  put32(&mut root,36,2).unwrap();put32(&mut root,40,h.major|1).unwrap();put32(&mut root,44,3).unwrap();update_dx(&mut root,32,limit,2,true,inode_seed).unwrap();
  let mut reader=Reader{blocks:[[0;1024];4]};
  for(n,ino,name)in[(2,21,b"other-entry".as_slice()),(3,22,wanted.as_slice())]{let b=&mut reader.blocks[n];put32(b,0,ino).unwrap();put16(b,4,1012).unwrap();b[6]=name.len()as u8;b[7]=1;b[8..8+name.len()].copy_from_slice(name);write_leaf_tail(b,inode_seed).unwrap();}
  let mut scratch=[0u8;1024];
  assert_eq!(crate::ops::lookup_indexed_directory_entry(&sb,2,9,&root,&mut reader,4,100,wanted,None,&mut scratch),Ok(22));
  let mut seen=0;crate::ops::iterate_indexed_directory(&sb,2,9,&root,&mut reader,4,100,None,&mut scratch,|entry|{seen+=1;assert!(entry.inode==2||entry.inode==21||entry.inode==22);Ok(true)}).unwrap();assert_eq!(seen,4);
  let mut map=[crate::htree::DirectoryHashMapEntry::default();4];let mut previous=None;let mut sorted_seen=0;
  crate::ops::iterate_indexed_directory_sorted(&sb,2,9,&root,&mut reader,4,100,None,&mut scratch,&mut map,|entry|{
   let key=(entry.hash.major,entry.hash.minor);if let Some(p)=previous{assert!(p<=key);}previous=Some(key);sorted_seen+=1;Ok(true)
  }).unwrap();assert_eq!(sorted_seen,4);
 }

 #[test]fn leaf_split_rebalances_entries_and_refreshes_both_checksums(){
  let mut sb=[0u8;1024];put32(&mut sb,100,0x0400).unwrap();sb[373]=1;sb[104..120].copy_from_slice(&[0x22;16]);crate::update_superblock_checksum(&mut sb).unwrap();
  let fs_seed=crate::ext4_metadata_checksum_seed(&sb).unwrap();let seed=crate::checksum::ext4_inode_csum_seed(fs_seed,12,7);
  let mut old=[0u8;1024];put32(&mut old,0,20).unwrap();put16(&mut old,4,512).unwrap();old[6]=5;old[7]=1;old[8..13].copy_from_slice(b"alpha");
  put32(&mut old,512,21).unwrap();put16(&mut old,516,500).unwrap();old[518]=5;old[519]=1;old[520..525].copy_from_slice(b"bravo");
  write_leaf_tail(&mut old,seed).unwrap();crate::update_directory_block_checksum(&sb,12,7,&mut old).unwrap();
  let mut new=[0u8;1024];let mut scratch=[0u8;1024];let mut map=[DirectoryMapEntry::default();8];
  let separator=split_directory_leaf(&sb,12,7,&mut old,&mut new,1,[0;4],None,&mut map,&mut scratch).unwrap();
  crate::validate_directory_block_checksum(&sb,12,7,&old).unwrap();crate::validate_directory_block_checksum(&sb,12,7,&new).unwrap();
  assert!(separator&!1<=0xffff_fffc);assert_ne!(u32at(&old,0).unwrap(),0);assert_ne!(u32at(&new,0).unwrap(),0);
  assert_eq!(old[6] as usize+new[6] as usize,10);
 }
 #[test]fn root_growth_moves_index_entries_to_checked_dx_node(){
  let seed=0x4455_6677;let mut root=[0u8;1024];put32(&mut root,0,2).unwrap();put16(&mut root,4,12).unwrap();root[6]=1;root[8]=b'.';
  put32(&mut root,12,2).unwrap();put16(&mut root,16,1012).unwrap();root[18]=2;root[20..22].copy_from_slice(b"..");root[28]=1;root[29]=8;
  let limit=(1024-40)/8;put16(&mut root,32,limit as u16).unwrap();put16(&mut root,34,2).unwrap();put32(&mut root,36,2).unwrap();put32(&mut root,40,0x4000).unwrap();put32(&mut root,44,3).unwrap();
  update_dx(&mut root,32,limit,2,true,seed).unwrap();let mut node=[0u8;1024];
  grow_dx_root(&mut root,&mut node,4,true,seed).unwrap();
  assert_eq!(root[30],1);assert_eq!(u16at(&root,34).unwrap(),1);assert_eq!(u32at(&root,36).unwrap(),4);
  assert_eq!(u32at(&node,0).unwrap(),0);assert_eq!(u16at(&node,4).unwrap(),1024);assert_eq!(u16at(&node,10).unwrap(),2);
  let node_limit=(1024-16)/8;assert_eq!(dx_checksum(&node,8,node_limit,2,seed).unwrap(),u32at(&node,8+node_limit*8+4).unwrap());
 }

 #[test]fn collision_lookup_climbs_parent_and_enters_sibling_dx_node(){
  struct Reader{blocks:[[u8;1024];6]}
  impl crate::htree::DirectoryBlockReader for Reader{fn read_directory_block(&mut self,n:u32,out:&mut[u8])->Result<(),Error>{let src=self.blocks.get(n as usize).ok_or(Error::OutOfRange)?;if out.len()!=1024{return Err(Error::InvalidGeometry)}out.copy_from_slice(src);Ok(())}}
  let seed=crate::checksum::ext4_inode_csum_seed(0x3344_5566,2,4);
  let hs=[9u32,8,7,6];let wanted=b"sibling-node-hit";let h=hash_directory_name(4,hs,wanted,None).unwrap();
  let mut root=[0u8;1024];put32(&mut root,0,2).unwrap();put16(&mut root,4,12).unwrap();root[6]=1;root[8]=b'.';
  put32(&mut root,12,2).unwrap();put16(&mut root,16,1012).unwrap();root[18]=2;root[20..22].copy_from_slice(b"..");
  root[28]=1;root[29]=8;root[30]=1;let rl=(1024-40)/8;put16(&mut root,32,rl as u16).unwrap();put16(&mut root,34,2).unwrap();
  put32(&mut root,36,1).unwrap();put32(&mut root,40,h.major|1).unwrap();put32(&mut root,44,2).unwrap();update_dx(&mut root,32,rl,2,true,seed).unwrap();
  let mut reader=Reader{blocks:[[0;1024];6]};
  for (node,leaf) in [(1u32,3u32),(2u32,4u32)]{let b=&mut reader.blocks[node as usize];put32(b,0,0).unwrap();put16(b,4,1024).unwrap();let lim=(1024-16)/8;put16(b,8,lim as u16).unwrap();put16(b,10,1).unwrap();put32(b,12,leaf).unwrap();update_dx(b,8,lim,1,true,seed).unwrap();}
  for (block,ino,name) in [(3usize,21u32,b"other".as_slice()),(4usize,22u32,wanted.as_slice())]{let b=&mut reader.blocks[block];put32(b,0,ino).unwrap();put16(b,4,1012).unwrap();b[6]=name.len()as u8;b[7]=1;b[8..8+name.len()].copy_from_slice(name);write_leaf_tail(b,seed).unwrap();}
  let mut scratch=[0u8;1024];assert_eq!(crate::htree::lookup_htree_name(&mut reader,&root,6,100,wanted,hs,None,true,true,seed,&mut scratch),Ok(22));
 }

}
