ENTRY(_start)

/* High-half virtual image with low physical load addresses. The bootloader
 * establishes these aliases and switches CR3 before entering _start. */
KERNEL_VMA = 0xffffffff80000000;
KERNEL_LMA = 0x00200000;

PHDRS
{
    text   PT_LOAD FLAGS(5); /* R-X */
    rodata PT_LOAD FLAGS(4); /* R-- */
    data   PT_LOAD FLAGS(6); /* RW- */
}

SECTIONS
{
    . = KERNEL_VMA;
    __kernel_virtual_start = .;

    .text : AT(KERNEL_LMA)
    {
        . = ALIGN(4K);
        __text_start = .;
        *(.text._start)
        *(.text .text.*)
        __text_end = .;
    } :text
    __text_phys_start = LOADADDR(.text);
    __text_phys_end = LOADADDR(.text) + SIZEOF(.text);

    . = ALIGN(4K);
    .rodata : AT(ALIGN(LOADADDR(.text) + SIZEOF(.text), 4K))
    {
        __rodata_start = .;
        *(.rodata .rodata.*)
        __rodata_end = .;
    } :rodata
    __rodata_phys_start = LOADADDR(.rodata);
    __rodata_phys_end = LOADADDR(.rodata) + SIZEOF(.rodata);

    . = ALIGN(4K);
    .data : AT(ALIGN(LOADADDR(.rodata) + SIZEOF(.rodata), 4K))
    {
        __data_start = .;
        *(.data .data.*)
        __data_end = .;
    } :data
    __data_phys_start = LOADADDR(.data);
    __data_phys_end = LOADADDR(.data) + SIZEOF(.data);

    . = ALIGN(4K);
    .bss : AT(ALIGN(LOADADDR(.data) + SIZEOF(.data), 4K))
    {
        __bss_start = .;
        *(COMMON)
        *(.bss .bss.*)
        __bss_end = .;
    } :data
    __bss_phys_start = LOADADDR(.bss);
    __bss_phys_end = LOADADDR(.bss) + SIZEOF(.bss);

    __kernel_virtual_end = .;
    /DISCARD/ : { *(.eh_frame .eh_frame_hdr .comment) }
}
