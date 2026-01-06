ENTRY(start)

SECTIONS
{
    . = 1M;

    .boot :
    {
        KEEP(*(.multiboot_header))
    }

    .text :
    {
        *(.text .text.*)
    }

    .rodata :
    {
        *(.rodata .rodata.*)
    }

    .data :
    {
        *(.data .data.*)
    }

    .tdata : ALIGN(16)
    {
        __tdata_start = .;
        *(.tdata .tdata.*)
        __tdata_end = .;
    }

    .tbss (NOLOAD) : ALIGN(16)
    {
        __tbss_start = .;
        *(.tbss .tbss.*)
        __tbss_end = .;
    }

    __tls_start = __tdata_start;
    __tls_end   = __tbss_end;
}
