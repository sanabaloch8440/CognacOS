/*
 * Small freestanding RTL subset for the native NTDLL bootstrap.
 * These routines intentionally have no libc or Wine dependencies.
 */
#include <windows.h>
#include <stddef.h>

__declspec(dllexport)
SIZE_T NTAPI RtlCompareMemory(const void *left, const void *right, SIZE_T length)
{
    const unsigned char *a = (const unsigned char *)left;
    const unsigned char *b = (const unsigned char *)right;
    SIZE_T i = 0;
    while (i < length && a[i] == b[i]) ++i;
    return i;
}

__declspec(dllexport)
PVOID NTAPI RtlSecureZeroMemory(PVOID destination, SIZE_T length)
{
    volatile unsigned char *p = (volatile unsigned char *)destination;
    SIZE_T i;
    for (i = 0; i < length; ++i) p[i] = 0;
    return destination;
}

__declspec(dllexport)
PVOID NTAPI RtlCopyMemory(PVOID destination, const VOID *source, SIZE_T length)
{
    unsigned char *d = (unsigned char *)destination;
    const unsigned char *s = (const unsigned char *)source;
    SIZE_T i;
    for (i = 0; i < length; ++i) d[i] = s[i];
    return destination;
}

__declspec(dllexport)
PVOID NTAPI RtlMoveMemory(PVOID destination, const VOID *source, SIZE_T length)
{
    unsigned char *d = (unsigned char *)destination;
    const unsigned char *s = (const unsigned char *)source;
    if (d < s) {
        SIZE_T i;
        for (i = 0; i < length; ++i) d[i] = s[i];
    } else if (d > s) {
        while (length) { --length; d[length] = s[length]; }
    }
    return destination;
}

__declspec(dllexport)
PVOID NTAPI RtlFillMemory(PVOID destination, SIZE_T length, UCHAR fill)
{
    unsigned char *d = (unsigned char *)destination;
    SIZE_T i;
    for (i = 0; i < length; ++i) d[i] = fill;
    return destination;
}
