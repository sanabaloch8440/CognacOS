//! Bounded JBD2 journal discovery, validation and replay.
//! Supports legacy and checksum-v2/v3 JBD2 replay with bounded tag/revoke
//! parsing. Checksum-v1, async commit, and fast commits remain unsupported.
pub const JBD2_MAGIC:u32=0xc03b3998;
pub const JBD2_SUPERBLOCK_V2:u32=4;
pub const JBD2_DESCRIPTOR_BLOCK:u32=1;
pub const JBD2_COMMIT_BLOCK:u32=2;
pub const JBD2_REVOKE_BLOCK:u32=5;
pub const FEATURE_INCOMPAT_REVOKE:u32=1;
pub const FEATURE_INCOMPAT_64BIT:u32=2;
pub const FEATURE_INCOMPAT_ASYNC_COMMIT:u32=4;
pub const FEATURE_INCOMPAT_CSUM_V2:u32=8;
pub const FEATURE_INCOMPAT_CSUM_V3:u32=16;
pub const FEATURE_INCOMPAT_FAST_COMMIT:u32=32;
/// On-disk JBD2 checksum feature selected by the journal superblock.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum ChecksumMode { None, V1, V2, V3 }

/// Decode JBD2 checksum feature bits and reject contradictory combinations.
/// V1 is the COMPAT checksum bit; V2/V3 are INCOMPAT bits.
pub fn checksum_mode(feature_compat:u32,feature_incompat:u32)->Result<ChecksumMode,Error>{
 let v1=feature_compat&0x0001!=0;
 let v2=feature_incompat&FEATURE_INCOMPAT_CSUM_V2!=0;
 let v3=feature_incompat&FEATURE_INCOMPAT_CSUM_V3!=0;
 if (v1 as u8+v2 as u8+v3 as u8)>1{return Err(Error::UnsupportedFeature)}
 Ok(if v1{ChecksumMode::V1}else if v2{ChecksumMode::V2}else if v3{ChecksumMode::V3}else{ChecksumMode::None})
}

/// Verify the trailing CRC32C on a checksum-v2/v3 descriptor or revoke block.
pub fn verify_metadata_checksum(uuid:&[u8;16],block:&[u8])->bool{
 let seed=crate::checksum::jbd2_csum_seed(uuid);
 crate::checksum::verify_jbd2_metadata_block(seed,block)
}

/// Verify the checksum-v2/v3 commit block checksum (stored at bytes 16..20).
pub fn verify_commit_checksum(uuid:&[u8;16],block:&[u8])->bool{
 if block.len()<20||block[12]!=4||block[13]!=4{return false}
 let seed=crate::checksum::jbd2_csum_seed(uuid);
 let Some(expected)=crate::checksum::jbd2_commit_checksum(seed,block) else{return false};
 let stored=u32::from_be_bytes([block[16],block[17],block[18],block[19]]);
 expected==stored
}

/// Verify a checksum-v2 (16-bit) or checksum-v3 (32-bit) descriptor tag
/// against the complete journal data block.
pub fn verify_tag_checksum(uuid:&[u8;16],sequence:u32,data:&[u8],stored:u32,mode:ChecksumMode)->bool{
 if mode!=ChecksumMode::V2&&mode!=ChecksumMode::V3{return false}
 let seed=crate::checksum::jbd2_csum_seed(uuid);
 let calculated=crate::checksum::jbd2_tag_checksum(seed,sequence,data);
 match mode{ChecksumMode::V2=>(calculated as u16)==(stored as u16),ChecksumMode::V3=>calculated==stored,_=>false}
}

const SUPPORTED_INCOMPAT:u32=FEATURE_INCOMPAT_REVOKE|FEATURE_INCOMPAT_64BIT;
const TAG_ESCAPE:u32=1;
const TAG_SAME_UUID:u32=2;
const TAG_DELETED:u32=4;
const TAG_LAST:u32=8;
pub const MAX_TRANSACTION_TAGS:usize=256;
pub const MAX_TRANSACTION_REVOKES:usize=256;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Error{Truncated,BadMagic,UnsupportedFeature,InvalidGeometry,InvalidSequence,InvalidTag,InvalidRevoke,InvalidChecksum,OutOfRange,Io,NoScratch,TransactionTooLarge}
fn be32(b:&[u8],o:usize)->Result<u32,Error>{let e=o.checked_add(4).ok_or(Error::OutOfRange)?;let s=b.get(o..e).ok_or(Error::Truncated)?;Ok(u32::from_be_bytes([s[0],s[1],s[2],s[3]]))}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct JournalSuperblock{pub block_size:u32,pub max_len:u32,pub first:u32,pub sequence:u32,pub start:u32,pub feature_incompat:u32,pub block_count_64:bool,pub checksum_mode:ChecksumMode,pub uuid:[u8;16]}
impl JournalSuperblock{
 pub fn parse(b:&[u8])->Result<Self,Error>{
  if b.len()<1024{return Err(Error::Truncated)}
  if be32(b,0)?!=JBD2_MAGIC||be32(b,4)?!=JBD2_SUPERBLOCK_V2{return Err(Error::BadMagic)}
  let block_size=be32(b,12)?;let max_len=be32(b,16)?;let first=be32(b,20)?;let sequence=be32(b,24)?;let start=be32(b,28)?;let feature_compat=be32(b,36)?;let feature_incompat=be32(b,40)?;let checksum_mode=checksum_mode(feature_compat,feature_incompat)?;let mut uuid=[0u8;16];uuid.copy_from_slice(&b[48..64]);
  if checksum_mode==ChecksumMode::V2||checksum_mode==ChecksumMode::V3 {
   if b[80]!=4{return Err(Error::UnsupportedFeature)}
   let stored=be32(b,252)?;
   let calculated=crate::checksum::jbd2_superblock_checksum(b).ok_or(Error::InvalidGeometry)?;
   if stored!=calculated{return Err(Error::InvalidChecksum)}
  }
  if block_size as usize!=b.len()||block_size<1024||!block_size.is_power_of_two()||max_len<2||first==0||first>=max_len||start>=max_len{return Err(Error::InvalidGeometry)}
  // JBD2 checksum-v1 is a COMPAT feature, not an incompat bit. We do not
  // implement its transaction-wide CRC validation, so accepting it would
  // allow corrupted log payloads/commits to be replayed without integrity
  // verification. Fail closed until checksum-v1 is implemented.
  if checksum_mode==ChecksumMode::V1{return Err(Error::UnsupportedFeature)}
  if feature_incompat&!(SUPPORTED_INCOMPAT|FEATURE_INCOMPAT_CSUM_V2|FEATURE_INCOMPAT_CSUM_V3)!=0{return Err(Error::UnsupportedFeature)}
  if checksum_mode!=ChecksumMode::None && feature_incompat&FEATURE_INCOMPAT_ASYNC_COMMIT!=0{return Err(Error::UnsupportedFeature)}
  Ok(Self{block_size,max_len,first,sequence,start,feature_incompat,block_count_64:feature_incompat&FEATURE_INCOMPAT_64BIT!=0,checksum_mode,uuid})
 }
 pub fn needs_recovery(&self)->bool{self.start!=0}
}
/// Commit one bounded metadata transaction using ordered-data mode.
/// All ordered data blocks reach stable storage before the journal commit;
/// metadata is journaled, committed, then checkpointed to home blocks. If a
/// crash occurs after the commit record, replay is idempotent. The caller must
/// serialize transactions for this journal and supply exact block-sized data.
pub fn commit_ordered_with_revokes<D:ReplayDevice,M:JournalBlockMap>(
 d:&mut D,m:&mut M,journal_superblock:&mut[u8],
 ordered_targets:&[u64],ordered_data:&[&[u8]],
 metadata_targets:&[u64],metadata_data:&[&[u8]],revoked_targets:&[u64],
 descriptor:&mut[u8],
)->Result<u32,Error>{
 if ordered_targets.len()!=ordered_data.len()||metadata_targets.len()!=metadata_data.len()
  ||metadata_targets.is_empty()||descriptor.len()<1024{return Err(Error::InvalidGeometry)}
 let sb=JournalSuperblock::parse(journal_superblock)?;
 if sb.checksum_mode!=ChecksumMode::None{return Err(Error::UnsupportedFeature)}
 let bs=sb.block_size as usize;
 if descriptor.len()!=bs||journal_superblock.len()!=bs{return Err(Error::NoScratch)}
 if sb.start!=0{return Err(Error::InvalidSequence)}
 if metadata_targets.len()>MAX_TRANSACTION_TAGS{return Err(Error::TransactionTooLarge)}
 let tag_size=if sb.block_count_64{12usize}else{8usize};
 let tag_bytes=tag_size.checked_mul(metadata_targets.len()).ok_or(Error::OutOfRange)?;
 let required=1usize.checked_add(metadata_targets.len()).and_then(|n|n.checked_add(if revoked_targets.is_empty(){0}else{1})).and_then(|n|n.checked_add(1)).ok_or(Error::OutOfRange)?;
 if required>=sb.max_len as usize||12usize.checked_add(tag_bytes).and_then(|n|n.checked_add(16)).ok_or(Error::OutOfRange)?>bs{return Err(Error::TransactionTooLarge)}
 let revoke_width=if sb.block_count_64{8usize}else{4usize};
 if !revoked_targets.is_empty(){
  if sb.feature_incompat&FEATURE_INCOMPAT_REVOKE==0{return Err(Error::UnsupportedFeature)}
  if revoked_targets.len()>MAX_TRANSACTION_REVOKES||16usize.checked_add(revoked_targets.len().checked_mul(revoke_width).ok_or(Error::OutOfRange)?).ok_or(Error::OutOfRange)?>bs{return Err(Error::TransactionTooLarge)}
  if revoked_targets.iter().any(|b|*b>=d.filesystem_blocks()){return Err(Error::OutOfRange)}
 }
 for (target,data) in ordered_targets.iter().zip(ordered_data.iter()){
  if *target>=d.filesystem_blocks()||data.len()!=bs{return Err(Error::OutOfRange)}
 }
 for (target,data) in metadata_targets.iter().zip(metadata_data.iter()){
  if *target>=d.filesystem_blocks()||data.len()!=bs{return Err(Error::OutOfRange)}
 }
 // Ordered data is deliberately written and flushed before any metadata log
 // record can become committed.
 for (target,data) in ordered_targets.iter().zip(ordered_data.iter()){
  d.write_block(*target,data).map_err(|_|Error::Io)?;
 }
 d.flush().map_err(|_|Error::Io)?;
 let sequence=sb.sequence;
 let first=sb.first;
 // Publish the recovery start before writing the transaction. An interrupted
 // pre-commit log is detected as incomplete and must never be replayed.
 journal_superblock[28..32].copy_from_slice(&first.to_be_bytes());
 let sb_phys=m.map_journal_block(0).map_err(|_|Error::Io)?;
 if sb_phys>=d.filesystem_blocks(){return Err(Error::OutOfRange)}
 d.write_block(sb_phys,journal_superblock).map_err(|_|Error::Io)?;
 d.flush().map_err(|_|Error::Io)?;
 descriptor.fill(0);
 descriptor[0..4].copy_from_slice(&JBD2_MAGIC.to_be_bytes());
 descriptor[4..8].copy_from_slice(&JBD2_DESCRIPTOR_BLOCK.to_be_bytes());
 descriptor[8..12].copy_from_slice(&sequence.to_be_bytes());
 let uuid=journal_superblock.get(48..64).ok_or(Error::Truncated)?;
 let mut off=12usize;
 for (i,target) in metadata_targets.iter().enumerate(){
  let mut flags=if i+1==metadata_targets.len(){TAG_LAST}else{0};
  if i!=0{flags|=TAG_SAME_UUID}
  descriptor[off..off+4].copy_from_slice(&(*target as u32).to_be_bytes());
  descriptor[off+4..off+8].copy_from_slice(&flags.to_be_bytes());
  off+=8;
  if sb.block_count_64{descriptor[off..off+4].copy_from_slice(&((*target>>32) as u32).to_be_bytes());off+=4}
  if i==0{descriptor[off..off+16].copy_from_slice(uuid);off+=16}
 }
 let mut cursor=first;
 let desc_phys=m.map_journal_block(cursor).map_err(|_|Error::Io)?;
 d.write_block(desc_phys,descriptor).map_err(|_|Error::Io)?;
 cursor=next_log(&sb,cursor);
 for data in metadata_data{
  let phys=m.map_journal_block(cursor).map_err(|_|Error::Io)?;
  d.write_block(phys,data).map_err(|_|Error::Io)?;
  cursor=next_log(&sb,cursor);
 }
 if !revoked_targets.is_empty(){
  descriptor.fill(0);
  descriptor[0..4].copy_from_slice(&JBD2_MAGIC.to_be_bytes());
  descriptor[4..8].copy_from_slice(&JBD2_REVOKE_BLOCK.to_be_bytes());
  descriptor[8..12].copy_from_slice(&sequence.to_be_bytes());
  let bytes=16usize+revoked_targets.len()*revoke_width;
  descriptor[12..16].copy_from_slice(&(bytes as u32).to_be_bytes());
  for (i,target) in revoked_targets.iter().enumerate(){
   let o=16+i*revoke_width;
   descriptor[o..o+4].copy_from_slice(&(*target as u32).to_be_bytes());
   if sb.block_count_64{descriptor[o+4..o+8].copy_from_slice(&((*target>>32) as u32).to_be_bytes())}
  }
  let revoke_phys=m.map_journal_block(cursor).map_err(|_|Error::Io)?;
  d.write_block(revoke_phys,descriptor).map_err(|_|Error::Io)?;
  cursor=next_log(&sb,cursor);
 }
 descriptor.fill(0);
 descriptor[0..4].copy_from_slice(&JBD2_MAGIC.to_be_bytes());
 descriptor[4..8].copy_from_slice(&JBD2_COMMIT_BLOCK.to_be_bytes());
 descriptor[8..12].copy_from_slice(&sequence.to_be_bytes());
 let commit_phys=m.map_journal_block(cursor).map_err(|_|Error::Io)?;
 d.write_block(commit_phys,descriptor).map_err(|_|Error::Io)?;
 d.flush().map_err(|_|Error::Io)?;
 // The committed log is now authoritative. Checkpoint home metadata and only
 // then clear the journal's recovery-start pointer.
 for (target,data) in metadata_targets.iter().zip(metadata_data.iter()){
  d.write_block(*target,data).map_err(|_|Error::Io)?;
 }
 d.flush().map_err(|_|Error::Io)?;
 journal_superblock[28..32].fill(0);
 journal_superblock[24..28].copy_from_slice(&sequence.wrapping_add(1).to_be_bytes());
 d.write_block(sb_phys,journal_superblock).map_err(|_|Error::Io)?;
 d.flush().map_err(|_|Error::Io)?;
 Ok(sequence)
}

/// Commit ordered data and journaled metadata without revoke records.
pub fn commit_ordered<D:ReplayDevice,M:JournalBlockMap>(
 d:&mut D,m:&mut M,journal_superblock:&mut[u8], ordered_targets:&[u64],ordered_data:&[&[u8]],
 metadata_targets:&[u64],metadata_data:&[&[u8]],descriptor:&mut[u8],
)->Result<u32,Error>{
 commit_ordered_with_revokes(d,m,journal_superblock,ordered_targets,ordered_data,
  metadata_targets,metadata_data,&[],descriptor)
}

/// Full journal recovery entry point. The caller reads the journal inode's
/// logical block zero into journal_superblock and supplies a mapping for its
/// extents. After replaying every complete committed transaction, it clears
/// the recovery-start field and advances the sequence. An incomplete tail is
/// discarded only after all preceding committed transactions are checkpointed.
pub trait ReplayTargetPolicy {
 fn validate_replay_target(&mut self, target: u64) -> Result<(), Error>;
}

struct FilesystemBoundsOnly;
impl ReplayTargetPolicy for FilesystemBoundsOnly {
 fn validate_replay_target(&mut self, _target: u64) -> Result<(), Error> { Ok(()) }
}

pub fn recover<D:ReplayDevice,M:JournalBlockMap>(
 d:&mut D,m:&mut M,journal_superblock:&mut[u8],scratch:&mut[u8],payload:&mut[u8],
)->Result<ReplayReport,Error>{
 recover_with_policy(d,m,journal_superblock,scratch,payload,&mut FilesystemBoundsOnly)
}

/// Recovery variant that validates every home-block target before replay.
/// Mount code should reject journal self-overwrite and apply its validated
/// metadata/system-zone policy here.
pub fn recover_with_policy<D:ReplayDevice,M:JournalBlockMap,P:ReplayTargetPolicy>(
 d:&mut D,m:&mut M,journal_superblock:&mut[u8],scratch:&mut[u8],payload:&mut[u8],
 policy:&mut P,
)->Result<ReplayReport,Error>{
 let sb=JournalSuperblock::parse(journal_superblock)?;
 if !sb.needs_recovery(){return Ok(ReplayReport::default())}
 let report=replay_with_policy(d,m,&sb,scratch,payload,policy)?;
 journal_superblock[28..32].fill(0);
 journal_superblock[24..28].copy_from_slice(&sb.sequence.wrapping_add(report.transactions_replayed).to_be_bytes());
 let physical=m.map_journal_block(0).map_err(|_|Error::Io)?;
 if physical>=d.filesystem_blocks(){return Err(Error::OutOfRange)}
 d.write_block(physical,journal_superblock).map_err(|_|Error::Io)?;
 d.flush().map_err(|_|Error::Io)?;
 Ok(report)
}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct JournalInodeInfo{pub inode_number:u32,pub uuid:[u8;16]}
/// Discover an internal JBD2 journal from the ext4 superblock. External/// journal devices are intentionally rejected until the storage stack can
/// bind and validate the external device UUID and lifetime.
pub fn discover_internal_journal(ext4_superblock:&[u8])->Result<JournalInodeInfo,Error>{
 if ext4_superblock.len()<1024{return Err(Error::Truncated)}
 let compat=u32::from_le_bytes(ext4_superblock[92..96].try_into().map_err(|_|Error::Truncated)?);
 if compat&0x0004==0{return Err(Error::InvalidGeometry)} let inode=u32::from_le_bytes(ext4_superblock[224..228].try_into().map_err(|_|Error::Truncated)?);
 let external=u32::from_le_bytes(ext4_superblock[228..232].try_into().map_err(|_|Error::Truncated)?);
 if inode==0||external!=0{return Err(Error::UnsupportedFeature)}
 let mut uuid=[0u8;16];uuid.copy_from_slice(&ext4_superblock[208..224]);
 Ok(JournalInodeInfo{inode_number:inode,uuid})
}
/// Bind the journal superblock to the UUID recorded by ext4. A zero ext4
/// journal UUID means the filesystem did not pin a journal UUID.
pub fn validate_journal_uuid(ext4_superblock:&[u8],journal_superblock:&[u8])->Result<(),Error>{
 let info=discover_internal_journal(ext4_superblock)?;
 if info.uuid.iter().all(|b|*b==0){return Ok(())}
 if journal_superblock.len()<64||journal_superblock[48..64]!=info.uuid{return Err(Error::BadMagic)}
 Ok(())
}

pub trait ReplayDevice{
 fn filesystem_blocks(&self)->u64;
 fn read_block(&mut self,block:u64,out:&mut[u8])->Result<(),()>;
 fn write_block(&mut self,block:u64,data:&[u8])->Result<(),()>;
 fn flush(&mut self)->Result<(),()>;
}
pub trait JournalBlockMap{fn map_journal_block(&mut self,logical:u32)->Result<u64,()>;}
impl<F:FnMut(u32)->Result<u64,()>> JournalBlockMap for F{fn map_journal_block(&mut self,n:u32)->Result<u64,()>{self(n)}}
#[derive(Clone,Copy)]
struct Tag{target:u64,journal_data:u32,flags:u32,sequence:u32}
impl Tag{const EMPTY:Self=Self{target:0,journal_data:0,flags:0,sequence:0};}
#[derive(Clone,Copy)]
struct Revoke{target:u64,sequence:u32}
impl Revoke{const EMPTY:Self=Self{target:0,sequence:0};}
fn sequence_at_or_after(a:u32,b:u32)->bool{(a.wrapping_sub(b) as i32)>=0}
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub struct ReplayReport{pub transactions_seen:u32,pub transactions_replayed:u32,pub blocks_replayed:u32,pub blocks_revoked:u32,pub stopped_on_incomplete:bool}
fn next_log(sb:&JournalSuperblock,at:u32)->u32{let n=at+1;if n>=sb.max_len{sb.first}else{n}}
fn read_journal<D:ReplayDevice,M:JournalBlockMap>(d:&mut D,m:&mut M,n:u32,out:&mut[u8])->Result<(),Error>{let p=m.map_journal_block(n).map_err(|_|Error::Io)?;if p>=d.filesystem_blocks(){return Err(Error::OutOfRange)}d.read_block(p,out).map_err(|_|Error::Io)}
fn write_target<D:ReplayDevice>(d:&mut D,target:u64,data:&mut[u8],escaped:bool)->Result<(),Error>{if target>=d.filesystem_blocks(){return Err(Error::OutOfRange)}if escaped{data[..4].copy_from_slice(&JBD2_MAGIC.to_be_bytes())}d.write_block(target,data).map_err(|_|Error::Io)}
/// Replay complete committed transactions. map resolves journal logical
/// blocks through the journal inode's extent tree. Two distinct block-sized
/// scratch buffers are required. Data is not applied until a commit block with
/// the expected sequence has been observed.
pub fn replay<D:ReplayDevice,M:JournalBlockMap>(d:&mut D,m:&mut M,sb:&JournalSuperblock,scratch:&mut[u8],payload:&mut[u8])->Result<ReplayReport,Error>{
 replay_with_policy(d,m,sb,scratch,payload,&mut FilesystemBoundsOnly)
}

pub fn replay_with_policy<D:ReplayDevice,M:JournalBlockMap,P:ReplayTargetPolicy>(
 d:&mut D,m:&mut M,sb:&JournalSuperblock,scratch:&mut[u8],payload:&mut[u8],policy:&mut P,
)->Result<ReplayReport,Error>{
 let bs=sb.block_size as usize;if scratch.len()!=bs||payload.len()!=bs{return Err(Error::NoScratch)}
 if !sb.needs_recovery(){return Ok(ReplayReport::default())}
 let mut report=ReplayReport::default();let mut cursor=sb.start;let mut expected=sb.sequence;
 let mut tags=[Tag::EMPTY;MAX_TRANSACTION_TAGS];let mut tag_count=0usize;let mut revokes=[Revoke::EMPTY;MAX_TRANSACTION_REVOKES];let mut revoke_count=0usize;let mut guard=0u32;
 while guard<sb.max_len{
  guard+=1;read_journal(d,m,cursor,scratch)?;
  if be32(scratch,0)?!=JBD2_MAGIC||be32(scratch,8)?!=expected{report.stopped_on_incomplete=true;break}
  // Keep each transaction provisional until its matching commit block has
  // been validated. In particular, never let descriptor tags or revoke
  // records from an interrupted tail leak into the final replay pass.
  let transaction_tag_start=tag_count;
  let transaction_revoke_start=revoke_count;
  let mut tx_cursor=cursor;let mut committed=false;let mut malformed=false;let mut tx_guard=0u32;
  while tx_guard<sb.max_len{
   tx_guard+=1;read_journal(d,m,tx_cursor,scratch)?;
   if be32(scratch,0)?!=JBD2_MAGIC||be32(scratch,8)?!=expected{malformed=true;break}
   match be32(scratch,4)?{
    JBD2_DESCRIPTOR_BLOCK=>{
     if sb.checksum_mode==ChecksumMode::V2||sb.checksum_mode==ChecksumMode::V3{if !verify_metadata_checksum(&sb.uuid,scratch){return Err(Error::InvalidTag)}}
     let mut off=12usize;
     loop{
      if tag_count>=MAX_TRANSACTION_TAGS{return Err(Error::TransactionTooLarge)}
      let (mut target,flags,stored_csum)=match sb.checksum_mode {
       ChecksumMode::V2=>{
        let target=be32(scratch,off)? as u64;
        let checksum=u16::from_be_bytes([*scratch.get(off+4).ok_or(Error::InvalidTag)?,*scratch.get(off+5).ok_or(Error::InvalidTag)?]) as u32;
        let flags=u16::from_be_bytes([*scratch.get(off+6).ok_or(Error::InvalidTag)?,*scratch.get(off+7).ok_or(Error::InvalidTag)?]) as u32;
        off=off.checked_add(8).ok_or(Error::OutOfRange)?;(target,flags,checksum)
       },
       ChecksumMode::V3=>{
        let mut target=be32(scratch,off)? as u64;let flags=be32(scratch,off+4)?;
        off=off.checked_add(8).ok_or(Error::OutOfRange)?;
        let high=be32(scratch,off)?;off=off.checked_add(4).ok_or(Error::OutOfRange)?;
        if sb.block_count_64{target|=(high as u64)<<32}
        let checksum=be32(scratch,off)?;off=off.checked_add(4).ok_or(Error::OutOfRange)?;(target,flags,checksum)
       },
       _=>{
        let target=be32(scratch,off)? as u64;let flags=be32(scratch,off+4)?;
        off=off.checked_add(8).ok_or(Error::OutOfRange)?;(target,flags,0)
       }
      };
      if flags&!(TAG_ESCAPE|TAG_SAME_UUID|TAG_DELETED|TAG_LAST)!=0{return Err(Error::InvalidTag)}
      if sb.block_count_64{target|=(be32(scratch,off)? as u64)<<32;off=off.checked_add(4).ok_or(Error::OutOfRange)?}
      if flags&TAG_SAME_UUID==0{let end=off.checked_add(16).ok_or(Error::OutOfRange)?;if end>bs.saturating_sub(if sb.checksum_mode==ChecksumMode::None{0}else{4}){return Err(Error::InvalidTag)}if scratch[off..end]!=sb.uuid{return Err(Error::InvalidTag)}off=end}
      if off>bs.saturating_sub(if sb.checksum_mode==ChecksumMode::None{0}else{4}){return Err(Error::InvalidTag)}
      if target>=d.filesystem_blocks(){return Err(Error::OutOfRange)}
      let data_block=next_log(sb,tx_cursor);tags[tag_count]=Tag{target,journal_data:data_block,flags,sequence:expected};tag_count+=1;
      if flags&TAG_DELETED==0{
       tx_cursor=data_block;read_journal(d,m,tx_cursor,payload)?;
       if sb.checksum_mode==ChecksumMode::V2||sb.checksum_mode==ChecksumMode::V3{
        if !verify_tag_checksum(&sb.uuid,expected,payload,stored_csum,sb.checksum_mode){return Err(Error::InvalidTag)}
       }
       tx_cursor=next_log(sb,tx_cursor)
      }
      if flags&TAG_LAST!=0{break}
      if off>bs.saturating_sub(if sb.checksum_mode==ChecksumMode::None{0}else{4}){return Err(Error::InvalidTag)}
     }
    }
    JBD2_REVOKE_BLOCK=>{
     if sb.checksum_mode==ChecksumMode::V2||sb.checksum_mode==ChecksumMode::V3{if !verify_metadata_checksum(&sb.uuid,scratch){return Err(Error::InvalidRevoke)}}
     if sb.feature_incompat&FEATURE_INCOMPAT_REVOKE==0{return Err(Error::UnsupportedFeature)}
     let count=be32(scratch,12)? as usize;let width=if sb.block_count_64{8}else{4};
     if count<16||count>bs||(count-16)%width!=0{return Err(Error::InvalidRevoke)}
     let n=(count-16)/width;if revoke_count.checked_add(n).ok_or(Error::OutOfRange)?>MAX_TRANSACTION_REVOKES{return Err(Error::TransactionTooLarge)}
     for i in 0..n{let o=16+i*width;let block=if width==8{((be32(scratch,o+4)? as u64)<<32)|(be32(scratch,o)? as u64)}else{be32(scratch,o)? as u64};if block>=d.filesystem_blocks(){return Err(Error::OutOfRange)}revokes[revoke_count]=Revoke{target:block,sequence:expected};revoke_count+=1}
     tx_cursor=next_log(sb,tx_cursor);
    }
    JBD2_COMMIT_BLOCK=>{
     if sb.checksum_mode==ChecksumMode::V2||sb.checksum_mode==ChecksumMode::V3{if !verify_commit_checksum(&sb.uuid,scratch){return Err(Error::InvalidSequence)}}
     committed=true;tx_cursor=next_log(sb,tx_cursor);break
    }
    _=>return Err(Error::InvalidSequence)
   }
  }
  report.transactions_seen+=1;if malformed||!committed{
   tag_count=transaction_tag_start;
   revoke_count=transaction_revoke_start;
   report.stopped_on_incomplete=true;
   break
  }
  report.transactions_replayed+=1;expected=expected.wrapping_add(1);cursor=tx_cursor;
  if cursor==sb.start{break}
 }
 // Revoke records can occur after the transaction whose writes they cancel.
 // Defer all home-block writes until the complete committed log has been scanned.
 for tag in tags[..tag_count].iter().copied(){
  if tag.flags&TAG_DELETED!=0{continue}
  if revokes[..revoke_count].iter().any(|r|r.target==tag.target&&sequence_at_or_after(r.sequence,tag.sequence)){report.blocks_revoked+=1;continue}
  policy.validate_replay_target(tag.target)?;
  read_journal(d,m,tag.journal_data,payload)?;write_target(d,tag.target,payload,tag.flags&TAG_ESCAPE!=0)?;report.blocks_replayed+=1;
 }
 if report.blocks_replayed!=0{d.flush().map_err(|_|Error::Io)?}
 Ok(report)
}
#[cfg(test)]
mod tests{
 use super::*;
 #[test]fn journal_superblock_checks_geometry_and_unsupported_features(){
  let mut b=[0u8;1024];b[0..4].copy_from_slice(&JBD2_MAGIC.to_be_bytes());b[4..8].copy_from_slice(&JBD2_SUPERBLOCK_V2.to_be_bytes());b[12..16].copy_from_slice(&1024u32.to_be_bytes());b[16..20].copy_from_slice(&64u32.to_be_bytes());b[20..24].copy_from_slice(&1u32.to_be_bytes());b[24..28].copy_from_slice(&7u32.to_be_bytes());b[28..32].copy_from_slice(&3u32.to_be_bytes());
  let sb=JournalSuperblock::parse(&b).unwrap();assert!(sb.needs_recovery());b[40..44].copy_from_slice(&FEATURE_INCOMPAT_CSUM_V3.to_be_bytes());b[80]=4;let csum=crate::checksum::jbd2_superblock_checksum(&b).unwrap();b[252..256].copy_from_slice(&csum.to_be_bytes());assert_eq!(JournalSuperblock::parse(&b).unwrap().checksum_mode,ChecksumMode::V3);b[100]^=1;assert_eq!(JournalSuperblock::parse(&b),Err(Error::InvalidChecksum));b[100]^=1;
  b[40..44].fill(0);b[36..40].copy_from_slice(&1u32.to_be_bytes());assert_eq!(JournalSuperblock::parse(&b),Err(Error::UnsupportedFeature)); }
 #[test]fn clean_journal_is_noop_without_io(){
  struct D;impl ReplayDevice for D{fn filesystem_blocks(&self)->u64{10}fn read_block(&mut self,_:u64,_:&mut[u8])->Result<(),()>{Err(())}fn write_block(&mut self,_:u64,_:&[u8])->Result<(),()>{Err(())}fn flush(&mut self)->Result<(),()>{Ok(())}}
  let sb=JournalSuperblock{block_size:1024,max_len:64,first:1,sequence:1,start:0,feature_incompat:0,block_count_64:false,checksum_mode:ChecksumMode::None,uuid:[0;16]};let mut d=D;let mut a=[0u8;1024];let mut b=[0u8;1024];assert_eq!(replay(&mut d,&mut |_|Err(()),&sb,&mut a,&mut b),Ok(ReplayReport::default()));
 }
 struct Mem{blocks:[[u8;1024];64],flushes:u32}
 impl ReplayDevice for Mem{
  fn filesystem_blocks(&self)->u64{64}
  fn read_block(&mut self,n:u64,out:&mut[u8])->Result<(),()>{let b=self.blocks.get(n as usize).ok_or(())?;if out.len()!=1024{return Err(())}out.copy_from_slice(b);Ok(())}
  fn write_block(&mut self,n:u64,data:&[u8])->Result<(),()>{let b=self.blocks.get_mut(n as usize).ok_or(())?;if data.len()!=1024{return Err(())}b.copy_from_slice(data);Ok(())}
  fn flush(&mut self)->Result<(),()>{self.flushes+=1;Ok(())}
 }
 fn header(b:&mut[u8;1024],kind:u32,seq:u32){b.fill(0);b[0..4].copy_from_slice(&JBD2_MAGIC.to_be_bytes());b[4..8].copy_from_slice(&kind.to_be_bytes());b[8..12].copy_from_slice(&seq.to_be_bytes());}
 #[test]fn replay_applies_only_a_complete_committed_transaction(){
  let mut d=Mem{blocks:[[0;1024];64],flushes:0};
  header(&mut d.blocks[2],JBD2_DESCRIPTOR_BLOCK,1);
  d.blocks[2][12..16].copy_from_slice(&7u32.to_be_bytes());
  d.blocks[2][16..20].copy_from_slice(&TAG_LAST.to_be_bytes());
  d.blocks[3].fill(0x5a);
  header(&mut d.blocks[4],JBD2_COMMIT_BLOCK,1);
  let sb=JournalSuperblock{block_size:1024,max_len:64,first:1,sequence:1,start:2,feature_incompat:0,block_count_64:false,checksum_mode:ChecksumMode::None,uuid:[0;16]};
  let mut a=[0u8;1024];let mut b=[0u8;1024];
  let r=replay(&mut d,&mut |n|Ok(n as u64),&sb,&mut a,&mut b).unwrap();
  assert_eq!(r.transactions_replayed,1);assert_eq!(r.blocks_replayed,1);assert_eq!(d.blocks[7],[0x5a;1024]);
 }
 #[test]fn committed_transactions_survive_an_incomplete_later_tail(){
  let mut d=Mem{blocks:[[0;1024];64],flushes:0};
  header(&mut d.blocks[2],JBD2_DESCRIPTOR_BLOCK,1);
  d.blocks[2][12..16].copy_from_slice(&7u32.to_be_bytes());
  d.blocks[2][16..20].copy_from_slice(&TAG_LAST.to_be_bytes());
  d.blocks[3].fill(0x5a);
  header(&mut d.blocks[4],JBD2_COMMIT_BLOCK,1);
  header(&mut d.blocks[5],JBD2_DESCRIPTOR_BLOCK,2);
  d.blocks[5][12..16].copy_from_slice(&8u32.to_be_bytes());
  d.blocks[5][16..20].copy_from_slice(&TAG_LAST.to_be_bytes());
  d.blocks[6].fill(0x7c);
  // Block 7 is not a commit record for transaction 2.
  let sb=JournalSuperblock{block_size:1024,max_len:64,first:1,sequence:1,start:2,feature_incompat:0,block_count_64:false,checksum_mode:ChecksumMode::None,uuid:[0;16]};
  let mut a=[0u8;1024];let mut b=[0u8;1024];
  let r=replay(&mut d,&mut |n|Ok(n as u64),&sb,&mut a,&mut b).unwrap();
  assert!(r.stopped_on_incomplete);
  assert_eq!(r.transactions_replayed,1);
  assert_eq!(r.blocks_replayed,1);
  assert_eq!(d.blocks[7],[0x5a;1024]);
  assert_eq!(d.blocks[8],[0;1024]);
 }
 #[test]fn incomplete_transaction_tags_are_never_replayed(){
  let mut d=Mem{blocks:[[0;1024];64],flushes:0};
  header(&mut d.blocks[2],JBD2_DESCRIPTOR_BLOCK,1);
  d.blocks[2][12..16].copy_from_slice(&7u32.to_be_bytes());
  d.blocks[2][16..20].copy_from_slice(&TAG_LAST.to_be_bytes());
  d.blocks[3].fill(0x7c);
  // Block 4 is deliberately not a matching commit record.
  let sb=JournalSuperblock{block_size:1024,max_len:64,first:1,sequence:1,start:2,feature_incompat:0,block_count_64:false,checksum_mode:ChecksumMode::None,uuid:[0;16]};
  let mut a=[0u8;1024];let mut b=[0u8;1024];
  let r=replay(&mut d,&mut |n|Ok(n as u64),&sb,&mut a,&mut b).unwrap();
  assert!(r.stopped_on_incomplete);
  assert_eq!(r.transactions_replayed,0);
  assert_eq!(r.blocks_replayed,0);
  assert_eq!(d.blocks[7],[0;1024]);
 }
 #[test]fn later_revoke_suppresses_older_transaction_write(){
  let mut d=Mem{blocks:[[0;1024];64],flushes:0};
  header(&mut d.blocks[2],JBD2_DESCRIPTOR_BLOCK,1);
  d.blocks[2][12..16].copy_from_slice(&7u32.to_be_bytes());d.blocks[2][16..20].copy_from_slice(&TAG_LAST.to_be_bytes());
  d.blocks[3].fill(0x6b);header(&mut d.blocks[4],JBD2_COMMIT_BLOCK,1);
  header(&mut d.blocks[5],JBD2_REVOKE_BLOCK,2);d.blocks[5][12..16].copy_from_slice(&20u32.to_be_bytes());d.blocks[5][16..20].copy_from_slice(&7u32.to_be_bytes());
  header(&mut d.blocks[6],JBD2_COMMIT_BLOCK,2);
  let sb=JournalSuperblock{block_size:1024,max_len:64,first:1,sequence:1,start:2,feature_incompat:FEATURE_INCOMPAT_REVOKE,block_count_64:false,checksum_mode:ChecksumMode::None,uuid:[0;16]};
  let mut a=[0u8;1024];let mut b=[0u8;1024];let r=replay(&mut d,&mut |n|Ok(n as u64),&sb,&mut a,&mut b).unwrap();
  assert_eq!(r.transactions_replayed,2);assert_eq!(r.blocks_replayed,0);assert_eq!(r.blocks_revoked,1);assert_eq!(d.blocks[7],[0;1024]);
 }
 #[test]fn ordered_commit_checkpoints_metadata_and_clears_recovery_pointer(){
  let mut d=Mem{blocks:[[0;1024];64],flushes:0};
  let mut js=[0u8;1024];js[0..4].copy_from_slice(&JBD2_MAGIC.to_be_bytes());js[4..8].copy_from_slice(&JBD2_SUPERBLOCK_V2.to_be_bytes());
  js[12..16].copy_from_slice(&1024u32.to_be_bytes());js[16..20].copy_from_slice(&64u32.to_be_bytes());js[20..24].copy_from_slice(&1u32.to_be_bytes());js[24..28].copy_from_slice(&9u32.to_be_bytes());
  let ordered=[0x31u8;1024];let metadata=[0x42u8;1024];let mut descriptor=[0u8;1024];
  let seq=commit_ordered(&mut d,&mut |n|Ok(n as u64),&mut js,&[11],&[&ordered],&[10],&[&metadata],&mut descriptor).unwrap();
  assert_eq!(seq,9);assert_eq!(d.blocks[11],ordered);assert_eq!(d.blocks[10],metadata);assert_eq!(u32::from_be_bytes(js[28..32].try_into().unwrap()),0);assert_eq!(u32::from_be_bytes(js[24..28].try_into().unwrap()),10);assert!(d.flushes>=5);
 }
 #[test]fn ordered_commit_writes_revoke_record_before_commit(){
  let mut d=Mem{blocks:[[0;1024];64],flushes:0};
  let mut js=[0u8;1024];js[0..4].copy_from_slice(&JBD2_MAGIC.to_be_bytes());js[4..8].copy_from_slice(&JBD2_SUPERBLOCK_V2.to_be_bytes());
  js[12..16].copy_from_slice(&1024u32.to_be_bytes());js[16..20].copy_from_slice(&64u32.to_be_bytes());js[20..24].copy_from_slice(&1u32.to_be_bytes());js[24..28].copy_from_slice(&3u32.to_be_bytes());js[40..44].copy_from_slice(&FEATURE_INCOMPAT_REVOKE.to_be_bytes());
  let metadata=[0x73u8;1024];let mut scratch=[0u8;1024];
  commit_ordered_with_revokes(&mut d,&mut |n|Ok(n as u64),&mut js,&[],&[],&[10],&[&metadata],&[11],&mut scratch).unwrap();
  assert_eq!(be32(&d.blocks[3],4).unwrap(),JBD2_REVOKE_BLOCK);assert_eq!(be32(&d.blocks[3],12).unwrap(),20);assert_eq!(be32(&d.blocks[3],16).unwrap(),11);assert_eq!(d.blocks[10],metadata);
 }
 #[test]fn recovery_discards_an_incomplete_tail_and_clears_start(){
  let mut d=Mem{blocks:[[0;1024];64],flushes:0};
  let mut js=[0u8;1024];js[0..4].copy_from_slice(&JBD2_MAGIC.to_be_bytes());js[4..8].copy_from_slice(&JBD2_SUPERBLOCK_V2.to_be_bytes());
  js[12..16].copy_from_slice(&1024u32.to_be_bytes());js[16..20].copy_from_slice(&64u32.to_be_bytes());js[20..24].copy_from_slice(&1u32.to_be_bytes());js[24..28].copy_from_slice(&8u32.to_be_bytes());js[28..32].copy_from_slice(&2u32.to_be_bytes());
  let mut a=[0u8;1024];let mut b=[0u8;1024];let r=recover(&mut d,&mut |n|Ok(n as u64),&mut js,&mut a,&mut b).unwrap();
  assert!(r.stopped_on_incomplete);assert_eq!(r.transactions_replayed,0);assert_eq!(be32(&js,28).unwrap(),0);assert_eq!(be32(&js,24).unwrap(),8);
 }
 #[test]fn replay_policy_rejects_journal_self_overwrite_before_home_write(){
  struct RejectJournalBlock(u64);
  impl ReplayTargetPolicy for RejectJournalBlock {   fn validate_replay_target(&mut self,target:u64)->Result<(),Error>{
    if target==self.0{Err(Error::InvalidTag)}else{Ok(())}
   }
  }
  let mut d=Mem{blocks:[[0;1024];64],flushes:0};  header(&mut d.blocks[2],JBD2_DESCRIPTOR_BLOCK,1);
  d.blocks[2][12..16].copy_from_slice(&7u32.to_be_bytes());
  d.blocks[2][16..20].copy_from_slice(&TAG_LAST.to_be_bytes());
  d.blocks[3].fill(0x5a);
  header(&mut d.blocks[4],JBD2_COMMIT_BLOCK,1);
  let sb=JournalSuperblock{block_size:1024,max_len:64,first:1,sequence:1,start:2,feature_incompat:0,block_count_64:false,checksum_mode:ChecksumMode::None,uuid:[0;16]};
  let mut a=[0u8;1024];let mut b=[0u8;1024];
  let result=replay_with_policy(&mut d,&mut |n|Ok(n as u64),&sb,&mut a,&mut b,&mut RejectJournalBlock(7));
  assert_eq!(result,Err(Error::InvalidTag));
  assert_eq!(d.blocks[7],[0;1024]);
 }
 #[test]fn ext4_internal_journal_discovery_pins_inode_and_uuid(){
  let mut sb=[0u8;1024];sb[92..96].copy_from_slice(&4u32.to_le_bytes());sb[208..224].copy_from_slice(&[0x5a;16]);sb[224..228].copy_from_slice(&8u32.to_le_bytes());
  let info=discover_internal_journal(&sb).unwrap();assert_eq!(info.inode_number,8);
  let mut js=[0u8;1024];js[48..64].copy_from_slice(&[0x5a;16]);assert_eq!(validate_journal_uuid(&sb,&js),Ok(()));
  js[48]=0;assert_eq!(validate_journal_uuid(&sb,&js),Err(Error::BadMagic));
 }

 #[test]fn checksum_feature_bits_are_mutually_exclusive(){
  assert_eq!(checksum_mode(0,0),Ok(ChecksumMode::None));
  assert_eq!(checksum_mode(1,0),Ok(ChecksumMode::V1));
  assert_eq!(checksum_mode(0,FEATURE_INCOMPAT_CSUM_V2),Ok(ChecksumMode::V2));
  assert_eq!(checksum_mode(0,FEATURE_INCOMPAT_CSUM_V3),Ok(ChecksumMode::V3));
  assert_eq!(checksum_mode(1,FEATURE_INCOMPAT_CSUM_V2),Err(Error::UnsupportedFeature));
  assert_eq!(checksum_mode(0,FEATURE_INCOMPAT_CSUM_V2|FEATURE_INCOMPAT_CSUM_V3),Err(Error::UnsupportedFeature));
 }
 #[test]fn checksum_v2_v3_tag_helpers_validate_full_data_block(){
  let uuid=[0x31;16];let seed=crate::checksum::jbd2_csum_seed(&uuid);let data=[0x62;1024];
  let sum=crate::checksum::jbd2_tag_checksum(seed,19,&data);
  assert!(verify_tag_checksum(&uuid,19,&data,sum,ChecksumMode::V3));
  assert!(verify_tag_checksum(&uuid,19,&data,sum&0xffff,ChecksumMode::V2));
  assert!(!verify_tag_checksum(&uuid,20,&data,sum,ChecksumMode::V3));
  assert!(!verify_tag_checksum(&uuid,19,&data,sum,ChecksumMode::None));
 }

 #[test]fn replay_validates_checksum_v2_descriptor_tag_data_and_commit(){
  let uuid=[0x19;16];let seed=crate::checksum::jbd2_csum_seed(&uuid);
  let mut d=Mem{blocks:[[0;1024];64],flushes:0};
  header(&mut d.blocks[2],JBD2_DESCRIPTOR_BLOCK,1);
  d.blocks[2][12..16].copy_from_slice(&7u32.to_be_bytes());
  let payload=[0x6du8;1024];let sum=crate::checksum::jbd2_tag_checksum(seed,1,&payload);
  d.blocks[2][16..18].copy_from_slice(&(sum as u16).to_be_bytes());
  d.blocks[2][18..20].copy_from_slice(&TAG_LAST.to_be_bytes());d.blocks[2][20..36].copy_from_slice(&uuid);
  let ds=crate::checksum::jbd2_metadata_block_checksum(seed,&d.blocks[2]).unwrap();
  d.blocks[2][1020..].copy_from_slice(&ds.to_be_bytes());
  d.blocks[3]=payload;
  header(&mut d.blocks[4],JBD2_COMMIT_BLOCK,1);d.blocks[4][12]=4;d.blocks[4][13]=4;
  let cs=crate::checksum::jbd2_commit_checksum(seed,&d.blocks[4]).unwrap();
  d.blocks[4][16..20].copy_from_slice(&cs.to_be_bytes());
  let sb=JournalSuperblock{block_size:1024,max_len:64,first:1,sequence:1,start:2,feature_incompat:FEATURE_INCOMPAT_CSUM_V2,block_count_64:false,checksum_mode:ChecksumMode::V2,uuid};
  let mut a=[0u8;1024];let mut b=[0u8;1024];
  let report=replay(&mut d,&mut |n|Ok(n as u64),&sb,&mut a,&mut b).unwrap();
  assert_eq!(report.blocks_replayed,1);assert_eq!(d.blocks[7],payload);
  d.blocks[3][10]^=1;
  assert_eq!(replay(&mut d,&mut |n|Ok(n as u64),&sb,&mut a,&mut b),Err(Error::InvalidTag));
 }
 #[test]fn replay_validates_checksum_v3_tag_layout_and_full_checksum(){
  let uuid=[0x29;16];let seed=crate::checksum::jbd2_csum_seed(&uuid);
  let mut d=Mem{blocks:[[0;1024];64],flushes:0};
  header(&mut d.blocks[2],JBD2_DESCRIPTOR_BLOCK,1);
  d.blocks[2][12..16].copy_from_slice(&7u32.to_be_bytes());
  d.blocks[2][16..20].copy_from_slice(&TAG_LAST.to_be_bytes());
  d.blocks[2][20..24].copy_from_slice(&0u32.to_be_bytes());
  let payload=[0x7eu8;1024];let sum=crate::checksum::jbd2_tag_checksum(seed,1,&payload);
  d.blocks[2][24..28].copy_from_slice(&sum.to_be_bytes());d.blocks[2][28..44].copy_from_slice(&uuid);
  let ds=crate::checksum::jbd2_metadata_block_checksum(seed,&d.blocks[2]).unwrap();
  d.blocks[2][1020..].copy_from_slice(&ds.to_be_bytes());
  d.blocks[3]=payload;
  header(&mut d.blocks[4],JBD2_COMMIT_BLOCK,1);d.blocks[4][12]=4;d.blocks[4][13]=4;
  let cs=crate::checksum::jbd2_commit_checksum(seed,&d.blocks[4]).unwrap();
  d.blocks[4][16..20].copy_from_slice(&cs.to_be_bytes());
  let sb=JournalSuperblock{block_size:1024,max_len:64,first:1,sequence:1,start:2,feature_incompat:FEATURE_INCOMPAT_CSUM_V3,block_count_64:false,checksum_mode:ChecksumMode::V3,uuid};
  let mut a=[0u8;1024];let mut b=[0u8;1024];
  let report=replay(&mut d,&mut |n|Ok(n as u64),&sb,&mut a,&mut b).unwrap();
  assert_eq!(report.blocks_replayed,1);assert_eq!(d.blocks[7],payload);
  d.blocks[2][24]^=1;
  assert_eq!(replay(&mut d,&mut |n|Ok(n as u64),&sb,&mut a,&mut b),Err(Error::InvalidTag));
 }
 #[test]fn checksum_v2_v3_block_verifiers_detect_corruption(){
  let uuid=[0x27;16];let seed=crate::checksum::jbd2_csum_seed(&uuid);
  let mut metadata=[0x45;1024];let sum=crate::checksum::jbd2_metadata_block_checksum(seed,&metadata).unwrap();
  metadata[1020..].copy_from_slice(&sum.to_be_bytes());assert!(verify_metadata_checksum(&uuid,&metadata));
  metadata[99]^=1;assert!(!verify_metadata_checksum(&uuid,&metadata));
  let mut commit=[0x56;1024];commit[12]=4;commit[13]=4;let sum=crate::checksum::jbd2_commit_checksum(seed,&commit).unwrap();
  commit[16..20].copy_from_slice(&sum.to_be_bytes());assert!(verify_commit_checksum(&uuid,&commit));
  commit[200]^=1;assert!(!verify_commit_checksum(&uuid,&commit));
 }
}