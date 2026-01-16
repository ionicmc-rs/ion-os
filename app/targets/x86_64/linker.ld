ENTRY(start)

PHDRS {
    load PT_LOAD FLAGS(0x7);
}

SECTIONS
{
    . = 1M;

    .boot : ALIGN(16)
    {
        KEEP(*(.multiboot_header))
    }: load

    .text : ALIGN(16)
    {
        *(.text .text.*)
    }: load

    .rodata : ALIGN(16)
    {
        *(.rodata .rodata.*)
    }: load

    .data : ALIGN(16)
    {
        *(.data .data.*)
    }: load

    .tdata : ALIGN(16)
    {
        __tdata_start = .;
        *(.tdata .tdata.*)
        __tdata_end = .;
    }: load

    .tbss : ALIGN(16)
    {
        __tbss_start = .;
        *(.tbss .tbss.*)
        __tbss_end = .;
    }: load

    .bss : ALIGN(16) {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        __bss_end = .;
    }: load
    /tls/ : { }

    __tls_start = ADDR(.tdata);
    __tls_end   = ADDR(.tbss) + SIZEOF(.tbss);
}
