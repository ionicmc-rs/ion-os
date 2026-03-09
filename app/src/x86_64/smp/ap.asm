[BITS 16]

section .ap_trampoline exec
global smp_start16

extern ap_entry

smp_start16:

    mov al, 'A'
    out 0xE9, al

    cli

    ; -------------------------
    ; load temporary GDT
    ; -------------------------
    lgdt [gdt_ptr - $$]

    ; -------------------------
    ; enable protected mode
    ; -------------------------
    mov eax, cr0
    or eax, 1
    mov cr0, eax

    jmp 0x08:(protected_mode - $$)

; --------------------------------------------------

[BITS 32]

protected_mode:

    ; enable PAE
    mov eax, cr4
    or eax, (1 << 5)
    mov cr4, eax

    ; load kernel page table
    mov eax, [ap_pml4_ptr]
    mov cr3, eax

    ; enable long mode
    mov ecx, 0xC0000080
    rdmsr
    or eax, (1 << 8)
    wrmsr

    ; enable paging
    mov eax, cr0
    or eax, (1 << 31)
    mov cr0, eax

    jmp 0x28:long_mode

; --------------------------------------------------

[BITS 64]

long_mode:

    mov rsp, [rel ap_stack]

    call ap_entry

.hang:
    hlt
    jmp .hang

; --------------------------------------------------
; Data used by BSP to configure trampoline
; --------------------------------------------------

global ap_pml4_ptr

align 8

ap_pml4_ptr:
    dq 0

global ap_stack

align 8

ap_stack:
    dq 0

; --------------------------------------------------
; GDT (minimal)
; --------------------------------------------------

align 8

gdt:
    dq 0

.code32:
    dq 0x00CF9A000000FFFF

.code64:
    dq 0x00AF9A000000FFFF

gdt_ptr:
    dw gdt_end - gdt - 1
    dd gdt

gdt_end: