use std::{collections::BTreeSet, env, fs, path::PathBuf};
struct Export { ordinal: u32, name: String, forwarder: String }
fn read_exports(path: &str) -> Vec<Export> {
 println!("cargo:rerun-if-changed={path}");
 let raw=fs::read_to_string(path).unwrap_or_else(|e|panic!("cannot read {path}: {e}"));
 let mut out=Vec::new(); let mut ordinals=BTreeSet::new(); let mut names=BTreeSet::new();
 for (line_no,line) in raw.lines().enumerate() {
  if line.is_empty()||line.starts_with('#'){continue}
  let f:Vec<_>=line.split('\t').collect();
  assert_eq!(f.len(),3,"{path}:{} expected ordinal<TAB>name<TAB>forwarder",line_no+1);
  let ordinal:u32=f[0].parse().expect("invalid ordinal");
  assert!(ordinals.insert(ordinal),"duplicate ordinal {ordinal}");
  let name=f[1].to_owned(); if !name.is_empty(){assert!(names.insert(name.clone()),"duplicate export {name}");}
  out.push(Export{ordinal,name,forwarder:f[2].to_owned()});
 }
 assert!(!out.is_empty(),"empty export inventory"); out
}
fn expected_def(exports:&[Export])->String {
 let mut out=String::from("LIBRARY syssetup.dll\nEXPORTS\n");
 for e in exports {
  if e.forwarder.is_empty(){out.push_str(&format!("    {} @{}\n",e.name,e.ordinal));}
  else{out.push_str(&format!("    {}={} @{}\n",e.name,e.forwarder,e.ordinal));}
 }
 out
}
fn main(){
 println!("cargo:rerun-if-changed=src/lib.rs"); println!("cargo:rerun-if-changed=exports/syssetup-x64.def");
 let exports=read_exports("exports/windows-11-x64-exports.tsv");
 let def=fs::read_to_string("exports/syssetup-x64.def").expect("missing syssetup .def");
 assert_eq!(def,expected_def(&exports),"syssetup .def differs from pinned manifest");
 let mut generated=String::from("// Generated from the local Windows x64 export inventory.\n");
 for e in &exports { if e.forwarder.is_empty(){generated.push_str(&format!("#[export_name = {:?}]\npub extern \"system\" fn cognac_syssetup_export_{}() -> usize {{ 0 }}\n",e.name,e.ordinal));}}
 let out=PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
 fs::write(out.join("syssetup_export_stubs.rs"),generated).expect("write export stubs");
 println!("cargo:warning=syssetup: {} export slots; one SetupAPI forwarder; remaining bodies are placeholders",exports.len());
}
