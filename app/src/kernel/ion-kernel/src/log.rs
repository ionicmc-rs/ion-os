//! Tools for logging to VGA buffer.
//! 
//! These are meant for users, but we plan to add a `serial_log`
use core::fmt;

use crate::text::{Color, print, println, query_print_color, set_print_color};

/// Log levels
/// 
/// Ideally, you would use the logging macros [`info`], [`trace`], [`debug`], [`error`], and [`warn`]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    /// Trace log
    Trace,
    /// Debug log, does not show in release
    Debug,
    /// Info log
    Info,
    /// Warning log
    Warn,
    /// Error Log
    Error,
}

/// Low‑level logging function: forwards to println
#[inline]
#[track_caller]
pub fn log(level: Level, args: fmt::Arguments) {
    if !cfg!(debug_assertions) && level == Level::Debug {
        return;
    }
    let loc = core::panic::Location::caller();
    let (fore, back) = query_print_color().tupled();
    print!("[");
    let col = match level {
        Level::Debug => Color::Green,
        Level::Error => Color::LightRed,
        Level::Trace => Color::Magenta,
        Level::Info => Color::LightCyan,
        Level::Warn => Color::Yellow
    };
    set_print_color(col, Color::Black);

    print!("{level:?}");

    set_print_color(fore, back);
    println!(" {}] {}", loc, args);
}

/// Info log
pub macro info($($args:tt)*) {
    $crate::log::log($crate::log::Level::Info, format_args!($($args)*))
}

/// Warn log
pub macro warn($($args:tt)*) {
    $crate::log::log($crate::log::Level::Warn, format_args!($($args)*))
}

/// Trace log
pub macro trace($($args:tt)*) {
    $crate::log::log($crate::log::Level::Trace, format_args!($($args)*))
}

/// Error log
pub macro error($($args:tt)*) {
    $crate::log::log($crate::log::Level::Error, format_args!($($args)*))
}

/// Debug log, will not show in release.
pub macro debug($($args:tt)*) {
    $crate::log::log($crate::log::Level::Debug, format_args!($($args)*))
}