//! Optional firmware-security discovery and TPM 2.0 measured-boot support.
//! Absence of Secure Boot or a TPM never blocks boot; signature verification of
//! the kernel is independent and remains mandatory.
use uefi::{boot, cstr16, runtime::VariableVendor, Status};

pub const SECURE_BOOT_KNOWN: u64 = 1 << 0;
pub const SECURE_BOOT_ENABLED: u64 = 1 << 1;
pub const SETUP_MODE_KNOWN: u64 = 1 << 2;
pub const SETUP_MODE_ENABLED: u64 = 1 << 3;
pub const TPM2_PRESENT: u64 = 1 << 4;
pub const KERNEL_MEASURED: u64 = 1 << 5;
pub const KERNEL_SIGNATURE_VERIFIED: u64 = 1 << 6;

const EV_EFI_BOOT_SERVICES_APPLICATION: u32 = 0x8000_0003;
const PCR_BOOT_MANAGER_CODE: u32 = 4;
const TCG2_EVENT_HEADER_SIZE: usize = 14;

#[repr(C, packed)]
struct Tcg2BootServiceCapability {
    size: u8,
    structure_version_major: u8,
    structure_version_minor: u8,
    protocol_version_major: u8,
    protocol_version_minor: u8,
    hash_algorithm_bitmap: u32,
    supported_event_logs: u32,
    tpm_present_flag: u8,
    max_command_size: u16,
    max_response_size: u16,
    manufacturer_id: u32,
    number_of_pcr_banks: u32,
    active_pcr_banks: u32,
}

type GetCapability = unsafe extern "efiapi" fn(
    this: *mut Tcg2Protocol,
    capability: *mut Tcg2BootServiceCapability,
) -> Status;
type HashLogExtendEvent = unsafe extern "efiapi" fn(
    this: *mut Tcg2Protocol,
    flags: u64,
    data_to_hash: *const u8,
    data_to_hash_len: usize,
    event: *mut u8,
) -> Status;

#[repr(C)]
#[uefi::proto::unsafe_protocol("607f766c-7455-42be-930b-e4d76db2720f")]
struct Tcg2Protocol {
    get_capability: Option<GetCapability>,
    get_event_log: usize,
    hash_log_extend_event: Option<HashLogExtendEvent>,
    submit_command: usize,
    get_active_pcr_banks: usize,
    set_active_pcr_banks: usize,
    get_result_of_set_active_pcr_banks: usize,
}

fn read_boolean_variable(name: &'static uefi::CStr16) -> Option<bool> {
    let mut value = [0u8; 1];
    let (bytes, _) = uefi::runtime::get_variable(name, &VariableVendor::GLOBAL, &mut value).ok()?;
    (bytes.len() == 1).then_some(bytes[0] != 0)
}

/// Read firmware state before ExitBootServices. Unknown is deliberately distinct
/// from disabled so missing/broken firmware variables are not misreported.
pub fn firmware_security_status() -> u64 {
    let mut flags = 0;
    if let Some(enabled) = read_boolean_variable(cstr16!("SecureBoot")) {
        flags |= SECURE_BOOT_KNOWN;
        if enabled { flags |= SECURE_BOOT_ENABLED; }
    }
    if let Some(setup) = read_boolean_variable(cstr16!("SetupMode")) {
        flags |= SETUP_MODE_KNOWN;
        if setup { flags |= SETUP_MODE_ENABLED; }
    }
    flags
}

#[repr(C, align(8))]
struct EventBuffer([u8; 4 + TCG2_EVENT_HEADER_SIZE + 64]);

/// Ask the firmware TCG2 protocol to hash the exact bytes that were verified and
/// extend PCR 4. The firmware maintains the platform-standard event log/banks.
pub fn measure_kernel(image: &[u8]) -> (bool, bool) {
    let handle = match boot::get_handle_for_protocol::<Tcg2Protocol>() {
        Ok(handle) => handle,
        Err(_) => return (false, false),
    };
    let mut protocol = match boot::open_protocol_exclusive::<Tcg2Protocol>(handle) {
        Ok(protocol) => protocol,
        Err(_) => return (false, false),
    };
    let mut capability = Tcg2BootServiceCapability {
        size: core::mem::size_of::<Tcg2BootServiceCapability>() as u8,
        structure_version_major: 1, structure_version_minor: 0,
        protocol_version_major: 1, protocol_version_minor: 0,
        hash_algorithm_bitmap: 0, supported_event_logs: 0, tpm_present_flag: 0,
        max_command_size: 0, max_response_size: 0, manufacturer_id: 0,
        number_of_pcr_banks: 0, active_pcr_banks: 0,
    };
    let Some(get_capability) = protocol.get_capability else { return (false, false); };
    // SAFETY: protocol is open and capability is a writable UEFI TCG2 structure.
    if unsafe { get_capability(&mut *protocol, &mut capability) } != Status::SUCCESS {
        return (false, false);
    }
    if capability.tpm_present_flag == 0 { return (false, false); }
    let Some(hash_log_extend_event) = protocol.hash_log_extend_event else { return (true, false); };

    let description = b"CognacOS verified kernel PE32+ image";
    let total = 4 + TCG2_EVENT_HEADER_SIZE + description.len();
    let mut event = EventBuffer([0; 4 + TCG2_EVENT_HEADER_SIZE + 64]);
    event.0[0..4].copy_from_slice(&(total as u32).to_le_bytes());
    event.0[4..8].copy_from_slice(&(TCG2_EVENT_HEADER_SIZE as u32).to_le_bytes());
    event.0[8..10].copy_from_slice(&1u16.to_le_bytes()); // EFI_TCG2_EVENT_HEADER_VERSION
    event.0[10..14].copy_from_slice(&PCR_BOOT_MANAGER_CODE.to_le_bytes());
    event.0[14..18].copy_from_slice(&EV_EFI_BOOT_SERVICES_APPLICATION.to_le_bytes());
    event.0[18..18 + description.len()].copy_from_slice(description);

    // SAFETY: both buffers remain live for the firmware call and the event is
    // aligned, sized, and formatted per EFI_TCG2_EVENT.
    let status = unsafe {
        hash_log_extend_event(&mut *protocol, 0, image.as_ptr(), image.len(), event.0.as_mut_ptr())
    };
    (true, status == Status::SUCCESS)
}
