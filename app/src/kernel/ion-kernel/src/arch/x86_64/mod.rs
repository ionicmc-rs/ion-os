//! Tools for x86_64 specific code

/// cpu instructions
pub mod cpu;
/// const defs
pub mod defs;
/// interrupt instructions
pub mod interrupts;
pub use interrupts::{
    LocalApic, cpu_lapic, current_cpu_lapic, init_bsp_cpu_lapic, init_cpu_lapic, init_cpu0_lapic,
};
/// context
pub mod ctx;
/// allocating frames
pub mod frame_allocator;
/// memory paging
pub mod mmu;
/// Ports
pub mod port;
/// Multi-core **initialization**
pub mod smp;
/// Syscall **implementation**
pub mod syscall;
/// Thread-local storage
pub mod tls;
