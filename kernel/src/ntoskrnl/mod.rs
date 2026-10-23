//! NTOSKRNL export catalogue and safe callable-import dispatch.
//! Export metadata targets Windows 11 x64 10.0.26100.8036-AMD64; most exports remain unsupported.
use crate::pe::ImportSymbol;

pub const STATUS_NOT_IMPLEMENTED: u32 = 0xC000_0002;
pub const STATUS_PROCEDURE_NOT_FOUND: u32 = 0xC000_007A;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KernelModule { Ex, Ke, Mm, Ob, Ps, Io, Cm, Se, Rtl, Zw, Other }
#[derive(Clone, Copy, Debug)]
pub struct Export { pub name: &'static str, pub ordinal: u16, pub module: KernelModule }

mod ex; #[path = "ke/mod.rs"] pub(crate) mod ke; mod mm; mod ob; mod ps; mod io; mod cm; mod se; mod rtl; #[path = "rtl_runtime/mod.rs"] mod rtl_runtime; mod zw; mod other;
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

/// Validate the implemented page-table translation and checked-copy ABI surface.
pub fn memory_manager_self_test() -> bool { mm::runtime::self_test() }

/// Return an address only when the requested symbol has an ABI-correct callable
/// implementation. Export presence alone is not sufficient to resolve a driver import.
pub fn resolve(dll: &[u8], symbol: ImportSymbol<'_>) -> Option<u64> {
 // Windows x64 HAL imports are resolved separately from ntoskrnl imports.
 // Only signature-compatible implementations are returned; unsupported HAL
 // exports must remain unresolved rather than point at a generic stub.
 if dll.eq_ignore_ascii_case(b"hal.dll") {
  return match symbol {
   ImportSymbol::Name(requested) => crate::hal::exports::resolve(requested),
   ImportSymbol::Ordinal(ordinal) => crate::hal::exports::resolve_ordinal(ordinal),
  };
 }
 if !(dll.eq_ignore_ascii_case(b"ntoskrnl.exe") || dll.eq_ignore_ascii_case(b"ntkrnlmp.exe") || dll.eq_ignore_ascii_case(b"ntkrnlpa.exe") || dll.eq_ignore_ascii_case(b"ntkrpamp.exe")) { return None; }
 let name = match symbol {
  ImportSymbol::Name(requested) => exports().find(|e|e.name.as_bytes().eq_ignore_ascii_case(requested)).map(|e|e.name)?,
  ImportSymbol::Ordinal(ordinal) => manifest_26100::ENTRIES.iter().find(|e|e.ordinal==ordinal)?.name?,
 };
 if matches!(name, "KeQueryPerformanceCounter" | "KeStallExecutionProcessor")
     && crate::apic::tsc_frequency_hz().is_none() {
  return None;
 }
 let address = match name {
  "KeQueryPerformanceCounter" => ke::runtime::ke_query_performance_counter as usize,
  "KeStallExecutionProcessor" => ke::runtime::ke_stall_execution_processor as usize,
  "MmAllocateContiguousMemory" => mm::runtime::mm_allocate_contiguous_memory as usize,
  "MmAllocateContiguousMemorySpecifyCache" => mm::runtime::mm_allocate_contiguous_memory_specify_cache as usize,
  "MmFreeContiguousMemory" => mm::runtime::mm_free_contiguous_memory as usize,
  "MmFreeContiguousMemorySpecifyCache" => mm::runtime::mm_free_contiguous_memory_specify_cache as usize,
  "MmSizeOfMdl" => mm::runtime::mm_size_of_mdl as usize,
  "MmGetPhysicalAddress" => mm::runtime::mm_get_physical_address as usize,
  "MmGetVirtualForPhysical" => mm::runtime::mm_get_virtual_for_physical as usize,
  "MmIsAddressValid" => mm::runtime::mm_is_address_valid as usize,
  "MmIsNonPagedSystemAddressValid" => mm::runtime::mm_is_non_paged_system_address_valid as usize,
  "MmCopyMemory" => mm::runtime::mm_copy_memory as usize,
  "KeGetCurrentIrql" => ke::runtime::ke_get_current_irql as usize,
  "KeGetEffectiveIrql" => ke::runtime::ke_get_current_irql as usize,
  "KeRaiseIrqlToDpcLevel" => ke::runtime::ke_raise_irql_to_dpc_level as usize,
  "KeRaiseIrqlToSynchLevel" => ke::runtime::ke_raise_irql_to_synch_level as usize,
  "KeAcquireSpinLockRaiseToSynch" => ke::runtime::ke_acquire_spin_lock_raise_to_synch as usize,
  "KeLowerIrql" => ke::runtime::ke_lower_irql as usize,
  "KeAcquireSpinLock" => ke::runtime::ke_acquire_spin_lock as usize,
  "KeAcquireSpinLockRaiseToDpc" => ke::runtime::ke_acquire_spin_lock_raise_to_dpc as usize,
  "KeReleaseSpinLock" => ke::runtime::ke_release_spin_lock as usize,
  "KeAcquireSpinLockAtDpcLevel" => ke::runtime::ke_acquire_spin_lock_at_dpc_level as usize,
  "KeReleaseSpinLockFromDpcLevel" => ke::runtime::ke_release_spin_lock_from_dpc_level as usize,
  "KeTryToAcquireSpinLockAtDpcLevel" => ke::runtime::ke_try_to_acquire_spin_lock_at_dpc_level as usize,
  "KeQueryMaximumProcessorCountEx" => ke::runtime::ke_query_maximum_processor_count_ex as usize,
  "KeQueryMaximumProcessorCount" => ke::runtime::ke_query_maximum_processor_count as usize,
  "KeGetCurrentNodeNumber" => ke::runtime::ke_get_current_node_number as usize,
  "KeGetRecommendedSharedDataAlignment" => ke::runtime::ke_get_recommended_shared_data_alignment as usize,
  "KeInitializeSpinLock" => ke::runtime::ke_initialize_spin_lock as usize,
  "KeTestSpinLock" => ke::runtime::ke_test_spin_lock as usize,
  "KeFlushWriteBuffer" => ke::runtime::ke_flush_write_buffer as usize,
  "KeInitializeEvent" => ke::runtime::ke_initialize_event as usize,
  "KeClearEvent" => ke::runtime::ke_clear_event as usize,
  "KeResetEvent" => ke::runtime::ke_reset_event as usize,
  "KeReadStateEvent" => ke::runtime::ke_read_state_event as usize,
  "KeSetEvent" => ke::runtime::ke_set_event as usize,
  "KeInitializeSemaphore" => ke::runtime::ke_initialize_semaphore as usize,
  "KeReadStateSemaphore" => ke::runtime::ke_read_state_semaphore as usize,
  "KeReleaseSemaphore" => ke::runtime::ke_release_semaphore as usize,
  "KeWaitForSingleObject" => ke::runtime::ke_wait_for_single_object as usize,
  "KeWaitForMultipleObjects" => ke::runtime::ke_wait_for_multiple_objects as usize,
  "KeDelayExecutionThread" => ke::runtime::ke_delay_execution_thread as usize,
  "HalAcpiGetTableEx" => crate::hal::exports::hal_acpi_get_table_ex as usize,
  "HalQueryRealTimeClock" => crate::hal::exports::hal_query_real_time_clock as usize,
  "HalRequestIpi" => crate::hal::exports::hal_request_ipi as usize,
  "HalRequestIpiSpecifyVector" => crate::hal::exports::hal_request_ipi_specify_vector as usize,
  "HalAllProcessorsStarted" => crate::hal::exports::hal_all_processors_started as usize,
  "HalDisplayString" => crate::hal::exports::hal_display_string as usize,
  "HalGetAdapter" => crate::hal::exports::hal_get_adapter as usize,\n  "HalAllocateCommonBuffer" => crate::hal::exports::hal_allocate_common_buffer as usize,
  "HalFlushCommonBuffer" => crate::hal::exports::hal_flush_common_buffer as usize,
  "HalFreeCommonBuffer" => crate::hal::exports::hal_free_common_buffer as usize,
  "HalGetBusData" => crate::hal::exports::hal_get_bus_data as usize,
  "HalGetBusDataByOffset" => crate::hal::exports::hal_get_bus_data_by_offset as usize,
  "HalSetBusData" => crate::hal::exports::hal_set_bus_data as usize,
  "HalSetBusDataByOffset" => crate::hal::exports::hal_set_bus_data_by_offset as usize,
  "HalTranslateBusAddress" => crate::hal::exports::hal_translate_bus_address as usize,
  "HalGetProcessorIdByNtNumber" => crate::hal::exports::hal_get_processor_id_by_nt_number as usize,
  "HalIsHyperThreadingEnabled" => crate::hal::exports::hal_is_hyper_threading_enabled as usize,

  "ExAllocatePoolWithTag" => ex::pool::ex_allocate_pool_with_tag as usize,
  "ExAllocatePool2" => ex::pool::ex_allocate_pool2 as usize,
  "ExAllocatePool3" => ex::pool::ex_allocate_pool3 as usize,
  "ExAllocatePoolWithTagPriority" => ex::pool::ex_allocate_pool_with_tag_priority as usize,
  "ExFreePoolWithTag" => ex::pool::ex_free_pool_with_tag as usize,
  "ExFreePool" => ex::pool::ex_free_pool as usize,
  "ExFreePool2" => ex::pool::ex_free_pool2 as usize,
  "ExQueryPoolBlockSize" => ex::pool::ex_query_pool_block_size as usize,
  "KeQueryActiveProcessorCountEx" => ke::runtime::ke_query_active_processor_count_ex as usize,
  "KeQueryActiveProcessorCount" => ke::runtime::ke_query_active_processor_count as usize,
  "KeQueryActiveGroupCount" => ke::runtime::ke_query_active_group_count as usize,
  "KeGetCurrentProcessorNumberEx" => ke::runtime::ke_get_current_processor_number_ex as usize,
  "KeGetCurrentProcessorIndex" => ke::runtime::ke_get_current_processor_index as usize,
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
  "RtlCopyVolatileMemory" => rtl_runtime::rtl_copy_volatile_memory as usize,
  "RtlMoveVolatileMemory" => rtl_runtime::rtl_move_volatile_memory as usize,
  "RtlZeroVolatileMemory" => rtl_runtime::rtl_zero_volatile_memory as usize,
  "RtlFillMemoryUlong" => rtl_runtime::rtl_fill_memory_ulong as usize,
  "RtlFillMemoryUlonglong" => rtl_runtime::rtl_fill_memory_ulonglong as usize,
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
  "RtlFreeUnicodeString" => rtl_runtime::rtl_free_unicode_string as usize,
  "RtlCreateUnicodeString" => rtl_runtime::rtl_create_unicode_string as usize,
  "RtlCreateUnicodeStringFromAsciiz" => rtl_runtime::rtl_create_unicode_string_from_asciiz as usize,
  "RtlFreeAnsiString" => rtl_runtime::rtl_free_ansi_string as usize,
  "RtlFreeOemString" => rtl_runtime::rtl_free_oem_string as usize,
  "RtlFreeUTF8String" => rtl_runtime::rtl_free_utf8_string as usize,
  "RtlGetEnabledExtendedFeatures" => rtl_runtime::rtl_get_enabled_extended_features as usize,
  "RtlGetProductInfo" => rtl_runtime::rtl_get_product_info as usize,
  "RtlQueryElevationFlags" => rtl_runtime::rtl_query_elevation_flags as usize,
  "RtlLocalTimeToSystemTime" => rtl_runtime::rtl_local_time_to_system_time as usize,
  "RtlSystemTimeToLocalTime" => rtl_runtime::rtl_system_time_to_local_time as usize,
  "RtlIsBitmapClear" => rtl_runtime::rtl_is_bitmap_clear as usize,
  "RtlIsBitmapEmptyEx" => rtl_runtime::rtl_is_bitmap_empty_ex as usize,
  "RtlFindNextForwardRunClearCapped" => rtl_runtime::rtl_find_next_forward_run_clear_capped as usize,
  "RtlIsNtDdiVersionAvailableEx" => rtl_runtime::rtl_is_nt_ddi_version_available_ex as usize,
  "RtlIsValidOemCharacterEx" => rtl_runtime::rtl_is_valid_oem_character_ex as usize,
  // All other names deliberately fail closed until their own signatures and semantics are implemented.
  "RtlCreateAcl" => rtl_runtime::rtl_create_acl as usize,
  "RtlValidAcl" => rtl_runtime::rtl_valid_acl as usize,
  "RtlGetAce" => rtl_runtime::rtl_get_ace as usize,
  "RtlFirstFreeAce" => rtl_runtime::rtl_first_free_ace as usize,
  "RtlDeleteAce" => rtl_runtime::rtl_delete_ace as usize,
  "RtlAddAccessAllowedAce" => rtl_runtime::rtl_add_access_allowed_ace as usize,
  "RtlAddAccessAllowedAceEx" => rtl_runtime::rtl_add_access_allowed_ace_ex as usize,
  "RtlAddAccessDeniedAceEx" => rtl_runtime::rtl_add_access_denied_ace_ex as usize,
  "RtlCreateSecurityDescriptor" => rtl_runtime::rtl_create_security_descriptor as usize,
  "RtlValidSecurityDescriptor" => rtl_runtime::rtl_valid_security_descriptor as usize,
  "RtlLengthSecurityDescriptor" => rtl_runtime::rtl_length_security_descriptor as usize,
  "RtlGetControlSecurityDescriptor" => rtl_runtime::rtl_get_control_security_descriptor as usize,
  "RtlGetDaclSecurityDescriptor" => rtl_runtime::rtl_get_dacl_security_descriptor as usize,
  "RtlGetSaclSecurityDescriptor" => rtl_runtime::rtl_get_sacl_security_descriptor as usize,
  "RtlGetOwnerSecurityDescriptor" => rtl_runtime::rtl_get_owner_security_descriptor as usize,
  "RtlGetGroupSecurityDescriptor" => rtl_runtime::rtl_get_group_security_descriptor as usize,
  "RtlSetDaclSecurityDescriptor" => rtl_runtime::rtl_set_dacl_security_descriptor as usize,
  "RtlSetSaclSecurityDescriptor" => rtl_runtime::rtl_set_sacl_security_descriptor as usize,
  "RtlSetOwnerSecurityDescriptor" => rtl_runtime::rtl_set_owner_security_descriptor as usize,
  "RtlSetGroupSecurityDescriptor" => rtl_runtime::rtl_set_group_security_descriptor as usize,
  "RtlOwnerAcesPresent" => rtl_runtime::rtl_owner_aces_present as usize,
  "RtlGetAcesBufferSize" => rtl_runtime::rtl_get_aces_buffer_size as usize,
  "RtlSetControlSecurityDescriptor" => rtl_runtime::rtl_set_control_security_descriptor as usize,
  "RtlFindAceByType" => rtl_runtime::rtl_find_ace_by_type as usize,
  "RtlAddAccessAllowedObjectAce" => rtl_runtime::rtl_add_access_allowed_object_ace as usize,
  "RtlAddAccessDeniedObjectAce" => rtl_runtime::rtl_add_access_denied_object_ace as usize,
  "RtlAddAuditAccessObjectAce" => rtl_runtime::rtl_add_audit_access_object_ace as usize,
  "RtlAddAuditAccessAceEx" => rtl_runtime::rtl_add_audit_access_ace_ex as usize,
  "RtlAddMandatoryAce" => rtl_runtime::rtl_add_mandatory_ace as usize,
  "RtlAddProcessTrustLabelAce" => rtl_runtime::rtl_add_process_trust_label_ace as usize,
  "RtlInitStringEx" => rtl_runtime::rtl_init_string_ex as usize,
  "RtlInitUTF8String" => rtl_runtime::rtl_init_utf8_string as usize,
  "RtlInitUTF8StringEx" => rtl_runtime::rtl_init_utf8_string_ex as usize,
  "RtlUTF8ToUnicodeN" => rtl_runtime::rtl_utf8_to_unicode_n as usize,
  "RtlUnicodeToUTF8N" => rtl_runtime::rtl_unicode_to_utf8_n as usize,
  "RtlUTF8StringToUnicodeString" => rtl_runtime::rtl_utf8_string_to_unicode_string as usize,
  "RtlUnicodeStringToUTF8String" => rtl_runtime::rtl_unicode_string_to_utf8_string as usize,
  "RtlOemToUnicodeN" => rtl_runtime::rtl_oem_to_unicode_n as usize,
  "RtlUnicodeToOemN" => rtl_runtime::rtl_unicode_to_oem_n as usize,
  "RtlUpcaseUnicodeToOemN" => rtl_runtime::rtl_upcase_unicode_to_oem_n as usize,
  "RtlUpcaseUnicodeToMultiByteN" => rtl_runtime::rtl_upcase_unicode_to_multi_byte_n as usize,
  "RtlUpcaseUnicodeStringToOemString" => rtl_runtime::rtl_upcase_unicode_string_to_oem_string as usize,
  "RtlUpcaseUnicodeStringToCountedOemString" => rtl_runtime::rtl_upcase_unicode_string_to_counted_oem_string as usize,
  "RtlUnicodeStringToCountedOemString" => rtl_runtime::rtl_unicode_string_to_counted_oem_string as usize,
  "RtlOemStringToCountedUnicodeString" => rtl_runtime::rtl_oem_string_to_counted_unicode_string as usize,
  "RtlOemStringToUnicodeString" => rtl_runtime::rtl_oem_string_to_unicode_string as usize,
  "RtlUnicodeStringToOemString" => rtl_runtime::rtl_unicode_string_to_oem_string as usize,
  "RtlUpcaseUnicodeToCustomCPN" => rtl_runtime::rtl_upcase_unicode_to_custom_cpn as usize,
  "RtlUnicodeToCustomCPN" => rtl_runtime::rtl_unicode_to_custom_cpn as usize,
  "RtlIpv4StringToAddressA" => rtl_runtime::rtl_ipv4_string_to_address_a as usize,
  "RtlIpv4StringToAddressW" => rtl_runtime::rtl_ipv4_string_to_address_w as usize,
  "RtlIpv4StringToAddressExA" => rtl_runtime::rtl_ipv4_string_to_address_ex_a as usize,
  "RtlIpv4StringToAddressExW" => rtl_runtime::rtl_ipv4_string_to_address_ex_w as usize,
  "RtlIpv4AddressToStringExA" => rtl_runtime::rtl_ipv4_address_to_string_ex_a as usize,
  "RtlIpv4AddressToStringExW" => rtl_runtime::rtl_ipv4_address_to_string_ex_w as usize,
  "RtlEthernetStringToAddressA" => rtl_runtime::rtl_ethernet_string_to_address_a as usize,
  "RtlEthernetStringToAddressW" => rtl_runtime::rtl_ethernet_string_to_address_w as usize,
  "RtlRunOnceInitialize" => rtl_runtime::rtl_run_once_initialize as usize,
  "RtlRunOnceBeginInitialize" => rtl_runtime::rtl_run_once_begin_initialize as usize,
  "RtlRunOnceComplete" => rtl_runtime::rtl_run_once_complete as usize,
  "RtlRunOnceExecuteOnce" => rtl_runtime::rtl_run_once_execute_once as usize,
  "RtlMapGenericMask" => rtl_runtime::rtl_map_generic_mask as usize,
  "RtlMapSecurityErrorToNtStatus" => rtl_runtime::rtl_map_security_error_to_nt_status as usize,
  "RtlIsApiSetImplemented" => rtl_runtime::rtl_is_api_set_implemented as usize,
  "RtlIsFunctionalityAvailable" => rtl_runtime::rtl_is_functionality_available as usize,
  "RtlIsNameLegalDOS8Dot3" => rtl_runtime::rtl_is_name_legal_dos8_dot3 as usize,
  "RtlGetVersion" => rtl_runtime::rtl_get_version as usize,
  "RtlValidRelativeSecurityDescriptor" => rtl_runtime::rtl_valid_relative_security_descriptor as usize,
  "RtlGetDefaultCodePage" => rtl_runtime::rtl_get_default_code_page as usize,
  "RtlIsNonEmptyDirectoryReparsePointAllowed" => rtl_runtime::rtl_is_non_empty_directory_reparse_point_allowed as usize,
  "RtlIsPartialPlaceholder" => rtl_runtime::rtl_is_partial_placeholder as usize,
  "RtlIsPartialPlaceholderFileHandle" => rtl_runtime::rtl_is_partial_placeholder_file_handle as usize,
  "RtlInterlockedSetBitRun" => rtl_runtime::rtl_interlocked_set_bit_run as usize,
  "RtlInterlockedClearBitRun" => rtl_runtime::rtl_interlocked_clear_bit_run as usize,
  "RtlInterlockedSetBitRunEx" => rtl_runtime::rtl_interlocked_set_bit_run_ex as usize,
  "RtlInterlockedClearBitRunEx" => rtl_runtime::rtl_interlocked_clear_bit_run_ex as usize,
  "RtlFindClearRuns" => rtl_runtime::bitmap_abi::rtl_find_clear_runs as usize,
  "RtlCopyLuidAndAttributesArray" => rtl_runtime::rtl_copy_luid_and_attributes_array as usize,
  "RtlConvertSidToUnicodeString" => rtl_runtime::rtl_convert_sid_to_unicode_string as usize,
  "RtlEqualWnfChangeStamps" => rtl_runtime::rtl_equal_wnf_change_stamps as usize,
  "RtlImageNtHeader" => rtl_runtime::rtl_image_nt_header as usize,
  "RtlImageNtHeaderEx" => rtl_runtime::rtl_image_nt_header_ex as usize,
  "RtlImageDirectoryEntryToData" => rtl_runtime::rtl_image_directory_entry_to_data as usize,
  "RtlVerifyVersionInfo" => rtl_runtime::rtl_verify_version_info as usize,
  "RtlIsGenericTableEmpty" => rtl_runtime::rtl_is_generic_table_empty as usize,
  "RtlIsGenericTableEmptyAvl" => rtl_runtime::rtl_is_generic_table_empty_avl as usize,
  "RtlNumberGenericTableElements" => rtl_runtime::rtl_number_generic_table_elements as usize,
  "RtlNumberGenericTableElementsAvl" => rtl_runtime::rtl_number_generic_table_elements_avl as usize,
  "RtlSetPortableOperatingSystem" => rtl_runtime::rtl_set_portable_operating_system as usize,
  "RtlSetConsoleSessionForegroundProcessId" => rtl_runtime::rtl_set_console_session_foreground_process_id as usize,
  "RtlGetConsoleSessionForegroundProcessId" => rtl_runtime::rtl_get_console_session_foreground_process_id as usize,
  "RtlSetProcessPlaceholderCompatibilityMode" => rtl_runtime::rtl_set_process_placeholder_compatibility_mode as usize,
  "RtlQueryProcessPlaceholderCompatibilityMode" => rtl_runtime::rtl_query_process_placeholder_compatibility_mode as usize,
  "RtlSetThreadPlaceholderCompatibilityMode" => rtl_runtime::rtl_set_thread_placeholder_compatibility_mode as usize,
  "RtlQueryThreadPlaceholderCompatibilityMode" => rtl_runtime::rtl_query_thread_placeholder_compatibility_mode as usize,
  "RtlGetNtSystemRoot" => rtl_runtime::rtl_get_nt_system_root as usize,
  "RtlAbsoluteToSelfRelativeSD" => rtl_runtime::rtl_absolute_to_self_relative_sd as usize,
  "RtlSelfRelativeToAbsoluteSD" => rtl_runtime::rtl_self_relative_to_absolute_sd as usize,
  "RtlAddAce" => rtl_runtime::rtl_add_ace as usize,
  "RtlCopySidAndAttributesArray" => rtl_runtime::rtl_copy_sid_and_attributes_array as usize,
  "RtlIsCloudFilesPlaceholder" => rtl_runtime::rtl_is_cloud_files_placeholder as usize,
  "RtlIsPartialPlaceholderFileInfo" => rtl_runtime::rtl_is_partial_placeholder_file_info as usize,

  "RtlExtractBitMap" => rtl_runtime::rtl_extract_bit_map as usize,
  "RtlExtractBitMapEx" => rtl_runtime::rtl_extract_bit_map_ex as usize,
  "RtlIpv6AddressToStringA" => rtl_runtime::rtl_ipv6_address_to_string_a as usize,
  "RtlIpv6AddressToStringW" => rtl_runtime::rtl_ipv6_address_to_string_w as usize,
  "RtlIpv6AddressToStringExA" => rtl_runtime::rtl_ipv6_address_to_string_ex_a as usize,
  "RtlIpv6AddressToStringExW" => rtl_runtime::rtl_ipv6_address_to_string_ex_w as usize,
  "RtlIpv6StringToAddressA" => rtl_runtime::rtl_ipv6_string_to_address_a as usize,
  "RtlIpv6StringToAddressW" => rtl_runtime::rtl_ipv6_string_to_address_w as usize,
  "RtlIpv6StringToAddressExA" => rtl_runtime::rtl_ipv6_string_to_address_ex_a as usize,
  "RtlIpv6StringToAddressExW" => rtl_runtime::rtl_ipv6_string_to_address_ex_w as usize,
  "RtlUnicodeStringToOemSize" => rtl_runtime::rtl_unicode_string_to_oem_size as usize,
  "RtlOemStringToUnicodeSize" => rtl_runtime::rtl_oem_string_to_unicode_size as usize,
  "RtlxUnicodeStringToOemSize" => rtl_runtime::rtlx_unicode_string_to_oem_size as usize,
  "RtlxOemStringToUnicodeSize" => rtl_runtime::rtlx_oem_string_to_unicode_size as usize,
  "RtlDuplicateUnicodeString" => rtl_runtime::rtl_duplicate_unicode_string as usize,
  "RtlStringFromGUIDEx" => rtl_runtime::rtl_string_from_guid_ex as usize,
  "RtlGetEnabledExtendedAndSupervisorFeatures" => rtl_runtime::rtl_get_enabled_extended_and_supervisor_features as usize,
  "RtlPrefetchMemoryNonTemporal" => rtl_runtime::rtl_prefetch_memory_non_temporal as usize,
  "RtlInterlockedSetClearRun" => rtl_runtime::rtl_interlocked_set_clear_run as usize,
  _ => return None,
 };
 Some(address as u64)
}

/// Configure the fixed Windows-style time-zone bias used by RTL conversions.
pub fn set_time_zone_bias_minutes(bias_minutes: i32) -> bool {
 rtl_runtime::set_time_zone_bias_minutes(bias_minutes)
}

pub fn named_export_count() -> usize { exports().count() }
pub fn ordinal_only_export_count() -> usize { manifest_26100::ORDINAL_ONLY.len() }
pub fn self_test() -> bool {
 named_export_count()==3352 && ordinal_only_export_count()==37 && exports().all(|e|!e.name.is_empty())
 && resolve(b"ntoskrnl.exe",ImportSymbol::Ordinal(1)).is_none()
 && resolve(b"kernel32.dll",ImportSymbol::Ordinal(1)).is_none()
 && resolve(b"ntoskrnl.exe",ImportSymbol::Name(b"KeGetCurrentIrql")).is_some()
 && resolve(b"ntoskrnl.exe",ImportSymbol::Name(b"HalGetBusData")).is_some()
 && resolve(b"ntoskrnl.exe",ImportSymbol::Name(b"HalGetBusDataByOffset")).is_some()
 && resolve(b"ntoskrnl.exe",ImportSymbol::Name(b"HalSetBusData")).is_some()
 && resolve(b"ntoskrnl.exe",ImportSymbol::Name(b"HalSetBusDataByOffset")).is_some()
 && resolve(b"ntoskrnl.exe",ImportSymbol::Name(b"HalGetProcessorIdByNtNumber")).is_some()
 && resolve(b"ntoskrnl.exe",ImportSymbol::Name(b"HalIsHyperThreadingEnabled")).is_some()
 && resolve(b"ntoskrnl.exe",ImportSymbol::Name(b"ExAllocatePool2")).is_some()
 && resolve(b"ntoskrnl.exe",ImportSymbol::Name(b"ExAllocatePool3")).is_some()
 && resolve(b"ntoskrnl.exe",ImportSymbol::Name(b"ExFreePool2")).is_some()
 && resolve(b"hal.dll",ImportSymbol::Name(b"HalQueryMaximumProcessorCount")).is_some()
 && resolve(b"hal.dll",ImportSymbol::Ordinal(47)).is_some()
 && resolve(b"hal.dll",ImportSymbol::Name(b"HalAcpiGetTableEx")).is_some()
 && resolve(b"hal.dll",ImportSymbol::Name(b"HalQueryRealTimeClock")).is_some()
 && resolve(b"hal.dll",ImportSymbol::Name(b"HalTranslateBusAddress")).is_some()
 && resolve(b"hal.dll",ImportSymbol::Ordinal(1)).is_some()
 && resolve(b"hal.dll",ImportSymbol::Ordinal(48)).is_some()
 // Windows x64 exposes KeQueryPerformanceCounter through HAL imports and,
 // on modern x64 releases, also through the kernel. Both import paths must
 // converge on one implementation so counter epoch/frequency cannot diverge.
 && (resolve(b"hal.dll",ImportSymbol::Name(b"KeQueryPerformanceCounter"))
     == crate::apic::tsc_frequency_hz().map(|_| ke::runtime::ke_query_performance_counter as usize as u64))
 && (resolve(b"ntoskrnl.exe",ImportSymbol::Name(b"KeQueryPerformanceCounter"))
     == crate::apic::tsc_frequency_hz().map(|_| ke::runtime::ke_query_performance_counter as usize as u64))
 && resolve(b"ntoskrnl.exe",ImportSymbol::Name(b"KeFlushWriteBuffer")).is_some()
 && (resolve(b"ntoskrnl.exe",ImportSymbol::Name(b"KeStallExecutionProcessor")).is_some()
     == crate::apic::tsc_frequency_hz().is_some())
 && resolve(b"ntoskrnl.exe",ImportSymbol::Name(b"ExAllocatePoolWithTagPriority")).is_some()
 && resolve(b"ntoskrnl.exe",ImportSymbol::Name(b"ExAllocatePoolWithTag")).is_some()
 && resolve(b"ntoskrnl.exe",ImportSymbol::Name(b"ExFreePoolWithTag")).is_some()
 && resolve(b"ntoskrnl.exe",ImportSymbol::Name(b"ExFreePool")).is_some()
 && resolve(b"ntoskrnl.exe",ImportSymbol::Name(b"ExQueryPoolBlockSize")).is_some()
 && resolve(b"ntoskrnl.exe",ImportSymbol::Name(b"RtlAreBitsClear")).is_some()
 && resolve(b"ntoskrnl.exe",ImportSymbol::Name(b"RtlComputeCrc32")).is_some()
 && resolve(b"ntoskrnl.exe",ImportSymbol::Name(b"RtlFillMemory")).is_some()
 && abi_signatures::documented_signature("KeGetCurrentIrql").is_some()
}
