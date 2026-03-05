ENTRY(_start)

PHDRS {
    load PT_LOAD FLAGS(0x5);
}

SECTIONS
{
    . = 0x10000;

    .text : ALIGN(16)
    {
        *(.text .text.*)
    } :load

    .rodata : ALIGN(16)
    {
        *(.rodata .rodata.*)
    } :load

    .data : ALIGN(16)
    {
        *(.data .data.*)
    } :load

    .bss : ALIGN(16) {
        *(.bss .bss.*)
        *(COMMON)
    } :load
}
