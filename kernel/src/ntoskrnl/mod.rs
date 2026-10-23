//! NTOSKRNL export catalogue and safe callable-import dispatch.
//! Export metadata targets Windows 11 x64 10.0.26100.8036-AMD64; most exports remain unsupported.
use crate::pe::ImportSymbol;

pub const STATUS_NOT_IMPLEMENTED: u32 = 0xC000_0002;
pub const STATUS_PROCEDURE_NOT_FOUND: u32 = 0xC000_007A;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KernelModule { Ex, Ke, Mm, Ob, Ps, Io, Cm, Se, Rtl, Zw, Other }
#[derive(Clone, Copy, Debug)]
pub struct Export { pub name: &'static str, pub ordinal: u16, pub module: KernelModule }

mod ex; mod ke; mod mm; mod ob; mod ps; mod io; mod cm; mod se; mod rtl; mod rtl_runtime; mod zw; mod other;
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
