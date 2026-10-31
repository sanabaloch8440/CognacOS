use std::{collections::{BTreeMap,BTreeSet},fs,path::PathBuf};
const BASE:[(&str,&str,&str);5]=[
 ("kernel32","dlls/kernel32/exports/windows-11-x64-exports.tsv","dlls/kernel32/exports/kernel32-x64.def"),
 ("kernelbase","dlls/kernelbase/exports/windows-11-x64-exports.tsv","dlls/kernelbase/exports/kernelbase-x64.def"),
 ("shell32","dlls/shell32/exports/windows-11-x64-exports.tsv","dlls/shell32/exports/shell32-x64.def"),
 ("msvcrt","dlls/msvcrt/exports/windows-11-x64-exports.tsv","dlls/msvcrt/exports/msvcrt-x64.def"),
 ("syssetup","dlls/syssetup/exports/windows-11-x64-exports.tsv","dlls/syssetup/exports/syssetup-x64.def")];
#[derive(Clone)]struct Entry{ordinal:u32,name:String,target:String}
fn read(path:&str)->Result<String,String>{fs::read_to_string(path).map_err(|e|format!("{path}: {e}"))}
fn load(path:&str)->Result<Vec<Entry>,String>{
 let mut out=Vec::new();let mut ords=BTreeSet::new();let mut names=BTreeSet::new();
 for (i,line) in read(path)?.lines().enumerate(){if line.is_empty()||line.starts_with('#'){continue}let f:Vec<_>=line.split('\t').collect();if f.len()!=3{return Err(format!("{path}:{} expected ordinal<TAB>name<TAB>forwarder",i+1))}
 let ordinal=f[0].parse::<u32>().map_err(|_|format!("{path}:{} invalid ordinal",i+1))?;
 if !ords.insert(ordinal){return Err(format!("{path}: duplicate ordinal {ordinal}"))}
 if !f[1].is_empty()&&!names.insert(f[1].to_owned()){return Err(format!("{path}: duplicate export name {}",f[1]))}
 out.push(Entry{ordinal,name:f[1].to_owned(),target:f[2].to_owned()});
 }Ok(out)
}
fn expected(dll:&str,records:&[Entry])->String{
 let mut s=format!("LIBRARY {dll}.dll\nEXPORTS\n");
 for e in records {let symbol=if e.name.is_empty(){format!("CognacOrdinalStub_{}",e.ordinal)}else{e.name.clone()};
 if !e.target.is_empty(){s.push_str(&format!("    {symbol}={} @{}{}\n",e.target,e.ordinal,if e.name.is_empty(){" NONAME"}else{""}));}
 else if e.name.is_empty(){s.push_str(&format!("    {symbol} @{} NONAME\n",e.ordinal));}
 else{s.push_str(&format!("    {} @{}\n",e.name,e.ordinal));}
 }s
}
pub fn run()->Result<(),String>{
 let root=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");let mut all=BTreeMap::<String,Vec<Entry>>::new();
 for (dll,tsv,def) in BASE {let records=load(root.join(tsv).to_str().ok_or("invalid path")?)?;
 let actual=read(root.join(def).to_str().ok_or("invalid path")?)?;let want=expected(dll,&records);
 if actual!=want{return Err(format!("{def}: does not exactly match {tsv}"))}
 let fw=records.iter().filter(|x|!x.target.is_empty()).count();let ord=records.iter().filter(|x|x.name.is_empty()).count();
 println!("{dll}: {} entries; {fw} forwarders; {ord} ordinal-only; .def exact",records.len());all.insert(dll.to_owned(),records);}
 let kb_names:BTreeSet<_>=all["kernelbase"].iter().filter(|x|!x.name.is_empty()).map(|x|x.name.as_str()).collect();
 let nt_text=read("ntdll/exports/windows-11-x64-exports.txt")?;
 let nt_names:BTreeSet<_>=nt_text.lines().map(str::trim).filter(|x|!x.is_empty()&&!x.starts_with('#')).collect();
 let mut kb=0;let mut nt=0;let mut missing=Vec::new();
 for e in &all["kernel32"] {let target=e.target.to_ascii_uppercase();if target.starts_with("KERNELBASE."){kb+=1;let n=e.target.split_once('.').unwrap().1;if !kb_names.contains(n){missing.push(format!("{} -> {}",e.name,e.target));}}
 else if target.starts_with("NTDLL."){nt+=1;let n=e.target.split_once('.').unwrap().1;if !nt_names.contains(n){missing.push(format!("{} -> {}",e.name,e.target));}}}
 if !missing.is_empty(){return Err(format!("kernel32 forwarder targets missing: {:?}",&missing[..missing.len().min(20)]))}
 println!("kernel32 forwarders: {kb} to KERNELBASE, {nt} to NTDLL; all targets exist; API-set forwarders retained verbatim");
 println!("All export manifests are local; no network access is used.");Ok(())
}
