//! ext2/ext3/ext4 filesystem driver core for CognacOS.
//! Writes are limited to existing allocated data blocks; journaled metadata mutation is not implemented.
#![no_std]
#![allow(dead_code)]

/// Canonical Windows NT ABI declarations shared with ntoskrnl and NTDLL.
pub use cognacos_nt_abi as nt_abi;
#[cfg(test)] extern crate std;
pub const SUPERBLOCK_BYTES:usize=1024;
pub const SUPERBLOCK_DISK_OFFSET:u64=1024;
pub const EXT_MAGIC:u16=0xef53;
pub const INCOMPAT_FILETYPE:u32=0x0002;
pub const INCOMPAT_META_BG:u32=0x0010;
pub const INCOMPAT_EXTENTS:u32=0x0040;
pub const INCOMPAT_64BIT:u32=0x0080;
pub const INCOMPAT_FLEX_BG:u32=0x0200;
const SUPPORTED_RO_COMPAT:u32=0x0003; // sparse_super and large_file only
const KNOWN_INCOMPAT:u32=INCOMPAT_FILETYPE|INCOMPAT_EXTENTS|INCOMPAT_FLEX_BG|INCOMPAT_64BIT;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Error{Truncated,BadMagic,InvalidBlockSize,InvalidInodeSize,InvalidGeometry,UnsupportedIncompatFeature,OutOfRange,InvalidDirectoryEntry,UnsupportedFormat,UnsupportedRoCompatFeature,UnsupportedCompatFeature,InvalidExtentTree,InvalidChecksum,InvalidPartitionTable,UnsupportedPartitionTable,Io}

/// Translate filesystem-core errors into the canonical NTSTATUS ABI at the driver boundary.
pub const fn error_to_ntstatus(error: Error) -> nt_abi::ntstatus::NTSTATUS {
    use nt_abi::ntstatus::*;
    match error {
        Error::Truncated => STATUS_BUFFER_TOO_SMALL,
        Error::UnsupportedIncompatFeature | Error::UnsupportedFormat | Error::UnsupportedRoCompatFeature
        | Error::UnsupportedCompatFeature | Error::UnsupportedPartitionTable => STATUS_NOT_IMPLEMENTED,
        Error::Io => STATUS_UNSUCCESSFUL,
        Error::BadMagic | Error::InvalidBlockSize | Error::InvalidInodeSize | Error::InvalidGeometry
        | Error::OutOfRange | Error::InvalidDirectoryEntry | Error::InvalidExtentTree
        | Error::InvalidChecksum | Error::InvalidPartitionTable => STATUS_INVALID_PARAMETER,
    }
}
fn u16at(b:&[u8],o:usize)->Result<u16,Error>{let e=o.checked_add(2).ok_or(Error::OutOfRange)?;let s=b.get(o..e).ok_or(Error::Truncated)?;Ok(u16::from_le_bytes([s[0],s[1]]))}
fn u32at(b:&[u8],o:usize)->Result<u32,Error>{let e=o.checked_add(4).ok_or(Error::OutOfRange)?;let s=b.get(o..e).ok_or(Error::Truncated)?;Ok(u32::from_le_bytes([s[0],s[1],s[2],s[3]]))}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct Superblock{pub inodes_count:u32,pub blocks_count:u64,pub first_data_block:u32,pub block_size:u32,pub blocks_per_group:u32,pub inodes_per_group:u32,pub inode_size:u16,pub descriptor_size:u16,pub revision:u32,pub feature_compat:u32,pub feature_incompat:u32,pub feature_ro_compat:u32,pub read_only:bool}
impl Superblock{
 pub fn parse(b:&[u8])->Result<Self,Error>{
  if b.len()<SUPERBLOCK_BYTES{return Err(Error::Truncated)} if u16at(b,56)?!=EXT_MAGIC{return Err(Error::BadMagic)}
  let revision=u32at(b,76)?;let log=u32at(b,24)?;if log>6{return Err(Error::InvalidBlockSize)}
  let block_size=1024u32.checked_shl(log).ok_or(Error::InvalidBlockSize)?;
  let inode_size=if revision==0{128}else{u16at(b,88)?};
  if inode_size<128||inode_size as u32>block_size||!inode_size.is_power_of_two(){return Err(Error::InvalidInodeSize)}
  let feature_compat=u32at(b,92)?;let feature_incompat=u32at(b,96)?;let feature_ro_compat=u32at(b,100)?;
  let inodes_count=u32at(b,0)?;let blocks_lo=u32at(b,4)?;
  let blocks_count=if feature_incompat&INCOMPAT_64BIT!=0{((u32at(b,336)? as u64)<<32)|(blocks_lo as u64)}else{blocks_lo as u64};
  let descriptor_size=if feature_incompat&INCOMPAT_64BIT!=0{let n=u16at(b,254)?;if n<64||n as u32>block_size||n%8!=0{return Err(Error::UnsupportedFormat)}n}else{32};
  let first_data_block=u32at(b,20)?;
  let blocks_per_group=u32at(b,32)?;let inodes_per_group=u32at(b,40)?;
  if inodes_count==0||blocks_count==0||blocks_per_group==0||inodes_per_group==0||(first_data_block as u64)>=blocks_count||(block_size==1024&&first_data_block!=1)||(block_size>1024&&first_data_block!=0){return Err(Error::InvalidGeometry)}
  if feature_ro_compat & !SUPPORTED_RO_COMPAT != 0{return Err(Error::UnsupportedRoCompatFeature)}
  // DIR_INDEX requires htree-aware directory parsing; never treat an indexed root as linear entries.
  if feature_compat & 0x0020 != 0{return Err(Error::UnsupportedCompatFeature)}if revision>1{return Err(Error::UnsupportedFormat)}
  if feature_ro_compat & !SUPPORTED_RO_COMPAT != 0{return Err(Error::UnsupportedRoCompatFeature)}
  if revision>1{return Err(Error::UnsupportedFormat)}
  if feature_incompat&INCOMPAT_META_BG!=0||feature_incompat&!KNOWN_INCOMPAT!=0{return Err(Error::UnsupportedIncompatFeature)}
  Ok(Self{inodes_count,blocks_count,first_data_block,block_size,blocks_per_group,inodes_per_group,inode_size,descriptor_size,revision,feature_compat,feature_incompat,feature_ro_compat,read_only:true})
 }
 pub fn group_count(&self)->Result<u32,Error>{let n=self.blocks_count.checked_sub(self.first_data_block as u64).ok_or(Error::InvalidGeometry)?;let n=n.checked_add(self.blocks_per_group as u64-1).ok_or(Error::OutOfRange)?/(self.blocks_per_group as u64);if n==0||n>u32::MAX as u64{Err(Error::InvalidGeometry)}else{Ok(n as u32)}}
 pub fn inode_location(&self,ino:u32)->Result<(u32,u32),Error>{if ino==0||ino>self.inodes_count{return Err(Error::OutOfRange)}let z=ino-1;let g=z/self.inodes_per_group;if g>=self.group_count()?{return Err(Error::OutOfRange)}Ok((g,(z%self.inodes_per_group).checked_mul(self.inode_size as u32).ok_or(Error::OutOfRange)?))}
 pub fn has_extents(&self)->bool{self.feature_incompat&INCOMPAT_EXTENTS!=0}
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct GroupDescriptor{pub block_bitmap:u64,pub inode_bitmap:u64,pub inode_table:u64,pub free_blocks_count:u32,pub free_inodes_count:u32}
impl GroupDescriptor{pub fn parse(b:&[u8],sb:&Superblock)->Result<Self,Error>{if b.len()<sb.descriptor_size as usize{return Err(Error::Truncated)}let wide=sb.feature_incompat&INCOMPAT_64BIT!=0;let bb=u32at(b,0)? as u64;let ib=u32at(b,4)? as u64;let it=u32at(b,8)? as u64;let (bb_hi,ib_hi,it_hi,fb_hi,fi_hi)=if wide{(u16at(b,32)? as u64,u16at(b,36)? as u64,u16at(b,40)? as u64,u16at(b,44)? as u32,u16at(b,46)? as u32)}else{(0,0,0,0,0)};let d=Self{block_bitmap:bb|(bb_hi<<32),inode_bitmap:ib|(ib_hi<<32),inode_table:it|(it_hi<<32),free_blocks_count:u16at(b,12)? as u32|(fb_hi<<16),free_inodes_count:u16at(b,14)? as u32|(fi_hi<<16)};if d.block_bitmap>=sb.blocks_count||d.inode_bitmap>=sb.blocks_count||d.inode_table>=sb.blocks_count{return Err(Error::OutOfRange)}Ok(d)}}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct Inode{pub mode:u16,pub size:u64,pub blocks_512:u32,pub flags:u32,pub is_directory:bool,pub is_regular_file:bool,pub is_symlink:bool}
impl Inode{pub fn parse(b:&[u8],size:u16,revision:u32,ro_compat:u32)->Result<Self,Error>{if size<128||b.len()<size as usize{return Err(Error::Truncated)}let mode=u16at(b,0)?;let size_lo=u32at(b,4)? as u64;let blocks_512=u32at(b,28)?;let flags=u32at(b,32)?;let is_regular_file=mode&0xf000==0x8000;let is_directory=mode&0xf000==0x4000;let is_symlink=mode&0xf000==0xa000;let hi=if is_regular_file&&revision!=0&&ro_compat&2!=0&&size>=112{u32at(b,108)? as u64}else{0};Ok(Self{mode,size:(hi<<32)|size_lo,blocks_512,flags,is_directory,is_regular_file,is_symlink})}}
pub fn validate_directory_block(b:&[u8],total_inodes:u32)->Result<usize,Error>{if b.is_empty(){return Err(Error::Truncated)}let(mut off,mut count)=(0usize,0usize);while off<b.len(){if b.len()-off<8{return Err(Error::InvalidDirectoryEntry)}let ino=u32at(b,off)?;let rec=u16at(b,off+4)? as usize;let n=b[off+6] as usize;if rec<8||rec&3!=0||rec>b.len()-off||n>rec-8||(ino!=0&&ino>total_inodes){return Err(Error::InvalidDirectoryEntry)}if b[off+8..off+8+n].iter().any(|c|*c==0||*c==b'/'){return Err(Error::InvalidDirectoryEntry)}count+=1;off+=rec;}Ok(count)}
pub trait ReadOnlyBlockDevice{type DeviceError;fn capacity_bytes(&self)->u64;fn read_at(&mut self,offset:u64,out:&mut[u8])->Result<(),Self::DeviceError>;}
/// Writable block-device contract. Implementations serialize writes and flush.
pub trait BlockDevice:ReadOnlyBlockDevice{
 fn write_at(&mut self,offset:u64,data:&[u8])->Result<(),Self::DeviceError>;
 fn flush(&mut self)->Result<(),Self::DeviceError>;
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum WriteError{Filesystem(Error),ReadOnly,OutOfRange,Io,PartialBlock}
/// Driver facade; write support intentionally excludes allocation and metadata.
pub struct ExtFilesystem<D>{device:D,superblock:Superblock,read_only:bool}
impl<D:ReadOnlyBlockDevice> ExtFilesystem<D>{
 pub fn mount_read_only(mut device:D)->Result<Self,Error>{let sb=read_superblock(&mut device)?;Ok(Self{device,superblock:sb,read_only:true})}
 /// Mount a filesystem from a bounded partition view of a whole-disk device.
 pub fn mount_partition_read_only(device:D,partition:DiskPartition)->Result<ExtFilesystem<PartitionDevice<D>>,Error>{
  let view=PartitionDevice::new(device,partition)?;
  ExtFilesystem::mount_read_only(view)
 }
 pub fn superblock(&self)->&Superblock{&self.superblock}
 pub fn into_device(self)->D{self.device}
}
impl<D:BlockDevice> ExtFilesystem<D>{
 /// Mount writable only through a partition-bounded block-device view. The
 /// existing filesystem policy still rejects unsupported journaling/checksum
 /// features and does not allocate blocks or mutate metadata.
 pub fn mount_partition_writable(device:D,partition:DiskPartition)->Result<ExtFilesystem<PartitionDevice<D>>,Error>{
  let view=PartitionDevice::new(device,partition)?;
  ExtFilesystem::mount_writable(view)
 }
 pub fn mount_writable(mut device:D)->Result<Self,Error>{
  let mut raw=[0u8;SUPERBLOCK_BYTES];device.read_at(SUPERBLOCK_DISK_OFFSET,&mut raw).map_err(|_|Error::Io)?;
  let sb=Superblock::parse(&raw)?;validate_superblock_checksum(&raw)?;
  if sb.feature_compat&0x0004!=0{return Err(Error::UnsupportedCompatFeature)}
  if sb.feature_incompat&0x0004!=0{return Err(Error::UnsupportedIncompatFeature)}
  if sb.feature_ro_compat&0x0400!=0{return Err(Error::UnsupportedRoCompatFeature)}
  let end=sb.blocks_count.checked_mul(sb.block_size as u64).ok_or(Error::OutOfRange)?;
  if end>device.capacity_bytes(){return Err(Error::OutOfRange)}
  Ok(Self{device,superblock:sb,read_only:false})
 }
 /// Write within an existing regular file's current logical size. This prevents
 /// a data-write request from using a valid extent beyond EOF to overwrite data
 /// that belongs to another file or filesystem structure. Allocation and inode
 /// size changes are intentionally not performed here.
 pub fn write_file_data(&mut self,extent_root:&[u8],inode_size:u64,file_offset:u64,data:&[u8],scratch:&mut[u8])->Result<usize,WriteError>{
  let end=file_offset.checked_add(data.len() as u64).ok_or(WriteError::OutOfRange)?;
  if end>inode_size{return Err(WriteError::OutOfRange)}
  self.write_existing_extent_unchecked(extent_root,file_offset,data,scratch)
 }
 fn write_existing_extent_unchecked(&mut self,extent_root:&[u8],file_offset:u64,data:&[u8],scratch:&mut[u8])->Result<usize,WriteError>{
  if self.read_only{return Err(WriteError::ReadOnly)}if data.is_empty(){return Ok(0)}
  let bs=self.superblock.block_size as usize;if scratch.len()!=bs{return Err(WriteError::PartialBlock)}
  let mut written=0usize;
  while written<data.len(){
   let absolute=file_offset.checked_add(written as u64).ok_or(WriteError::OutOfRange)?;
   let logical=absolute/(bs as u64);if logical>u32::MAX as u64{return Err(WriteError::OutOfRange)}
   let within=(absolute%(bs as u64)) as usize;let amount=core::cmp::min(bs-within,data.len()-written);
   let physical=map_extent_block(&mut self.device,&self.superblock,extent_root,logical as u32,scratch).map_err(WriteError::Filesystem)?.ok_or(WriteError::OutOfRange)?;
   let block_offset=physical.checked_mul(bs as u64).ok_or(WriteError::OutOfRange)?;
   if within!=0||amount!=bs{read_block(&mut self.device,&self.superblock,physical,scratch).map_err(WriteError::Filesystem)?;}
   scratch[within..within+amount].copy_from_slice(&data[written..written+amount]);
   self.device.write_at(block_offset,scratch).map_err(|_|WriteError::Io)?;written+=amount;
  }
  self.device.flush().map_err(|_|WriteError::Io)?;Ok(written)
 }
}

pub fn read_superblock<D:ReadOnlyBlockDevice>(dev:&mut D)->Result<Superblock,Error>{let end=SUPERBLOCK_DISK_OFFSET.checked_add(SUPERBLOCK_BYTES as u64).ok_or(Error::OutOfRange)?;if dev.capacity_bytes()<end{return Err(Error::OutOfRange)}let mut b=[0u8;SUPERBLOCK_BYTES];dev.read_at(SUPERBLOCK_DISK_OFFSET,&mut b).map_err(|_|Error::Io)?;let sb=Superblock::parse(&b)?;let fs_end=sb.blocks_count.checked_mul(sb.block_size as u64).ok_or(Error::OutOfRange)?;if fs_end>dev.capacity_bytes(){return Err(Error::OutOfRange)}Ok(sb)}

/// Reads one complete filesystem block. The device read is rejected if either
/// the filesystem geometry or the physical device capacity would be exceeded.
pub fn read_block<D:ReadOnlyBlockDevice>(dev:&mut D,sb:&Superblock,block:u64,out:&mut[u8])->Result<(),Error>{
 if out.len()!=sb.block_size as usize||block>=sb.blocks_count{return Err(Error::OutOfRange)}
 let offset=block.checked_mul(sb.block_size as u64).ok_or(Error::OutOfRange)?;
 let end=offset.checked_add(out.len() as u64).ok_or(Error::OutOfRange)?;
 let fs_end=(sb.blocks_count as u64).checked_mul(sb.block_size as u64).ok_or(Error::OutOfRange)?;
 if end>fs_end||end>dev.capacity_bytes(){return Err(Error::OutOfRange)}
 dev.read_at(offset,out).map_err(|_|Error::Io)
}

const EXTENT_MAGIC:u16=0xf30a;
const EXTENT_HEADER_SIZE:usize=12;
const EXTENT_ENTRY_SIZE:usize=12;
const MAX_EXTENT_DEPTH:u16=5;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
enum ExtentStep{Found(u64),ZeroOrHole,Child(u64)}
fn inspect_extent_node(node:&[u8],logical:u32,sb:&Superblock,expected_depth:Option<u16>)->Result<(u16,ExtentStep),Error>{
 if node.len()<EXTENT_HEADER_SIZE||u16at(node,0)?!=EXTENT_MAGIC{return Err(Error::InvalidExtentTree)}
 let entries=u16at(node,2)? as usize;
 let max=u16at(node,4)? as usize;
 let depth=u16at(node,6)?;
 let capacity=(node.len()-EXTENT_HEADER_SIZE)/EXTENT_ENTRY_SIZE;
 if depth>MAX_EXTENT_DEPTH||expected_depth.map_or(false,|d|d!=depth)||max==0||max>capacity||entries>max{return Err(Error::InvalidExtentTree)}
 if entries==0{return Ok((depth,ExtentStep::ZeroOrHole))}
 if depth==0{
  let mut previous_end:Option<u64>=None;
  let mut result=ExtentStep::ZeroOrHole;
  for i in 0..entries{
   let o=EXTENT_HEADER_SIZE+i*EXTENT_ENTRY_SIZE;
   let first=u32at(node,o)?;
   let raw_len=u16at(node,o+4)?;
   if raw_len==0{return Err(Error::InvalidExtentTree)}
   let uninitialized=raw_len>0x8000;
   let len=if uninitialized{raw_len-0x8000}else{raw_len} as u64;
   let physical=((u16at(node,o+6)? as u64)<<32)|(u32at(node,o+8)? as u64);
   let logical_end=(first as u64).checked_add(len).ok_or(Error::OutOfRange)?;
   let physical_end=physical.checked_add(len).ok_or(Error::OutOfRange)?;
   if previous_end.map_or(false,|end|(first as u64) < end)||physical==0||physical_end>sb.blocks_count{return Err(Error::InvalidExtentTree)}
   previous_end=Some(logical_end);
   if logical>=first&&(logical as u64)<logical_end{
    result=if uninitialized{ExtentStep::ZeroOrHole}else{ExtentStep::Found(physical+(logical-first) as u64)};
   }
  }
  Ok((depth,result))
 }else{
  let mut previous_key:Option<u32>=None;
  let mut selected:Option<u64>=None;
  for i in 0..entries{
   let o=EXTENT_HEADER_SIZE+i*EXTENT_ENTRY_SIZE;
   let key=u32at(node,o)?;
   let child=((u16at(node,o+8)? as u64)<<32)|(u32at(node,o+4)? as u64);
   if previous_key.map_or(false,|prev|key<=prev)||child==0||child>=sb.blocks_count{return Err(Error::InvalidExtentTree)}
   previous_key=Some(key);
   if key<=logical{selected=Some(child)}
  }
  Ok((depth,selected.map(ExtentStep::Child).unwrap_or(ExtentStep::ZeroOrHole)))
 }
}

/// Maps a logical file block using the inode's 60-byte extent root. scratch
/// must be one filesystem block. None denotes a hole or an uninitialized
/// extent; the file-read layer must return zeroes for either case. No writes
/// or journal replay are performed.
pub fn map_extent_block<D:ReadOnlyBlockDevice>(dev:&mut D,sb:&Superblock,root:&[u8],logical:u32,scratch:&mut[u8])->Result<Option<u64>,Error>{
 if !sb.has_extents()||root.len()<60||scratch.len()!=sb.block_size as usize{return Err(Error::UnsupportedFormat)}
 let mut is_root=true;
 let mut expected_depth=None;
 for _ in 0..=MAX_EXTENT_DEPTH{
  let node=if is_root{&root[..60]}else{&scratch[..]};
  let (depth,step)=inspect_extent_node(node,logical,sb,expected_depth)?;
  match step{
   ExtentStep::Found(block)=>return Ok(Some(block)),
   ExtentStep::ZeroOrHole=>return Ok(None),
   ExtentStep::Child(child)=>{
    if depth==0{return Err(Error::InvalidExtentTree)}
    read_block(dev,sb,child,scratch)?;
    expected_depth=Some(depth-1);
    is_root=false;
   }
  }
 }
 Err(Error::InvalidExtentTree)
}

pub fn self_test()->bool{let mut b=[0u8;SUPERBLOCK_BYTES];b[0..4].copy_from_slice(&512u32.to_le_bytes());b[4..8].copy_from_slice(&4096u32.to_le_bytes());b[20..24].copy_from_slice(&1u32.to_le_bytes());b[32..36].copy_from_slice(&1024u32.to_le_bytes());b[40..44].copy_from_slice(&128u32.to_le_bytes());b[56..58].copy_from_slice(&EXT_MAGIC.to_le_bytes());b[76..80].copy_from_slice(&1u32.to_le_bytes());b[88..90].copy_from_slice(&128u16.to_le_bytes());b[96..100].copy_from_slice(&(INCOMPAT_FILETYPE|INCOMPAT_EXTENTS).to_le_bytes());let s=match Superblock::parse(&b){Ok(s)=>s,Err(_)=>return false};if s.block_size!=1024||s.inode_size!=128||s.group_count()!=Ok(4)||s.inode_location(129)!=Ok((1,0))||!s.read_only{return false}let mut wide=b;wide[96..100].copy_from_slice(&(INCOMPAT_FILETYPE|INCOMPAT_EXTENTS|INCOMPAT_64BIT).to_le_bytes());wide[254..256].copy_from_slice(&64u16.to_le_bytes());wide[336..340].copy_from_slice(&1u32.to_le_bytes());let wide_sb=match Superblock::parse(&wide){Ok(v) if v.blocks_count==0x1_0000_1000 && v.descriptor_size==64=>v,_=>return false};let mut gd=[0u8;64];gd[0..4].copy_from_slice(&1u32.to_le_bytes());gd[4..8].copy_from_slice(&2u32.to_le_bytes());gd[8..12].copy_from_slice(&3u32.to_le_bytes());gd[12..14].copy_from_slice(&5u16.to_le_bytes());gd[14..16].copy_from_slice(&6u16.to_le_bytes());gd[32..34].copy_from_slice(&1u16.to_le_bytes());gd[36..38].copy_from_slice(&1u16.to_le_bytes());gd[40..42].copy_from_slice(&1u16.to_le_bytes());gd[44..46].copy_from_slice(&1u16.to_le_bytes());gd[46..48].copy_from_slice(&1u16.to_le_bytes());if !matches!(GroupDescriptor::parse(&gd,&wide_sb),Ok(g) if g.block_bitmap==0x1_0000_0001 && g.inode_bitmap==0x1_0000_0002 && g.inode_table==0x1_0000_0003 && g.free_blocks_count==0x1_0005 && g.free_inodes_count==0x1_0006){return false}let mut bad=b;bad[96..100].copy_from_slice(&0x8000_0000u32.to_le_bytes());if Superblock::parse(&bad)!=Err(Error::UnsupportedIncompatFeature){return false}let mut d=[0u8;12];d[0..4].copy_from_slice(&2u32.to_le_bytes());d[4..6].copy_from_slice(&12u16.to_le_bytes());d[6]=4;d[8..12].copy_from_slice(b"test");if validate_directory_block(&d,512)!=Ok(1){return false}d[4..6].copy_from_slice(&0u16.to_le_bytes());if validate_directory_block(&d,512)!=Err(Error::InvalidDirectoryEntry){return false}
 let mut root=[0u8;60];root[0..2].copy_from_slice(&EXTENT_MAGIC.to_le_bytes());root[2..4].copy_from_slice(&1u16.to_le_bytes());root[4..6].copy_from_slice(&4u16.to_le_bytes());root[6..8].copy_from_slice(&0u16.to_le_bytes());root[12..16].copy_from_slice(&7u32.to_le_bytes());root[16..18].copy_from_slice(&2u16.to_le_bytes());root[20..24].copy_from_slice(&100u32.to_le_bytes());
 struct NoRead;impl ReadOnlyBlockDevice for NoRead{type DeviceError=();fn capacity_bytes(&self)->u64{4096*1024}fn read_at(&mut self,_:u64,_:&mut[u8])->Result<(),Self::DeviceError>{Err(())}}
 let mut dev=NoRead;let mut scratch=[0u8;1024];if map_extent_block(&mut dev,&s,&root,8,&mut scratch)!=Ok(Some(101)){return false}if map_extent_block(&mut dev,&s,&root,20,&mut scratch)!=Ok(None){return false}
 root[0..2].copy_from_slice(&0u16.to_le_bytes());inspect_extent_node(&root,8,&s,Some(0))==Err(Error::InvalidExtentTree) && legacy_mapping_self_test(&s) && crc32c(!0u32,b"123456789")==0x1cf9_6d7c && validate_superblock_checksum(&b)==Ok(())}


/// Resolve an ext2/ext3 legacy block map (12 direct, single, double and
/// triple indirect pointers). This is needed for older ext2/ext3 volumes and
/// inodes without EXTENTS. The caller supplies two block-sized scratch buffers
/// so indirect levels can be traversed without allocation. A zero pointer is a
/// sparse hole and must be returned as zero-filled file data by the caller.
pub fn map_legacy_block<D: ReadOnlyBlockDevice>(
    dev: &mut D,
    sb: &Superblock,
    inode_i_block: &[u8],
    logical: u64,
    scratch_a: &mut [u8],
    scratch_b: &mut [u8],
) -> Result<Option<u64>, Error> {
    if inode_i_block.len() < 60
        || scratch_a.len() != sb.block_size as usize
        || scratch_b.len() != sb.block_size as usize
        || sb.block_size < 1024
        || sb.block_size % 4 != 0
    {
        return Err(Error::UnsupportedFormat);
    }
    let pointers_per_block = (sb.block_size / 4) as u64;
    let direct_count = 12u64;
    let single_span = pointers_per_block;
    let double_span = pointers_per_block.checked_mul(pointers_per_block).ok_or(Error::OutOfRange)?;
    let triple_span = double_span.checked_mul(pointers_per_block).ok_or(Error::OutOfRange)?;
    let (root_slot, depth, mut index) = if logical < direct_count {
        let p = u32at(inode_i_block, logical as usize * 4)? as u64;
        return validate_legacy_data_pointer(sb, p);
    } else {
        let n = logical - direct_count;
        if n < single_span {
            (12usize, 1u32, n)
        } else if n - single_span < double_span {
            (13usize, 2u32, n - single_span)
        } else if n - single_span - double_span < triple_span {
            (14usize, 3u32, n - single_span - double_span)
        } else {
            return Err(Error::OutOfRange);
        }
    };

    let mut divisor = 1u64;
    for _ in 1..depth {
        divisor = divisor.checked_mul(pointers_per_block).ok_or(Error::OutOfRange)?;
    }
    let mut indices = [0usize; 3];
    for level in 0..depth as usize {
        indices[level] = (index / divisor) as usize;
        index %= divisor;
        if level + 1 < depth as usize {
            divisor /= pointers_per_block;
        }
    }

    let mut block = u32at(inode_i_block, root_slot * 4)? as u64;
    if block == 0 {
        return Ok(None);
    }
    if block >= sb.blocks_count {
        return Err(Error::OutOfRange);
    }
    let mut use_a = true;
    for level in 0..depth as usize {
        let target = if use_a { &mut *scratch_a } else { &mut *scratch_b };
        read_block(dev, sb, block, target)?;
        let entry_offset = indices[level].checked_mul(4).ok_or(Error::OutOfRange)?;
        block = u32at(target, entry_offset)? as u64;
        if block == 0 {
            return Ok(None);
        }
        if block >= sb.blocks_count {
            return Err(Error::OutOfRange);
        }
        if level + 1 == depth as usize {
            return Ok(Some(block));
        }
        use_a = !use_a;
    }
    Err(Error::OutOfRange)
}

fn validate_legacy_data_pointer(sb: &Superblock, block: u64) -> Result<Option<u64>, Error> {
    if block == 0 {
        Ok(None)
    } else if block >= sb.blocks_count {
        Err(Error::OutOfRange)
    } else {
        Ok(Some(block))
    }
}

fn legacy_mapping_self_test(sb: &Superblock) -> bool {
    struct FixtureDevice;
    impl ReadOnlyBlockDevice for FixtureDevice {
        type DeviceError = ();
        fn capacity_bytes(&self) -> u64 { 4096 * 1024 }
        fn read_at(&mut self, offset: u64, out: &mut [u8]) -> Result<(), Self::DeviceError> {
            out.fill(0);
            if out.len() != 1024 || offset % 1024 != 0 { return Err(()); }
            let block = offset / 1024;
            let put = |out: &mut [u8], slot: usize, value: u32| {
                out[slot * 4..slot * 4 + 4].copy_from_slice(&value.to_le_bytes());
            };
            match block {
                20 => { put(out, 0, 30); put(out, 1, 31); }
                21 => put(out, 0, 22),
                22 => put(out, 0, 32),
                23 => put(out, 0, 24),
                24 => put(out, 0, 25),
                25 => put(out, 0, 33),
                _ => {}
            }
            Ok(())
        }
    }
    let mut dev = FixtureDevice;
    let mut inode = [0u8; 60];
    inode[0..4].copy_from_slice(&15u32.to_le_bytes());
    inode[12 * 4..13 * 4].copy_from_slice(&20u32.to_le_bytes());
    inode[13 * 4..14 * 4].copy_from_slice(&21u32.to_le_bytes());
    inode[14 * 4..15 * 4].copy_from_slice(&23u32.to_le_bytes());
    let mut a = [0u8; 1024];
    let mut b = [0u8; 1024];
    if map_legacy_block(&mut dev, sb, &inode, 0, &mut a, &mut b) != Ok(Some(15)) { return false; }
    if map_legacy_block(&mut dev, sb, &inode, 12, &mut a, &mut b) != Ok(Some(30)) { return false; }
    if map_legacy_block(&mut dev, sb, &inode, 13, &mut a, &mut b) != Ok(Some(31)) { return false; }
    if map_legacy_block(&mut dev, sb, &inode, 12 + 256, &mut a, &mut b) != Ok(Some(32)) { return false; }
    if map_legacy_block(&mut dev, sb, &inode, 12 + 256 + 256 * 256, &mut a, &mut b) != Ok(Some(33)) { return false; }
    inode[12 * 4..13 * 4].copy_from_slice(&0u32.to_le_bytes());
    if map_legacy_block(&mut dev, sb, &inode, 12, &mut a, &mut b) != Ok(None) { return false; }
    inode[0..4].copy_from_slice(&4096u32.to_le_bytes());
    map_legacy_block(&mut dev, sb, &inode, 0, &mut a, &mut b) == Err(Error::OutOfRange)
}


/// Reflected CRC32C (Castagnoli) used by ext4 metadata checksums. The seed
/// is passed through unchanged and there is no implicit final XOR, matching
/// Linux's crc32c(seed, data, len) convention.
pub fn crc32c(seed: u32, bytes: &[u8]) -> u32 {
    let mut crc = seed;
    for byte in bytes {
        crc ^= *byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0x82f6_3b78 } else { crc >> 1 };
        }
    }
    crc
}

/// Verify the ext4 superblock checksum when metadata_csum is enabled.
/// This does not make the filesystem mountable by itself: group descriptors,
/// inode records, extent blocks, directory blocks and journal records also
/// need their own feature-correct validation before being consumed.
pub fn validate_superblock_checksum(raw: &[u8]) -> Result<(), Error> {
    const RO_COMPAT_METADATA_CSUM: u32 = 0x0400;
    const CHECKSUM_TYPE_OFFSET: usize = 373;
    const CHECKSUM_OFFSET: usize = 1020;
    if raw.len() < SUPERBLOCK_BYTES { return Err(Error::Truncated); }
    let ro_compat = u32at(raw, 100)?;
    if ro_compat & RO_COMPAT_METADATA_CSUM == 0 { return Ok(()); }
    if raw[CHECKSUM_TYPE_OFFSET] != 1 { return Err(Error::UnsupportedFormat); }
    let stored = u32at(raw, CHECKSUM_OFFSET)?;
    let calculated = crc32c(!0u32, &raw[..CHECKSUM_OFFSET]);
    if stored == calculated { Ok(()) } else { Err(Error::InvalidChecksum) }
}


/// Checked half-open partition extent, expressed in logical sectors.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct DiskPartition { pub first_lba:u64, pub sector_count:u64, pub sector_size:u32 }
impl DiskPartition {
    pub fn validate<D:ReadOnlyBlockDevice>(&self,device:&D)->Result<(),Error>{
        if self.sector_size<512||!self.sector_size.is_power_of_two()||self.sector_count==0{return Err(Error::InvalidGeometry)}
        let start=self.first_lba.checked_mul(self.sector_size as u64).ok_or(Error::OutOfRange)?;
        let bytes=self.sector_count.checked_mul(self.sector_size as u64).ok_or(Error::OutOfRange)?;
        if start.checked_add(bytes).ok_or(Error::OutOfRange)?>device.capacity_bytes(){return Err(Error::OutOfRange)}
        Ok(())
    }
    pub fn start_byte(&self)->Result<u64,Error>{self.first_lba.checked_mul(self.sector_size as u64).ok_or(Error::OutOfRange)}
    pub fn capacity_bytes(&self)->Result<u64,Error>{self.sector_count.checked_mul(self.sector_size as u64).ok_or(Error::OutOfRange)}
}

/// Read a primary MBR partition by zero-based slot. Extended/logical partition
/// chains are deliberately rejected; use GPT on modern disks.
pub fn read_mbr_partition<D:ReadOnlyBlockDevice>(device:&mut D,slot:usize,sector_size:u32)->Result<DiskPartition,Error>{
    if slot>=4||sector_size<512||!sector_size.is_power_of_two(){return Err(Error::InvalidGeometry)}
    if device.capacity_bytes()<sector_size as u64{return Err(Error::Truncated)}
    let mut sector=[0u8;512];
    device.read_at(0,&mut sector).map_err(|_|Error::Io)?;
    if sector[510]!=0x55||sector[511]!=0xaa{return Err(Error::InvalidPartitionTable)}
    let o=446+slot*16;let kind=sector[o+4];
    let first=u32::from_le_bytes([sector[o+8],sector[o+9],sector[o+10],sector[o+11]]) as u64;
    let count=u32::from_le_bytes([sector[o+12],sector[o+13],sector[o+14],sector[o+15]]) as u64;
    if kind==0||count==0{return Err(Error::InvalidPartitionTable)}
    if matches!(kind,0x05|0x0f|0x85){return Err(Error::UnsupportedPartitionTable)}
    let p=DiskPartition{first_lba:first,sector_count:count,sector_size};p.validate(device)?;Ok(p)
}

/// Read and validate the four primary MBR entries. Extended/logical and
/// protective-GPT layouts are rejected by mutation helpers: changing only the
/// primary table in those formats would leave the partition chain inconsistent.
fn read_mbr_sector<D: ReadOnlyBlockDevice>(
    device: &mut D, sector_size: u32, sector: &mut [u8;4096],
) -> Result<(),Error> {
    if !(512..=4096).contains(&sector_size) || !sector_size.is_power_of_two() {
        return Err(Error::InvalidGeometry);
    }
    if device.capacity_bytes() < sector_size as u64 { return Err(Error::Truncated); }
    device.read_at(0,&mut sector[..sector_size as usize]).map_err(|_|Error::Io)?;
    if sector[510]!=0x55 || sector[511]!=0xaa { return Err(Error::InvalidPartitionTable); }
    for slot in 0..4 {
        let o=446+slot*16;
        let status=sector[o];
        let kind=sector[o+4];
        if status!=0 && status!=0x80 { return Err(Error::InvalidPartitionTable); }
        if kind==0 {
            if sector[o+1..o+16].iter().any(|b|*b!=0) {
                return Err(Error::InvalidPartitionTable);
            }
            continue;
        }
        if matches!(kind,0x05|0x0f|0x85|0xee) {
            return Err(Error::UnsupportedPartitionTable);
        }
        let first=u32at(sector,o+8)? as u64;
        let count=u32at(sector,o+12)? as u64;
        if first==0 || count==0 { return Err(Error::InvalidPartitionTable); }
        let end=first.checked_add(count).ok_or(Error::OutOfRange)?;
        if end>device.capacity_bytes()/(sector_size as u64) || end>u32::MAX as u64+1 {
            return Err(Error::OutOfRange);
        }
        for other in 0..slot {
            let p=446+other*16;
            if sector[p+4]==0 { continue; }
            let a=u32at(sector,p+8)? as u64;
            let n=u32at(sector,p+12)? as u64;
            let b=a.checked_add(n).ok_or(Error::OutOfRange)?;
            if first<b && a<end { return Err(Error::InvalidPartitionTable); }
        }
    }
    Ok(())
}

fn write_mbr_sector<D: BlockDevice>(
    device: &mut D, sector_size: u32, sector: &[u8;4096],
) -> Result<(),Error> {
    device.write_at(0,&sector[..sector_size as usize]).map_err(|_|Error::Io)?;
    device.flush().map_err(|_|Error::Io)
}

/// Create a primary MBR partition in the first unused slot. The partition
/// type must be a non-extended, non-protective type. MBR has no redundant
/// table or checksum, so this operation cannot provide GPT-like power-loss
/// atomicity; callers should prefer GPT on modern disks.
pub fn create_mbr_partition<D: BlockDevice>(
    device: &mut D, sector_size: u32, partition_type: u8,
    first_lba: u64, sector_count: u64, bootable: bool,
) -> Result<u8,Error> {
    if !(512..=4096).contains(&sector_size) || !sector_size.is_power_of_two() {
        return Err(Error::InvalidGeometry);
    }
    if partition_type==0 || matches!(partition_type,0x05|0x0f|0x85|0xee)
        || first_lba==0 || sector_count==0 {
        return Err(Error::InvalidPartitionTable);
    }
    let end=first_lba.checked_add(sector_count).ok_or(Error::OutOfRange)?;
    if first_lba>u32::MAX as u64 || sector_count>u32::MAX as u64
        || end>u32::MAX as u64+1
        || end>device.capacity_bytes()/(sector_size as u64) {
        return Err(Error::OutOfRange);
    }
    let mut sector=[0u8;4096];
    read_mbr_sector(device,sector_size,&mut sector)?;
    for slot in 0..4 {
        let o=446+slot*16;
        if sector[o+4]==0 { continue; }
        let first=u32at(&sector,o+8)? as u64;
        let count=u32at(&sector,o+12)? as u64;
        let old_end=first.checked_add(count).ok_or(Error::OutOfRange)?;
        if first<end && first_lba<old_end { return Err(Error::InvalidPartitionTable); }
    }
    let slot=(0..4).find(|i|sector[446+i*16+4]==0).ok_or(Error::OutOfRange)?;
    let o=446+slot*16;
    sector[o..o+16].fill(0);
    sector[o]=if bootable {0x80} else {0};
    sector[o+1..o+4].copy_from_slice(&[0xfe,0xff,0xff]);
    sector[o+4]=partition_type;
    sector[o+5..o+8].copy_from_slice(&[0xfe,0xff,0xff]);
    sector[o+8..o+12].copy_from_slice(&(first_lba as u32).to_le_bytes());
    sector[o+12..o+16].copy_from_slice(&(sector_count as u32).to_le_bytes());
    write_mbr_sector(device,sector_size,&sector)?;
    Ok(slot as u8)
}

/// Remove a primary MBR entry without erasing the partition's contents.
pub fn delete_mbr_partition<D: BlockDevice>(
    device: &mut D, sector_size: u32, slot: u8,
) -> Result<(),Error> {
    if slot>=4 { return Err(Error::OutOfRange); }
    let mut sector=[0u8;4096];
    read_mbr_sector(device,sector_size,&mut sector)?;
    let o=446+slot as usize*16;
    if sector[o+4]==0 { return Err(Error::InvalidPartitionTable); }
    sector[o..o+16].fill(0);
    write_mbr_sector(device,sector_size,&sector)
}

/// Extend a primary MBR partition in place. Shrinking and moving its start
/// are not allowed; all neighbouring primary partitions are checked.
pub fn extend_mbr_partition<D: BlockDevice>(
    device: &mut D, sector_size: u32, slot: u8, new_sector_count: u64,
) -> Result<DiskPartition,Error> {
    if slot>=4 || new_sector_count==0 || new_sector_count>u32::MAX as u64 {
        return Err(Error::OutOfRange);
    }
    let mut sector=[0u8;4096];
    read_mbr_sector(device,sector_size,&mut sector)?;
    let o=446+slot as usize*16;
    if sector[o+4]==0 { return Err(Error::InvalidPartitionTable); }
    let first=u32at(&sector,o+8)? as u64;
    let old_count=u32at(&sector,o+12)? as u64;
    if new_sector_count<=old_count { return Err(Error::InvalidGeometry); }
    let end=first.checked_add(new_sector_count).ok_or(Error::OutOfRange)?;
    if end>u32::MAX as u64+1 || end>device.capacity_bytes()/(sector_size as u64) {
        return Err(Error::OutOfRange);
    }
    for other in 0..4 {
        if other==slot as usize { continue; }
        let p=446+other*16;
        if sector[p+4]==0 { continue; }
        let a=u32at(&sector,p+8)? as u64;
        let n=u32at(&sector,p+12)? as u64;
        let b=a.checked_add(n).ok_or(Error::OutOfRange)?;
        if first<b && a<end { return Err(Error::InvalidPartitionTable); }
    }
    sector[o+12..o+16].copy_from_slice(&(new_sector_count as u32).to_le_bytes());
    write_mbr_sector(device,sector_size,&sector)?;
    Ok(DiskPartition{first_lba:first,sector_count:new_sector_count,sector_size})
}

/// IEEE CRC-32 used by GPT (not ext4's CRC32C).
fn crc32_ieee_extend(mut crc:u32,bytes:&[u8])->u32{
    for byte in bytes{crc^=*byte as u32;for _ in 0..8{crc=if crc&1!=0{(crc>>1)^0xedb8_8320}else{crc>>1};}}
    crc
}
pub fn crc32_ieee(bytes:&[u8])->u32{!crc32_ieee_extend(!0u32,bytes)}

/// Validate the primary GPT header and entry-array CRCs and return one
/// zero-based partition entry. Bounds are capped to 128 standard 128-byte
/// entries to keep early-boot memory use predictable.
pub fn read_gpt_partition<D:ReadOnlyBlockDevice>(device:&mut D,slot:u32,sector_size:u32)->Result<DiskPartition,Error>{
    if sector_size<512||sector_size>4096||!sector_size.is_power_of_two(){return Err(Error::InvalidGeometry)}
    let disk_sectors=device.capacity_bytes()/(sector_size as u64);
    if disk_sectors<3{return Err(Error::Truncated)}
    let ss=sector_size as usize;let mut sector=[0u8;4096];
    device.read_at(sector_size as u64,&mut sector[..ss]).map_err(|_|Error::Io)?;
    if &sector[..8]!=b"EFI PART"{return Err(Error::InvalidPartitionTable)}
    let header_size=u32::from_le_bytes(sector[12..16].try_into().map_err(|_|Error::Truncated)?) as usize;
    if header_size<92||header_size>ss{return Err(Error::InvalidPartitionTable)}
    let stored=u32::from_le_bytes(sector[16..20].try_into().map_err(|_|Error::Truncated)?);
    let mut copy=[0u8;4096];copy[..header_size].copy_from_slice(&sector[..header_size]);copy[16..20].fill(0);
    if crc32_ieee(&copy[..header_size])!=stored{return Err(Error::InvalidChecksum)}
    let current=u64::from_le_bytes(sector[24..32].try_into().map_err(|_|Error::Truncated)?);
    let backup=u64::from_le_bytes(sector[32..40].try_into().map_err(|_|Error::Truncated)?);
    let first_usable=u64::from_le_bytes(sector[40..48].try_into().map_err(|_|Error::Truncated)?);
    let last_usable=u64::from_le_bytes(sector[48..56].try_into().map_err(|_|Error::Truncated)?);
    let entries_lba=u64::from_le_bytes(sector[72..80].try_into().map_err(|_|Error::Truncated)?);
    let count=u32::from_le_bytes(sector[80..84].try_into().map_err(|_|Error::Truncated)?);
    let size=u32::from_le_bytes(sector[84..88].try_into().map_err(|_|Error::Truncated)?);
    let entries_crc=u32::from_le_bytes(sector[88..92].try_into().map_err(|_|Error::Truncated)?);
    if current!=1||backup!=disk_sectors-1||backup==current||first_usable>last_usable||last_usable>=disk_sectors
        ||count==0||count>128||slot>=count||size!=128||entries_lba<2 {
        return Err(Error::UnsupportedPartitionTable)
    }
    let array_len=(count as usize).checked_mul(size as usize).ok_or(Error::OutOfRange)?;
    let array_offset=entries_lba.checked_mul(sector_size as u64).ok_or(Error::OutOfRange)?;
    let array_end=array_offset.checked_add(array_len as u64).ok_or(Error::OutOfRange)?;
    let first_usable_offset=first_usable.checked_mul(sector_size as u64).ok_or(Error::OutOfRange)?;
    if array_end>device.capacity_bytes(){return Err(Error::OutOfRange)}
    if array_end>first_usable_offset{return Err(Error::InvalidPartitionTable)}
    // Stream the bounded entry array through CRC32 using the existing sector
    // buffer instead of allocating a 16 KiB table on the early kernel stack.
    let mut crc=!0u32;let mut done=0usize;
    while done<array_len {
        let n=(array_len-done).min(sector.len());
        let offset=array_offset.checked_add(done as u64).ok_or(Error::OutOfRange)?;
        device.read_at(offset,&mut sector[..n]).map_err(|_|Error::Io)?;
        crc=crc32_ieee_extend(crc,&sector[..n]);done+=n;
    }
    if !crc!=entries_crc{return Err(Error::InvalidChecksum)}
    let entry_offset=array_offset.checked_add((slot as u64).checked_mul(128).ok_or(Error::OutOfRange)?).ok_or(Error::OutOfRange)?;
    let mut entry=[0u8;128];
    device.read_at(entry_offset,&mut entry).map_err(|_|Error::Io)?;
    let e=&entry[..];
    if e[..16].iter().all(|b|*b==0){return Err(Error::InvalidPartitionTable)}
    let first=u64::from_le_bytes(e[32..40].try_into().map_err(|_|Error::Truncated)?);
    let last=u64::from_le_bytes(e[40..48].try_into().map_err(|_|Error::Truncated)?);
    if first<first_usable||last>last_usable||first>last{return Err(Error::InvalidPartitionTable)}
    let p=DiskPartition{first_lba:first,sector_count:last.checked_sub(first).and_then(|n|n.checked_add(1)).ok_or(Error::OutOfRange)?,sector_size};
    p.validate(device)?;Ok(p)
}

#[derive(Debug,PartialEq,Eq)]
pub enum PartitionIoError<E>{OutOfRange,Device(E)}

/// A block-device view restricted to a previously validated partition.
pub struct PartitionDevice<D>{device:D,partition:DiskPartition}
impl<D:ReadOnlyBlockDevice> PartitionDevice<D>{
    pub fn new(device:D,partition:DiskPartition)->Result<Self,Error>{partition.validate(&device)?;Ok(Self{device,partition})}
    pub fn partition(&self)->DiskPartition{self.partition}
    pub fn into_device(self)->D{self.device}
    fn translate(&self,offset:u64,len:usize)->Result<u64,PartitionIoError<D::DeviceError>>{
        let cap=self.partition.sector_count.checked_mul(self.partition.sector_size as u64).ok_or(PartitionIoError::OutOfRange)?;
        let end=offset.checked_add(len as u64).ok_or(PartitionIoError::OutOfRange)?;
        if end>cap{return Err(PartitionIoError::OutOfRange)}
        self.partition.first_lba.checked_mul(self.partition.sector_size as u64)
            .and_then(|base|base.checked_add(offset)).ok_or(PartitionIoError::OutOfRange)
    }
}
impl<D:ReadOnlyBlockDevice> ReadOnlyBlockDevice for PartitionDevice<D>{
    type DeviceError=PartitionIoError<D::DeviceError>;
    fn capacity_bytes(&self)->u64{self.partition.sector_count.saturating_mul(self.partition.sector_size as u64)}
    fn read_at(&mut self,offset:u64,out:&mut[u8])->Result<(),Self::DeviceError>{
        let absolute=self.translate(offset,out.len()).map_err(|e|e)?;
        self.device.read_at(absolute,out).map_err(PartitionIoError::Device)
    }
}
impl<D:BlockDevice> BlockDevice for PartitionDevice<D>{
    fn write_at(&mut self,offset:u64,data:&[u8])->Result<(),Self::DeviceError>{
        let absolute=self.translate(offset,data.len()).map_err(|e|e)?;
        self.device.write_at(absolute,data).map_err(PartitionIoError::Device)
    }
    fn flush(&mut self)->Result<(),Self::DeviceError>{self.device.flush().map_err(PartitionIoError::Device)}
}



const INCOMPAT_CSUM_SEED: u32 = 0x2000;
const RO_COMPAT_METADATA_CSUM: u32 = 0x0400;
const EXT4_SB_UUID_OFFSET: usize = 104;
const EXT4_SB_CHECKSUM_SEED_OFFSET: usize = 624;
const EXT4_GD_CHECKSUM_OFFSET: usize = 30;
const EXT4_INODE_CHECKSUM_LO_OFFSET: usize = 124;
const EXT4_INODE_CHECKSUM_HI_OFFSET: usize = 130;

/// Return the filesystem-wide metadata checksum seed. With
/// INCOMPAT_CSUM_SEED, ext4 stores the seed in s_checksum_seed; otherwise
/// it is CRC32C(~0, s_uuid), as specified by ext4.
pub fn ext4_metadata_checksum_seed(superblock: &[u8]) -> Result<u32, Error> {
    if superblock.len() < SUPERBLOCK_BYTES { return Err(Error::Truncated); }
    if u32at(superblock,100)? & RO_COMPAT_METADATA_CSUM == 0 {
        return Err(Error::UnsupportedFormat);
    }
    if superblock[373] != 1 { return Err(Error::UnsupportedFormat); }
    let incompat=u32at(superblock,96)?;
    if incompat & INCOMPAT_CSUM_SEED != 0 {
        return u32at(superblock,EXT4_SB_CHECKSUM_SEED_OFFSET);
    }
    let uuid=&superblock[EXT4_SB_UUID_OFFSET..EXT4_SB_UUID_OFFSET+16];
    Ok(crc32c(!0u32,uuid))
}

/// Compute the ext4 metadata_csum group descriptor checksum. The checksum
/// field itself (bg_checksum at offset 0x1e) is excluded from the stream.
pub fn group_descriptor_checksum(
    superblock: &[u8], group: u32, descriptor: &[u8],
) -> Result<u16,Error> {
    if descriptor.len()<32 { return Err(Error::Truncated); }
    let seed=ext4_metadata_checksum_seed(superblock)?;
    let crc=crc32c(seed,&group.to_le_bytes());
    let crc=crc32c(crc,&descriptor[..EXT4_GD_CHECKSUM_OFFSET]);
    let crc=crc32c(crc,&descriptor[EXT4_GD_CHECKSUM_OFFSET+2..]);
    Ok(crc as u16)
}

pub fn validate_group_descriptor_checksum(
    superblock: &[u8], group: u32, descriptor: &[u8],
) -> Result<(),Error> {
    if u16at(descriptor,EXT4_GD_CHECKSUM_OFFSET)?==group_descriptor_checksum(superblock,group,descriptor)? {
        Ok(())
    } else { Err(Error::InvalidChecksum) }
}

pub fn update_group_descriptor_checksum(
    superblock: &[u8], group: u32, descriptor: &mut [u8],
) -> Result<(),Error> {
    let checksum=group_descriptor_checksum(superblock,group,descriptor)?;
    let target=descriptor.get_mut(EXT4_GD_CHECKSUM_OFFSET..EXT4_GD_CHECKSUM_OFFSET+2)
        .ok_or(Error::Truncated)?;
    target.copy_from_slice(&checksum.to_le_bytes());
    Ok(())
}

/// Calculate the inode checksum using the inode number, generation and inode
/// bytes. The checksum fields are treated as zero while calculating.
pub fn inode_checksum(superblock: &[u8], inode_number: u32, inode: &[u8]) -> Result<u32,Error> {
    if inode.len()<128 || inode_number==0 { return Err(Error::Truncated); }
    let seed=ext4_metadata_checksum_seed(superblock)?;
    let generation=u32at(inode,100)?;
    let mut crc=crc32c(seed,&inode_number.to_le_bytes());
    crc=crc32c(crc,&generation.to_le_bytes());
    crc=crc32c(crc,&inode[..EXT4_INODE_CHECKSUM_LO_OFFSET]);
    crc=crc32c(crc,&[0u8;2]);
    let has_hi=inode.len()>=132 && u16at(inode,128)? >= 4;
    if has_hi {
        crc=crc32c(crc,&inode[126..EXT4_INODE_CHECKSUM_HI_OFFSET]);
        crc=crc32c(crc,&[0u8;2]);
        crc=crc32c(crc,&inode[EXT4_INODE_CHECKSUM_HI_OFFSET+2..]);
    } else {
        crc=crc32c(crc,&inode[126..]);
    }
    Ok(crc)
}

pub fn validate_inode_checksum(
    superblock: &[u8], inode_number: u32, inode: &[u8],
) -> Result<(),Error> {
    let calculated=inode_checksum(superblock,inode_number,inode)?;
    let stored_lo=u16at(inode,EXT4_INODE_CHECKSUM_LO_OFFSET)? as u32;
    let has_hi=inode.len()>=132 && u16at(inode,128)? >= 4;
    let stored=if has_hi {
        stored_lo | ((u16at(inode,EXT4_INODE_CHECKSUM_HI_OFFSET)? as u32)<<16)
    } else { stored_lo };
    let mask=if has_hi { u32::MAX } else { 0xffff };
    if calculated & mask == stored { Ok(()) } else { Err(Error::InvalidChecksum) }
}

pub fn update_inode_checksum(
    superblock: &[u8], inode_number: u32, inode: &mut [u8],
) -> Result<(),Error> {
    let calculated=inode_checksum(superblock,inode_number,inode)?;
    let has_hi=inode.len()>=132 && u16at(inode,128)? >= 4;
    inode[EXT4_INODE_CHECKSUM_LO_OFFSET..EXT4_INODE_CHECKSUM_LO_OFFSET+2]
        .copy_from_slice(&(calculated as u16).to_le_bytes());
    if has_hi {
        inode[EXT4_INODE_CHECKSUM_HI_OFFSET..EXT4_INODE_CHECKSUM_HI_OFFSET+2]
            .copy_from_slice(&((calculated>>16) as u16).to_le_bytes());
    }
    Ok(())
}


fn ext4_inode_checksum_seed(
    superblock: &[u8], inode_number: u32, generation: u32,
) -> Result<u32,Error> {
    let seed=ext4_metadata_checksum_seed(superblock)?;
    let crc=crc32c(seed,&inode_number.to_le_bytes());
    Ok(crc32c(crc,&generation.to_le_bytes()))
}

/// Recalculate the ext4 superblock checksum after a caller has changed fields.
pub fn update_superblock_checksum(raw: &mut [u8]) -> Result<(),Error> {
    if raw.len()<SUPERBLOCK_BYTES { return Err(Error::Truncated); }
    if u32at(raw,100)? & RO_COMPAT_METADATA_CSUM == 0 { return Ok(()); }
    if raw[373]!=1 { return Err(Error::UnsupportedFormat); }
    let checksum=crc32c(!0u32,&raw[..1020]);
    raw[1020..1024].copy_from_slice(&checksum.to_le_bytes());
    Ok(())
}

fn metadata_bitmap_checksum(superblock: &[u8], bitmap: &[u8]) -> Result<u32,Error> {
    let seed=ext4_metadata_checksum_seed(superblock)?;
    Ok(crc32c(seed,bitmap))
}
fn read_bitmap_checksum(descriptor: &[u8], low_offset: usize, high_offset: usize) -> Result<u32,Error> {
    let low=u16at(descriptor,low_offset)? as u32;
    if descriptor.len()>=64 {
        Ok(low | ((u16at(descriptor,high_offset)? as u32)<<16))
    } else { Ok(low) }
}
fn write_bitmap_checksum(
    descriptor: &mut [u8], low_offset: usize, high_offset: usize, checksum: u32,
) -> Result<(),Error> {
    let low=descriptor.get_mut(low_offset..low_offset+2).ok_or(Error::Truncated)?;
    low.copy_from_slice(&(checksum as u16).to_le_bytes());
    if descriptor.len()>=64 {
        let high=descriptor.get_mut(high_offset..high_offset+2).ok_or(Error::Truncated)?;
        high.copy_from_slice(&((checksum>>16) as u16).to_le_bytes());
    }
    Ok(())
}
pub fn update_block_bitmap_checksum(
    superblock: &[u8], descriptor: &mut [u8], bitmap: &[u8],
) -> Result<(),Error> {
    write_bitmap_checksum(descriptor,24,56,metadata_bitmap_checksum(superblock,bitmap)?)
}
pub fn validate_block_bitmap_checksum(
    superblock: &[u8], descriptor: &[u8], bitmap: &[u8],
) -> Result<(),Error> {
    if read_bitmap_checksum(descriptor,24,56)?==metadata_bitmap_checksum(superblock,bitmap)? {
        Ok(())
    } else { Err(Error::InvalidChecksum) }
}
pub fn update_inode_bitmap_checksum(
    superblock: &[u8], descriptor: &mut [u8], bitmap: &[u8],
) -> Result<(),Error> {
    write_bitmap_checksum(descriptor,26,58,metadata_bitmap_checksum(superblock,bitmap)?)
}
pub fn validate_inode_bitmap_checksum(
    superblock: &[u8], descriptor: &[u8], bitmap: &[u8],
) -> Result<(),Error> {
    if read_bitmap_checksum(descriptor,26,58)?==metadata_bitmap_checksum(superblock,bitmap)? {
        Ok(())
    } else { Err(Error::InvalidChecksum) }
}

/// Validate/update the checksum tail on an external extent-tree block.
pub fn validate_extent_block_checksum(
    superblock: &[u8], inode_number: u32, generation: u32, block: &[u8],
) -> Result<(),Error> {
    if block.len()<16 || u16at(block,0)?!=EXTENT_MAGIC { return Err(Error::InvalidExtentTree); }
    let max=u16at(block,4)? as usize;
    let tail=EXTENT_HEADER_SIZE.checked_add(max.checked_mul(EXTENT_ENTRY_SIZE).ok_or(Error::OutOfRange)?)
        .ok_or(Error::OutOfRange)?;
    if tail.checked_add(4).ok_or(Error::OutOfRange)?!=block.len() { return Err(Error::InvalidExtentTree); }
    let seed=ext4_inode_checksum_seed(superblock,inode_number,generation)?;
    let calculated=crc32c(seed,&block[..tail]);
    if u32at(block,tail)?==calculated { Ok(()) } else { Err(Error::InvalidChecksum) }
}
pub fn update_extent_block_checksum(
    superblock: &[u8], inode_number: u32, generation: u32, block: &mut [u8],
) -> Result<(),Error> {
    if block.len()<16 || u16at(block,0)?!=EXTENT_MAGIC { return Err(Error::InvalidExtentTree); }
    let max=u16at(block,4)? as usize;
    let tail=EXTENT_HEADER_SIZE.checked_add(max.checked_mul(EXTENT_ENTRY_SIZE).ok_or(Error::OutOfRange)?)
        .ok_or(Error::OutOfRange)?;
    if tail.checked_add(4).ok_or(Error::OutOfRange)?!=block.len() { return Err(Error::InvalidExtentTree); }
    let seed=ext4_inode_checksum_seed(superblock,inode_number,generation)?;
    let checksum=crc32c(seed,&block[..tail]);
    block[tail..tail+4].copy_from_slice(&checksum.to_le_bytes());
    Ok(())
}

/// Validate/update the 12-byte ext4 directory checksum tail. The block must
/// end in the reserved-zero/rec_len=12/file_type=0xDE tail record.
pub fn validate_directory_block_checksum(
    superblock: &[u8], inode_number: u32, generation: u32, block: &[u8],
) -> Result<(),Error> {
    if block.len()<12 { return Err(Error::Truncated); }
    let tail=block.len()-12;
    if u32at(block,tail)?!=0 || u16at(block,tail+4)?!=12
        || block[tail+6]!=0 || block[tail+7]!=0xde {
        return Err(Error::InvalidDirectoryEntry);
    }
    let seed=ext4_inode_checksum_seed(superblock,inode_number,generation)?;
    let calculated=crc32c(seed,&block[..block.len()-4]);
    if u32at(block,block.len()-4)?==calculated { Ok(()) } else { Err(Error::InvalidChecksum) }
}
pub fn update_directory_block_checksum(
    superblock: &[u8], inode_number: u32, generation: u32, block: &mut [u8],
) -> Result<(),Error> {
    if block.len()<12 { return Err(Error::Truncated); }
    let tail=block.len()-12;
    if u32at(block,tail)?!=0 || u16at(block,tail+4)?!=12
        || block[tail+6]!=0 || block[tail+7]!=0xde {
        return Err(Error::InvalidDirectoryEntry);
    }
    let seed=ext4_inode_checksum_seed(superblock,inode_number,generation)?;
    let checksum=crc32c(seed,&block[..block.len()-4]);
    let end=block.len();
    block[end-4..end].copy_from_slice(&checksum.to_le_bytes());
    Ok(())
}

/// Validated GPT layout used by the mutation routines. The entry array is
/// intentionally capped at 128 x 128-byte entries to bound early-boot memory.
#[derive(Clone, Copy)]
struct GptLayout {
    sector_size: usize,
    disk_sectors: u64,
    header_size: usize,
    entry_count: usize,
    entry_size: usize,
    primary_entries_lba: u64,
    backup_entries_lba: u64,
    first_usable: u64,
    last_usable: u64,
}

const GPT_MAX_ENTRIES_BYTES: usize = 128 * 128;

fn gpt_u64(b: &[u8], o: usize) -> Result<u64, Error> {
    let e = o.checked_add(8).ok_or(Error::OutOfRange)?;
    let s = b.get(o..e).ok_or(Error::Truncated)?;
    Ok(u64::from_le_bytes(s.try_into().map_err(|_| Error::Truncated)?))
}
fn put_u32(b: &mut [u8], o: usize, v: u32) -> Result<(), Error> {
    let e=o.checked_add(4).ok_or(Error::OutOfRange)?;
    b.get_mut(o..e).ok_or(Error::Truncated)?.copy_from_slice(&v.to_le_bytes());
    Ok(())
}
fn put_u64(b: &mut [u8], o: usize, v: u64) -> Result<(), Error> {
    let e=o.checked_add(8).ok_or(Error::OutOfRange)?;
    b.get_mut(o..e).ok_or(Error::Truncated)?.copy_from_slice(&v.to_le_bytes());
    Ok(())
}

fn validate_gpt_header(raw: &[u8], sector_size: usize, expected_lba: u64) -> Result<usize, Error> {
    if raw.len() < sector_size || &raw[..8] != b"EFI PART" {
        return Err(Error::InvalidPartitionTable);
    }
    let header_size=u32at(raw,12)? as usize;
    if !(92..=sector_size).contains(&header_size) { return Err(Error::InvalidPartitionTable); }
    if gpt_u64(raw,24)? != expected_lba { return Err(Error::InvalidPartitionTable); }
    let stored=u32at(raw,16)?;
    let mut copy=[0u8;4096];
    copy[..header_size].copy_from_slice(&raw[..header_size]);
    copy[16..20].fill(0);
    if crc32_ieee(&copy[..header_size]) != stored { return Err(Error::InvalidChecksum); }
    Ok(header_size)
}

fn read_gpt_layout<D: ReadOnlyBlockDevice>(
    device: &mut D,
    sector_size: u32,
    primary: &mut [u8;4096],
    backup: &mut [u8;4096],
    entries: &mut [u8;GPT_MAX_ENTRIES_BYTES],
) -> Result<GptLayout, Error> {
    if !(512..=4096).contains(&sector_size) || !sector_size.is_power_of_two() {
        return Err(Error::InvalidGeometry);
    }
    let ss=sector_size as usize;
    let disk_sectors=device.capacity_bytes()/(sector_size as u64);
    if disk_sectors<6 { return Err(Error::Truncated); }
    device.read_at(sector_size as u64,&mut primary[..ss]).map_err(|_|Error::Io)?;
    let header_size=validate_gpt_header(primary,ss,1)?;
    let backup_lba=gpt_u64(primary,32)?;
    if backup_lba!=disk_sectors-1 { return Err(Error::UnsupportedPartitionTable); }
    device.read_at(backup_lba*(sector_size as u64),&mut backup[..ss]).map_err(|_|Error::Io)?;
    let backup_header_size=validate_gpt_header(backup,ss,backup_lba)?;
    if backup_header_size!=header_size || gpt_u64(backup,32)?!=1 {
        return Err(Error::InvalidPartitionTable);
    }
    let first_usable=gpt_u64(primary,40)?;
    let last_usable=gpt_u64(primary,48)?;
    if first_usable>last_usable || last_usable>=backup_lba { return Err(Error::InvalidPartitionTable); }
    let entry_count=u32at(primary,80)? as usize;
    let entry_size=u32at(primary,84)? as usize;
    if entry_count==0 || entry_count>128 || entry_size!=128 {
        return Err(Error::UnsupportedPartitionTable);
    }
    if u32at(backup,80)? as usize!=entry_count || u32at(backup,84)? as usize!=entry_size {
        return Err(Error::InvalidPartitionTable);
    }
    let array_len=entry_count.checked_mul(entry_size).ok_or(Error::OutOfRange)?;
    let primary_entries_lba=gpt_u64(primary,72)?;
    let backup_entries_lba=gpt_u64(backup,72)?;
    let array_sectors=(array_len+ss-1)/ss;
    if primary_entries_lba<2 || primary_entries_lba.checked_add(array_sectors as u64).ok_or(Error::OutOfRange)?>first_usable
        || backup_entries_lba.checked_add(array_sectors as u64).ok_or(Error::OutOfRange)?>backup_lba
        || backup_entries_lba<=last_usable {
        return Err(Error::InvalidPartitionTable);
    }
    let array_offset=primary_entries_lba.checked_mul(sector_size as u64).ok_or(Error::OutOfRange)?;
    device.read_at(array_offset,&mut entries[..array_len]).map_err(|_|Error::Io)?;
    let entries_crc=crc32_ieee(&entries[..array_len]);
    if entries_crc!=u32at(primary,88)? || entries_crc!=u32at(backup,88)? {
        return Err(Error::InvalidChecksum);
    }
    // Validate the backup entry array independently before modifying either
    // copy; matching header CRC fields alone do not prove the backup data is sound.
    let backup_offset=backup_entries_lba.checked_mul(sector_size as u64).ok_or(Error::OutOfRange)?;
    let mut backup_crc=!0u32;
    let mut checked=0usize;
    let mut chunk=[0u8;4096];
    while checked<array_len {
        let n=core::cmp::min(chunk.len(),array_len-checked);
        let at=backup_offset.checked_add(checked as u64).ok_or(Error::OutOfRange)?;
        device.read_at(at,&mut chunk[..n]).map_err(|_|Error::Io)?;
        backup_crc=crc32_ieee_extend(backup_crc,&chunk[..n]);
        checked+=n;
    }
    if !backup_crc!=entries_crc { return Err(Error::InvalidChecksum); }
    Ok(GptLayout{sector_size:ss,disk_sectors,header_size,entry_count,entry_size,
        primary_entries_lba,backup_entries_lba,first_usable,last_usable})
}

/// Sector-preserving write: callers may update a short GPT entry array without
/// issuing a partial physical-sector write to the underlying disk driver.
fn write_sector_region<D: BlockDevice>(
    device: &mut D, offset: u64, bytes: &[u8], sector_size: usize,
) -> Result<(), Error> {
    if offset % sector_size as u64 != 0 { return Err(Error::InvalidGeometry); }
    let mut sector=[0u8;4096];
    let mut done=0usize;
    while done<bytes.len() {
        let n=core::cmp::min(sector_size,bytes.len()-done);
        let at=offset.checked_add(done as u64).ok_or(Error::OutOfRange)?;
        if n==sector_size {
            device.write_at(at,&bytes[done..done+n]).map_err(|_|Error::Io)?;
        } else {
            device.read_at(at,&mut sector[..sector_size]).map_err(|_|Error::Io)?;
            sector[..n].copy_from_slice(&bytes[done..done+n]);
            device.write_at(at,&sector[..sector_size]).map_err(|_|Error::Io)?;
        }
        done+=n;
    }
    Ok(())
}

fn write_gpt_pair<D: BlockDevice>(
    device: &mut D,
    layout: &GptLayout,
    primary: &mut [u8;4096],
    backup: &mut [u8;4096],
    entries: &[u8;GPT_MAX_ENTRIES_BYTES],
) -> Result<(), Error> {
    let len=layout.entry_count.checked_mul(layout.entry_size).ok_or(Error::OutOfRange)?;
    let crc=crc32_ieee(&entries[..len]);
    put_u32(primary,88,crc)?;
    put_u32(backup,88,crc)?;
    put_u32(primary,16,0)?;
    put_u32(backup,16,0)?;
    let primary_header_crc=crc32_ieee(&primary[..layout.header_size]);
    let backup_header_crc=crc32_ieee(&backup[..layout.header_size]);
    put_u32(primary,16,primary_header_crc)?;
    put_u32(backup,16,backup_header_crc)?;

    // Commit the backup copy first. A power loss can leave a stale primary,
    // but the backup remains independently valid and recoverable.
    let backup_entries=layout.backup_entries_lba.checked_mul(layout.sector_size as u64).ok_or(Error::OutOfRange)?;
    write_sector_region(device,backup_entries,&entries[..len],layout.sector_size)?;
    write_sector_region(device,gpt_u64(backup,24)?.checked_mul(layout.sector_size as u64).ok_or(Error::OutOfRange)?,
        &backup[..layout.sector_size],layout.sector_size)?;
    device.flush().map_err(|_|Error::Io)?;
    let primary_entries=layout.primary_entries_lba.checked_mul(layout.sector_size as u64).ok_or(Error::OutOfRange)?;
    write_sector_region(device,primary_entries,&entries[..len],layout.sector_size)?;
    write_sector_region(device,layout.sector_size as u64,&primary[..layout.sector_size],layout.sector_size)?;
    device.flush().map_err(|_|Error::Io)?;
    Ok(())
}

fn gpt_entry_is_empty(entry: &[u8]) -> bool { entry[..16].iter().all(|b|*b==0) }
fn gpt_entry_range(entry: &[u8]) -> Result<(u64,u64),Error> {
    Ok((gpt_u64(entry,32)?,gpt_u64(entry,40)?))
}
fn validate_gpt_new_range(
    entries: &[u8], count: usize, first: u64, last: u64,
    usable_first: u64, usable_last: u64, ignore_slot: Option<usize>,
) -> Result<(),Error> {
    if first<usable_first || last>usable_last || first>last { return Err(Error::OutOfRange); }
    for slot in 0..count {
        if Some(slot)==ignore_slot { continue; }
        let e=&entries[slot*128..(slot+1)*128];
        if gpt_entry_is_empty(e) { continue; }
        let (a,b)=gpt_entry_range(e)?;
        if first<=b && a<=last { return Err(Error::InvalidPartitionTable); }
    }
    Ok(())
}

/// Create a GPT partition in the first unused entry. The caller supplies the
/// 16-byte type and unique GUIDs; GUID generation is deliberately left to the
/// caller so this no_std driver never relies on weak/random identifiers.
pub fn create_gpt_partition<D: BlockDevice>(
    device: &mut D, sector_size: u32, type_guid: [u8;16], unique_guid: [u8;16],
    first_lba: u64, last_lba: u64, attributes: u64, name_utf16: [u16;36],
) -> Result<u32,Error> {
    if type_guid.iter().all(|b|*b==0) || unique_guid.iter().all(|b|*b==0) {
        return Err(Error::InvalidPartitionTable);
    }
    let mut primary=[0u8;4096]; let mut backup=[0u8;4096];
    let mut entries=[0u8;GPT_MAX_ENTRIES_BYTES];
    let layout=read_gpt_layout(device,sector_size,&mut primary,&mut backup,&mut entries)?;
    validate_gpt_new_range(&entries,layout.entry_count,first_lba,last_lba,
        layout.first_usable,layout.last_usable,None)?;
    for slot in 0..layout.entry_count {
        let e=&entries[slot*128..(slot+1)*128];
        if !gpt_entry_is_empty(e) && e[16..32]==unique_guid { return Err(Error::InvalidPartitionTable); }
    }
    let slot=(0..layout.entry_count).find(|i|gpt_entry_is_empty(&entries[i*128..(i+1)*128]))
        .ok_or(Error::OutOfRange)?;
    let e=&mut entries[slot*128..(slot+1)*128];
    e.fill(0); e[..16].copy_from_slice(&type_guid); e[16..32].copy_from_slice(&unique_guid);
    e[32..40].copy_from_slice(&first_lba.to_le_bytes());
    e[40..48].copy_from_slice(&last_lba.to_le_bytes());
    e[48..56].copy_from_slice(&attributes.to_le_bytes());
    for (i,ch) in name_utf16.iter().enumerate() {
        e[56+i*2..58+i*2].copy_from_slice(&ch.to_le_bytes());
    }
    write_gpt_pair(device,&layout,&mut primary,&mut backup,&entries)?;
    Ok(slot as u32)
}

/// Delete a GPT partition entry. This changes only partition metadata; it does
/// not erase or overwrite the partition's former contents.
pub fn delete_gpt_partition<D: BlockDevice>(
    device: &mut D, sector_size: u32, slot: u32,
) -> Result<(),Error> {
    let mut primary=[0u8;4096]; let mut backup=[0u8;4096];
    let mut entries=[0u8;GPT_MAX_ENTRIES_BYTES];
    let layout=read_gpt_layout(device,sector_size,&mut primary,&mut backup,&mut entries)?;
    let slot=slot as usize;
    if slot>=layout.entry_count { return Err(Error::OutOfRange); }
    let e=&mut entries[slot*128..(slot+1)*128];
    if gpt_entry_is_empty(e) { return Err(Error::InvalidPartitionTable); }
    e.fill(0);
    write_gpt_pair(device,&layout,&mut primary,&mut backup,&entries)
}

/// Extend an existing GPT partition to a new inclusive ending LBA. Shrinking,
/// overlap, and movement of the starting LBA are intentionally not permitted.
pub fn extend_gpt_partition<D: BlockDevice>(
    device: &mut D, sector_size: u32, slot: u32, new_last_lba: u64,
) -> Result<DiskPartition,Error> {
    let mut primary=[0u8;4096]; let mut backup=[0u8;4096];
    let mut entries=[0u8;GPT_MAX_ENTRIES_BYTES];
    let layout=read_gpt_layout(device,sector_size,&mut primary,&mut backup,&mut entries)?;
    let slot=slot as usize;
    if slot>=layout.entry_count { return Err(Error::OutOfRange); }
    let (first,old_last)={
        let e=&entries[slot*128..(slot+1)*128];
        if gpt_entry_is_empty(e) { return Err(Error::InvalidPartitionTable); }
        gpt_entry_range(e)?
    };
    if new_last_lba<=old_last { return Err(Error::InvalidGeometry); }
    validate_gpt_new_range(&entries,layout.entry_count,first,new_last_lba,
        layout.first_usable,layout.last_usable,Some(slot))?;
    entries[slot*128+40..slot*128+48].copy_from_slice(&new_last_lba.to_le_bytes());
    write_gpt_pair(device,&layout,&mut primary,&mut backup,&entries)?;
    Ok(DiskPartition{first_lba:first,sector_count:new_last_lba.checked_sub(first)
        .and_then(|n|n.checked_add(1)).ok_or(Error::OutOfRange)?,sector_size:layout.sector_size as u32})
}


#[cfg(test)]
mod host_tests {
    use super::*;

    #[test]
    fn parser_mapping_and_crc_smoke_tests_pass() {
        assert!(self_test());
    }

    #[test]
    fn metadata_csum_superblock_is_checked_and_corruption_rejected() {
        let mut raw = [0u8; SUPERBLOCK_BYTES];
        raw[100..104].copy_from_slice(&0x0400u32.to_le_bytes());
        raw[CHECKSUM_TYPE_OFFSET_FOR_TEST..CHECKSUM_TYPE_OFFSET_FOR_TEST + 1].copy_from_slice(&[1]);
        let checksum = crc32c(!0u32, &raw[..1020]);
        raw[1020..1024].copy_from_slice(&checksum.to_le_bytes());
        assert_eq!(validate_superblock_checksum(&raw), Ok(()));
        raw[200] ^= 0x40;
        assert_eq!(validate_superblock_checksum(&raw), Err(Error::InvalidChecksum));
    }

    struct NoIoDevice;
    impl ReadOnlyBlockDevice for NoIoDevice {
        type DeviceError = ();
        fn capacity_bytes(&self) -> u64 { 4096 }
        fn read_at(&mut self, _: u64, _: &mut [u8]) -> Result<(), Self::DeviceError> { Err(()) }
    }
    impl BlockDevice for NoIoDevice {
        fn write_at(&mut self, _: u64, _: &[u8]) -> Result<(), Self::DeviceError> { Err(()) }
        fn flush(&mut self) -> Result<(), Self::DeviceError> { Err(()) }
    }

    #[test]
    fn mbr_primary_create_extend_delete_and_overlap_checks() {
        let mut disk=MemDisk { bytes:[0;32768], flush_count:0 };
        disk.bytes[510]=0x55; disk.bytes[511]=0xaa;
        let slot=create_mbr_partition(&mut disk,512,0x83,4,8,false).unwrap();
        assert_eq!(slot,0);
        let p=read_mbr_partition(&mut disk,0,512).unwrap();
        assert_eq!((p.first_lba,p.sector_count),(4,8));
        assert_eq!(extend_mbr_partition(&mut disk,512,0,12).unwrap().sector_count,12);
        assert_eq!(create_mbr_partition(&mut disk,512,0x83,10,4,false),Err(Error::InvalidPartitionTable));
        assert_eq!(delete_mbr_partition(&mut disk,512,0),Ok(()));
        assert_eq!(read_mbr_partition(&mut disk,0,512),Err(Error::InvalidPartitionTable));
        assert!(disk.flush_count>=3);
    }

    #[test]
    fn mbr_mutation_rejects_protective_and_extended_layouts() {
        let mut disk=MemDisk { bytes:[0;32768], flush_count:0 };
        disk.bytes[510]=0x55; disk.bytes[511]=0xaa;
        disk.bytes[450]=0xee;
        assert_eq!(create_mbr_partition(&mut disk,512,0x83,4,8,false),Err(Error::UnsupportedPartitionTable));
        disk.bytes[450]=0x05;
        assert_eq!(delete_mbr_partition(&mut disk,512,0),Err(Error::UnsupportedPartitionTable));
    }

    #[test]
    fn writes_past_inode_eof_are_rejected_before_device_io() {
        let sb = Superblock {
            inodes_count: 1, blocks_count: 4, first_data_block: 1,
            block_size: 1024, blocks_per_group: 3, inodes_per_group: 1,
            inode_size: 128, descriptor_size: 32, revision: 1,
            feature_compat: 0, feature_incompat: INCOMPAT_EXTENTS,
            feature_ro_compat: 0, read_only: true,
        };
        let mut fs = ExtFilesystem { device: NoIoDevice, superblock: sb, read_only: false };
        let mut scratch = [0u8; 1024];
        assert_eq!(
            fs.write_file_data(&[0; 60], 4, 4, &[1], &mut scratch),
            Err(WriteError::OutOfRange)
        );
    }

    struct MemDisk { bytes:[u8;32768], flush_count:u32 }
    impl ReadOnlyBlockDevice for MemDisk {
        type DeviceError=();
        fn capacity_bytes(&self)->u64{self.bytes.len() as u64}
        fn read_at(&mut self,offset:u64,out:&mut[u8])->Result<(),Self::DeviceError>{
            let start=offset as usize;let end=start.checked_add(out.len()).ok_or(())?;
            let src=self.bytes.get(start..end).ok_or(())?;out.copy_from_slice(src);Ok(())
        }
    }
    impl BlockDevice for MemDisk {
        fn write_at(&mut self,offset:u64,data:&[u8])->Result<(),Self::DeviceError>{
            let start=offset as usize;let end=start.checked_add(data.len()).ok_or(())?;
            let dst=self.bytes.get_mut(start..end).ok_or(())?;dst.copy_from_slice(data);Ok(())
        }
        fn flush(&mut self)->Result<(),Self::DeviceError>{self.flush_count+=1;Ok(())}
    }

    #[test]
    fn writes_existing_extent_data_with_read_modify_write_and_flush() {
        let mut disk = MemDisk { bytes: [0; 32768], flush_count: 0 };
        let data_start = 5 * 1024;
        disk.bytes[data_start..data_start + 1024].fill(0x5a);
        let sb = Superblock {
            inodes_count: 8, blocks_count: 16, first_data_block: 1,
            block_size: 1024, blocks_per_group: 15, inodes_per_group: 8,
            inode_size: 128, descriptor_size: 32, revision: 1,
            feature_compat: 0, feature_incompat: INCOMPAT_EXTENTS,
            feature_ro_compat: 0, read_only: true,
        };
        let mut fs = ExtFilesystem { device: disk, superblock: sb, read_only: false };
        let mut root = [0u8; 60];
        root[0..2].copy_from_slice(&EXTENT_MAGIC.to_le_bytes());
        root[2..4].copy_from_slice(&1u16.to_le_bytes());
        root[4..6].copy_from_slice(&4u16.to_le_bytes());
        root[6..8].copy_from_slice(&0u16.to_le_bytes());
        root[12..16].copy_from_slice(&0u32.to_le_bytes());
        root[16..18].copy_from_slice(&1u16.to_le_bytes());
        root[20..24].copy_from_slice(&5u32.to_le_bytes());
        let mut scratch = [0u8; 1024];
        assert_eq!(fs.write_file_data(&root, 1024, 13, &[0xa1, 0xb2, 0xc3], &mut scratch), Ok(3));
        let disk = fs.into_device();
        assert_eq!(&disk.bytes[data_start + 12..data_start + 17], &[0x5a, 0xa1, 0xb2, 0xc3, 0x5a]);
        assert_eq!(disk.flush_count, 1, "successful file writes must flush the device");
    }

    #[test]
    fn mbr_partition_view_translates_and_contains_io() {
        let mut disk=MemDisk{bytes:[0;32768],flush_count:0};
        disk.bytes[510]=0x55;disk.bytes[511]=0xaa;
        let e=446;
        disk.bytes[e+4]=0x83;
        disk.bytes[e+8..e+12].copy_from_slice(&2u32.to_le_bytes());
        disk.bytes[e+12..e+16].copy_from_slice(&4u32.to_le_bytes());
        let p=read_mbr_partition(&mut disk,0,512).unwrap();
        assert_eq!(p,DiskPartition{first_lba:2,sector_count:4,sector_size:512});
        let mut view=PartitionDevice::new(disk,p).unwrap();
        view.write_at(512,&[0xa5,0x5a]).unwrap();
        let mut got=[0;2];view.read_at(512,&mut got).unwrap();
        assert_eq!(got,[0xa5,0x5a]);
        assert_eq!(view.read_at(2048,&mut got),Err(PartitionIoError::OutOfRange));
        assert_eq!(view.write_at(2047,&[1,2]),Err(PartitionIoError::OutOfRange));
        let disk=view.into_device();
        assert_eq!(&disk.bytes[2*512+512..2*512+514],&[0xa5,0x5a]);
    }

    #[test]
    fn gpt_header_and_entry_array_crc_are_validated() {
        let mut disk=MemDisk{bytes:[0;32768],flush_count:0};
        let entries=&mut disk.bytes[2*512..2*512+128];
        entries[..16].fill(0x11);
        entries[32..40].copy_from_slice(&4u64.to_le_bytes());
        entries[40..48].copy_from_slice(&7u64.to_le_bytes());
        let entries_crc=crc32_ieee(entries);
        let h=&mut disk.bytes[512..1024];
        h[..8].copy_from_slice(b"EFI PART");
        h[8..12].copy_from_slice(&0x0001_0000u32.to_le_bytes());
        h[12..16].copy_from_slice(&92u32.to_le_bytes());
        h[24..32].copy_from_slice(&1u64.to_le_bytes());
        h[32..40].copy_from_slice(&63u64.to_le_bytes());
        h[40..48].copy_from_slice(&3u64.to_le_bytes());
        h[48..56].copy_from_slice(&62u64.to_le_bytes());
        h[72..80].copy_from_slice(&2u64.to_le_bytes());
        h[80..84].copy_from_slice(&1u32.to_le_bytes());
        h[84..88].copy_from_slice(&128u32.to_le_bytes());
        h[88..92].copy_from_slice(&entries_crc.to_le_bytes());
        let crc=crc32_ieee(&h[..92]);
        h[16..20].copy_from_slice(&crc.to_le_bytes());
        let p=read_gpt_partition(&mut disk,0,512).unwrap();
        assert_eq!(p,DiskPartition{first_lba:4,sector_count:4,sector_size:512});
        disk.bytes[512+24]^=1;
        assert_eq!(read_gpt_partition(&mut disk,0,512),Err(Error::InvalidChecksum));
    }


    #[test]
    fn ext_filesystem_mounts_from_partition_relative_superblock() {
        let mut disk=MemDisk{bytes:[0;32768],flush_count:0};
        let start=4*512+SUPERBLOCK_DISK_OFFSET as usize;
        let sb=&mut disk.bytes[start..start+SUPERBLOCK_BYTES];
        sb[0..4].copy_from_slice(&4u32.to_le_bytes());
        sb[4..8].copy_from_slice(&4u32.to_le_bytes());
        sb[20..24].copy_from_slice(&1u32.to_le_bytes());
        sb[24..28].copy_from_slice(&0u32.to_le_bytes());
        sb[32..36].copy_from_slice(&3u32.to_le_bytes());
        sb[40..44].copy_from_slice(&4u32.to_le_bytes());
        sb[56..58].copy_from_slice(&EXT_MAGIC.to_le_bytes());
        sb[76..80].copy_from_slice(&0u32.to_le_bytes());
        let partition=DiskPartition{first_lba:4,sector_count:16,sector_size:512};
        let fs=ExtFilesystem::mount_partition_read_only(disk,partition).unwrap();
        assert!(fs.superblock().read_only);
        assert_eq!(fs.superblock().blocks_count,4);
        let disk=fs.into_device().into_device();
        assert_eq!(&disk.bytes[start+56..start+58],&EXT_MAGIC.to_le_bytes());
    }

}


#[cfg(test)]
mod ext4_metadata_checksum_tests {
    use super::*;
    fn checksum_superblock()->[u8;SUPERBLOCK_BYTES] {
        let mut sb=[0u8;SUPERBLOCK_BYTES];
        sb[96..100].copy_from_slice(&INCOMPAT_CSUM_SEED.to_le_bytes());
        sb[100..104].copy_from_slice(&RO_COMPAT_METADATA_CSUM.to_le_bytes());
        sb[373]=1;
        sb[EXT4_SB_CHECKSUM_SEED_OFFSET..EXT4_SB_CHECKSUM_SEED_OFFSET+4]
            .copy_from_slice(&0x1234_5678u32.to_le_bytes());
        sb
    }
    #[test]
    fn group_descriptor_checksum_round_trip_and_corruption_detection() {
        let sb=checksum_superblock();
        let mut gd=[0u8;64];
        gd[12..16].copy_from_slice(&17u32.to_le_bytes());
        update_group_descriptor_checksum(&sb,7,&mut gd).unwrap();
        assert_eq!(validate_group_descriptor_checksum(&sb,7,&gd),Ok(()));
        assert_eq!(validate_group_descriptor_checksum(&sb,8,&gd),Err(Error::InvalidChecksum));
        gd[12]^=1;
        assert_eq!(validate_group_descriptor_checksum(&sb,7,&gd),Err(Error::InvalidChecksum));
    }
    #[test]
    fn inode_checksum_round_trip_with_and_without_high_word() {
        let sb=checksum_superblock();
        let mut inode=[0u8;256];
        inode[100..104].copy_from_slice(&0x55aa_u32.to_le_bytes());
        update_inode_checksum(&sb,42,&mut inode).unwrap();
        assert_eq!(validate_inode_checksum(&sb,42,&inode),Ok(()));
        assert_eq!(validate_inode_checksum(&sb,43,&inode),Err(Error::InvalidChecksum));
        inode[8]^=1;
        assert_eq!(validate_inode_checksum(&sb,42,&inode),Err(Error::InvalidChecksum));
        let mut small=[0u8;128];
        small[100..104].copy_from_slice(&9u32.to_le_bytes());
        update_inode_checksum(&sb,2,&mut small).unwrap();
        assert_eq!(validate_inode_checksum(&sb,2,&small),Ok(()));
    }
}


#[cfg(test)]
mod ext4_metadata_blocks_checksum_tests {
    use super::*;
    fn sb()->[u8;SUPERBLOCK_BYTES] {
        let mut b=[0u8;SUPERBLOCK_BYTES];
        b[96..100].copy_from_slice(&INCOMPAT_CSUM_SEED.to_le_bytes());
        b[100..104].copy_from_slice(&RO_COMPAT_METADATA_CSUM.to_le_bytes());
        b[373]=1;
        b[EXT4_SB_CHECKSUM_SEED_OFFSET..EXT4_SB_CHECKSUM_SEED_OFFSET+4]
            .copy_from_slice(&0x89ab_cdefu32.to_le_bytes());
        b
    }
    #[test]
    fn superblock_and_bitmap_checksums_update_and_detect_corruption() {
        let mut superblock=sb();
        update_superblock_checksum(&mut superblock).unwrap();
        assert_eq!(validate_superblock_checksum(&superblock),Ok(()));
        let mut gd=[0u8;64];
        let bitmap=[0x55u8;32];
        update_block_bitmap_checksum(&superblock,&mut gd,&bitmap).unwrap();
        update_inode_bitmap_checksum(&superblock,&mut gd,&bitmap).unwrap();
        assert_eq!(validate_block_bitmap_checksum(&superblock,&gd,&bitmap),Ok(()));
        assert_eq!(validate_inode_bitmap_checksum(&superblock,&gd,&bitmap),Ok(()));
        let mut corrupt=bitmap; corrupt[0]^=1;
        assert_eq!(validate_block_bitmap_checksum(&superblock,&gd,&corrupt),Err(Error::InvalidChecksum));
    }
    #[test]
    fn extent_and_directory_block_checksums_round_trip() {
        let superblock=sb();
        let mut extent=[0u8;1024];
        extent[0..2].copy_from_slice(&EXTENT_MAGIC.to_le_bytes());
        extent[4..6].copy_from_slice(&((1024-16)/12 as usize as u16).to_le_bytes());
        // Use the canonical capacity value for a 1 KiB extent block.
        extent[4..6].copy_from_slice(&84u16.to_le_bytes());
        update_extent_block_checksum(&superblock,7,9,&mut extent).unwrap();
        assert_eq!(validate_extent_block_checksum(&superblock,7,9,&extent),Ok(()));
        let mut directory=[0u8;1024];
        let tail=1012;
        directory[tail+4..tail+6].copy_from_slice(&12u16.to_le_bytes());
        directory[tail+7]=0xde;
        update_directory_block_checksum(&superblock,7,9,&mut directory).unwrap();
        assert_eq!(validate_directory_block_checksum(&superblock,7,9,&directory),Ok(()));
        directory[3]^=1;
        assert_eq!(validate_directory_block_checksum(&superblock,7,9,&directory),Err(Error::InvalidChecksum));
    }
}

#[cfg(test)]
mod gpt_mutation_tests {
    use super::*;
    struct Disk { bytes: [u8;32768] }
    impl ReadOnlyBlockDevice for Disk {
        type DeviceError=();
        fn capacity_bytes(&self)->u64 { self.bytes.len() as u64 }
        fn read_at(&mut self,o:u64,b:&mut[u8])->Result<(),()> {
            let s=o as usize; let e=s.checked_add(b.len()).ok_or(())?;
            b.copy_from_slice(self.bytes.get(s..e).ok_or(())?); Ok(())
        }
    }
    impl BlockDevice for Disk {
        fn write_at(&mut self,o:u64,b:&[u8])->Result<(),()> {
            let s=o as usize; let e=s.checked_add(b.len()).ok_or(())?;
            self.bytes.get_mut(s..e).ok_or(())?.copy_from_slice(b); Ok(())
        }
        fn flush(&mut self)->Result<(),()> { Ok(()) }
    }
    fn make_disk()->Disk {
        let mut d=Disk{bytes:[0;32768]};
        let count=4usize; let array_len=count*128;
        let mut entries=[0u8;512];
        entries[..16].fill(0x11);
        entries[16..32].fill(0x22);
        entries[32..40].copy_from_slice(&8u64.to_le_bytes());
        entries[40..48].copy_from_slice(&15u64.to_le_bytes());
        let crc=crc32_ieee(&entries[..array_len]);
        d.bytes[2*512..2*512+array_len].copy_from_slice(&entries[..array_len]);
        let backup_entries_lba=62u64;
        d.bytes[backup_entries_lba as usize*512..backup_entries_lba as usize*512+array_len].copy_from_slice(&entries[..array_len]);
        for (lba,alt,entries_lba) in [(1u64,63u64,2u64),(63u64,1u64,62u64)] {
            let h=&mut d.bytes[lba as usize*512..(lba as usize+1)*512];
            h[..8].copy_from_slice(b"EFI PART"); h[8..12].copy_from_slice(&0x0001_0000u32.to_le_bytes());
            h[12..16].copy_from_slice(&92u32.to_le_bytes()); h[24..32].copy_from_slice(&lba.to_le_bytes());
            h[32..40].copy_from_slice(&alt.to_le_bytes()); h[40..48].copy_from_slice(&4u64.to_le_bytes());
            h[48..56].copy_from_slice(&60u64.to_le_bytes()); h[72..80].copy_from_slice(&entries_lba.to_le_bytes());
            h[80..84].copy_from_slice(&(count as u32).to_le_bytes()); h[84..88].copy_from_slice(&128u32.to_le_bytes());
            h[88..92].copy_from_slice(&crc.to_le_bytes());
            let c=crc32_ieee(&h[..92]); h[16..20].copy_from_slice(&c.to_le_bytes());
        }
        d
    }
    #[test]
    fn gpt_partition_create_extend_delete_updates_both_crc_protected_copies() {
        let mut d=make_disk();
        let slot=create_gpt_partition(&mut d,512,[0x33;16],[0x44;16],20,29,0,[0;36]).unwrap();
        assert_eq!(slot,1);
        assert_eq!(read_gpt_partition(&mut d,slot,512).unwrap(),
            DiskPartition{first_lba:20,sector_count:10,sector_size:512});
        let p=extend_gpt_partition(&mut d,512,slot,39).unwrap();
        assert_eq!(p,DiskPartition{first_lba:20,sector_count:20,sector_size:512});
        assert_eq!(read_gpt_partition(&mut d,slot,512).unwrap(),p);
        delete_gpt_partition(&mut d,512,slot).unwrap();
        assert_eq!(read_gpt_partition(&mut d,slot,512),Err(Error::InvalidPartitionTable));
        // The primary and backup header checksums and both entry-array CRCs
        // are revalidated by every mutation's subsequent read.
        let mut p=[0u8;4096]; let mut b=[0u8;4096]; let mut e=[0u8;GPT_MAX_ENTRIES_BYTES];
        read_gpt_layout(&mut d,512,&mut p,&mut b,&mut e).unwrap();
    }
    #[test]
    fn gpt_partition_mutations_reject_overlap_and_out_of_usable_bounds() {
        let mut d=make_disk();
        assert_eq!(create_gpt_partition(&mut d,512,[0x33;16],[0x44;16],14,20,0,[0;36]),
            Err(Error::InvalidPartitionTable));
        assert_eq!(create_gpt_partition(&mut d,512,[0x33;16],[0x44;16],59,61,0,[0;36]),
            Err(Error::OutOfRange));
    }
}

const CHECKSUM_TYPE_OFFSET_FOR_TEST: usize = 373;

#[cfg(test)]
mod ntstatus_adapter_tests {
    use super::*;
    #[test]
    fn filesystem_errors_map_to_shared_ntstatus() {
        use nt_abi::ntstatus::*;
        assert_eq!(error_to_ntstatus(Error::Truncated), STATUS_BUFFER_TOO_SMALL);
        assert_eq!(error_to_ntstatus(Error::UnsupportedIncompatFeature), STATUS_NOT_IMPLEMENTED);
        assert_eq!(error_to_ntstatus(Error::Io), STATUS_UNSUCCESSFUL);
        assert_eq!(error_to_ntstatus(Error::InvalidExtentTree), STATUS_INVALID_PARAMETER);
    }
}
