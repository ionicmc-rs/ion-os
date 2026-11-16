[BITS 16]

section .ap_init exec
global smp_start16

extern ap_entry

smp_start16:

    mov al, 'A'
    out 0xE9, al

    cli

    ; -------------------------
    ; load temporary GDT
    ; -------------------------
    lgdt [gdt_ptr]

    mov al, 'B'
    out 0xE9, al

    ; -------------------------
    ; enable protected mode
    ; -------------------------
    mov eax, cr0
    or eax, 1
    mov cr0, eax

    mov al, 'C'
    out 0xE9, al

    jmp DWORD 0x08:protected_mode

; --------------------------------------------------

[BITS 32]

protected_mode:

    mov al, 'P'
    out 0xE9, al

    ; Entering protected mode only updates CS via the far jump above. Load a
    ; flat data/stack selector before any memory access.
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax

    ; Match the BSP's FPU/SSE setup before entering Rust or taking an
    ; interrupt. The interrupt context code uses FXSAVE/FXRSTOR on every CPU.
    mov eax, cr0
    and eax, ~(1 << 2)      ; EM = 0
    and eax, ~(1 << 3)      ; TS = 0
    or  eax,  (1 << 1)      ; MP = 1
    mov cr0, eax

    mov eax, cr4
    or  eax, 1 << 9         ; OSFXSR
    or  eax, 1 << 10        ; OSXMMEXCPT
    mov cr4, eax

    ; Enable the same XSAVE state components as the BSP when supported.
    mov eax, 1
    cpuid
    mov ebx, ecx            ; preserve CPUID.1:ECX feature bits
    test ebx, 1 << 26       ; XSAVE
    jz .fpu_ready

    mov eax, cr4
    or  eax, 1 << 18        ; OSXSAVE
    mov cr4, eax

    xor edx, edx
    xor ecx, ecx            ; XCR0
    mov eax, (1 << 0) | (1 << 1) ; x87 | SSE
    test ebx, 1 << 28       ; AVX
    jz .set_xcr0
    or eax, 1 << 2          ; YMM state
.set_xcr0:
    xsetbv
.fpu_ready:

    ; enable PAE
    mov eax, cr4
    or eax, (1 << 5)
    mov cr4, eax

    mov al, 'D'
    out 0xE9, al

    ; load kernel page table
    mov eax, [ap_pml4_ptr]
    mov cr3, eax

    mov al, 'E'
    out 0xE9, al

    ; enable long mode
    mov ecx, 0xC0000080
    rdmsr
    or eax, (1 << 8)
    wrmsr

    mov al, 'L'
    out 0xE9, al

    ; enable paging
    mov eax, cr0
    or eax, (1 << 31)
    mov cr0, eax

    jmp 0x18:long_mode

; --------------------------------------------------

[BITS 64]

long_mode:

    ; Match the BSP long-mode entry path before calling into Rust.
    mov ax, 0
    mov ss, ax
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax

    ; Select this AP's private stack by its initial APIC ID. The BSP can now
    ; start several APs without racing updates to one shared stack pointer.
    mov eax, 1
    cpuid
    shr ebx, 24
    lea rax, [rel ap_stacks_by_id]
    mov rsp, [rax + rbx * 8]

    call ap_entry

.hang:
    hlt
    jmp .hang

; --------------------------------------------------
; Data used by BSP to configure trampoline
; --------------------------------------------------

section .ap_data write

global ap_pml4_ptr

align 8

ap_pml4_ptr:
    dq 0

global ap_stacks_by_id

align 8

ap_stacks_by_id:
    times 256 dq 0

; --------------------------------------------------
; GDT (minimal)
; --------------------------------------------------

align 8

gdt:
    dq 0

.code32:
    dq 0x00CF9A000000FFFF

.data:
    dq 0x00CF92000000FFFF

.code64:
    dq 0x00AF9A000000FFFF

gdt_ptr:
    dw gdt_end - gdt - 1
    dd gdt

gdt_end:

section .note.GNU-stack noalloc noexec nowrite progbits
