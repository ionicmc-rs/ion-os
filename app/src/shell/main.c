#include <stdint.h>
#include "ion_os.h"

#include <stdint.h>
#include "console.h"
#include "ion_os.h"

__attribute__((noreturn))
void _start() {

    char buffer[BUFFER_LEN];

    while (1) {
        write_stdout(prompt, sizeof(prompt) - 1);

        int len = read_stdin_line(buffer, BUFFER_LEN);

        if (len < 0)
            continue;
    }
}

__attribute__((section(".ion_os.requests")))
__attribute__((used))
static const struct IonRequests ion_requests = {
    .magic = 0x524E4F49,  // "IONR"
    .version = 1,
    .size = sizeof(struct IonRequests),
    .stack_size = 0x40000,
    .stack_align = 16,
};