#pragma once
#include <stdint.h>
#include <stddef.h>

/**
 * Represents the .ion_os.requests section of an Ion OS ELF
 */
struct IonRequests {
    uint32_t magic;
    uint16_t version;
    uint16_t size;
    uint64_t stack_size;
    uint64_t stack_align;
} __attribute__((packed));

static inline long syscall(
    long group,
    long sel,
    long arg0,
    long arg1,
    long arg2,
    long arg3,
    long arg4
) {
    register long r10 __asm__("r10") = arg2;
    register long r8  __asm__("r8")  = arg3;
    register long r9  __asm__("r9")  = arg4;

    long ret;

    __asm__ volatile (
        "syscall"
        : "=a"(ret)
        : "a"(group),
          "D"(sel),
          "S"(arg0),
          "d"(arg1),
          "r"(r10),
          "r"(r8),
          "r"(r9)
        : "rcx", "r11", "memory"
    );

    return ret;
}

#ifndef ION_SYSCALLS
#define ION_SYSCALLS

#define SYS_GROUP_PROC 1
#define SYS_GROUP_IO 2

#define SYS_PROC_EXIT 0

#define SYS_IO_WRITE 0
#define SYS_IO_READ 1

#define WSTDOUT_FD 1
#define RSTDIN_FD 4

#endif