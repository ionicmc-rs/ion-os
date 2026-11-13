use core::num::NonZeroU8;

use crate::{
    arch::{
        current_cpu_lapic,
        smp::{self, cpu_index_from_apic_id},
    },
    scheduler::Scheduler,
    serial_println,
};

pub struct CpuLocalStorage {
    pub id: u8,
    pub scheduler: Scheduler,
    pub event_logger_head: usize,
}

pub static mut CPU_LS: [Option<CpuLocalStorage>; 64] = [const { None }; 64];

pub fn init() {
    let count = smp::cpu_count();

    for x in 0..count {
        let scheduler = Scheduler::new();

        unsafe {
            CPU_LS[x] = Some(CpuLocalStorage {
                id: x as u8,
                scheduler,
                event_logger_head: 0,
            });
        }
    }
}

pub fn cpu_cls(id: usize) -> Option<&'static CpuLocalStorage> {
    unsafe { CPU_LS[id].as_ref() }
}

/// Get the current CPU's local storage
pub fn current() -> Option<&'static CpuLocalStorage> {
    // Use the per-CPU LAPIC to get the correct LAPIC ID for the current CPU
    let lapic = current_cpu_lapic()?;
    let apic_id = unsafe { lapic.id() };
    let index = cpu_index_from_apic_id(apic_id)?;
    cpu_cls(index)
}
