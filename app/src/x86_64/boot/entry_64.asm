global long_mode_start
extern kernel_main
extern stack_top
extern boot_info_data


section .text
bits 64
long_mode_start:
    mov rsp, stack_top

    mov ax, 0
    mov ss, ax
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax

    mov dword [0xb8000], 0x0F314F54    ; "S1" (white on black), 0x4F is 'O', adjust as you like
    mov dword [0xb8004], 0x0F324F00


    ; write 'X' to 0xE9 debug port
    mov al, 'X'
    out 0xE9, al

    ; Pass BootInfo in rdi (SysV AMD64 ABI)
    lea rdi, [rel boot_info_data]

    ; Store full 64-bit kernel_main in BootInfo
    mov rax, kernel_main
    mov [boot_info_data + 0x30], rax

    call rax

    hlt