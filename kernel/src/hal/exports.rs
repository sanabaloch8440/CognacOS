//! Windows 11 x64 HAL export compatibility catalogue.
//!
//! The modern Windows x64 HAL forwards these exports to ntoskrnl. This is a
//! name/ordinal inventory only: it does not claim Windows-compatible semantics.
//! No export is callable until its exact ABI and behavior are implemented;
//! generic function pointers would risk corrupting driver calls.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StubState { NotImplemented, Implemented }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HalExport { pub ordinal: u16, pub name: &'static str, pub state: StubState }

/// Windows 11 x64 10.0.26100 HAL exports, in ordinal order.
pub const EXPORTS: &[HalExport] = &[
    HalExport { ordinal: 1, name: "HalAcpiGetTableEx", state: StubState::Implemented },
    HalExport { ordinal: 2, name: "HalAcquireDisplayOwnership", state: StubState::NotImplemented },
    HalExport { ordinal: 3, name: "HalAdjustResourceList", state: StubState::NotImplemented },
    HalExport { ordinal: 4, name: "HalAllProcessorsStarted", state: StubState::Implemented },
    HalExport { ordinal: 5, name: "HalAllocateAdapterChannel", state: StubState::NotImplemented },
    HalExport { ordinal: 6, name: "HalAllocateCommonBuffer", state: StubState::Implemented },
    HalExport { ordinal: 7, name: "HalAllocateCrashDumpRegisters", state: StubState::NotImplemented },
    HalExport { ordinal: 8, name: "HalAllocateHardwareCounters", state: StubState::NotImplemented },
    HalExport { ordinal: 9, name: "HalAssignSlotResources", state: StubState::NotImplemented },
    HalExport { ordinal: 10, name: "HalBugCheckSystem", state: StubState::NotImplemented },
    HalExport { ordinal: 11, name: "HalCalibratePerformanceCounter", state: StubState::NotImplemented },
    HalExport { ordinal: 12, name: "HalClearSoftwareInterrupt", state: StubState::NotImplemented },
    HalExport { ordinal: 13, name: "HalConvertDeviceIdtToIrql", state: StubState::NotImplemented },
    HalExport { ordinal: 14, name: "HalDisableInterrupt", state: StubState::NotImplemented },
    HalExport { ordinal: 15, name: "HalDisplayString", state: StubState::Implemented },
    HalExport { ordinal: 16, name: "HalDmaAllocateCrashDumpRegistersEx", state: StubState::NotImplemented },
    HalExport { ordinal: 17, name: "HalDmaFreeCrashDumpRegistersEx", state: StubState::NotImplemented },
    HalExport { ordinal: 18, name: "HalEnableInterrupt", state: StubState::NotImplemented },
    HalExport { ordinal: 19, name: "HalEnumerateEnvironmentVariablesEx", state: StubState::NotImplemented },
    HalExport { ordinal: 20, name: "HalEnumerateProcessors", state: StubState::NotImplemented },
    HalExport { ordinal: 21, name: "HalFlushCommonBuffer", state: StubState::Implemented },
    HalExport { ordinal: 22, name: "HalFreeCommonBuffer", state: StubState::Implemented },
    HalExport { ordinal: 23, name: "HalFreeHardwareCounters", state: StubState::NotImplemented },
    HalExport { ordinal: 24, name: "HalGetAdapter", state: StubState::Implemented },
    HalExport { ordinal: 25, name: "HalGetBusData", state: StubState::Implemented },
    HalExport { ordinal: 26, name: "HalGetBusDataByOffset", state: StubState::Implemented },
    HalExport { ordinal: 27, name: "HalGetEnvironmentVariable", state: StubState::NotImplemented },
    HalExport { ordinal: 28, name: "HalGetEnvironmentVariableEx", state: StubState::NotImplemented },
    HalExport { ordinal: 29, name: "HalGetInterruptTargetInformation", state: StubState::NotImplemented },
    HalExport { ordinal: 30, name: "HalGetInterruptVector", state: StubState::NotImplemented },
    HalExport { ordinal: 31, name: "HalGetMemoryCachingRequirements", state: StubState::NotImplemented },
    HalExport { ordinal: 32, name: "HalGetMessageRoutingInfo", state: StubState::NotImplemented },
    HalExport { ordinal: 33, name: "HalGetProcessorIdByNtNumber", state: StubState::Implemented },
    HalExport { ordinal: 34, name: "HalGetVectorInput", state: StubState::NotImplemented },
    HalExport { ordinal: 35, name: "HalHandleMcheck", state: StubState::NotImplemented },
    HalExport { ordinal: 36, name: "HalHandleNMI", state: StubState::NotImplemented },
    HalExport { ordinal: 37, name: "HalInitSystem", state: StubState::NotImplemented },
    HalExport { ordinal: 38, name: "HalInitializeBios", state: StubState::NotImplemented },
    HalExport { ordinal: 39, name: "HalInitializeOnResume", state: StubState::NotImplemented },
    HalExport { ordinal: 40, name: "HalInitializeProcessor", state: StubState::NotImplemented },
    HalExport { ordinal: 41, name: "HalIsHyperThreadingEnabled", state: StubState::Implemented },
    HalExport { ordinal: 42, name: "HalMakeBeep", state: StubState::NotImplemented },
    HalExport { ordinal: 43, name: "HalPerformEndOfInterrupt", state: StubState::NotImplemented },
    HalExport { ordinal: 44, name: "HalProcessorIdle", state: StubState::NotImplemented },
    HalExport { ordinal: 45, name: "HalQueryDisplayParameters", state: StubState::NotImplemented },
    HalExport { ordinal: 46, name: "HalQueryEnvironmentVariableInfoEx", state: StubState::NotImplemented },
    HalExport { ordinal: 47, name: "HalQueryMaximumProcessorCount", state: StubState::Implemented },
    HalExport { ordinal: 48, name: "HalQueryRealTimeClock", state: StubState::Implemented },
    HalExport { ordinal: 49, name: "HalReadDmaCounter", state: StubState::NotImplemented },
    HalExport { ordinal: 50, name: "HalRegisterDynamicProcessor", state: StubState::NotImplemented },
    HalExport { ordinal: 51, name: "HalRegisterErrataCallbacks", state: StubState::NotImplemented },
    HalExport { ordinal: 52, name: "HalReportResourceUsage", state: StubState::NotImplemented },
    HalExport { ordinal: 53, name: "HalRequestClockInterrupt", state: StubState::NotImplemented },
    HalExport { ordinal: 54, name: "HalRequestDeferredRecoveryServiceInterrupt", state: StubState::NotImplemented },
    HalExport { ordinal: 55, name: "HalRequestIpi", state: StubState::Implemented },
    HalExport { ordinal: 56, name: "HalRequestIpiSpecifyVector", state: StubState::Implemented },
    HalExport { ordinal: 57, name: "HalRequestSoftwareInterrupt", state: StubState::NotImplemented },
    HalExport { ordinal: 58, name: "HalReturnToFirmware", state: StubState::NotImplemented },
    HalExport { ordinal: 59, name: "HalSendNMI", state: StubState::NotImplemented },
    HalExport { ordinal: 60, name: "HalSendSoftwareInterrupt", state: StubState::NotImplemented },
    HalExport { ordinal: 61, name: "HalSetBusData", state: StubState::Implemented },
    HalExport { ordinal: 62, name: "HalSetBusDataByOffset", state: StubState::Implemented },
    HalExport { ordinal: 63, name: "HalSetDisplayParameters", state: StubState::NotImplemented },
    HalExport { ordinal: 64, name: "HalSetEnvironmentVariable", state: StubState::NotImplemented },
    HalExport { ordinal: 65, name: "HalSetEnvironmentVariableEx", state: StubState::NotImplemented },
    HalExport { ordinal: 66, name: "HalSetProfileInterval", state: StubState::NotImplemented },
    HalExport { ordinal: 67, name: "HalSetRealTimeClock", state: StubState::NotImplemented },
    HalExport { ordinal: 68, name: "HalStartDynamicProcessor", state: StubState::NotImplemented },
    HalExport { ordinal: 69, name: "HalStartNextProcessor", state: StubState::NotImplemented },
    HalExport { ordinal: 70, name: "HalStartProfileInterrupt", state: StubState::NotImplemented },
    HalExport { ordinal: 71, name: "HalStopProfileInterrupt", state: StubState::NotImplemented },
    HalExport { ordinal: 72, name: "HalSystemVectorDispatchEntry", state: StubState::NotImplemented },
    HalExport { ordinal: 73, name: "HalTranslateBusAddress", state: StubState::Implemented },
    HalExport { ordinal: 74, name: "HalWheaUpdateCmciPolicy", state: StubState::NotImplemented },
    HalExport { ordinal: 75, name: "IoFlushAdapterBuffers", state: StubState::NotImplemented },
    HalExport { ordinal: 76, name: "IoFreeAdapterChannel", state: StubState::NotImplemented },
    HalExport { ordinal: 77, name: "IoFreeMapRegisters", state: StubState::NotImplemented },
    HalExport { ordinal: 78, name: "IoMapTransfer", state: StubState::NotImplemented },
    HalExport { ordinal: 79, name: "IoReadPartitionTable", state: StubState::NotImplemented },
    HalExport { ordinal: 80, name: "IoSetPartitionInformation", state: StubState::NotImplemented },
    HalExport { ordinal: 81, name: "IoWritePartitionTable", state: StubState::NotImplemented },
    HalExport { ordinal: 82, name: "KdComPortInUse", state: StubState::NotImplemented },
    HalExport { ordinal: 83, name: "KdHvComPortInUse", state: StubState::NotImplemented },
    HalExport { ordinal: 84, name: "KeFlushWriteBuffer", state: StubState::Implemented },
    HalExport { ordinal: 85, name: "KeQueryPerformanceCounter", state: StubState::Implemented },
    HalExport { ordinal: 86, name: "KeStallExecutionProcessor", state: StubState::Implemented },
    HalExport { ordinal: 87, name: "x86BiosAllocateBuffer", state: StubState::NotImplemented },
    HalExport { ordinal: 88, name: "x86BiosCall", state: StubState::NotImplemented },
    HalExport { ordinal: 89, name: "x86BiosFreeBuffer", state: StubState::NotImplemented },
    HalExport { ordinal: 90, name: "x86BiosReadMemory", state: StubState::NotImplemented },
    HalExport { ordinal: 91, name: "x86BiosWriteMemory", state: StubState::NotImplemented },
];


const PCI_CONFIG_ADDRESS: u16 = 0x0cf8;
const PCI_CONFIG_DATA: u16 = 0x0cfc;
const PCI_CONFIGURATION_BUS_DATA_TYPE: u32 = 5;
static PCI_CONFIG_LOCK: crate::hal::IrqSpinLock<()> = crate::hal::IrqSpinLock::new(());

#[inline]
fn pci_config_address(bus: u32, slot: u32, offset: u32) -> Option<u32> {
    if bus > 255 || slot > 255 || offset > 255 { return None; }
    // PCI_SLOT_NUMBER packs device in bits 0..4 and function in bits 5..7.
    // Mechanism #1 places device in config-address bits 11..15 and function
    // in bits 8..10; shifting the packed slot byte directly is incorrect.
    let device = slot & 0x1f;
    let function = (slot >> 5) & 0x07;
    Some(0x8000_0000 | (bus << 16) | (device << 11) | (function << 8) | (offset & !3))
}

unsafe fn pci_read_dword(bus: u32, slot: u32, offset: u32) -> Option<u32> {
    let address = pci_config_address(bus, slot, offset)?;
    unsafe {
        crate::hal::write_port_u32(PCI_CONFIG_ADDRESS, address);
        Some(crate::hal::read_port_u32(PCI_CONFIG_DATA))
    }
}

/// Read PCI configuration space using legacy mechanism #1. Only PCI config
/// bus data (type 5), bus 0..255, device/function slot 0..255 and offsets
/// within the conventional 256-byte header are supported.
pub unsafe extern "win64" fn hal_get_bus_data_by_offset(
    bus_data_type: u32, bus_number: u32, slot_number: u32,
    buffer: *mut u8, offset: u32, length: u32,
) -> u32 {
    if bus_data_type != PCI_CONFIGURATION_BUS_DATA_TYPE || buffer.is_null()
        || length == 0 || offset >= 256 || slot_number > 255 || bus_number > 255 {
        return 0;
    }
    let count = length.min(256 - offset);
    let _guard = PCI_CONFIG_LOCK.lock();
    let vendor = match unsafe { pci_read_dword(bus_number, slot_number, 0) } {
        Some(value) => value & 0xffff,
        None => return 0,
    };
    if vendor == 0xffff { return 0; }
    for index in 0..count {
        let at = offset + index;
        let value = match unsafe { pci_read_dword(bus_number, slot_number, at) } {
            Some(value) => (value >> ((at & 3) * 8)) as u8,
            None => break,
        };
        unsafe { buffer.add(index as usize).write(value); }
    }
    count
}

/// Conservative byte-level write policy for legacy PCI configuration access.
/// The command register is needed by ordinary bus drivers to enable memory/I/O
/// decoding and bus mastering. Its adjacent status register is write-one-to-clear,
/// so bytes 0x06..0x07 must never be included in a read/modify/write. Extended
/// capability bytes are a legacy best-effort path; drivers should prefer the PCI
/// bus interface, which can apply device-specific policy.
#[inline]
fn pci_config_byte_writable(offset: u32) -> bool {
    offset == 0x04 || offset == 0x05 || offset >= 0x40
}

/// Write PCI configuration space using legacy mechanism #1. Writes to the
/// vendor/device ID, BARs, and status register are rejected. Returns the number
/// of bytes actually written, which can be smaller than the requested length.
pub unsafe extern "win64" fn hal_set_bus_data_by_offset(
    bus_data_type: u32, bus_number: u32, slot_number: u32,
    buffer: *const u8, offset: u32, length: u32,
) -> u32 {
    if bus_data_type != PCI_CONFIGURATION_BUS_DATA_TYPE || buffer.is_null()
        || length == 0 || offset >= 256 || slot_number > 255 || bus_number > 255 {
        return 0;
    }
    let count = length.min(256 - offset);
    let _guard = PCI_CONFIG_LOCK.lock();
    let vendor = match unsafe { pci_read_dword(bus_number, slot_number, 0) } {
        Some(value) => value & 0xffff,
        None => return 0,
    };
    if vendor == 0xffff { return 0; }
    let mut written = 0u32;
    for index in 0..count {
        let at = offset + index;
        if !pci_config_byte_writable(at) { continue; }
        let address = match pci_config_address(bus_number, slot_number, at) {
            Some(value) => value,
            None => break,
        };
        let shift = (at & 3) * 8;
        unsafe {
            crate::hal::write_port_u32(PCI_CONFIG_ADDRESS, address);
            let old = crate::hal::read_port_u32(PCI_CONFIG_DATA);
            let byte = buffer.add(index as usize).read() as u32;
            let updated = (old & !(0xff << shift)) | (byte << shift);
            crate::hal::write_port_u32(PCI_CONFIG_DATA, updated);
        }
        written += 1;
    }
    written
}

pub unsafe extern "win64" fn hal_get_bus_data(
    bus_data_type: u32, bus_number: u32, slot_number: u32,
    buffer: *mut u8, length: u32,
) -> u32 {
    unsafe { hal_get_bus_data_by_offset(bus_data_type, bus_number, slot_number, buffer, 0, length) }
}

pub unsafe extern "win64" fn hal_set_bus_data(
    bus_data_type: u32, bus_number: u32, slot_number: u32,
    buffer: *const u8, length: u32,
) -> u32 {
    unsafe { hal_set_bus_data_by_offset(bus_data_type, bus_number, slot_number, buffer, 0, length) }
}




/// Windows TIME_FIELDS-compatible structure used by the legacy RTC HAL API.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct HalTimeFields {
    pub year: i16,
    pub month: i16,
    pub day: i16,
    pub hour: i16,
    pub minute: i16,
    pub second: i16,
    pub milliseconds: i16,
    pub weekday: i16,
}

static CMOS_RTC_LOCK: crate::hal::IrqSpinLock<()> = crate::hal::IrqSpinLock::new(());

unsafe fn cmos_read(register: u8) -> u8 {
    unsafe {
        crate::hal::write_port_u8(0x70, register & 0x7f);
        crate::hal::read_port_u8(0x71)
    }
}

fn rtc_decode(value: u8, binary_mode: bool) -> Option<u8> {
    if binary_mode { return Some(value); }
    let lo = value & 0x0f;
    let hi = (value >> 4) & 0x0f;
    if lo > 9 || hi > 9 { return None; }
    Some(hi * 10 + lo)
}

/// Read the battery-backed CMOS real-time clock. This implementation rejects
/// an updating or malformed RTC sample instead of returning fabricated time.
pub unsafe extern "win64" fn hal_query_real_time_clock(fields: *mut HalTimeFields) -> u8 {
    if fields.is_null() { return 0; }
    let _guard = CMOS_RTC_LOCK.lock();
    let mut sample = [0u8; 8];
    let mut accepted = false;
    for _ in 0..1000 {
        let status_a = unsafe { cmos_read(0x0a) };
        if status_a & 0x80 != 0 { continue; }
        let status_b = unsafe { cmos_read(0x0b) };
        sample[0] = unsafe { cmos_read(0x00) };
        sample[1] = unsafe { cmos_read(0x02) };
        sample[2] = unsafe { cmos_read(0x04) };
        sample[3] = unsafe { cmos_read(0x06) };
        sample[4] = unsafe { cmos_read(0x07) };
        sample[5] = unsafe { cmos_read(0x08) };
        sample[6] = unsafe { cmos_read(0x09) };
        sample[7] = unsafe { cmos_read(0x32) };
        if unsafe { cmos_read(0x0a) } & 0x80 == 0 {
            let binary = status_b & 0x04 != 0;
            let hour_pm = sample[2] & 0x80 != 0;
            let raw_hour = sample[2] & 0x7f;
            let Some(second) = rtc_decode(sample[0], binary) else { return 0; };
            let Some(minute) = rtc_decode(sample[1], binary) else { return 0; };
            let Some(mut hour) = rtc_decode(raw_hour, binary) else { return 0; };
            let Some(weekday) = rtc_decode(sample[3], binary) else { return 0; };
            let Some(day) = rtc_decode(sample[4], binary) else { return 0; };
            let Some(month) = rtc_decode(sample[5], binary) else { return 0; };
            let Some(year) = rtc_decode(sample[6], binary) else { return 0; };
            let century = rtc_decode(sample[7], binary).filter(|v| (19..=99).contains(v)).unwrap_or(20);
            if status_b & 0x02 == 0 {
                if !(1..=12).contains(&hour) { return 0; }
                hour = (hour % 12) + if hour_pm { 12 } else { 0 };
            } else if hour > 23 { return 0; }
            if second > 59 || minute > 59 || day == 0 || day > 31 || month == 0 || month > 12 || weekday == 0 || weekday > 7 {
                return 0;
            }
            let decoded = HalTimeFields {
                year: (century as i16 * 100 + year as i16),
                month: month as i16, day: day as i16, hour: hour as i16,
                minute: minute as i16, second: second as i16,
                milliseconds: 0, weekday: (weekday as i16 - 1),
            };
            unsafe { fields.write(decoded); }
            accepted = true;
            break;
        }
    }
    accepted as u8
}

/// Translate a bus-relative address on the conventional x86 PC buses.
///
/// This platform currently has identity-mapped CPU/bus physical addresses and
/// no bridge-window translation database. Support only the PC-compatible bus
/// types for which identity translation is valid; reject unknown buses and
/// invalid I/O-port addresses instead of returning a fabricated translation.
pub unsafe extern "win64" fn hal_translate_bus_address(
    interface_type: u32,
    _bus_number: u32,
    bus_address: i64,
    address_space: *mut u32,
    translated_address: *mut i64,
) -> u8 {
    if address_space.is_null() || translated_address.is_null() {
        return 0;
    }
    // INTERFACE_TYPE: Internal, Isa, Eisa, MicroChannel, and PCIBus.
    if !matches!(interface_type, 0 | 1 | 2 | 3 | 5) {
        return 0;
    }
    let space = unsafe { address_space.read() };
    if space > 1 { return 0; }
    let address = bus_address;
    if address < 0 { return 0; }
    // x86 port I/O uses a 16-bit port number. Do not truncate a bad address.
    if space == 1 && address > u16::MAX as i64 { return 0; }
    unsafe { translated_address.write(address); }
    1
}


/// Retrieve a validated firmware ACPI table by four-byte signature and instance.
///
/// This x64 HAL entry point is available after ACPI discovery has recorded the
/// RSDP. The returned pointer references the firmware table in the early
/// identity-mapped physical window and remains firmware-owned; callers must
/// treat it as read-only. STATUS_NOT_FOUND is returned when discovery has not
/// run or the requested instance is absent.
pub unsafe extern "win64" fn hal_acpi_get_table_ex(
    signature: u32,
    instance: u32,
    table: *mut *mut core::ffi::c_void,
    table_size: *mut u32,
) -> i32 {
    const STATUS_SUCCESS: i32 = 0;
    const STATUS_INVALID_PARAMETER: i32 = 0xC000_000D_u32 as i32;
    const STATUS_NOT_FOUND: i32 = 0xC000_0225_u32 as i32;
    if table.is_null() || table_size.is_null() {
        return STATUS_INVALID_PARAMETER;
    }
    unsafe {
        table.write(core::ptr::null_mut());
        table_size.write(0);
    }
    let rsdp = crate::acpi::rsdp_address();
    if rsdp == 0 { return STATUS_NOT_FOUND; }
    let signature = signature.to_le_bytes();
    match unsafe { crate::acpi::find_table(rsdp, signature, instance as usize) } {
        Ok(bytes) => {
            let Ok(length) = u32::try_from(bytes.len()) else { return STATUS_INVALID_PARAMETER; };
            unsafe {
                table.write(bytes.as_ptr() as *mut core::ffi::c_void);
                table_size.write(length);
            }
            STATUS_SUCCESS
        }
        Err(_) => STATUS_NOT_FOUND,
    }
}

/// Map an NT logical processor number to its firmware/APIC processor ID.
/// Unknown or not-yet-discovered logical processors return ULONG_MAX.
pub unsafe extern "win64" fn hal_get_processor_id_by_nt_number(nt_number: u32) -> u32 {
    let Ok(cpu) = crate::hal::cpu_local(nt_number as usize) else { return u32::MAX; };
    let id = cpu.apic_id.load(core::sync::atomic::Ordering::Acquire);
    if id == u32::MAX { u32::MAX } else { id }
}

/// Report SMT availability from architectural CPUID topology. This reports
/// what the current processor exposes to the OS, not a vendor-specific brand.
pub unsafe extern "win64" fn hal_is_hyper_threading_enabled() -> u8 {
    let max_leaf = unsafe { core::arch::x86_64::__cpuid(0) }.eax;
    if max_leaf >= 0x0b {
        let topology = unsafe { core::arch::x86_64::__cpuid_count(0x0b, 0) };
        if topology.ebx != 0 { return (topology.ebx > 1) as u8; }
    }
    if max_leaf >= 1 {
        let leaf = unsafe { core::arch::x86_64::__cpuid(1) };
        let htt = (leaf.edx & (1 << 28)) != 0;
        let logical_count = (leaf.ebx >> 16) & 0xff;
        return (htt && logical_count > 1) as u8;
    }
    0
}

/// Return the maximum logical-processor capacity exposed by this HAL.
pub unsafe extern "win64" fn hal_query_maximum_processor_count() -> u32 {
    super::MAX_CPUS as u32
}

/// Early boot/debug display path. This compatibility implementation writes
/// a bounded NUL-terminated string to the kernel's initialized COM1 console.
pub unsafe extern "win64" fn hal_display_string(string: *const u8) {
    if string.is_null() { return; }
    let mut chunk = [0u8; 128];
    let mut used = 0usize;
    for index in 0..4096usize {
        let byte = unsafe { string.add(index).read() };
        if byte == 0 { break; }
        chunk[used] = byte;
        used += 1;
        if used == chunk.len() {
            unsafe { crate::serial_write(&chunk); }
            used = 0;
        }
    }
    if used != 0 {
        unsafe { crate::serial_write(&chunk[..used]); }
    }
}

/// Reports whether every CPU reserved by MADT discovery has completed AP
/// startup. Failed/offline discovered processors keep this false; this avoids
/// telling the NT kernel that SMP initialization completed when it did not.
pub extern "win64" fn hal_all_processors_started() -> u8 {
    for index in 0..crate::hal::MAX_CPUS {
        match crate::hal::cpu_state(index) {
            Ok(crate::hal::CPU_DISCOVERED | crate::hal::CPU_STARTING) => return 0,
            Ok(crate::hal::CPU_FAILED) => return 0,
            _ => {}
        }
    }
    (crate::hal::online_cpu_count() != 0) as u8
}


/// Return the bootstrap DMA_ADAPTER for legacy drivers that request a
/// Legacy HalGetAdapter has no DEVICE_OBJECT argument in this ABI, so it cannot
/// select a device isolation domain. Fail closed; drivers must use IoGetDmaAdapter.
pub unsafe extern "win64" fn hal_get_adapter(_device_description:*const core::ffi::c_void,number_of_map_registers:*mut u32)->*mut core::ffi::c_void{
 if !number_of_map_registers.is_null(){unsafe{number_of_map_registers.write(0)}}
 core::ptr::null_mut()
}
/// Allocate common memory mapped through the adapter's registered IOMMU domain.
pub unsafe extern "win64" fn hal_allocate_common_buffer(adapter:*mut core::ffi::c_void,length:u32,logical_address:*mut i64,_cache_enabled:u8)->*mut core::ffi::c_void{
 if adapter.is_null()||logical_address.is_null()||length==0{return core::ptr::null_mut()}
 match unsafe{crate::dma::iommu_allocate_common(adapter,length as usize,crate::dma::adapter_maximum_address(adapter))}{
  Ok((cpu,logical))=>{unsafe{logical_address.write(logical as i64)}cpu.cast()},
  Err(_)=>core::ptr::null_mut()
 }
}
/// Flush CPU/device visibility through the IOMMU backend; missing sync services fail closed.
pub unsafe extern "win64" fn hal_flush_common_buffer(adapter:*mut core::ffi::c_void,length:u32,logical_address:i64,virtual_address:*mut core::ffi::c_void,write_to_device:u8){
 if adapter.is_null()||virtual_address.is_null()||length==0||logical_address<0{return}
 let _=write_to_device;
 let _=unsafe{crate::dma::flush_common_buffer_for_adapter(adapter,virtual_address.cast(),length as usize)};
}
/// Unmap the logical aperture before releasing backing pages.
pub unsafe extern "win64" fn hal_free_common_buffer(adapter:*mut core::ffi::c_void,length:u32,logical_address:i64,virtual_address:*mut core::ffi::c_void,_cache_enabled:u8){
 if adapter.is_null()||virtual_address.is_null()||length==0||logical_address<0{return}
 let _=unsafe{crate::dma::free_common_buffer_for_adapter(adapter,virtual_address.cast(),logical_address as u64,length as usize)};
}

/// The Windows x64 HAL forwards Ke routines to ntoskrnl. Keep the forwarding
/// decision here so both hal.dll name/ordinal imports and ntoskrnl imports land
/// on the same ABI-correct implementation.


/// Resolve only ABI-reviewed HAL exports. Unsupported names fail closed.
/// Request the kernel's generic IPI on the processors selected by the NT
/// affinity mask. Targets publish pending work for their scheduler/DPC path.
pub unsafe extern "win64" fn hal_request_ipi(target_set: u64) {
    unsafe { hal_request_ipi_specify_vector(target_set, super::apic::HAL_IPI_VECTOR as u32); }
}

/// Request a caller-selected fixed-vector IPI on every online processor in
/// the supplied NT affinity mask. The vector's IDT entry must already be
/// installed on all target CPUs; this Windows ABI returns void.
pub unsafe extern "win64" fn hal_request_ipi_specify_vector(target_set: u64, vector: u32) {
    if vector < 0x20 || vector >= 0xff { return; }
    for logical_id in 0..crate::hal::MAX_CPUS {
        if target_set & (1u64 << logical_id) == 0 { continue; }
        if crate::hal::cpu_state(logical_id).ok() != Some(crate::hal::CPU_ONLINE) { continue; }
        let Ok(cpu) = crate::hal::cpu_local(logical_id) else { continue; };
        let apic_id = cpu.apic_id.load(core::sync::atomic::Ordering::Acquire);
        if apic_id > u8::MAX as u32 { continue; }
        let _ = unsafe { super::apic::request_ipi(apic_id as u8, vector as u8) };
    }
}

pub fn resolve(name: &[u8]) -> Option<u64> {
    if name.eq_ignore_ascii_case(b"HalQueryRealTimeClock") {
        Some(hal_query_real_time_clock as usize as u64)
    } else if name.eq_ignore_ascii_case(b"HalAcpiGetTableEx") {
        Some(hal_acpi_get_table_ex as usize as u64)
    } else if name.eq_ignore_ascii_case(b"HalDisplayString") {
        Some(hal_display_string as usize as u64)
    } else if name.eq_ignore_ascii_case(b"HalQueryMaximumProcessorCount") {
        Some(hal_query_maximum_processor_count as usize as u64)
    } else if name.eq_ignore_ascii_case(b"HalAllProcessorsStarted") {
        Some(hal_all_processors_started as usize as u64)
    } else if name.eq_ignore_ascii_case(b"KeFlushWriteBuffer") {
        Some(crate::ntoskrnl::ke::runtime::ke_flush_write_buffer as usize as u64)
    } else if name.eq_ignore_ascii_case(b"KeQueryPerformanceCounter") {
        super::apic::tsc_frequency_hz().map(|_| crate::ntoskrnl::ke::runtime::ke_query_performance_counter as usize as u64)
    } else if name.eq_ignore_ascii_case(b"KeStallExecutionProcessor") {
        super::apic::tsc_frequency_hz().map(|_| crate::ntoskrnl::ke::runtime::ke_stall_execution_processor as usize as u64)
    } else if name.eq_ignore_ascii_case(b"HalFlushCommonBuffer") {
        Some(hal_flush_common_buffer as usize as u64)
    } else if name.eq_ignore_ascii_case(b"HalAllocateCommonBuffer") {
        Some(hal_allocate_common_buffer as usize as u64)
    } else if name.eq_ignore_ascii_case(b"HalFreeCommonBuffer") {
        Some(hal_free_common_buffer as usize as u64)
    } else if name.eq_ignore_ascii_case(b"HalGetAdapter") {
        Some(hal_get_adapter as usize as u64)
    } else if name.eq_ignore_ascii_case(b"HalTranslateBusAddress") {
        Some(hal_translate_bus_address as usize as u64)
    } else if name.eq_ignore_ascii_case(b"HalGetBusData") {
        Some(hal_get_bus_data as usize as u64)
    } else if name.eq_ignore_ascii_case(b"HalGetBusDataByOffset") {
        Some(hal_get_bus_data_by_offset as usize as u64)
    } else if name.eq_ignore_ascii_case(b"HalSetBusData") {
        Some(hal_set_bus_data as usize as u64)
    } else if name.eq_ignore_ascii_case(b"HalSetBusDataByOffset") {
        Some(hal_set_bus_data_by_offset as usize as u64)
    } else if name.eq_ignore_ascii_case(b"HalRequestIpi") {
        Some(hal_request_ipi as usize as u64)
    } else if name.eq_ignore_ascii_case(b"HalRequestIpiSpecifyVector") {
        Some(hal_request_ipi_specify_vector as usize as u64)
    } else if name.eq_ignore_ascii_case(b"HalGetProcessorIdByNtNumber") {
        Some(hal_get_processor_id_by_nt_number as usize as u64)
    } else if name.eq_ignore_ascii_case(b"HalIsHyperThreadingEnabled") {
        Some(hal_is_hyper_threading_enabled as usize as u64)
    } else { None }
}

pub fn resolve_ordinal(ordinal: u16) -> Option<u64> {
    match ordinal {
        48 => Some(hal_query_real_time_clock as usize as u64),
        1 => Some(hal_acpi_get_table_ex as usize as u64),
        4 => Some(hal_all_processors_started as usize as u64),
        15 => Some(hal_display_string as usize as u64),
        47 => Some(hal_query_maximum_processor_count as usize as u64),
        84 => Some(crate::ntoskrnl::ke::runtime::ke_flush_write_buffer as usize as u64),
        85 => super::apic::tsc_frequency_hz().map(|_| crate::ntoskrnl::ke::runtime::ke_query_performance_counter as usize as u64),
        86 => super::apic::tsc_frequency_hz().map(|_| crate::ntoskrnl::ke::runtime::ke_stall_execution_processor as usize as u64),
        21 => Some(hal_flush_common_buffer as usize as u64),
        6 => Some(hal_allocate_common_buffer as usize as u64),
        22 => Some(hal_free_common_buffer as usize as u64),
        25 => Some(hal_get_bus_data as usize as u64),
        26 => Some(hal_get_bus_data_by_offset as usize as u64),
        61 => Some(hal_set_bus_data as usize as u64),
        62 => Some(hal_set_bus_data_by_offset as usize as u64),
        24 => Some(hal_get_adapter as usize as u64),
        73 => Some(hal_translate_bus_address as usize as u64),
        55 => Some(hal_request_ipi as usize as u64),
        56 => Some(hal_request_ipi_specify_vector as usize as u64),
        33 => Some(hal_get_processor_id_by_nt_number as usize as u64),
        41 => Some(hal_is_hyper_threading_enabled as usize as u64),
        _ => None,
    }
}

pub fn find_by_name(name: &[u8]) -> Option<&'static HalExport> {
    EXPORTS.iter().find(|entry| entry.name.as_bytes().eq_ignore_ascii_case(name))
}
pub fn find_by_ordinal(ordinal: u16) -> Option<&'static HalExport> {
    EXPORTS.iter().find(|entry| entry.ordinal == ordinal)
}
pub fn count() -> usize { EXPORTS.len() }

pub fn self_test() -> bool {
    EXPORTS.len() == 91
        && EXPORTS.iter().enumerate().all(|(i, entry)| entry.ordinal as usize == i + 1)
        && find_by_name(b"HalAcpiGetTableEx").map(|e| e.ordinal) == Some(1)
        && find_by_name(b"HalAllProcessorsStarted").map(|e| e.ordinal) == Some(4)
        && find_by_name(b"halrequestipi").map(|e| e.ordinal) == Some(55)
        && find_by_ordinal(91).map(|entry| entry.name) == Some("x86BiosWriteMemory")
        && find_by_name(b"not-a-hal-export").is_none()
        && resolve(b"HalQueryRealTimeClock").is_some()
        && resolve_ordinal(48).is_some()
        && resolve(b"HalAcpiGetTableEx").is_some()
        && resolve_ordinal(1).is_some()
        && resolve(b"HalDisplayString").is_some()
        && resolve_ordinal(15).is_some()
        && resolve(b"HalQueryMaximumProcessorCount").is_some()
        && resolve(b"HalAllProcessorsStarted").is_some()
        && resolve_ordinal(4).is_some()
        && pci_config_address(0, 0, 0) == Some(0x8000_0000)
        && pci_config_address(0, 1, 0) == Some(0x8000_0800)
        && pci_config_address(0, 0x20, 0) == Some(0x8000_0100)
        && pci_config_address(255, 255, 255) == Some(0x80ff_fffc)
        && pci_config_address(256, 0, 0).is_none()
        && pci_config_address(0, 256, 0).is_none()
        && pci_config_byte_writable(0x04)
        && pci_config_byte_writable(0x05)
        && !pci_config_byte_writable(0x00)
        && !pci_config_byte_writable(0x06)
        && !pci_config_byte_writable(0x10)
        && pci_config_byte_writable(0x40)
        && resolve(b"HalFlushCommonBuffer").is_some()
        && resolve_ordinal(21).is_some()
        && resolve(b"HalAllocateCommonBuffer").is_some()
        && resolve(b"HalFreeCommonBuffer").is_some()
        && resolve_ordinal(6).is_some()
        && resolve_ordinal(22).is_some()
        && resolve(b"HalTranslateBusAddress").is_some()
        && resolve_ordinal(73).is_some()
        && resolve(b"HalGetBusData").is_some()
        && resolve(b"HalGetBusDataByOffset").is_some()
        && resolve(b"HalSetBusData").is_some()
        && resolve(b"HalSetBusDataByOffset").is_some()
        && resolve(b"HalRequestIpi").is_some()
        && resolve_ordinal(55).is_some()
        && resolve(b"HalRequestIpiSpecifyVector").is_some()
        && resolve_ordinal(56).is_some()
        && resolve(b"HalGetProcessorIdByNtNumber").is_some()
        && resolve(b"HalIsHyperThreadingEnabled").is_some()
        && resolve_ordinal(33).is_some()
        && resolve_ordinal(41).is_some()
        && resolve_ordinal(25).is_some()
        && resolve_ordinal(26).is_some()
        && resolve_ordinal(61).is_some()
        && resolve_ordinal(62).is_some()
        && resolve(b"KeFlushWriteBuffer").is_some()
        && (resolve(b"KeQueryPerformanceCounter").is_some() == super::apic::tsc_frequency_hz().is_some())
        && (resolve(b"KeStallExecutionProcessor").is_some() == super::apic::tsc_frequency_hz().is_some())
        && resolve_ordinal(47).is_some()
        && (resolve_ordinal(85).is_some() == super::apic::tsc_frequency_hz().is_some())
        && (resolve_ordinal(86).is_some() == super::apic::tsc_frequency_hz().is_some())
        && resolve(b"KeFlushWriteBuffer") == Some(crate::ntoskrnl::ke::runtime::ke_flush_write_buffer as usize as u64)
        && (resolve(b"KeQueryPerformanceCounter") == super::apic::tsc_frequency_hz().map(|_| crate::ntoskrnl::ke::runtime::ke_query_performance_counter as usize as u64))
        && (resolve(b"KeStallExecutionProcessor") == super::apic::tsc_frequency_hz().map(|_| crate::ntoskrnl::ke::runtime::ke_stall_execution_processor as usize as u64))
}
