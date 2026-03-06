#include "console.h"
#include "ion_os.h"

void write_stdout(const char* buf, size_t len) {
    syscall(
        SYS_GROUP_IO,
        SYS_IO_WRITE,
        WSTDOUT_FD,
        (long)buf,
        len,
        0,
        0
    );
}

int read_stdin_line(char* buffer, size_t max) {
    size_t i = 0;

    while (1) {
        char c;

        long r = syscall(
            SYS_GROUP_IO,
            SYS_IO_READ,
            RSTDIN_FD,
            (long)&c,
            1,
            0,
            0
        );

        if (r == 0)
            continue;

        if (c == '\n')
            break;

        buffer[i++] = c;

        if (i >= max)
            return -1;
    }

    buffer[i] = 0;
    return i;
}

__attribute__((noreturn))
void exit(int status) {
    syscall(
        SYS_GROUP_PROC,
        SYS_PROC_EXIT,
        status,
        0,
        0,
        0,
        0
    );

    for (;;); // should never return
}