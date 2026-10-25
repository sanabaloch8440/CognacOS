"""Unit tests for the compile-time NTDLL PE helper."""
import importlib.util
import pathlib
import unittest

MODULE_PATH = pathlib.Path(__file__).with_name("ntdll_link_helper.py")
SPEC = importlib.util.spec_from_file_location("ntdll_link_helper", MODULE_PATH)
helper = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(helper)

class PEParserTests(unittest.TestCase):
    def test_rejects_non_pe_data(self):
        with self.assertRaises(helper.PEError):
            helper.parse_pe(b"not a PE image")

    def test_rejects_truncated_mz_header(self):
        with self.assertRaises(helper.PEError):
            helper.parse_pe(b"MZ")

    def test_bounds_checked_integer_reads(self):
        with self.assertRaises(helper.PEError):
            helper.u16(b"\x00", 0)
        with self.assertRaises(helper.PEError):
            helper.u32(b"\x00\x00", 0)

if __name__ == "__main__":
    unittest.main()
