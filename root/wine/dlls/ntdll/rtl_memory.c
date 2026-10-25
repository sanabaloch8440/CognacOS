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
