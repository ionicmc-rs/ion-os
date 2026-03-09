//! Tools for x86_64 specific code

/// const defs
pub mod defs;
/// cpu instructions
pub mod cpu;
/// interrupt instructions
pub mod interrupts;
pub use interrupts::{cpu_lapic, current_cpu_lapic, LocalApic, init_cpu0_lapic, init_cpu_lapic, init_bsp_cpu_lapic};
/// context
pub mod ctx;
/// Thread-local storage
pub mod tls;
/// allocating frames
pub mod frame_allocator;
/// memory paging
pub mod mmu;
/// Ports
pub mod port;
/// Syscall **implementation**
pub mod syscall;
/// Multi-core **initialization**
pub mod smp;