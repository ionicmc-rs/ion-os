//! Minimal PS/2 keyboard scancode-set control (0xF0).
//! - Set: send 0xF0, then 1/2/3; expect ACK (0xFA) or RESEND (0xFE).
//! - Get: send 0xF0, then 0; expect ACK, then a value indicating set.
//! 
//! Handles both "raw" (1/2/3) and "translated" (0x43/0x41/0x3F) returns.

use pc_keyboard::{ScancodeSet1, ScancodeSet2};
use x86_64::instructions::port::Port;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ps2Resp {
    Ack,    // 0xFA
    Resend, // 0xFE
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScancodeSet {
    Set1,
    Set2,
    Set3,
    None
}

impl pc_keyboard::ScancodeSet for ScancodeSet {
    fn advance_state(&mut self, code: u8) -> Result<Option<pc_keyboard::KeyEvent>, pc_keyboard::Error> {
        match self {
            // if we have not set, use set 1
            // if the set is 3, we do not support that (yet), use 1 instead.
            ScancodeSet::None  |
             ScancodeSet::Set1 |
             ScancodeSet::Set3
                => ScancodeSet1::new().advance_state(code),
            ScancodeSet::Set2 => ScancodeSet2::new().advance_state(code)
        }
    }
}

#[derive(Debug)]
pub enum Ps2Error {
    Timeout,
    ResendLimit,
    UnexpectedByte(u8),
    NoAck,
    InvalidScancodeId(u8),
}

/// Abstract PS/2 I/O. Implement for your controller.
/// Typical flow: wait input buffer clear, write data; then poll for bytes.
pub trait Ps2Io {
    /// Write a data byte to the PS/2 device.
    fn write_data(&mut self, byte: u8) -> Result<(), Ps2Error>;
    /// Read a data byte with a timeout (device-to-host).
    fn read_data(&mut self) -> Result<u8, Ps2Error>;
    /// Optional: small delay or controller status checks (can be no-op).
    fn tiny_delay(&mut self) {}
}

#[derive(Debug, Clone, Copy)]
pub struct DefaultIO;

impl Ps2Io for DefaultIO {
    fn write_data(&mut self, byte: u8) -> Result<(), Ps2Error> {
        let mut status_port: Port<u8> = Port::new(0x64);
        let mut data_port: Port<u8> = Port::new(0x60);

        // Wait until input buffer is clear (bit 1 == 0)
        for _ in 0..100_000 {
            let status = unsafe { status_port.read() };
            if status & 0x02 == 0 {
                unsafe { data_port.write(byte) };
                return Ok(());
            }
        }
        Err(Ps2Error::Timeout)
    }

    fn read_data(&mut self) -> Result<u8, Ps2Error> {
        let mut status_port: Port<u8> = Port::new(0x64);
        let mut data_port: Port<u8> = Port::new(0x60);

        // Wait until output buffer is full (bit 0 == 1)
        for _ in 0..100_000 {
            let status = unsafe { status_port.read() };
            if status & 0x01 != 0 {
                let byte = unsafe { data_port.read() };
                return Ok(byte);
            }
        }
        Err(Ps2Error::Timeout)
    }

    fn tiny_delay(&mut self) {
        // Legacy I/O delay using port 0x80
        let mut delay_port: Port<u8> = Port::new(0x80);
        for _ in 0..10 {
            unsafe { delay_port.write(0u8) };
        }
    }
}

/// Translate byte to Ps2Resp if applicable.
fn parse_resp(b: u8) -> Option<Ps2Resp> {
    match b {
        0xFA => Some(Ps2Resp::Ack),
        0xFE => Some(Ps2Resp::Resend),
        _ => None,
    }
}

/// Keyboard command: 0xF0 (get/set current scan code set)
const CMD_SCANCODE_SET: u8 = 0xF0;

/// Subcommand values for 0xF0
const SUB_GET: u8 = 0x00;
const SUB_SET1: u8 = 0x01;
const SUB_SET2: u8 = 0x02;
const SUB_SET3: u8 = 0x03;

/// "Translated" identifiers sometimes returned on GET (post-ACK).
const ID_SET1_TRANS: u8 = 0x43;
const ID_SET2_TRANS: u8 = 0x41;
const ID_SET3_TRANS: u8 = 0x3F;

/// Map returned identifier to ScancodeSet, handling both raw and translated cases.
fn map_scancode_id(id: u8) -> Option<ScancodeSet> {
    match id {
        // Raw identifiers (1/2/3)
        0x01 => Some(ScancodeSet::Set1),
        0x02 => Some(ScancodeSet::Set2),
        0x03 => Some(ScancodeSet::Set3),
        // Translated identifiers (0x43/0x41/0x3F)
        ID_SET1_TRANS => Some(ScancodeSet::Set1),
        ID_SET2_TRANS => Some(ScancodeSet::Set2),
        ID_SET3_TRANS => Some(ScancodeSet::Set3),
        _ => None,
    }
}

/// Send a byte and expect ACK or RESEND; perform limited resends.
/// Returns Ok(()) after a final ACK; Err on timeout or exceeding retries.
fn send_with_ack<I: Ps2Io>(io: &mut I, byte: u8, max_resends: usize) -> Result<(), Ps2Error> {
    let mut tries = 0;
    loop {
        io.write_data(byte)?;
        let resp = io.read_data()?;
        match parse_resp(resp) {
            Some(Ps2Resp::Ack) => return Ok(()),
            Some(Ps2Resp::Resend) if tries < max_resends => {
                tries += 1;
                io.tiny_delay();
                continue;
            }
            Some(Ps2Resp::Resend) => return Err(Ps2Error::ResendLimit),
            None => return Err(Ps2Error::UnexpectedByte(resp)),
        }
    }
}

/// Set scancode set (1/2/3).
pub fn set_scancode_set<I: Ps2Io>(io: &mut I, set: ScancodeSet) -> Result<(), Ps2Error> {
    // Step 1: Send command 0xF0
    send_with_ack(io, CMD_SCANCODE_SET, 5)?;
    // Step 2: Send subcommand 1/2/3
    let sub = match set {
        ScancodeSet::Set1 | ScancodeSet::None => SUB_SET1,
        ScancodeSet::Set2 => SUB_SET2,
        ScancodeSet::Set3 => SUB_SET3,
    };
    send_with_ack(io, sub, 5)?;
    Ok(())
}

/// Get current scancode set; handles both raw (1/2/3) and translated (0x43/0x41/0x3F).
pub fn get_scancode_set<I: Ps2Io>(io: &mut I) -> Result<ScancodeSet, Ps2Error> {
    // Step 1: Send command 0xF0
    send_with_ack(io, CMD_SCANCODE_SET, 5)?;
    // Step 2: Send subcommand 0 (GET)
    send_with_ack(io, SUB_GET, 5)?;
    // Step 3: Read identifier byte (after ACK)
    let id = io.read_data()?;
    match map_scancode_id(id) {
        Some(set) => Ok(set),
        None => Err(Ps2Error::InvalidScancodeId(id)),
    }
}

use pc_keyboard::KeyCode;

/// Represents a Set 1 scancode sequence.
/// Most keys are one byte, but extended keys use an E0 prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Set1Code {
    Single(u8),
    Extended(u8), // prefixed with 0xE0
}

pub fn keycode_to_set1(code: KeyCode) -> Option<Set1Code> {
    match code {
        // Number row
        KeyCode::Escape      => Some(Set1Code::Single(0x01)),
        KeyCode::Key1        => Some(Set1Code::Single(0x02)),
        KeyCode::Key2        => Some(Set1Code::Single(0x03)),
        KeyCode::Key3        => Some(Set1Code::Single(0x04)),
        KeyCode::Key4        => Some(Set1Code::Single(0x05)),
        KeyCode::Key5        => Some(Set1Code::Single(0x06)),
        KeyCode::Key6        => Some(Set1Code::Single(0x07)),
        KeyCode::Key7        => Some(Set1Code::Single(0x08)),
        KeyCode::Key8        => Some(Set1Code::Single(0x09)),
        KeyCode::Key9        => Some(Set1Code::Single(0x0A)),
        KeyCode::Key0        => Some(Set1Code::Single(0x0B)),
        KeyCode::OemMinus    => Some(Set1Code::Single(0x0C)),
        KeyCode::OemPlus     => Some(Set1Code::Single(0x0D)),
        KeyCode::Backspace   => Some(Set1Code::Single(0x0E)),

        // Top row
        KeyCode::Tab         => Some(Set1Code::Single(0x0F)),
        KeyCode::Q           => Some(Set1Code::Single(0x10)),
        KeyCode::W           => Some(Set1Code::Single(0x11)),
        KeyCode::E           => Some(Set1Code::Single(0x12)),
        KeyCode::R           => Some(Set1Code::Single(0x13)),
        KeyCode::T           => Some(Set1Code::Single(0x14)),
        KeyCode::Y           => Some(Set1Code::Single(0x15)),
        KeyCode::U           => Some(Set1Code::Single(0x16)),
        KeyCode::I           => Some(Set1Code::Single(0x17)),
        KeyCode::O           => Some(Set1Code::Single(0x18)),
        KeyCode::P           => Some(Set1Code::Single(0x19)),
        KeyCode::Oem4        => Some(Set1Code::Single(0x1A)),
        KeyCode::Oem6        => Some(Set1Code::Single(0x1B)),
        KeyCode::Return      => Some(Set1Code::Single(0x1C)),

        // Home row
        KeyCode::CapsLock    => Some(Set1Code::Single(0x1D)),
        KeyCode::A           => Some(Set1Code::Single(0x1E)),
        KeyCode::S           => Some(Set1Code::Single(0x1F)),
        KeyCode::D           => Some(Set1Code::Single(0x20)),
        KeyCode::F           => Some(Set1Code::Single(0x21)),
        KeyCode::G           => Some(Set1Code::Single(0x22)),
        KeyCode::H           => Some(Set1Code::Single(0x23)),
        KeyCode::J           => Some(Set1Code::Single(0x24)),
        KeyCode::K           => Some(Set1Code::Single(0x25)),
        KeyCode::L           => Some(Set1Code::Single(0x26)),
        KeyCode::Oem1        => Some(Set1Code::Single(0x27)),
        KeyCode::Oem3        => Some(Set1Code::Single(0x28)),
        KeyCode::Oem8       => Some(Set1Code::Single(0x29)),

        // Bottom row
        KeyCode::LShift      => Some(Set1Code::Single(0x2A)),
        KeyCode::Oem5        => Some(Set1Code::Single(0x2B)),
        KeyCode::Z           => Some(Set1Code::Single(0x2C)),
        KeyCode::X           => Some(Set1Code::Single(0x2D)),
        KeyCode::C           => Some(Set1Code::Single(0x2E)),
        KeyCode::V           => Some(Set1Code::Single(0x2F)),
        KeyCode::B           => Some(Set1Code::Single(0x30)),
        KeyCode::N           => Some(Set1Code::Single(0x31)),
        KeyCode::M           => Some(Set1Code::Single(0x32)),
        KeyCode::OemComma    => Some(Set1Code::Single(0x33)),
        KeyCode::OemPeriod   => Some(Set1Code::Single(0x34)),
        KeyCode::Oem2        => Some(Set1Code::Single(0x35)),
        KeyCode::RShift      => Some(Set1Code::Single(0x36)),

        // Control keys
        KeyCode::NumpadMultiply => Some(Set1Code::Single(0x37)),
        KeyCode::LAlt        => Some(Set1Code::Single(0x38)),
        KeyCode::Spacebar    => Some(Set1Code::Single(0x39)),
        KeyCode::CapsLock    => Some(Set1Code::Single(0x3A)),

        // Function keys
        KeyCode::F1          => Some(Set1Code::Single(0x3B)),
        KeyCode::F2          => Some(Set1Code::Single(0x3C)),
        KeyCode::F3          => Some(Set1Code::Single(0x3D)),
        KeyCode::F4          => Some(Set1Code::Single(0x3E)),
        KeyCode::F5          => Some(Set1Code::Single(0x3F)),
        KeyCode::F6          => Some(Set1Code::Single(0x40)),
        KeyCode::F7          => Some(Set1Code::Single(0x41)),
        KeyCode::F8          => Some(Set1Code::Single(0x42)),
        KeyCode::F9          => Some(Set1Code::Single(0x43)),
        KeyCode::F10         => Some(Set1Code::Single(0x44)),
        KeyCode::F11         => Some(Set1Code::Single(0x57)),
        KeyCode::F12         => Some(Set1Code::Single(0x58)),

        // Lock keys
        KeyCode::NumpadLock     => Some(Set1Code::Single(0x45)),
        KeyCode::ScrollLock  => Some(Set1Code::Single(0x46)),

        // Keypad
        KeyCode::Numpad7     => Some(Set1Code::Single(0x47)),
        KeyCode::Numpad8     => Some(Set1Code::Single(0x48)),
        KeyCode::Numpad9     => Some(Set1Code::Single(0x49)),
        KeyCode::NumpadSubtract => Some(Set1Code::Single(0x4A)),
        KeyCode::Numpad4     => Some(Set1Code::Single(0x4B)),
        KeyCode::Numpad5     => Some(Set1Code::Single(0x4C)),
        KeyCode::Numpad6     => Some(Set1Code::Single(0x4D)),
        KeyCode::NumpadAdd  => Some(Set1Code::Single(0x4E)),
        KeyCode::Numpad1     => Some(Set1Code::Single(0x4F)),
        KeyCode::Numpad2     => Some(Set1Code::Single(0x50)),
        KeyCode::Numpad3     => Some(Set1Code::Single(0x51)),
        KeyCode::Numpad0     => Some(Set1Code::Single(0x52)),
        KeyCode::NumpadPeriod   => Some(Set1Code::Single(0x53)),

        // Extended keys (E0 prefix in Set 1)
        KeyCode::ArrowUp     => Some(Set1Code::Extended(0x48)),
        KeyCode::ArrowDown   => Some(Set1Code::Extended(0x50)),
        KeyCode::ArrowLeft   => Some(Set1Code::Extended(0x4B)),
        KeyCode::ArrowRight  => Some(Set1Code::Extended(0x4D)),
        KeyCode::Insert      => Some(Set1Code::Extended(0x52)),
        KeyCode::Delete      => Some(Set1Code::Extended(0x53)),
        KeyCode::Home        => Some(Set1Code::Extended(0x47)),
        KeyCode::End         => Some(Set1Code::Extended(0x4F)),
        KeyCode::PageUp      => Some(Set1Code::Extended(0x49)),
        KeyCode::PageDown    => Some(Set1Code::Extended(0x51)),
        KeyCode::RControl    => Some(Set1Code::Extended(0x1D)),
        KeyCode::RAlt2       => Some(Set1Code::Extended(0x38)),
        KeyCode::ArrowUp     => Some(Set1Code::Extended(0x48)),
        KeyCode::ArrowDown   => Some(Set1Code::Extended(0x50)),
        KeyCode::ArrowLeft   => Some(Set1Code::Extended(0x4B)),
        KeyCode::ArrowRight  => Some(Set1Code::Extended(0x4D)),
        KeyCode::Insert      => Some(Set1Code::Extended(0x52)),
        KeyCode::Delete      => Some(Set1Code::Extended(0x53)),
        KeyCode::Home        => Some(Set1Code::Extended(0x47)),
        KeyCode::End         => Some(Set1Code::Extended(0x4F)),
        KeyCode::PageUp      => Some(Set1Code::Extended(0x49)),
        KeyCode::PageDown    => Some(Set1Code::Extended(0x51)),

        // Extra keypad keys (E0 variants in Set 1)
        KeyCode::NumpadEnter  => Some(Set1Code::Extended(0x1C)),
        KeyCode::NumpadDivide => Some(Set1Code::Extended(0x35)),

        // Windows / Menu keys (on modern 104-key layouts)
        KeyCode::Apps => Some(Set1Code::Extended(0x5B)),
        KeyCode::RWin => Some(Set1Code::Extended(0x5C)),
        // KeyCode::Menu        => Some(Set1Code::Extended(0x5D)),

        // Pause/Break is special: in Set 1 it sends a multi-byte sequence (E1 1D 45 E1 9D C5)
        KeyCode::PauseBreak  => None, // handle separately if needed

        // Print Screen is also special: E0 2A E0 37 (make), E0 B7 E0 AA (break)
        KeyCode::PrintScreen => None, // handle separately

        // F13–F24 (if your KeyCode enum has them) are not in classic Set 1
        // You can extend with vendor-specific codes if needed.

        _ => None,

    }
}
