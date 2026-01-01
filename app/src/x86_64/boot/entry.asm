; ============================================
; Ion OS 32-bit bootstrap with debug prints
; - Robust paging bring-up (PML4->PDPT->PD 2MiB)
; - OSXSAVE/SSE/AVX enable (if supported)
; - VGA breadcrumbs + QEMU debug port (0xE9)
; ============================================

global start
global stack_top
extern long_mode_start           ; 64-bit entry point (defined in a separate bits 64 file)

section .data
global boot_info_data
boot_info_data:
    dd 0              ; +0x00 multiboot_magic
    dd 0              ; +0x04 multiboot_info
    dd 0              ; +0x08 cpuid_edx
    dd 0              ; +0x0C cpuid_ecx
    dq 0              ; +0x10 page_table_base
    dq 0              ; +0x18 stack_top
    dq 0              ; +0x20 framebuffer_addr
    dq 0              ; +0x28 memory_map_addr
    dq 0              ; +0x30 kernel_entry

section .text
bits 32
start:
    ; Set initial stack pointer
    mov     esp, stack_top


    ; Preserve entry registers
    push    eax
    push    ebx

    ; Marker S1: at very start
    mov     dword [0xb8000], 0x0F314F53      ; "S1"
    mov     al, '1'
    out     0xE9, al

    ; Store stack_top (u64)
    mov     eax, stack_top
    mov     [boot_info_data + 0x18], eax
    mov     dword [boot_info_data + 0x1C], 0

    mov     dword [0xb8000], 0x0F314F53      ; "S1"
    mov     al, '2'
    out     0xE9, al

    ; Restore multiboot registers
    pop     ebx               ; ebx = multiboot info
    pop     eax               ; eax = multiboot magic
    mov     [boot_info_data + 0x00], eax
    mov     [boot_info_data + 0x04], ebx

    call    check_multiboot
    ; TODO: Support for Mutltiboot1
    call    check_multiboot2_mmap

    ; Marker S3
    mov     dword [0xb8000+8], 0x0F334F53
    mov     al, '3'
    out     0xE9, al

    call    check_cpuid

    ; Marker S4
    mov     dword [0xb8000+12], 0x0F344F53
    mov     al, '4'
    out     0xE9, al

    call    check_long_mode

    ; Marker S5
    mov     dword [0xb8000+16], 0x0F354F53
    mov     al, '5'
    out     0xE9, al

    ; Enable OSXSAVE/SSE/AVX if supported
    call    enable_osxsave_and_avx

    ; Marker S6
    mov     dword [0xb8000+20], 0x0F364F53
    mov     al, '6'
    out     0xE9, al

    ; Build identity map (first 1 GiB with 2MiB pages)
    call    setup_page_tables

    ; Marker S7
    mov     dword [0xb8000+24], 0x0F374F53
    mov     al, '7'
    out     0xE9, al

    ; Enable paging and long mode
    call    enable_paging

    ; Marker SP: after paging enabled
    mov     dword [0xb8000+28], 0x0F504F53   ; "SP"
    mov     al, 'P'
    out     0xE9, al

    ; Load GDT and far jump to 64-bit entry
    lgdt    [gdt64.pointer]

    ; Marker SG
    mov     dword [0xb8000+32], 0x0F474F53   ; "SG"
    mov     al, 'G'
    out     0xE9, al

    jmp     gdt64.code_segment:long_mode_start

    ; If we reach here, far jump failed
    mov     dword [0xb8000+36], 0x0F464F53   ; "SF"
    mov     al, 'F'
    out     0xE9, al
    hlt

; --- Checks ---

check_multiboot1_mmap:
    ; EBX points to multiboot1 info struct
    mov     esi, ebx           ; info ptr
    mov     eax, [esi + 0x00]  ; flags
    test    eax, 1 << 6        ; bit 6 => mmap_* present
    jz      .no_mmap

    mov     eax, [esi + 0x30]  ; mmap_addr (phys)
    mov     [boot_info_data + 0x28], eax
    mov     dword [boot_info_data + 0x2C], 0  ; high dword zero for 64-bit field

    ; Optional: publish framebuffer_addr if bit 12 set (video info)
    ; mov     eax, [esi + 0x54] ; fb_addr low (if VBE/graphics provided)
    ; mov     [boot_info_data + 0x20], eax
    ; mov     dword [boot_info_data + 0x24], 0

    jmp     .done
.no_mmap:
    ; leave memory_map_addr = 0 to signal “not available”
    mov al, "M",
    jmp error
.done:
    ret

check_multiboot2_mmap:
    ; EBX -> multiboot2 info header
    mov     esi, ebx                  ; base
    mov     ecx, [esi + 0]            ; total_size
    sub     ecx, 8                    ; account for first 8 byte header.
    add     esi, 8                    ; first tag (skip header)

.next_tag:
    cmp     ecx, 0
    jbe     .done

    mov     eax, [esi + 0]            ; tag type
    mov     edx, [esi + 4]            ; tag size

    cmp     eax, 6                    ; memory map tag
    jne     .check_fb
    ; Publish the address of the entries (the payload after entry_size/version)
    ; entries start at esi + 16
    lea     eax, [esi + 16]
    mov     [boot_info_data + 0x28], eax
    mov     dword [boot_info_data + 0x2C], 0
    jmp     .advance

.check_fb:
    cmp     eax, 8                    ; framebuffer tag
    jne     .advance
    ; Tag layout: type=8, size, then fields; first qword is framebuffer address
    ; For simplicity publish fb addr low dword
    mov     eax, [esi + 8]            ; framebuffer_addr low
    mov     [boot_info_data + 0x20], eax
    mov     dword [boot_info_data + 0x24], 0

.advance:
    ; advance to next tag (size rounded up to 8-byte alignment)
    mov     eax, edx
    add     eax, 7
    and     eax, -8
    add     esi, eax
    sub     ecx, eax
    jmp     .next_tag
.error
    mov al, "M"
    jmp error
.done:
    ret

check_multiboot:
    ; Accept MB2 or MB1, depending on your loader
    cmp     eax, 0x36d76289              ; Multiboot2
    je      .ok
    cmp     eax, 0x2BADB002              ; Multiboot1
    jne     .no_multiboot
.ok:
    ret
.no_multiboot:
    mov     dword [0xb8000+40], 0x0F4D4F53  ; "SM"
    mov     al, 'M'
    out     0xE9, al
    mov     al, "M"
    jmp     error

check_cpuid:
    pushfd
    pop     eax
    mov     ecx, eax
    xor     eax, 1 << 21
    push    eax
    popfd
    pushfd
    pop     eax
    push    ecx
    popfd
    cmp     eax, ecx
    je      .no_cpuid

    mov     eax, 1
    cpuid
    mov     [boot_info_data + 0x08], edx
    mov     [boot_info_data + 0x0C], ecx
    ret
.no_cpuid:
    mov     dword [0xb8000+44], 0x0F434F53  ; "SC"
    mov     al, 'C'
    out     0xE9, al
    mov     al, "C"
    jmp     error

enable_osxsave_and_avx:
    mov     eax, 1
    cpuid
    mov     ebx, ecx            ; ECX features

    test    ebx, 1 << 26        ; XSAVE
    jz      .done

    ; CR0: EM=0, MP=1
    mov     eax, cr0
    and     eax, ~(1 << 2)      ; EM = 0
    or      eax,  (1 << 1)      ; MP = 1
    mov     cr0, eax

    ; CR4: OSFXSR=1, OSXMMEXCPT=1, OSXSAVE=1
    mov     eax, cr4
    or      eax, 1 << 9
    or      eax, 1 << 10
    or      eax, 1 << 18
    mov     cr4, eax

    ; XCR0: x87|SSE, optionally AVX
    xor     edx, edx
    mov     ecx, 0
    mov     eax, (1 << 0) | (1 << 1)
    test    ebx, 1 << 28        ; AVX
    jz      .set_xcr0
    or      eax, 1 << 2
.set_xcr0:
    xsetbv
.done:
    ret

check_long_mode:
    mov     eax, 0x80000000
    cpuid
    cmp     eax, 0x80000001
    jb      .no_long_mode

    mov     eax, 0x80000001
    cpuid
    test    edx, 1 << 29        ; LM
    jz      .no_long_mode
    ret
.no_long_mode:
    mov     dword [0xb8000+48], 0x0F4C4F53  ; "SL"
    mov     al, 'L'
    out     0xE9, al
    mov     al, "L"
    jmp     error

; --- Helpers: zero a 4KiB page at EDI ---
zero_page:
    push    eax
    push    ecx
    push    edi
    mov     ecx, 4096/4
    xor     eax, eax
    rep     stosd
    pop     edi
    pop     ecx
    pop     eax
    ret

; --- Paging setup (PML4 -> PDPT -> PD 2MiB identity map of first 1GiB) ---

setup_page_tables:
    ; Clear PML4, PDPT, PD
    mov     edi, page_table_l4
    call    zero_page
    mov     edi, page_table_l3
    call    zero_page
    mov     edi, page_table_l2
    call    zero_page

    ; PML4[0] -> PDPT
    mov     eax, page_table_l3
    or      eax, 0b11
    mov     [page_table_l4 + 0], eax
    mov     dword [page_table_l4 + 4], 0

    ; PDPT[0] -> PD
    mov     eax, page_table_l2
    or      eax, 0b11
    mov     [page_table_l3 + 0], eax
    mov     dword [page_table_l3 + 4], 0

    ; Fill 512 PDEs (2MiB pages): identity map 0..1GiB
    xor     ecx, ecx
.fill_pdes:
    mov     eax, 0x200000
    mul     ecx                               ; edx:eax = ecx * 2MiB
    or      eax, 0b10000011                   ; P|RW|PS
    mov     [page_table_l2 + ecx * 8 + 0], eax
    mov     [page_table_l2 + ecx * 8 + 4], edx
    inc     ecx
    cmp     ecx, 512
    jne     .fill_pdes

    ; Publish PML4 base
    mov     eax, page_table_l4
    mov     [boot_info_data + 0x10], eax
    mov     dword [boot_info_data + 0x14], 0
    ret

enable_paging:
    ; Marker S8: before paging
    mov     dword [0xb8000+52], 0x0F384F53
    mov     al, '8'
    out     0xE9, al

    ; Enable PAE first
    mov     eax, cr4
    or      eax, 1 << 5                      ; CR4.PAE
    mov     cr4, eax

    ; Load CR3 with PML4 phys
    mov     eax, page_table_l4
    mov     cr3, eax

    ; Enable Long Mode (EFER.LME)
    mov     ecx, 0xC0000080                  ; EFER
    rdmsr
    or      eax, 1 << 8                      ; LME
    wrmsr

    ; Enable paging (CR0.PG)
    mov     eax, cr0
    or      eax, 1 << 31
    mov     cr0, eax

    ; Post-paging breadcrumb
    mov     dword [0xb8000+56], 0x0F504F53   ; "SP"
    mov     al, 'p'
    out     0xE9, al
    ret

error:
    ; "ERR: X"
    mov     dword [0xb8000], 0x4f524f45
    mov     dword [0xb8004], 0x4f3a4f52
    mov     dword [0xb8008], 0x4f204f20
    mov     byte  [0xb800a], al
    mov     al, 'E'
    out     0xE9, al
    hlt

section .bss
align 4096
page_table_l4:
    resb 4096
page_table_l3:
    resb 4096
page_table_l2:
    resb 4096

stack_bottom:
    resb 4096 * 16 ; 64 KiB stack
stack_top:



section .rodata
gdt64:
    dq 0
.code_segment: equ $ - gdt64
    ; 64-bit code segment: R | E | S | P | L
    dq (1 << 41) | (1 << 43) | (1 << 44) | (1 << 47) | (1 << 53)
.pointer:
    dw $ - gdt64 - 1
    dq gdt64