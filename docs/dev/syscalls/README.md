# IonOS syscalls

we support syscalls on IonOS on all supported architectures:

- [x86_64](x86_64/README.md)

## General ABI

syscalls follow this ABI:

```asm

arg0: group
arg1: selection

arg2: syscall arg0
arg3: syscall arg1
arg4: syscall arg2
arg5: syscall arg3
arg6: syscall arg4

ret0: result
ret1: metadata/err
```

this is stable across all architectures, which registers are used however is arch-dependent.

## Reading this documentation

All syscall groups can be found here, each contains related operations for that group

- [G1 - process](../group1-process.md)
- [G2 - IO](../group2-io.md)
- [G3 - thread](../group3-thread.md)