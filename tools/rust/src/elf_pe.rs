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
 let headers=align((peoff+4+20+optional+40*segs.len()) as u64,FILE_ALIGN)? as usize;
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

pub fn convert_file(input:&Path,output:&Path,image_base:u64,dll:bool)->Result<(),String>{let data=fs::read(input).map_err(|e|e.to_string())?;let pe=convert_elf64_to_pe32plus(&data,image_base,dll)?;fs::write(output,&pe).map_err(|e|e.to_string())?;Ok(())}

#[cfg(test)]
mod tests{
 use super::*;
 #[test] fn rejects_non_elf(){assert!(convert_elf64_to_pe32plus(b"not elf",BASE,false).is_err());}
 #[test] fn rejects_truncated_elf(){let mut b=vec![0;64];b[..4].copy_from_slice(b"\x7fELF");assert!(convert_elf64_to_pe32plus(&b,BASE,false).is_err());}
 #[test] fn converts_minimal_static_elf_to_amd64_pe32plus(){
  let mut b=vec![0u8;0x1004];b[..4].copy_from_slice(b"\\x7fELF");b[4..7].copy_from_slice(&[2,1,1]);
  b[16..18].copy_from_slice(&2u16.to_le_bytes());b[18..20].copy_from_slice(&62u16.to_le_bytes());
  b[24..32].copy_from_slice(&(BASE+0x1000).to_le_bytes());b[32..40].copy_from_slice(&64u64.to_le_bytes());
  b[52..54].copy_from_slice(&64u16.to_le_bytes());b[54..56].copy_from_slice(&56u16.to_le_bytes());b[56..58].copy_from_slice(&1u16.to_le_bytes());
  let p=64usize;b[p..p+4].copy_from_slice(&1u32.to_le_bytes());b[p+4..p+8].copy_from_slice(&5u32.to_le_bytes());
  b[p+8..p+16].copy_from_slice(&0x1000u64.to_le_bytes());b[p+16..p+24].copy_from_slice(&(BASE+0x1000).to_le_bytes());
  b[p+32..p+40].copy_from_slice(&4u64.to_le_bytes());b[p+40..p+48].copy_from_slice(&0x1000u64.to_le_bytes());
  b[0x1000..0x1004].copy_from_slice(&[0x90,0x90,0xeb,0xfe]);
  let pe=convert_elf64_to_pe32plus(&b,BASE,false).unwrap();
  assert_eq!(&pe[..2],b"MZ");assert_eq!(&pe[0x80..0x84],b"PE\\0\\0");
  assert_eq!(u16at(&pe,0x84).unwrap(),0x8664);assert_eq!(u16at(&pe,0x98).unwrap(),0x20b);
 }

}
