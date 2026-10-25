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


class SyscallStubTests(unittest.TestCase):
    def test_extracts_canonical_stub(self):
        image = bytearray(64)
        image[8:21] = bytes.fromhex("4c 8b d1 b8 34 12 00 00 0f 05 c3 90 90")
        rows = helper.extract_syscalls(bytes(image), {"NtClose": 8}, ["NtClose"])
        self.assertEqual(rows, [("NtClose", "NTCLOSE", 0x1234)])

    def test_accepts_windows_shared_user_data_check(self):
        image = bytearray(64)
        image[8:26] = bytes.fromhex(
            "4c 8b d1 b8 0a 00 00 00 f6 04 25 08 03 fe 7f 01 75 03"
        )
        image[26:29] = bytes.fromhex("0f 05 c3")
        rows = helper.extract_syscalls(bytes(image), {"NtYieldExecution": 8})
        self.assertEqual(rows[0], ("NtYieldExecution", "NTYIELDEXECUTION", 10))

    def test_required_export_must_be_a_syscall_stub(self):
        image = bytearray(64)
        with self.assertRaisesRegex(helper.PEError, "not recognized syscall stubs"):
            helper.extract_syscalls(bytes(image), {"NtClose": 8}, ["NtClose"])

    def test_ignores_non_syscall_exports(self):
        image = bytearray(64)
        image[8:21] = bytes.fromhex("4c 8b d1 b8 01 00 00 00 c3 90 90 90 90")
        with self.assertRaisesRegex(helper.PEError, "no canonical"):
            helper.extract_syscalls(bytes(image), {"NtClose": 8})


if __name__ == "__main__":
    unittest.main()
