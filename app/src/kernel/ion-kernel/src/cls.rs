use core::num::NonZeroU8;

use crate::{arch::smp, scheduler::Scheduler};

pub struct CpuLocalStorage {
    pub id: u8,
    pub scheduler: Scheduler
}

pub static mut CPU_LS: [Option<CpuLocalStorage>; 64] = [const { None }; 64];

pub fn init() {
    let count = smp::cpu_count();

    for x in 0..count {
        let scheduler = Scheduler::new();

        unsafe { CPU_LS [x-1] = Some(CpuLocalStorage {
            id: x as u8,
            scheduler
        }); }
    }
}

pub fn cpu_cls(id: usize) -> Option<&'static CpuLocalStorage> {
    unsafe { CPU_LS [id].as_ref() }
}