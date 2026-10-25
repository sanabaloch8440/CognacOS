/*
 * Minimal native LdrGetProcedureAddress for already-mapped PE32+ modules.
 * It resolves names and ordinals in a validated in-memory export directory.
 * Forwarded exports and loading/unloading dependencies are not yet supported.
 */
#include "cognac_ntdll.h"

typedef LONG NTSTATUS;
#define STATUS_SUCCESS ((NTSTATUS)0x00000000L)
#define STATUS_INVALID_PARAMETER ((NTSTATUS)0xC000000DL)
#define STATUS_INVALID_IMAGE_FORMAT ((NTSTATUS)0xC000007BL)
#define STATUS_PROCEDURE_NOT_FOUND ((NTSTATUS)0xC000007AL)
#define STATUS_DLL_NOT_FOUND ((NTSTATUS)0xC0000135L)

static USHORT pe_u16(const UCHAR *p)
{
    return (USHORT)((USHORT)p[0] | ((USHORT)p[1] << 8));
}

static ULONG pe_u32(const UCHAR *p)
{
    return (ULONG)p[0] | ((ULONG)p[1] << 8) |
           ((ULONG)p[2] << 16) | ((ULONG)p[3] << 24);
}

static int rva_range_ok(ULONG rva, ULONG length, ULONG image_size)
{
    return rva <= image_size && length <= image_size - rva;
}

static ULONG_PTR pe_u64(const UCHAR *p)
{
    return (ULONG_PTR)pe_u32(p) | ((ULONG_PTR)pe_u32(p + 4) << 32);
}

static WCHAR loader_fold_char(WCHAR value)
{
    if (value >= (WCHAR)'A' && value <= (WCHAR)'Z')
        return (WCHAR)(value + ('a' - 'A'));
    return value;
}

static PVOID loader_current_peb(void)
{
    PVOID peb;
    __asm__ volatile ("movq %%gs:0x60, %0" : "=r"(peb));
    return peb;
}

/* Find a DLL that is already present in the PEB load-order list. This does
 * not load images or resolve search paths; LdrLoadDll requires kernel file and
 * address-space services that are not yet exposed to native user mode. */
__declspec(dllexport)
NTSTATUS NTAPI LdrGetDllHandle(PVOID dll_path, PULONG dll_characteristics,
                               PCCOGNAC_UNICODE_STRING dll_name, PVOID *dll_handle)
{
    const UCHAR *peb, *ldr, *head;
    ULONG_PTR link, head_address;
    ULONG entries;
    USHORT i;
    (void)dll_path;
    (void)dll_characteristics;
    if (!dll_name || !dll_handle || (dll_name->Length & 1) ||
        (dll_name->Length && !dll_name->Buffer) || dll_name->Length > 512)
        return STATUS_INVALID_PARAMETER;
    *dll_handle = NULL;
    peb = (const UCHAR *)loader_current_peb();
    if (!peb) return STATUS_DLL_NOT_FOUND;
    ldr = (const UCHAR *)pe_u64(peb + 0x18);
    if (!ldr) return STATUS_DLL_NOT_FOUND;
    head = ldr + 0x10;
    head_address = (ULONG_PTR)head;
    link = pe_u64(head);
    for (entries = 0; link && link != head_address && entries < 64; ++entries) {
        const UCHAR *entry = (const UCHAR *)link;
        USHORT name_length = pe_u16(entry + 0x58);
        ULONG_PTR name_buffer = pe_u64(entry + 0x60);
        int equal = 1;
        if (!(name_length & 1) && name_length == dll_name->Length &&
            (!name_length || name_buffer)) {
            for (i = 0; i < name_length / sizeof(WCHAR); ++i) {
                WCHAR left = pe_u16((const UCHAR *)name_buffer + i * sizeof(WCHAR));
                WCHAR right = dll_name->Buffer[i];
                if (loader_fold_char(left) != loader_fold_char(right)) {
                    equal = 0;
                    break;
                }
            }
            if (equal) {
                ULONG_PTR base = pe_u64(entry + 0x30);
                if (!base) return STATUS_DLL_NOT_FOUND;
                *dll_handle = (PVOID)base;
                return STATUS_SUCCESS;
            }
        }
        link = pe_u64(entry);
    }
    return STATUS_DLL_NOT_FOUND;
}

__declspec(dllexport)
NTSTATUS NTAPI LdrGetProcedureAddress(PVOID dll_handle,
                                      PCCOGNAC_ANSI_STRING procedure_name,
                                      ULONG procedure_number,
                                      PVOID *procedure_address)
{
    const UCHAR *base = (const UCHAR *)dll_handle;
    const UCHAR *coff, *optional, *exports;
    ULONG pe_offset, optional_size, image_size;
    ULONG export_rva, export_size, ordinal_base;
    ULONG function_count, name_count, functions_rva, names_rva, ordinals_rva;
    ULONG functions, names, ordinals, function_index, function_rva;
    ULONG index;
    if (!base || !procedure_address) return STATUS_INVALID_PARAMETER;
    *procedure_address = NULL;
    if (pe_u16(base) != 0x5a4d) return STATUS_INVALID_IMAGE_FORMAT;
    pe_offset = pe_u32(base + 0x3c);
    if (pe_offset > 0x100000u) return STATUS_INVALID_IMAGE_FORMAT;
    if (pe_u32(base + pe_offset) != 0x00004550u) return STATUS_INVALID_IMAGE_FORMAT;
    coff = base + pe_offset + 4;
    if (pe_u16(coff) != 0x8664) return STATUS_INVALID_IMAGE_FORMAT;
    optional_size = pe_u16(coff + 16);
    optional = coff + 20;
    if (optional_size < 120 || pe_u16(optional) != 0x020b)
        return STATUS_INVALID_IMAGE_FORMAT;
    image_size = pe_u32(optional + 56);
    if (image_size < pe_offset + 24 + optional_size)
        return STATUS_INVALID_IMAGE_FORMAT;
    if (pe_u32(optional + 108) < 1) return STATUS_PROCEDURE_NOT_FOUND;
    export_rva = pe_u32(optional + 112);
    export_size = pe_u32(optional + 116);
    if (!export_rva || export_size < 40 ||
        !rva_range_ok(export_rva, export_size, image_size))
        return STATUS_PROCEDURE_NOT_FOUND;
    exports = base + export_rva;
    ordinal_base = pe_u32(exports + 16);
    function_count = pe_u32(exports + 20);
    name_count = pe_u32(exports + 24);
    functions_rva = pe_u32(exports + 28);
    names_rva = pe_u32(exports + 32);
    ordinals_rva = pe_u32(exports + 36);
    if (!function_count || function_count > image_size / 4 ||
        name_count > image_size / 4 ||
        !rva_range_ok(functions_rva, function_count * 4, image_size) ||
        !rva_range_ok(names_rva, name_count * 4, image_size) ||
        !rva_range_ok(ordinals_rva, name_count * 2, image_size))
        return STATUS_INVALID_IMAGE_FORMAT;
    functions = functions_rva;
    names = names_rva;
    ordinals = ordinals_rva;
    function_index = (ULONG)-1;
    if (procedure_name) {
        if (procedure_name->Length && !procedure_name->Buffer)
            return STATUS_INVALID_PARAMETER;
        for (index = 0; index < name_count; ++index) {
            ULONG name_rva = pe_u32(base + names + index * 4);
            ULONG length = 0;
            if (name_rva >= image_size) return STATUS_INVALID_IMAGE_FORMAT;
            while (length < image_size - name_rva && base[name_rva + length]) ++length;
            if (length == image_size - name_rva) return STATUS_INVALID_IMAGE_FORMAT;
            if (length != procedure_name->Length) continue;
            {
                ULONG character;
                for (character = 0; character < length; ++character)
                    if (base[name_rva + character] != (UCHAR)procedure_name->Buffer[character])
                        break;
                if (character == length) {
                    function_index = pe_u16(base + ordinals + index * 2);
                    break;
                }
            }
        }
    } else {
        if (procedure_number < ordinal_base) return STATUS_PROCEDURE_NOT_FOUND;
        function_index = procedure_number - ordinal_base;
    }
    if (function_index >= function_count) return STATUS_PROCEDURE_NOT_FOUND;
    function_rva = pe_u32(base + functions + function_index * 4);
    if (!function_rva || function_rva >= image_size) return STATUS_PROCEDURE_NOT_FOUND;
    /* Forwarder RVAs point back into the export directory; do not call them code. */
    if (function_rva >= export_rva && function_rva - export_rva < export_size)
        return STATUS_PROCEDURE_NOT_FOUND;
    *procedure_address = (PVOID)(base + function_rva);
    return STATUS_SUCCESS;
}
