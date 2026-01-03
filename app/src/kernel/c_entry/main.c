// This file only validates the boot info and passes it to rust.

#include <stdbool.h>
#include <stdint.h>

// BootInfo definition
typedef struct {
    uint32_t multiboot_magic;
    uint32_t multiboot_info;
    uint32_t cpuid_edx;
    uint32_t cpuid_ecx;
    uint64_t page_table_base;
    uint64_t stack_top;
    uint64_t framebuffer_addr;
    uint64_t memory_map_addr;
    uint64_t kernel_entry;
    uint64_t boot_entry;
} BootInfo;

// Wrapper struct for validation
typedef struct {
    const BootInfo* input;  // pointer to the BootInfo
    bool validity;          // result of validation
} ValidatedBootInfo;

// Forward declaration of Rust entry
extern void rust_kernel_entry(const ValidatedBootInfo* boot_info);
extern int* __errno_location();

// Validation function
static bool validate_boot_info(const BootInfo* bi) {
    int* errno = __errno_location();
    if (bi->multiboot_magic != 0x36d76289) {
        *errno = 7;
        return false;
    };
    if (bi->page_table_base == 0 || (bi->page_table_base & 0xFFF) != 0) {
        *errno = 2;
        return false;
    };
    if (bi->stack_top == 0 || (bi->stack_top & 0xF) != 0) {
        *errno = 2;
        return false;
    };
    if (bi->kernel_entry == 0) {
        *errno = 2;
        return false;
    }
    if (bi->framebuffer_addr == 0 ) {
        *errno = 2;
        return false;
    };
    if (bi->memory_map_addr == 0) {
        *errno = 2;
        return false;
    };
    return true;
}

// Kernel entry in C
void kernel_main(const BootInfo* bi) {
    volatile unsigned short* vga = (unsigned short*)0xB8000;

    // Put "OK" at top-left in light gray on black
    vga[0] = 0x074F; // 'O'
    vga[1] = 0x074B; // 'K'

    // Show multiboot magic for sanity
    vga[2] = 0x0730 + ((bi->multiboot_magic >> 0) & 0xF);
    vga[3] = 0x0730 + ((bi->multiboot_magic >> 4) & 0xF);

    ValidatedBootInfo vbi = {
        .input = bi,
        .validity = validate_boot_info(bi)
    };

    rust_kernel_entry(&vbi);
}
