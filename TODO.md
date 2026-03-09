# TODO - Fix CPU Local Storage

## Task: Fix the current issues in CPU local storage
- `arch` impl is too exposed
- It will always get CPU0 TLS

## Completed:

### Step 1: Fix `arch/x86_64/interrupts/apic.rs` ✅
- Kept `CPU0_LAPIC` and `cpu_lapic()` for BSP-only operations
- Added per-CPU LAPIC array `CPUS_LAPIC: [MaybeUninit<LocalApic>; 64]`
- Added `current_cpu_lapic()` function that returns current CPU's LAPIC
- Added `init_cpu_lapic(index)` to initialize a specific CPU's LAPIC
- Added `init_bsp_cpu_lapic()` to initialize BSP's per-CPU LAPIC entry

### Step 2: Update `arch/x86_64/interrupts/mod.rs` ✅
- Re-exported all new functions: `cpu_lapic`, `current_cpu_lapic`, `init_cpu0_lapic`, `init_cpu_lapic`, `init_bsp_cpu_lapic`
- Added call to `init_bsp_cpu_lapic()` during boot initialization

### Step 3: Fix `cls.rs` ✅
- Updated `current()` to use `current_cpu_lapic()` instead of `cpu_lapic()`

### Step 4: Update `smp/mod.rs` ✅
- Added call to `init_cpu_lapic(index)` for each AP during initialization

### Step 5: Update `arch/x86_64/mod.rs` ✅
- Re-exported all LAPIC functions from the interrupts module

