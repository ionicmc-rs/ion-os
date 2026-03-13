# I/O

This syscall group contains operations for accessing and managing File Descriptors (I/O).

## Available syscalls

### io.write

selector = 0

arguments
- arg0: fd (usize)
- arg1: buffer (*u8)
- arg2: length (usize)

#### Description

Writes to `fd` (file descriptor) from `buffer`, operating on `length` bytes.

#### Returns

returns the amount of bytes written.

#### Errors

on error, the return value is -1.

possible error return values:

| err | meaning
|-----|---------
| 1 | the fd does not exist
| 2 | the fd is not writable
| 3 | the fd is locked (by another thread)
| 4 | the buffer was 0 sized
| 5 | the buffer exceeded the maximum length (64 KiB)