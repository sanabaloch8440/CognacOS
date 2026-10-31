use serde_json::Value;
use std::{collections::BTreeMap,fs,path::Path};
fn u16r(b:&[u8],o:usize)->Result<u16,String>{Ok(u16::from_le_bytes(b.get(o..o+2).ok_or("truncated u16")?.try_into().unwrap()))}
fn u32r(b:&[u8],o:usize)->Result<u32,String>{Ok(u32::from_le_bytes(b.get(o..o+4).ok_or("truncated u32")?.try_into().unwrap()))}
fn rva(b:&[u8],v:u32,ss:&[(u32,u32,u32,u32)],hs:u32)->Result<usize,String>{if v<hs{if (v as usize)<b.len(){return Ok(v as usize)}return Err("header RVA outside file".into())}for &(va,vs,rs,ro) in ss{let span=vs.max(rs);if v>=va&&v<va.saturating_add(span){let d=v-va;if d>=rs{return Err(format!("RVA 0x{v:x} has no file backing"))}let p=ro.checked_add(d).ok_or("RVA overflow")? as usize;if p>=b.len(){return Err("RVA outside file".into())}return Ok(p)}}Err(format!("RVA 0x{v:x} does not map to a section"))}
fn cstr(b:&[u8],o:usize)->Result<String,String>{let tail=b.get(o..).ok_or("string offset outside file")?;let n=tail.iter().position(|x|*x==0).ok_or("unterminated export string")?;String::from_utf8(tail[..n].to_vec()).map_err(|_|"non-ASCII/UTF-8 export name".into())}
pub fn parse_exports(b:&[u8])->Result<BTreeMap<String,u32>,String>{
 if b.len()<2 || &b[..2]!=b"MZ"{return Err("missing MZ signature".into())}let pe=u32r(b,0x3c)? as usize;if b.get(pe..pe+4)!=Some(b"PE\0\0"){return Err("missing PE signature".into())}
 if u16r(b,pe+4)?!=0x8664{return Err("not AMD64".into())}if u16r(b,pe+24)?!=0x20b{return Err("not PE32+".into())}
 let n=u16r(b,pe+6)? as usize;let opt=pe+24;let os=u16r(b,pe+20)? as usize;let hs=u32r(b,opt+60)?;
 let sh=opt.checked_add(os).ok_or("section table overflow")?;let mut ss=Vec::new();
 for i in 0..n{let o=sh.checked_add(i*40).ok_or("section table overflow")?;ss.push((u32r(b,o+12)?,u32r(b,o+8)?,u32r(b,o+16)?,u32r(b,o+20)?));}
 let er=u32r(b,opt+112)?;let es=u32r(b,opt+116)?;if er==0||es<40{return Ok(BTreeMap::new())}
 let eo=rva(b,er,&ss,hs)?;let base=u32r(b,eo+16)?;let nf=u32r(b,eo+20)?;let nn=u32r(b,eo+24)?;
 let eat=rva(b,u32r(b,eo+28)?,&ss,hs)?;let names=rva(b,u32r(b,eo+32)?,&ss,hs)?;let ords=rva(b,u32r(b,eo+36)?,&ss,hs)?;
 let mut out=BTreeMap::new();for i in 0..nn{let nr=u32r(b,names+i as usize*4)?;let name=cstr(b,rva(b,nr,&ss,hs)?)?;let ix=u16r(b,ords+i as usize*2)? as u32;if ix>=nf{return Err(format!("bad ordinal index for {name}"))}let ord=base.checked_add(ix).ok_or("ordinal overflow")?;if u32r(b,eat+ix as usize*4)?==0{return Err(format!("{name} points to empty EAT slot"))}if out.insert(name.clone(),ord).is_some(){return Err(format!("duplicate export name: {name}"))}}
 Ok(out)
}
pub fn validate(pe_path:&Path,manifest_path:&Path,required:&[String])->Result<usize,String>{
 let b=fs::read(pe_path).map_err(|e|e.to_string())?;let m:Value=serde_json::from_slice(&fs::read(manifest_path).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
 let mut expected=BTreeMap::new();for x in m["exports"].as_array().ok_or("manifest exports missing")?{if let Some(n)=x["name"].as_str(){let ord=x["ordinal"].as_u64().ok_or("manifest ordinal missing")? as u32;expected.insert(n.to_owned(),ord);}}
 let actual=parse_exports(&b)?;let extras:Vec<_>=actual.keys().filter(|n|!expected.contains_key(*n)).cloned().collect();let mismatches:Vec<_>=actual.iter().filter(|(n,o)|expected.get(*n)!=Some(*o)).map(|(n,o)|format!("{n}={o} expected {:?}",expected.get(n))).collect();let missing:Vec<_>=required.iter().filter(|n|!actual.contains_key(*n)).cloned().collect();
 if !extras.is_empty()||!mismatches.is_empty()||!missing.is_empty(){return Err(format!("extra exports: {extras:?}; ordinal mismatches: {mismatches:?}; required missing: {missing:?}"))}
 let version=m["source"]["version"].as_str().unwrap_or("unknown");println!("Validated {} PE exports against {version}",actual.len());Ok(actual.len())
}
#[cfg(test)]mod tests{use super::*;#[test]fn rejects_bad_signature(){assert!(parse_exports(b"bad").is_err());}}
