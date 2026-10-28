use std::{collections::{BTreeMap,BTreeSet},env,fs,path::PathBuf};
#[derive(Debug)] struct Export { ordinal:u32, name:String, forwarder:String }
fn read_exports(path:&str)->Vec<Export>{
 println!("cargo:rerun-if-changed={path}");
 let raw=fs::read_to_string(path).unwrap_or_else(|e|panic!("cannot read {path}: {e}"));
 let mut out=Vec::new(); let mut ords=BTreeSet::new(); let mut names=BTreeSet::new();
 for (i,line) in raw.lines().enumerate(){let line=line.trim_end();if line.is_empty()||line.starts_with('#'){continue}
  let f:Vec<&str>=line.split('\t').collect();assert_eq!(f.len(),3,"{path}:{} expected ordinal<TAB>name<TAB>forwarder",i+1);
  let ordinal:u32=f[0].parse().unwrap_or_else(|_|panic!("{path}:{} bad ordinal",i+1));assert!(ords.insert(ordinal),"{path}: duplicate ordinal {ordinal}");
  let name=f[1].to_owned();let forwarder=f[2].to_owned();
  if !name.is_empty(){assert!(names.insert(name.clone()),"{path}: duplicate export name {name}");}
  out.push(Export{ordinal,name,forwarder});
 }
 assert!(!out.is_empty(),"{path}: no exports");out
}
fn expected_def(dll:&str, exports:&[Export])->String{
 let mut s=format!("LIBRARY {dll}.dll\nEXPORTS\n");
 for e in exports {let entry=if e.name.is_empty(){format!("CognacOrdinalStub_{}",e.ordinal)}else{e.name.clone()};
  if !e.forwarder.is_empty(){s.push_str(&format!("    {entry}={} @{}{}\n",e.forwarder,e.ordinal,if e.name.is_empty(){" NONAME"}else{""}));}
  else if e.name.is_empty(){s.push_str(&format!("    {entry} @{} NONAME\n",e.ordinal));}
  else{s.push_str(&format!("    {} @{}\n",e.name,e.ordinal));}
 }
 s
}
fn main(){
 println!("cargo:rerun-if-changed=src/lib.rs");println!("cargo:rerun-if-changed=exports/kernel32-x64.def");
 let path="exports/windows-11-x64-exports.tsv";let exports=read_exports(path);
 let def_path="exports/kernel32-x64.def";let actual=fs::read_to_string(def_path).expect("missing .def");let expected=expected_def("kernel32",&exports);
 assert_eq!(actual,expected,"kernel32 .def diverges from the pinned Windows export manifest");
 let mut out=String::from("// Generated from the pinned Windows 11 x64 export manifest.\n");
 let manual:[&str;4]=["GetLastError","SetLastError","GetCurrentProcess","GetCurrentThread"];
 for e in &exports {
  if !e.forwarder.is_empty() || manual.contains(&e.name.as_str()){continue}
  if e.name.is_empty(){out.push_str(&format!("#[no_mangle]\npub extern \"system\" fn CognacOrdinalStub_{}() -> ULONG_PTR {{ 0 }}\n",e.ordinal));}
  else {out.push_str(&format!("#[export_name = {:?}]\npub extern \"system\" fn cognac_export_{}() -> ULONG_PTR {{ 0 }}\n",e.name,e.ordinal));}
 }
 let p=PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));fs::write(p.join("kernel32_export_stubs.rs"),out).expect("write generated exports");
 let fw=exports.iter().filter(|e|!e.forwarder.is_empty()).count();let unnamed=exports.iter().filter(|e|e.name.is_empty()).count();
 println!("cargo:warning=kernel32: {} exact reference exports, {fw} forwarders, {unnamed} ordinal-only entries; bodies remain placeholders",exports.len());
}
