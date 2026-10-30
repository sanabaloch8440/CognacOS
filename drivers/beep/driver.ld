ENTRY(DriverEntry)
SECTIONS
{
  . = 0xffff800190000000;
  .text : ALIGN(4096) { *(.text .text.*) }
  .rdata : ALIGN(4096) { *(.rodata .rodata.*) }
  .data : ALIGN(4096) { *(.data .data.*) }
  .bss : ALIGN(4096) { *(.bss .bss.* COMMON) }
  /DISCARD/ : { *(.eh_frame .eh_frame_hdr .comment .note .note.*) }
}
