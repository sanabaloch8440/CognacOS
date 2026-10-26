//! Writable REGF primitives for the Configuration Manager.
//! Callers must serialize mutations and provide durable hive + journal storage.
//! The redo journal here is CognacOS's storage protocol; it is not the Windows HvLE format.
const BASE: usize = 4096;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error { Truncated, BadSignature, BadChecksum, Dirty, InvalidCell, InvalidBin, InvalidKey, InvalidValue, NoSpace, Journal, UnsupportedIndex }
fn r16(b:&[u8],o:usize)->Result<u16,Error>{let x=b.get(o..o+2).ok_or(Error::Truncated)?;Ok(u16::from_le_bytes([x[0],x[1]]))}
fn r32(b:&[u8],o:usize)->Result<u32,Error>{let x=b.get(o..o+4).ok_or(Error::Truncated)?;Ok(u32::from_le_bytes([x[0],x[1],x[2],x[3]]))}
fn w16(b:&mut[u8],o:usize,v:u16){b[o..o+2].copy_from_slice(&v.to_le_bytes())}
fn w32(b:&mut[u8],o:usize,v:u32){b[o..o+4].copy_from_slice(&v.to_le_bytes())}
fn align8(n:usize)->Option<usize>{n.checked_add(7).map(|v|v&!7)}
fn checksum(image:&[u8])->u32{image.iter().fold(0x811c9dc5u32,|h,b|(h^*b as u32).wrapping_mul(0x01000193))}
fn fold(c:u16)->u16{if (65..=90).contains(&c){c+32}else{c}}
fn eq(a:&[u16],b:&[u16])->bool{a.len()==b.len()&&a.iter().zip(b).all(|(x,y)|fold(*x)==fold(*y))}

/// Validates basic REGF header/checksum before exposing mutable operations.
pub struct HiveWriter<'a>{bytes:&'a mut [u8]}
impl<'a> HiveWriter<'a>{
 pub fn open(bytes:&'a mut[u8])->Result<Self,Error>{
  if bytes.len()<BASE||bytes.get(0..4)!=Some(&b"regf"[..]){return Err(Error::BadSignature)}
  if r32(bytes,4)?!=r32(bytes,8)?{return Err(Error::Dirty)}
  let mut c=0u32;for o in (0..0x1fc).step_by(4){c^=r32(bytes,o)?;}
  let c=if c==0{1}else if c==u32::MAX{u32::MAX-1}else{c};
  if c!=r32(bytes,0x1fc)?{return Err(Error::BadChecksum)}
  let bins=r32(bytes,0x28)? as usize;
  if bins<0x1000||bins%0x1000!=0||BASE.checked_add(bins).filter(|e|*e<=bytes.len()).is_none(){return Err(Error::InvalidBin)}
  Ok(Self{bytes})
 }
 pub fn bytes(&self)->&[u8]{self.bytes}
 fn cell(&self,id:u32)->Result<&[u8],Error>{
  if id==u32::MAX{return Err(Error::InvalidCell)}
  let p=BASE.checked_add(id as usize).ok_or(Error::InvalidCell)?;
  let end=BASE+r32(self.bytes,0x28)? as usize;
  if p<BASE+0x20||p+4>end{return Err(Error::InvalidCell)}
  let n=i32::from_le_bytes(self.bytes.get(p..p+4).ok_or(Error::Truncated)?.try_into().map_err(|_|Error::Truncated)?);
  if n>=0||n==i32::MIN{return Err(Error::InvalidCell)}
  let n=n.unsigned_abs() as usize;
  if n<8||n%8!=0||p.checked_add(n).filter(|e|*e<=end).is_none(){return Err(Error::InvalidCell)}
  self.bytes.get(p+4..p+n).ok_or(Error::Truncated)
 }
 fn cell_mut(&mut self,id:u32)->Result<&mut[u8],Error>{
  if id==u32::MAX{return Err(Error::InvalidCell)}
  let p=BASE.checked_add(id as usize).ok_or(Error::InvalidCell)?;
  let end=BASE+r32(self.bytes,0x28)? as usize;
  if p<BASE+0x20||p+4>end{return Err(Error::InvalidCell)}
  let n=i32::from_le_bytes(self.bytes.get(p..p+4).ok_or(Error::Truncated)?.try_into().map_err(|_|Error::Truncated)?);
  if n>=0||n==i32::MIN{return Err(Error::InvalidCell)}
  let n=n.unsigned_abs() as usize;
  if n<8||p.checked_add(n).filter(|e|*e<=end).is_none(){return Err(Error::InvalidCell)}
  self.bytes.get_mut(p+4..p+n).ok_or(Error::Truncated)
 }
 /// Allocate from a free HBIN cell, splitting it while preserving 8-byte alignment.
 pub fn allocate_cell(&mut self,payload_len:usize)->Result<u32,Error>{
  let need=align8(payload_len.checked_add(4).ok_or(Error::InvalidCell)?).ok_or(Error::InvalidCell)?;
  let end=BASE+r32(self.bytes,0x28)? as usize;let mut bin=BASE;
  while bin<end{
   if self.bytes.get(bin..bin+4)!=Some(&b"hbin"[..]){return Err(Error::InvalidBin)}
   let size=r32(self.bytes,bin+8)? as usize;let be=bin.checked_add(size).ok_or(Error::InvalidBin)?;
   if size<0x1000||be>end{return Err(Error::InvalidBin)}
   let mut p=bin+0x20;
   while p<be{
    let raw=i32::from_le_bytes(self.bytes.get(p..p+4).ok_or(Error::Truncated)?.try_into().map_err(|_|Error::Truncated)?);
    if raw==0||raw==i32::MIN{return Err(Error::InvalidCell)}
    let n=raw.unsigned_abs() as usize;
    if n<8||n%8!=0||p.checked_add(n).filter(|e|*e<=be).is_none(){return Err(Error::InvalidCell)}
    if raw>0&&n>=need{
     let used=if n-need>=8{need}else{n};
     w32(self.bytes,p,(-(used as i32)) as u32);self.bytes[p+4..p+used].fill(0);
     if n>used{w32(self.bytes,p+used,(n-used) as u32);self.bytes[p+used+4..p+n].fill(0);}
     return u32::try_from(p-BASE).map_err(|_|Error::InvalidCell)
    }p+=n;
   }if p!=be{return Err(Error::InvalidBin)}bin=be;
  }Err(Error::NoSpace)
 }
 fn encode_name(name:&[u16])->Result<([u8;1024],usize,bool),Error>{
  if name.len()>512{return Err(Error::InvalidKey)}let compressed=name.iter().all(|c|*c<=255);let mut b=[0;1024];
  if compressed{for(i,c)in name.iter().enumerate(){b[i]=*c as u8}Ok((b,name.len(),true))}
  else{for(i,c)in name.iter().enumerate(){b[i*2..i*2+2].copy_from_slice(&c.to_le_bytes())}Ok((b,name.len()*2,false))}
 }
 /// Create an NK cell and publish it through a replacement LI subkey index.
 /// Old index cells are retained to avoid dangling readers during publication.
 pub fn create_subkey(&mut self,parent:u32,name:&[u16])->Result<u32,Error>{
  if name.is_empty(){return Err(Error::InvalidKey)}let(enc,n,compressed)=Self::encode_name(name)?;
  let p=self.cell(parent)?;if p.get(0..2)!=Some(&b"nk"[..])||p.len()<0x4c{return Err(Error::InvalidKey)}
  let count=r32(p,0x14)?;let old_list=r32(p,0x1c)?;
  if count>0{
   let list=self.cell(old_list)?;if list.get(0..2)!=Some(&b"li"[..])||list.len()<2+count as usize*4{return Err(Error::UnsupportedIndex)}
   for i in 0..count{let id=r32(list,2+i as usize*4)?;let c=self.cell(id)?;if c.get(0..2)!=Some(&b"nk"[..])||c.len()<0x4c{return Err(Error::InvalidKey)}
    let flags=r16(c,2)?;let len=r16(c,0x48)? as usize;let bytes=c.get(0x4c..0x4c+len).ok_or(Error::InvalidKey)?;
    let mut old=[0u16;512];let units=if flags&0x20!=0{for(j,b)in bytes.iter().enumerate(){old[j]=*b as u16}len}else{if len%2!=0{return Err(Error::InvalidKey)}for j in 0..len/2{old[j]=u16::from_le_bytes([bytes[j*2],bytes[j*2+1]])}len/2};
    if eq(&old[..units],name){return Err(Error::InvalidKey)}
   }
  }
  let id=self.allocate_cell(0x4c+n)?;
  {let c=self.cell_mut(id)?;c[..2].copy_from_slice(b"nk");w16(c,2,if compressed{0x20}else{0});w32(c,0x10,parent);w32(c,0x14,0);w32(c,0x18,0);w32(c,0x1c,u32::MAX);w32(c,0x20,u32::MAX);w32(c,0x24,0);w32(c,0x28,u32::MAX);w32(c,0x2c,u32::MAX);w32(c,0x30,u32::MAX);w16(c,0x48,n as u16);w16(c,0x4a,0);c[0x4c..0x4c+n].copy_from_slice(&enc[..n]);}
  let new_count=count.checked_add(1).ok_or(Error::InvalidKey)?;if new_count>u16::MAX as u32{return Err(Error::InvalidKey)}
  let list_id=self.allocate_cell(2+new_count as usize*4)?;
  let mut old=[0u8;4096];let old_n=count as usize*4;if old_n>old.len(){return Err(Error::InvalidKey)}
  if count>0{let list=self.cell(old_list)?;if list.get(0..2)!=Some(&b"li"[..])||list.len()<2+old_n{return Err(Error::UnsupportedIndex)}old[..old_n].copy_from_slice(&list[2..2+old_n]);}
  {let c=self.cell_mut(list_id)?;c[..2].copy_from_slice(b"li");w16(c,2,new_count as u16);c[4..4+old_n].copy_from_slice(&old[..old_n]);w32(c,2+old_n,id);}
  let p=self.cell_mut(parent)?;w32(p,0x14,new_count);w32(p,0x1c,list_id);Ok(id)
 }
 /// Set a value. Replacements keep the existing list slot; new values append
 /// to a replacement value-list cell. Value/data cells are allocated before publication.
 pub fn set_value(&mut self,key_id:u32,name:&[u16],kind:u32,data:&[u8])->Result<(),Error>{
  let(enc,n,compressed)=Self::encode_name(name)?;let k=self.cell(key_id)?;
  if k.get(0..2)!=Some(&b"nk"[..])||k.len()<0x4c{return Err(Error::InvalidKey)}
  let count=r32(k,0x24)?;let old_list=r32(k,0x28)?;let mut slot=None;
  if count>0{let list=self.cell(old_list)?;if list.len()<count as usize*4{return Err(Error::InvalidValue)}
   for i in 0..count{let v=self.cell(r32(list,i as usize*4)?)?;if v.get(0..2)!=Some(&b"vk"[..])||v.len()<0x14{return Err(Error::InvalidValue)}
    let len=r16(v,2)? as usize;let flags=r16(v,0x10)?;let mut found=false;
    if flags&1!=0{found=len==name.len()&&v[0x14..0x14+len].iter().zip(name).all(|(a,b)|fold(*a as u16)==fold(*b))}
    else if len%2==0&&len==name.len()*2{found=v[0x14..0x14+len].chunks_exact(2).zip(name).all(|(a,b)|fold(u16::from_le_bytes([a[0],a[1]]))==fold(*b))}
    if found{slot=Some(i);break}
   }
  }
  let inline=data.len()<=4;let data_id=if inline||data.is_empty(){u32::MAX}else{let d=self.allocate_cell(data.len())?;self.cell_mut(d)?.get_mut(..data.len()).ok_or(Error::InvalidValue)?.copy_from_slice(data);d};
  let vk_id=self.allocate_cell(0x14+n)?;
  {let v=self.cell_mut(vk_id)?;v[..2].copy_from_slice(b"vk");w16(v,2,n as u16);w32(v,4,if inline{data.len() as u32|0x8000_0000}else{data.len() as u32});
   let field=if inline{let mut d=[0u8;4];d[..data.len()].copy_from_slice(data);u32::from_le_bytes(d)}else{data_id};w32(v,8,field);w32(v,0xc,kind);w16(v,0x10,if compressed{1}else{0});w16(v,0x12,0);v[0x14..0x14+n].copy_from_slice(&enc[..n]);}
  if let Some(i)=slot{let l=self.cell_mut(old_list)?;w32(l,i as usize*4,vk_id);}
  else{let new_count=count.checked_add(1).ok_or(Error::InvalidValue)?;let new_list=self.allocate_cell(new_count as usize*4)?;
   let mut old=[0u8;4096];let bytes=count as usize*4;if bytes>old.len(){return Err(Error::InvalidValue)}
   if count>0{let l=self.cell(old_list)?;if l.len()<bytes{return Err(Error::InvalidValue)}old[..bytes].copy_from_slice(&l[..bytes]);}
   {let l=self.cell_mut(new_list)?;l[..bytes].copy_from_slice(&old[..bytes]);w32(l,bytes,vk_id);}
   let k=self.cell_mut(key_id)?;w32(k,0x24,new_count);w32(k,0x28,new_list);
  }Ok(())
 }
 /// Mark the hive dirty before a cell mutation. Persist the redo journal's
 /// PREPARED record first; recovery must run before any REGF parser is opened.
 pub fn mark_dirty(&mut self,sequence:u32)->Result<(),Error>{
  if self.bytes.len()<BASE{return Err(Error::Truncated)}
  w32(self.bytes,4,sequence);w32(self.bytes,8,sequence.wrapping_add(1).max(1));w32(self.bytes,0x1fc,0);Ok(())
 }
 /// Remove a value by replacing the value-list cell without moving any
 /// surviving VK cells. Detached cells are reclaimed by a later compaction pass.
 pub fn delete_value(&mut self,key_id:u32,name:&[u16])->Result<(),Error>{
  let k=self.cell(key_id)?;if k.get(0..2)!=Some(&b"nk"[..])||k.len()<0x4c{return Err(Error::InvalidKey)}
  let count=r32(k,0x24)?;if count==0{return Err(Error::InvalidValue)}let old_list=r32(k,0x28)?;
  let list=self.cell(old_list)?;if list.len()<count as usize*4{return Err(Error::InvalidValue)}
  let mut indexes=[0u32;1024];if count as usize>indexes.len(){return Err(Error::InvalidValue)}
  let mut found=false;let mut kept=0usize;
  for i in 0..count as usize{
   let id=r32(list,i*4)?;let v=self.cell(id)?;if v.get(0..2)!=Some(&b"vk"[..])||v.len()<0x14{return Err(Error::InvalidValue)}
   let n=r16(v,2)? as usize;let flags=r16(v,0x10)?;let mut matches=false;
   if flags&1!=0{matches=n==name.len()&&v[0x14..0x14+n].iter().zip(name).all(|(a,b)|fold(*a as u16)==fold(*b))}
   else if n%2==0&&n==name.len()*2{matches=v[0x14..0x14+n].chunks_exact(2).zip(name).all(|(a,b)|fold(u16::from_le_bytes([a[0],a[1]]))==fold(*b))}
   if matches{found=true}else{indexes[kept]=id;kept+=1}
  }
  if !found{return Err(Error::InvalidValue)}
  if kept==0{let k=self.cell_mut(key_id)?;w32(k,0x24,0);w32(k,0x28,u32::MAX);return Ok(())}
  let new_list=self.allocate_cell(kept*4)?;
  {let l=self.cell_mut(new_list)?;for i in 0..kept{w32(l,i*4,indexes[i]);}}
  let k=self.cell_mut(key_id)?;w32(k,0x24,kept as u32);w32(k,0x28,new_list);Ok(())
 }
 /// Remove an empty subkey from its parent's index. Deleting a non-empty key
 /// is rejected; surviving NK cell indices remain stable.
 pub fn delete_key(&mut self,key_id:u32)->Result<(),Error>{
  let k=self.cell(key_id)?;if k.get(0..2)!=Some(&b"nk"[..])||k.len()<0x4c{return Err(Error::InvalidKey)}
  if r32(k,0x14)?!=0||r32(k,0x24)?!=0{return Err(Error::InvalidKey)}
  let parent=r32(k,0x10)?;if parent==u32::MAX||parent==key_id{return Err(Error::InvalidKey)}
  let p=self.cell(parent)?;let count=r32(p,0x14)?;let old_list=r32(p,0x1c)?;
  if count==0||count as usize>1024{return Err(Error::InvalidKey)}
  let list=self.cell(old_list)?;if list.get(0..2)!=Some(&b"li"[..])||list.len()<2+count as usize*4{return Err(Error::UnsupportedIndex)}
  let mut indexes=[0u32;1024];let mut kept=0usize;let mut found=false;
  for i in 0..count as usize{let id=r32(list,2+i*4)?;if id==key_id{found=true}else{indexes[kept]=id;kept+=1}}
  if !found{return Err(Error::InvalidKey)}
  if kept==0{let p=self.cell_mut(parent)?;w32(p,0x14,0);w32(p,0x1c,u32::MAX);return Ok(())}
  let new_list=self.allocate_cell(2+kept*4)?;
  {let l=self.cell_mut(new_list)?;l[..2].copy_from_slice(b"li");w16(l,2,kept as u16);for i in 0..kept{w32(l,2+i*4,indexes[i]);}}
  let p=self.cell_mut(parent)?;w32(p,0x14,kept as u32);w32(p,0x1c,new_list);Ok(())
 }
 pub fn update_base_block(&mut self,sequence:u32)->Result<(),Error>{
  if self.bytes.len()<BASE{return Err(Error::Truncated)}w32(self.bytes,4,sequence);w32(self.bytes,8,sequence);
  let mut c=0u32;for o in (0..0x1fc).step_by(4){c^=r32(self.bytes,o)?;}if c==0{c=1}else if c==u32::MAX{c=u32::MAX-1}w32(self.bytes,0x1fc,c);Ok(())
 }
}

/// Full before/after image redo journal. Caller must persist PREPARED + before
/// image before changing the hive; persist the hive before COMMITTED; and replay
/// this journal before opening the hive at boot. This is not a parser for Windows HvLE.
pub struct RedoJournal;
impl RedoJournal{
 const H:usize=32;const PREPARED:u32=1;const COMMITTED:u32=2;
 pub fn capacity(hive_len:usize)->Option<usize>{Self::H.checked_add(hive_len.checked_mul(2)?)}
 pub fn begin(hive:&[u8],j:&mut[u8])->Result<u32,Error>{
  let need=Self::capacity(hive.len()).ok_or(Error::Journal)?;if j.len()<need||hive.len()<BASE{return Err(Error::Journal)}
  j[..need].fill(0);j[..4].copy_from_slice(b"CGRJ");w32(j,4,1);w32(j,8,Self::PREPARED);w32(j,12,hive.len() as u32);w32(j,16,checksum(hive));j[Self::H..Self::H+hive.len()].copy_from_slice(hive);Ok(r32(hive,4)?.wrapping_add(1).max(1))
 }
 pub fn commit(hive:&[u8],j:&mut[u8])->Result<(),Error>{
  let n=r32(j,12)? as usize;if j.get(0..4)!=Some(&b"CGRJ"[..])||r32(j,8)?!=Self::PREPARED||hive.len()!=n||j.len()<Self::capacity(n).ok_or(Error::Journal)?{return Err(Error::Journal)}
  let p=Self::H+n;j[p..p+n].copy_from_slice(hive);w32(j,20,checksum(hive));w32(j,8,Self::COMMITTED);Ok(())
 }
 pub fn recover(hive:&mut[u8],j:&[u8])->Result<bool,Error>{
  if j.iter().all(|b|*b==0){return Ok(false)}
  if j.get(0..4)!=Some(&b"CGRJ"[..])||r32(j,4)?!=1{return Err(Error::Journal)}let n=r32(j,12)? as usize;
  if n!=hive.len()||j.len()<Self::capacity(n).ok_or(Error::Journal)?{return Err(Error::Journal)}
  let (off,sum)=match r32(j,8)?{Self::PREPARED=>(Self::H,r32(j,16)?),Self::COMMITTED=>(Self::H+n,r32(j,20)?),_=>return Err(Error::Journal)};
  let image=j.get(off..off+n).ok_or(Error::Truncated)?;if checksum(image)!=sum{return Err(Error::BadChecksum)}hive.copy_from_slice(image);Ok(true)
 }
}

