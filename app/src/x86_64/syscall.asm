global syscall_entry
extern syscall_dispatch

syscall_entry:
    sub rsp, 7*8        ; space for SyscallFrame

    mov [rsp + 0*8], rax
    mov [rsp + 1*8], rdi
    mov [rsp + 2*8], rsi
    mov [rsp + 3*8], rdx
    mov [rsp + 4*8], r10
    mov [rsp + 5*8], r8
    mov [rsp + 6*8], r9

    mov rdi, rsp        ; &SyscallFrame

    push rcx
    push r11

    call syscall_dispatch

    pop r11
    pop rcx
    add rsp, 7*8
    sysret
