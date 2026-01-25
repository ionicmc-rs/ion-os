// app/src/kernel/ion-kernel/src/disk/default_drivers/mod.rs

use crate::log;

pub mod virtio;

/// Probe all built-in disk drivers
pub fn init() {
    log::info!("disk: probing default drivers");

    // Order matters later (virtio first is fine)
    virtio::probe_and_register();
}

#[cfg(feature = "test")]
#[allow(missing_docs)]
pub mod test;