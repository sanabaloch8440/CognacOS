#!/usr/bin/env python3
"""Focused regression tests for QEMU crash-address resolution."""
import importlib.util
from pathlib import Path
import unittest

MODULE_PATH = Path(__file__).with_name("resolve_qemu_trace.py")
SPEC = importlib.util.spec_from_file_location("resolve_qemu_trace", MODULE_PATH)
resolver = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(resolver)


class ResolverTests(unittest.TestCase):
    def test_accepts_source_line_with_zero_column(self):
        self.assertTrue(resolver.useful_source_line(
            "RtlIntegerToUnicodeString\n/home/runner/ntdll/src/native/rtl_string.rs:398:0"))

    def test_rejects_unknown_addr2line_records(self):
        self.assertFalse(resolver.useful_source_line("probe_symbol\n??:0"))
        self.assertFalse(resolver.useful_source_line("probe_symbol\n??:?"))
        self.assertFalse(resolver.useful_source_line("probe_symbol only"))

    def test_exec_section_range_is_half_open(self):
        ranges = [(0x1000, 0x1100, ".text")]
        self.assertEqual(resolver.in_ranges(0x1000, ranges), ".text")
        self.assertEqual(resolver.in_ranges(0x10ff, ranges), ".text")
        self.assertIsNone(resolver.in_ranges(0x1100, ranges))
        self.assertIsNone(resolver.in_ranges(0x0fff, ranges))

    def test_nonmatching_pc_does_not_call_elf_fallback(self):
        # The path need not exist: an out-of-range PC must be rejected before
        # addr2line/GDB are invoked, preventing unrelated-image disassembly.
        self.assertEqual(
            resolver.resolve_elf(Path("/does/not/exist"), 0xdeadbeef,
                                 [(0x1000, 0x2000, ".text")]),
            "")


if __name__ == "__main__":
    unittest.main()
