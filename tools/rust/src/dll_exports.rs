use std::{collections::BTreeMap, fs, path::Path};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Export { name: Option<String>, forwarder: Option<String> }

fn u16r(b:&[u8],o:usize)->Result<u16,String>{Ok(u16::from_le_bytes(b.get(o..o.checked_add(2).ok_or("offset overflow")?).ok_or("truncated u16")?.try_into().unwrap()))}
fn u32r(b:&[u8],o:usize)->Result<u32,String>{Ok(u32::from_le_bytes(b.get(o..o.checked_add(4).ok_or("offset overflow")?).ok_or("truncated u32")?.try_into().unwrap()))}
fn range(b:&[u8],o:usize,n:usize)->Result<&[u8],String>{b.get(o..o.checked_add(n).ok_or("range overflow")?).ok_or("truncated PE data".into())}
fn rva(b:&[u8],v:u32,ss:&[(u32,u32,u32,u32)],hs:u32)->Result<usize,String>{
 if v<hs {let o=v as usize;if o<b.len(){return Ok(o)}return Err("header RVA outside file".into())}
 for &(va,vs,rs,ro) in ss {let span=vs.max(rs);if v>=va&&v<va.checked_add(span).ok_or("section RVA overflow")? {let d=v-va;if d>=rs{return Err(format!("RVA 0x{v:x} points to unbacked section bytes"))}let o=ro.checked_add(d).ok_or("raw offset overflow")? as usize;if o>=b.len(){return Err("RVA maps beyond file".into())}return Ok(o)}}
 Err(format!("RVA 0x{v:x} is outside mapped sections"))
}
fn cstr(b:&[u8],o:usize)->Result<String,String>{let tail=b.get(o..).ok_or("string offset outside file")?;let n=tail.iter().position(|x|*x==0).ok_or("unterminated export string")?;let s=&tail[..n];if !s.is_ascii(){return Err("non-ASCII export string".into())}Ok(String::from_utf8(s.to_vec()).unwrap())}

fn parse_pe(b:&[u8])->Result<BTreeMap<u32,Export>,String>{
 if range(b,0,2)?!=b"MZ"{return Err("missing DOS signature".into())}
 let pe=u32r(b,0x3c)? as usize;if range(b,pe,4)?!=b"PE\0\0"{return Err("missing PE signature".into())}
 if u16r(b,pe+4)?!=0x8664{return Err("expected AMD64 image".into())}
 let sections=u16r(b,pe+6)? as usize;let opt=pe.checked_add(24).ok_or("optional header overflow")?;
 let opt_size=u16r(b,pe+20)? as usize;if u16r(b,opt)?!=0x20b{return Err("expected PE32+ optional header".into())}
 if opt_size<120{return Err("PE32+ optional header too small for data directories".into())}
 let headers=u32r(b,opt+60)?;let export_rva=u32r(b,opt+112)?;let export_size=u32r(b,opt+116)?;
 if export_rva==0||export_size==0{return Err("no export directory".into())}
 let table=opt.checked_add(opt_size).ok_or("section table overflow")?;let mut ss=Vec::new();
 for i in 0..sections {let o=table.checked_add(i.checked_mul(40).ok_or("section index overflow")?).ok_or("section offset overflow")?;range(b,o,40)?;ss.push((u32r(b,o+12)?,u32r(b,o+8)?,u32r(b,o+16)?,u32r(b,o+20)?));}
 let eo=rva(b,export_rva,&ss,headers)?;range(b,eo,40)?;
 let base=u32r(b,eo+16)?;let funcs=u32r(b,eo+20)? as usize;let names=u32r(b,eo+24)? as usize;
 if funcs>1_000_000||names>funcs{return Err("unreasonable export table counts".into())}
 let eat=rva(b,u32r(b,eo+28)?,&ss,headers)?;let npt=if names>0{rva(b,u32r(b,eo+32)?,&ss,headers)?}else{0};let ords=if names>0{rva(b,u32r(b,eo+36)?,&ss,headers)?}else{0};
 let mut names_by_index:BTreeMap<usize,Vec<String>>=BTreeMap::new();
 for i in 0..names {let nr=u32r(b,npt.checked_add(i*4).ok_or("name pointer overflow")?)?;let ix=u16r(b,ords.checked_add(i*2).ok_or("ordinal table overflow")?)? as usize;if ix>=funcs{return Err(format!("export name ordinal index {ix} out of range"))}let name=cstr(b,rva(b,nr,&ss,headers)?)?;names_by_index.entry(ix).or_default().push(name);}
 let export_end=export_rva.checked_add(export_size).ok_or("export directory range overflow")?;
 let mut out=BTreeMap::new();
 for i in 0..funcs {let fr=u32r(b,eat.checked_add(i.checked_mul(4).ok_or("EAT index overflow")?).ok_or("EAT offset overflow")?)?;if fr==0{continue}
 let ordinal=base.checked_add(i as u32).ok_or("export ordinal overflow")?;
 let ns=names_by_index.remove(&i).unwrap_or_default();if ns.len()>1{return Err(format!("ordinal {ordinal} has multiple names; alias exports are unsupported by TSV schema"))}
 let forwarder=if fr>=export_rva&&fr<export_end{Some(cstr(b,rva(b,fr,&ss,headers)?)?)}else{None};
 out.insert(ordinal,Export{name:ns.into_iter().next(),forwarder});
 }
 if !names_by_index.is_empty(){return Err("unconsumed named export indices".into())}
 Ok(out)
}
fn manifest(path:&Path)->Result<BTreeMap<u32,Export>,String>{
 let text=fs::read_to_string(path).map_err(|e|format!("{}: {e}",path.display()))?;let mut out=BTreeMap::new();
 for (i,line) in text.lines().enumerate(){if line.is_empty()||line.starts_with('#'){continue}let f:Vec<_>=line.split('\t').collect();if f.len()!=3{return Err(format!("{}:{} expected ordinal<TAB>name<TAB>forwarder",path.display(),i+1))}
 let ordinal=f[0].parse::<u32>().map_err(|_|format!("{}:{} invalid ordinal",path.display(),i+1))?;
 let entry=Export{name:if f[1].is_empty(){None}else{Some(f[1].to_owned())},forwarder:if f[2].is_empty(){None}else{Some(f[2].to_owned())}};
 if out.insert(ordinal,entry).is_some(){return Err(format!("{}:{} duplicate ordinal {ordinal}",path.display(),i+1))}
 }Ok(out)
}
pub fn named_exports(path:&Path)->Result<BTreeMap<String,u32>,String>{
 let bytes=fs::read(path).map_err(|e|format!("{}: {e}",path.display()))?;
 let parsed=parse_pe(&bytes)?;
 let mut out=BTreeMap::new();
 for (ordinal,entry) in parsed {if let Some(name)=entry.name {if out.insert(name.clone(),ordinal).is_some(){return Err(format!("duplicate named export {name}"));}}}
 Ok(out)
}
pub fn verify(name:&str,pe:&Path,tsv:&Path)->Result<(),String>{
 let bytes=fs::read(pe).map_err(|e|format!("{}: {e}",pe.display()))?;let expected=manifest(tsv)?;let actual=parse_pe(&bytes)?;
 let missing:Vec<_>=expected.keys().filter(|k|!actual.contains_key(k)).copied().collect();
 let extra:Vec<_>=actual.keys().filter(|k|!expected.contains_key(k)).copied().collect();
 let mismatch:Vec<_>=expected.iter().filter_map(|(k,e)|actual.get(k).filter(|a|*a!=e).map(|a|format!("ordinal {k}: expected {e:?}, actual {a:?}"))).collect();
 if !missing.is_empty()||!extra.is_empty()||!mismatch.is_empty(){return Err(format!("FAIL {name}: reference={} PE={} missing={missing:?} extra={extra:?} mismatched={mismatch:#?}",expected.len(),actual.len()))}
 let forwarders=actual.values().filter(|x|x.forwarder.is_some()).count();let ordinal_only=actual.values().filter(|x|x.name.is_none()).count();
 println!("PASS {name}: {} exports, {forwarders} forwarders, {ordinal_only} ordinal-only exports match names/ordinals/forwarders exactly",actual.len());Ok(())
}
pub fn run(args:&[String])->Result<(),String>{
 let mut specs=Vec::new();let mut i=0;
 while i<args.len(){
  if args[i]!="--dll"{return Err(format!("unknown argument {}",args[i]))}
  i+=1;let spec=args.get(i).ok_or("--dll requires NAME,PE,TSV")?;
  let (name,rest)=spec.split_once(',').ok_or("invalid --dll specification")?;
  let (pe,tsv)=rest.split_once(',').ok_or("invalid --dll specification; expected NAME,PE,TSV")?;
  specs.push((name.to_owned(),pe.to_owned(),tsv.to_owned()));i+=1;
 }
 if specs.is_empty(){return Err("at least one --dll NAME,PE,TSV is required".into())}
 for (name,pe,tsv) in specs{verify(&name,Path::new(&pe),Path::new(&tsv))?;}
 Ok(())
}

#[cfg(test)]
mod tests {
 use super::*;
 #[test] fn rejects_bad_dos_signature(){assert!(parse_pe(b"bad").is_err());}
 #[test] fn rejects_truncated_dos_header(){assert!(parse_pe(b"MZ").is_err());}
 #[test] fn parses_named_and_ordinal_only_forwarder_exports(){
  let mut b=vec![0u8;0x400];b[..2].copy_from_slice(b"MZ");b[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
  b[0x80..0x84].copy_from_slice(b"PE\0\0");b[0x84..0x86].copy_from_slice(&0x8664u16.to_le_bytes());
  b[0x86..0x88].copy_from_slice(&1u16.to_le_bytes());b[0x94..0x96].copy_from_slice(&240u16.to_le_bytes());
  let o=0x98;b[o..o+2].copy_from_slice(&0x20bu16.to_le_bytes());b[o+60..o+64].copy_from_slice(&0x200u32.to_le_bytes());
  b[o+112..o+116].copy_from_slice(&0x1000u32.to_le_bytes());b[o+116..o+120].copy_from_slice(&0x100u32.to_le_bytes());
  let sh=o+240;b[sh..sh+8].copy_from_slice(b".edata\0\0");b[sh+8..sh+12].copy_from_slice(&0x100u32.to_le_bytes());b[sh+12..sh+16].copy_from_slice(&0x1000u32.to_le_bytes());b[sh+16..sh+20].copy_from_slice(&0x200u32.to_le_bytes());b[sh+20..sh+24].copy_from_slice(&0x200u32.to_le_bytes());
  let e=0x200;b[e+16..e+20].copy_from_slice(&1u32.to_le_bytes());b[e+20..e+24].copy_from_slice(&2u32.to_le_bytes());b[e+24..e+28].copy_from_slice(&1u32.to_le_bytes());b[e+28..e+32].copy_from_slice(&0x1040u32.to_le_bytes());b[e+32..e+36].copy_from_slice(&0x1048u32.to_le_bytes());b[e+36..e+40].copy_from_slice(&0x104cu32.to_le_bytes());
  b[0x240..0x244].copy_from_slice(&0x2000u32.to_le_bytes());b[0x244..0x248].copy_from_slice(&0x1060u32.to_le_bytes());
  b[0x248..0x24c].copy_from_slice(&0x1050u32.to_le_bytes());b[0x24c..0x24e].copy_from_slice(&0u16.to_le_bytes());
  b[0x250..0x25c].copy_from_slice(b"NamedExport\0");b[0x260..0x26f].copy_from_slice(b"KERNEL32.Sleep\0");
  let got=parse_pe(&b).unwrap();assert_eq!(got.get(&1).unwrap().name.as_deref(),Some("NamedExport"));
  assert_eq!(got.get(&2).unwrap().name,None);assert_eq!(got.get(&2).unwrap().forwarder.as_deref(),Some("KERNEL32.Sleep"));
 }
 #[test] fn parses_manifest_ordinal_only_and_forwarder_records(){
  let p=std::env::temp_dir().join(format!("cognacos-export-{}.tsv",std::process::id()));
  fs::write(&p,"# test\n1\tNamed\t\n2\t\tKERNEL32.Sleep\n").unwrap();
  let m=manifest(&p).unwrap();let _=fs::remove_file(&p);
  assert_eq!(m.get(&1).unwrap().name.as_deref(),Some("Named"));
  assert_eq!(m.get(&2).unwrap().name,None);
  assert_eq!(m.get(&2).unwrap().forwarder.as_deref(),Some("KERNEL32.Sleep"));
 }
}
