global context_switch
global thread_trampoline
global interrupt_entry_32
extern timer_isr

section .text
bits 64

; ------------------------------------------------------------
; context_switch(old: *mut Context, new: *const Context)
; rdi = old
; rsi = new
; ------------------------------------------------------------
context_switch:
    ; -------------------------
    ; Save old context
    ; -------------------------
    mov     [rdi + 0x00], rsp
    mov     [rdi + 0x08], r15
    mov     [rdi + 0x10], r14
    mov     [rdi + 0x18], r13
    mov     [rdi + 0x20], r12
    mov     [rdi + 0x28], rbx
    mov     [rdi + 0x30], rbp

    ; -------------------------
    ; Restore new context
    ; -------------------------
    mov     rsp, [rsi + 0x00]
    mov     r15, [rsi + 0x08]
    mov     r14, [rsi + 0x10]
    mov     r13, [rsi + 0x18]
    mov     r12, [rsi + 0x20]
    mov     rbx, [rsi + 0x28]
    mov     rbp, [rsi + 0x30]

    ret

; basic entry for threads
;
; no args - is no return
thread_trampoline:
    mov al, 'T'
    out 0xE9, al


    mov rdi, r12
    ; interrupts on the new thread.
    call r15
    ud2