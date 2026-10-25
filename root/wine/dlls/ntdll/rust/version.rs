//! Source-first Rust port of Wine NTDLL version-comparison logic from version.c.
//!
//! This module ports the deterministic comparison core and ABI structure. It
//! deliberately takes the running OS version as an input: the final NTDLL
//! export wrapper must obtain that data from CognacOS's authoritative version
//! source rather than hard-code a Windows build number.
#![allow(non_snake_case)]
pub type ULONG=u32; pub type USHORT=u16; pub type UCHAR=u8; pub type NTSTATUS=i32; pub type DWORDLONG=u64;
pub const STATUS_SUCCESS:NTSTATUS=0;
pub const STATUS_INVALID_PARAMETER:NTSTATUS=0xC000_000D_u32 as i32;
pub const STATUS_REVISION_MISMATCH:NTSTATUS=0xC000_0059_u32 as i32;
pub const VER_MINORVERSION:ULONG=0x1; pub const VER_MAJORVERSION:ULONG=0x2; pub const VER_BUILDNUMBER:ULONG=0x4; pub const VER_PLATFORMID:ULONG=0x8;
pub const VER_SERVICEPACKMINOR:ULONG=0x10; pub const VER_SERVICEPACKMAJOR:ULONG=0x20; pub const VER_SUITENAME:ULONG=0x40; pub const VER_PRODUCT_TYPE:ULONG=0x80;
pub const VER_EQUAL:UCHAR=1; pub const VER_GREATER:UCHAR=2; pub const VER_GREATER_EQUAL:UCHAR=3; pub const VER_LESS:UCHAR=4; pub const VER_LESS_EQUAL:UCHAR=5; pub const VER_AND:UCHAR=6; pub const VER_OR:UCHAR=7;
const SHIFT_MINOR:u32=0; const SHIFT_MAJOR:u32=3; const SHIFT_BUILD:u32=6; const SHIFT_PLATFORM:u32=9; const SHIFT_SP_MINOR:u32=12; const SHIFT_SP_MAJOR:u32=15; const SHIFT_SUITE:u32=18; const SHIFT_PRODUCT:u32=21;
#[repr(C)]
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct RTL_OSVERSIONINFOEXW { pub dwOSVersionInfoSize:ULONG,pub dwMajorVersion:ULONG,pub dwMinorVersion:ULONG,pub dwBuildNumber:ULONG,pub dwPlatformId:ULONG,pub szCSDVersion:[u16;128],pub wServicePackMajor:USHORT,pub wServicePackMinor:USHORT,pub wSuiteMask:USHORT,pub wProductType:UCHAR,pub wReserved:UCHAR }
impl Default for RTL_OSVERSIONINFOEXW { fn default()->Self{Self{dwOSVersionInfoSize:core::mem::size_of::<Self>() as ULONG,dwMajorVersion:0,dwMinorVersion:0,dwBuildNumber:0,dwPlatformId:0,szCSDVersion:[0;128],wServicePackMajor:0,wServicePackMinor:0,wSuiteMask:0,wProductType:0,wReserved:0}} }
#[inline] fn condition(mask:DWORDLONG,shift:u32)->UCHAR{((mask>>shift)&7) as UCHAR}
#[inline] fn compare_value(left:ULONG,right:ULONG,op:UCHAR)->NTSTATUS{let ok=match op{VER_EQUAL=>left==right,VER_GREATER=>left>right,VER_GREATER_EQUAL=>left>=right,VER_LESS=>left<right,VER_LESS_EQUAL=>left<=right,_=>return STATUS_REVISION_MISMATCH};if ok{STATUS_SUCCESS}else{STATUS_REVISION_MISMATCH}}
fn update_condition(last:&mut UCHAR,next:UCHAR)->UCHAR{match *last{0=>*last=next,VER_EQUAL=>if (VER_EQUAL..=VER_LESS_EQUAL).contains(&next){*last=next;return next},VER_GREATER|VER_GREATER_EQUAL=>if (VER_EQUAL..=VER_GREATER_EQUAL).contains(&next){return next},VER_LESS|VER_LESS_EQUAL=>if next==VER_EQUAL||(VER_LESS..=VER_LESS_EQUAL).contains(&next){return next},_=>{}}if next==0{*last|=0x10}*last&0x0f}
/// Implements the deterministic comparison core of RtlVerifyVersionInfo.
/// The current version must come from the OS's authoritative version provider.
pub fn verify_version_info(current:&RTL_OSVERSIONINFOEXW,requested:&RTL_OSVERSIONINFOEXW,type_mask:ULONG,condition_mask:DWORDLONG)->NTSTATUS{
 if type_mask==0||condition_mask==0{return STATUS_INVALID_PARAMETER}
 let checks=[(VER_PRODUCT_TYPE,SHIFT_PRODUCT,current.wProductType as ULONG,requested.wProductType as ULONG),(VER_PLATFORMID,SHIFT_PLATFORM,current.dwPlatformId,requested.dwPlatformId),(VER_BUILDNUMBER,SHIFT_BUILD,current.dwBuildNumber,requested.dwBuildNumber)];
 for(flag,shift,left,right)in checks{if type_mask&flag!=0{let s=compare_value(left,right,condition(condition_mask,shift));if s!=STATUS_SUCCESS{return s}}}
 if type_mask&VER_SUITENAME!=0{match condition(condition_mask,SHIFT_SUITE){VER_AND if requested.wSuiteMask&current.wSuiteMask!=requested.wSuiteMask=>return STATUS_REVISION_MISMATCH,VER_OR if requested.wSuiteMask!=0&&requested.wSuiteMask&current.wSuiteMask==0=>return STATUS_REVISION_MISMATCH,VER_AND|VER_OR=>{},_=>return STATUS_INVALID_PARAMETER}}
 if type_mask&(VER_MAJORVERSION|VER_MINORVERSION|VER_SERVICEPACKMAJOR|VER_SERVICEPACKMINOR)!=0{let mut last=0;let mut do_next=true;let versions=[(VER_MAJORVERSION,SHIFT_MAJOR,current.dwMajorVersion,requested.dwMajorVersion),(VER_MINORVERSION,SHIFT_MINOR,current.dwMinorVersion,requested.dwMinorVersion),(VER_SERVICEPACKMAJOR,SHIFT_SP_MAJOR,current.wServicePackMajor as ULONG,requested.wServicePackMajor as ULONG),(VER_SERVICEPACKMINOR,SHIFT_SP_MINOR,current.wServicePackMinor as ULONG,requested.wServicePackMinor as ULONG)];for(flag,shift,left,right)in versions{if type_mask&flag==0||!do_next{continue}let op=update_condition(&mut last,condition(condition_mask,shift));let s=compare_value(left,right,op);if s!=STATUS_SUCCESS{return s}do_next=left==right&&(VER_EQUAL..=VER_LESS_EQUAL).contains(&op)}}
 STATUS_SUCCESS
}
/// Copies version fields from a supplied authoritative record. It does not invent an OS version.
pub unsafe fn copy_version_info(output:*mut RTL_OSVERSIONINFOEXW,current:&RTL_OSVERSIONINFOEXW)->NTSTATUS{
 if output.is_null(){return STATUS_INVALID_PARAMETER}
 // SAFETY: caller supplies writable version-info storage; read its advertised size first.
 let size=unsafe{core::ptr::addr_of!((*output).dwOSVersionInfoSize).read()};
 if size<276{return STATUS_INVALID_PARAMETER}
 // SAFETY: output points to the ABI structure; only write the extended tail when its size is advertised.
 unsafe{core::ptr::addr_of_mut!((*output).dwMajorVersion).write(current.dwMajorVersion);core::ptr::addr_of_mut!((*output).dwMinorVersion).write(current.dwMinorVersion);core::ptr::addr_of_mut!((*output).dwBuildNumber).write(current.dwBuildNumber);core::ptr::addr_of_mut!((*output).dwPlatformId).write(current.dwPlatformId);core::ptr::addr_of_mut!((*output).szCSDVersion).write(current.szCSDVersion);if size>=core::mem::size_of::<RTL_OSVERSIONINFOEXW>() as ULONG{core::ptr::addr_of_mut!((*output).wServicePackMajor).write(current.wServicePackMajor);core::ptr::addr_of_mut!((*output).wServicePackMinor).write(current.wServicePackMinor);core::ptr::addr_of_mut!((*output).wSuiteMask).write(current.wSuiteMask);core::ptr::addr_of_mut!((*output).wProductType).write(current.wProductType);core::ptr::addr_of_mut!((*output).wReserved).write(current.wReserved)}}STATUS_SUCCESS
}
#[cfg(test)]mod tests{use super::*;fn v(a:u32,b:u32,c:u32)->RTL_OSVERSIONINFOEXW{RTL_OSVERSIONINFOEXW{dwMajorVersion:a,dwMinorVersion:b,dwBuildNumber:c,dwPlatformId:2,wProductType:1,..Default::default()}}fn cond(op:u64,shift:u32)->u64{op<<shift}
#[test]fn build_ordering(){let cur=v(10,0,26100);let req=v(10,0,22631);assert_eq!(verify_version_info(&cur,&req,VER_BUILDNUMBER,cond(VER_GREATER,SHIFT_BUILD)),STATUS_SUCCESS);assert_eq!(verify_version_info(&cur,&req,VER_BUILDNUMBER,cond(VER_LESS,SHIFT_BUILD)),STATUS_REVISION_MISMATCH);let req=v(10,0,26100);assert_eq!(verify_version_info(&cur,&req,VER_BUILDNUMBER,cond(VER_EQUAL,SHIFT_BUILD)),STATUS_SUCCESS)}
#[test]fn major_minor_ordering(){let cur=v(10,0,1);let req=v(10,1,0);let mask=cond(VER_GREATER_EQUAL,SHIFT_MAJOR)|cond(VER_LESS,SHIFT_MINOR);assert_eq!(verify_version_info(&cur,&req,VER_MAJORVERSION|VER_MINORVERSION,mask),STATUS_SUCCESS)}
#[test]fn suite_and_product(){let mut cur=v(10,0,1);let mut req=v(10,0,1);cur.wSuiteMask=0b1010;req.wSuiteMask=0b0010;assert_eq!(verify_version_info(&cur,&req,VER_SUITENAME,cond(VER_AND,SHIFT_SUITE)),STATUS_SUCCESS);req.wProductType=3;assert_eq!(verify_version_info(&cur,&req,VER_PRODUCT_TYPE,cond(VER_EQUAL,SHIFT_PRODUCT)),STATUS_REVISION_MISMATCH)}
#[test]fn rejects_bad_masks(){let x=v(10,0,1);assert_eq!(verify_version_info(&x,&x,0,1),STATUS_INVALID_PARAMETER);assert_eq!(verify_version_info(&x,&x,VER_SUITENAME,cond(VER_EQUAL,SHIFT_SUITE)),STATUS_INVALID_PARAMETER)}
#[test]fn x64_layout(){assert_eq!(core::mem::size_of::<RTL_OSVERSIONINFOEXW>(),284)}
}

// Product constants used by Wine's RtlGetProductInfo compatibility routine.
pub const VER_NT_WORKSTATION: UCHAR = 1;
pub const VER_NT_DOMAIN_CONTROLLER: UCHAR = 2;
pub const VER_NT_SERVER: UCHAR = 3;
pub const PRODUCT_UNDEFINED: ULONG = 0x0000_0000;
pub const PRODUCT_ULTIMATE_N: ULONG = 0x0000_0027;
pub const PRODUCT_STANDARD_SERVER: ULONG = 0x0000_0007;

/// Port of Wine's RtlGetNtVersionNumbers output convention. The high nibble
/// of the build value is set; nullable output pointers are intentionally valid.
pub unsafe fn get_nt_version_numbers(current: &RTL_OSVERSIONINFOEXW,
    major: *mut ULONG, minor: *mut ULONG, build: *mut ULONG) {
    // SAFETY: each non-null pointer is caller-provided writable ULONG storage.
    unsafe {
        if !major.is_null() { major.write(current.dwMajorVersion); }
        if !minor.is_null() { minor.write(current.dwMinorVersion); }
        if !build.is_null() { build.write(0xF000_0000 | current.dwBuildNumber); }
    }
}

/// Port of Wine's RtlGetNtProductType: report the provider's product type.
/// A null pointer is tolerated, matching the source routine's behavior.
pub unsafe fn get_nt_product_type(current: &RTL_OSVERSIONINFOEXW, out: *mut ULONG) -> bool {
    // SAFETY: out is only written when non-null.
    unsafe { if !out.is_null() { out.write(current.wProductType as ULONG); } }
    true
}

/// Port of Wine's current RtlGetProductInfo policy, not a complete catalogue
/// of Windows SKU mappings. Wine reports Ultimate N for workstations and
/// Standard Server for other product types when major version >= 6.
pub unsafe fn get_product_info(current: &RTL_OSVERSIONINFOEXW, major: ULONG,
    _minor: ULONG, _sp_major: ULONG, _sp_minor: ULONG, out: *mut ULONG) -> bool {
    if out.is_null() { return false; }
    // SAFETY: out is checked non-null and points to writable ULONG storage.
    unsafe {
        if major < 6 {
            out.write(PRODUCT_UNDEFINED);
            return false;
        }
        out.write(if current.wProductType == VER_NT_WORKSTATION {
            PRODUCT_ULTIMATE_N
        } else {
            PRODUCT_STANDARD_SERVER
        });
    }
    true
}

/// A source-level equivalent of the C RtlGetVersion field-copy behavior.
/// It writes the base OSVERSIONINFOW fields, and the extended tail only when
/// the caller's size exactly equals RTL_OSVERSIONINFOEXW. The caller must
/// provide writable memory for the advertised structure size.
pub unsafe fn get_version_fields(output: *mut RTL_OSVERSIONINFOEXW,
    current: &RTL_OSVERSIONINFOEXW) -> NTSTATUS {
    if output.is_null() { return STATUS_INVALID_PARAMETER; }
    // SAFETY: caller guarantees output has at least the base structure and is writable.
    let advertised = unsafe { core::ptr::addr_of!((*output).dwOSVersionInfoSize).read() };
    if advertised < 276 { return STATUS_INVALID_PARAMETER; }
    // SAFETY: fields written are within the base OSVERSIONINFOW layout.
    unsafe {
        core::ptr::addr_of_mut!((*output).dwMajorVersion).write(current.dwMajorVersion);
        core::ptr::addr_of_mut!((*output).dwMinorVersion).write(current.dwMinorVersion);
        core::ptr::addr_of_mut!((*output).dwBuildNumber).write(current.dwBuildNumber);
        core::ptr::addr_of_mut!((*output).dwPlatformId).write(current.dwPlatformId);
        core::ptr::addr_of_mut!((*output).szCSDVersion).write(current.szCSDVersion);
        if advertised == core::mem::size_of::<RTL_OSVERSIONINFOEXW>() as ULONG {
            core::ptr::addr_of_mut!((*output).wServicePackMajor).write(current.wServicePackMajor);
            core::ptr::addr_of_mut!((*output).wServicePackMinor).write(current.wServicePackMinor);
            core::ptr::addr_of_mut!((*output).wSuiteMask).write(current.wSuiteMask);
            core::ptr::addr_of_mut!((*output).wProductType).write(current.wProductType);
            core::ptr::addr_of_mut!((*output).wReserved).write(current.wReserved);
        }
    }
    STATUS_SUCCESS
}

#[cfg(test)]
mod added_version_api_tests {
    use super::*;
    #[test]
    fn version_numbers_sets_build_flag_and_allows_null_outputs() {
        let current = RTL_OSVERSIONINFOEXW { dwMajorVersion: 10, dwMinorVersion: 0,
            dwBuildNumber: 26100, ..Default::default() };
        let mut major = 0; let mut minor = 99; let mut build = 0;
        unsafe { get_nt_version_numbers(&current, &mut major, &mut minor, &mut build); }
        assert_eq!((major, minor, build), (10, 0, 0xF000_65F4));
        unsafe { get_nt_version_numbers(&current, core::ptr::null_mut(), core::ptr::null_mut(), core::ptr::null_mut()); }
    }
    #[test]
    fn product_type_and_product_info() {
        let mut current = RTL_OSVERSIONINFOEXW { dwMajorVersion: 10,
            wProductType: VER_NT_WORKSTATION, ..Default::default() };
        let mut out = 0;
        unsafe { assert!(get_nt_product_type(&current, &mut out)); }
        assert_eq!(out, VER_NT_WORKSTATION as ULONG);
        unsafe { assert!(get_product_info(&current, 10, 0, 0, 0, &mut out)); }
        assert_eq!(out, PRODUCT_ULTIMATE_N);
        current.wProductType = VER_NT_SERVER;
        unsafe { assert!(get_product_info(&current, 10, 0, 0, 0, &mut out)); }
        assert_eq!(out, PRODUCT_STANDARD_SERVER);
        unsafe { assert!(!get_product_info(&current, 5, 0, 0, 0, &mut out)); }
        assert_eq!(out, PRODUCT_UNDEFINED);
    }
    #[test]
    fn version_copy_only_populates_extended_tail_for_exact_size() {
        let current = RTL_OSVERSIONINFOEXW { dwMajorVersion: 10, dwBuildNumber: 26100,
            wServicePackMajor: 2, ..Default::default() };
        let mut out = RTL_OSVERSIONINFOEXW { dwOSVersionInfoSize: 276,
            wServicePackMajor: 77, ..Default::default() };
        unsafe { assert_eq!(get_version_fields(&mut out, &current), STATUS_SUCCESS); }
        assert_eq!(out.dwBuildNumber, 26100);
        assert_eq!(out.wServicePackMajor, 77);
        out.dwOSVersionInfoSize = 284;
        unsafe { assert_eq!(get_version_fields(&mut out, &current), STATUS_SUCCESS); }
        assert_eq!(out.wServicePackMajor, 2);
    }
}


/// Port of Wine's VerSetConditionMask. When several type bits are present,
/// the source's priority order selects exactly one condition slot per call.
pub fn set_condition_mask(mut mask: DWORDLONG, type_mask: ULONG, condition_value: UCHAR) -> DWORDLONG {
    let condition_value = condition_value & 0x07;
    let selected = if type_mask & VER_PRODUCT_TYPE != 0 { Some(SHIFT_PRODUCT) }
        else if type_mask & VER_SUITENAME != 0 { Some(SHIFT_SUITE) }
        else if type_mask & VER_SERVICEPACKMAJOR != 0 { Some(SHIFT_SP_MAJOR) }
        else if type_mask & VER_SERVICEPACKMINOR != 0 { Some(SHIFT_SP_MINOR) }
        else if type_mask & VER_PLATFORMID != 0 { Some(SHIFT_PLATFORM) }
        else if type_mask & VER_BUILDNUMBER != 0 { Some(SHIFT_BUILD) }
        else if type_mask & VER_MAJORVERSION != 0 { Some(SHIFT_MAJOR) }
        else if type_mask & VER_MINORVERSION != 0 { Some(SHIFT_MINOR) }
        else { None };
    if let Some(shift) = selected {
        mask |= (condition_value as DWORDLONG) << shift;
    }
    mask
}

#[cfg(test)]
mod condition_mask_tests {
    use super::*;
    #[test]
    fn set_condition_mask_uses_windows_slots_and_priority() {
        let mask = set_condition_mask(0, VER_MAJORVERSION, VER_GREATER_EQUAL);
        assert_eq!((mask >> SHIFT_MAJOR) & 7, VER_GREATER_EQUAL as u64);
        let mask = set_condition_mask(mask, VER_MINORVERSION, VER_LESS);
        assert_eq!((mask >> SHIFT_MINOR) & 7, VER_LESS as u64);
        let both = set_condition_mask(0, VER_PRODUCT_TYPE | VER_BUILDNUMBER, VER_EQUAL);
        assert_eq!((both >> SHIFT_PRODUCT) & 7, VER_EQUAL as u64);
        assert_eq!((both >> SHIFT_BUILD) & 7, 0);
    }
    #[test]
    fn set_condition_mask_truncates_condition_to_three_bits() {
        let mask = set_condition_mask(0, VER_BUILDNUMBER, 0xff);
        assert_eq!((mask >> SHIFT_BUILD) & 7, 7);
    }
}


/// Wine configuration aliases accepted by version.c's `Version` value.
/// These are compatibility profiles, not a statement about CognacOS's own
/// running version. Alias matching is ASCII case-insensitive, like the source
/// configuration parser's registry value handling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum WindowsVersionAlias {
    Win20, Win30, Win31, Win95, Win98, WinMe, Nt351, Nt40, Win2000,
    WinXp, WinXp64, Win2003, Vista, Win2008, Win2008R2, Win7, Win8,
    Win81, Win10, Win11,
}
const VERSION_ALIASES: &[(&str, WindowsVersionAlias)] = &[
    ("win20", WindowsVersionAlias::Win20), ("win30", WindowsVersionAlias::Win30),
    ("win31", WindowsVersionAlias::Win31), ("win95", WindowsVersionAlias::Win95),
    ("win98", WindowsVersionAlias::Win98), ("winme", WindowsVersionAlias::WinMe),
    ("nt351", WindowsVersionAlias::Nt351), ("nt40", WindowsVersionAlias::Nt40),
    ("win2000", WindowsVersionAlias::Win2000), ("win2k", WindowsVersionAlias::Win2000),
    ("nt2k", WindowsVersionAlias::Win2000), ("nt2000", WindowsVersionAlias::Win2000),
    ("winxp", WindowsVersionAlias::WinXp), ("winxp64", WindowsVersionAlias::WinXp64),
    ("win2003", WindowsVersionAlias::Win2003), ("win2k3", WindowsVersionAlias::Win2003),
    ("vista", WindowsVersionAlias::Vista), ("winvista", WindowsVersionAlias::Vista),
    ("win2008", WindowsVersionAlias::Win2008), ("win2k8", WindowsVersionAlias::Win2008),
    ("win2008r2", WindowsVersionAlias::Win2008R2), ("win2k8r2", WindowsVersionAlias::Win2008R2),
    ("win7", WindowsVersionAlias::Win7), ("win8", WindowsVersionAlias::Win8),
    ("win81", WindowsVersionAlias::Win81), ("win10", WindowsVersionAlias::Win10),
    ("win11", WindowsVersionAlias::Win11),
];

/// Resolve a Wine `Software\\Wine\\Version` profile name. Unknown names
/// return None; callers can then fall back to registry-derived version data.
pub fn parse_version_alias(name: &str) -> Option<WindowsVersionAlias> {
    VERSION_ALIASES.iter().find_map(|(candidate, alias)|
        if candidate.eq_ignore_ascii_case(name) { Some(*alias) } else { None })
}

fn decimal_component(text: &str) -> Option<u32> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) { return None; }
    text.parse::<u32>().ok()
}

/// Parse an NT-style CurrentVersion string such as `10.0`. This mirrors the
/// source fallback used when CurrentMajorVersionNumber/CurrentMinorVersionNumber
/// are not present. Extra components are ignored, as in Wine's wcstoul path.
pub fn parse_nt_current_version(text: &str) -> Option<(u32, u32)> {
    let (major, rest) = text.split_once('.')?;
    let minor = rest.split('.').next()?;
    Some((decimal_component(major)?, decimal_component(minor)?))
}

/// Parse Win9x's VersionNumber (for example `4.10.2222`) and preserve its
/// packed major/minor bits in dwBuildNumber. The third component is optional.
pub fn parse_win9x_version_number(text: &str) -> Option<(u32, u32, u32)> {
    let mut parts = text.split('.');
    let major = decimal_component(parts.next()?)?;
    let minor = decimal_component(parts.next()?)?;
    let build = match parts.next() {
        Some(v) => decimal_component(v)?,
        None => 0,
    };
    if parts.next().is_some() || major > 255 || minor > 255 { return None; }
    let packed = build | (((minor & 0xff) | ((major & 0xff) << 8)) << 16);
    Some((major, minor, packed))
}

#[cfg(test)]
mod registry_version_parser_tests {
    use super::*;
    #[test]
    fn aliases_cover_wine_names_and_ignore_ascii_case() {
        assert_eq!(parse_version_alias("WINXP"), Some(WindowsVersionAlias::WinXp));
        assert_eq!(parse_version_alias("win2k8r2"), Some(WindowsVersionAlias::Win2008R2));
        assert_eq!(parse_version_alias("not-a-profile"), None);
    }
    #[test]
    fn nt_version_string_parser() {
        assert_eq!(parse_nt_current_version("10.0"), Some((10, 0)));
        assert_eq!(parse_nt_current_version("6.3.9600"), Some((6, 3)));
        assert_eq!(parse_nt_current_version("x.0"), None);
        assert_eq!(parse_nt_current_version("10"), None);
    }
    #[test]
    fn win9x_version_string_preserves_packed_build() {
        assert_eq!(parse_win9x_version_number("4.10.2222"), Some((4, 10, 0x040a08ae)));
        assert_eq!(parse_win9x_version_number("4.0.950"), Some((4, 0, 0x040003b6)));
        assert_eq!(parse_win9x_version_number("4.999.1"), None);
        assert_eq!(parse_win9x_version_number("4.10.x"), None);
    }
}


// Source-defined Wine compatibility profiles from version.c. These records
// describe an explicitly selected compatibility profile, not the host OS.
pub const VER_PLATFORM_WIN32S: ULONG = 0;
pub const VER_PLATFORM_WIN32_WINDOWS: ULONG = 1;
pub const VER_PLATFORM_WIN32_NT: ULONG = 2;
pub const VER_SUITE_SINGLEUSERTS: USHORT = 0x0100;

fn profile(major: ULONG, minor: ULONG, build: ULONG, platform: ULONG,
    csd: &[u16], sp_major: USHORT, sp_minor: USHORT, suite: USHORT,
    product: UCHAR, reserved: UCHAR) -> RTL_OSVERSIONINFOEXW {
    let mut value = RTL_OSVERSIONINFOEXW {
        dwMajorVersion: major, dwMinorVersion: minor, dwBuildNumber: build,
        dwPlatformId: platform, wServicePackMajor: sp_major,
        wServicePackMinor: sp_minor, wSuiteMask: suite,
        wProductType: product, wReserved: reserved, ..Default::default()
    };
    let count = core::cmp::min(csd.len(), value.szCSDVersion.len() - 1);
    value.szCSDVersion[..count].copy_from_slice(&csd[..count]);
    value
}

const fn wide_ascii<const N: usize>(s: &[u8; N]) -> [u16; N] {
    let mut out = [0u16; N];
    let mut i = 0;
    while i < N { out[i] = s[i] as u16; i += 1; }
    out
}
const CSD_SP5: [u16; 14] = wide_ascii(b"Service Pack 5");
const CSD_SP6A: [u16; 15] = wide_ascii(b"Service Pack 6a");
const CSD_SP4: [u16; 14] = wide_ascii(b"Service Pack 4");
const CSD_SP3: [u16; 14] = wide_ascii(b"Service Pack 3");
const CSD_SP2: [u16; 14] = wide_ascii(b"Service Pack 2");
const CSD_SP1: [u16; 14] = wide_ascii(b"Service Pack 1");
const CSD_WIN98: [u16; 3] = wide_ascii(b" A ");
const CSD_WINME: [u16; 1] = wide_ascii(b" ");

/// Get the source-defined record for a Wine compatibility alias.
pub fn version_profile(alias: WindowsVersionAlias) -> RTL_OSVERSIONINFOEXW {
    use WindowsVersionAlias::*;
    match alias {
        Win20 => profile(2, 0, 0, VER_PLATFORM_WIN32S, &[], 0, 0, 0, 0, 0),
        Win30 => profile(3, 0, 0, VER_PLATFORM_WIN32S, &[], 0, 0, 0, 0, 0),
        Win31 => profile(3, 10, 0, VER_PLATFORM_WIN32S, &[], 0, 0, 0, 0, 0),
        Win95 => profile(4, 0, 0x040003b6, VER_PLATFORM_WIN32_WINDOWS, &[], 0, 0, 0, 0, 0),
        Win98 => profile(4, 10, 0x040a08ae, VER_PLATFORM_WIN32_WINDOWS, &CSD_WIN98, 0, 0, 0, 0, 0),
        WinMe => profile(4, 90, 0x045a0bb8, VER_PLATFORM_WIN32_WINDOWS, &CSD_WINME, 0, 0, 0, 0, 0),
        Nt351 => profile(3, 51, 1057, VER_PLATFORM_WIN32_NT, &CSD_SP5, 5, 0, 0, VER_NT_WORKSTATION, 0),
        Nt40 => profile(4, 0, 1381, VER_PLATFORM_WIN32_NT, &CSD_SP6A, 6, 0, 0, VER_NT_WORKSTATION, 0),
        Win2000 => profile(5, 0, 2195, VER_PLATFORM_WIN32_NT, &CSD_SP4, 4, 0, 0, VER_NT_WORKSTATION, 30),
        WinXp => profile(5, 1, 2600, VER_PLATFORM_WIN32_NT, &CSD_SP3, 3, 0, VER_SUITE_SINGLEUSERTS, VER_NT_WORKSTATION, 30),
        WinXp64 => profile(5, 2, 3790, VER_PLATFORM_WIN32_NT, &CSD_SP2, 2, 0, VER_SUITE_SINGLEUSERTS, VER_NT_WORKSTATION, 0),
        Win2003 => profile(5, 2, 3790, VER_PLATFORM_WIN32_NT, &CSD_SP2, 2, 0, VER_SUITE_SINGLEUSERTS, VER_NT_SERVER, 0),
        Vista => profile(6, 0, 6002, VER_PLATFORM_WIN32_NT, &CSD_SP2, 2, 0, VER_SUITE_SINGLEUSERTS, VER_NT_WORKSTATION, 0),
        Win2008 => profile(6, 0, 6002, VER_PLATFORM_WIN32_NT, &CSD_SP2, 2, 0, VER_SUITE_SINGLEUSERTS, VER_NT_SERVER, 0),
        Win2008R2 => profile(6, 1, 7601, VER_PLATFORM_WIN32_NT, &CSD_SP1, 1, 0, VER_SUITE_SINGLEUSERTS, VER_NT_SERVER, 0),
        Win7 => profile(6, 1, 7601, VER_PLATFORM_WIN32_NT, &CSD_SP1, 1, 0, VER_SUITE_SINGLEUSERTS, VER_NT_WORKSTATION, 0),
        Win8 => profile(6, 2, 9200, VER_PLATFORM_WIN32_NT, &[], 0, 0, VER_SUITE_SINGLEUSERTS, VER_NT_WORKSTATION, 0),
        Win81 => profile(6, 3, 9600, VER_PLATFORM_WIN32_NT, &[], 0, 0, VER_SUITE_SINGLEUSERTS, VER_NT_WORKSTATION, 0),
        Win10 => profile(10, 0, 19045, VER_PLATFORM_WIN32_NT, &[], 0, 0, VER_SUITE_SINGLEUSERTS, VER_NT_WORKSTATION, 0),
        Win11 => profile(10, 0, 22000, VER_PLATFORM_WIN32_NT, &[], 0, 0, VER_SUITE_SINGLEUSERTS, VER_NT_WORKSTATION, 0),
    }
}

/// Resolve version_init precedence: application profile, default profile,
/// then a caller-supplied authoritative registry/OS record.
pub fn select_version_profile(app_alias: Option<&str>, default_alias: Option<&str>,
    registry_version: &RTL_OSVERSIONINFOEXW) -> RTL_OSVERSIONINFOEXW {
    app_alias.and_then(parse_version_alias)
        .or_else(|| default_alias.and_then(parse_version_alias))
        .map(version_profile)
        .unwrap_or(*registry_version)
}

#[cfg(test)]
mod version_profile_tests {
    use super::*;
    #[test]
    fn source_profile_values() {
        let xp = version_profile(WindowsVersionAlias::WinXp);
        assert_eq!((xp.dwMajorVersion, xp.dwMinorVersion, xp.dwBuildNumber), (5, 1, 2600));
        assert_eq!(xp.wServicePackMajor, 3);
        assert_eq!(xp.wSuiteMask, VER_SUITE_SINGLEUSERTS);
        assert_eq!(xp.wProductType, VER_NT_WORKSTATION);
        let server = version_profile(WindowsVersionAlias::Win2008R2);
        assert_eq!((server.dwMajorVersion, server.dwMinorVersion, server.dwBuildNumber), (6, 1, 7601));
        assert_eq!(server.wProductType, VER_NT_SERVER);
        assert_eq!(version_profile(WindowsVersionAlias::Win11).dwBuildNumber, 22000);
    }
    #[test]
    fn app_profile_precedes_default_and_registry_fallback() {
        let registry = RTL_OSVERSIONINFOEXW { dwMajorVersion: 10, dwMinorVersion: 0,
            dwBuildNumber: 26100, dwPlatformId: VER_PLATFORM_WIN32_NT, ..Default::default() };
        assert_eq!(select_version_profile(Some("win7"), Some("win11"), &registry).dwBuildNumber, 7601);
        assert_eq!(select_version_profile(Some("unknown"), Some("win11"), &registry).dwBuildNumber, 22000);
        assert_eq!(select_version_profile(Some("unknown"), Some("invalid"), &registry).dwBuildNumber, 26100);
    }
    #[test]
    fn csd_strings_are_utf16_and_terminated() {
        let xp = version_profile(WindowsVersionAlias::WinXp);
        assert_eq!(&xp.szCSDVersion[..14], &CSD_SP3);
        assert_eq!(xp.szCSDVersion[14], 0);
    }
}


/// Inputs observed by version.c's NT-style registry reader. The caller is
/// responsible for querying the named registry values and checking REG types.
#[derive(Clone, Copy, Debug, Default)]
pub struct NtRegistryVersionValues<'a> {
    pub major_dword: Option<u32>,
    pub minor_dword: Option<u32>,
    pub current_version: Option<&'a str>,
    pub build_number: Option<&'a str>,
    pub csd_version_utf16: Option<&'a [u16]>,
    pub control_csd_version: Option<u32>,
    pub product_type: Option<&'a str>,
}

fn parse_decimal_prefix(text: &str) -> Option<u32> {
    let text = text.trim_start();
    let digits: String = text.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() { None } else { digits.parse().ok() }
}

fn copy_utf16_csd(dst: &mut [u16; 128], source: &[u16]) {
    let count = core::cmp::min(source.iter().position(|c| *c == 0).unwrap_or(source.len()), dst.len() - 1);
    dst[..count].copy_from_slice(&source[..count]);
    dst[count] = 0;
}

/// Decode the value payloads returned by the NT registry queries in
/// get_nt_registry_version(). This does no registry I/O and accepts only
/// already type-checked values from the caller.
pub fn parse_nt_registry_version(values: NtRegistryVersionValues<'_>) -> Option<RTL_OSVERSIONINFOEXW> {
    let pair = match (values.major_dword, values.minor_dword) {
        (Some(major), Some(minor)) => Some((major, minor)),
        _ => values.current_version.and_then(parse_nt_current_version),
    }?;
    if pair.0 == 0 { return None; }

    let mut version = RTL_OSVERSIONINFOEXW {
        dwMajorVersion: pair.0,
        dwMinorVersion: pair.1,
        dwBuildNumber: values.build_number.and_then(parse_decimal_prefix).unwrap_or(0),
        dwPlatformId: VER_PLATFORM_WIN32_NT,
        ..Default::default()
    };
    if let Some(csd) = values.csd_version_utf16 {
        copy_utf16_csd(&mut version.szCSDVersion, csd);
    }
    if let Some(packed) = values.control_csd_version {
        let low = packed as u16;
        version.wServicePackMajor = ((low >> 8) & 0xff) as u16;
        version.wServicePackMinor = (low & 0xff) as u16;
    }
    version.wProductType = match values.product_type {
        Some(s) if s.eq_ignore_ascii_case("WinNT") => VER_NT_WORKSTATION,
        Some(s) if s.eq_ignore_ascii_case("LanmanNT") => VER_NT_DOMAIN_CONTROLLER,
        Some(s) if s.eq_ignore_ascii_case("ServerNT") => VER_NT_SERVER,
        _ => 0,
    };
    Some(version)
}

/// Decode Win9x VersionNumber and SubVersionNumber registry values. The
/// packed build field contains (major, minor) in its high word, as in Wine.
pub fn parse_win9x_registry_version(version_number: &str,
    sub_version_utf16: Option<&[u16]>) -> Option<RTL_OSVERSIONINFOEXW> {
    let (major, minor, build) = parse_win9x_version_number(version_number)?;
    if major == 0 { return None; }
    let mut version = RTL_OSVERSIONINFOEXW {
        dwMajorVersion: major,
        dwMinorVersion: minor,
        dwBuildNumber: build,
        dwPlatformId: VER_PLATFORM_WIN32_WINDOWS,
        ..Default::default()
    };
    if let Some(csd) = sub_version_utf16 {
        copy_utf16_csd(&mut version.szCSDVersion, csd);
    }
    Some(version)
}

#[cfg(test)]
mod registry_payload_tests {
    use super::*;
    #[test]
    fn nt_registry_prefers_complete_dword_pair_then_falls_back_to_string() {
        let v = parse_nt_registry_version(NtRegistryVersionValues {
            major_dword: Some(10), minor_dword: Some(0),
            current_version: Some("6.3.9600"), build_number: Some("26100"),
            ..Default::default()
        }).unwrap();
        assert_eq!((v.dwMajorVersion, v.dwMinorVersion, v.dwBuildNumber), (10, 0, 26100));
        let fallback = parse_nt_registry_version(NtRegistryVersionValues {
            major_dword: Some(10), minor_dword: None,
            current_version: Some("6.3.9600"), ..Default::default()
        }).unwrap();
        assert_eq!((fallback.dwMajorVersion, fallback.dwMinorVersion), (6, 3));
    }

    #[test]
    fn nt_registry_decodes_service_pack_and_product_type() {
        let csd: Vec<u16> = "Service Pack 2".encode_utf16().collect();
        let v = parse_nt_registry_version(NtRegistryVersionValues {
            major_dword: Some(10), minor_dword: Some(0),
            control_csd_version: Some(0x0000_0201),
            csd_version_utf16: Some(&csd), product_type: Some("lanmannt"),
            ..Default::default()
        }).unwrap();
        assert_eq!((v.wServicePackMajor, v.wServicePackMinor), (2, 1));
        assert_eq!(v.wProductType, VER_NT_DOMAIN_CONTROLLER);
        assert_eq!(&v.szCSDVersion[..14], &"Service Pack 2".encode_utf16().collect::<Vec<_>>()[..]);
        assert_eq!(v.szCSDVersion[14], 0);
        assert_eq!(parse_nt_registry_version(NtRegistryVersionValues {
            current_version: Some("0.0"), ..Default::default()
        }), None);
    }

    #[test]
    fn win9x_registry_keeps_packed_build_and_subversion() {
        let sub: Vec<u16> = " C ".encode_utf16().collect();
        let v = parse_win9x_registry_version("4.3.1212", Some(&sub)).unwrap();
        assert_eq!((v.dwMajorVersion, v.dwMinorVersion), (4, 3));
        assert_eq!(v.dwBuildNumber, 0x0403_04bc);
        assert_eq!(v.dwPlatformId, VER_PLATFORM_WIN32_WINDOWS);
        assert_eq!(&v.szCSDVersion[..3], &[32, 67, 32]);
        assert_eq!(v.szCSDVersion[3], 0);
    }

    #[test]
    fn malformed_registry_payloads_do_not_create_versions() {
        assert!(parse_win9x_registry_version("4.x.1", None).is_none());
        assert!(parse_nt_registry_version(NtRegistryVersionValues {
            current_version: Some("bad"), ..Default::default()
        }).is_none());
    }
}
