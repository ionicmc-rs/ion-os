#![allow(unused)]
use core::{cell::OnceCell, ops::{Deref, DerefMut}};

use x86_64::{instructions::port::{Port, PortGeneric, ReadWriteAccess}, structures::idt::InterruptStackFrame};

use crate::{interrupts::{keyboard::ps2::{DefaultIO, set_scancode_set}, pic8259::handlers::notify}, serial_println, text::{WRITER, print}};

use pc_keyboard::{DecodedKey, HandleControl, KeyCode, Keyboard, ScancodeSet, ScancodeSet1, ScancodeSet2, layouts::{self, Us104Key}};
use spin::{Mutex, MutexGuard};

lazy_static::lazy_static! {
    static ref KEYBOARD: Mutex<Keyboard<layouts::Us104Key, ps2::ScancodeSet>> = {
        // let mut data = Port::new(0x60);
        // let mut write = Port::new(0x64);
    
        Mutex::new(Keyboard::new(ps2::ScancodeSet::None, Us104Key, HandleControl::Ignore))
    };
}

struct Once {
    init: *const ps2::ScancodeSet
}

impl Once {
    pub const fn new(init: &ps2::ScancodeSet) -> Self {
        Self { init: init as *const ps2::ScancodeSet }
    }

    pub const fn new_ptr(init: *const ps2::ScancodeSet) -> Self {
        Self { init }
    }

    pub fn set(&self, set: ps2::ScancodeSet) {
        let ptr = self.init as *mut ps2::ScancodeSet;
        unsafe {
            *ptr = set
        }
    }

    pub fn query(&self) -> ps2::ScancodeSet {
        unsafe { *self.init }
    }
}

unsafe impl Sync for Once {}

static mut SCAN_CODE_SET_IS_SET: ps2::ScancodeSet = ps2::ScancodeSet::None;
static SCAN_CODE_SET_QUERIED: Once = Once::new_ptr(unsafe { &raw const SCAN_CODE_SET_IS_SET });

/// Handler Keyboard Input
pub extern "x86-interrupt" fn keyboard_interrupt_handler(
    _stack_frame: InterruptStackFrame)
{
    // Note: the current implementation is simply here as a placeholder until we implement multi-tasking,
    // which is soon.


    use x86_64::instructions::{port::Port, interrupts};
    
    interrupts::without_interrupts(|| {
        let mut port = Port::new(0x60);
    
        let scancode: u8 = unsafe { port.read() };
    
        let mut keyboard = KEYBOARD.lock();
            // To impl
            // if SCAN_CODE_SET_QUERIED.query() == ps2::ScancodeSet::None {
            //     // let mut data = Port::new(0x60);
            //     // let mut write = Port::new(0x64);
            //     // if let Some(set) = query_scan_code(&mut data, &mut write) {
            //     //     *keyboard = Keyboard::new(set, Us104Key, HandleControl::Ignore);
            //     //     SCAN_CODE_SET_QUERIED.toggle();
            //     // }

            //     set_scancode_set(&mut DefaultIO, ps2::ScancodeSet::Set1);

            //     *keyboard = Keyboard::new(ps2::ScancodeSet::Set1, Us104Key, HandleControl::Ignore);
            //     SCAN_CODE_SET_QUERIED.set(ps2::ScancodeSet::Set1);
            // }
            if let Ok(Some(key_event)) = keyboard.add_byte(scancode) {
                if let Some(key) = keyboard.process_keyevent(key_event) {
                    match key {
                        DecodedKey::Unicode(character) => { 
                            if character as u8 == 8 {
                                x86_64::instructions::interrupts::without_interrupts(|| {
                                    let mut lock = WRITER.lock();
                                    lock.backspace();
                                    drop(lock);
                                })
                            } else if character as u8 == 9 {
                                use core::fmt::Write;
                                x86_64::instructions::interrupts::without_interrupts(|| {
                                    let mut lock = WRITER.lock();
                                    write!(lock, "    ");
                                    drop(lock);
                                })
                            } else if character as u8 == 46 {
                                x86_64::instructions::interrupts::without_interrupts(|| {
                                    let mut lock = WRITER.lock();
                                    lock.delete_row();
                                    drop(lock);
                                })
                            } else {
                                print!("{}", character);
                                serial_println!("{}", character as u8);
                            }
                        },
                        DecodedKey::RawKey(key) => {
                            if key == pc_keyboard::KeyCode::Backspace {
                                x86_64::instructions::interrupts::without_interrupts(|| {
                                    let mut lock = WRITER.lock();
                                    lock.backspace();
                                    drop(lock);
                                })
                            } else if key == KeyCode::Delete {
                                x86_64::instructions::interrupts::without_interrupts(|| {
                                    let mut lock = WRITER.lock();
                                    lock.delete_row();
                                    drop(lock);
                                })
                            } else {
                                print!("{:?}", key)
                            }
                        },
                    }
                }
            }
        notify!(unsafe Keyboard);
    })
}

mod ps2;