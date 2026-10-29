ENTRY(DriverEntry)
PHDRS { text PT_LOAD FLAGS(5); rodata PT_LOAD FLAGS(4); data PT_LOAD FLAGS(6); }
SECTIONS {
 . = 0xffff800190000000 + 0x1000;
 .text : ALIGN(0x1000) { *(.text .text.*) *(.ltext .ltext.*) } :text
 . = ALIGN(0x1000);
 .rodata : { *(.rodata .rodata.*) *(.lrodata .lrodata.*) *(.eh_frame .eh_frame.*) } :rodata
 . = ALIGN(0x1000);
 .data : { *(.data .data.*) *(.ldata .ldata.*) } :data
 .bss : { *(.bss .bss.*) *(.lbss .lbss.*) *(COMMON) } :data
 /DISCARD/ : { *(.comment) *(.note*) }
}
