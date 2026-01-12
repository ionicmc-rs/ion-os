//! Contains Definitions for a PIC8259 Controller
//! 
//! In reality, QEMU probably emulates an APIC controller, but backwards compatibility is supported, and a PIC controller is simpler to
//! Set up.
//! 
//! Typically, you do not use this at all - interrupt handlers will handle everything.
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

/// Index for Hardware Interrupts.
/// 
/// List
/// - Timer: 32
/// - Keyboard: 33
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    /// Index for a Timer Interrupt
    /// 
    /// The Timer Interrupt is used to detect the Intel 8253 hardware interrupt, which runs all the time at a fixed interval.
    /// 
    /// Equivalent to the [`PIC_1_OFFSET`]
    Timer = PIC_1_OFFSET,
    /// Index for a Keyboard Interrupt.
    /// 
    /// The keyboard interrupt is used to detect key inputs.
    /// 
    /// Equivalent to [`PIC_1_OFFSET`] + 1
    Keyboard,
}

impl InterruptIndex {
    /// Converts the index to a u8
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    /// Converts the index to a usize
    pub fn as_usize(self) -> usize {
        usize::from(self.as_u8())
    }
}

/// Contains the basic handlers for Hardware Interrupts.
pub mod handlers {
    use x86_64::structures::idt::InterruptStackFrame;

    /// Notifies that the interrupt handler has ended.
    /// 
    /// Requires an explicit `unsafe` keyword.
    pub macro notify {
        (unsafe $name:ident) => {
            unsafe {
                super::PICS.lock()
                    .notify_end_of_interrupt(super::InterruptIndex::$name.as_u8());
            }
        }
    }

    /// Intel 8253 timer interrupt.
    /// 
    /// simply notifies PIC that the interrupt was handled, but this is subject to change!
    pub extern "x86-interrupt" fn timer(_frame: InterruptStackFrame) {
        notify!(unsafe Timer);
    }
}