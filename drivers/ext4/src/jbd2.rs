//! Bounded JBD2 journal discovery, validation and replay.
//! Supports legacy JBD2 tags with optional 64-bit block numbers and revoke
//! records. Journals requiring checksum-v2/v3, async commit, or fast commits
//! are rejected because their integrity semantics are not implemented here.
#![no_std]
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
const SUPPORTED_INCOMPAT:u32=FEATURE_INCOMPAT_REVOKE|FEATURE_INCOMPAT_64BIT;
const TAG_ESCAPE:u32=1;
const TAG_SAME_UUID:u32=2;
const TAG_DELETED:u32=4;
const TAG_LAST:u32=8;
pub const MAX_TRANSACTION_TAGS:usize=256;
pub const MAX_TRANSACTION_REVOKES:usize=256;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Error{Truncated,BadMagic,UnsupportedFeature,InvalidGeometry,InvalidSequence,InvalidTag,InvalidRevoke,OutOfRange,Io,NoScratch,TransactionTooLarge}
fn be32(b:&[u8],o:usize)->Result<u32,Error>{let e=o.checked_add(4).ok_or(Error::OutOfRange)?;let s=b.get(o..e).ok_or(Error::Truncated)?;Ok(u32::from_be_bytes([s[0],s[1],s[2],s[3]]))}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct JournalSuperblock{pub block_size:u32,pub max_len:u32,pub first:u32,pub sequence:u32,pub start:u32,pub feature_incompat:u32,pub block_count_64:bool}
impl JournalSuperblock{
 pub fn parse(b:&[u8])->Result<Self,Error>{
  if b.len()<1024{return Err(Error::Truncated)}
  if be32(b,0)?!=JBD2_MAGIC||be32(b,4)?!=JBD2_SUPERBLOCK_V2{return Err(Error::BadMagic)}
  let block_size=be32(b,12)?;let max_len=be32(b,16)?;let first=be32(b,20)?;let sequence=be32(b,24)?;let start=be32(b,28)?;let feature_incompat=be32(b,40)?;
  if block_size as usize!=b.len()||block_size<1024||!block_size.is_power_of_two()||max_len<2||first==0||first>=max_len||start>=max_len{return Err(Error::InvalidGeometry)}
  if feature_incompat&!SUPPORTED_INCOMPAT!=0{return Err(Error::UnsupportedFeature)}
  Ok(Self{block_size,max_len,first,sequence,start,feature_incompat,block_count_64:feature_incompat&FEATURE_INCOMPAT_64BIT!=0})
 }
 pub fn needs_recovery(&self)->bool{self.start!=0}
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
struct Tag{target:u64,journal_data:u32,flags:u32}
impl Tag{const EMPTY:Self=Self{target:0,journal_data:0,flags:0};}
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
 let bs=sb.block_size as usize;if scratch.len()!=bs||payload.len()!=bs{return Err(Error::NoScratch)}
 if !sb.needs_recovery(){return Ok(ReplayReport::default())}
 let mut report=ReplayReport::default();let mut cursor=sb.start;let mut expected=sb.sequence;
 let mut tags=[Tag::EMPTY;MAX_TRANSACTION_TAGS];let mut revokes=[0u64;MAX_TRANSACTION_REVOKES];let mut guard=0u32;
 while guard<sb.max_len{
  guard+=1;read_journal(d,m,cursor,scratch)?;
  if be32(scratch,0)?!=JBD2_MAGIC||be32(scratch,8)?!=expected{report.stopped_on_incomplete=true;break}
  let mut tag_count=0usize;let mut revoke_count=0usize;let mut tx_cursor=cursor;let mut committed=false;let mut malformed=false;let mut tx_guard=0u32;
  while tx_guard<sb.max_len{
   tx_guard+=1;read_journal(d,m,tx_cursor,scratch)?;
   if be32(scratch,0)?!=JBD2_MAGIC||be32(scratch,8)?!=expected{malformed=true;break}
   match be32(scratch,4)?{
    JBD2_DESCRIPTOR_BLOCK=>{
     let mut off=12usize;
     loop{
      if tag_count>=MAX_TRANSACTION_TAGS{return Err(Error::TransactionTooLarge)}
      let mut target=be32(scratch,off)? as u64;let flags=be32(scratch,off+4)?;off=off.checked_add(8).ok_or(Error::OutOfRange)?;
      if sb.block_count_64{target|=(be32(scratch,off)? as u64)<<32;off=off.checked_add(4).ok_or(Error::OutOfRange)?}
      if flags&TAG_SAME_UUID==0{off=off.checked_add(16).ok_or(Error::OutOfRange)?;if off>bs{return Err(Error::InvalidTag)}}
      if target>=d.filesystem_blocks(){return Err(Error::OutOfRange)}
      let data_block=next_log(sb,tx_cursor);tags[tag_count]=Tag{target,journal_data:data_block,flags};tag_count+=1;
      if flags&TAG_DELETED==0{tx_cursor=data_block;read_journal(d,m,tx_cursor,payload)?;tx_cursor=next_log(sb,tx_cursor)}
      if flags&TAG_LAST!=0{break}
      if off>bs{return Err(Error::InvalidTag)}
     }
    }
    JBD2_REVOKE_BLOCK=>{
     let count=be32(scratch,12)? as usize;let width=if sb.block_count_64{8}else{4};
     if count<16||count>bs||(count-16)%width!=0{return Err(Error::InvalidRevoke)}
     let n=(count-16)/width;if revoke_count.checked_add(n).ok_or(Error::OutOfRange)?>MAX_TRANSACTION_REVOKES{return Err(Error::TransactionTooLarge)}
     for i in 0..n{let o=16+i*width;let block=if width==8{((be32(scratch,o+4)? as u64)<<32)|(be32(scratch,o)? as u64)}else{be32(scratch,o)? as u64};if block>=d.filesystem_blocks(){return Err(Error::OutOfRange)}revokes[revoke_count]=block;revoke_count+=1}
     tx_cursor=next_log(sb,tx_cursor);
    }
    JBD2_COMMIT_BLOCK=>{committed=true;tx_cursor=next_log(sb,tx_cursor);break}
    _=>{malformed=true;break}
   }
  }
  report.transactions_seen+=1;if malformed||!committed{report.stopped_on_incomplete=true;break}
  for tag in tags[..tag_count].iter().copied(){
   if tag.flags&TAG_DELETED!=0{continue}
   if revokes[..revoke_count].iter().any(|b|*b==tag.target){report.blocks_revoked+=1;continue}
   read_journal(d,m,tag.journal_data,payload)?;write_target(d,tag.target,payload,tag.flags&TAG_ESCAPE!=0)?;report.blocks_replayed+=1;
  }
  d.flush().map_err(|_|Error::Io)?;report.transactions_replayed+=1;expected=expected.wrapping_add(1);cursor=tx_cursor;
  if cursor==sb.start{break}
 }
 Ok(report)
}
#[cfg(test)]
mod tests{
 use super::*;
 #[test]fn journal_superblock_checks_geometry_and_unsupported_features(){
  let mut b=[0u8;1024];b[0..4].copy_from_slice(&JBD2_MAGIC.to_be_bytes());b[4..8].copy_from_slice(&JBD2_SUPERBLOCK_V2.to_be_bytes());b[12..16].copy_from_slice(&1024u32.to_be_bytes());b[16..20].copy_from_slice(&64u32.to_be_bytes());b[20..24].copy_from_slice(&1u32.to_be_bytes());b[24..28].copy_from_slice(&7u32.to_be_bytes());b[28..32].copy_from_slice(&3u32.to_be_bytes());
  let sb=JournalSuperblock::parse(&b).unwrap();assert!(sb.needs_recovery());b[40..44].copy_from_slice(&FEATURE_INCOMPAT_CSUM_V3.to_be_bytes());assert_eq!(JournalSuperblock::parse(&b),Err(Error::UnsupportedFeature));
 }
 #[test]fn clean_journal_is_noop_without_io(){
  struct D;impl ReplayDevice for D{fn filesystem_blocks(&self)->u64{10}fn read_block(&mut self,_:u64,_:&mut[u8])->Result<(),()>{Err(())}fn write_block(&mut self,_:u64,_:&[u8])->Result<(),()>{Err(())}fn flush(&mut self)->Result<(),()>{Ok(())}}
  let sb=JournalSuperblock{block_size:1024,max_len:64,first:1,sequence:1,start:0,feature_incompat:0,block_count_64:false};let mut d=D;let mut a=[0u8;1024];let mut b=[0u8;1024];assert_eq!(replay(&mut d,&mut |_|Err(()),&sb,&mut a,&mut b),Ok(ReplayReport::default()));
 }
}
