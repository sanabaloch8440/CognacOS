ENTRY(_start)
/* The initial kernel runs from an identity-mapped physical address.
 * High-half virtual addressing is introduced only after page-table setup exists. */
SECTIONS
{
    . = 0x00200000;
    .text : ALIGN(4K) { *(.text._start) *(.text .text.*) }
    .rodata : ALIGN(4K) { *(.rodata .rodata.*) }
    .data : ALIGN(4K) { *(.data .data.*) }
    .bss : ALIGN(4K) { *(COMMON) *(.bss .bss.*) }
    /DISCARD/ : { *(.eh_frame .eh_frame_hdr .comment) }
}
