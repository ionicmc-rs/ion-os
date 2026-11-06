#pragma once
#include <stdint.h>
#include <stddef.h>

#define BUFFER_LEN 256

static char prompt[] = "ish$ ";
static char welcome_message[] = "Welcome to the Ion Shell (ish)!\nType 'help' for a list of commands.\n";

void write_stdout(const char* buf, size_t len);
int read_stdin_line(char* buffer, size_t max);

__attribute__((noreturn))
void exit(int status);