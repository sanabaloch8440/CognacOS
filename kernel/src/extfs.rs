//! Read-only ext2/ext3/ext4 metadata parser for native RdotOS.
//! No writes or journal replay. Ext3Fsd-derived work is isolated pending GPLv2 review.
#![allow(dead_code)]
pub const SUPERBLOCK_BYTES:usize=1024;
pub const SUPERBLOCK_DISK_OFFSET:u64=1024;
pub const EXT_MAGIC:u16=0xef53;
pub const INCOMPAT_FILETYPE:u32=0x0002;
pub const INCOMPAT_META_BG:u32=0x0010;
pub const INCOMPAT_EXTENTS:u32=0x0040;
pub const INCOMPAT_64BIT:u32=0x0080;
pub const INCOMPAT_FLEX_BG:u32=0x0200;
const KNOWN_INCOMPAT:u32=INCOMPAT_FILETYPE|INCOMPAT_EXTENTS|INCOMPAT_FLEX_BG;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Error{Truncated,BadMagic,InvalidBlockSize,InvalidInodeSize,InvalidGeometry,UnsupportedIncompatFeature,OutOfRange,InvalidDirectoryEntry,UnsupportedFormat}
fn u16at(b:&[u8],o:usize)->Result<u16,Error>{let e=o.checked_add(2).ok_or(Error::OutOfRange)?;let s=b.get(o..e).ok_or(Error::Truncated)?;Ok(u16::from_le_bytes([s[0],s[1]]))}
fn u32at(b:&[u8],o:usize)->Result<u32,Error>{let e=o.checked_add(4).ok_or(Error::OutOfRange)?;let s=b.get(o..e).ok_or(Error::Truncated)?;Ok(u32::from_le_bytes([s[0],s[1],s[2],s[3]]))}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct Superblock{pub inodes_count:u32,pub blocks_count:u32,pub first_data_block:u32,pub block_size:u32,pub blocks_per_group:u32,pub inodes_per_group:u32,pub inode_size:u16,pub revision:u32,pub feature_compat:u32,pub feature_incompat:u32,pub feature_ro_compat:u32,pub read_only:bool}
impl Superblock{
 pub fn parse(b:&[u8])->Result<Self,Error>{
  if b.len()<SUPERBLOCK_BYTES{return Err(Error::Truncated)} if u16at(b,56)?!=EXT_MAGIC{return Err(Error::BadMagic)}
  let revision=u32at(b,76)?;let log=u32at(b,24)?;if log>6{return Err(Error::InvalidBlockSize)}
  let block_size=1024u32.checked_shl(log).ok_or(Error::InvalidBlockSize)?;
  let inode_size=if revision==0{128}else{u16at(b,88)?};
  if inode_size<128||inode_size as u32>block_size||!inode_size.is_power_of_two(){return Err(Error::InvalidInodeSize)}
  let inodes_count=u32at(b,0)?;let blocks_count=u32at(b,4)?;let first_data_block=u32at(b,20)?;
  let blocks_per_group=u32at(b,32)?;let inodes_per_group=u32at(b,40)?;
  if inodes_count==0||blocks_count==0||blocks_per_group==0||inodes_per_group==0||first_data_block>=blocks_count||(block_size==1024&&first_data_block!=1)||(block_size>1024&&first_data_block!=0){return Err(Error::InvalidGeometry)}
  let feature_compat=u32at(b,92)?;let feature_incompat=u32at(b,96)?;let feature_ro_compat=u32at(b,100)?;
  if feature_incompat&INCOMPAT_64BIT!=0||feature_incompat&INCOMPAT_META_BG!=0||feature_incompat&!KNOWN_INCOMPAT!=0{return Err(Error::UnsupportedIncompatFeature)}
  Ok(Self{inodes_count,blocks_count,first_data_block,block_size,blocks_per_group,inodes_per_group,inode_size,revision,feature_compat,feature_incompat,feature_ro_compat,read_only:true})
 }
 pub fn group_count(&self)->Result<u32,Error>{let n=self.blocks_count.checked_sub(self.first_data_block).ok_or(Error::InvalidGeometry)?;let n=n.checked_add(self.blocks_per_group-1).ok_or(Error::OutOfRange)?/self.blocks_per_group;if n==0{Err(Error::InvalidGeometry)}else{Ok(n)}}
 pub fn inode_location(&self,ino:u32)->Result<(u32,u32),Error>{if ino==0||ino>self.inodes_count{return Err(Error::OutOfRange)}let z=ino-1;let g=z/self.inodes_per_group;if g>=self.group_count()?{return Err(Error::OutOfRange)}Ok((g,(z%self.inodes_per_group).checked_mul(self.inode_size as u32).ok_or(Error::OutOfRange)?))}
 pub fn has_extents(&self)->bool{self.feature_incompat&INCOMPAT_EXTENTS!=0}
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct GroupDescriptor{pub block_bitmap:u32,pub inode_bitmap:u32,pub inode_table:u32,pub free_blocks_count:u16,pub free_inodes_count:u16}
impl GroupDescriptor{pub fn parse(b:&[u8],sb:&Superblock)->Result<Self,Error>{if b.len()<32{return Err(Error::Truncated)}if sb.feature_incompat&INCOMPAT_64BIT!=0{return Err(Error::UnsupportedFormat)}let d=Self{block_bitmap:u32at(b,0)?,inode_bitmap:u32at(b,4)?,inode_table:u32at(b,8)?,free_blocks_count:u16at(b,12)?,free_inodes_count:u16at(b,14)?};if d.block_bitmap>=sb.blocks_count||d.inode_bitmap>=sb.blocks_count||d.inode_table>=sb.blocks_count{return Err(Error::OutOfRange)}Ok(d)}}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct Inode{pub mode:u16,pub size:u64,pub blocks_512:u32,pub flags:u32,pub is_directory:bool,pub is_regular_file:bool,pub is_symlink:bool}
impl Inode{pub fn parse(b:&[u8],size:u16,revision:u32,ro_compat:u32)->Result<Self,Error>{if size<128||b.len()<size as usize{return Err(Error::Truncated)}let mode=u16at(b,0)?;let size_lo=u32at(b,4)? as u64;let blocks_512=u32at(b,28)?;let flags=u32at(b,32)?;let is_regular_file=mode&0xf000==0x8000;let is_directory=mode&0xf000==0x4000;let is_symlink=mode&0xf000==0xa000;let hi=if is_regular_file&&revision!=0&&ro_compat&2!=0&&size>=112{u32at(b,108)? as u64}else{0};Ok(Self{mode,size:(hi<<32)|size_lo,blocks_512,flags,is_directory,is_regular_file,is_symlink})}}
pub fn validate_directory_block(b:&[u8],total_inodes:u32)->Result<usize,Error>{if b.is_empty(){return Err(Error::Truncated)}let(mut off,mut count)=(0usize,0usize);while off<b.len(){if b.len()-off<8{return Err(Error::InvalidDirectoryEntry)}let ino=u32at(b,off)?;let rec=u16at(b,off+4)? as usize;let n=b[off+6] as usize;if rec<8||rec&3!=0||rec>b.len()-off||n>rec-8||(ino!=0&&ino>total_inodes){return Err(Error::InvalidDirectoryEntry)}if b[off+8..off+8+n].iter().any(|c|*c==0||*c==b'/'){return Err(Error::InvalidDirectoryEntry)}count+=1;off+=rec;}Ok(count)}
pub trait ReadOnlyBlockDevice{type DeviceError;fn capacity_bytes(&self)->u64;fn read_at(&mut self,offset:u64,out:&mut[u8])->Result<(),Self::DeviceError>;}
pub fn read_superblock<D:ReadOnlyBlockDevice>(dev:&mut D)->Result<Superblock,Error>{let end=SUPERBLOCK_DISK_OFFSET.checked_add(SUPERBLOCK_BYTES as u64).ok_or(Error::OutOfRange)?;if dev.capacity_bytes()<end{return Err(Error::OutOfRange)}let mut b=[0u8;SUPERBLOCK_BYTES];dev.read_at(SUPERBLOCK_DISK_OFFSET,&mut b).map_err(|_|Error::Truncated)?;Superblock::parse(&b)}
pub fn self_test()->bool{let mut b=[0u8;SUPERBLOCK_BYTES];b[0..4].copy_from_slice(&128u32.to_le_bytes());b[4..8].copy_from_slice(&4096u32.to_le_bytes());b[20..24].copy_from_slice(&1u32.to_le_bytes());b[32..36].copy_from_slice(&1024u32.to_le_bytes());b[40..44].copy_from_slice(&128u32.to_le_bytes());b[56..58].copy_from_slice(&EXT_MAGIC.to_le_bytes());b[76..80].copy_from_slice(&1u32.to_le_bytes());b[88..90].copy_from_slice(&128u16.to_le_bytes());b[96..100].copy_from_slice(&INCOMPAT_FILETYPE.to_le_bytes());let s=match Superblock::parse(&b){Ok(s)=>s,Err(_)=>return false};if s.block_size!=1024||s.inode_size!=128||s.group_count()!=Ok(4)||s.inode_location(129)!=Ok((1,0))||!s.read_only{return false}let mut bad=b;bad[96..100].copy_from_slice(&INCOMPAT_64BIT.to_le_bytes());if Superblock::parse(&bad)!=Err(Error::UnsupportedIncompatFeature){return false}let mut d=[0u8;12];d[0..4].copy_from_slice(&2u32.to_le_bytes());d[4..6].copy_from_slice(&12u16.to_le_bytes());d[6]=4;d[8..12].copy_from_slice(b"test");if validate_directory_block(&d,128)!=Ok(1){return false}d[4..6].copy_from_slice(&0u16.to_le_bytes());validate_directory_block(&d,128)==Err(Error::InvalidDirectoryEntry)}
