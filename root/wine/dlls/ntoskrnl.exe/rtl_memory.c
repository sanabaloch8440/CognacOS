/*
 * NT kernel RTL memory primitives.
 *
 * This file is part of CognacOS's temporary Wine ntoskrnl compatibility
 * backend. Keep these implementations freestanding in spirit: no heap,
 * process, server, or user-mode runtime dependency.
 */
#include "ntoskrnl_private.h"

#undef RtlCopyMemory
#undef RtlMoveMemory
#undef RtlFillMemory
#undef RtlZeroMemory

void WINAPI RtlCopyMemory(void *dest, const void *src, SIZE_T length)
{
    unsigned char *d = dest;
    const unsigned char *s = src;
    SIZE_T i;

    for (i = 0; i < length; ++i) d[i] = s[i];
}

void WINAPI RtlMoveMemory(void *dest, const void *src, SIZE_T length)
{
    unsigned char *d = dest;
    const unsigned char *s = src;

    if ((ULONG_PTR)d < (ULONG_PTR)s)
    {
        SIZE_T i;
        for (i = 0; i < length; ++i) d[i] = s[i];
    }
    else if ((ULONG_PTR)d > (ULONG_PTR)s)
    {
        while (length)
        {
            --length;
            d[length] = s[length];
        }
    }
}

void WINAPI RtlFillMemory(void *dest, SIZE_T length, BYTE value)
{
    unsigned char *d = dest;
    SIZE_T i;

    for (i = 0; i < length; ++i) d[i] = value;
}

void WINAPI RtlZeroMemory(void *dest, SIZE_T length)
{
    volatile unsigned char *d = dest;

    while (length--) *d++ = 0;
}

SIZE_T WINAPI RtlCompareMemory(const void *left, const void *right, SIZE_T length)
{
    const unsigned char *a = left;
    const unsigned char *b = right;
    SIZE_T matched = 0;

    /* NT semantics: count the matching prefix, stopping at first mismatch. */
    while (matched < length && a[matched] == b[matched]) ++matched;
    return matched;
}

ULONG WINAPI RtlCompareMemoryUlong(const void *source, SIZE_T length, ULONG pattern)
{
    const unsigned char *s = source;
    const unsigned char *p = (const unsigned char *)&pattern;
    SIZE_T offset;

    /* Return the matching byte count, stopping at the first mismatching ULONG. */
    for (offset = 0; offset + sizeof(ULONG) <= length; offset += sizeof(ULONG))
    {
        SIZE_T j;
        for (j = 0; j < sizeof(ULONG); ++j)
            if (s[offset + j] != p[j]) return (ULONG)offset;
    }
    return (ULONG)length;
}

void WINAPI RtlFillMemoryUlong(void *dest, SIZE_T length, ULONG pattern)
{
    unsigned char *d = dest;
    const unsigned char *p = (const unsigned char *)&pattern;
    SIZE_T i;

    /* Repeat the pattern bytewise so unaligned buffers are handled safely. */
    for (i = 0; i < length; ++i) d[i] = p[i & 3];
}
