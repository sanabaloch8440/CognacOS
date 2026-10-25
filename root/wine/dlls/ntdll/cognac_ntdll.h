/*
 * Minimal freestanding Win64 ABI types for CognacOS's native NTDLL.
 * Deliberately avoids Windows SDK declarations/import attributes so the
 * implementation can define NTDLL exports without dllimport conflicts.
 */
#ifndef COGNAC_NTDLL_H
#define COGNAC_NTDLL_H

#include <stddef.h>
#include <stdint.h>

typedef int32_t LONG;
typedef uint32_t ULONG;
typedef uint32_t DWORD;
typedef uintptr_t ULONG_PTR;
typedef intptr_t LONG_PTR;
typedef size_t SIZE_T;
typedef uint8_t BYTE;
typedef uint8_t UCHAR;
typedef uint8_t BOOLEAN;
typedef uint16_t WCHAR;
typedef void *HANDLE;
typedef void *HINSTANCE;
typedef void *LPVOID;
typedef void *PVOID;
typedef ULONG *PULONG;
typedef int BOOL;

#ifndef TRUE
#define TRUE 1
#endif
#ifndef FALSE
#define FALSE 0
#endif

#ifndef WINAPI
#define WINAPI
#endif
#ifndef NTAPI
#define NTAPI
#endif

#endif
