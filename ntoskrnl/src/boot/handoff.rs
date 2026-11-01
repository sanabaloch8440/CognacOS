//! UEFI boot handoff and firmware-provided native asset catalog.
use crate::*;

const BOOT_INFO_MAGIC: u64 = 0x5244_4F54_4F53_4249;
#[repr(C)]
#[derive(Clone,Copy)]
struct BootFile { address:u64, length:u64 }
impl BootFile { const EMPTY:Self=Self{address:0,length:0}; }
static BOOT_HIVE_ADDRESS:[core::sync::atomic::AtomicU64;7]=[const{core::sync::atomic::AtomicU64::new(0)};7];
static BOOT_HIVE_LENGTH:[core::sync::atomic::AtomicU64;7]=[const{core::sync::atomic::AtomicU64::new(0)};7];
static BOOT_NLS_ADDRESS:[core::sync::atomic::AtomicU64;8]=[const{core::sync::atomic::AtomicU64::new(0)};8];
static BOOT_NLS_LENGTH:[core::sync::atomic::AtomicU64;8]=[const{core::sync::atomic::AtomicU64::new(0)};8];
static BOOT_NTDLL_ADDRESS:core::sync::atomic::AtomicU64=core::sync::atomic::AtomicU64::new(0);
static BOOT_NTDLL_LENGTH:core::sync::atomic::AtomicU64=core::sync::atomic::AtomicU64::new(0);
static BOOT_SYSTEM_DLL_ADDRESS:[core::sync::atomic::AtomicU64;4]=[const{core::sync::atomic::AtomicU64::new(0)};4];
static BOOT_SYSTEM_DLL_LENGTH:[core::sync::atomic::AtomicU64;4]=[const{core::sync::atomic::AtomicU64::new(0)};4];
const BOOT_SYSTEM_DLL_NAMES:[&str;4]=["kernel32.dll","kernelbase.dll","shell32.dll","msvcrt.dll"];
static BOOT_BEEP_ADDRESS:core::sync::atomic::AtomicU64=core::sync::atomic::AtomicU64::new(0);
static BOOT_BEEP_LENGTH:core::sync::atomic::AtomicU64=core::sync::atomic::AtomicU64::new(0);
static BOOT_SECURITY_FLAGS:core::sync::atomic::AtomicU64=core::sync::atomic::AtomicU64::new(0);
static BOOT_KERNEL_DIGEST:[core::sync::atomic::AtomicU8;32]=[const{core::sync::atomic::AtomicU8::new(0)};32];

/// Security properties attested by the bootloader before transferring control.
pub fn boot_security_flags()->u64{BOOT_SECURITY_FLAGS.load(core::sync::atomic::Ordering::Acquire)}
/// SHA-256 of the exact kernel PE32+ file verified by the bootloader.
pub fn boot_kernel_digest()->[u8;32]{
    let mut digest=[0u8;32];
    for (index,byte) in digest.iter_mut().enumerate(){
        *byte=BOOT_KERNEL_DIGEST[index].load(core::sync::atomic::Ordering::Acquire);
    }
    digest
}
pub fn boot_kernel_signature_verified()->bool{boot_security_flags()&(1<<6)!=0}
pub fn boot_secure_boot_enabled()->Option<bool>{
    let flags=boot_security_flags();
    if flags&(1<<0)==0{None}else{Some(flags&(1<<1)!=0)}
}
pub fn boot_tpm_kernel_measurement_recorded()->bool{boot_security_flags()&(1<<5)!=0}
static mut THREAD_SMOKE_STACK: [u8; 16384] = [0; 16384];
static mut THREAD_BLOCK_STACK: [u8; 16384] = [0; 16384];
static mut THREAD_WAKE_STACK: [u8; 16384] = [0; 16384];
static THREAD_SMOKE_RAN: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
static THREAD_BLOCK_STAGE: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);

extern "C" fn thread_smoke_entry(_argument: *mut core::ffi::c_void) {
    THREAD_SMOKE_RAN.store(true, core::sync::atomic::Ordering::Release);
}
extern "C" fn thread_block_smoke_entry(_argument: *mut core::ffi::c_void) {
    THREAD_BLOCK_STAGE.store(1, core::sync::atomic::Ordering::Release);
    if thread::block_current().is_err() {
        THREAD_BLOCK_STAGE.store(0xff, core::sync::atomic::Ordering::Release);
        return;
    }
    THREAD_BLOCK_STAGE.store(3, core::sync::atomic::Ordering::Release);
}
extern "C" fn thread_wake_smoke_entry(argument: *mut core::ffi::c_void) {
    if thread::wake(argument as u64).is_ok() {
        THREAD_BLOCK_STAGE.store(2, core::sync::atomic::Ordering::Release);
    } else {
        THREAD_BLOCK_STAGE.store(0xfe, core::sync::atomic::Ordering::Release);
    }
}

unsafe extern "C" {
    pub(crate) static __text_start u8;
    pub(crate) static __text_end u8;
    pub(crate) static __rodata_start u8;
    pub(crate) static __rodata_end u8;
    pub(crate) static __data_start u8;
    pub(crate) static __data_end u8;
    pub(crate) static __bss_start u8;
    pub(crate) static __bss_end u8;
    pub(crate) static __text_phys_start u8;
    pub(crate) static __text_phys_end u8;
    pub(crate) static __rodata_phys_start u8;
    pub(crate) static __rodata_phys_end u8;
    pub(crate) static __data_phys_start u8;
    pub(crate) static __data_phys_end u8;
    pub(crate) static __bss_phys_start u8;
    pub(crate) static __bss_phys_end u8;
}

#[repr(C)]
pub(crate) struct BootInfo {
    pub(crate) magic: u64,
    pub(crate) version: u32,
    pub(crate) struct_size: u32,
    pml4_physical: u64,
    pub(crate) memory_map: *const memory::MemoryDescriptor,
    pub(crate) memory_map_entries: usize,
    pub(crate) memory_descriptor_size: usize,
    pub(crate) memory_descriptor_version: u32,
    pub(crate) kernel_stack_guard_page: u64,
    pub(crate) kernel_stack_pages: u64,
    pub(crate) kernel_stack_virtual_guard: u64,
    pub(crate) rsdp_address: u64,
    pub(crate) ap_trampoline_physical: u64,
    pub(crate) kernel_physical_sections: [(u64, u64); 4],
    pub(crate) kernel_physical_section_count: usize,
    pub(crate) registry_hives: [BootFile;7],
    pub(crate) nls_files: [BootFile;8],
    pub(crate) ntdll_file: BootFile,
    pub(crate) system_dlls: [BootFile;4],
    pub(crate) beep_file: BootFile,
    pub(crate) security_flags: u64,
    pub(crate) kernel_digest: [u8; 32],
}


pub fn boot_hive_bytes(name:&str)->Option<&'static [u8]>{
 let kind=ntoskrnl::cm::hive::hive_kind(name).ok()? as usize;
 let address=BOOT_HIVE_ADDRESS.get(kind)?.load(core::sync::atomic::Ordering::Acquire);
 let length=BOOT_HIVE_LENGTH.get(kind)?.load(core::sync::atomic::Ordering::Acquire) as usize;
 if address==0||length==0||length>256*1024*1024{return None;}
 Some(unsafe{core::slice::from_raw_parts(address as *const u8,length)})
}
pub fn boot_ntdll_bytes()->Option<&'static [u8]>{
 let address=BOOT_NTDLL_ADDRESS.load(core::sync::atomic::Ordering::Acquire);
 let length=BOOT_NTDLL_LENGTH.load(core::sync::atomic::Ordering::Acquire) as usize;
 if address==0||length==0||length>16*1024*1024{return None;}
 Some(unsafe{core::slice::from_raw_parts(address as *const u8,length)})
}
pub fn boot_system_dll_bytes(name:&str)->Option<&'static [u8]>{
 let index=BOOT_SYSTEM_DLL_NAMES.iter().position(|candidate|candidate.eq_ignore_ascii_case(name))?;
 let address=BOOT_SYSTEM_DLL_ADDRESS[index].load(core::sync::atomic::Ordering::Acquire);
 let length=BOOT_SYSTEM_DLL_LENGTH[index].load(core::sync::atomic::Ordering::Acquire) as usize;
 if address==0||length==0||length>16*1024*1024{return None;}
 Some(unsafe{core::slice::from_raw_parts(address as *const u8,length)})
}
pub fn boot_beep_bytes()->Option<&'static [u8]>{
 let address=BOOT_BEEP_ADDRESS.load(core::sync::atomic::Ordering::Acquire);
 let length=BOOT_BEEP_LENGTH.load(core::sync::atomic::Ordering::Acquire) as usize;
 if address==0||length==0||length>4*1024*1024{return None;}
 Some(unsafe{core::slice::from_raw_parts(address as *const u8,length)})
}
pub fn boot_nls_bytes(name:&str)->Option<&'static [u8]>{
 let index=nls::FILES.iter().position(|f|f.name.eq_ignore_ascii_case(name))?;
 let address=BOOT_NLS_ADDRESS[index].load(core::sync::atomic::Ordering::Acquire);
 let length=BOOT_NLS_LENGTH[index].load(core::sync::atomic::Ordering::Acquire) as usize;
 if address==0||length==0||length>nls::MAX_NLS_FILE_SIZE{return None;}
 Some(unsafe{core::slice::from_raw_parts(address as *const u8,length)})
}
pub(crate) unsafe fn install_boot_assets(info:&BootInfo){
 let mut hive_count=0usize;let mut nls_count=0usize;
 for i in 0..info.registry_hives.len(){
  let file=info.registry_hives[i];if file.address==0&&file.length==0{continue;}
  if file.address==0||file.length==0||file.length>256*1024*1024||file.address.checked_add(file.length).is_none_or(|e|e>0x1_0000_0000){unsafe{serial_write(b"Rejected malformed boot registry asset descriptor.\r\n");}continue;}
  let bytes=unsafe{core::slice::from_raw_parts(file.address as *const u8,file.length as usize)};
  if ntoskrnl::cm::hive::Hive::open(bytes).is_err(){unsafe{serial_write(b"Rejected invalid or dirty native registry hive.\r\n");}continue;}
  if ntoskrnl::cm::runtime::mount_hive_bytes(ntoskrnl::cm::hive::HIVE_NAMES[i],bytes).is_err(){
   unsafe{serial_write(b"Configuration Manager rejected registry hive mount.\r\n");}
   continue;
  }
  BOOT_HIVE_ADDRESS[i].store(file.address,core::sync::atomic::Ordering::Release);
  BOOT_HIVE_LENGTH[i].store(file.length,core::sync::atomic::Ordering::Release);hive_count+=1;
 }
 let ntdll=info.ntdll_file;
 if ntdll.address!=0||ntdll.length!=0 {
  if ntdll.address==0||ntdll.length==0||ntdll.length>16*1024*1024||ntdll.address.checked_add(ntdll.length).is_none_or(|e|e>0x1_0000_0000){
   unsafe{serial_write(b"Rejected malformed boot ntdll.dll descriptor.\r\n");}
  }else{
   BOOT_NTDLL_ADDRESS.store(ntdll.address,core::sync::atomic::Ordering::Release);
   BOOT_NTDLL_LENGTH.store(ntdll.length,core::sync::atomic::Ordering::Release);
  }
 }
 for (index,file) in info.system_dlls.iter().copied().enumerate() {
  if file.address==0&&file.length==0 { continue; }
  if file.address==0||file.length==0||file.length>16*1024*1024||file.address.checked_add(file.length).is_none_or(|e|e>0x1_0000_0000) {
   unsafe{serial_write(b"Rejected malformed boot system DLL descriptor.\\r\\n");}
   continue;
  }
  let bytes=unsafe{core::slice::from_raw_parts(file.address as *const u8,file.length as usize)};
  if bytes.get(..2)==Some(b"MZ") {
   BOOT_SYSTEM_DLL_ADDRESS[index].store(file.address,core::sync::atomic::Ordering::Release);
   BOOT_SYSTEM_DLL_LENGTH[index].store(file.length,core::sync::atomic::Ordering::Release);
  } else {
   unsafe{serial_write(b"Rejected staged system DLL without MZ signature.\\r\\n");}
  }
 }
 let beep=info.beep_file;
 if beep.address!=0||beep.length!=0 {
  if beep.address==0||beep.length==0||beep.length>4*1024*1024||beep.address.checked_add(beep.length).is_none_or(|e|e>0x1_0000_0000){
   unsafe{serial_write(b"Rejected malformed boot beep.sys descriptor.\r\n");}
  }else{
   let bytes=unsafe{core::slice::from_raw_parts(beep.address as *const u8,beep.length as usize)};
   if bytes.get(..2)==Some(b"MZ") {
    BOOT_BEEP_ADDRESS.store(beep.address,core::sync::atomic::Ordering::Release);
    BOOT_BEEP_LENGTH.store(beep.length,core::sync::atomic::Ordering::Release);
    unsafe{serial_write(b"Native beep.sys driver staged for runtime loading.\r\n");}
   }else{unsafe{serial_write(b"Rejected boot beep.sys image without MZ signature.\r\n");}}
  }
 }
 for i in 0..info.nls_files.len(){
  let file=info.nls_files[i];if file.address==0&&file.length==0{continue;}
  if file.address==0||file.length==0||file.length>nls::MAX_NLS_FILE_SIZE as u64||file.address.checked_add(file.length).is_none_or(|e|e>0x1_0000_0000){unsafe{serial_write(b"Rejected malformed boot NLS asset descriptor.\r\n");}continue;}
  let bytes=unsafe{core::slice::from_raw_parts(file.address as *const u8,file.length as usize)};
  if nls::NlsFile::open(nls::FILES[i].name,bytes).is_err(){unsafe{serial_write(b"Rejected invalid native NLS file.\r\n");}continue;}
  BOOT_NLS_ADDRESS[i].store(file.address,core::sync::atomic::Ordering::Release);
  BOOT_NLS_LENGTH[i].store(file.length,core::sync::atomic::Ordering::Release);nls_count+=1;
 }
 unsafe{serial_write(b"Native Windows assets staged: registry hives=");serial_write_hex(hive_count as u64);serial_write(b", NLS tables=");serial_write_hex(nls_count as u64);serial_write(b"\r\n");}
}

