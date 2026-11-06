#include "console.h"
#include "ion_os.h"

static int strings_equal(const char *left, const char *right) {
    while (*left && *right && *left == *right) {
        ++left;
        ++right;
    }

    return *left == *right;
}

static size_t string_length(const char *text) {
    size_t length = 0;
    while (text[length])
        ++length;
    return length;
}

static void print_help(void) {
    static const char message[] =
        "Built-in commands:\n"
        "  help          Show this list\n"
        "  echo <text>   Print text\n"
        "  clear         Clear the terminal\n"
        "  exit          Exit the shell\n";
    write_stdout(message, sizeof(message) - 1);
}

static void run_command(char *line) {
    char *command = line;

    while (*command == ' ' || *command == '\t')
        ++command;

    if (*command == '\0')
        return;

    char *argument = command;
    while (*argument && *argument != ' ' && *argument != '\t')
        ++argument;

    if (*argument) {
        *argument++ = '\0';
        while (*argument == ' ' || *argument == '\t')
            ++argument;
    }

    if (strings_equal(command, "help")) {
        print_help();
    } else if (strings_equal(command, "echo")) {
        write_stdout(argument, string_length(argument));
        write_stdout("\n", 1);
    } else if (strings_equal(command, "clear")) {
        static const char clear_screen[] = "\033[2J\033[H";
        write_stdout(clear_screen, sizeof(clear_screen) - 1);
    } else if (strings_equal(command, "exit")) {
        exit(0);
    } else {
        static const char unknown[] = "ish: command not found\n";
        write_stdout(unknown, sizeof(unknown) - 1);
    }
}

__attribute__((noreturn))
void _start() {

    write_stdout(welcome_message, sizeof(welcome_message) - 1);

    char buffer[BUFFER_LEN];

    while (1) {
        write_stdout(prompt, sizeof(prompt) - 1);

        int len = read_stdin_line(buffer, BUFFER_LEN);

        if (len < 0)
            exit(1);
        
        write_stdout("\n", 1);
        run_command(buffer);
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
