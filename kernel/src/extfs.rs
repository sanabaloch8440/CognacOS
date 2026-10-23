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
const SUPPORTED_RO_COMPAT:u32=0x0003; // sparse_super and large_file only
const KNOWN_INCOMPAT:u32=INCOMPAT_FILETYPE|INCOMPAT_EXTENTS|INCOMPAT_FLEX_BG;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Error{Truncated,BadMagic,InvalidBlockSize,InvalidInodeSize,InvalidGeometry,UnsupportedIncompatFeature,OutOfRange,InvalidDirectoryEntry,UnsupportedFormat,UnsupportedRoCompatFeature,UnsupportedCompatFeature,InvalidExtentTree,InvalidChecksum,Io}
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
  let feature_compat=u32at(b,92)?;let feature_incompat=u32at(b,96)?;let feature_ro_compat=u32at(b,100)?;if feature_ro_compat & !SUPPORTED_RO_COMPAT != 0{return Err(Error::UnsupportedRoCompatFeature)}
  // DIR_INDEX requires htree-aware directory parsing; never treat an indexed root as linear entries.
  if feature_compat & 0x0020 != 0{return Err(Error::UnsupportedCompatFeature)}if revision>1{return Err(Error::UnsupportedFormat)}
  if feature_ro_compat & !SUPPORTED_RO_COMPAT != 0{return Err(Error::UnsupportedRoCompatFeature)}
  if revision>1{return Err(Error::UnsupportedFormat)}
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
pub fn read_superblock<D:ReadOnlyBlockDevice>(dev:&mut D)->Result<Superblock,Error>{let end=SUPERBLOCK_DISK_OFFSET.checked_add(SUPERBLOCK_BYTES as u64).ok_or(Error::OutOfRange)?;if dev.capacity_bytes()<end{return Err(Error::OutOfRange)}let mut b=[0u8;SUPERBLOCK_BYTES];dev.read_at(SUPERBLOCK_DISK_OFFSET,&mut b).map_err(|_|Error::Io)?;let sb=Superblock::parse(&b)?;let fs_end=(sb.blocks_count as u64).checked_mul(sb.block_size as u64).ok_or(Error::OutOfRange)?;if fs_end>dev.capacity_bytes(){return Err(Error::OutOfRange)}Ok(sb)}

/// Reads one complete filesystem block. The device read is rejected if either
/// the filesystem geometry or the physical device capacity would be exceeded.
pub fn read_block<D:ReadOnlyBlockDevice>(dev:&mut D,sb:&Superblock,block:u64,out:&mut[u8])->Result<(),Error>{
 if out.len()!=sb.block_size as usize||block>=sb.blocks_count as u64{return Err(Error::OutOfRange)}
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
   if previous_end.map_or(false,|end|(first as u64) < end)||physical==0||physical_end>sb.blocks_count as u64{return Err(Error::InvalidExtentTree)}
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
   if previous_key.map_or(false,|prev|key<=prev)||child==0||child>=sb.blocks_count as u64{return Err(Error::InvalidExtentTree)}
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

pub fn self_test()->bool{let mut b=[0u8;SUPERBLOCK_BYTES];b[0..4].copy_from_slice(&512u32.to_le_bytes());b[4..8].copy_from_slice(&4096u32.to_le_bytes());b[20..24].copy_from_slice(&1u32.to_le_bytes());b[32..36].copy_from_slice(&1024u32.to_le_bytes());b[40..44].copy_from_slice(&128u32.to_le_bytes());b[56..58].copy_from_slice(&EXT_MAGIC.to_le_bytes());b[76..80].copy_from_slice(&1u32.to_le_bytes());b[88..90].copy_from_slice(&128u16.to_le_bytes());b[96..100].copy_from_slice(&(INCOMPAT_FILETYPE|INCOMPAT_EXTENTS).to_le_bytes());let s=match Superblock::parse(&b){Ok(s)=>s,Err(_)=>return false};if s.block_size!=1024||s.inode_size!=128||s.group_count()!=Ok(4)||s.inode_location(129)!=Ok((1,0))||!s.read_only{return false}let mut bad=b;bad[96..100].copy_from_slice(&INCOMPAT_64BIT.to_le_bytes());if Superblock::parse(&bad)!=Err(Error::UnsupportedIncompatFeature){return false}let mut d=[0u8;12];d[0..4].copy_from_slice(&2u32.to_le_bytes());d[4..6].copy_from_slice(&12u16.to_le_bytes());d[6]=4;d[8..12].copy_from_slice(b"test");if validate_directory_block(&d,512)!=Ok(1){return false}d[4..6].copy_from_slice(&0u16.to_le_bytes());if validate_directory_block(&d,512)!=Err(Error::InvalidDirectoryEntry){return false}
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
    if block >= sb.blocks_count as u64 {
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
        if block >= sb.blocks_count as u64 {
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
    } else if block >= sb.blocks_count as u64 {
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
}
const CHECKSUM_TYPE_OFFSET_FOR_TEST: usize = 373;
