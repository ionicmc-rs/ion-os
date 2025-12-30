section .multiboot_header
header_start:
    ; multiboot 2 number
    dd 0xe85250d6
    ; arch
    dd 0 ; protected mode i386
    ; header len
    dd header_end - header_start
    ; checksum
    dd 0x100000000 - (0xe85250d6 + 0 + (header_end - header_start))

    ; end tag
    dw 0
    dw 0
    dd 0
header_end: