ENTRY(start)

KERNEL_VMA = 0xffffffff80000000;

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

    .boot_text : ALIGN(16) { *(.boot_text) }: boot
    .boot_rodata : ALIGN(16) { *(.boot_rodata) }: boot

    .ap_data : ALIGN(8)
    {
        KEEP(*(.ap_data))
    }: ap_data

    .boot_data : ALIGN(16) { *(.boot_data) }: ap_data
    .boot_bss (NOLOAD) : ALIGN(16) { *(.boot_bss) }: ap_data

    /* Bootstrap remains identity mapped; the kernel proper runs high. */
    . = KERNEL_VMA + 1M;
    __kernel_load_start = 1M;


    .text : AT(ADDR(.text) - KERNEL_VMA) ALIGN(16)
    {
        *(.text .text.*)
    }: text

    .rodata : AT(ADDR(.rodata) - KERNEL_VMA) ALIGN(16)
    {
        *(.rodata .rodata.*)
    }: rodata

    .data : AT(ADDR(.data) - KERNEL_VMA) ALIGN(16)
    {
        *(.data .data.*)
    }: data

    .tdata : AT(ADDR(.tdata) - KERNEL_VMA) ALIGN(16)
    {
        __tdata_start = .;
        *(.tdata .tdata.*)
        __tdata_end = .;
    }: data

    .tbss : AT(ADDR(.tbss) - KERNEL_VMA) ALIGN(16)
    {
        __tbss_start = .;
        *(.tbss .tbss.*)
        __tbss_end = .;
    }: data

    .bss : AT(ADDR(.bss) - KERNEL_VMA) ALIGN(16) {
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
