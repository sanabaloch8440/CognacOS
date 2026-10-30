//! Native x64 rundll32.exe command-line DLL entry-point host.
use std::ffi::{c_char, c_void, OsStr};
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::{env, process};

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn LoadLibraryW(name: *const u16) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
    fn FreeLibrary(module: *mut c_void) -> i32;
    fn GetLastError() -> u32;
}

#[derive(Debug, PartialEq, Eq)]
struct Invocation { library: String, entry: String, arguments: Vec<String> }

fn parse_invocation(args: &[String]) -> Result<Invocation, &'static str> {
    if args.len() < 2 { return Err("usage: rundll32.exe <dll>,<entry> [arguments]"); }
    let (library, entry, tail) = match args[1].rsplit_once(',') {
        Some((dll, ep)) if !dll.is_empty() && !ep.is_empty() => (dll, ep, 2),
        Some(_) => return Err("DLL path and entry point must both be non-empty"),
        None if args.len() >= 3 => (args[1].as_str(), args[2].as_str(), 3),
        None => return Err("missing DLL entry point; use <dll>,<entry>"),
    };
    Ok(Invocation { library: library.into(), entry: entry.into(), arguments: args.iter().skip(tail).cloned().collect() })
}

#[cfg(windows)]
fn wide(value: &OsStr) -> Vec<u16> { value.encode_wide().chain(std::iter::once(0)).collect() }

#[cfg(windows)]
fn run(inv: Invocation) -> Result<(), String> {
    let dll_path=wide(Path::new(&inv.library).as_os_str());
    let module=unsafe{LoadLibraryW(dll_path.as_ptr())};
    if module.is_null(){return Err(format!("LoadLibraryW failed (Win32 error {})",unsafe{GetLastError()}));}
    let address=if let Some(raw)=inv.entry.strip_prefix('#') {
        let ordinal:u16=raw.parse().map_err(|_|"invalid ordinal entry point".to_string())?;
        unsafe{GetProcAddress(module,ordinal as usize as *const c_char)}
    } else {
        let mut name=inv.entry.as_bytes().to_vec();
        if name.contains(&0){unsafe{FreeLibrary(module);}return Err("entry point contains NUL".into());}
        name.push(0);
        unsafe{GetProcAddress(module,name.as_ptr().cast())}
    };
    if address.is_null(){let e=unsafe{GetLastError()};unsafe{FreeLibrary(module);}return Err(format!("GetProcAddress failed (Win32 error {e})"));}
    let mut tail:Vec<u16>=inv.arguments.join(" ").encode_utf16().collect();tail.push(0);
    // AMD64 uses the unified Windows x64 calling convention for CALLBACK.
    type RundllEntry=unsafe extern "system" fn(*mut c_void,*mut c_void,*const u16,i32);
    let entry:RundllEntry=unsafe{std::mem::transmute(address)};
    unsafe{entry(std::ptr::null_mut(),module,tail.as_ptr(),1);FreeLibrary(module);}
    Ok(())
}

#[cfg(not(windows))]
fn main(){eprintln!("rundll32 is a Windows x64 program; build for x86_64-pc-windows-msvc.");process::exit(2);}

#[cfg(windows)]
fn main(){
    let args:Vec<String>=env::args().map(|a|a.to_string_lossy().into_owned()).collect();
    let inv=match parse_invocation(&args){Ok(v)=>v,Err(e)=>{eprintln!("{e}");process::exit(2);}};
    if let Err(e)=run(inv){eprintln!("rundll32: {e}");process::exit(1);}
}

#[cfg(test)]
mod tests{
 use super::*;
 #[test] fn parses_comma_form(){let a=vec!["rundll32.exe".into(),"shell32.dll,Control_RunDLL".into(),"app.cpl".into()];let p=parse_invocation(&a).unwrap();assert_eq!(p.library,"shell32.dll");assert_eq!(p.entry,"Control_RunDLL");assert_eq!(p.arguments,vec!["app.cpl"]);}
 #[test] fn parses_separate_form(){let a=vec!["rundll32.exe".into(),"sample.dll".into(),"Entry".into(),"one".into()];assert_eq!(parse_invocation(&a).unwrap().arguments,vec!["one"]);}
 #[test] fn rejects_missing_entry(){assert!(parse_invocation(&["rundll32.exe".into(),"sample.dll".into()]).is_err());}
}
