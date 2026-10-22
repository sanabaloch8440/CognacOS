ENTRY(_start)
/* The initial kernel runs from an identity-mapped physical address.
 * High-half virtual addressing is introduced only after page-table setup exists. */
SECTIONS
{
    . = 0x00200000;
    .text : ALIGN(4K) { __text_start = .; *(.text._start) *(.text .text.*) __text_end = .; }
    .rodata : ALIGN(4K) { __rodata_start = .; *(.rodata .rodata.*) __rodata_end = .; }
    .data : ALIGN(4K) { __data_start = .; *(.data .data.*) __data_end = .; }
    .bss : ALIGN(4K) { __bss_start = .; *(COMMON) *(.bss .bss.*) __bss_end = .; }
    /DISCARD/ : { *(.eh_frame .eh_frame_hdr .comment) }
}
