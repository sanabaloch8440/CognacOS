use std::fs;
use std::path::Path;

pub const BASE: u64 = 0xffff_ffff_7fe0_0000;
const PAGE: u64 = 0x1000;
const FILE_ALIGN: u64 = 0x200;
const SECT_ALIGN: u64 = 0x1000;
const PT_LOAD: u32 = 1;
const PF_X: u32 = 1;
const PF_W: u32 = 2;

#[derive(Clone)]
struct Segment { vaddr:u64, filesz:u64, memsz:u64, offset:u64, flags:u32 }
fn bad(s:&str)->String{s.to_owned()}
fn slice(b:&[u8],o:u64,n:u64)->Result<&[u8],String>{let a=usize::try_from(o).map_err(|_|bad("offset overflow"))?;let z=usize::try_from(o.checked_add(n).ok_or("range overflow")?).map_err(|_|bad("range overflow"))?;b.get(a..z).ok_or_else(||bad("truncated input"))}
fn u16at(b:&[u8],o:usize)->Result<u16,String>{Ok(u16::from_le_bytes(b.get(o..o+2).ok_or("truncated u16")?.try_into().unwrap()))}
fn u32at(b:&[u8],o:usize)->Result<u32,String>{Ok(u32::from_le_bytes(b.get(o..o+4).ok_or("truncated u32")?.try_into().unwrap()))}
fn u64at(b:&[u8],o:usize)->Result<u64,String>{Ok(u64::from_le_bytes(b.get(o..o+8).ok_or("truncated u64")?.try_into().unwrap()))}
fn put(b:&mut [u8],o:usize,v:&[u8])->Result<(),String>{let e=o.checked_add(v.len()).ok_or("output overflow")?;b.get_mut(o..e).ok_or("output range overflow")?.copy_from_slice(v);Ok(())}
fn p16(b:&mut [u8],o:usize,v:u16)->Result<(),String>{put(b,o,&v.to_le_bytes())}
fn p32(b:&mut [u8],o:usize,v:u32)->Result<(),String>{put(b,o,&v.to_le_bytes())}
fn p64(b:&mut [u8],o:usize,v:u64)->Result<(),String>{put(b,o,&v.to_le_bytes())}
fn align(v:u64,a:u64)->Result<u64,String>{v.checked_add(a-1).map(|x|x&!(a-1)).ok_or_else(||bad("alignment overflow"))}

pub fn convert_elf64_to_pe32plus(blob:&[u8], image_base:u64, dll:bool)->Result<Vec<u8>,String>{
 if blob.len()<64 || &blob[..4]!=b"\x7fELF" {return Err(bad("input is not ELF"));}
 if blob[4..7]!=[2,1,1] || u16at(blob,16)?!=2 || u16at(blob,18)?!=62{return Err(bad("expected little-endian ELF64 ET_EXEC x86-64"));}
 if u16at(blob,52)?!=64 || u16at(blob,54)?!=56{return Err(bad("unsupported ELF header/program-header size"));}
 let entry=u64at(blob,24)?;let phoff=u64at(blob,32)?;let phnum=u16at(blob,56)? as u64;
 if phnum==0||phnum>128||phoff.checked_add(phnum*56).filter(|x|*x<=blob.len() as u64).is_none(){return Err(bad("invalid program-header table"));}
 let mut segs:Vec<Segment>=Vec::new();let mut entry_ok=false;
 for i in 0..phnum{
  let o=usize::try_from(phoff+i*56).map_err(|_|bad("program header overflow"))?;
  let kind=u32at(blob,o)?;let flags=u32at(blob,o+4)?;
  if kind!=PT_LOAD{continue;}
  let off=u64at(blob,o+8)?;let va=u64at(blob,o+16)?;let fs=u64at(blob,o+32)?;let ms=u64at(blob,o+40)?;
  if ms==0||fs>ms||off.checked_add(fs).filter(|x|*x<=blob.len() as u64).is_none(){return Err(bad("invalid PT_LOAD range"));}
  let end=va.checked_add(ms).ok_or("PT_LOAD address overflow")?;
  if va<image_base||end<=va{return Err(bad("PT_LOAD outside fixed high-half image"));}
  let rva=va-image_base;if rva%PAGE!=0{return Err(bad("PT_LOAD RVA must be page aligned"));}
  if fs>0 && off%FILE_ALIGN!=rva%FILE_ALIGN && (va.wrapping_sub(off))%PAGE!=0{return Err(bad("invalid ELF PT_LOAD alignment"));}
  if flags&PF_X!=0&&va<=entry&&entry<end{entry_ok=true;}
  if segs.iter().any(|s|va<s.vaddr+s.memsz&&s.vaddr<end){return Err(bad("overlapping PT_LOAD virtual ranges"));}
  segs.push(Segment{vaddr:va,filesz:fs,memsz:ms,offset:off,flags});
 }
 if segs.is_empty()||!entry_ok{return Err(bad("no loadable executable segment contains entry point"));}
 segs.sort_by_key(|s|s.vaddr);
 if image_base&0xffff!=0||segs[0].vaddr-image_base<PAGE{return Err(bad("image base must be 64 KiB aligned with a nonzero section RVA"));}
 let peoff=0x80usize;let optional=240usize;
 let headers=align((peoff+4+20+optional+40*(segs.len()+2)) as u64,FILE_ALIGN)? as usize;
 let mut raw_cursor=headers as u64;
 let mut raw_sizes=Vec::new();let mut raw_offsets=Vec::new();
 for s in &segs{let size=if s.filesz==0{0}else{align(s.filesz,FILE_ALIGN)?};raw_sizes.push(size);raw_offsets.push(if size==0{0}else{let x=raw_cursor;raw_cursor=raw_cursor.checked_add(size).ok_or("file size overflow")?;x});}
 let size_image=align(segs.iter().map(|s|s.vaddr-image_base+s.memsz).max().unwrap(),SECT_ALIGN)?;
 let total=usize::try_from(raw_cursor).map_err(|_|bad("image too large"))?;
 let mut out=vec![0u8;total];put(&mut out,0,b"MZ")?;p32(&mut out,0x3c,peoff as u32)?;put(&mut out,peoff,b"PE\0\0")?;
 p16(&mut out,peoff+4,0x8664)?;p16(&mut out,peoff+6,segs.len() as u16)?;p16(&mut out,peoff+20,optional as u16)?;p16(&mut out,peoff+22,0x0022|if dll{0x2000}else{0})?;
 let o=peoff+24;p16(&mut out,o,0x20b)?;
 let text_raw:u64=segs.iter().zip(&raw_sizes).filter(|(s,_)|s.flags&PF_X!=0).map(|(_,n)|*n).sum();
 let other_raw:u64=segs.iter().zip(&raw_sizes).filter(|(s,_)|s.flags&PF_X==0).map(|(_,n)|*n).sum();
 let bss:u64=segs.iter().map(|s|s.memsz-s.filesz).sum();
 p32(&mut out,o+4,text_raw as u32)?;p32(&mut out,o+8,other_raw as u32)?;p32(&mut out,o+12,bss as u32)?;
 p32(&mut out,o+16,(entry-image_base) as u32)?;p32(&mut out,o+20,(segs.iter().find(|s|s.flags&PF_X!=0).unwrap().vaddr-image_base) as u32)?;
 p64(&mut out,o+24,image_base)?;p32(&mut out,o+32,SECT_ALIGN as u32)?;p32(&mut out,o+36,FILE_ALIGN as u32)?;
 p16(&mut out,o+40,6)?;p16(&mut out,o+44,6)?;
 p32(&mut out,o+56,size_image as u32)?;p32(&mut out,o+60,headers as u32)?;
 p16(&mut out,o+68,1)?;p16(&mut out,o+70,0x0100)?;
 p64(&mut out,o+72,0x100000)?;p64(&mut out,o+80,0x1000)?;p64(&mut out,o+88,0x100000)?;p64(&mut out,o+96,0x1000)?;
 p32(&mut out,o+108,16)?;
 for (i,s) in segs.iter().enumerate(){
  let sh=o+optional+i*40;let name=match i{0=>b".text\0\0\0".as_slice(),1=>b".rdata\0\0".as_slice(),2=>b".data\0\0\0".as_slice(),3=>b".bss\0\0\0\0".as_slice(),_=>b".extra\0\0".as_slice()};
  put(&mut out,sh,name)?;p32(&mut out,sh+8,s.memsz as u32)?;p32(&mut out,sh+12,(s.vaddr-image_base) as u32)?;p32(&mut out,sh+16,raw_sizes[i] as u32)?;p32(&mut out,sh+20,raw_offsets[i] as u32)?;
  let ch=if s.flags&PF_X!=0{0x60000020}else if s.flags&PF_W!=0{0xc0000040}else{0x40000040};p32(&mut out,sh+36,ch)?;
  if s.filesz>0{let src=slice(blob,s.offset,s.filesz)?;put(&mut out,raw_offsets[i] as usize,src)?;}
 }
 Ok(out)
}



#[derive(Clone)]
struct ImportSlot { dll:String, name:String, rva:u32, section:u16 }

/// Find explicit IAT slots emitted by the driver's x86-64 import thunks.
/// Symbols use __imp_ntoskrnl_<name> or __imp_hal_<name>; slots must be
/// contiguous, writable, eight-byte aligned, and initially zero.
fn collect_import_slots(elf:&[u8],image_base:u64)->Result<Vec<ImportSlot>,String>{
 if elf.len()<64{return Err("truncated ELF import symbol table".into())}
 let shoff=u64at(elf,40)?;let ents=u16at(elf,58)? as u64;let count=u16at(elf,60)? as u64;
 if ents<64||count==0||shoff.checked_add(ents.checked_mul(count).ok_or("section table overflow")?).filter(|x|*x<=elf.len() as u64).is_none(){return Err("invalid ELF section-header table".into())}
 let mut sections=Vec::new();
 for i in 0..count{let o=usize::try_from(shoff+i*ents).map_err(|_|"section offset overflow")?;sections.push((u32at(elf,o)?,u32at(elf,o+4)?,u64at(elf,o+8)?,u64at(elf,o+16)?,u64at(elf,o+24)?,u64at(elf,o+32)?,u32at(elf,o+40)?,u32at(elf,o+44)?,u64at(elf,o+48)?,u64at(elf,o+56)?));}
 let phoff=u64at(elf,32)?;let phents=u16at(elf,54)? as u64;let phnum=u16at(elf,56)? as u64;
 let mut slots:Vec<ImportSlot>=Vec::new();
 for sec in &sections{
  let (_name,kind,_flags,_addr,off,size,link,_info,_align,entsize)=*sec;
  if kind!=2{continue}
  if link as usize>=sections.len()||entsize<24||size%entsize!=0{return Err("invalid ELF import symbol/string table".into())}
  let strings_sec=sections[link as usize];let strings=slice(elf,strings_sec.4,strings_sec.5)?;
  let table=slice(elf,off,size)?;
  for sym in table.chunks_exact(entsize as usize){
   let name_offset=u32::from_le_bytes(sym[0..4].try_into().unwrap()) as usize;
   let info=sym[4];let shndx=u16::from_le_bytes(sym[6..8].try_into().unwrap());
   let value=u64::from_le_bytes(sym[8..16].try_into().unwrap());
   let symbol_size=u64::from_le_bytes(sym[16..24].try_into().unwrap());
   if shndx==0||(info>>4)!=1||(info&15)!=1||name_offset>=strings.len(){continue}
   let end=strings[name_offset..].iter().position(|b|*b==0).map(|n|name_offset+n).ok_or("unterminated ELF import symbol")?;
   let symbol=std::str::from_utf8(&strings[name_offset..end]).map_err(|_|"non-UTF8 ELF import symbol")?;
   let (dll,name)=if let Some(n)=symbol.strip_prefix("__imp_ntoskrnl_"){("ntoskrnl.exe",n)}
    else if let Some(n)=symbol.strip_prefix("__imp_hal_"){("hal.dll",n)}
    else{continue};
   if name.is_empty()||name.len()>240||!name.is_ascii(){return Err(format!("invalid PE import name {name}"))}
   if symbol_size!=8||value&7!=0||value<image_base||value-image_base>u32::MAX as u64{return Err(format!("invalid IAT slot for {name}"))}
   let target=sections.get(shndx as usize).ok_or("IAT symbol has invalid section index")?;
   if target.1!=1||target.2&3!=3||value<target.3||value.checked_add(8).is_none_or(|end|end>target.3+target.5){return Err(format!("IAT slot {name} is not in writable allocated data"))}
   let mut found_file_slot=false;
   for i in 0..phnum{
    let o=usize::try_from(phoff+i*phents).map_err(|_|"program header offset overflow")?;
    if u32at(elf,o)?!=PT_LOAD{continue}
    let flags=u32at(elf,o+4)?;let file_off=u64at(elf,o+8)?;let va=u64at(elf,o+16)?;let file_size=u64at(elf,o+32)?;
    if flags&PF_W!=0&&value>=va&&value.checked_add(8).is_some_and(|e|e<=va.saturating_add(file_size)){
     let slot_off=file_off.checked_add(value-va).ok_or("IAT file offset overflow")?;
     if u64at(elf,usize::try_from(slot_off).map_err(|_|"IAT offset overflow")?)?!=0{return Err(format!("IAT slot {name} is not initially zero"))}
     found_file_slot=true;break;
    }
   }
   if !found_file_slot{return Err(format!("IAT slot {name} is not backed by writable PT_LOAD data"))}
   if slots.iter().any(|slot|slot.dll==dll&&slot.name==name){return Err(format!("duplicate PE import {dll}!{name}"))}
   slots.push(ImportSlot{dll:dll.to_owned(),name:name.to_owned(),rva:(value-image_base) as u32,section:shndx});
  }
 }
 slots.sort_by(|a,b|a.dll.cmp(&b.dll).then(a.rva.cmp(&b.rva)));
 for group in slots.chunk_by(|a,b|a.dll==b.dll){
  if group.windows(2).any(|pair|pair[1].section!=pair[0].section||pair[1].rva!=pair[0].rva+8){
   return Err(format!("{} IAT slots must be contiguous and ordered",group[0].dll));
  }
 }
 Ok(slots)
}

fn append_import_section(pe:&mut Vec<u8>,slots:&[ImportSlot])->Result<(),String>{
 if slots.is_empty(){return Ok(())}
 let mut groups:Vec<(String,Vec<ImportSlot>)>=Vec::new();
 for slot in slots{
  if let Some((_,items))=groups.iter_mut().find(|(dll,_)|*dll==slot.dll){items.push(slot.clone())}
  else{groups.push((slot.dll.clone(),vec![slot.clone()]))}
 }
 groups.sort_by(|a,b|a.0.cmp(&b.0));
 for (dll,items) in &groups{
  if items.windows(2).any(|pair|pair[1].section!=pair[0].section||pair[1].rva!=pair[0].rva+8){
   return Err(format!("{dll} IAT is not contiguous"));
  }
 }
 let descriptor_size=(groups.len()+1).checked_mul(20).ok_or("import descriptor size overflow")?;
 let mut data=vec![0u8;descriptor_size];
 let mut ilts=Vec::new();
 for (_,items) in &groups{
  while data.len()%8!=0{data.push(0)}
  let offset=data.len();
  data.resize(offset+(items.len()+1)*8,0);
  ilts.push(offset);
 }
 let mut dll_offsets=Vec::new();
 for (dll,_) in &groups{
  let offset=data.len();data.extend_from_slice(dll.as_bytes());data.push(0);dll_offsets.push(offset);
 }
 let mut hint_names=Vec::new();
 for (_,items) in &groups{
  let mut offsets=Vec::new();
  for item in items{
   if data.len()%2!=0{data.push(0)}
   let offset=data.len();data.extend_from_slice(&0u16.to_le_bytes());data.extend_from_slice(item.name.as_bytes());data.push(0);offsets.push(offset);
  }
  hint_names.push(offsets);
 }
 let peoff=u32at(pe,0x3c)? as usize;let coff=peoff+4;let nsec=u16at(pe,coff+2)? as usize;let opt=coff+20;let optsize=u16at(pe,coff+16)? as usize;let sh=opt+optsize;
 let headers=u32at(pe,opt+60)? as usize;let new_header=sh.checked_add(nsec.checked_mul(40).ok_or("section table overflow")?).ok_or("section header overflow")?;
 if new_header+40>headers{return Err("PE header has no room for .idata section header".into())}
 let image_base=u64at(pe,opt+24)?;let _=image_base;
 let file_align=u32at(pe,opt+36)? as usize;let sect_align=u32at(pe,opt+32)? as usize;
 if file_align==0||!file_align.is_power_of_two()||sect_align==0||!sect_align.is_power_of_two(){return Err("invalid PE alignment for import section".into())}
 let image_size=u32at(pe,opt+56)? as usize;let rva=(image_size+sect_align-1)&!(sect_align-1);
 for (i,(_,items)) in groups.iter().enumerate(){
  for (j,_) in items.iter().enumerate(){
   let thunk_rva=rva.checked_add(hint_names[i][j]).ok_or("import name RVA overflow")?;
   p64(&mut data,ilts[i]+j*8,thunk_rva as u64)?;
  }
  let desc=i*20;
  p32(&mut data,desc,(rva+ilts[i]) as u32)?;
  p32(&mut data,desc+12,(rva+dll_offsets[i]) as u32)?;
  p32(&mut data,desc+16,items[0].rva)?;
 }
 let raw_off=(pe.len()+file_align-1)&!(file_align-1);let raw_size=(data.len()+file_align-1)&!(file_align-1);
 pe.resize(raw_off+raw_size,0);pe[raw_off..raw_off+data.len()].copy_from_slice(&data);
 put(pe,new_header,b".idata\0\0")?;p32(pe,new_header+8,data.len() as u32)?;p32(pe,new_header+12,rva as u32)?;p32(pe,new_header+16,raw_size as u32)?;p32(pe,new_header+20,raw_off as u32)?;p32(pe,new_header+36,0x40000040)?;
 p16(pe,coff+2,(nsec+1) as u16)?;p32(pe,opt+56,align((rva+data.len()) as u64,sect_align as u64)? as u32)?;
 p32(pe,opt+112+8,rva as u32)?;p32(pe,opt+112+12,descriptor_size as u32)?;
 Ok(())
}

fn append_imports(pe:&mut Vec<u8>,elf:&[u8],image_base:u64)->Result<(),String>{
 let slots=collect_import_slots(elf,image_base)?;
 append_import_section(pe,&slots)
}

fn manifest_exports(elf:&[u8],manifest:&Path,image_base:u64)->Result<Vec<(String,u32,u64)>,String>{
 let json:serde_json::Value=serde_json::from_slice(&fs::read(manifest).map_err(|e|e.to_string())?).map_err(|e|format!("invalid export manifest: {e}"))?;
 let mut allowed=std::collections::BTreeMap::<String,u32>::new();
 for e in json["exports"].as_array().ok_or("manifest exports must be an array")? {
  if let (Some(name),Some(ord))=(e["name"].as_str(),e["ordinal"].as_u64()) {
   if ord==0||ord>65535{return Err(format!("invalid export ordinal {ord}"));}
   if allowed.insert(name.to_owned(),ord as u32).is_some(){return Err(format!("duplicate manifest export {name}"));}
  }
 }
 if elf.len()<64{return Err("truncated ELF symbol table".into())}
 let shoff=u64at(elf,40)?;let ents=u16at(elf,58)? as u64;let count=u16at(elf,60)? as u64;
 if ents<64||count==0||shoff.checked_add(ents.checked_mul(count).ok_or("section table overflow")?).filter(|x|*x<=elf.len() as u64).is_none(){return Err("invalid ELF section-header table".into())}
 let mut sections=Vec::new();
 for i in 0..count {let o=usize::try_from(shoff+i*ents).map_err(|_|"section offset overflow")?;sections.push((u32at(elf,o)?,u32at(elf,o+4)?,u64at(elf,o+8)?,u64at(elf,o+16)?,u64at(elf,o+24)?,u64at(elf,o+32)?,u32at(elf,o+40)?,u32at(elf,o+44)?,u64at(elf,o+48)?,u64at(elf,o+56)?));}
 let mut found=std::collections::BTreeMap::<String,(u32,u64)>::new();
 for sec in &sections {let (_nm,kind,_flags,_addr,off,size,link,_info,_align,entsize)=*sec;if kind!=2{continue}
  if link as usize>=sections.len()||entsize<24||size%entsize!=0{return Err("invalid ELF symbol/string table".into())}
  let strings_sec=sections[link as usize];let strings=slice(elf,strings_sec.4,strings_sec.5)?;
  let table=slice(elf,off,size)?;
  for sym in table.chunks_exact(entsize as usize){let no=u32::from_le_bytes(sym[0..4].try_into().unwrap()) as usize;let info=sym[4];let shndx=u16::from_le_bytes(sym[6..8].try_into().unwrap());let value=u64::from_le_bytes(sym[8..16].try_into().unwrap());
   if shndx==0||(info>>4)!=1&&(info>>4)!=2||(info&15)!=2||no>=strings.len(){continue}
   let end=strings[no..].iter().position(|b|*b==0).map(|n|no+n).ok_or("unterminated ELF symbol name")?;
   let name=std::str::from_utf8(&strings[no..end]).map_err(|_|"non-UTF8 ELF symbol")?;
   if let Some(ord)=allowed.get(name){if let Some((_,old))=found.insert(name.to_owned(),(*ord,value)){if old!=value{return Err(format!("duplicate Windows export symbol {name}"))}}}
  }
 }
 let mut out:Vec<_>=found.into_iter().map(|(n,(o,v))|(n,o,v)).collect();
 out.sort_by(|a,b|a.0.cmp(&b.0));
 for (n,_,v) in &out {if *v<image_base||*v-image_base>u32::MAX as u64{return Err(format!("export {n} outside PE image"))}}
 Ok(out)
}
fn append_exports(pe:&mut Vec<u8>,exports:&[(String,u32,u64)])->Result<(),String>{
 if exports.is_empty(){return Ok(())}
 let peoff=u32at(pe,0x3c)? as usize;let coff=peoff+4;let nsec=u16at(pe,coff+2)? as usize;let opt=coff+20;let optsize=u16at(pe,coff+16)? as usize;let sh=opt+optsize;
 let headers=u32at(pe,opt+60)? as usize;
 let new_header=sh.checked_add(nsec.checked_mul(40).ok_or("section table overflow")?).ok_or("section header overflow")?;
 if new_header+40>headers{return Err("PE header has no room for .edata section header".into())}
 let image_base=u64at(pe,opt+24)?;let max_ord=exports.iter().map(|x|x.1).max().unwrap() as usize;
 let eat_off=40usize;let names_off=eat_off.checked_add(max_ord*4).ok_or("EAT overflow")?;let ords_off=names_off.checked_add(exports.len()*4).ok_or("name table overflow")?;let strings_off=ords_off.checked_add(exports.len()*2).ok_or("ordinal table overflow")?;
 let mut strings=Vec::new();let dll_off=strings_off;strings.extend_from_slice(b"ntoskrnl.exe\0");let mut name_offs=Vec::new();
 for (name,_,_) in exports{name_offs.push(strings_off+strings.len());strings.extend_from_slice(name.as_bytes());strings.push(0);}
 let mut data=vec![0u8;strings_off+strings.len()];
 for (name,ord,value) in exports{let off=eat_off+(*ord as usize-1)*4;data[off..off+4].copy_from_slice(&((*value-image_base) as u32).to_le_bytes());let _=name;}
 for (i,(_,ord,_)) in exports.iter().enumerate(){let off=ords_off+i*2;data[off..off+2].copy_from_slice(&((*ord as u16-1).to_le_bytes()));}
 data[strings_off..].copy_from_slice(&strings);
 let file_align=u32at(pe,opt+36)? as usize;let sect_align=u32at(pe,opt+32)? as usize;
 let raw_off=(pe.len()+file_align-1)&!(file_align-1);let raw_size=(data.len()+file_align-1)&!(file_align-1);
 let image_size=u32at(pe,opt+56)? as usize;let rva=(image_size+sect_align-1)&!(sect_align-1);
 for (i,off) in name_offs.iter().enumerate(){let ptr=names_off+i*4;data[ptr..ptr+4].copy_from_slice(&((rva+off) as u32).to_le_bytes());}
 let mut d=Vec::with_capacity(40);d.extend_from_slice(&[0;12]);d.extend_from_slice(&((rva+dll_off) as u32).to_le_bytes());d.extend_from_slice(&1u32.to_le_bytes());d.extend_from_slice(&(max_ord as u32).to_le_bytes());d.extend_from_slice(&(exports.len() as u32).to_le_bytes());d.extend_from_slice(&((rva+eat_off) as u32).to_le_bytes());d.extend_from_slice(&((rva+names_off) as u32).to_le_bytes());d.extend_from_slice(&((rva+ords_off) as u32).to_le_bytes());data[..40].copy_from_slice(&d);
 pe.resize(raw_off+raw_size,0);pe[raw_off..raw_off+data.len()].copy_from_slice(&data);
 pe[new_header..new_header+8].copy_from_slice(b".edata\0\0");p32(pe,new_header+8,data.len() as u32)?;p32(pe,new_header+12,rva as u32)?;p32(pe,new_header+16,raw_size as u32)?;p32(pe,new_header+20,raw_off as u32)?;p32(pe,new_header+36,0x40000040)?;
 p16(pe,coff+2,(nsec+1) as u16)?;p32(pe,opt+56,align((rva+data.len()) as u64,sect_align as u64)? as u32)?;p32(pe,opt+112,rva as u32)?;p32(pe,opt+116,data.len() as u32)?;
 Ok(())
}

pub fn convert_file(input:&Path,output:&Path,image_base:u64,dll:bool,export_manifest:Option<&Path>)->Result<(),String>{let data=fs::read(input).map_err(|e|e.to_string())?;let mut pe=convert_elf64_to_pe32plus(&data,image_base,dll)?;append_imports(&mut pe,&data,image_base)?;if let Some(path)=export_manifest{let exports=manifest_exports(&data,path,image_base)?;append_exports(&mut pe,&exports)?;}fs::write(output,&pe).map_err(|e|e.to_string())?;Ok(())}

#[cfg(test)]
mod tests{
 use super::*;
 #[test] fn rejects_non_elf(){assert!(convert_elf64_to_pe32plus(b"not elf",BASE,false).is_err());}
 #[test] fn rejects_truncated_elf(){let mut b=vec![0;64];b[..4].copy_from_slice(b"\x7fELF");assert!(convert_elf64_to_pe32plus(&b,BASE,false).is_err());}
 #[test] fn appends_manifest_exports_with_sparse_ordinals(){
  let mut b=vec![0u8;0x1004];b[..4].copy_from_slice(b"\x7fELF");b[4..7].copy_from_slice(&[2,1,1]);
  b[16..18].copy_from_slice(&2u16.to_le_bytes());b[18..20].copy_from_slice(&62u16.to_le_bytes());
  b[24..32].copy_from_slice(&(BASE+0x1000).to_le_bytes());b[32..40].copy_from_slice(&64u64.to_le_bytes());
  b[52..54].copy_from_slice(&64u16.to_le_bytes());b[54..56].copy_from_slice(&56u16.to_le_bytes());b[56..58].copy_from_slice(&1u16.to_le_bytes());
  let p=64usize;b[p..p+4].copy_from_slice(&1u32.to_le_bytes());b[p+4..p+8].copy_from_slice(&5u32.to_le_bytes());
  b[p+8..p+16].copy_from_slice(&0x1000u64.to_le_bytes());b[p+16..p+24].copy_from_slice(&(BASE+0x1000).to_le_bytes());
  b[p+32..p+40].copy_from_slice(&4u64.to_le_bytes());b[p+40..p+48].copy_from_slice(&0x1000u64.to_le_bytes());
  b[0x1000..0x1004].copy_from_slice(&[0x90,0x90,0xeb,0xfe]);
  let mut pe=convert_elf64_to_pe32plus(&b,BASE,false).unwrap();
  append_exports(&mut pe,&[("Alpha".into(),1,BASE+0x1000),("Omega".into(),3,BASE+0x1000)]).unwrap();
  let peoff=u32at(&pe,0x3c).unwrap() as usize;let opt=peoff+24;
  assert_eq!(u16at(&pe,peoff+6).unwrap(),2);
  let er=u32at(&pe,opt+112).unwrap() as usize;
  assert_eq!(u32at(&pe,opt+116).unwrap()>40,true);
  let sh=opt+240+40;assert_eq!(&pe[sh..sh+6],b".edata");
  let raw=u32at(&pe,sh+20).unwrap() as usize;
  assert_eq!(u32at(&pe,raw+20).unwrap(),3);
  assert_eq!(u32at(&pe,raw+24).unwrap(),2);
  let eat=u32at(&pe,raw+28).unwrap() as usize-er;
  assert_eq!(u32at(&pe,raw+eat).unwrap(),0x1000);
  assert_eq!(u32at(&pe,raw+eat+4).unwrap(),0);
  assert_eq!(u32at(&pe,raw+eat+8).unwrap(),0x1000);
 }
 #[test] fn emits_standard_pe_import_directory_for_ntoskrnl_iat(){
  let mut b=vec![0u8;0x1004];b[..4].copy_from_slice(b"\x7fELF");b[4..7].copy_from_slice(&[2,1,1]);
  b[16..18].copy_from_slice(&2u16.to_le_bytes());b[18..20].copy_from_slice(&62u16.to_le_bytes());
  b[24..32].copy_from_slice(&(BASE+0x1000).to_le_bytes());b[32..40].copy_from_slice(&64u64.to_le_bytes());
  b[52..54].copy_from_slice(&64u16.to_le_bytes());b[54..56].copy_from_slice(&56u16.to_le_bytes());b[56..58].copy_from_slice(&1u16.to_le_bytes());
  let p=64usize;b[p..p+4].copy_from_slice(&1u32.to_le_bytes());b[p+4..p+8].copy_from_slice(&5u32.to_le_bytes());
  b[p+8..p+16].copy_from_slice(&0x1000u64.to_le_bytes());b[p+16..p+24].copy_from_slice(&(BASE+0x1000).to_le_bytes());
  b[p+32..p+40].copy_from_slice(&4u64.to_le_bytes());b[p+40..p+48].copy_from_slice(&0x1000u64.to_le_bytes());
  b[0x1000..0x1004].copy_from_slice(&[0x90,0x90,0xeb,0xfe]);
  let mut pe=convert_elf64_to_pe32plus(&b,BASE,false).unwrap();
  let slots=[
   ImportSlot{dll:"ntoskrnl.exe".into(),name:"MmMapIoSpace".into(),rva:0x2000,section:2},
   ImportSlot{dll:"ntoskrnl.exe".into(),name:"MmUnmapIoSpace".into(),rva:0x2008,section:2},
  ];
  append_import_section(&mut pe,&slots).unwrap();
  let peoff=u32at(&pe,0x3c).unwrap() as usize;let opt=peoff+24;
  assert_eq!(u16at(&pe,peoff+6).unwrap(),2);
  let import_rva=u32at(&pe,opt+112+8).unwrap();let import_size=u32at(&pe,opt+112+12).unwrap();
  assert_ne!(import_rva,0);assert_eq!(import_size,40);
  let sh=opt+240+40;assert_eq!(&pe[sh..sh+6],b".idata");
  let raw=u32at(&pe,sh+20).unwrap() as usize;
  let oft=u32at(&pe,raw).unwrap() as usize;
  let dll_rva=u32at(&pe,raw+12).unwrap() as usize;
  assert_eq!(u32at(&pe,raw+16).unwrap(),0x2000);
  let rva_base=u32at(&pe,sh+12).unwrap() as usize;
  let dll_off=raw+(dll_rva-rva_base);
  assert_eq!(&pe[dll_off..dll_off+12],b"ntoskrnl.exe\0");
  let ilt_off=raw+(oft-rva_base);
  let first_name_rva=u64::from_le_bytes(pe[ilt_off..ilt_off+8].try_into().unwrap()) as usize;
  let first_name_off=raw+(first_name_rva-rva_base);
  assert_eq!(u16at(&pe,first_name_off).unwrap(),0);
  assert_eq!(&pe[first_name_off+2..first_name_off+14],b"MmMapIoSpace\0");
 }
 #[test] fn converts_minimal_static_elf_to_amd64_pe32plus(){
  let mut b=vec![0u8;0x1004];b[..4].copy_from_slice(b"\x7fELF");b[4..7].copy_from_slice(&[2,1,1]);
  b[16..18].copy_from_slice(&2u16.to_le_bytes());b[18..20].copy_from_slice(&62u16.to_le_bytes());
  b[24..32].copy_from_slice(&(BASE+0x1000).to_le_bytes());b[32..40].copy_from_slice(&64u64.to_le_bytes());
  b[52..54].copy_from_slice(&64u16.to_le_bytes());b[54..56].copy_from_slice(&56u16.to_le_bytes());b[56..58].copy_from_slice(&1u16.to_le_bytes());
  let p=64usize;b[p..p+4].copy_from_slice(&1u32.to_le_bytes());b[p+4..p+8].copy_from_slice(&5u32.to_le_bytes());
  b[p+8..p+16].copy_from_slice(&0x1000u64.to_le_bytes());b[p+16..p+24].copy_from_slice(&(BASE+0x1000).to_le_bytes());
  b[p+32..p+40].copy_from_slice(&4u64.to_le_bytes());b[p+40..p+48].copy_from_slice(&0x1000u64.to_le_bytes());
  b[0x1000..0x1004].copy_from_slice(&[0x90,0x90,0xeb,0xfe]);
  let pe=convert_elf64_to_pe32plus(&b,BASE,false).unwrap();
  assert_eq!(&pe[..2],b"MZ");assert_eq!(&pe[0x80..0x84],b"PE\0\0");
  assert_eq!(u16at(&pe,0x84).unwrap(),0x8664);assert_eq!(u16at(&pe,0x98).unwrap(),0x20b);
 }

}
