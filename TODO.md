# Plan: Fix ELF Loading - #UD Issue

## Problem Analysis
- User threads currently get a #UD (undefined instruction) fault
- Root cause: The user context's CR3 is never set to the user's address space
- In `load_elf()`, a new address space is created but not attached to the context

## Solution
1. Modify `load_elf()` in `elf.rs` to set the address space on the context after creating it
2. Need to modify `LoadedElf` struct to include the address space
3. Modify `Thread::for_user()` in `thread/mod.rs` to set the address space on the context

## Files to Edit
1. `app/src/kernel/ion-kernel/src/process/elf.rs` - Set address space on context
2. `app/src/kernel/ion-kernel/src/thread/mod.rs` - Apply address space to thread context

## Implementation Steps
1. In `elf.rs`: Add address space to `LoadedElf` struct
2. In `elf.rs`: Set the address space CR3 on the context before returning
3. In `thread/mod.rs`: Apply the address space to the thread's context

