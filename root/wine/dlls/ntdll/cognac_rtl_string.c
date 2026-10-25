/*
 * Freestanding string helpers for the CognacOS native NTDLL.
 *
 * This is a deliberately dependency-free subset of the RTL string contract.
 * Case-insensitive comparison currently folds ASCII A-Z; full Windows
 * Unicode upcase-table semantics are not claimed.
 */
#include "cognac_ntdll.h"

#define STATUS_SUCCESS ((LONG)0x00000000L)
#define STATUS_INVALID_PARAMETER ((LONG)0xC000000DL)
#define STATUS_BUFFER_TOO_SMALL ((LONG)0xC0000023L)

static UCHAR fold_ansi(UCHAR value, BOOLEAN ignore_case)
{
    if (ignore_case && value >= 'A' && value <= 'Z')
        return (UCHAR)(value + ('a' - 'A'));
    return value;
}

static WCHAR fold_unicode(WCHAR value, BOOLEAN ignore_case)
{
    if (ignore_case && value >= (WCHAR)'A' && value <= (WCHAR)'Z')
        return (WCHAR)(value + ('a' - 'A'));
    return value;
}

void WINAPI RtlInitAnsiString(PCOGNAC_ANSI_STRING destination, const char *source)
{
    SIZE_T length = 0;
    if (!destination) return;
    if (!source) {
        destination->Length = 0;
        destination->MaximumLength = 0;
        destination->Buffer = NULL;
        return;
    }
    while (length < 0xfffeu && source[length]) ++length;
    destination->Length = (USHORT)length;
    destination->MaximumLength = (USHORT)(length + 1);
    destination->Buffer = (char *)source;
}

void WINAPI RtlInitUnicodeString(PCOGNAC_UNICODE_STRING destination, const WCHAR *source)
{
    SIZE_T length = 0;
    if (!destination) return;
    if (!source) {
        destination->Length = 0;
        destination->MaximumLength = 0;
        destination->Buffer = NULL;
        return;
    }
    /* Keep room for a terminating WCHAR in the 16-bit byte-count fields. */
    while (length < 0x7ffeu && source[length]) ++length;
    destination->Length = (USHORT)(length * sizeof(WCHAR));
    destination->MaximumLength = (USHORT)(destination->Length + sizeof(WCHAR));
    destination->Buffer = (WCHAR *)source;
}

LONG WINAPI RtlCompareString(PCCOGNAC_ANSI_STRING first,
                             PCCOGNAC_ANSI_STRING second,
                             BOOLEAN ignore_case)
{
    USHORT i, limit;
    if (!first || !second) return first ? 1 : (second ? -1 : 0);
    if ((first->Length && !first->Buffer) || (second->Length && !second->Buffer))
        return first->Length ? 1 : (second->Length ? -1 : 0);
    limit = first->Length < second->Length ? first->Length : second->Length;
    for (i = 0; i < limit; ++i) {
        UCHAR a = fold_ansi((UCHAR)first->Buffer[i], ignore_case);
        UCHAR b = fold_ansi((UCHAR)second->Buffer[i], ignore_case);
        if (a != b) return (LONG)a - (LONG)b;
    }
    return (LONG)first->Length - (LONG)second->Length;
}

BOOLEAN WINAPI RtlEqualString(PCCOGNAC_ANSI_STRING first,
                              PCCOGNAC_ANSI_STRING second,
                              BOOLEAN ignore_case)
{
    return (BOOLEAN)(first && second && first->Length == second->Length &&
                     RtlCompareString(first, second, ignore_case) == 0);
}

LONG WINAPI RtlCompareUnicodeString(PCCOGNAC_UNICODE_STRING first,
                                    PCCOGNAC_UNICODE_STRING second,
                                    BOOLEAN ignore_case)
{
    USHORT i, first_count, second_count, limit;
    if (!first || !second) return first ? 1 : (second ? -1 : 0);
    if ((first->Length & 1) || (second->Length & 1)) return (first->Length & 1) ? 1 : -1;
    if ((first->Length && !first->Buffer) || (second->Length && !second->Buffer))
        return first->Length ? 1 : (second->Length ? -1 : 0);
    first_count = (USHORT)(first->Length / sizeof(WCHAR));
    second_count = (USHORT)(second->Length / sizeof(WCHAR));
    limit = first_count < second_count ? first_count : second_count;
    for (i = 0; i < limit; ++i) {
        WCHAR a = fold_unicode(first->Buffer[i], ignore_case);
        WCHAR b = fold_unicode(second->Buffer[i], ignore_case);
        if (a != b) return (LONG)a - (LONG)b;
    }
    return (LONG)first_count - (LONG)second_count;
}

BOOLEAN WINAPI RtlEqualUnicodeString(PCCOGNAC_UNICODE_STRING first,
                                     PCCOGNAC_UNICODE_STRING second,
                                     BOOLEAN ignore_case)
{
    return (BOOLEAN)(first && second && first->Length == second->Length &&
                     RtlCompareUnicodeString(first, second, ignore_case) == 0);
}

void WINAPI RtlCopyUnicodeString(PCOGNAC_UNICODE_STRING destination,
                                 PCCOGNAC_UNICODE_STRING source)
{
    USHORT count, i;
    if (!destination) return;
    destination->Length = 0;
    if (!destination->Buffer || destination->MaximumLength < sizeof(WCHAR)) return;
    if (!source || !source->Buffer || !source->Length) {
        destination->Buffer[0] = 0;
        return;
    }
    count = (USHORT)(source->Length / sizeof(WCHAR));
    if (count > (USHORT)(destination->MaximumLength / sizeof(WCHAR) - 1))
        count = (USHORT)(destination->MaximumLength / sizeof(WCHAR) - 1);
    if ((ULONG_PTR)destination->Buffer < (ULONG_PTR)source->Buffer) {
        for (i = 0; i < count; ++i) destination->Buffer[i] = source->Buffer[i];
    } else if ((ULONG_PTR)destination->Buffer > (ULONG_PTR)source->Buffer) {
        i = count;
        while (i) { --i; destination->Buffer[i] = source->Buffer[i]; }
    }
    destination->Buffer[count] = 0;
    destination->Length = (USHORT)(count * sizeof(WCHAR));
}

static int unicode_digit(WCHAR value)
{
    if (value >= (WCHAR)'0' && value <= (WCHAR)'9') return value - (WCHAR)'0';
    if (value >= (WCHAR)'a' && value <= (WCHAR)'z') return value - (WCHAR)'a' + 10;
    if (value >= (WCHAR)'A' && value <= (WCHAR)'Z') return value - (WCHAR)'A' + 10;
    return -1;
}

LONG WINAPI RtlUnicodeStringToInteger(PCCOGNAC_UNICODE_STRING string,
                                      ULONG base, PULONG output)
{
    USHORT index = 0, count;
    ULONG value = 0;
    int sign = 1, digit, found = 0;
    if (!string || !output || (string->Length & 1) ||
        (string->Length && !string->Buffer)) return STATUS_INVALID_PARAMETER;
    count = (USHORT)(string->Length / sizeof(WCHAR));
    while (index < count && (string->Buffer[index] == ' ' ||
           string->Buffer[index] == '\t' || string->Buffer[index] == '\r' ||
           string->Buffer[index] == '\n')) ++index;
    if (index < count && (string->Buffer[index] == '+' || string->Buffer[index] == '-')) {
        if (string->Buffer[index] == '-') sign = -1;
        ++index;
    }
    if (base == 0) base = 10;
    if (base == 16 && index + 1 < count && string->Buffer[index] == '0' &&
        (string->Buffer[index + 1] == 'x' || string->Buffer[index + 1] == 'X')) index += 2;
    if (base < 2 || base > 36) return STATUS_INVALID_PARAMETER;
    for (; index < count; ++index) {
        digit = unicode_digit(string->Buffer[index]);
        if (digit < 0) {
            if (string->Buffer[index] == ' ' || string->Buffer[index] == '\t' ||
                string->Buffer[index] == '\r' || string->Buffer[index] == '\n') {
                do { ++index; } while (index < count &&
                    (string->Buffer[index] == ' ' || string->Buffer[index] == '\t' ||
                     string->Buffer[index] == '\r' || string->Buffer[index] == '\n'));
                if (index == count) break;
            }
            return STATUS_INVALID_PARAMETER;
        }
        if ((ULONG)digit >= base || value > (0xffffffffu - (ULONG)digit) / base)
            return STATUS_INVALID_PARAMETER;
        value = value * base + (ULONG)digit;
        found = 1;
    }
    if (!found) return STATUS_INVALID_PARAMETER;
    *output = sign < 0 ? (ULONG)(0u - value) : value;
    return STATUS_SUCCESS;
}

LONG WINAPI RtlIntegerToUnicodeString(ULONG value, ULONG base,
                                      PCOGNAC_UNICODE_STRING destination)
{
    WCHAR reversed[32];
    USHORT count = 0, i;
    ULONG original_base = base;
    if (!destination || !destination->Buffer) return STATUS_INVALID_PARAMETER;
    destination->Length = 0;
    if (base == 0) base = 10;
    if (base != 2 && base != 8 && base != 10 && base != 16)
        return STATUS_INVALID_PARAMETER;
    do {
        ULONG digit = value % base;
        reversed[count++] = (WCHAR)(digit < 10 ? '0' + digit : 'A' + digit - 10);
        value /= base;
    } while (value && count < (USHORT)(sizeof(reversed) / sizeof(reversed[0])));
    (void)original_base;
    if ((ULONG)(count + 1) * sizeof(WCHAR) > destination->MaximumLength)
        return STATUS_BUFFER_TOO_SMALL;
    for (i = 0; i < count; ++i) destination->Buffer[i] = reversed[count - i - 1];
    destination->Buffer[count] = 0;
    destination->Length = (USHORT)(count * sizeof(WCHAR));
    return STATUS_SUCCESS;
}
