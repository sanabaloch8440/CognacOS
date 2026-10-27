#!/usr/bin/env python3
"""Source-contract checks for CognacOS Windows XSTATE integration."""
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
xstate = (ROOT / "kernel/src/ntoskrnl/ke/runtime/xstate.rs").read_text()
address_space = (ROOT / "kernel/src/ntoskrnl/mm/address_space.rs").read_text()
user_range = (ROOT / "kernel/src/ntoskrnl/mm/user_range.rs").read_text()
process = (ROOT / "kernel/src/process.rs").read_text()
syscall = (ROOT / "kernel/src/syscall.rs").read_text()
arch = (ROOT / "kernel/src/ntoskrnl/ke/arch.rs").read_text()
ntdll = (ROOT / "ntdll/src/native/exception.rs").read_text()

assert "const _: [(); 0x348] = [(); core::mem::size_of::<WindowsXStateConfiguration>()]" in xstate
assert "const _: [(); 0x348] = [(); core::mem::size_of::<WindowsXStateConfiguration>()]" in ntdll
assert "KUSER_XSTATE_OFFSET: u64 = 0x3d8" in process
assert "KUSER_SHARED_DATA_ADDRESS: u64 = crate::ntoskrnl::mm::address_space::KUSER_SHARED_DATA_ADDRESS" in process
assert "KUSER_SHARED_DATA_ADDRESS: u64 = 0x0000_0000_7ffe_0000" in user_range
assert "pub unsafe fn map_kuser_shared_page" in address_space
assert "map_kuser_shared_page(space)?" in process
assert "record_kuser_tables(self.root, self.kuser_tables)" in address_space
assert "self.kuser_tables = [0; 3]" in address_space
assert "let enabled = XSTATE_X87 | XSTATE_SSE" in xstate
assert "enabled_supervisor_features: 0" in xstate
assert "struct XStateSaveRecord" in xstate and "core::mem::size_of::<XStateSaveRecord>()" in xstate
assert "const _: [(); 0x38] = [(); core::mem::size_of::<XStateSaveRecord>()]" in xstate
assert "valid_xstate_save_record(save)" in xstate
assert "mm_is_non_paged_system_address_writable" in xstate
assert "Do not clear this buffer with Rust before XSAVE" in xstate
assert "xstate_policy_compatible" in xstate
assert "ki_xstate_nested_save_restore_self_test" in xstate
assert "RtlCopyContext" in ntdll and "RtlCopyExtendedContext" in ntdll
assert "RtlLocateExtendedFeature2" in ntdll
assert "config as *const WindowsXStateConfiguration).read_unaligned()" in ntdll

def check_entry(source: str, entry: str, dispatch: str, save: str, restore: str, ret: str) -> None:
    start = source.index(entry)
    end = source.find('"#);', start)
    if end < 0:
        end = len(source)
    body = source[start:end]
    assert body.index(save) < body.index(dispatch), f"{entry}: user XSTATE must be saved before Rust dispatch"
    assert body.index(dispatch) < body.index(restore), f"{entry}: user XSTATE must be restored after dispatch"
    assert body.index(restore) < body.index(ret), f"{entry}: restore must precede return"

check_entry(syscall, "cognacos_windows_syscall_entry:", "call cognacos_windows_syscall_dispatch",
            "call cognacos_save_current_user_xstate_asm", "call cognacos_current_user_xstate_ptr", "pop r15")
check_entry(syscall, "rdotos_syscall_entry:", "call rdotos_syscall_dispatch",
            "call cognacos_save_current_user_xstate_asm", "call cognacos_current_user_xstate_ptr", "pop r15")
check_entry(arch, "cognacos_exception_common:", "call cognacos_exception_dispatch",
            "call cognacos_save_current_user_xstate_asm", "call cognacos_current_user_xstate_ptr", "pop r15")
check_entry(arch, "cognacos_interrupt_common:", "call cognacos_external_interrupt_dispatch",
            "call cognacos_save_current_user_xstate_asm", "call cognacos_current_user_xstate_ptr", "pop r15")

print("XSTATE source contracts passed: Windows shared ABI, opaque save storage, user trap/syscall ordering, feature policy, and context APIs.")
