use std::{collections::BTreeMap,collections::BTreeSet,env,fs,path::PathBuf};
fn inventory(path:&str)->BTreeSet<String>{println!("cargo:rerun-if-changed={path}");fs::read_to_string(path).expect("read inventory").lines().map(str::trim).filter(|s|!s.is_empty()&&!s.starts_with('#')).map(|s|{assert!(s.chars().all(|c|c.is_ascii_alphanumeric()||c=='_'),"invalid export {s}");s.to_owned()}).collect()}
fn main(){
 println!("cargo:rerun-if-changed=src/lib.rs"); println!("cargo:rerun-if-changed=exports/kernelbase-x64.def");
 let names=inventory("exports/windows-11-x64-exports.txt");
 let nt=inventory("../../ntdll/exports/windows-11-x64-exports.txt");
 let def=fs::read_to_string("exports/kernelbase-x64.def").expect("missing .def");
 let mut declared=BTreeSet::new(); let mut forwarders=BTreeMap::new();
 for raw in def.lines(){let line=raw.trim();if line.is_empty()||line.starts_with(';')||line=="EXPORTS"||line.starts_with("LIBRARY "){continue}
  let token=line.split_whitespace().next().unwrap(); let (name,target)=token.split_once('=').map_or((token,None),|(n,t)|(n,Some(t)));
  assert!(names.contains(name), "kernelbase .def export missing from inventory: {name}");
  if let Some(target)=target {let (module,symbol)=target.split_once('.').expect("forwarder must be DLL.symbol");assert_eq!(module,"NTDLL","unsupported forwarder module");assert!(nt.contains(symbol),"forwarder target absent from NTDLL inventory: {target}");forwarders.insert(name.to_owned(),target.to_owned());}
  declared.insert(name.to_owned());
 }
 assert_eq!(names,declared,"kernelbase .def and inventory mismatch");
 let manual=["GetLastError","SetLastError","GetCurrentProcess","GetCurrentThread"];
 let mut out=String::from("// Generated placeholder exports; forwarders are emitted by the PE linker.\n");let mut count=0;
 for name in &names {if manual.contains(&name.as_str())||forwarders.contains_key(name){continue}out.push_str(&format!("#[no_mangle]\npub extern \"system\" fn {name}() -> ULONG_PTR {{ 0 }}\n"));count+=1;}
 let p=PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));fs::write(p.join("kernelbase_export_stubs.rs"),out).expect("write exports");
 println!("cargo:warning=generated {count} kernelbase placeholder functions and {} PE forwarders; inventory is not exhaustive",forwarders.len());
}
