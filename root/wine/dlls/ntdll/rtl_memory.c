/*
 * Shared RTL memory routines for Wine's NTDLL and CognacOS native NTDLL.
 * Keep the behavior here common to both build modes.
 */
#ifdef COGNAC_NATIVE_NTDLL
#include <windows.h>
#include <stddef.h>
#else
#include <string.h>
#include "windef.h"
#include "winternl.h"
#endif

#undef RtlCopyMemory
#undef RtlMoveMemory
#undef RtlFillMemory
#undef RtlZeroMemory

void WINAPI RtlCopyMemory(void *dest, const void *src, SIZE_T len)
{
#ifdef COGNAC_NATIVE_NTDLL
    unsigned char *d = (unsigned char *)dest;
    const unsigned char *s = (const unsigned char *)src;
    SIZE_T i;
    for (i = 0; i < len; ++i) d[i] = s[i];
#else
    memcpy(dest, src, len);
#endif
}

void WINAPI RtlMoveMemory(void *dest, const void *src, SIZE_T len)
{
#ifdef COGNAC_NATIVE_NTDLL
    unsigned char *d = (unsigned char *)dest;
    const unsigned char *s = (const unsigned char *)src;
    SIZE_T i;
    if ((ULONG_PTR)d < (ULONG_PTR)s)
        for (i = 0; i < len; ++i) d[i] = s[i];
    else if ((ULONG_PTR)d > (ULONG_PTR)s)
        while (len) { --len; d[len] = s[len]; }
#else
    memmove(dest, src, len);
#endif
}

void WINAPI RtlFillMemory(void *dest, SIZE_T len, BYTE fill)
{
#ifdef COGNAC_NATIVE_NTDLL
    unsigned char *d = (unsigned char *)dest;
    SIZE_T i;
    for (i = 0; i < len; ++i) d[i] = fill;
#else
    memset(dest, fill, len);
#endif
}

void WINAPI RtlZeroMemory(void *dest, SIZE_T len)
{
#ifdef COGNAC_NATIVE_NTDLL
    volatile unsigned char *d = (volatile unsigned char *)dest;
    while (len--) *d++ = 0;
#else
    memset(dest, 0, len);
#endif
}

SIZE_T WINAPI RtlCompareMemory(const void *src1, const void *src2, SIZE_T len)
{
    const unsigned char *a = (const unsigned char *)src1;
    const unsigned char *b = (const unsigned char *)src2;
    SIZE_T i = 0;
    while (i < len && a[i] == b[i]) ++i;
    return i;
}


/* Windows NTDLL exports use ULONG return/length types for these helpers. */
ULONG WINAPI RtlCompareMemoryUlong(void *src, SIZE_T len, ULONG pattern)
{
    const unsigned char *bytes = (const unsigned char *)src;
    const unsigned char *pat = (const unsigned char *)&pattern;
    SIZE_T i;

    for (i = 0; i < len; ++i)
        if (bytes[i] != pat[i & 3]) break;
    return (ULONG)i;
}

ULONG WINAPI RtlComputeCrc32(ULONG initial, const UCHAR *buffer, ULONG length)
{
    const unsigned char *bytes = (const unsigned char *)buffer;
    ULONG crc = ~initial;
    ULONG i;
    unsigned int bit;

    if (!buffer && length) return initial;
    for (i = 0; i < length; ++i)
    {
        crc ^= bytes[i];
        for (bit = 0; bit < 8; ++bit)
            crc = (crc & 1) ? ((crc >> 1) ^ 0xedb88320UL) : (crc >> 1);
    }
    return ~crc;
}

ULONG WINAPI RtlCrc32(ULONG initial, const UCHAR *buffer, ULONG length)
{
    return RtlComputeCrc32(initial, buffer, length);
}

BOOLEAN WINAPI RtlIsZeroMemory(void *buffer, SIZE_T length)
{
    const volatile unsigned char *bytes = (const volatile unsigned char *)buffer;
    SIZE_T i;

    if (!length) return TRUE;
    if (!buffer) return FALSE;
    for (i = 0; i < length; ++i)
        if (bytes[i]) return FALSE;
    return TRUE;
}
