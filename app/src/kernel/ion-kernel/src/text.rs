//! Tools for handling On Screen Text
//! 
//! # Safety
//! In general, the functions of this module are always safe to call, as if you print to VGA while video memory is being used in graphics mode,
//! it works perfectly fine, with the only problem being that it is covered by the video rendering.
//! 
//! In-fact, "background logs", as we call them, are encouraged! As when we crash, logs appear for a few moments, which may help in some capacity.
//! 
//! Note that we have not implement Video Rendering yet.
//! # Features
//! This module supports VGA printing with a foreground and background [`Color`]
//! 
//! The following operations are allowed for colors:
//! - [`set_print_color(fore, back)`](set_print_color): Set the current print color
//! - [`query_print_color()`]: Get the current print color
//! - [`reset_print_color()`]: Resets the Print Color to White On Black.
//! 
//! Additionally, this module exports [`WRITER`] (static), which lives at 0xb8000 (the VGA buffer), and allows for I/O Operations on the VGA
//! buffer. [`WRITER`] implements [`Write`](core::fmt::Write), which allows you to write to the VGA buffer.
//! 
//! However, the most common use case for this module, is [`print`] and [`println`], the printing macros.
//! 
//! # Example
//! Set color and print
//! ```
//! use crate::text::{set_print_color, reset_print_color, println, Color};
//! 
//! set_print_color(Color::LightCyan, Color::Black);
//! println!("Highlighted Text: {}", "Important!");
//! 
//! // ensure the print color is safe for logs
//! 
//! reset_print_color();
//! ```
//! Query the print color and print it.
//! ```
//! use crate::text::{query_print_color, WRITER};
//! use x86_64::instructions::interrupts;
//! 
//! let color = query_print_color().tupled();
//! 
//! // prevent deadlocks
//! interrupts::without_interrupts(|| {
//!     let lock = WRITER.lock();
//!     write!(lock, "The colors are: {} on {}", color.0, color.1);
//!     
//!     // explicit drop.
//!     drop(lock)
//! })
//! ```

/// A Color Used for the VGA Buffer.
/// 
/// Used in tandem with [`set_print_color`] to color the VGA output.
/// # Example
/// ```
/// use crate::test::{set_print_color, Color};
/// 
/// set_print_color(Color::White, Color::Black);
/// ```
#[allow(dead_code)]
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Color {
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Magenta = 5,
    Brown = 6,
    LightGray = 7,
    DarkGray = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    Pink = 13,
    Yellow = 14,
    White = 15,
}

/// Colors (Foreground/Background) Used in the VGA Buffer
/// 
/// Internal API, not to be created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct ColorCode(u8);

impl ColorCode {
    /// Creates a new [`ColorCode`] using the passed in colors.
    pub fn new(foreground: Color, background: Color) -> ColorCode {
        ColorCode((background as u8) << 4 | (foreground as u8))
    }

    /// Returned As (fore, back)
    pub fn tupled(self) -> (Color, Color) {
        let combined_value = self.0;
        let original_background = (combined_value & 0xF0) >> 4;
        let original_foreground = combined_value & 0x0F;

        let fore = unsafe { mem::transmute::<u8, Color>(original_foreground) };
        let back = unsafe { mem::transmute::<u8, Color>(original_background) };

        (fore, back)
    }
}

// Actual VGA impl

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
struct ScreenChar {
    ascii_character: u8,
    color_code: ColorCode,
}

const BUFFER_HEIGHT: usize = 25;
const BUFFER_WIDTH: usize = 80;

use volatile::Volatile;

#[repr(transparent)]
#[derive(Debug)]
struct Buffer {
    chars: [[Volatile<ScreenChar>; BUFFER_WIDTH]; BUFFER_HEIGHT],
}

/// Writer used to add text to the VGA Buffer
/// 
/// Internal API, only use in [`WRITER`]
#[derive(Debug)]
pub struct Writer {
    column_position: usize,
    color_code: ColorCode,
    buffer: &'static mut Buffer,
}

impl Writer {
    /// write a byte to the VGA Buffer
    /// 
    /// use [`write_char`] to write a char instead
    pub fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => self.new_line(),
            byte => {
                if self.column_position >= BUFFER_WIDTH {
                    self.new_line();
                }

                let row = BUFFER_HEIGHT - 1;
                let col = self.column_position;

                let color_code = self.color_code;
                self.buffer.chars[row][col].write(ScreenChar {
                    ascii_character: byte,
                    color_code,
                });
                self.column_position += 1;
            }
        }
    }

    /// Removes the most recent character.
    pub fn backspace(&mut self) {
        self.column_position = self.column_position.saturating_sub(1);
        
        let row = BUFFER_HEIGHT - 1;
        let col = self.column_position;

        // deletion
        self.buffer.chars[row][col].write(ScreenChar {
            ascii_character: b' ',
            color_code: self.color_code,
        });
    }

    /// deletes the current row.
    pub fn delete_row(&mut self) {
        self.clear_row(BUFFER_HEIGHT - 1);
        self.column_position = 0;
    }

    /// Writes a character to the [`Writer`]
    pub fn write_char(&mut self, character: char) {
        self.write_byte(character as u8);
    }

    /// Writes a string using a for loop.
    pub fn write_string(&mut self, s: &str) {
        for char in s.chars() {
            match char {
                // printable ASCII byte or newline
                ' '..='~' | '\n' => self.write_char(char),
                // not part of printable ASCII range
                _ => self.write_byte(0xfe),
            }

        }
    }

    fn new_line(&mut self) {
        for row in 1..BUFFER_HEIGHT {
            for col in 0..BUFFER_WIDTH {
                let character = self.buffer.chars[row][col].read();
                self.buffer.chars[row - 1][col].write(character);
            }
        }
        self.clear_row(BUFFER_HEIGHT - 1);
        self.column_position = 0;
    }

    fn clear_row(&mut self, row: usize) {
        let blank = ScreenChar {
            ascii_character: b' ',
            color_code: self.color_code,
        };
        for col in 0..BUFFER_WIDTH {
            self.buffer.chars[row][col].write(blank);
        }
    }
}

use core::{fmt, mem};

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string(s);
        Ok(())
    }
}

// Global Writer

use lazy_static::lazy_static;
use spin::Mutex;

#[cfg(feature = "test")]
use crate::test::{TestInfo, TestResult};

lazy_static! {
    /// The Global Writer
    /// 
    /// Typically, you wouldn't use this, instead, you would use the [`println`] or [`print`] macros.
    /// 
    /// But if those don't suffice, you can use this instead.
    /// # Example
    /// minimal example with no deadlock guard
    /// ```
    /// let lock = WRITER.lock();
    /// write!(f, "Hello, ").unwrap();
    /// lock.write_str("World!").unwrap();
    /// ```
    pub static ref WRITER: Mutex<Writer> = Mutex::new(Writer {
        column_position: 0,
        color_code: ColorCode::new(Color::White, Color::Black),
        buffer: unsafe { &mut *(0xb8000 as *mut Buffer) },
    });
}

/// Prints the passed in text, without a newline at the end
pub macro print($($arg:tt)*) {
    $crate::text::_print(format_args!($($arg)*))
}

/// Prints the passed in text, with a newline at the end
/// 
/// you can call this without arguments to simply add a newline to the VGA buffer
pub macro println {
    () => ($crate::text::print!("\n")),
    ($($arg:tt)*) => ($crate::text::print!("{}\n", format_args!($($arg)*)))
}

cfg_if::cfg_if! {
    if #[cfg(debug_assertions)] {
        /// prints only in debug assertions
        pub macro debug_print($($arg:tt)*) {
            $crate::text::_print(format_args!($($arg)*))
        }

        /// prints only in debug assertions
        pub macro debug_println {
            () => ($crate::text::print!("\n")),
            ($($arg:tt)*) => ($crate::text::print!("{}\n", format_args!($($arg)*)))
        }
    } else {
        /// prints only in debug assertions
        pub macro debug_print($($arg:tt)*) {
            // do nothing
        }
        
        /// prints only in debug assertions
        pub macro debug_println($($arg:tt)*) {
            // do nothing
        }
    }
}

/// sets the global print color
pub fn set_print_color(fore: Color, back: Color) {
    WRITER.lock().color_code = ColorCode::new(fore, back);
    // lock is dropped here, WRITER is released for future use.
}

/// resets the global print color to White on Black
pub fn reset_print_color() {
    set_print_color(Color::White, Color::Black);
}

/// Gets the global print color
#[allow(unused)]
pub fn query_print_color() -> ColorCode {
    WRITER.lock().color_code
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    use x86_64::instructions::interrupts;

    // Even though `write_fmt` always returns `Ok(())`, we are better off ignoring the value instead of
    // panicking.
    //
    // this also must run without interrupts, as some of our interrupt handlers may print to the VGA
    // buffer, which could cause a deadlock if we are already printing. see 
    // https://os.phil-opp.com/hardware-interrupts/#provoking-a-deadlock
    let _ = interrupts::without_interrupts(|| {
        WRITER.lock().write_fmt(args)
    });
}

// test


#[cfg(feature = "test")]
/// Tests println output is valid
pub fn test_println_output(_: TestInfo) -> TestResult {
    use crate::test::{TestResult};
    use core::fmt::Write;
    use x86_64::instructions::interrupts;

    let s = "Some test string that fits on a single line";
    interrupts::without_interrupts(|| {
        let mut writer = WRITER.lock();
        writeln!(writer, "\n{}", s).expect("writeln failed");
        for (i, c) in s.chars().enumerate() {
            let screen_char = writer.buffer.chars[BUFFER_HEIGHT - 2][i].read();
            assert_eq!(char::from(screen_char.ascii_character), c);
        }
    });
    TestResult::Ok
}