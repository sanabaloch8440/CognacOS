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
 /// Reuse immutable hive bytes after the caller has validated this exact image
 /// with `open`; the supplied metadata must come from that validation.
 ///
 /// # Safety
 /// `bytes` must be unchanged from the validated image for the lifetime of
 /// the returned view, and `info` must be the BaseInfo parsed from those bytes.
 pub unsafe fn from_validated(bytes:&'a [u8], info:BaseInfo)->Self { Self { bytes, info } }
 pub fn open(bytes:&'a[u8])->Result<Self,Error>{
  if bytes.len()<BASE_BLOCK_SIZE||bytes.get(0..4)!=Some(&b"regf"[..]){return Err(Error::BadSignature)}
  let sequence_primary=u32at(bytes,4)?;let sequence_secondary=u32at(bytes,8)?;
  if sequence_primary!=sequence_secondary{return Err(Error::DirtyHive)}
  let major=u32at(bytes,0x14)?;let minor=u32at(bytes,0x18)?;
  if major!=1||minor<3||minor>6||u32at(bytes,0x20)?!=1{return Err(Error::UnsupportedVersion)}
  let bins_size=u32at(bytes,0x28)?;
  let root_cell=u32at(bytes,0x24)?;
  if bins_size<0x1000||bins_size as usize%0x1000!=0||BASE_BLOCK_SIZE.checked_add(bins_size as usize).ok_or(Error::InvalidHiveSize)?>bytes.len(){return Err(Error::InvalidHiveSize)}
  let stored=u32at(bytes,0x1fc)?;let mut checksum=0u32;for off in (0..0x1fc).step_by(4){checksum^=u32at(bytes,off)?;}
  let expected=if checksum==0{1}else if checksum==u32::MAX{u32::MAX-1}else{checksum};
  if expected!=stored{return Err(Error::BadChecksum)}
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
 fn key<'h>(&'h self,index:u32)->Result<Key<'h, 'a>,Error>{
  let c=self.cell(index)?;if c.get(0..2)!=Some(&b"nk"[..])||c.len()<0x4c{return Err(Error::InvalidKey)}
  let flags=u16at(c,2)?;let name_len=u16at(c,0x48)? as usize;
  if name_len>c.len()-0x4c{return Err(Error::InvalidKey)}
  Ok(Key{hive:self,index,cell:c,flags,name:&c[0x4c..0x4c+name_len]})
 }
 pub fn root<'h>(&'h self)->Result<Key<'h, 'a>,Error>{self.key(self.info.root_cell)}
 pub fn key_at<'h>(&'h self,index:u32)->Result<Key<'h, 'a>,Error>{self.key(index)}
 pub fn find_path<'h>(&'h self,path:&[u16])->Result<Key<'h, 'a>,Error>{
  let mut current=self.root()?;let mut start=0;
  while start<path.len(){while start<path.len()&&path[start]==b'\\' as u16{start+=1}if start==path.len(){break}
   let mut end=start;while end<path.len()&&path[end]!=b'\\' as u16{end+=1}
   current=current.find_subkey(&path[start..end])?;start=end;
  }Ok(current)
 }
 pub fn raw_bytes(&self)->&'a[u8]{self.bytes}
}
#[derive(Clone,Copy)]
pub struct Key<'h,'a>{hive:&'h Hive<'a>,index:u32,cell:&'a[u8],flags:u16,name:&'a[u8]}
impl<'h,'a> Key<'h,'a>{
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
 pub fn find_path(&self,path:&[u16])->Result<Key<'h, 'a>,Error>{
  let mut current=*self;let mut start=0;
  while start<path.len(){while start<path.len()&&path[start]==b'\\' as u16{start+=1}if start==path.len(){break}
   let mut end=start;while end<path.len()&&path[end]!=b'\\' as u16{end+=1}
   current=current.find_subkey(&path[start..end])?;start=end;
  }Ok(current)
 }
 pub fn subkey_count(&self)->Result<u32,Error>{u32at(self.cell,0x14)}
 pub fn find_subkey(&self,name:&[u16])->Result<Key<'h, 'a>,Error>{
  let count=u32at(self.cell,0x14)?;let list=u32at(self.cell,0x1c)?;
  if count>1_000_000{return Err(Error::InvalidKey)}if count==0{return Err(Error::NotFound)}
  self.search_index(list,count,name,0)
 }
 fn search_index(&self,list:u32,count:u32,name:&[u16],depth:u8)->Result<Key<'h, 'a>,Error>{
  if depth>8{return Err(Error::InvalidKey)}let c=self.hive.cell(list)?;let sig=c.get(0..2).ok_or(Error::InvalidKey)?;
  if sig==b"li"{
   if c.len()<4+count as usize*4 || u16at(c,2)? as u32 != count {return Err(Error::InvalidKey)}
   for i in 0..count{let k=self.hive.key(u32at(c,4+i as usize*4)?)?;if key_matches(&k,name)?{return Ok(k)}}
  }else if sig==b"lf"||sig==b"lh"{
   if c.len()<4+count as usize*8 || u16at(c,2)? as u32 != count {return Err(Error::InvalidKey)}
   for i in 0..count{let k=self.hive.key(u32at(c,4+i as usize*8)?)?;if key_matches(&k,name)?{return Ok(k)}}
  }else if sig==b"ri"{
   let n=(c.len().saturating_sub(4))/4;
   if u16at(c,2)? as usize != n {return Err(Error::InvalidKey)}
   for i in 0..n{let sub=u32at(c,4+i*4)?;let sc=self.hive.cell(sub)?;let ss=sc.get(0..2).ok_or(Error::InvalidKey)?;
    let entries=if ss==b"li"||ss==b"ri"{sc.len().saturating_sub(4)/4}else if ss==b"lf"||ss==b"lh"{sc.len().saturating_sub(4)/8}else{return Err(Error::UnsupportedIndex)} as u32;
    if let Ok(k)=self.search_index(sub,entries,name,depth+1){return Ok(k)}
   }
  }else{return Err(Error::UnsupportedIndex)}
  Err(Error::NotFound)
 }
 pub fn value_count(&self)->Result<u32,Error>{u32at(self.cell,0x24)}
 pub fn find_value(&self,name:&[u16])->Result<Value<'_, 'a>,Error>{
  let count=self.value_count()?;if count>1_000_000{return Err(Error::InvalidKey)}if count==0{return Err(Error::NotFound)}
  let list=u32at(self.cell,0x28)?;let c=self.hive.cell(list)?;if c.len()<count as usize*4{return Err(Error::InvalidValue)}
  for i in 0..count{let v=Value::parse(self.hive,u32at(c,i as usize*4)?)?;if v.name_matches(name)?{return Ok(v)}}Err(Error::NotFound)
 }
}
fn key_matches(k:&Key<'_, '_>,name:&[u16])->Result<bool,Error>{let mut temp=[0u16;512];let n=k.name_utf16(&mut temp)?;Ok(eq_name(&temp[..n],name))}
#[derive(Clone,Copy)]
pub struct Value<'h,'a>{hive:&'h Hive<'a>,cell:&'a[u8],name:&'a[u8],compressed:bool,pub kind:u32,pub data_len:u32,data_offset:u32,inline:bool}
impl<'h,'a> Value<'h,'a>{
 fn parse(hive:&'h Hive<'a>,index:u32)->Result<Self,Error>{
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
  let c=self.hive.cell(self.data_offset)?;
  if c.len()>=n{out[..n].copy_from_slice(&c[..n]);return Ok(n)}
  // Windows uses a "db" big-data cell and a segment-list cell for large values.
  if c.get(0..2)!=Some(&b"db"[..])||c.len()<8{return Err(Error::InvalidValue)}
  let segments=u16at(c,2)? as usize;let list_index=u32at(c,4)?;let list=self.hive.cell(list_index)?;
  if segments==0||list.len()<segments.checked_mul(4).ok_or(Error::InvalidValue)?{return Err(Error::InvalidValue)}
  let mut copied=0usize;
  for i in 0..segments{if copied==n{break;}let segment_index=u32at(list,i*4)?;let segment=self.hive.cell(segment_index)?;let take=core::cmp::min(segment.len(),n-copied);out[copied..copied+take].copy_from_slice(&segment[..take]);copied+=take;}
  if copied!=n{return Err(Error::InvalidValue)}Ok(n)
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


/// Writable REGF image operations. The caller must exclusively own the backing
/// bytes and persist the returned image to stable storage in the documented order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirtyHiveState {
    pub dirty: bool,
    pub sequence: u32,
    pub dirty_generation: u64,
}
pub struct WritableHive<'a> {
    bytes: &'a mut [u8],
    info: BaseInfo,
    dirty: bool,
    generation: u64,
}
impl<'a> WritableHive<'a> {
    pub fn open(bytes: &'a mut [u8]) -> Result<Self, Error> {
        let parsed = Hive::open(bytes)?;
        let info = parsed.info;
        Ok(Self { bytes, info, dirty: false, generation: 0 })
    }
    pub fn state(&self) -> DirtyHiveState {
        DirtyHiveState { dirty: self.dirty, sequence: self.info.sequence_primary, dirty_generation: self.generation }
    }
    pub fn bytes(&self) -> &[u8] { self.bytes }
    pub fn bytes_mut_for_persistence(&mut self) -> &mut [u8] { self.bytes }

    /// Mark the hive dirty before modifying any cell. The primary/secondary
    /// sequence mismatch is the Windows base-block dirty indicator.
    pub fn begin_update(&mut self) -> Result<u32, Error> {
        if self.dirty { return Ok(self.info.sequence_primary); }
        let next = self.info.sequence_primary.wrapping_add(1).max(1);
        put32(self.bytes, 4, next);
        put32(self.bytes, 8, self.info.sequence_secondary);
        self.info.sequence_primary = next;
        self.dirty = true;
        self.generation = self.generation.wrapping_add(1);
        self.recompute_checksum()?;
        Ok(next)
    }

    /// Complete a successful transaction. A caller persisting to disk should
    /// flush cell changes before this base block and flush the base block last.
    pub fn commit_update(&mut self) -> Result<u32, Error> {
        if !self.dirty { return Ok(self.info.sequence_primary); }
        put32(self.bytes, 8, self.info.sequence_primary);
        self.info.sequence_secondary = self.info.sequence_primary;
        self.recompute_checksum()?;
        self.dirty = false;
        Ok(self.info.sequence_primary)
    }

    pub fn recompute_checksum(&mut self) -> Result<u32, Error> {
        if self.bytes.len() < BASE_BLOCK_SIZE { return Err(Error::Truncated); }
        let mut checksum = 0u32;
        for off in (0..0x1fc).step_by(4) { checksum ^= u32at(self.bytes, off)?; }
        let checksum = if checksum == 0 { 1 } else if checksum == u32::MAX { u32::MAX - 1 } else { checksum };
        put32(self.bytes, 0x1fc, checksum);
        Ok(checksum)
    }

    fn cell_absolute(&self, index: u32) -> Result<usize, Error> {
        let abs = BASE_BLOCK_SIZE.checked_add(index as usize).ok_or(Error::InvalidCell)?;
        if abs < BASE_BLOCK_SIZE + 0x20 || abs.checked_add(4).filter(|e| *e <= BASE_BLOCK_SIZE + self.info.bins_size as usize).is_none() {
            return Err(Error::InvalidCell);
        }
        Ok(abs)
    }

    /// Allocate a cell, splitting a free cell where possible. The returned
    /// index is stable relative to the first HBIN, as in native NK/VK records.
    pub fn allocate_cell(&mut self, payload_len: usize) -> Result<u32, Error> {
        if !self.dirty { return Err(Error::DirtyHive); }
        let needed = payload_len.checked_add(4).and_then(|n| n.checked_add(7)).ok_or(Error::InvalidCell)? & !7;
        if needed < 8 || needed > i32::MAX as usize { return Err(Error::InvalidCell); }
        let end = BASE_BLOCK_SIZE.checked_add(self.info.bins_size as usize).ok_or(Error::InvalidHiveSize)?;
        let mut bin = BASE_BLOCK_SIZE;
        while bin < end {
            if self.bytes.get(bin..bin+4) != Some(&b"hbin"[..]) { return Err(Error::InvalidBin); }
            let size = u32at(self.bytes, bin+8)? as usize;
            let mut pos = bin + 0x20;
            let bin_end = bin.checked_add(size).ok_or(Error::InvalidBin)?;
            while pos < bin_end {
                let raw = i32::from_le_bytes(self.bytes.get(pos..pos+4).ok_or(Error::Truncated)?.try_into().map_err(|_| Error::Truncated)?);
                if raw == 0 || raw == i32::MIN { return Err(Error::InvalidCell); }
                let cell_size = raw.unsigned_abs() as usize;
                if cell_size < 8 || pos.checked_add(cell_size).filter(|e| *e <= bin_end).is_none() { return Err(Error::InvalidCell); }
                if raw > 0 && cell_size >= needed {
                    let leftover = cell_size - needed;
                    let used = if leftover >= 8 { needed } else { cell_size };
                    put32(self.bytes, pos, (-(used as i32)) as u32);
                    self.bytes[pos+4..pos+used].fill(0);
                    if leftover >= 8 { put32(self.bytes, pos+used, leftover as u32); }
                    return u32::try_from(pos - BASE_BLOCK_SIZE).map_err(|_| Error::InvalidCell);
                }
                pos += cell_size;
            }
            if pos != bin_end { return Err(Error::InvalidBin); }
            bin = bin_end;
        }
        Err(Error::OutputTooSmall)
    }

    pub fn free_cell(&mut self, index: u32) -> Result<(), Error> {
        if !self.dirty { return Err(Error::DirtyHive); }
        let abs = self.cell_absolute(index)?;
        let raw = i32::from_le_bytes(self.bytes.get(abs..abs+4).ok_or(Error::Truncated)?.try_into().map_err(|_| Error::Truncated)?);
        if raw >= 0 || raw == i32::MIN { return Err(Error::InvalidCell); }
        let size = raw.unsigned_abs() as usize;
        let end = abs.checked_add(size).ok_or(Error::InvalidCell)?;
        if end > BASE_BLOCK_SIZE + self.info.bins_size as usize { return Err(Error::InvalidCell); }
        put32(self.bytes, abs, size as u32);
        self.bytes[abs+4..end].fill(0);
        Ok(())
    }

    fn allocated_cell_mut(&mut self, index: u32) -> Result<&mut [u8], Error> {
        let abs = self.cell_absolute(index)?;
        let raw = i32::from_le_bytes(self.bytes.get(abs..abs+4).ok_or(Error::Truncated)?.try_into().map_err(|_| Error::Truncated)?);
        if raw >= 0 || raw == i32::MIN { return Err(Error::InvalidCell); }
        let size = raw.unsigned_abs() as usize;
        if size < 8 { return Err(Error::InvalidCell); }
        let end = abs.checked_add(size).ok_or(Error::InvalidCell)?;
        if end > BASE_BLOCK_SIZE + self.info.bins_size as usize { return Err(Error::InvalidCell); }
        self.bytes.get_mut(abs+4..end).ok_or(Error::Truncated)
    }

    /// Append a subkey cell index to an NK record. Rebuilds the index as a
    /// native "li" list and updates the stable NK cell's count/list fields.
    pub fn append_subkey_index(&mut self, parent_nk: u32, child_nk: u32) -> Result<u32, Error> {
        if !self.dirty { return Err(Error::DirtyHive); }
        let parent_abs = self.cell_absolute(parent_nk)?;
        let (count, old_list) = {
            let raw = i32::from_le_bytes(self.bytes.get(parent_abs..parent_abs+4).ok_or(Error::Truncated)?.try_into().map_err(|_| Error::Truncated)?);
            if raw >= 0 || raw == i32::MIN { return Err(Error::InvalidCell); }
            let len = raw.unsigned_abs() as usize - 4;
            let parent_cell = self.bytes.get(parent_abs+4..parent_abs+4+len).ok_or(Error::Truncated)?;
            if parent_cell.get(0..2) != Some(&b"nk"[..]) || parent_cell.len() < 0x4c { return Err(Error::InvalidKey); }
            (u32at(parent_cell, 0x14)?, u32at(parent_cell, 0x1c)?)
        };
        if count >= 0xffff { return Err(Error::InvalidKey); }
        let mut entries = [0u32; 4096];
        if count as usize >= entries.len() { return Err(Error::InvalidKey); }
        if count != 0 {
            let old = self.cell_payload(old_list)?;
            let sig = old.get(0..2).ok_or(Error::InvalidKey)?;
            if sig == b"li" {
                if old.len() < 4 + count as usize * 4 || u16at(old, 2)? as u32 != count { return Err(Error::InvalidKey); }
                for i in 0..count as usize { entries[i] = u32at(old, 4+i*4)?; }
            } else if sig == b"lf" || sig == b"lh" {
                if old.len() < 4 + count as usize * 8 || u16at(old, 2)? as u32 != count { return Err(Error::InvalidKey); }
                for i in 0..count as usize { entries[i] = u32at(old, 4+i*8)?; }
            } else { return Err(Error::UnsupportedIndex); }
        }
        entries[count as usize] = child_nk;
        let payload_len = 4 + (count as usize + 1) * 4;
        let list = self.allocate_cell(payload_len)?;
        {
            let dst = self.allocated_cell_mut(list)?;
            dst[0..2].copy_from_slice(b"li");
            put16(dst, 2, (count+1) as u16);
            for i in 0..=count as usize { put32(dst, 4+i*4, entries[i]); }
        }
        {
            let dst = self.allocated_cell_mut(parent_nk)?;
            put32(dst, 0x14, count+1);
            put32(dst, 0x1c, list);
        }
        if count != 0 && old_list != u32::MAX { let _ = self.free_cell(old_list); }
        Ok(list)
    }

    /// Append a VK cell index to an NK record's value list.
    pub fn append_value_index(&mut self, key_nk: u32, value_vk: u32) -> Result<u32, Error> {
        if !self.dirty { return Err(Error::DirtyHive); }
        let abs = self.cell_absolute(key_nk)?;
        let (count, old_list) = {
            let raw = i32::from_le_bytes(self.bytes.get(abs..abs+4).ok_or(Error::Truncated)?.try_into().map_err(|_| Error::Truncated)?);
            if raw >= 0 || raw == i32::MIN { return Err(Error::InvalidCell); }
            let len = raw.unsigned_abs() as usize - 4;
            let cell = self.bytes.get(abs+4..abs+4+len).ok_or(Error::Truncated)?;
            if cell.get(0..2) != Some(&b"nk"[..]) || cell.len() < 0x2c { return Err(Error::InvalidKey); }
            (u32at(cell,0x24)?,u32at(cell,0x28)?)
        };
        if count >= 0xffff { return Err(Error::InvalidValue); }
        let mut entries=[0u32;4096];
        if count as usize >= entries.len(){return Err(Error::InvalidValue);}
        if count != 0 {
            let old=self.cell_payload(old_list)?;
            if old.len()<count as usize*4{return Err(Error::InvalidValue);}
            for i in 0..count as usize { entries[i]=u32at(old,i*4)?; }
        }
        entries[count as usize]=value_vk;
        let list=self.allocate_cell((count as usize+1)*4)?;
        {
            let dst=self.allocated_cell_mut(list)?;
            for i in 0..=count as usize {put32(dst,i*4,entries[i]);}
        }
        {
            let dst=self.allocated_cell_mut(key_nk)?;
            put32(dst,0x24,count+1);put32(dst,0x28,list);
        }
        if count!=0&&old_list!=u32::MAX {let _=self.free_cell(old_list);}
        Ok(list)
    }

    fn cell_payload(&self, index:u32)->Result<&[u8],Error>{
        let abs=self.cell_absolute(index)?;
        let raw=i32::from_le_bytes(self.bytes.get(abs..abs+4).ok_or(Error::Truncated)?.try_into().map_err(|_|Error::Truncated)?);
        if raw>=0||raw==i32::MIN{return Err(Error::InvalidCell);}
        let size=raw.unsigned_abs() as usize;
        if size<8{return Err(Error::InvalidCell);}
        self.bytes.get(abs+4..abs+size).ok_or(Error::Truncated)
    }

    /// Update the base-block checksum after cell changes without clearing dirty
    /// state; used when a durable writer persists the dirty image before commit.
    pub fn sync_dirty_base_block(&mut self) -> Result<u32, Error> { self.recompute_checksum() }
}

/// A bounded redo record for recovery. This is CognacOS's staging journal, not
/// the Windows HvLE transaction-log format; native HvLE replay is intentionally
/// kept separate until its checksum/hash and dirty-page semantics are validated.
pub fn replay_cognacos_redo_log(hive: &mut [u8], log: &[u8]) -> Result<usize, Error> {
    if log.len() < 16 || log.get(0..4) != Some(&b"CNL1"[..]) { return Err(Error::InvalidValue); }
    let sequence = u32at(log,4)?;
    let count = u32at(log,8)? as usize;
    let expected = u32at(log,12)?;
    if count > 4096 { return Err(Error::InvalidValue); }
    let mut checksum=0u32;
    for b in &log[16..] { checksum=checksum.rotate_left(5) ^ (*b as u32); }
    if checksum != expected { return Err(Error::BadChecksum); }
    let mut cursor=16usize;
    let mut records=0usize;
    for _ in 0..count {
        let off=u32at(log,cursor)? as usize; let len=u32at(log,cursor+4)? as usize; cursor=cursor.checked_add(8).ok_or(Error::InvalidValue)?;
        let data=log.get(cursor..cursor.checked_add(len).ok_or(Error::InvalidValue)?).ok_or(Error::Truncated)?;
        let end=off.checked_add(len).ok_or(Error::InvalidValue)?;
        if end > hive.len() || off < BASE_BLOCK_SIZE { return Err(Error::InvalidValue); }
        hive[off..end].copy_from_slice(data);
        cursor=cursor.checked_add(len).ok_or(Error::InvalidValue)?;
        records+=1;
    }
    if cursor != log.len() { return Err(Error::InvalidValue); }
    if hive.len() < BASE_BLOCK_SIZE || hive.get(0..4) != Some(&b"regf"[..]) { return Err(Error::BadSignature); }
    put32(hive,4,sequence);
    put32(hive,8,sequence);
    let mut checksum=0u32;
    for off in (0..0x1fc).step_by(4) { checksum ^= u32at(hive,off)?; }
    let checksum=if checksum==0 {1} else if checksum==u32::MAX {u32::MAX-1} else {checksum};
    put32(hive,0x1fc,checksum);
    Ok(records)
}

#[cfg(test)]
mod tests{
 use super::*;
 #[test]fn creates_openable_native_hive(){let mut bytes=[0u8;8192];let n=create_minimal_hive(&mut bytes).unwrap();assert_eq!(n,8192);let h=Hive::open(&bytes[..n]).unwrap();assert_eq!(h.info.root_cell,0x20);assert_eq!(h.root().unwrap().subkey_count().unwrap(),0);}
 #[test]fn mounts_named_hives_without_reencoding(){let mut bytes=[0u8;8192];let n=create_named_hive("SYSTEM",&mut bytes).unwrap();let mut set=HiveSet::new();set.mount("system",&bytes[..n]).unwrap();assert_eq!(set.mount_count(),1);assert_eq!(set.open("SYSTEM").unwrap().info.root_cell,0x20);assert!(create_named_hive("not-a-hive",&mut bytes).is_err());}
 #[test]fn rejects_bad_checksum(){let mut bytes=[0u8;8192];create_minimal_hive(&mut bytes).unwrap();bytes[0x1fc]^=0x10;assert!(matches!(Hive::open(&bytes),Err(Error::BadChecksum)));}
 #[test]fn writable_allocator_splits_free_cell_and_updates_sequence(){
  let mut bytes=[0u8;8192];create_minimal_hive(&mut bytes).unwrap();
  let mut h=WritableHive::open(&mut bytes).unwrap();
  assert!(!h.state().dirty);
  h.begin_update().unwrap();
  let cell=h.allocate_cell(0x4d).unwrap();
  assert_eq!(cell,0x70);
  { let nk=h.allocated_cell_mut(cell).unwrap(); nk[0..2].copy_from_slice(b"nk"); put16(nk,2,0x20); put16(nk,0x48,1); nk[0x4c]=b'X'; }
  h.append_subkey_index(0x20,cell).unwrap();
  h.commit_update().unwrap();
  let parsed=Hive::open(h.bytes()).unwrap();
  assert_eq!(parsed.root().unwrap().subkey_count().unwrap(),1);
  let mut name=[0u16;4];assert_eq!(parsed.root().unwrap().find_subkey(&[b'X' as u16]).unwrap().name_utf16(&mut name).unwrap(),1);
 }
 #[test]fn redo_log_replays_checked_patch_and_repairs_base_block(){
  let mut hive=[0u8;8192];create_minimal_hive(&mut hive).unwrap();
  let payload=[0x41u8,0x42,0x43,0x44];
  let mut log=[0u8;28];log[0..4].copy_from_slice(b"CNL1");put32(&mut log,4,2);put32(&mut log,8,1);put32(&mut log,16,BASE_BLOCK_SIZE as u32+0x80);put32(&mut log,20,4);log[24..28].copy_from_slice(&payload);
  let mut checksum=0u32;for b in &log[16..]{checksum=checksum.rotate_left(5)^(*b as u32);}put32(&mut log,12,checksum);
  assert_eq!(replay_cognacos_redo_log(&mut hive,&log).unwrap(),1);
  assert_eq!(&hive[BASE_BLOCK_SIZE+0x80..BASE_BLOCK_SIZE+0x84],&payload);
  assert_eq!(u32at(&hive,4).unwrap(),2);assert_eq!(u32at(&hive,8).unwrap(),2);
  assert!(Hive::open(&hive).is_ok());
 }
 #[test]fn rejects_dirty_hive_until_log_replay(){let mut bytes=[0u8;8192];create_minimal_hive(&mut bytes).unwrap();bytes[8..12].copy_from_slice(&2u32.to_le_bytes());assert!(matches!(Hive::open(&bytes),Err(Error::DirtyHive)));}
}
