global usr_test_start
usr_test_start:
    mov rax, 1
    mov rsi, 42
    syscall
    hlt ; should never reach
