use pic8259::ChainedPics;
use spin;

/// 1st Offset use for [`PICS`]
pub const PIC_1_OFFSET: u8 = 32;
/// 2st Offset use for [`PICS`]
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

/// PIC8259 Controller. (Hardware Controller.)
/// 
/// Likely, this will not actually be used, but it is easier to set up, and modern APIC controllers
/// accept this.
pub static PICS: spin::Mutex<ChainedPics> =
    spin::Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

/// Inits the PIC8259 Controller.
pub fn init() {
    unsafe { PICS.lock().initialize() };
}