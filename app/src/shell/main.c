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
            exit(1);
        
        write_stdout("\n", 1);
    }
}

#define ALIGN_UP(x,a) (((x)+(a)-1)&~((a)-1))

__attribute__((section(".ion_os.requests")))
__attribute__((used))
static const struct IonRequests ion_requests = {
    .magic = 0x524E4F49,  // "IONR"
    .version = 1,
    .size = ALIGN_UP(sizeof(struct IonRequests), 8),

    .stack_tag_ty = 1,
    .stack_tag_size = 
        sizeof(uint64_t)    // size
        + sizeof(uint64_t)  // align
        + sizeof(uint32_t)  // ty
        + sizeof(uint32_t), // size (this field)

    .stack_size = 0x40000,
    .stack_align = 16,

    .end_tag_ty = 0,
    .end_tag_size = 8 // with tag header
};