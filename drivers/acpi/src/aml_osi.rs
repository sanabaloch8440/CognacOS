//! Windows-compatible ACPI Operating System Interfaces (_OSI) policy.
//!
//! These are ACPI interface-level strings, not Windows version macros and not
//! an architecture/CPU identifier. The profile intentionally advertises the
//! documented Windows 11 interfaces while retaining the older Windows levels
//! that Windows 11 firmware commonly probes.

#![no_std]

/// Version-interface strings documented by Microsoft for Windows ACPI.
/// A newer Windows release reports support for earlier Windows interface
/// levels as well as its own level.
const WINDOWS_LEVELS: &[&[u8]] = &[
    b"Windows 2000",
    b"Windows 2001",
    b"Windows 2001 SP1",
    b"Windows 2001.1",
    b"Windows 2001 SP2",
    b"Windows 2001.1 SP1",
    b"Windows 2006",
    b"Windows 2006 SP1",
    b"Windows 2006.1",
    b"Windows 2009",
    b"Windows 2012",
    b"Windows 2013",
    b"Windows 2015",
    b"Windows 2016",
    b"Windows 2017",
    b"Windows 2017.2",
    b"Windows 2018",
    b"Windows 2018.2",
    b"Windows 2019",
    b"Windows 2020",
    b"Windows 2021",
    b"Windows 2022",
];

/// Return the ACPI _OSI result for CognacOS's Windows 11 x64 compatibility
/// profile. ACPI defines TRUE as all bits set (0xFFFF_FFFF), not integer 1.
/// Unknown OS-version strings and optional feature-group strings return zero.
pub const fn windows_11_x64_osi(argument: &[u8]) -> u64 {
    let mut index = 0;
    while index < WINDOWS_LEVELS.len() {
        let candidate = WINDOWS_LEVELS[index];
        if candidate.len() == argument.len() {
            let mut byte = 0;
            let mut equal = true;
            while byte < candidate.len() {
                if candidate[byte] != argument[byte] {
                    equal = false;
                    break;
                }
                byte += 1;
            }
            if equal {
                return 0xFFFF_FFFF;
            }
        }
        index += 1;
    }
    0
}

#[cfg(test)]
mod tests {
    use super::windows_11_x64_osi;

    #[test]
    fn advertises_windows_11_and_22h2_interface_levels() {
        assert_eq!(windows_11_x64_osi(b"Windows 2021"), 0xFFFF_FFFF);
        assert_eq!(windows_11_x64_osi(b"Windows 2022"), 0xFFFF_FFFF);
    }

    #[test]
    fn preserves_older_windows_firmware_compatibility_levels() {
        for level in [b"Windows 2009".as_slice(), b"Windows 2012", b"Windows 2015", b"Windows 2020"] {
            assert_eq!(windows_11_x64_osi(level), 0xFFFF_FFFF);
        }
    }

    #[test]
    fn rejects_unknown_versions_and_unimplemented_feature_groups() {
        assert_eq!(windows_11_x64_osi(b"Windows 2023"), 0);
        assert_eq!(windows_11_x64_osi(b"Linux"), 0);
        assert_eq!(windows_11_x64_osi(b"Extended Address Space Descriptor"), 0);
    }
}

/// Evaluate _OSI with the immutable DMI-selected platform policy layered over
/// the Windows 11 compatibility profile. A quirk may suppress an advertised
/// interface or explicitly advertise one; all other strings retain defaults.
pub fn windows_11_x64_osi_with_quirks(
    argument: &[u8],
    policy: &crate::quirks::QuirkPolicy,
) -> u64 {
    if let Ok(name) = core::str::from_utf8(argument) {
        if !policy.osi_is_advertised(name, windows_11_x64_osi(argument) != 0) {
            return 0;
        }
        if policy.osi_is_advertised(name, false) {
            return 0xFFFF_FFFF;
        }
    }
    windows_11_x64_osi(argument)
}


/// Read-only platform policy published by the kernel through the versioned
/// HAL ACPI service contract. The pointer remains valid for the kernel lifetime.
static PLATFORM_QUIRK_POLICY: core::sync::atomic::AtomicPtr<crate::quirks::QuirkPolicy> =
    core::sync::atomic::AtomicPtr::new(core::ptr::null_mut());

/// Install a kernel-owned immutable policy after validating its ABI version
/// and extent. Passing a null pointer clears the optional policy.
pub unsafe fn install_platform_quirk_policy(
    policy: *const core::ffi::c_void,
    size: u32,
    version: u32,
) -> bool {
    if policy.is_null() {
        PLATFORM_QUIRK_POLICY.store(core::ptr::null_mut(), core::sync::atomic::Ordering::Release);
        return true;
    }
    if version != cognacos_hal_contract::ACPI_PLATFORM_QUIRK_POLICY_VERSION
        || size as usize < core::mem::size_of::<crate::quirks::QuirkPolicy>()
        || (policy as usize) % core::mem::align_of::<crate::quirks::QuirkPolicy>() != 0
    {
        return false;
    }
    PLATFORM_QUIRK_POLICY.store(policy.cast_mut().cast(), core::sync::atomic::Ordering::Release);
    true
}

/// The interpreter's predefined _OSI object consults the immutable platform
/// policy when installed; otherwise it preserves the normal Windows 11 profile.
/// Borrow the currently installed immutable platform policy, if the
/// HAL has published one for this boot.
pub fn platform_quirk_policy() -> Option<&'static crate::quirks::QuirkPolicy> {
    let policy = PLATFORM_QUIRK_POLICY.load(core::sync::atomic::Ordering::Acquire);
    if policy.is_null() { None } else { Some(unsafe { &*policy }) }
}

pub fn platform_osi(argument: &[u8]) -> u64 {
    let policy = PLATFORM_QUIRK_POLICY.load(core::sync::atomic::Ordering::Acquire);
    if policy.is_null() {
        windows_11_x64_osi(argument)
    } else {
        windows_11_x64_osi_with_quirks(argument, unsafe { &*policy })
    }
}
