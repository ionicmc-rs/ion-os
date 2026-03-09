use crate::{arch::{self, interrupts, smp::{self, discover_cpus, get_madt}}, hlt_loop, log};

pub struct CpuInfo(smp::CpuInfo);

unsafe extern "C" {
    static smp_start16: u8;
}

pub fn start() {
    let madt = get_madt();

    unsafe { discover_cpus(madt.0, madt.1) };

    for x in 0..smp::cpu_count() {
        let cpu = smp::cpu(x);

        log::debug!("discovered CPU{x}: {cpu:#?}");
    }

    if smp::cpu_count() == 0 {
        log::debug!("no other cores found!");
    }

    // actual start

    unsafe { smp::start_aps( &smp_start16 as *const u8 as u64) };
}

#[unsafe(no_mangle)]
pub extern "C" fn ap_entry() -> ! {
    log::debug!("Hello from the other core!");

    hlt_loop()
}