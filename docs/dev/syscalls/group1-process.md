# Process

This syscall group contains operations for managing processes.

## Available syscalls

### process.exit

selector = 0

arguments:
- arg0: exit code (i32)

#### Description

this syscall exits the current process with the given exit code.

this is preferred over [thread.exit] when the entire process must exit, rather than one thread.