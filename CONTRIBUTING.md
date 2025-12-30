# Ion OS Contributing Guide

This Guide contains information on the Ion OS build system, the "bootloader" (more information later.), The C Entry, and the Kernel.

## A Big Thanks To
This os is heavily inspired by [Philipp Oppermann's blog](os.phill-opp.com) and `CodePulse`'s (2 Episode) [YouTube series](https://www.youtube.com/watch?v=FkrpUaGThTQ&t=503s), please read/watch before contributing!

## 1. Build System 
### Required Tools

We require a few tools, Most of them will be installed by default on your system.

1. Docker. (And preferably Docker Desktop)
2. QEMU (We Plan on adding support for other Virtual Machines.)
3. Make (installed by default on linux)

And thats about it, the rest is handled inside of docker.

### Building
This section only covers building, for running, see [Running](#running)

First, we need to enter docker.

For unix based devices (linux, macos), enter the docker build environment using:
```bash
docker run --rm -it -v $(pwd):/root/env ion-os-buildenv
```

> [!TODO]
> Add command for windows

The prompt should change to a basic, all-white, root prompt at /root/env (~/env)

---

Next, we will use make (on docker) to build required object files.

for now, do not worry of the implementation details, the will be explained later.

Run the following command:
```bash
make clean-build
```
For Tests (please see [Testing](#2-testing) before testing.)
```bash
make clean-test
```

`clean-` deletes any previous object files created, and then `build` or `test` specifies you what target to build.

You should see a lot of commands running, such as cargo build, nasm, the linker, and finally grub (xorriso). You will find the final ISO file at
dist/x86_64/kernel.iso
> [!NOTE]
> It is not necessary to use Docker, however it is recommended instead of installing executables on your device.
#### Implementation Details
The make file contains rules to build files only if they do not exist, which is why we use the `clean-` commands. If you wish, separate `build-x86_64-` commands exist for `build` and `test`, which do not clean the output files. This usually stops cargo from building.

Either way, the make file first finds all assembly files (app/x86_64/boot/(entry.asm, entry_64.asm, header.asm)), and uses `nasm`, and a path substitution to convert them to object files at build/x86_64/(file).o, which are object files

next, we find all C files (app/src/kernel/c_entry/main.c), and convert them to object files as well, using a `cross-compiler.`[^1] The object files can be found at build/x86_64/kernel/(file).o

finally, we convert our Rust code to object files, however, unlike C and ASM, Rust does not support single-file-compilation, instead, we use `cargo build`, with the out dir set to the build folder. we then copy the static object file (`.a`) to the `ion_kernel.a` file.

After creating all the object files, we then link everything together using a linker (`x86_64-elf-ld` in specific), with the output being dist/x86_64/kernel.bin

however, this is not the final result - most Virtual Machines expect an ISO File. Additionally, we haven't linked a bootloader (more information on that later) yet!

luckily - grub provides a tool (`grub-mk-rescue`) which creates an iso file, linking with the bootloader. This Creates our final ISO.

[^1]: A compiles that converts code to a target other than the host.
### Running
First you need to build the ISO. See [Building](#building) if you haven't already.

Outside of you `docker` environment (I'd recommend a separate terminal), run the following command to run using QEMU
```bash
make run-qemu
```
For Tests (please see [Testing](#2-testing) before testing.)
```bash
make run-qemu-tests
```

This uses QEMU to run our ISO, assuming you are at the home directory, and prints serial to the terminal.

> [!NOTE]
> We plan to add support for other virtual machines soon.
## 2. Testing
We do not support ASM and C testing, only rust.

Unfortunately, due to our minimal setup, [Philip's OS Guide](#a-big-thanks-to) cannot help us with testing, as we use grub instead of their bootloader.

Fortunately, I was able to create another solution using #\[cfg(...)\] statements.
### Writing Tests
to run code only when testing, using this cfg directive
```rust
#[cfg(feature = "test")]
pub fn my_code(...) {}
```
`my_code()` will only appear during Tests

to actually write tests, you need to have a type that implements the `Testable` trait.
```rust
trait Testable {
    fn run(info: TestInfo) -> TestResult;
}
```
For more information, view the [`code's documentation`](/app/src/kernel/ion-kernel/src/test.rs)

By Default, `Fn(TestInfo) -> TestResult` types will implement Testable.
```rust
use crate::test::{TestInfo, TestResult, test_assert_eq};
fn trivial_assertion(_: TestInfo) -> TestResult {
    test_assert_eq!(1, 1, "Huh?")
}
```

as you'll notice, `trivial_assertion` has no `#[test]`, as it isn't available in `#![no_std]`. Instead, add it to the test runner in main.

```rust
pub unsafe extern "C" fn rust_kernel_entry(boot_info: *const BootInfoC) -> ! {
    // ...

    cfg_if! {
        if #[cfg(feature = "test")] {
            run_tests(&[
                // ...
                &trivial_assertion
                // ...    
            ])
        }
    }

    // ...
}
```

> [!IMPORTANT]
> The `test` implementation details are currently unstable and subject to change, so keep that in mind!
>
> for example, there is a plan to switch over to `custom_test_frameworks`.

when you run tests, trivial_assertion will be ran, and you'll get a nice interface with data of tests in your terminal.
## 3. Documenting And Commenting Standards
Mostly, we follow rust's core/std libraries' linting/commenting/documenting standards. This mostly applies to `unsafe` code.

Here is an example
```rust
/// Represents an exit code.
/// 
/// The Exit code can either be `Ok` or `Failure`, which represent `1` and `0` respectively. This type also implements `TryFrom<{integer/float}>,
/// where 1 and 0 map to the exit codes.
/// # Platform Specifics
/// This enum behaves the same on all platforms // do not actually do this in code.
/// # Example
/// ```
/// use crate::{ExitCode, exit};
/// 
/// fn may_fail() -> Result<(), &'static str> {
///     Err("Failed!");
/// }
/// 
/// if may_fail().is_err() {
///     exit(ExitCode::Failure)
/// } else {
///     exit(ExitCode::Ok)
/// }
/// ```
#[repr(u8)] // more on this in the next section
pub enum ExitCode {
    Ok,
    Failure
}

impl ExitCode {
    /// Converts a 1 or 0 to an ExitCode
    /// 
    /// # Safety
    /// the argument must be 1 or 0.
    /// # Example
    /// ```
    /// ...
    /// ```
    pub unsafe fn from_u8_unchecked(int: u8) -> Self {
        // do not actually do this in code
        
        // Safety: the caller ensures safety
        unsafe {
            core::mem::transmute::<u8, self>(int);
        }
    }
}
```

The General Format for docs is:
```rust
/// [Summary]
/// 
/// [Desc.]
/// # <Safety>
/// [Unsafe Preconditions]
/// # Example(s)
/// [Example(s)]
```
Examples may be omitted for private interfaces and/or irrelevant items (rust_kernel_entry, _print, etc.)

> [!NOTE]
> By Default, Clippy will force you to have `# Safety` docs, but `// Safety:` comments.

For Safety Comments:
```rust
// Safety: ...
```
## 4. Size Concerns
Currently, our stack is very limited - only 64 KIB. While it is being worked on, we still recommend using `Box`, `Vec`, and other alloc types
(when we implement them) for now, as we do not have enough memory, and finishing the memory causes [triple faults]()

The issue for this can be found [here](#1)

## 5. File Structure
Usually, when you have more than one module related to one topic, you should have a folder housing all the modules

Example
```
// incorrect
c_types.rs
c_fns.rs

// correct
c_link
 | - c_types.rs
 | - c_fns.rs
```

## 6. Github Workflow
This section contains instructions on how to use github to contribute.

### Issues

There are 3 types of Github Issues

- tracking:
  - these track a feature implementation/removal.
  - similar to Rust's.
- reports:
  - report a problem with the OS.
- enhance:
  - requests for features, docs, or other items.

Use labels properly, and prefer to use templates.

### Pull Requests

Unfortunately, we currently don't have a proper way to test code using github workflows.

for Your Pull Request to be accepted, the following criteria must be met:

- tests must pass
  - you must also add tests.
- You must have a tracking issue for this feature, or at least it must be referenced in it.
- The code must compile on all targets.
- The code must properly implement the required feature.

### Discussions

You can gather opinions/constructive criticism through Discussions.

### Branches

Each Feature should have its own branch until it is complete, where it is then merged into the branch for the current version, which is eventually merged into main.

### Releases

Release Version Format 
```
total.major.minor
```

the total is always 0 until we reach some type of big milestone, for example, graphical interface

## Info
This Sections Contains all the info about IonOS

### BootLoader
bootloader: Grub
- package `grub-common` 2.06-13+deb12u1
- package `grub-pc-bin` 2.06-13+deb12u1

system: multiboot2

bits: 64

paging: yes

stack size: (CURRENT) 64 KiB

stores boot info in the following format(s)
```asm
boot_info_data:
    dd 0              ; +0x00 multiboot_magic
    dd 0              ; +0x04 multiboot_info
    dd 0              ; +0x08 cpuid_edx
    dd 0              ; +0x0C cpuid_ecx
    dq 0              ; +0x10 page_table_base
    dq 0              ; +0x18 stack_top
    dq 0              ; +0x20 framebuffer_addr
    dq 0              ; +0x28 memory_map_addr
    dq 0              ; +0x30 kernel_entry
```
C:
```c
typedef struct {
    uint32_t multiboot_magic;
    uint32_t multiboot_info;
    uint32_t cpuid_edx;
    uint32_t cpuid_ecx;
    uint64_t page_table_base;
    uint64_t stack_top;
    uint64_t framebuffer_addr;
    uint64_t memory_map_addr;
    uint64_t kernel_entry;
    uint64_t boot_entry;
} BootInfo;
```
converts to:
```c
typedef struct {
    struct BootInfo* data;
    bool valid;
} ValidatedBootInfo;
```
...which is passed into Rust.
```rust
#[repr(C)]
pub struct BootInfoC {
    input_ptr: *const BootInfoInput,
    valid: bool
}
```
...which is converted to:
```rust
/// Boot info, with proper Rust types.
pub struct BootInfo {
    /// Multiboot magic value
    /// 
    /// equal to 0x36d76289
    pub multiboot_magic: u32,
    /// multiboot info, stored as a 32bit pointer.
    pub multiboot_info: SmallPtr<()>, // this should be a ptr, but it is 32 bits, and we must keep the
                                      // the memory layout the same as BootInfoInput
    /// The edx register after querying the `cpuid` command
    /// 
    /// this is stored as bit flags.
    pub cpuid_edx: BitFlags,
    /// The ecx register after querying the `cpuid` command
    /// 
    /// this is stored as bit flags
    pub cpuid_ecx: BitFlags,
    /// pointer to page table base
    pub page_table_base: NonNull<()>,
    /// pointer to stack top
    pub stack_top: NonNull<()>,
    /// pointer to frame buffer, always null for now.
    /// 
    /// WARNING: We will change this to a `NonNull` eventually, so plan accordingly.
    pub frame_buffer: *const (),
    /// pointer memory map, always null for now.
    /// 
    /// WARNING: We will change this to a `NonNull` eventually, so plan accordingly.
    pub mem_map_addr: *const (),
    /// C kernel entry, as a function pointer
    /// 
    /// note: one of the unsafe preconditions to call this function is that nothing is initialized yet, however,
    /// by calling this function at any point after the init call in [`rust_kernel_entry`](crate::rust_kernel_entry), 
    /// we have violated this.
    /// 
    /// TLDR: do not call this.
    pub kernel_entry: unsafe extern "C" fn(BootInfoInput) -> !
}
```
For more information, read the [Code's Documentation](/app/src/kernel/ion-kernel/src/c_lib/mod.rs)

### Kernel
languages: 
- Rust (nightly latest)
```
rustc 1.94.0-nightly (21cf7fb3f 2025-12-28)
```
- C
```
Using built-in specs.
COLLECT_GCC=x86_64-elf-gcc
COLLECT_LTO_WRAPPER=/usr/local/libexec/gcc/x86_64-elf/13.2.0/lto-wrapper
Target: x86_64-elf
Configured with: ../gcc-13.2.0/configure --target=x86_64-elf --prefix=/usr/local --disable-nls --enable-languages=c,c++ --without-headers
Thread model: single
Supported LTO compression algorithms: zlib
gcc version 13.2.0 (GCC)
```
- NASM
```
NASM version 2.16.01
```
target: `x86_64-unknown-none`
```json
{
    "llvm-target": "x86_64-unknown-none",
    "data-layout": "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128",
    "arch": "x86_64",
    "target-endian": "little",
    "target-pointer-width": 64,
    "target-c-int-width": 32,
    "os": "none",
    "executables": true,
    "linker-flavor": "ld.lld",
    "linker": "rust-lld",
    "panic-strategy": "abort",
    "disable-redzone": true,
    "features": "-mmx,-sse,+soft-float",
    "rustc-abi": "x86-softfloat"
}
```
build system: Cargo (integrated with ASM + C)
```
cargo 1.94.0-nightly (94c368ad2 2025-12-26)
```
panic handler: custom, no_std

linker: `x86_64-elf-ld` with explicit layout validation
```
GNU ld (GNU Binutils) 2.41
Copyright (C) 2023 Free Software Foundation, Inc.
This program is free software; you may redistribute it under the terms of
the GNU General Public License version 3 or (at your option) a later version.
This program has absolutely no warranty.
```

### Memory
frame allocator: bootloader memory map parser

page tables: dynamic mapping, 4-level (PML4)

allocation: static + dynamic hybrid

logging: zero-allocation, compile-time filtered macros

### Interrupts
IDT: from the x86_64 crate, as InterruptDescriptorTable

The IDT has a large size, which may cause the stack to be used up.

List of handled Interrupts:
- breakpoints
- double faults
- page faults

> [!IMPORTANT]
> im bad at writing info, please go easy on me :(