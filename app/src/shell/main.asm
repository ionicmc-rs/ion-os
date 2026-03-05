bits 64
; NASM x86 assembly for sh
; we write directly in assembly to ensure that it cannot bug.

; syscall consts

%macro SYSCALL 2
    mov rax, %1
    mov rdi, %2
    syscall
%endmacro

; groups

SYS_GROUP_PROC equ 1
SYS_GROUP_IO equ 2
; TODO: SYS_GROUP_FS

; selections: proc

SYS_PROC_EXIT equ 0
; TODO: SYS_PROC_EXEC

; selections: I/O

SYS_IO_WRITE equ 0
SYS_IO_READ equ 1
; TODO: SYS_IO_OPEN
; TODO: SYS_IO_CLOSE

section .text

global _start
_start:
    ; TODO: copy default prompt

    lea rdi, [rel default_prompt]
    mov rsi, DEF_PROMPT_LEN
    call set_prompt

    call main

    jmp exit

; extern "C" fn exit(code: i32) -> ! 
exit:
    mov rsi, rdi ; exit code
    SYSCALL SYS_GROUP_PROC, SYS_PROC_EXIT
    ud2

; main logic
main:

.loop:

    call print_prompt

    call read_stdin_line ; TODO: literally anything

    jmp .loop

.done:
    ret

read_stdin_line:
    push rbx
    lea rbx, [rel read_buffer]

    xor ecx, ecx
.loop:
    mov rsi, 6                ; stdin
    lea rdx, [rel single_char_buffer]
    mov r10, 1
    SYSCALL SYS_GROUP_IO, SYS_IO_READ

    cmp rax, 0 ; in-case nothing was read - wait for user input
    je .loop

    mov al, [rel single_char_buffer]
    cmp al, 10                ; '\n'
    je .done

    mov [rbx + rcx], al
    inc rcx

    cmp rcx, BUF_LEN
    jae .failed

    jmp .loop

.done:
    mov byte [rbx + rcx], 0
    pop rbx
    mov rax, rcx
    ret

.failed:
    pop rbx
    mov rax, -1
    ret

print_prompt:
    mov rsi, 1
    lea rdx, [rel prompt]
    mov r10, [rel current_prompt_len]
    SYSCALL SYS_GROUP_IO, SYS_IO_WRITE
    ret

set_prompt:
    ; rdi = src
    ; rsi = len

    mov rdx, rsi          ; len
    mov r8, rdx
    mov rsi, rdi          ; src
    lea rdi, [rel prompt] ; dst
    call memcpy

    mov [rel current_prompt_len], r8

    ret

memcpy:
    test rdx, rdx
    jz .done

.loop:
    mov al, [rsi]
    mov [rdi], al

    inc rsi
    inc rdi
    dec rdx
    jnz .loop

.done:
    ret

sysret_:
    sysret

section .data

current_prompt_len:
    dq 0

section .bss

prompt:
    resb PROMPT_LEN

read_buffer:
    resb BUF_LEN

single_char_buffer:
    resb 1

BUF_LEN equ 256
PROMPT_LEN equ 256

section .rodata

default_prompt:
    db "ish$"
DEF_PROMPT_LEN equ $ - default_prompt

help_msg:
    db "Ion Shell v1.1: The default shell for Ion OS"

section .ion_os.requests
requests:
db "IONR"
dw REQ_VER
dw REQ_LEN

dq STACK_SIZE
dq STACK_ALIGN

REQ_LEN equ $ - requests
REQ_VER equ 1
STACK_SIZE equ 0x40000
STACK_ALIGN equ 16