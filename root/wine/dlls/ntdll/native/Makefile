# Standalone native PE32+ bootstrap target; separate from Wine's Makefile.in.
# Requires an x86_64-w64-mingw32 toolchain.
CROSS ?= x86_64-w64-mingw32-
CC := $(CROSS)gcc
OBJDUMP := $(CROSS)objdump
OUTPUT ?= ntdll.dll
CFLAGS := -O2 -Wall -Wextra -Werror -ffreestanding -fno-stack-protector
LDFLAGS := -shared -nostdlib -Wl,--subsystem,windows -Wl,--dynamicbase -Wl,--nxcompat -Wl,--high-entropy-va -Wl,--image-base,0x180000000 -Wl,--entry,DllMain

.PHONY: all check clean
all: $(OUTPUT)

$(OUTPUT): ntdll_native.c rtl_native.c ntdll_native.def syscall_x86_64.S
	$(CC) $(CFLAGS) $(LDFLAGS) -o $@ ntdll_native.c rtl_native.c syscall_x86_64.S ntdll_native.def

check: $(OUTPUT)
	$(OBJDUMP) -f $(OUTPUT) | grep -q 'pei-x86-64'
	$(OBJDUMP) -p $(OUTPUT) | grep -q 'DLL'
	$(OBJDUMP) -p $(OUTPUT) | grep -q 'NtClose'
	$(OBJDUMP) -p $(OUTPUT) | grep -q 'RtlCompareMemory'
	$(OBJDUMP) -p $(OUTPUT) | grep -q 'CognacNtdllConfigureSyscalls'

clean:
	rm -f $(OUTPUT)
