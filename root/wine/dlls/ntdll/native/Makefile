# Standalone native PE32+ bootstrap target; independent of Wine's Unix ntdll.so.
# Requires an x86_64-w64-mingw32 toolchain. The CI workflow installs it.
CROSS ?= x86_64-w64-mingw32-
CC := $(CROSS)gcc
OBJDUMP := $(CROSS)objdump
OUTPUT ?= ntdll.dll
CFLAGS := -O2 -Wall -Wextra -Werror -ffreestanding -fno-stack-protector -fno-builtin -fno-asynchronous-unwind-tables -fno-unwind-tables
LDFLAGS := -shared -nostdlib -Wl,--subsystem,windows -Wl,--dynamicbase -Wl,--nxcompat -Wl,--high-entropy-va -Wl,--image-base,0x180000000 -Wl,--entry,DllMain -Wl,--no-insert-timestamp

.PHONY: all check clean
all: $(OUTPUT)

$(OUTPUT): ntdll_native.c rtl_native.c ntdll_native.def syscall_x86_64.S
	$(CC) $(CFLAGS) $(LDFLAGS) -o $@ ntdll_native.c rtl_native.c syscall_x86_64.S ntdll_native.def

# Validate the actual PE output, including that it remains freestanding.
check: $(OUTPUT)
	$(OBJDUMP) -f $(OUTPUT) | grep -q 'pei-x86-64'
	$(OBJDUMP) -p $(OUTPUT) | grep -q 'DLL'
	$(OBJDUMP) -p $(OUTPUT) | grep -q 'AddressOfEntryPoint'
	$(OBJDUMP) -p $(OUTPUT) | grep -q 'NtClose'
	$(OBJDUMP) -p $(OUTPUT) | grep -q 'NtYieldExecution'
	$(OBJDUMP) -p $(OUTPUT) | grep -q 'RtlCompareMemory'
	$(OBJDUMP) -p $(OUTPUT) | grep -q 'CognacNtdllConfigureSyscalls'
	! $(OBJDUMP) -p $(OUTPUT) | grep -q 'DLL Name:'

clean:
	rm -f $(OUTPUT)
