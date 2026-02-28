ENTRY(_start)

SECTIONS
{
    . = 0x400000;

    .text : {
        *(.text*)
    }

    .rodata : {
        *(.rodata*)
    }

    .data : {
        *(.data*)
    }

    .bss : {
        *(COMMON)
        *(.bss*)
    }

    .ion_os.requests : {
        *(.ion_os.requests)
    }

    .irt : {
        KEEP(*(.irt*))
    }
}