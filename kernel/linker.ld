ENTRY(_start)
SECTIONS
{
    . = 0xFFFFFFFF80000000;
    .text : ALIGN(4K) { *(.text._start) *(.text .text.*) }
    .rodata : ALIGN(4K) { *(.rodata .rodata.*) }
    .data : ALIGN(4K) { *(.data .data.*) }
    .bss : ALIGN(4K) { *(COMMON) *(.bss .bss.*) }
}
