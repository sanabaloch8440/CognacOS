#!/usr/bin/env python3
"""Validate every PE32+ driver image and its import contract. Native driver images may import explicitly enumerated ntoskrnl exports; the loader resolves these before DriverEntry."""
import struct
from pathlib import Path

EXPECTED_IMPORTS = {
    "ataport.sys": {
        "ntoskrnl.exe": [
            "IoCompleteRequest", "IoCreateDevice", "IoDeleteDevice",
            "IoAttachDeviceToDeviceStack", "IoDetachDevice", "IoCallDriver",
            "IoCopyCurrentIrpStackLocationToNext", "IoSkipCurrentIrpStackLocation",
            "IoSetCompletionRoutine", "IoPnPQueryBusInterface",
            "IoPnPRegisterDeviceDriver", "IoPnPEnumerateBus",
        ],
    },
    "disk.sys": {
        "ntoskrnl.exe": [
            "IoCreateDevice", "IoDeleteDevice", "IoAttachDeviceToDeviceStack",
            "IoDetachDevice", "IoCallDriver", "IoSkipCurrentIrpStackLocation",
            "IoSetCompletionRoutine", "IoPnPRegisterDeviceDriver", "IoCompleteRequest",
        ],
    },
    "partmgr.sys": {
        "ntoskrnl.exe": [
            "IoCreateDevice", "IoDeleteDevice", "IoAttachDeviceToDeviceStack",
            "IoDetachDevice", "IoCallDriver", "IoSkipCurrentIrpStackLocation",
            "IoCopyCurrentIrpStackLocationToNext", "IoSetCompletionRoutine",
            "IoAllocateIrp", "IoGetNextIrpStackLocation", "IoFreeIrp",
            "IoCompleteRequest", "IoPnPRegisterDeviceDriver",
            "IoPnPEnumerateBus", "IoMountFAT32Volume",
        ],
    },
    "pciidex.sys": {
        "ntoskrnl.exe": [
            "IoCreateDevice", "IoDeleteDevice", "IoAttachDeviceToDeviceStack",
            "IoDetachDevice", "IoCallDriver", "IoSkipCurrentIrpStackLocation",
            "IoSetCompletionRoutine", "IoPnPQueryBusInterface",
            "IoPnPRegisterDeviceDriver",
        ],
    },
    "pciide.sys": {
        "ntoskrnl.exe": [
            "IoCreateDevice", "IoDeleteDevice", "IoAttachDeviceToDeviceStack",
            "IoDetachDevice", "IoCallDriver", "IoSkipCurrentIrpStackLocation",
            "IoCopyCurrentIrpStackLocationToNext", "IoSetCompletionRoutine",
            "IoPnPQueryBusInterface", "IoPnPRegisterDeviceDriver",
            "IoAllocateIrp", "IoGetNextIrpStackLocation", "IoFreeIrp", "DbgPrint",
        ],
    },
    "pci.sys": {
        "ntoskrnl.exe": ["IoCreateDevice", "IoDeleteDevice", "ExAllocatePoolWithTag", "ExFreePoolWithTag"],
    },
    "acpi.sys": {
        "hal.dll": ["HalAcpiGetTableEx"],
    },
    "usbxhci.sys": {
        "ntoskrnl.exe": [
            "IoCreateDevice", "IoDeleteDevice", "IoGetDmaAdapter",
            "MmMapIoSpace", "MmUnmapIoSpace", "IoConnectInterrupt",
            "IoDisconnectInterrupt", "KeInitializeDpc", "KeInsertQueueDpc",
            "IoCompleteRequest",
        ],
        "hal.dll": [
            "HalEnableInterrupt", "HalDisableInterrupt",
            "HalAllocateCommonBuffer", "HalFreeCommonBuffer",
            "HalGetBusDataByOffset", "HalSetBusDataByOffset",
        ],
    },
}


def parse_imports(p, rva_to_offset, read_c_string, import_rva, import_size):
    imports = {}
    iat_rvas = []
    descriptor_offset = rva_to_offset(import_rva)
    descriptor_limit = descriptor_offset + import_size
    while descriptor_offset + 20 <= descriptor_limit:
        original_thunk, timestamp, forwarder, name_rva, first_thunk = struct.unpack_from(
            "<IIIII", p, descriptor_offset
        )
        descriptor_offset += 20
        if not any((original_thunk, timestamp, forwarder, name_rva, first_thunk)):
            break
        dll = read_c_string(rva_to_offset(name_rva)).lower()
        assert original_thunk and first_thunk, "import descriptor lacks lookup/IAT tables"
        iat_rvas.append(first_thunk)
        names = []
        index = 0
        while True:
            thunk = struct.unpack_from("<Q", p, rva_to_offset(original_thunk + index * 8))[0]
            if thunk == 0:
                break
            assert not (thunk & (1 << 63)), "ordinal imports are not expected"
            hint_name = rva_to_offset(thunk & 0xffffffff)
            names.append(read_c_string(hint_name + 2))
            index += 1
        imports[dll] = names
    return imports, iat_rvas


def main():
    for path in sorted(Path("build/drivers").iterdir()):
        p = path.read_bytes()
        assert p[:2] == b"MZ", f"{path}: missing MZ"
        pe = struct.unpack_from("<I", p, 0x3c)[0]
        assert p[pe:pe+4] == b"PE\0\0", f"{path}: missing PE"
        assert struct.unpack_from("<H", p, pe+4)[0] == 0x8664, f"{path}: wrong machine"
        assert struct.unpack_from("<H", p, pe+22)[0] & 0x2000, f"{path}: not a DLL image"
        opt = pe + 24
        assert struct.unpack_from("<H", p, opt)[0] == 0x20b, f"{path}: not PE32+"
        assert struct.unpack_from("<H", p, opt + 68)[0] == 1, f"{path}: not native subsystem"
        assert struct.unpack_from("<I", p, opt + 16)[0] != 0, f"{path}: no entry point"

        import_rva, import_size = struct.unpack_from("<II", p, opt + 112 + 8)
        section_count = struct.unpack_from("<H", p, pe + 6)[0]
        optional_size = struct.unpack_from("<H", p, pe + 20)[0]
        section_table = pe + 24 + optional_size

        def rva_to_offset(rva):
            for index in range(section_count):
                sh = section_table + index * 40
                virtual_size, virtual_address, raw_size, raw_offset = struct.unpack_from(
                    "<IIII", p, sh + 8
                )
                span = max(virtual_size, raw_size)
                if virtual_address <= rva < virtual_address + span:
                    return raw_offset + (rva - virtual_address)
            raise AssertionError(f"{path}: RVA 0x{rva:x} is not backed by a PE section")

        def read_c_string(offset):
            end = p.find(b"\0", offset)
            assert end >= 0, f"{path}: unterminated import string"
            return p[offset:end].decode("ascii")

        if path.name in EXPECTED_IMPORTS:
            assert import_rva and import_size >= 40, f"{path}: missing import directory"
            imports, iat_rvas = parse_imports(
                p, rva_to_offset, read_c_string, import_rva, import_size
            )
            expected = EXPECTED_IMPORTS[path.name]
            assert imports == expected, f"{path}: unexpected imports: {imports}"
            assert len(iat_rvas) == len(imports), (
                f"{path}: expected one IAT per imported DLL; "
                f"found {len(iat_rvas)} for {len(imports)} DLLs"
            )
            for first_thunk in iat_rvas:
                iat_section = next(
                    (
                        struct.unpack_from("<I", p, section_table + i * 40 + 36)[0]
                        for i in range(section_count)
                        if struct.unpack_from("<I", p, section_table + i * 40 + 12)[0]
                        <= first_thunk
                        < struct.unpack_from("<I", p, section_table + i * 40 + 12)[0]
                        + max(
                            struct.unpack_from("<I", p, section_table + i * 40 + 8)[0],
                            struct.unpack_from("<I", p, section_table + i * 40 + 16)[0],
                        )
                    ),
                    0,
                )
                assert iat_section & 0x80000000, (
                    f"{path}: IAT at RVA 0x{first_thunk:x} is not in writable data"
                )
        else:
            assert (import_rva, import_size) == (0, 0), (
                f"{path}: unexpected PE imports: {import_rva:#x}/{import_size}"
            )

        assert struct.unpack_from("<II", p, opt + 112 + 5 * 8) == (0, 0), (
            f"{path}: relocations unsupported by stage-1 loader"
        )
        print(f"validated native PE32+ driver: {path} ({len(p)} bytes)")


if __name__ == "__main__":
    main()
