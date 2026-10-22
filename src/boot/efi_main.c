// RdotOS UEFI bootstrap. This is an EFI application, not yet the privileged kernel.
#include <stdint.h>
#include <stddef.h>

#if defined(__clang__)
#define EFIABI
#else
#define EFIABI
#endif

typedef uint64_t EFI_STATUS;
typedef uint16_t CHAR16;
typedef void *EFI_HANDLE;
typedef uint64_t UINTN;
typedef struct { uint64_t Signature; uint32_t Revision, HeaderSize, CRC32, Reserved; } EFI_TABLE_HEADER;
typedef struct EFI_SIMPLE_TEXT_OUTPUT_PROTOCOL EFI_SIMPLE_TEXT_OUTPUT_PROTOCOL;
struct EFI_SIMPLE_TEXT_OUTPUT_PROTOCOL {
    EFI_STATUS (EFIABI *Reset)(EFI_SIMPLE_TEXT_OUTPUT_PROTOCOL *, uint8_t);
    EFI_STATUS (EFIABI *OutputString)(EFI_SIMPLE_TEXT_OUTPUT_PROTOCOL *, CHAR16 *);
    void *TestString;
    void *QueryMode;
    void *SetMode;
    void *SetAttribute;
    void *ClearScreen;
    void *SetCursorPosition;
    void *EnableCursor;
    void *Mode;
};
typedef struct {
    EFI_TABLE_HEADER Hdr;
    void *RaiseTPL, *RestoreTPL, *AllocatePages, *FreePages, *GetMemoryMap;
    void *AllocatePool, *FreePool, *CreateEvent, *SetTimer, *WaitForEvent;
    void *SignalEvent, *CloseEvent, *CheckEvent, *InstallProtocolInterface;
    void *ReinstallProtocolInterface, *UninstallProtocolInterface, *HandleProtocol;
    void *Reserved, *RegisterProtocolNotify, *LocateHandle, *LocateDevicePath;
    void *InstallConfigurationTable, *LoadImage, *StartImage, *Exit;
    void *UnloadImage, *ExitBootServices, *GetNextMonotonicCount, *Stall;
    void *SetWatchdogTimer, *ConnectController, *DisconnectController;
    void *OpenProtocol, *CloseProtocol, *OpenProtocolInformation;
    void *ProtocolsPerHandle, *LocateHandleBuffer, *LocateProtocol;
    void *InstallMultipleProtocolInterfaces, *UninstallMultipleProtocolInterfaces;
    void *CalculateCrc32, *CopyMem, *SetMem, *CreateEventEx;
} EFI_BOOT_SERVICES;
typedef struct {
    EFI_TABLE_HEADER Hdr;
    void *GetTime, *SetTime, *GetWakeupTime, *SetWakeupTime;
    void *SetVirtualAddressMap, *ConvertPointer;
    EFI_STATUS (EFIABI *GetVariable)(CHAR16 *, void *, uint32_t *, UINTN *, void *);
} EFI_RUNTIME_SERVICES;
typedef struct {
    EFI_TABLE_HEADER Hdr;
    CHAR16 *FirmwareVendor;
    uint32_t FirmwareRevision;
    EFI_HANDLE ConsoleInHandle;
    void *ConIn;
    EFI_HANDLE ConsoleOutHandle;
    EFI_SIMPLE_TEXT_OUTPUT_PROTOCOL *ConOut;
    EFI_HANDLE StandardErrorHandle;
    EFI_SIMPLE_TEXT_OUTPUT_PROTOCOL *StdErr;
    EFI_RUNTIME_SERVICES *RuntimeServices;
    EFI_BOOT_SERVICES *BootServices;
    UINTN NumberOfTableEntries;
    void *ConfigurationTable;
} EFI_SYSTEM_TABLE;

typedef struct { uint32_t Data1; uint16_t Data2, Data3; uint8_t Data4[8]; } EFI_GUID;
static EFI_GUID gEfiGlobalVariableGuid = {
    0x8BE4DF61, 0x93CA, 0x11D2, {0xAA,0x0D,0x00,0xE0,0x98,0x03,0x2B,0x8C}
};
static CHAR16 secureBootName[] = {'S','e','c','u','r','e','B','o','o','t',0};
static CHAR16 line1[] = {'R','d','o','t','O','S',' ','U','E','F','I',' ','b','o','o','t','s','t','r','a','p', '\r','\n',0};
static CHAR16 line2[] = {'A','r','c','h','i','t','e','c','t','u','r','e',':',' ','x','8','6','-','6','4',' ','n','a','t','i','v','e','\r','\n',0};
static CHAR16 secureOn[] = {'F','i','r','m','w','a','r','e',' ','S','e','c','u','r','e',' ','B','o','o','t',':',' ','e','n','a','b','l','e','d','\r','\n',0};
static CHAR16 secureOff[] = {'F','i','r','m','w','a','r','e',' ','S','e','c','u','r','e',' ','B','o','o','t',':',' ','d','i','s','a','b','l','e','d','\r','\n',0};
static CHAR16 secureUnknown[] = {'F','i','r','m','w','a','r','e',' ','S','e','c','u','r','e',' ','B','o','o','t',':',' ','u','n','a','v','a','i','l','a','b','l','e','\r','\n',0};
static CHAR16 warning[] = {'N','O','T','E',':',' ','b','o','o','t','-','c','h','a','i','n',' ','s','i','g','n','a','t','u','r','e',' ','v','e','r','i','f','i','c','a','t','i','o','n',' ','i','s',' ','n','o','t',' ','y','e','t',' ','i','m','p','l','e','m','e','n','t','e','d','.','\r','\n',0};
static CHAR16 next[] = {'K','e','r','n','e','l',' ','h','a','n','d','o','f','f',' ','i','s',' ','n','o','t',' ','y','e','t',' ','e','n','a','b','l','e','d','.','\r','\n',0};

EFI_STATUS EFIABI efi_main(EFI_HANDLE image, EFI_SYSTEM_TABLE *st) {
    (void)image;
    if (!st || !st->ConOut || !st->ConOut->OutputString) return 0x8000000000000002ULL;
    st->ConOut->OutputString(st->ConOut, line1);
    st->ConOut->OutputString(st->ConOut, line2);
    uint8_t enabled = 0;
    uint32_t attrs = 0;
    UINTN size = sizeof(enabled);
    EFI_STATUS status = st->RuntimeServices && st->RuntimeServices->GetVariable
        ? st->RuntimeServices->GetVariable(secureBootName, &gEfiGlobalVariableGuid, &attrs, &size, &enabled)
        : 0x8000000000000002ULL;
    if (status == 0 && size >= 1) st->ConOut->OutputString(st->ConOut, enabled ? secureOn : secureOff);
    else st->ConOut->OutputString(st->ConOut, secureUnknown);
    st->ConOut->OutputString(st->ConOut, warning);
    st->ConOut->OutputString(st->ConOut, next);
    return 0;
}
