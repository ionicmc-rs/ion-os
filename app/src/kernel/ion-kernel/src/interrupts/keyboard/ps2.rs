//! Minimal PS/2 keyboard scancode-set control (0xF0).
//! - Set: send 0xF0, then 1/2/3; expect ACK (0xFA) or RESEND (0xFE).
//! - Get: send 0xF0, then 0; expect ACK, then a value indicating set.
//! 
//! Handles both "raw" (1/2/3) and "translated" (0x43/0x41/0x3F) returns.
//! 
//! currently unused, but subject to change!
#![allow(unused)]

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


