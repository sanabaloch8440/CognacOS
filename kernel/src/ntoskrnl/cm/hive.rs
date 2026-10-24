//! Read-only parser and minimal creator for native Windows REGF registry hives.
//! Hive bytes are kept in their original on-disk representation; this module never
//! converts a Windows hive into a private database format.
#![allow(dead_code)]

pub const BASE_BLOCK_SIZE: usize = 4096;
pub const REGF_SIGNATURE: &[u8;4] = b"regf";
pub const STATUS_SUCCESS: i32 = 0;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error { Truncated, BadSignature, BadChecksum, DirtyHive, UnsupportedVersion, InvalidHiveSize, InvalidBin, InvalidCell, InvalidKey, NotFound, InvalidValue, BufferTooSmall, OutputTooSmall, UnsupportedIndex }

fn u16at(b:&[u8],o:usize)->Result<u16,Error>{let s=b.get(o..o.checked_add(2).ok_or(Error::Truncated)?).ok_or(Error::Truncated)?;Ok(u16::from_le_bytes([s[0],s[1]]))}
fn u32at(b:&[u8],o:usize)->Result<u32,Error>{let s=b.get(o..o.checked_add(4).ok_or(Error::Truncated)?).ok_or(Error::Truncated)?;Ok(u32::from_le_bytes([s[0],s[1],s[2],s[3]]))}
fn put16(b:&mut[u8],o:usize,v:u16){b[o..o+2].copy_from_slice(&v.to_le_bytes())}
fn put32(b:&mut[u8],o:usize,v:u32){b[o..o+4].copy_from_slice(&v.to_le_bytes())}
fn fold(c:u16)->u16{if (b'A' as u16..=b'Z' as u16).contains(&c){c+32}else{c}}
fn eq_name(a:&[u16],b:&[u16])->bool{a.len()==b.len()&&a.iter().zip(b).all(|(x,y)|fold(*x)==fold(*y))}

#[derive(Clone,Copy,Debug)]
pub struct BaseInfo { pub root_cell:u32,pub bins_size:u32,pub major:u32,pub minor:u32,pub sequence_primary:u32,pub sequence_secondary:u32 }

pub struct Hive<'a>{bytes:&'a [u8],pub info:BaseInfo}
impl<'a> Hive<'a>{
 pub fn open(bytes:&'a[u8])->Result<Self,Error>{
  if bytes.len()<BASE_BLOCK_SIZE||bytes.get(0..4)!=Some(&b"regf"[..]){return Err(Error::BadSignature)}
  let sequence_primary=u32at(bytes,4)?;let sequence_secondary=u32at(bytes,8)?;
  if sequence_primary!=sequence_secondary{return Err(Error::DirtyHive)}
  let major=u32at(bytes,0x14)?;let minor=u32at(bytes,0x18)?;
  if major!=1||minor<3||minor>6{return Err(Error::UnsupportedVersion)}
  let bins_size=u32at(bytes,0x28)?;
  let root_cell=u32at(bytes,0x24)?;
  if bins_size<0x1000||bins_size as usize%0x1000!=0||BASE_BLOCK_SIZE.checked_add(bins_size as usize).ok_or(Error::InvalidHiveSize)?>bytes.len(){return Err(Error::InvalidHiveSize)}
  let stored=u32at(bytes,0x1fc)?;let mut checksum=0u32;for off in (0..0x1fc).step_by(4){checksum^=u32at(bytes,off)?;}
  if checksum!=stored && !(checksum==0xffff_ffff&&stored==0xffff_ffff) && !(checksum==0&&stored==1){return Err(Error::BadChecksum)}
  let hive=Self{bytes,info:BaseInfo{root_cell,bins_size,major,minor,sequence_primary,sequence_secondary}};
  hive.validate_bins()?;hive.key(root_cell)?;Ok(hive)
 }
 fn validate_bins(&self)->Result<(),Error>{
  let end=BASE_BLOCK_SIZE+self.info.bins_size as usize;let mut pos=BASE_BLOCK_SIZE;
  while pos<end{
   if self.bytes.get(pos..pos+4)!=Some(&b"hbin"[..]){return Err(Error::InvalidBin)}
   let rel=u32at(self.bytes,pos+4)? as usize;let size=u32at(self.bytes,pos+8)? as usize;
   if rel!=pos-BASE_BLOCK_SIZE||size<0x1000||size%0x1000!=0||pos.checked_add(size).filter(|e|*e<=end).is_none(){return Err(Error::InvalidBin)}
   let mut cell=pos+0x20;let bin_end=pos+size;
   while cell<bin_end{
    let raw=i32::from_le_bytes(self.bytes.get(cell..cell+4).ok_or(Error::Truncated)?.try_into().map_err(|_|Error::Truncated)?);
    if raw==0||raw==i32::MIN{return Err(Error::InvalidCell)}
    let n=raw.unsigned_abs() as usize;
    if n<8||n%8!=0||cell.checked_add(n).filter(|e|*e<=bin_end).is_none(){return Err(Error::InvalidCell)}
    cell+=n;
   }
   if cell!=bin_end{return Err(Error::InvalidCell)}pos=bin_end;
  }
  if pos!=end{return Err(Error::InvalidBin)}Ok(())
 }
 fn cell(&self,index:u32)->Result<&'a[u8],Error>{
  if index==u32::MAX{return Err(Error::InvalidCell)}
  let abs=BASE_BLOCK_SIZE.checked_add(index as usize).ok_or(Error::InvalidCell)?;
  if abs<BASE_BLOCK_SIZE+0x20||abs+4>BASE_BLOCK_SIZE+self.info.bins_size as usize{return Err(Error::InvalidCell)}
  let raw=i32::from_le_bytes(self.bytes.get(abs..abs+4).ok_or(Error::Truncated)?.try_into().map_err(|_|Error::Truncated)?);
  if raw>=0||raw==i32::MIN{return Err(Error::InvalidCell)}
  let size=raw.unsigned_abs() as usize;
  let end=abs.checked_add(size).ok_or(Error::InvalidCell)?;
  if size<8||end>BASE_BLOCK_SIZE+self.info.bins_size as usize{return Err(Error::InvalidCell)}
  self.bytes.get(abs+4..end).ok_or(Error::Truncated)
 }
 fn key(&self,index:u32)->Result<Key<'a>,Error>{
  let c=self.cell(index)?;if c.get(0..2)!=Some(&b"nk"[..])||c.len()<0x4c{return Err(Error::InvalidKey)}
  let flags=u16at(c,2)?;let name_len=u16at(c,0x48)? as usize;
  if name_len>c.len()-0x4c{return Err(Error::InvalidKey)}
  Ok(Key{hive:self,index,cell:c,flags,name:&c[0x4c..0x4c+name_len]})
 }
 pub fn root(&self)->Result<Key<'a>,Error>{self.key(self.info.root_cell)}
 pub fn find_path(&self,path:&[u16])->Result<Key<'a>,Error>{
  let mut current=self.root()?;let mut start=0;
  while start<path.len(){while start<path.len()&&path[start]==b'\\' as u16{start+=1}if start==path.len(){break}
   let mut end=start;while end<path.len()&&path[end]!=b'\\' as u16{end+=1}
   current=current.find_subkey(&path[start..end])?;start=end;
  }Ok(current)
 }
 pub fn raw_bytes(&self)->&'a[u8]{self.bytes}
}
#[derive(Clone,Copy)]
pub struct Key<'a>{hive:&'a Hive<'a>,index:u32,cell:&'a[u8],flags:u16,name:&'a[u8]}
impl<'a> Key<'a>{
 pub fn cell_index(&self)->u32{self.index}
 pub fn name_bytes(&self)->&'a[u8]{self.name}
 pub fn is_compressed_name(&self)->bool{self.flags&0x20!=0}
 pub fn name_utf16(&self,out:&mut[u16])->Result<usize,Error>{
  let n=if self.is_compressed_name(){self.name.len()}else{if self.name.len()%2!=0{return Err(Error::InvalidKey)}self.name.len()/2};
  if out.len()<n{return Err(Error::OutputTooSmall)}
  if self.is_compressed_name(){for(i,b)in self.name.iter().enumerate(){out[i]=*b as u16;}}
  else{for i in 0..n{out[i]=u16::from_le_bytes([self.name[i*2],self.name[i*2+1]]);}}
  Ok(n)
 }
 pub fn subkey_count(&self)->Result<u32,Error>{u32at(self.cell,0x14)}
 pub fn find_subkey(&self,name:&[u16])->Result<Key<'a>,Error>{
  let count=u32at(self.cell,0x14)?;let list=u32at(self.cell,0x1c)?;
  if count>1_000_000{return Err(Error::InvalidKey)}if count==0{return Err(Error::NotFound)}
  self.search_index(list,count,name,0)
 }
 fn search_index(&self,list:u32,count:u32,name:&[u16],depth:u8)->Result<Key<'a>,Error>{
  if depth>8{return Err(Error::InvalidKey)}let c=self.hive.cell(list)?;let sig=c.get(0..2).ok_or(Error::InvalidKey)?;
  if sig==b"li"{
   if c.len()<2+count as usize*4{return Err(Error::InvalidKey)}
   for i in 0..count{let k=self.hive.key(u32at(c,2+i as usize*4)?)?;if key_matches(&k,name)?{return Ok(k)}}
  }else if sig==b"lf"||sig==b"lh"{
   if c.len()<2+count as usize*8{return Err(Error::InvalidKey)}
   for i in 0..count{let k=self.hive.key(u32at(c,2+i as usize*8)?)?;if key_matches(&k,name)?{return Ok(k)}}
  }else if sig==b"ri"{
   let n=(c.len().saturating_sub(2))/4;
   for i in 0..n{let sub=u32at(c,2+i*4)?;let sc=self.hive.cell(sub)?;let ss=sc.get(0..2).ok_or(Error::InvalidKey)?;
    let entries=if ss==b"li"||ss==b"ri"{sc.len().saturating_sub(2)/4}else if ss==b"lf"||ss==b"lh"{sc.len().saturating_sub(2)/8}else{return Err(Error::UnsupportedIndex)} as u32;
    if let Ok(k)=self.search_index(sub,entries,name,depth+1){return Ok(k)}
   }
  }else{return Err(Error::UnsupportedIndex)}
  Err(Error::NotFound)
 }
 pub fn value_count(&self)->Result<u32,Error>{u32at(self.cell,0x24)}
 pub fn find_value(&self,name:&[u16])->Result<Value<'a>,Error>{
  let count=self.value_count()?;if count>1_000_000{return Err(Error::InvalidKey)}if count==0{return Err(Error::NotFound)}
  let list=u32at(self.cell,0x28)?;let c=self.hive.cell(list)?;if c.len()<count as usize*4{return Err(Error::InvalidValue)}
  for i in 0..count{let v=Value::parse(self.hive,u32at(c,i as usize*4)?)?;if v.name_matches(name)?{return Ok(v)}}Err(Error::NotFound)
 }
}
fn key_matches(k:&Key<'_>,name:&[u16])->Result<bool,Error>{let mut temp=[0u16;512];let n=k.name_utf16(&mut temp)?;Ok(eq_name(&temp[..n],name))}
#[derive(Clone,Copy)]
pub struct Value<'a>{hive:&'a Hive<'a>,cell:&'a[u8],name:&'a[u8],compressed:bool,pub kind:u32,data_len:u32,data_offset:u32,inline:bool}
impl<'a> Value<'a>{
 fn parse(hive:&'a Hive<'a>,index:u32)->Result<Self,Error>{
  let c=hive.cell(index)?;if c.get(0..2)!=Some(&b"vk"[..])||c.len()<0x14{return Err(Error::InvalidValue)}
  let n=u16at(c,2)? as usize;if n>c.len().saturating_sub(0x14){return Err(Error::InvalidValue)}
  let flags=u16at(c,0x10)?;let raw_len=u32at(c,4)?;let inline=raw_len&0x8000_0000!=0;let data_len=raw_len&0x7fff_ffff;
  Ok(Self{hive,cell:c,name:&c[0x14..0x14+n],compressed:flags&1!=0,kind:u32at(c,0xc)?,data_len,data_offset:u32at(c,8)?,inline})
 }
 pub fn name_matches(&self,name:&[u16])->Result<bool,Error>{
  let n=if self.compressed{self.name.len()}else{if self.name.len()%2!=0{return Err(Error::InvalidValue)}self.name.len()/2};if n!=name.len(){return Ok(false)}
  for i in 0..n{let c=if self.compressed{self.name[i] as u16}else{u16::from_le_bytes([self.name[i*2],self.name[i*2+1]])};if fold(c)!=fold(name[i]){return Ok(false)}}Ok(true)
 }
 pub fn data(&self,out:&mut[u8])->Result<usize,Error>{
  let n=self.data_len as usize;if out.len()<n{return Err(Error::BufferTooSmall)}if n==0{return Ok(0)}
  if self.inline{if n>4{return Err(Error::InvalidValue)}out[..n].copy_from_slice(&self.data_offset.to_le_bytes()[..n]);return Ok(n)}
  let c=self.hive.cell(self.data_offset)?;if c.len()<n{return Err(Error::InvalidValue)}out[..n].copy_from_slice(&c[..n]);Ok(n)
 }
}


#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum HiveKind{System,Software,Security,Sam,Default,Components,Bcd}
pub const HIVE_NAMES:[&str;7]=["SYSTEM","SOFTWARE","SECURITY","SAM","DEFAULT","COMPONENTS","BCD00000000"];
pub fn hive_kind(name:&str)->Result<HiveKind,Error>{if name.eq_ignore_ascii_case("SYSTEM"){Ok(HiveKind::System)}else if name.eq_ignore_ascii_case("SOFTWARE"){Ok(HiveKind::Software)}else if name.eq_ignore_ascii_case("SECURITY"){Ok(HiveKind::Security)}else if name.eq_ignore_ascii_case("SAM"){Ok(HiveKind::Sam)}else if name.eq_ignore_ascii_case("DEFAULT"){Ok(HiveKind::Default)}else if name.eq_ignore_ascii_case("COMPONENTS"){Ok(HiveKind::Components)}else if name.eq_ignore_ascii_case("BCD00000000"){Ok(HiveKind::Bcd)}else{Err(Error::BadSignature)}}
pub struct HiveSet<'a>{entries:[Option<&'a[u8]>;7]}
impl<'a> HiveSet<'a>{
 pub const fn new()->Self{Self{entries:[None;7]}}
 pub fn mount(&mut self,name:&str,bytes:&'a[u8])->Result<(),Error>{let kind=hive_kind(name)?;let _=Hive::open(bytes)?;self.entries[kind as usize]=Some(bytes);Ok(())}
 pub fn open(&self,name:&str)->Result<Hive<'a>,Error>{let kind=hive_kind(name)?;Hive::open(self.entries[kind as usize].ok_or(Error::NotFound)?)}
 pub fn is_mounted(&self,name:&str)->bool{hive_kind(name).ok().and_then(|k|self.entries[k as usize]).is_some()}
 pub fn mount_count(&self)->usize{self.entries.iter().filter(|e|e.is_some()).count()}
}
pub fn create_named_hive(name:&str,out:&mut[u8])->Result<usize,Error>{let _=hive_kind(name)?;create_minimal_hive(out)}

pub fn create_minimal_hive(out:&mut[u8])->Result<usize,Error>{
 const TOTAL:usize=BASE_BLOCK_SIZE+0x1000;const ROOT_CELL:u32=0x20;
 if out.len()<TOTAL{return Err(Error::OutputTooSmall)}out[..TOTAL].fill(0);
 out[0..4].copy_from_slice(b"regf");put32(out,4,1);put32(out,8,1);put32(out,0x14,1);put32(out,0x18,5);put32(out,0x1c,0);put32(out,0x20,1);put32(out,0x24,ROOT_CELL);put32(out,0x28,0x1000);put32(out,0x2c,1);
 out[0x1000..0x1004].copy_from_slice(b"hbin");put32(out,0x1004,0);put32(out,0x1008,0x1000);
 // One allocated root NK cell and one free cell; offsets are relative to HBIN data.
 put32(out,0x1020,(-0x50i32) as u32);let nk=0x1024;out[nk..nk+2].copy_from_slice(b"nk");put16(out,nk+2,0x0002);
 put32(out,nk+0x10,0xffff_ffff);put32(out,nk+0x14,0);put32(out,nk+0x18,0);put32(out,nk+0x1c,0xffff_ffff);put32(out,nk+0x20,0xffff_ffff);put32(out,nk+0x24,0);put32(out,nk+0x28,0xffff_ffff);put32(out,nk+0x2c,0xffff_ffff);put32(out,nk+0x30,0xffff_ffff);put16(out,nk+0x48,0);put16(out,nk+0x4a,0);
 put32(out,0x1070,0x0f90);
 let mut checksum=0u32;for off in (0..0x1fc).step_by(4){checksum^=u32::from_le_bytes(out[off..off+4].try_into().map_err(|_|Error::Truncated)?);}if checksum==0{checksum=1}else if checksum==u32::MAX{checksum=0xffff_fffe}put32(out,0x1fc,checksum);
 Ok(TOTAL)
}

#[cfg(test)]
mod tests{
 use super::*;
 #[test]fn creates_openable_native_hive(){let mut bytes=[0u8;8192];let n=create_minimal_hive(&mut bytes).unwrap();assert_eq!(n,8192);let h=Hive::open(&bytes[..n]).unwrap();assert_eq!(h.info.root_cell,0x20);assert_eq!(h.root().unwrap().subkey_count().unwrap(),0);}
 #[test]fn mounts_named_hives_without_reencoding(){let mut bytes=[0u8;8192];let n=create_named_hive("SYSTEM",&mut bytes).unwrap();let mut set=HiveSet::new();set.mount("system",&bytes[..n]).unwrap();assert_eq!(set.mount_count(),1);assert_eq!(set.open("SYSTEM").unwrap().info.root_cell,0x20);assert!(create_named_hive("not-a-hive",&mut bytes).is_err());}\n #[test]fn rejects_bad_checksum(){let mut bytes=[0u8;8192];create_minimal_hive(&mut bytes).unwrap();bytes[0x1fc]^=0x10;assert!(matches!(Hive::open(&bytes),Err(Error::BadChecksum)));}
}
