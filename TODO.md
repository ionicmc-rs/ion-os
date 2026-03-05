# IonShell Implementation TODO

## Phase 1: Kernel Syscalls - DONE
- [x] Add exec syscall to load and execute ELF programs (stub)
- [x] Add wait syscall to wait for child processes (stub)
- [x] Add pipe syscall for IPC (using existing stream pairs)
- [x] Add dup2 syscall for redirection
- [x] Add open syscall for file operations (stub)
- [x] Add close syscall
- [x] Add getcwd syscall
- [x] Add chdir syscall

## Phase 2: Userspace API - DONE
- [x] Update ion_api syscall wrappers

## Phase 3: Shell Implementation - DONE
- [x] Create shell entry point with .ion_os.requests
- [x] Implement REPL loop
- [x] Implement command parsing
- [x] Implement builtin commands (cd, pwd, exit, help)
- [ ] Implement external program execution (stub - needs exec implementation)
- [ ] Implement pipes (|)
- [ ] Implement redirection (>, >>, <)

## Phase 4: Testing
- [x] Build the shell
- [ ] Port to disk image
- [ ] Test in QEMU

