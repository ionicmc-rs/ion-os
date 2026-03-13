# x86_64 sycalls on IonOS

on x86_64, syscalls use the `syscall` instruction, with the following ABI.

```asm
rax -> group
rdi -> group selector

rsi -> arg0
rdx -> arg1
r10 -> arg2
r8  -> arg3
r9  -> arg4

rax -> result
rdi -> metadata/err
```

## Saved registers

syscalls follow the SysV ABI (except for rcx). The following (callee-saved) registers will NOT be lost on syscall:

```asm
rbx
rbp
r12
r13
r14
r15
rsp
rip
```

however, the following must be assumed to be lost.

```asm
rax
rdi
rsi
rdx
r10
r8
r9

# and these are clobbered by the CPU itself.
rcx
r11
```

## Example

here is an example of process.exit

```asm
mov rax, 1    # process
mov rdi, 0    # exit
mov rsi, 0    # code

syscall       # exit(0)
```