//! Native x64 rundll32.exe command-line DLL entry-point host.
#![cfg_attr(target_os = "windows", no_std)]
#![cfg_attr(target_os = "windows", no_main)]

#[cfg(windows)]
use core::ffi::{c_char, c_void};

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCommandLineW() -> *mut u16;
    fn LoadLibraryW(name: *const u16) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
    fn FreeLibrary(module: *mut c_void) -> i32;
    fn ExitProcess(code: u32) -> !;
}

#[cfg(windows)]
unsafe fn skip_space(mut p: *const u16) -> *const u16 {
    while !p.is_null() && unsafe { *p } != 0 && unsafe { *p } <= 0x20 { p = unsafe { p.add(1) }; }
    p
}

#[cfg(windows)]
unsafe fn token(mut p: *const u16, out: &mut [u16]) -> Result<*const u16, ()> {
    p = unsafe { skip_space(p) };
    if p.is_null() || unsafe { *p } == 0 { return Err(()); }
    let mut n=0usize; let mut quoted=false;
    while unsafe { *p } != 0 {
        let ch=unsafe{*p};
        if ch==b'"' as u16 { quoted=!quoted; p=unsafe{p.add(1)}; continue; }
        if !quoted && ch<=0x20 { break; }
        if n+1>=out.len(){return Err(());}
        out[n]=ch;n+=1;p=unsafe{p.add(1)};
    }
    out[n]=0;
    Ok(unsafe{skip_space(p)})
}

#[cfg(windows)]
#[no_mangle]
pub unsafe extern "system" fn rundll32_entry() -> ! {
    let command=unsafe{GetCommandLineW()};
    let mut executable=[0u16;1024];
    let mut module_name=[0u16;2048];
    let mut entry_name=[0u16;512];
    let mut p=match unsafe{token(command,&mut executable)} {Ok(v)=>v,Err(_)=>unsafe{ExitProcess(2)}};
    p=match unsafe{token(p,&mut module_name)} {Ok(v)=>v,Err(_)=>unsafe{ExitProcess(2)}};

    let comma=module_name.iter().position(|c|*c==b',' as u16);
    if let Some(index)=comma {
        let tail=module_name[index+1..].iter().position(|c|*c==0).unwrap_or(0);
        let count=tail;
        if count==0 || count>=entry_name.len(){unsafe{ExitProcess(2)}}
        entry_name[..count].copy_from_slice(&module_name[index+1..index+1+count]);
        module_name[index]=0;
    } else {
        p=match unsafe{token(p,&mut entry_name)} {Ok(v)=>v,Err(_)=>unsafe{ExitProcess(2)}};
    }

    let module=unsafe{LoadLibraryW(module_name.as_ptr())};
    if module.is_null(){unsafe{ExitProcess(1)}}
    let mut entry_ansi=[0u8;512];
    let mut ei=0usize;
    while ei<entry_name.len() && entry_name[ei]!=0 {
        if entry_name[ei]>0x7f {unsafe{FreeLibrary(module);ExitProcess(2)}}
        entry_ansi[ei]=entry_name[ei] as u8; ei+=1;
    }
    let address=if entry_name[0]==b'#' as u16 {
        let mut ordinal=0u16;
        let mut i=1usize;
        while i<entry_name.len() && entry_name[i]!=0 {
            let d=entry_name[i].wrapping_sub(b'0' as u16);
            if d>9 || ordinal>6553 {unsafe{FreeLibrary(module);ExitProcess(2)}}
            ordinal=ordinal*10+d as u16;i+=1;
        }
        if ordinal==0 {unsafe{FreeLibrary(module);ExitProcess(2)}}
        unsafe{GetProcAddress(module,ordinal as usize as *const c_char)}
    } else {
        unsafe{GetProcAddress(module,entry_ansi.as_ptr().cast())}
    };
    if address.is_null(){unsafe{FreeLibrary(module);ExitProcess(1)}}
    type RundllEntry=unsafe extern "system" fn(*mut c_void,*mut c_void,*const u16,i32);
    let callback:RundllEntry=unsafe{core::mem::transmute(address)};
    unsafe{callback(core::ptr::null_mut(),module,p,1);FreeLibrary(module);ExitProcess(0)}
}

#[cfg(target_os = "windows")]
#[panic_handler]
fn panic(_info:&core::panic::PanicInfo<'_>)->!{loop{core::hint::spin_loop();}}

#[cfg(not(windows))]
fn main() {
    eprintln!("rundll32 is a Windows x64 program; build for x86_64-pc-windows-msvc.");
}

#[cfg(all(test, not(windows)))]
mod tests {
    #[test]
    fn host_target_is_intentionally_not_a_windows_launcher() {
        assert!(cfg!(not(windows)));
    }
}
