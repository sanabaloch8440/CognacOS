//! NTOSKRNL export catalogue and safe callable-import dispatch.
//! Export metadata targets Windows 11 x64 10.0.26100.8036-AMD64; most exports remain unsupported.
use crate::pe::ImportSymbol;

pub const STATUS_NOT_IMPLEMENTED: u32 = 0xC000_0002;
pub const STATUS_PROCEDURE_NOT_FOUND: u32 = 0xC000_007A;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KernelModule { Ex, Ke, Mm, Ob, Ps, Io, Cm, Se, Rtl, Zw, Other }
#[derive(Clone, Copy, Debug)]
pub struct Export { pub name: &'static str, pub ordinal: u16, pub module: KernelModule }

mod ex; mod ke; mod mm; mod ob; mod ps; mod io; mod cm; mod se; mod rtl; #[path = "rtl_runtime/mod.rs"] mod rtl_runtime; mod zw; mod other;
mod manifest_26100;
pub mod abi_signatures;


pub fn exports() -> impl Iterator<Item = Export> {
 ex::EXPORTS.iter().map(|&(name,ordinal)|Export{name,ordinal,module:KernelModule::Ex})
 .chain(ke::EXPORTS.iter().map(|&(name,ordinal)|Export{name,ordinal,module:KernelModule::Ke}))
 .chain(mm::EXPORTS.iter().map(|&(name,ordinal)|Export{name,ordinal,module:KernelModule::Mm}))
 .chain(ob::EXPORTS.iter().map(|&(name,ordinal)|Export{name,ordinal,module:KernelModule::Ob}))
 .chain(ps::EXPORTS.iter().map(|&(name,ordinal)|Export{name,ordinal,module:KernelModule::Ps}))
 .chain(io::EXPORTS.iter().map(|&(name,ordinal)|Export{name,ordinal,module:KernelModule::Io}))
 .chain(cm::EXPORTS.iter().map(|&(name,ordinal)|Export{name,ordinal,module:KernelModule::Cm}))
 .chain(se::EXPORTS.iter().map(|&(name,ordinal)|Export{name,ordinal,module:KernelModule::Se}))
 .chain(rtl::EXPORTS.iter().map(|&(name,ordinal)|Export{name,ordinal,module:KernelModule::Rtl}))
 .chain(zw::EXPORTS.iter().map(|&(name,ordinal)|Export{name,ordinal,module:KernelModule::Zw}))
 .chain(other::EXPORTS.iter().map(|&(name,ordinal)|Export{name,ordinal,module:KernelModule::Other}))
}

/// Return an address only when the requested symbol has an ABI-correct callable
/// implementation. Export presence alone is not sufficient to resolve a driver import.
pub fn resolve(dll: &[u8], symbol: ImportSymbol<'_>) -> Option<u64> {
 if !(dll.eq_ignore_ascii_case(b"ntoskrnl.exe") || dll.eq_ignore_ascii_case(b"ntkrnlmp.exe") || dll.eq_ignore_ascii_case(b"ntkrnlpa.exe") || dll.eq_ignore_ascii_case(b"ntkrpamp.exe")) { return None; }
 let name = match symbol {
  ImportSymbol::Name(requested) => exports().find(|e|e.name.as_bytes().eq_ignore_ascii_case(requested)).map(|e|e.name)?,
  ImportSymbol::Ordinal(ordinal) => manifest_26100::ENTRIES.iter().find(|e|e.ordinal==ordinal)?.name?,
 };
 let address = match name {
  "KeGetCurrentIrql" => ke::ke_get_current_irql as usize,
  "KeQueryActiveProcessorCountEx" => ke::ke_query_active_processor_count_ex as usize,
  "KeQueryActiveProcessorCount" => ke::ke_query_active_processor_count as usize,
  "KeQueryActiveGroupCount" => ke::ke_query_active_group_count as usize,
  "KeGetCurrentProcessorNumberEx" => ke::ke_get_current_processor_number_ex as usize,
  "KeGetCurrentProcessorIndex" => ke::ke_get_current_processor_index as usize,
  "RtlAreBitsClear" => rtl_runtime::rtl_are_bits_clear as usize,
  "RtlAreBitsSet" => rtl_runtime::rtl_are_bits_set as usize,
  "RtlClearAllBits" => rtl_runtime::rtl_clear_all_bits as usize,
  "RtlClearBit" => rtl_runtime::rtl_clear_bit as usize,
  "RtlClearBits" => rtl_runtime::rtl_clear_bits as usize,
  "RtlFindClearBits" => rtl_runtime::rtl_find_clear_bits as usize,
  "RtlFindClearBitsAndSet" => rtl_runtime::rtl_find_clear_bits_and_set as usize,
  "RtlFindSetBits" => rtl_runtime::rtl_find_set_bits as usize,
  "RtlFindSetBitsAndClear" => rtl_runtime::rtl_find_set_bits_and_clear as usize,
  "RtlFindLeastSignificantBit" => rtl_runtime::rtl_find_least_significant_bit as usize,
  "RtlFindMostSignificantBit" => rtl_runtime::rtl_find_most_significant_bit as usize,
  "RtlNumberOfClearBits" => rtl_runtime::rtl_number_of_clear_bits as usize,
  "RtlNumberOfSetBits" => rtl_runtime::rtl_number_of_set_bits as usize,
  "RtlSetAllBits" => rtl_runtime::rtl_set_all_bits as usize,
  "RtlSetBit" => rtl_runtime::rtl_set_bit as usize,
  "RtlSetBits" => rtl_runtime::rtl_set_bits as usize,
  "RtlTestBit" => rtl_runtime::rtl_test_bit as usize,
  "RtlFindFirstRunClear" => rtl_runtime::rtl_find_first_run_clear as usize,
  "RtlFindLongestRunClear" => rtl_runtime::rtl_find_longest_run_clear as usize,
  "RtlFindNextForwardRunClear" => rtl_runtime::rtl_find_next_forward_run_clear as usize,
  "RtlFindNextForwardRunSet" => rtl_runtime::rtl_find_next_forward_run_set as usize,
  "RtlFindLastBackwardRunClear" => rtl_runtime::rtl_find_last_backward_run_clear as usize,
  "RtlCompareMemory" => rtl_runtime::rtl_compare_memory as usize,
  "RtlCompareMemoryUlong" => rtl_runtime::rtl_compare_memory_ulong as usize,
  "RtlComputeCrc32" => rtl_runtime::rtl_compute_crc32 as usize,
  "RtlCrc32" => rtl_runtime::rtl_crc32 as usize,
  "RtlDowncaseUnicodeChar" => rtl_runtime::rtl_downcase_unicode_char as usize,
  "RtlUpcaseUnicodeChar" => rtl_runtime::rtl_upcase_unicode_char as usize,
  "RtlEqualLuid" => rtl_runtime::rtl_equal_luid as usize,
  "RtlEqualString" => rtl_runtime::rtl_equal_string as usize,
  "RtlCompareString" => rtl_runtime::rtl_compare_string as usize,
  "RtlEqualUnicodeString" => rtl_runtime::rtl_equal_unicode_string as usize,
  "RtlCompareUnicodeString" => rtl_runtime::rtl_compare_unicode_string as usize,
  "RtlCopyLuid" => rtl_runtime::rtl_copy_luid as usize,
  "RtlCopyString" => rtl_runtime::rtl_copy_string as usize,
  "RtlCopyUnicodeString" => rtl_runtime::rtl_copy_unicode_string as usize,
  "RtlAreAllAccessesGranted" => rtl_runtime::rtl_are_all_accesses_granted as usize,
  "RtlAreAnyAccessesGranted" => rtl_runtime::rtl_are_any_accesses_granted as usize,
  "RtlCopyMemory" => rtl_runtime::rtl_copy_memory as usize,
  "RtlFillMemory" => rtl_runtime::rtl_fill_memory as usize,
  "RtlCopyMemoryNonTemporal" => rtl_runtime::rtl_copy_memory_non_temporal as usize,
  "RtlFillMemoryNonTemporal" => rtl_runtime::rtl_fill_memory_non_temporal as usize,
  "RtlMoveMemory" => rtl_runtime::rtl_move_memory as usize,
  "RtlZeroMemory" => rtl_runtime::rtl_zero_memory as usize,
  "RtlInitAnsiString" => rtl_runtime::rtl_init_ansi_string as usize,
  "RtlInitString" => rtl_runtime::rtl_init_string as usize,
  "RtlInitUnicodeString" => rtl_runtime::rtl_init_unicode_string as usize,
  "RtlPrefixUnicodeString" => rtl_runtime::rtl_prefix_unicode_string as usize,
  "RtlAppendUnicodeStringToString" => rtl_runtime::rtl_append_unicode_string_to_string as usize,
  "RtlAppendUnicodeToString" => rtl_runtime::rtl_append_unicode_to_string as usize,
  "RtlInitializeBitMap" => rtl_runtime::rtl_initialize_bit_map as usize,
  "RtlIsZeroMemory" => rtl_runtime::rtl_is_zero_memory as usize,
  "RtlUnsignedMultiplyHigh" => rtl_runtime::rtl_unsigned_multiply_high as usize,
  "RtlRandom" => rtl_runtime::rtl_random as usize,
  "RtlRandomEx" => rtl_runtime::rtl_random_ex as usize,
  "RtlCharToInteger" => rtl_runtime::rtl_char_to_integer as usize,
  "RtlInitAnsiStringEx" => rtl_runtime::rtl_init_ansi_string_ex as usize,
  "RtlInitUnicodeStringEx" => rtl_runtime::rtl_init_unicode_string_ex as usize,
  "RtlPrefixString" => rtl_runtime::rtl_prefix_string as usize,
  "RtlSuffixUnicodeString" => rtl_runtime::rtl_suffix_unicode_string as usize,
  "RtlCompareUnicodeStrings" => rtl_runtime::rtl_compare_unicode_strings as usize,
  "RtlUnicodeStringToInteger" => rtl_runtime::rtl_unicode_string_to_integer as usize,
  "RtlIntegerToUnicodeString" => rtl_runtime::rtl_integer_to_unicode_string as usize,
  "RtlIntegerToChar" => rtl_runtime::rtl_integer_to_char as usize,
  "RtlAnsiStringToUnicodeSize" => rtl_runtime::rtl_ansi_string_to_unicode_size as usize,
  "RtlUpperChar" => rtl_runtime::rtl_upper_char as usize,
  "RtlUnicodeStringToAnsiSize" => rtl_runtime::rtl_unicode_string_to_ansi_size as usize,
  "RtlAppendAsciizToString" => rtl_runtime::rtl_append_asciiz_to_string as usize,
  "RtlAppendStringToString" => rtl_runtime::rtl_append_string_to_string as usize,
  "RtlUpcaseUnicodeString" => rtl_runtime::rtl_upcase_unicode_string as usize,
  "RtlDowncaseUnicodeString" => rtl_runtime::rtl_downcase_unicode_string as usize,
  "RtlFindUnicodeSubstring" => rtl_runtime::rtl_find_unicode_substring as usize,
  "RtlHashUnicodeString" => rtl_runtime::rtl_hash_unicode_string as usize,
  "RtlLengthRequiredSid" => rtl_runtime::rtl_length_required_sid as usize,
  "RtlLengthSid" => rtl_runtime::rtl_length_sid as usize,
  "RtlValidSid" => rtl_runtime::rtl_valid_sid as usize,
  "RtlEqualSid" => rtl_runtime::rtl_equal_sid as usize,
  "RtlCopySid" => rtl_runtime::rtl_copy_sid as usize,
  "RtlInitializeSid" => rtl_runtime::rtl_initialize_sid as usize,
  "RtlSubAuthoritySid" => rtl_runtime::rtl_sub_authority_sid as usize,
  "RtlSubAuthorityCountSid" => rtl_runtime::rtl_sub_authority_count_sid as usize,
  "RtlValidateUnicodeString" => rtl_runtime::rtl_validate_unicode_string as usize,
  "RtlAnsiStringToUnicodeString" => rtl_runtime::rtl_ansi_string_to_unicode_string as usize,
  "RtlUnicodeStringToAnsiString" => rtl_runtime::rtl_unicode_string_to_ansi_string as usize,
  "RtlUpcaseUnicodeStringToAnsiString" => rtl_runtime::rtl_upcase_unicode_string_to_ansi_string as usize,
  "RtlDowncaseUnicodeStringToAnsiString" => rtl_runtime::rtl_downcase_unicode_string_to_ansi_string as usize,
  "RtlAnsiCharToUnicodeChar" => rtl_runtime::rtl_ansi_char_to_unicode_char as usize,
  "RtlUnicodeStringToInt64" => rtl_runtime::rtl_unicode_string_to_int64 as usize,
  "RtlInt64ToUnicodeString" => rtl_runtime::rtl_int64_to_unicode_string as usize,
  "RtlLargeIntegerToChar" => rtl_runtime::rtl_large_integer_to_char as usize,
  "RtlUpperString" => rtl_runtime::rtl_upper_string as usize,
  "RtlMultiByteToUnicodeSize" => rtl_runtime::rtl_multi_byte_to_unicode_size as usize,
  "RtlUnicodeToMultiByteSize" => rtl_runtime::rtl_unicode_to_multi_byte_size as usize,
  "RtlxAnsiStringToUnicodeSize" => rtl_runtime::rtlx_ansi_string_to_unicode_size as usize,
  "RtlxUnicodeStringToAnsiSize" => rtl_runtime::rtlx_unicode_string_to_ansi_size as usize,
  "RtlCrc64" => rtl_runtime::rtl_crc64 as usize,
  "RtlOemStringToUnicodeSize" => rtl_runtime::rtl_oem_string_to_unicode_size as usize,
  "RtlUnicodeStringToOemSize" => rtl_runtime::rtl_unicode_string_to_oem_size as usize,
  "RtlxOemStringToUnicodeSize" => rtl_runtime::rtlx_oem_string_to_unicode_size as usize,
  "RtlxUnicodeStringToOemSize" => rtl_runtime::rtlx_unicode_string_to_oem_size as usize,
  "RtlCompareAltitudes" => rtl_runtime::rtl_compare_altitudes as usize,
  "RtlMultiByteToUnicodeN" => rtl_runtime::rtl_multi_byte_to_unicode_n as usize,
  "RtlUnicodeToMultiByteN" => rtl_runtime::rtl_unicode_to_multi_byte_n as usize,
  "RtlCopyBitMap" => rtl_runtime::rtl_copy_bit_map as usize,
  "RtlIntersectBitMaps" => rtl_runtime::rtl_intersect_bit_maps as usize,
  "RtlMergeBitMaps" => rtl_runtime::rtl_merge_bit_maps as usize,
  "RtlShiftLeftBitMap" => rtl_runtime::rtl_shift_left_bit_map as usize,
  "RtlIpv4AddressToStringA" => rtl_runtime::rtl_ipv4_address_to_string_a as usize,
  "RtlIpv4AddressToStringW" => rtl_runtime::rtl_ipv4_address_to_string_w as usize,
  "RtlEthernetAddressToStringA" => rtl_runtime::rtl_ethernet_address_to_string_a as usize,
  "RtlEthernetAddressToStringW" => rtl_runtime::rtl_ethernet_address_to_string_w as usize,
  "RtlStringFromGUID" => rtl_runtime::rtl_string_from_guid as usize,
  "RtlGUIDFromString" => rtl_runtime::rtl_guid_from_string as usize,
  "RtlIntegerToUnicode" => rtl_runtime::rtl_integer_to_unicode as usize,
  "RtlCopyBitMapEx" => rtl_runtime::rtl_copy_bit_map_ex as usize,
  "RtlInitializeBitMapEx" => rtl_runtime::rtl_initialize_bit_map_ex as usize,
  "RtlAreBitsClearEx" => rtl_runtime::rtl_are_bits_clear_ex as usize,
  "RtlAreBitsSetEx" => rtl_runtime::rtl_are_bits_set_ex as usize,
  "RtlClearAllBitsEx" => rtl_runtime::rtl_clear_all_bits_ex as usize,
  "RtlClearBitEx" => rtl_runtime::rtl_clear_bit_ex as usize,
  "RtlClearBitsEx" => rtl_runtime::rtl_clear_bits_ex as usize,
  "RtlSetAllBitsEx" => rtl_runtime::rtl_set_all_bits_ex as usize,
  "RtlSetBitEx" => rtl_runtime::rtl_set_bit_ex as usize,
  "RtlSetBitsEx" => rtl_runtime::rtl_set_bits_ex as usize,
  "RtlTestBitEx" => rtl_runtime::rtl_test_bit_ex as usize,
  "RtlNumberOfSetBitsEx" => rtl_runtime::rtl_number_of_set_bits_ex as usize,
  "RtlNumberOfClearBitsEx" => rtl_runtime::rtl_number_of_clear_bits_ex as usize,
  "RtlNumberOfSetBitsInRange" => rtl_runtime::rtl_number_of_set_bits_in_range as usize,
  "RtlNumberOfClearBitsInRange" => rtl_runtime::rtl_number_of_clear_bits_in_range as usize,
  "RtlNumberOfSetBitsInRangeEx" => rtl_runtime::rtl_number_of_set_bits_in_range_ex as usize,
  "RtlFindClearBitsEx" => rtl_runtime::rtl_find_clear_bits_ex as usize,
  "RtlFindClearBitsAndSetEx" => rtl_runtime::rtl_find_clear_bits_and_set_ex as usize,
  "RtlFindSetBitsEx" => rtl_runtime::rtl_find_set_bits_ex as usize,
  "RtlFindSetBitsAndClearEx" => rtl_runtime::rtl_find_set_bits_and_clear_ex as usize,
  "RtlIntersectBitMapsEx" => rtl_runtime::rtl_intersect_bit_maps_ex as usize,
  "RtlMergeBitMapsEx" => rtl_runtime::rtl_merge_bit_maps_ex as usize,
  "RtlShiftLeftBitMapEx" => rtl_runtime::rtl_shift_left_bit_map_ex as usize,
  "RtlFindNextForwardRunClearEx" => rtl_runtime::rtl_find_next_forward_run_clear_ex as usize,
  "RtlFindNextForwardRunSetEx" => rtl_runtime::rtl_find_next_forward_run_set_ex as usize,
  "RtlTimeToElapsedTimeFields" => rtl_runtime::rtl_time_to_elapsed_time_fields as usize,
  "RtlSecondsSince1970ToTime" => rtl_runtime::rtl_seconds_since_1970_to_time as usize,
  "RtlSecondsSince1980ToTime" => rtl_runtime::rtl_seconds_since_1980_to_time as usize,
  "RtlTimeToSecondsSince1970" => rtl_runtime::rtl_time_to_seconds_since_1970 as usize,
  "RtlTimeToSecondsSince1980" => rtl_runtime::rtl_time_to_seconds_since_1980 as usize,
  "RtlTimeToTimeFields" => rtl_runtime::rtl_time_to_time_fields as usize,
  "RtlTimeFieldsToTime" => rtl_runtime::rtl_time_fields_to_time as usize,
  "RtlGetNtProductType" => rtl_runtime::rtl_get_nt_product_type as usize,
  "RtlGetNtGlobalFlags" => rtl_runtime::rtl_get_nt_global_flags as usize,
  "RtlGetSuiteMask" => rtl_runtime::rtl_get_suite_mask as usize,
  "RtlGetActiveConsoleId" => rtl_runtime::rtl_get_active_console_id as usize,
  "RtlSetActiveConsoleId" => rtl_runtime::rtl_set_active_console_id as usize,
  "RtlGetCurrentServiceSessionId" => rtl_runtime::rtl_get_current_service_session_id as usize,
  "RtlIsProcessorFeaturePresent" => rtl_runtime::rtl_is_processor_feature_present as usize,
  "RtlIsMultiSessionSku" => rtl_runtime::rtl_is_multi_session_sku as usize,
  "RtlIsMultiUsersInSessionSku" => rtl_runtime::rtl_is_multi_users_in_session_sku as usize,
  "RtlIsElevatedRid" => rtl_runtime::rtl_is_elevated_rid as usize,
  "RtlIsNtDdiVersionAvailable" => rtl_runtime::rtl_is_nt_ddi_version_available as usize,
  "RtlIsServicePackVersionInstalled" => rtl_runtime::rtl_is_service_pack_version_installed as usize,
  "RtlIsStateSeparationEnabled" => rtl_runtime::rtl_is_state_separation_enabled as usize,
  "RtlCheckPortableOperatingSystem" => rtl_runtime::rtl_check_portable_operating_system as usize,
  "RtlIsValidOemCharacter" => rtl_runtime::rtl_is_valid_oem_character as usize,
  "RtlNtStatusToDosError" => rtl_runtime::rtl_nt_status_to_dos_error as usize,
  "RtlNtStatusToDosErrorNoTeb" => rtl_runtime::rtl_nt_status_to_dos_error_no_teb as usize,
  "RtlNumberOfSetBitsUlongPtr" => rtl_runtime::rtl_number_of_set_bits_ulong_ptr as usize,
  "RtlUdiv128" => rtl_runtime::rtl_udiv128 as usize,
  // All other names deliberately fail closed until their own signatures and semantics are implemented.
  _ => return None,
 };
 Some(address as u64)
}

pub fn named_export_count() -> usize { exports().count() }
pub fn ordinal_only_export_count() -> usize { manifest_26100::ORDINAL_ONLY.len() }
pub fn self_test() -> bool {
 named_export_count()==3352 && ordinal_only_export_count()==37 && exports().all(|e|!e.name.is_empty())
 && resolve(b"ntoskrnl.exe",ImportSymbol::Ordinal(1)).is_none()
 && resolve(b"kernel32.dll",ImportSymbol::Ordinal(1)).is_none()
 && resolve(b"ntoskrnl.exe",ImportSymbol::Name(b"KeGetCurrentIrql")).is_some()
 && resolve(b"ntoskrnl.exe",ImportSymbol::Name(b"ExAllocatePool2")).is_none()
 && resolve(b"ntoskrnl.exe",ImportSymbol::Name(b"RtlAreBitsClear")).is_some()
 && resolve(b"ntoskrnl.exe",ImportSymbol::Name(b"RtlComputeCrc32")).is_some()
 && resolve(b"ntoskrnl.exe",ImportSymbol::Name(b"RtlFillMemory")).is_some()
 && abi_signatures::documented_signature("KeGetCurrentIrql").is_some()
}
