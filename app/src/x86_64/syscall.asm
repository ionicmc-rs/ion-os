global syscall_entry
extern syscall_dispatch

section .text

bits 64
syscall_entry:
    push rbp
    mov rbp, rsp            ; save stack pointer
    
    sub rsp, 8              ; keep SysV call alignment for the Rust callee

    push rcx            ; save rcx (return RIP for user)
    push r11            ; save r11 (RFlags for user)
    
    sub rsp, 7*8        ; space for SyscallFrame

    mov [rsp + 0*8], rax
    mov [rsp + 1*8], rdi
    mov [rsp + 2*8], rsi
    mov [rsp + 3*8], rdx
    mov [rsp + 4*8], r10
    mov [rsp + 5*8], r8
    mov [rsp + 6*8], r9

    mov rdi, rsp        ; &SyscallFrame

    call syscall_dispatch

    ; Preserve user-visible caller-saved argument registers across syscall return.
    ; (rax: syscall result, rdi: metadata are intentionally left as outputs)
    mov rsi, [rsp + 2*8]
    mov rdx, [rsp + 3*8]
    mov r10, [rsp + 4*8]
    mov r8,  [rsp + 5*8]
    mov r9,  [rsp + 6*8]

    add rsp, 7*8        ; drop(SyscallFrame)

    pop r11             ; restore user RFLAGS
    pop rcx             ; restore user RIP

    add rsp, 8          ; drop alignment slot
    
    mov rsp, rbp
    pop rbp

    ; o64 is required as nasm will produce sysretl by default
    o64 sysret