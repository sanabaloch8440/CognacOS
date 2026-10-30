//! VFS-facing HTree lookup and indexed readdir adapters.
use crate::{Error, htree::{DirectoryBlockReader,DirectoryEntryView,lookup_htree_name,iterate_htree_directory},hash::ext4_directory_hash_seed};

fn metadata_csum(sb:&[u8])->Result<bool,Error>{
 if sb.len()<1024{return Err(Error::Truncated)}
 let flags=u32::from_le_bytes(sb[100..104].try_into().map_err(|_|Error::Truncated)?);
 Ok(flags&0x0400!=0)
}

/// Lookup an on-disk filename in an indexed directory. The caller must pass
/// the prepared disk name (after fscrypt/casefold transforms) and a reader
/// that maps logical blocks using checked inode extents.
pub fn lookup_indexed_directory_entry<R:DirectoryBlockReader>(
 sb:&[u8],directory_inode:u32,generation:u32,root:&[u8],reader:&mut R,
 directory_blocks:u32,total_inodes:u32,disk_name:&[u8],siphash_key:Option<[u64;2]>,
 scratch:&mut[u8],
)->Result<u32,Error>{
 crate::validate_superblock_checksum(sb)?;
 let hash_seed=ext4_directory_hash_seed(sb)?;
 let csum=metadata_csum(sb)?;
 let inode_seed=if csum{crate::ext4_inode_checksum_seed(sb,directory_inode,generation)?}else{0};
 lookup_htree_name(reader,root,directory_blocks,total_inodes,disk_name,hash_seed,
  siphash_key,csum,inode_seed,scratch)
}

/// Enumerate indexed-directory entries and their hashes. VFS should collect
/// and sort by (major, minor, name) before assigning stable readdir cookies.
pub fn iterate_indexed_directory<R,F>(
 sb:&[u8],directory_inode:u32,generation:u32,root:&[u8],reader:&mut R,
 directory_blocks:u32,total_inodes:u32,siphash_key:Option<[u64;2]>,scratch:&mut[u8],visitor:F,
)->Result<(),Error>
where R:DirectoryBlockReader,
 F:FnMut(DirectoryEntryView<'_>)->Result<bool,Error>{
 crate::validate_superblock_checksum(sb)?;
 let hash_seed=ext4_directory_hash_seed(sb)?;
 let csum=metadata_csum(sb)?;
 let inode_seed=if csum{crate::ext4_inode_checksum_seed(sb,directory_inode,generation)?}else{0};
 iterate_htree_directory(reader,root,directory_blocks,total_inodes,hash_seed,
  siphash_key,csum,inode_seed,scratch,visitor)
}
