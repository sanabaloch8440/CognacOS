ENTRY(DriverEntry)
PHDRS { text PT_LOAD FLAGS(5); rodata PT_LOAD FLAGS(4); data PT_LOAD FLAGS(6); }
SECTIONS {
 . = 0xffff8000e0000000 + 0x1000;
 .text : ALIGN(0x1000) { *(.text .text.*) } :text
 . = ALIGN(0x1000);
 .rodata : { *(.rodata .rodata.*) *(.eh_frame .eh_frame.*) } :rodata
 . = ALIGN(0x1000);
 .data : { *(.data .data.*) } :data
 .bss : { *(.bss .bss.*) *(COMMON) } :data
 /DISCARD/ : { *(.comment) *(.note*) }
}
