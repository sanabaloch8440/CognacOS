//! Bounded ext4 mutation primitives. These routines update in-memory metadata
//! buffers and checksums; callers must commit the resulting buffers through
//! the JBD2 transaction layer before exposing a mutation as durable.
use super::{Error,Superblock,update_block_bitmap_checksum,update_inode_bitmap_checksum,
 update_group_descriptor_checksum,update_superblock_checksum,validate_superblock_checksum,validate_group_descriptor_checksum,
 validate_block_bitmap_checksum,validate_inode_bitmap_checksum,validate_directory_block_checksum,
 update_directory_block_checksum,validate_inode_checksum,update_inode_checksum};

fn le16(b:&[u8],o:usize)->Result<u16,Error>{let s=b.get(o..o+2).ok_or(Error::Truncated)?;Ok(u16::from_le_bytes([s[0],s[1]]))}
fn le32(b:&[u8],o:usize)->Result<u32,Error>{let s=b.get(o..o+4).ok_or(Error::Truncated)?;Ok(u32::from_le_bytes([s[0],s[1],s[2],s[3]]))}
fn put16(b:&mut[u8],o:usize,v:u16)->Result<(),Error>{b.get_mut(o..o+2).ok_or(Error::Truncated)?.copy_from_slice(&v.to_le_bytes());Ok(())}
fn put32(b:&mut[u8],o:usize,v:u32)->Result<(),Error>{b.get_mut(o..o+4).ok_or(Error::Truncated)?.copy_from_slice(&v.to_le_bytes());Ok(())}
fn metadata_csum(sb:&[u8])->Result<bool,Error>{Ok(le32(sb,100)?&0x0400!=0)}
fn find_bit(bitmap:&[u8],valid:u32,goal:u32)->Result<u32,Error>{
 if valid==0||valid as usize>bitmap.len().saturating_mul(8){return Err(Error::InvalidGeometry)}
 for n in 0..valid{let bit=(goal.wrapping_add(n))%valid;if bitmap[(bit/8) as usize]&(1<<(bit%8))==0{return Ok(bit)}}
 Err(Error::OutOfRange)
}
fn set_bit(bitmap:&mut[u8],bit:u32,set:bool)->Result<(),Error>{
 let byte=bitmap.get_mut((bit/8) as usize).ok_or(Error::OutOfRange)?;let mask=1u8<<(bit%8);
 if set{if *byte&mask!=0{return Err(Error::InvalidGeometry)}*byte|=mask}else{if *byte&mask==0{return Err(Error::InvalidGeometry)}*byte&=!mask}
 Ok(())
}
fn group_counter(desc:&[u8],low:usize,high:usize)->Result<u32,Error>{
 let lo=le16(desc,low)? as u32;if desc.len()>=64{Ok(lo|((le16(desc,high)? as u32)<<16))}else{Ok(lo)}
}
fn set_group_counter(desc:&mut[u8],low:usize,high:usize,value:u32)->Result<(),Error>{
 put16(desc,low,value as u16)?;if desc.len()>=64{put16(desc,high,(value>>16) as u16)?}else if value>u16::MAX as u32{return Err(Error::OutOfRange)}Ok(())
}
fn super_free_blocks(sb:&[u8])->Result<u64,Error>{
 let lo=le32(sb,12)? as u64;if le32(sb,96)?&super::INCOMPAT_64BIT!=0{Ok(lo|((le32(sb,344)? as u64)<<32))}else{Ok(lo)}
}
fn set_super_free_blocks(sb:&mut[u8],v:u64)->Result<(),Error>{
 put32(sb,12,v as u32)?;if le32(sb,96)?&super::INCOMPAT_64BIT!=0{put32(sb,344,(v>>32) as u32)?}else if v>u32::MAX as u64{return Err(Error::OutOfRange)}Ok(())
}
fn finish_group(sb:&[u8],group:u32,desc:&mut[u8],bitmap:&[u8],inode_bitmap:bool)->Result<(),Error>{
 if metadata_csum(sb)?{
  if inode_bitmap{update_inode_bitmap_checksum(sb,desc,bitmap)?}else{update_block_bitmap_checksum(sb,desc,bitmap)?}
  update_group_descriptor_checksum(sb,group,desc)?;
 }Ok(())
}
fn finish_super(sb:&mut[u8])->Result<(),Error>{if metadata_csum(sb)?{update_superblock_checksum(sb)?}Ok(())}
fn validate_group_metadata(sb:&[u8],group:u32,desc:&[u8],bitmap:&[u8],inode_bitmap:bool)->Result<(),Error>{
 if metadata_csum(sb)?{
  validate_superblock_checksum(sb)?;
  validate_group_descriptor_checksum(sb,group,desc)?;
  if inode_bitmap{validate_inode_bitmap_checksum(sb,desc,bitmap)?}else{validate_block_bitmap_checksum(sb,desc,bitmap)?}
 }Ok(())
}

/// Allocate one free bit in a group bitmap (ext4 bit numbering is LSB-first).
pub fn allocate_bitmap_bit(bitmap:&mut[u8],valid_bits:u32,goal:u32)->Result<u32,Error>{
 let bit=find_bit(bitmap,valid_bits,goal)?;set_bit(bitmap,bit,true)?;Ok(bit)
}
/// Release one previously allocated bitmap bit.
pub fn release_bitmap_bit(bitmap:&mut[u8],valid_bits:u32,bit:u32)->Result<(),Error>{
 if bit>=valid_bits{return Err(Error::OutOfRange)}set_bit(bitmap,bit,false)
}

/// Allocate an inode in a group and update the group/superblock free counters,
/// bitmap checksum, descriptor checksum and superblock checksum.
pub fn allocate_inode(
 sb:&mut[u8],group:u32,desc:&mut[u8],bitmap:&mut[u8],goal:u32,
)->Result<u32,Error>{
 let parsed=Superblock::parse(sb)?;
 if group>=parsed.group_count()?{return Err(Error::OutOfRange)}
 validate_group_metadata(sb,group,desc,bitmap,true)?;
 let first=group.checked_mul(parsed.inodes_per_group).ok_or(Error::OutOfRange)?;
 let valid=core::cmp::min(parsed.inodes_per_group,parsed.inodes_count.saturating_sub(first));
 let reserved=if group==0{le32(sb,84)?.saturating_sub(1)}else{0};
 let start=core::cmp::max(goal,reserved);
 let bit=allocate_bitmap_bit(bitmap,valid,start)?;
 let count=group_counter(desc,14,46)?;if count==0{set_bit(bitmap,bit,false)?;return Err(Error::InvalidGeometry)}
 set_group_counter(desc,14,46,count-1)?;
 let free=le32(sb,16)?;if free==0{set_bit(bitmap,bit,false)?;set_group_counter(desc,14,46,count)?;return Err(Error::InvalidGeometry)}
 put32(sb,16,free-1)?;
 finish_group(sb,group,desc,bitmap,true)?;finish_super(sb)?;
 first.checked_add(bit).and_then(|v|v.checked_add(1)).ok_or(Error::OutOfRange)
}

/// Allocate a data block from a block group, updating 64-bit counters and
/// metadata checksums. Returns the absolute filesystem block number.
pub fn allocate_block(
 sb:&mut[u8],group:u32,desc:&mut[u8],bitmap:&mut[u8],goal:u32,
)->Result<u64,Error>{
 let parsed=Superblock::parse(sb)?;
 if group>=parsed.group_count()?{return Err(Error::OutOfRange)}
 validate_group_metadata(sb,group,desc,bitmap,false)?;
 let first=parsed.first_data_block as u64+(group as u64)*(parsed.blocks_per_group as u64);
 let valid=core::cmp::min(parsed.blocks_per_group as u64,parsed.blocks_count.saturating_sub(first)) as u32;
 let bit=allocate_bitmap_bit(bitmap,valid,goal)?;
 let count=group_counter(desc,12,44)?;let free=super_free_blocks(sb)?;
 if count==0||free==0{set_bit(bitmap,bit,false)?;return Err(Error::InvalidGeometry)}
 set_group_counter(desc,12,44,count-1)?;set_super_free_blocks(sb,free-1)?;
 finish_group(sb,group,desc,bitmap,false)?;finish_super(sb)?;
 first.checked_add(bit as u64).ok_or(Error::OutOfRange)
}

/// Free a non-reserved inode and refresh all affected counters/checksums.
pub fn free_inode(sb:&mut[u8],group:u32,inode_number:u32,desc:&mut[u8],bitmap:&mut[u8])->Result<(),Error>{
 let parsed=Superblock::parse(sb)?;if group>=parsed.group_count()?{return Err(Error::OutOfRange)}
 validate_group_metadata(sb,group,desc,bitmap,true)?;
 let first=group.checked_mul(parsed.inodes_per_group).ok_or(Error::OutOfRange)?;
 let bit=inode_number.checked_sub(first+1).ok_or(Error::OutOfRange)?;
 if group>=parsed.group_count()?||inode_number>parsed.inodes_count||bit>=parsed.inodes_per_group{return Err(Error::OutOfRange)}
 if group==0&&inode_number<le32(sb,84)?{return Err(Error::InvalidGeometry)}
 set_bit(bitmap,bit,false)?;
 let count=group_counter(desc,14,46)?.checked_add(1).ok_or(Error::OutOfRange)?;
 set_group_counter(desc,14,46,count)?;put32(sb,16,le32(sb,16)?.checked_add(1).ok_or(Error::OutOfRange)?)?;
 finish_group(sb,group,desc,bitmap,true)?;finish_super(sb)
}

/// Free a block after the caller has verified it is not filesystem metadata.
pub fn free_block(sb:&mut[u8],group:u32,block:u64,desc:&mut[u8],bitmap:&mut[u8])->Result<(),Error>{
 let parsed=Superblock::parse(sb)?;if group>=parsed.group_count()?{return Err(Error::OutOfRange)}
 validate_group_metadata(sb,group,desc,bitmap,false)?;
 let first=parsed.first_data_block as u64+(group as u64)*(parsed.blocks_per_group as u64);
 let bit=block.checked_sub(first).ok_or(Error::OutOfRange)?;if bit>=parsed.blocks_per_group as u64||block>=parsed.blocks_count{return Err(Error::OutOfRange)}
 set_bit(bitmap,bit as u32,false)?;
 let count=group_counter(desc,12,44)?.checked_add(1).ok_or(Error::OutOfRange)?;
 set_group_counter(desc,12,44,count)?;let new_free=super_free_blocks(sb)?.checked_add(1).ok_or(Error::OutOfRange)?;set_super_free_blocks(sb,new_free)?;
 finish_group(sb,group,desc,bitmap,false)?;finish_super(sb)
}

fn dir_limit(block:&[u8],has_tail:bool)->Result<usize,Error>{
 if has_tail{if block.len()<12{return Err(Error::Truncated)}Ok(block.len()-12)}else{Ok(block.len())}
}
fn ideal_rec_len(name_len:usize)->Result<usize,Error>{
 if name_len==0||name_len>255{return Err(Error::InvalidDirectoryEntry)}
 Ok((8usize.checked_add(name_len).ok_or(Error::OutOfRange)?+3)&!3)
}
fn validate_dir_entries(block:&[u8],limit:usize,total_inodes:u32)->Result<(),Error>{
 let mut off=0usize;
 while off<limit{
  if limit-off<8{return Err(Error::InvalidDirectoryEntry)}
  let ino=le32(block,off)?;let rec=le16(block,off+4)? as usize;let name=block[off+6] as usize;
  if rec<8||rec&3!=0||rec>limit-off||name>rec-8||(ino!=0&&ino>total_inodes){return Err(Error::InvalidDirectoryEntry)}
  if block[off+8..off+8+name].iter().any(|c|*c==0||*c==b'/'){return Err(Error::InvalidDirectoryEntry)}
  off+=rec;
 }Ok(())
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct NewInodeMetadata{
 pub mode:u16,pub uid:u32,pub gid:u32,pub links:u16,pub generation:u32,
 pub atime:u32,pub ctime:u32,pub mtime:u32,
}
/// Initialize a newly allocated regular-file inode. Bitmap allocation and
/// parent-directory insertion are separate buffers and must be committed in
/// the same JBD2 transaction by the caller.
pub fn initialize_regular_inode(
 sb:&[u8],inode_number:u32,inode:&mut[u8],meta:NewInodeMetadata,
)->Result<(),Error>{
 if inode.len()<128||inode_number==0||meta.links==0||meta.mode&0xf000!=0x8000{return Err(Error::InvalidGeometry)}
 validate_superblock_checksum(sb)?;
 if inode.len()<Superblock::parse(sb)?.inode_size as usize{return Err(Error::Truncated)}
 inode.fill(0);
 put16(inode,0,meta.mode)?;put32(inode,8,meta.atime)?;put32(inode,12,meta.ctime)?;put32(inode,16,meta.mtime)?;
 put16(inode,26,meta.links)?;put32(inode,100,meta.generation)?;
 put16(inode,2,meta.uid as u16)?;put16(inode,24,meta.gid as u16)?;
 if inode.len()>=124{put16(inode,120,(meta.uid>>16) as u16)?;put16(inode,122,(meta.gid>>16) as u16)?}
 if inode.len()>=130{let extra=core::cmp::min(inode.len()-128,32)&!3;put16(inode,128,extra as u16)?}
 if metadata_csum(sb)?{update_inode_checksum(sb,inode_number,inode)?}
 Ok(())
}

/// Initialize a new directory inode with one inline extent for its first
/// data block. The block must already be allocated in the group's block bitmap.
pub fn initialize_directory_inode(
 sb:&[u8],inode_number:u32,inode:&mut[u8],meta:NewInodeMetadata,
 first_block:u64,
)->Result<(),Error>{
 let parsed=Superblock::parse(sb)?;
 if inode.len()<parsed.inode_size as usize||inode_number==0||meta.links<2
  ||meta.mode&0xf000!=0x4000||!parsed.has_extents()||first_block==0||first_block>=parsed.blocks_count{
  return Err(Error::InvalidGeometry)
 }
 validate_superblock_checksum(sb)?;
 inode.fill(0);put16(inode,0,meta.mode)?;
 put32(inode,8,meta.atime)?;put32(inode,12,meta.ctime)?;put32(inode,16,meta.mtime)?;
 put16(inode,26,meta.links)?;put32(inode,28,parsed.block_size/512)?;
 put32(inode,32,0x0008_0000)?;put32(inode,100,meta.generation)?;
 put16(inode,2,meta.uid as u16)?;put16(inode,24,meta.gid as u16)?;
 if inode.len()>=124{put16(inode,120,(meta.uid>>16) as u16)?;put16(inode,122,(meta.gid>>16) as u16)?}
 if inode.len()>=130{let extra=core::cmp::min(inode.len()-128,32)&!3;put16(inode,128,extra as u16)?}
 put32(inode,4,parsed.block_size)?;
 if inode.len()>=112{put32(inode,108,0)?}
 inode[40..100].fill(0);put16(inode,40,0xf30a)?;put16(inode,44,4)?;
 insert_inline_extent(&mut inode[40..100],0,first_block,1)?;
 if metadata_csum(sb)?{update_inode_checksum(sb,inode_number,inode)?}
 Ok(())
}

/// Construct the first directory block containing "." and "..", including
/// the ext4 metadata checksum tail when the feature is enabled.
pub fn initialize_directory_block(
 sb:&[u8],directory_inode:u32,parent_inode:u32,generation:u32,block:&mut[u8],
)->Result<(),Error>{
 let parsed=Superblock::parse(sb)?;
 if block.len()!=parsed.block_size as usize||directory_inode==0||parent_inode==0
  ||directory_inode>parsed.inodes_count||parent_inode>parsed.inodes_count{return Err(Error::InvalidGeometry)}
 validate_superblock_checksum(sb)?;
 let csum=metadata_csum(sb)?;let limit=dir_limit(block,csum)?;
 if limit<24{return Err(Error::InvalidGeometry)}
 block.fill(0);
 put32(block,0,directory_inode)?;put16(block,4,12)?;block[6]=1;block[7]=2;block[8]=b'.';
 put32(block,12,parent_inode)?;put16(block,16,(limit-12) as u16)?;block[18]=2;block[19]=2;block[20]=b'.';block[21]=b'.';
 if csum{
  let tail=limit;put32(block,tail,0)?;put16(block,tail+4,12)?;block[tail+6]=0;block[tail+7]=0xde;
  update_directory_block_checksum(sb,directory_inode,generation,block)?;
 }
 Ok(())
}

/// Create a regular file's inode and parent-directory entry in caller-owned
/// metadata buffers. The inode bitmap allocation and all changed blocks must
/// be included in the same JBD2 commit.
pub fn create_regular_file_entry(
 sb:&[u8],parent_inode:u32,parent_generation:u32,parent_directory:&mut[u8],
 inode_number:u32,inode:&mut[u8],meta:NewInodeMetadata,total_inodes:u32,name:&[u8],
)->Result<(),Error>{
 if meta.mode&0xf000!=0x8000{return Err(Error::InvalidGeometry)}
 initialize_regular_inode(sb,inode_number,inode,meta)?;
 insert_directory_entry(sb,parent_inode,parent_generation,parent_directory,total_inodes,inode_number,name,1)
}

/// Decrement a regular inode's link count and set deletion time when the last
/// link is removed. The caller frees its extents and inode bitmap in the same
/// transaction when reclaim_inode is true.
pub fn unlink_regular_inode(
 sb:&[u8],inode_number:u32,inode:&mut[u8],deletion_time:u32,
)->Result<bool,Error>{
 if inode.len()<128||le16(inode,0)?&0xf000!=0x8000{return Err(Error::UnsupportedFormat)}
 validate_inode_before_mutation(sb,inode_number,inode)?;
 let links=le16(inode,26)?;if links==0{return Err(Error::InvalidGeometry)}
 let remaining=links-1;put16(inode,26,remaining)?;
 if remaining==0{put32(inode,20,deletion_time)?}
 if metadata_csum(sb)?{update_inode_checksum(sb,inode_number,inode)?}
 Ok(remaining==0)
}

/// Remove a directory name and return its inode number. Link-count updates,
/// inode reclamation, and extent freeing must be staged in the same journal
/// transaction by the filesystem operation coordinator.
pub fn unlink_directory_name(
 sb:&[u8],directory_inode:u32,generation:u32,block:&mut[u8],total_inodes:u32,name:&[u8],
)->Result<u32,Error>{
 remove_directory_entry(sb,directory_inode,generation,block,total_inodes,name)
}

/// Insert a directory entry into existing slack/free space. If metadata_csum
/// is enabled, validate the old tail and refresh it after the mutation.
pub fn insert_directory_entry(
 sb:&[u8],directory_inode:u32,generation:u32,block:&mut[u8],total_inodes:u32,
 child_inode:u32,name:&[u8],file_type:u8,
)->Result<(),Error>{
 if child_inode==0||child_inode>total_inodes||name.is_empty()||name.len()>255{return Err(Error::InvalidDirectoryEntry)}
 validate_superblock_checksum(sb)?;
 validate_superblock_checksum(sb)?;
 let csum=metadata_csum(sb)?;if csum{validate_directory_block_checksum(sb,directory_inode,generation,block)?}
 let limit=dir_limit(block,csum)?;validate_dir_entries(block,limit,total_inodes)?;
 let need=ideal_rec_len(name.len())?;let mut off=0usize;let mut placed=false;
 while off<limit{
  let ino=le32(block,off)?;let rec=le16(block,off+4)? as usize;let n=block[off+6] as usize;
  if ino==0&&rec>=need{
   let use_len=if rec-need>=8{need}else{rec};
   put32(block,off,child_inode)?;put16(block,off+4,use_len as u16)?;block[off+6]=name.len() as u8;block[off+7]=file_type;
   block[off+8..off+8+name.len()].copy_from_slice(name);
   if rec>use_len{let free=off+use_len;put32(block,free,0)?;put16(block,free+4,(rec-use_len) as u16)?;block[free+6]=0;block[free+7]=0}
   placed=true;break
  }
  if ino!=0{
   let used=ideal_rec_len(n)?;if rec>=used&&rec-used>=need{
    put16(block,off+4,used as u16)?;
    let new=off+used;let leftover=rec-used;let use_len=if leftover-need>=8{need}else{leftover};
    put32(block,new,child_inode)?;put16(block,new+4,use_len as u16)?;block[new+6]=name.len() as u8;block[new+7]=file_type;
    block[new+8..new+8+name.len()].copy_from_slice(name);
    if leftover>use_len{let free=new+use_len;put32(block,free,0)?;put16(block,free+4,(leftover-use_len) as u16)?;block[free+6]=0;block[free+7]=0}
    placed=true;break
   }
  }
  off+=rec;
 }
 if !placed{return Err(Error::OutOfRange)}
 if csum{update_directory_block_checksum(sb,directory_inode,generation,block)?}Ok(())
}
/// Remove a named directory entry, merging its record length into the previous
/// entry when possible. The caller must separately free the inode/blocks in the
/// same JBD2 transaction.
pub fn remove_directory_entry(
 sb:&[u8],directory_inode:u32,generation:u32,block:&mut[u8],total_inodes:u32,name:&[u8],
)->Result<u32,Error>{
 let csum=metadata_csum(sb)?;if csum{validate_directory_block_checksum(sb,directory_inode,generation,block)?}
 let limit=dir_limit(block,csum)?;validate_dir_entries(block,limit,total_inodes)?;
 let mut off=0usize;let mut previous:Option<usize>=None;
 while off<limit{
  let ino=le32(block,off)?;let rec=le16(block,off+4)? as usize;let n=block[off+6] as usize;
  if ino!=0&&n==name.len()&&&block[off+8..off+8+n]==name{
   if let Some(prev)=previous{
    let prior=le16(block,prev+4)? as usize;put16(block,prev+4,prior.checked_add(rec).ok_or(Error::OutOfRange)? as u16)?;
    block[off..off+8].fill(0);
   }else{
    put32(block,off,0)?;block[off+6]=0;block[off+7]=0;block[off+8..off+rec].fill(0);
   }
   if csum{update_directory_block_checksum(sb,directory_inode,generation,block)?}
   return Ok(ino)
  }
  if ino!=0{previous=Some(off)}
  off+=rec;
 }
 Err(Error::OutOfRange)
}

/// Insert an initialized extent into an inode's inline extent root (depth zero).
/// Adjacent logical and physical runs are coalesced; external extent trees are
/// rejected rather than partially updated.
pub fn insert_inline_extent(root:&mut[u8],logical:u32,physical:u64,len:u16)->Result<(),Error>{
 if root.len()<60||le16(root,0)?!=0xf30a||le16(root,6)?!=0{return Err(Error::UnsupportedFormat)}
 if len==0||len>0x8000||physical==0||physical.checked_add(len as u64).ok_or(Error::OutOfRange)?>(1u64<<48){return Err(Error::OutOfRange)}
 let entries=le16(root,2)? as usize;let max=le16(root,4)? as usize;
 if entries>max||max>4{return Err(Error::InvalidExtentTree)}
 let end=(logical as u64).checked_add(len as u64).ok_or(Error::OutOfRange)?;
 for i in 0..entries{
  let o=12+i*12;let first=le32(root,o)?;let raw=le16(root,o+4)?;if raw==0{return Err(Error::InvalidExtentTree)}
  let n=if raw>0x8000{(raw-0x8000) as u64}else{raw as u64};
  let old_end=(first as u64)+n;if (logical as u64)<old_end&&(first as u64)<end{return Err(Error::InvalidExtentTree)}
 }
 // Build a sorted temporary root so insertion never leaves a half-shifted tree.
 let mut tmp=[0u8;60];tmp.copy_from_slice(&root[..60]);let mut pos=0usize;
 while pos<entries&&le32(&tmp,12+pos*12)?<logical{pos+=1}
 if pos>0{
  let p=12+(pos-1)*12;let p_log=le32(&tmp,p)?;let p_raw=le16(&tmp,p+4)?;let p_len=p_raw as u64;
  let p_phys=((le16(&tmp,p+6)? as u64)<<32)|(le32(&tmp,p+8)? as u64);
  if p_raw<=0x8000&&p_log as u64+p_len==logical as u64&&p_phys+p_len==physical&&p_len+len as u64<=0x8000{
   put16(&mut tmp,p+4,(p_len+len as u64) as u16)?;root[..60].copy_from_slice(&tmp);return Ok(())
  }
 }
 if pos<entries{
  let n=12+pos*12;let n_log=le32(&tmp,n)?;let n_raw=le16(&tmp,n+4)?;let n_phys=((le16(&tmp,n+6)? as u64)<<32)|(le32(&tmp,n+8)? as u64);
  if n_raw<=0x8000&&end==n_log as u64&&physical+len as u64==n_phys{
   let merged_len=len as u64+le16(&tmp,n+4)? as u64;
   if merged_len>0x8000{return Err(Error::OutOfRange)}
   put32(&mut tmp,n,logical)?;put16(&mut tmp,n+4,merged_len as u16)?;
   put16(&mut tmp,n+6,(physical>>32) as u16)?;put32(&mut tmp,n+8,physical as u32)?;
   root[..60].copy_from_slice(&tmp);return Ok(())
  }
 }
 if entries>=max{return Err(Error::OutOfRange)}
 for i in (pos..entries).rev(){let from=12+i*12;let to=from+12;tmp.copy_within(from..from+12,to)}
 let o=12+pos*12;put32(&mut tmp,o,logical)?;put16(&mut tmp,o+4,len)?;put16(&mut tmp,o+6,(physical>>32) as u16)?;put32(&mut tmp,o+8,physical as u32)?;
 put16(&mut tmp,2,(entries+1) as u16)?;root[..60].copy_from_slice(&tmp);Ok(())
}

/// Shrink an inline extent root to the requested logical block count. Returned
/// physical ranges must be released through block-group bitmap transactions.
pub fn truncate_inline_extents(root:&mut[u8],keep_blocks:u64,freed:&mut[(u64,u32)])->Result<usize,Error>{
 if root.len()<60||le16(root,0)?!=0xf30a||le16(root,6)?!=0{return Err(Error::UnsupportedFormat)}
 let entries=le16(root,2)? as usize;let max=le16(root,4)? as usize;if entries>max||max>4{return Err(Error::InvalidExtentTree)}
 // Preflight all output capacity and extent lengths before changing root.
 let mut required=0usize;
 for i in 0..entries{
  let o=12+i*12;let logical=le32(root,o)? as u64;let raw=le16(root,o+4)?;
  if raw==0{return Err(Error::InvalidExtentTree)}
  let len=if raw>0x8000{(raw-0x8000) as u64}else{raw as u64};
  if logical>=keep_blocks||logical.checked_add(len).ok_or(Error::OutOfRange)?>keep_blocks{required+=1}
 }
 if required>freed.len(){return Err(Error::TransactionTooLarge)}
 let mut removed=0usize;let mut out=0usize;
 for i in 0..entries{
  let o=12+i*12;let logical=le32(root,o)? as u64;let raw=le16(root,o+4)?;let len=if raw>0x8000{raw-0x8000}else{raw} as u64;
  let physical=((le16(root,o+6)? as u64)<<32)|(le32(root,o+8)? as u64);let end=logical.checked_add(len).ok_or(Error::OutOfRange)?;
  if logical>=keep_blocks{
   if removed>=freed.len(){return Err(Error::TransactionTooLarge)}
   freed[removed]=(physical,len as u32);removed+=1;continue
  }
  if end>keep_blocks{
   let keep=(keep_blocks-logical) as u16;let drop=(end-keep_blocks) as u32;
   if removed>=freed.len(){return Err(Error::TransactionTooLarge)}
   freed[removed]=(physical+keep as u64,drop);removed+=1;put16(root,o+4,if raw>0x8000{keep|0x8000}else{keep})?
  }
  if out!=i{root.copy_within(o..o+12,12+out*12)}
  out+=1;
 }
 root[12+out*12..60].fill(0);put16(root,2,out as u16)?;Ok(removed)
}

/// Update i_size (including the high word) and refresh the inode checksum.
/// The caller must update extents, i_blocks, timestamps and journal all changes.
pub fn set_inode_size(sb:&[u8],inode_number:u32,inode:&mut[u8],new_size:u64)->Result<(),Error>{
 if inode.len()<128||inode_number==0{return Err(Error::Truncated)}
 validate_inode_before_mutation(sb,inode_number,inode)?;
 if le16(inode,0)?&0xf000!=0x8000{return Err(Error::UnsupportedFormat)}
 put32(inode,4,new_size as u32)?;
 if inode.len()>=112{put32(inode,108,(new_size>>32) as u32)?}else if new_size>u32::MAX as u64{return Err(Error::OutOfRange)}
 if metadata_csum(sb)?{update_inode_checksum(sb,inode_number,inode)?}Ok(())
}

/// Resize a regular file whose extent tree is stored in the inode's inline
/// root. Shrinking returns physical ranges for the caller to free in the same
/// JBD2 transaction; extending creates a sparse tail and allocates no blocks.
/// External extent trees are rejected until their node checksums and split/merge
/// updates can be journaled as one transaction.
pub fn resize_inline_regular_file(
 sb:&[u8],inode_number:u32,inode:&mut[u8],new_size:u64,freed:&mut[(u64,u32)],
)->Result<usize,Error>{
 if inode.len()<128||le16(inode,0)?&0xf000!=0x8000{return Err(Error::UnsupportedFormat)}
 validate_inode_before_mutation(sb,inode_number,inode)?;
 let old_size=(le32(inode,4)? as u64)|((le32(inode,108)? as u64)<<32);
 let mut freed_count=0usize;
 if new_size<old_size{
  let bs=Superblock::parse(sb)?.block_size as u64;
  let keep=new_size.checked_add(bs-1).ok_or(Error::OutOfRange)?/bs;
  freed_count=truncate_inline_extents(&mut inode[40..100],keep,freed)?;
  let mut freed_blocks=0u64;
  for (start,count) in freed.iter().take(freed_count){let _=start;freed_blocks=freed_blocks.checked_add(*count as u64).ok_or(Error::OutOfRange)?}
  let sectors=freed_blocks.checked_mul(bs/512).ok_or(Error::OutOfRange)?;
  let lo=le32(inode,28)? as u64;let hi=if inode.len()>=120{le16(inode,116)? as u64}else{0};
  let current=lo|(hi<<32);let next=current.checked_sub(sectors).ok_or(Error::InvalidGeometry)?;
  put32(inode,28,next as u32)?;if inode.len()>=120{put16(inode,116,(next>>32) as u16)?}
 }
 put32(inode,4,new_size as u32)?;put32(inode,108,(new_size>>32) as u32)?;
 if metadata_csum(sb)?{update_inode_checksum(sb,inode_number,inode)?}
 Ok(freed_count)
}

/// Validate an inode before it is modified, when metadata_csum is enabled.
pub fn validate_inode_before_mutation(sb:&[u8],inode_number:u32,inode:&[u8])->Result<(),Error>{
 validate_superblock_checksum(sb)?;
 if metadata_csum(sb)?{validate_inode_checksum(sb,inode_number,inode)?}Ok(())
}

#[cfg(test)]
mod tests{
 use super::*;
 #[test]fn bitmap_allocation_and_release_are_bounded(){
  let mut b=[0u8;2];assert_eq!(allocate_bitmap_bit(&mut b,12,3),Ok(3));assert_eq!(allocate_bitmap_bit(&mut b,12,3),Ok(4));assert_eq!(release_bitmap_bit(&mut b,12,3),Ok(()));assert_eq!(release_bitmap_bit(&mut b,12,3),Err(Error::InvalidGeometry));assert_eq!(release_bitmap_bit(&mut b,12,12),Err(Error::OutOfRange));
 }
 #[test]fn directory_entry_insert_and_unlink_preserve_record_chain(){
  let mut b=[0u8;64];b[0..4].copy_from_slice(&2u32.to_le_bytes());b[4..6].copy_from_slice(&64u16.to_le_bytes());b[6]=1;b[7]=2;b[8]=b'.';
  insert_directory_entry(&[0u8;1024],2,0,&mut b,100,12,b"hello",1).unwrap();
  assert_eq!(le32(&b,12).unwrap(),12);assert_eq!(&b[20..25],b"hello");
  assert_eq!(remove_directory_entry(&[0u8;1024],2,0,&mut b,100,b"hello"),Ok(12));
 }
 #[test]fn metadata_checksum_allocator_updates_bitmap_group_and_superblock(){
  let mut sb=[0u8;1024];
  sb[0..4].copy_from_slice(&128u32.to_le_bytes());sb[4..8].copy_from_slice(&4096u32.to_le_bytes());
  sb[20..24].copy_from_slice(&1u32.to_le_bytes());sb[24..28].copy_from_slice(&0u32.to_le_bytes());
  sb[32..36].copy_from_slice(&1024u32.to_le_bytes());sb[40..44].copy_from_slice(&128u32.to_le_bytes());
  sb[56..58].copy_from_slice(&0xef53u16.to_le_bytes());sb[76..80].copy_from_slice(&1u32.to_le_bytes());
  sb[84..88].copy_from_slice(&11u32.to_le_bytes());sb[88..90].copy_from_slice(&128u16.to_le_bytes());
  sb[96..100].copy_from_slice(&(super::super::INCOMPAT_FILETYPE|super::super::INCOMPAT_EXTENTS).to_le_bytes());
  sb[100..104].copy_from_slice(&0x0400u32.to_le_bytes());sb[16..20].copy_from_slice(&100u32.to_le_bytes());
  sb[12..16].copy_from_slice(&400u32.to_le_bytes());sb[373]=1;sb[104..120].copy_from_slice(&[0x31;16]);
  let mut gd=[0u8;64];gd[0..4].copy_from_slice(&1u32.to_le_bytes());gd[4..8].copy_from_slice(&2u32.to_le_bytes());gd[8..12].copy_from_slice(&3u32.to_le_bytes());
  gd[12..14].copy_from_slice(&100u16.to_le_bytes());gd[14..16].copy_from_slice(&100u16.to_le_bytes());
  let mut ib=[0u8;16];let mut bb=[0u8;128];bb[0]=0x0f;
  update_inode_bitmap_checksum(&sb,&mut gd,&ib).unwrap();update_block_bitmap_checksum(&sb,&mut gd,&bb).unwrap();
  update_group_descriptor_checksum(&sb,0,&mut gd).unwrap();update_superblock_checksum(&mut sb).unwrap();
  let ino=allocate_inode(&mut sb,0,&mut gd,&mut ib,0).unwrap();assert_eq!(ino,11);
  validate_inode_bitmap_checksum(&sb,&gd,&ib).unwrap();validate_group_descriptor_checksum(&sb,0,&gd).unwrap();validate_superblock_checksum(&sb).unwrap();
  let block=allocate_block(&mut sb,0,&mut gd,&mut bb,0).unwrap();assert_eq!(block,5);
  validate_block_bitmap_checksum(&sb,&gd,&bb).unwrap();validate_group_descriptor_checksum(&sb,0,&gd).unwrap();validate_superblock_checksum(&sb).unwrap();
 }
 #[test]fn metadata_csum_directory_insert_remove_rewrites_tail(){
  let mut sb=[0u8;1024];sb[100..104].copy_from_slice(&0x0400u32.to_le_bytes());sb[373]=1;sb[104..120].copy_from_slice(&[0x22;16]);update_superblock_checksum(&mut sb).unwrap();
  let mut dir=[0u8;1024];dir[0..4].copy_from_slice(&2u32.to_le_bytes());dir[4..6].copy_from_slice(&1012u16.to_le_bytes());dir[6]=1;dir[7]=2;dir[8]=b'.';
  dir[1012..1016].copy_from_slice(&0u32.to_le_bytes());dir[1016..1018].copy_from_slice(&12u16.to_le_bytes());dir[1019]=0xde;
  update_directory_block_checksum(&sb,2,7,&mut dir).unwrap();
  insert_directory_entry(&sb,2,7,&mut dir,100,12,b"hello",1).unwrap();
  validate_directory_block_checksum(&sb,2,7,&dir).unwrap();
  assert_eq!(remove_directory_entry(&sb,2,7,&mut dir,100,b"hello"),Ok(12));
  validate_directory_block_checksum(&sb,2,7,&dir).unwrap();
 }
 #[test]fn directory_inode_and_dot_entries_are_initialized_with_checksums(){
  let mut sb=[0u8;1024];sb[0..4].copy_from_slice(&64u32.to_le_bytes());sb[4..8].copy_from_slice(&4096u32.to_le_bytes());sb[20..24].copy_from_slice(&1u32.to_le_bytes());sb[32..36].copy_from_slice(&1024u32.to_le_bytes());sb[40..44].copy_from_slice(&64u32.to_le_bytes());sb[56..58].copy_from_slice(&0xef53u16.to_le_bytes());sb[76..80].copy_from_slice(&1u32.to_le_bytes());sb[88..90].copy_from_slice(&128u16.to_le_bytes());sb[96..100].copy_from_slice(&(super::super::INCOMPAT_FILETYPE|super::super::INCOMPAT_EXTENTS).to_le_bytes());sb[100..104].copy_from_slice(&0x0400u32.to_le_bytes());sb[373]=1;sb[104..120].copy_from_slice(&[0x44;16]);update_superblock_checksum(&mut sb).unwrap();
  let meta=NewInodeMetadata{mode:0x41ed,uid:1000,gid:1000,links:2,generation:3,atime:1,ctime:2,mtime:2};
  let mut inode=[0u8;256];initialize_directory_inode(&sb,12,&mut inode,meta,50).unwrap();
  validate_inode_checksum(&sb,12,&inode).unwrap();assert_eq!(le32(&inode,4).unwrap(),1024);assert_eq!(le32(&inode,28).unwrap(),2);
  let mut block=[0u8;1024];initialize_directory_block(&sb,12,2,3,&mut block).unwrap();
  validate_directory_block_checksum(&sb,12,3,&block).unwrap();assert_eq!(le32(&block,0).unwrap(),12);assert_eq!(le32(&block,12).unwrap(),2);
 }
 #[test]fn regular_file_creation_and_last_unlink_update_inode_checksum(){
  let mut sb=[0u8;1024];sb[0..4].copy_from_slice(&64u32.to_le_bytes());sb[4..8].copy_from_slice(&4096u32.to_le_bytes());sb[20..24].copy_from_slice(&1u32.to_le_bytes());sb[32..36].copy_from_slice(&1024u32.to_le_bytes());sb[40..44].copy_from_slice(&64u32.to_le_bytes());sb[56..58].copy_from_slice(&0xef53u16.to_le_bytes());sb[76..80].copy_from_slice(&1u32.to_le_bytes());sb[88..90].copy_from_slice(&128u16.to_le_bytes());sb[96..100].copy_from_slice(&(super::super::INCOMPAT_FILETYPE|super::super::INCOMPAT_EXTENTS).to_le_bytes());sb[100..104].copy_from_slice(&0x0400u32.to_le_bytes());sb[373]=1;sb[104..120].copy_from_slice(&[0x51;16]);update_superblock_checksum(&mut sb).unwrap();
  let meta=NewInodeMetadata{mode:0x81a4,uid:1000,gid:1000,links:1,generation:9,atime:1,ctime:1,mtime:1};
  let mut inode=[0u8;256];initialize_regular_inode(&sb,20,&mut inode,meta).unwrap();validate_inode_checksum(&sb,20,&inode).unwrap();
  assert_eq!(unlink_regular_inode(&sb,20,&mut inode,55),Ok(true));assert_eq!(le16(&inode,26).unwrap(),0);assert_eq!(le32(&inode,20).unwrap(),55);validate_inode_checksum(&sb,20,&inode).unwrap();
 }
 #[test]fn truncate_regular_inode_updates_extent_size_blocks_and_checksum(){
  let mut sb=[0u8;1024];sb[0..4].copy_from_slice(&16u32.to_le_bytes());sb[4..8].copy_from_slice(&100u32.to_le_bytes());sb[20..24].copy_from_slice(&1u32.to_le_bytes());sb[32..36].copy_from_slice(&32u32.to_le_bytes());sb[40..44].copy_from_slice(&16u32.to_le_bytes());sb[56..58].copy_from_slice(&0xef53u16.to_le_bytes());sb[76..80].copy_from_slice(&1u32.to_le_bytes());sb[88..90].copy_from_slice(&128u16.to_le_bytes());sb[96..100].copy_from_slice(&(super::super::INCOMPAT_FILETYPE|super::super::INCOMPAT_EXTENTS).to_le_bytes());sb[100..104].copy_from_slice(&0x0400u32.to_le_bytes());sb[373]=1;sb[104..120].copy_from_slice(&[0x19;16]);update_superblock_checksum(&mut sb).unwrap();
  let mut inode=[0u8;256];inode[0..2].copy_from_slice(&0x8000u16.to_le_bytes());inode[4..8].copy_from_slice(&5120u32.to_le_bytes());
  inode[28..32].copy_from_slice(&10u32.to_le_bytes());inode[40..42].copy_from_slice(&0xf30au16.to_le_bytes());inode[42..44].copy_from_slice(&1u16.to_le_bytes());inode[44..46].copy_from_slice(&4u16.to_le_bytes());
  inode[52..56].copy_from_slice(&0u32.to_le_bytes());inode[56..58].copy_from_slice(&5u16.to_le_bytes());inode[60..64].copy_from_slice(&100u32.to_le_bytes());
  inode[100..104].copy_from_slice(&7u32.to_le_bytes());update_inode_checksum(&sb,12,&mut inode).unwrap();
  let mut freed=[(0u64,0u32);4];assert_eq!(resize_inline_regular_file(&sb,12,&mut inode,3072,&mut freed),Ok(1));
  assert_eq!(le32(&inode,4).unwrap(),3072);assert_eq!(le32(&inode,28).unwrap(),6);assert_eq!(freed[0],(103,2));
  validate_inode_checksum(&sb,12,&inode).unwrap();
 }
}
