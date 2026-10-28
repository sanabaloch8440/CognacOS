//! RTL time arithmetic and architecture-independent utility routines.
const TPS:i64=10_000_000;
const E1970:i64=116_444_736_000_000_000;
const E1980:i64=119_600_064_000_000_000;
use core::sync::atomic::{AtomicU32, AtomicU8, Ordering};
static ACTIVE_CONSOLE_ID: AtomicU32 = AtomicU32::new(u32::MAX);
static SESSION_SKU_FLAGS: AtomicU8 = AtomicU8::new(0);
pub fn configure_session_sku_flags(multi_session: bool, multi_users: bool) { SESSION_SKU_FLAGS.store((multi_session as u8) | ((multi_users as u8) << 1), Ordering::Release); }
#[repr(C)] pub struct RtlTimeFields { pub year:i16,pub month:i16,pub day:i16,pub hour:i16,pub minute:i16,pub second:i16,pub milliseconds:i16,pub weekday:i16 }
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_seconds_since_1970_to_time(s:u32,t:*mut i64){if !t.is_null(){unsafe{*t=E1970+(s as i64)*TPS;}}}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_seconds_since_1980_to_time(s:u32,t:*mut i64){if !t.is_null(){unsafe{*t=E1980+(s as i64)*TPS;}}}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_time_to_seconds_since_1970(t:*const i64,s:*mut u32)->u8{if t.is_null()||s.is_null(){return 0;}let v=unsafe{*t};if v<E1970{return 0;}let n=((v-E1970)/TPS) as u64;if n>u32::MAX as u64{return 0;}unsafe{*s=n as u32;}1}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_time_to_seconds_since_1980(t:*const i64,s:*mut u32)->u8{if t.is_null()||s.is_null(){return 0;}let v=unsafe{*t};if v<E1980{return 0;}let n=((v-E1980)/TPS) as u64;if n>u32::MAX as u64{return 0;}unsafe{*s=n as u32;}1}
fn leap(y:i32)->bool{y%4==0&&(y%100!=0||y%400==0)}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_time_to_elapsed_time_fields(t:*const i64,f:*mut RtlTimeFields){if t.is_null()||f.is_null(){return;}let mut v=unsafe{*t}.max(0);let days=v/864_000_000_000;v%=864_000_000_000;unsafe{*f=RtlTimeFields{year:(days/365) as i16,month:0,day:(days%365) as i16,hour:(v/36_000_000_000) as i16,minute:((v/600_000_000)%60) as i16,second:((v/TPS)%60) as i16,milliseconds:((v%TPS)/10_000) as i16,weekday:0};}}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_time_fields_to_time(f:*const RtlTimeFields,t:*mut i64)->u8{if f.is_null()||t.is_null(){return 0;}let x=unsafe{&*f};let y=x.year as i32;let m=x.month as usize;let d=x.day as i32;if y<1601||!(1..=12).contains(&m)||x.hour<0||x.hour>23||x.minute<0||x.minute>59||x.second<0||x.second>59||x.milliseconds<0||x.milliseconds>999{return 0;}let md=[31,if leap(y){29}else{28},31,30,31,30,31,31,30,31,30,31];if d<1||d>md[m-1]{return 0;}let mut days=0i64;for yr in 1601..y{days+=if leap(yr){366}else{365};}for mo in 1..m{days+=md[mo-1] as i64;}days+=d as i64-1;unsafe{*t=days*864_000_000_000+(x.hour as i64)*36_000_000_000+(x.minute as i64)*600_000_000+(x.second as i64)*TPS+(x.milliseconds as i64)*10_000;}1}
fn civil(days:i64)->(i32,i32,i32){let z=days+719468;let era=if z>=0{z}else{z-146096}/146097;let doe=z-era*146097;let yoe=(doe-doe/1460+doe/36524-doe/146096)/365;let mut y=yoe as i32+era as i32*400;let doy=doe-(365*yoe+yoe/4-yoe/100);let mp=(5*doy+2)/153;let d=(doy-(153*mp+2)/5+1) as i32;let m=(mp+if mp<10{3}else{-9}) as i32;if m<=2{y+=1;}(y,m,d)}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_time_to_time_fields(t:*const i64,f:*mut RtlTimeFields){if t.is_null()||f.is_null(){return;}let mut v=unsafe{*t};if v<0{return;}let days=v/864_000_000_000;v%=864_000_000_000;let(y,m,d)=civil(days-134774);unsafe{*f=RtlTimeFields{year:y as i16,month:m as i16,day:d as i16,hour:(v/36_000_000_000) as i16,minute:((v/600_000_000)%60) as i16,second:((v/TPS)%60) as i16,milliseconds:((v%TPS)/10_000) as i16,weekday:((days+1)%7) as i16};}}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_get_nt_product_type(out:*mut u32)->u8{if out.is_null(){return 0;}unsafe{*out=1;}1}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_get_nt_global_flags()->u32{0}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_get_suite_mask()->u32{0}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_get_active_console_id()->u32{ACTIVE_CONSOLE_ID.load(Ordering::Acquire)}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_set_active_console_id(id:u32){ACTIVE_CONSOLE_ID.store(id,Ordering::Release);}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_get_current_service_session_id()->u32{crate::process::session_id_for_pid(crate::thread::current_process_id()).unwrap_or(0)}
#[unsafe(no_mangle)]
pub unsafe extern "win64" fn rtl_is_processor_feature_present(feature: u32) -> u8 {
    #[cfg(target_arch = "x86_64")]
    {
        use core::arch::x86_64::{__cpuid, __cpuid_count};
        let basic_max = unsafe { __cpuid(0).eax };
        let leaf1 = if basic_max >= 1 { unsafe { __cpuid(1) } } else { unsafe { __cpuid(0) } };
        let ext_max = unsafe { __cpuid(0x8000_0000).eax };
        let ext1 = if ext_max >= 0x8000_0001 { unsafe { __cpuid(0x8000_0001) } } else { unsafe { __cpuid(0) } };
        let leaf7 = if basic_max >= 7 { unsafe { __cpuid_count(7, 0) } } else { unsafe { __cpuid(0) } };
        let has = match feature {
            2 => true, // CMPXCHG8B is mandatory on supported x86-64 CPUs.
            3 => leaf1.edx & (1 << 23) != 0, // MMX
            6 => leaf1.edx & (1 << 25) != 0, // SSE
            8 => leaf1.edx & (1 << 4) != 0, // RDTSC
            9 => leaf1.edx & (1 << 6) != 0, // PAE
            10 => leaf1.edx & (1 << 26) != 0, // SSE2
            12 => ext1.edx & (1 << 20) != 0, // NX
            13 => leaf1.ecx & (1 << 0) != 0, // SSE3
            14 => leaf1.ecx & (1 << 13) != 0, // CMPXCHG16B
            17 => leaf1.ecx & (1 << 26) != 0 && leaf1.ecx & (1 << 27) != 0, // XSAVE and OSXSAVE
            22 => leaf7.ebx & 1 != 0, // FSGSBASE
            28 => leaf1.ecx & (1 << 30) != 0, // RDRAND
            32 => ext1.edx & (1 << 27) != 0, // RDTSCP
            36 => leaf1.ecx & (1 << 9) != 0, // SSSE3
            37 => leaf1.ecx & (1 << 19) != 0, // SSE4.1
            38 => leaf1.ecx & (1 << 20) != 0, // SSE4.2
            50 => leaf7.ebx & (1 << 8) != 0, // BMI2
            // AVX-family flags require XCR0 state validation before advertising them.
            39 | 40 | 41 => false,
            _ => false,
        };
        has as u8
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = feature;
        0
    }
}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_is_multi_session_sku()->u8{((SESSION_SKU_FLAGS.load(Ordering::Acquire)&1!=0)||crate::process::has_multiple_sessions()) as u8}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_is_multi_users_in_session_sku()->u8{(SESSION_SKU_FLAGS.load(Ordering::Acquire)&2!=0) as u8}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_is_elevated_rid(rid:u32)->u8{(rid==0x3000) as u8}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_is_nt_ddi_version_available(version:u32)->u8{(version<=0x0A000000) as u8}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_is_service_pack_version_installed(_major:u16,_minor:u16,_flags:u32)->u8{0}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_is_state_separation_enabled()->u8{0}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_check_portable_operating_system(out:*mut u8)->i32{if out.is_null(){return 0xC000000Du32 as i32;}unsafe{*out=super::abi_extra::PORTABLE.load(core::sync::atomic::Ordering::Acquire);}0}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_is_valid_oem_character(c:u16)->u8{(c>0&&c<=255) as u8}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_nt_status_to_dos_error(s:u32)->u32{match s{0=>0,0xC0000005=>998,0xC0000008=>6,0xC000000D=>87,0xC000000F=>2,0xC0000017=>8,0xC0000022=>5,0xC0000034=>2,0xC0000035=>183,0xC000003A=>3,0xC000009A=>8,0xC00000BB=>50,0xC0000002=>120,_=>317}}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_nt_status_to_dos_error_no_teb(s:u32)->u32{unsafe{rtl_nt_status_to_dos_error(s)}}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_number_of_set_bits_ulong_ptr(v:usize)->u32{v.count_ones()}
#[unsafe(no_mangle)] pub unsafe extern "win64" fn rtl_udiv128(high:u64,low:u64,divisor:u64,remainder:*mut u64)->u64{if divisor==0{if !remainder.is_null(){unsafe{*remainder=0;}}return u64::MAX;}let n=((high as u128)<<64)|(low as u128);let q=n/(divisor as u128);if !remainder.is_null(){unsafe{*remainder=(n%(divisor as u128)) as u64;}}q as u64}
