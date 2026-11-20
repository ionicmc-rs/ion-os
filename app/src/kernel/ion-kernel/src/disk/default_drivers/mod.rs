use crate::log;

pub mod virtio;

/// Probe all built-in disk drivers
pub fn init() {
    log::info!("disk: probing default drivers");

    // Order matters
    // virtio should be the last one.
    virtio::probe_and_register();
}
