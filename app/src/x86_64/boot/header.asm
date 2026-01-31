section .multiboot_header
header_start:
    dd 0xe85250d6              ; multiboot2 magic
    dd 0                       ; arch = i386
    dd header_end - header_start
    dd 0x100000000 - (0xe85250d6 + 0 + (header_end - header_start))

    ; -----------------------------------------
    ; Framebuffer request tag
    ; -----------------------------------------
    dw 5                       ; tag type = framebuffer
    dw 0                       ; flags
    dd 20                      ; size of this tag

    dd 1024                    ; width
    dd 768                     ; height
    dd 32                      ; depth (bits per pixel)

    ; -----------------------------------------
    ; End tag (must be last)
    ; -----------------------------------------
    dw 0
    dw 0
    dd 0
header_end: