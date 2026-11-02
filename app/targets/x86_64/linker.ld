ENTRY(start)

PHDRS {
    boot PT_LOAD FLAGS(0x5);   /* low-memory boot and AP trampoline code */
    ap_data PT_LOAD FLAGS(0x6); /* mutable AP trampoline state */
    text PT_LOAD FLAGS(0x5);   /* read + execute */
    rodata PT_LOAD FLAGS(0x4); /* read only */
    data PT_LOAD FLAGS(0x6);   /* read + write */
}

SECTIONS
{
    . = 0x8000;

    .ap_init 0x8000 :
    {
        KEEP(*(.ap_init))
    }: boot
    
    .boot : ALIGN(16)
    {
        KEEP(*(.multiboot_header))
    }: boot

    .ap_data : ALIGN(8)
    {
        KEEP(*(.ap_data))
    }: ap_data

    . = 1M;


    .text : ALIGN(16)
    {
        *(.text .text.*)
    }: text

    .rodata : ALIGN(16)
    {
        *(.rodata .rodata.*)
    }: rodata

    .data : ALIGN(16)
    {
        *(.data .data.*)
    }: data

    .tdata : ALIGN(16)
    {
        __tdata_start = .;
        *(.tdata .tdata.*)
        __tdata_end = .;
    }: data

    .tbss : ALIGN(16)
    {
        __tbss_start = .;
        *(.tbss .tbss.*)
        __tbss_end = .;
    }: data

    .bss : ALIGN(16) {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        __bss_end = .;
    }: data
    /tls/ : { }
    __kernel_end = .;
    __tls_start = ADDR(.tdata);
    __tls_end   = ADDR(.tbss) + SIZEOF(.tbss);
}
