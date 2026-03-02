bits 64
section .text
global _start

_start:
    push rax ; test stack
    pop rax
    mov rax, 1  ; group: process
    mov rdi, 0  ; selection: exit
    mov rsi, 42 ; exit code
    syscall     ; exit(42)
    ud2

section .ion_os.requests,"ax",@progbits

align 8


dd 0x524E4F49 ; IONR     

dw 1
dw 24

dq 0x40000    ; size: 256 KiB
dq 16         ; align: 16

        