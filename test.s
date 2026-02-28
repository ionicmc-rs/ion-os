.intel_syntax noprefix
.global _start
_start:
    mov rax, 0 # Ion OS: ack
    syscall
    # user mode!

    mov rax, 3               # Ion OS: IO command group
    mov rdi, 0               # subcommand: write
    mov rsi, 0               # fd: stdout
    lea rdx, [rip + message] # buffer
    mov r10, 13              # len
    syscall

    mov rax, 1
    mov rdi, 0  # subcommand: exit

    mov rsi, 42 # exit code
    syscall     # sys exit

    ud2 # noreturn guard

.data
message:
    .ascii "Hello, Ion!\n"


.section .ion_os.requests,"a",@progbits

.align 8

/* RequestsHeader */
.long 0x524E4F49      /* "IONR" little-endian */
.word 1               /* version */
.word 24              /* total size */

/* StackRequest */
.quad 0x40000          /* stack size: 256 KiB */
.quad 16               /* alignment */
