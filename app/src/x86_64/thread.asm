global context_switch
global thread_trampoline
section .text
bits 64

; jumps to the entry point
; this never returns
thread_trampoline:
    mov rdi, r12      ; first argument
    call r15          ; r15 = actual entry fn
    ud2               ; entry must never return (`!`)

; context_switch(old: *mut Context, new: *const Context)
; rdi = old, rsi = new
context_switch:
    ; save old
    mov [rdi + 0x00], rsp
    mov [rdi + 0x08], r15
    mov [rdi + 0x10], r14
    mov [rdi + 0x18], r13
    mov [rdi + 0x20], r12
    mov [rdi + 0x28], rbx
    mov [rdi + 0x30], rbp

    ; restore new
    mov rsp, [rsi + 0x00]
    mov r15, [rsi + 0x08]
    mov r14, [rsi + 0x10]
    mov r13, [rsi + 0x18]
    mov r12, [rsi + 0x20]
    mov rbx, [rsi + 0x28]
    mov rbp, [rsi + 0x30]

    ret
