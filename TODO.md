# Triple Fault After ELF Loading - Analysis and Plan

## Problem Analysis

The kernel triple faults immediately after loading an ELF file. This happens because:

### Root Cause
The ELF loader in `app/src/kernel/ion-kernel/src/process/elf.rs` has a critical bug:

1. **Creates new address space** via `address_space::create_address_space()`
2. **Maps user data region** via `map_user_data(&mut space)` - this only maps 4MB at `USER_DATA_BASE` (0x7000_0000)
3. **Allocates virtual addresses** for ELF segments using `user_alloc.allocate()`
4. **Copies ELF data** to these virtual addresses

**The bug**: The code allocates virtual addresses and copies data to them, but **never allocates physical frames and maps them in the page table**. When the user thread executes and tries to access these addresses, the CPU generates a page fault because there's no valid page table entry - leading to a triple fault.

## Solution Plan

The fix requires modifying the ELF loading code to:

1. **For each PT_LOAD segment:**
   - Allocate physical frames for the segment
   - Map those physical frames to the virtual addresses in the page table
   - Then copy the ELF data to those now-valid virtual addresses

2. **For the stack:**
   - Allocate physical frames for the stack
   - Map them in the page table

## Files to Modify

- `app/src/kernel/ion-kernel/src/process/elf.rs` - Main ELF loading logic

## Implementation Steps

1. Modify the segment mapping loop to:
   - Allocate physical frames for each PT_LOAD segment
   - Map the frames to virtual addresses using page table functions
   - Copy data after mapping is established

2. Modify the stack allocation to:
   - Allocate physical frames for the stack
   - Map them in the page table

## Dependent Files

The following files contain the necessary infrastructure:
- `app/src/kernel/ion-kernel/src/arch/x86_64/mmu.rs` - Contains `map_4k` function
- `app/src/kernel/ion-kernel/src/arch/frame_allocator.rs` - Contains `alloc_4k_frame`
- `app/src/kernel/ion-kernel/src/address_space/mod.rs` - Contains `map_4k_into_space`

