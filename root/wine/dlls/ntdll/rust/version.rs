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
