//! This module is not for tests, but instead the test frame-work
//!
//! It includes the test runner, and other related items.
//!
//! # Writing your Tests
//! See CONTRIBUTING.md for better documentation
//! # Example
//! ```
//! use crate::test::{TestInfo, TestResult, test_assert_eq};
//! use core::any::type_id_of_val;
//!
//! fn test_info(inf: TestInfo) -> TestResult {
//!     test_assert_eq!(inf.ord, 0, "Make Me First!")?; // implements `Try`
//!     test_assert_eq!(inf.type_id, type_id_of_val(&test_info))?;
//!     TestResult::Ok
//! }
//! ```
#![cfg_attr(not(feature = "test"), allow(dead_code))]
use core::any::{Any, TypeId, type_name};

use alloc::{format, vec};

use crate::{
    log::error,
    serial_print, serial_println,
    thread::{self, JoinErrorKind, ThreadBuilder},
};

/// Info Passed to Tests
///
/// contains the index at which a test is executed, and the type ID.
///
/// this is subject to change in the future, plan accordingly!
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TestInfo<'a> {
    /// The index at which the test is ran
    pub ord: usize,
    /// TypeID of the Test.
    ///
    /// Usually a function's
    pub type_id: TypeId,
    options: &'a mut TestOptions,
}

impl TestInfo<'_> {
    /// tells the test framework this test should fail (panic)
    pub fn should_panic(&mut self) {
        self.options.should_panic = true;
    }

    /// tells the test framework this test should NOT panic (default)
    pub fn should_not_panic(&mut self) {
        self.options.should_panic = false;
    }

    /// ignores this test
    ///
    /// exits immediately
    pub fn ignore(&mut self) -> ! {
        self.options.ignored = true;

        thread::exit_current();
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct TestOptions {
    should_panic: bool,
    ignored: bool,
}

/// A Testable Object
///
/// This allows for any type to be a test.
///
/// by default, implemented for `T: Fn(TestInfo) -> TestResult`
pub trait Testable: Any + Send + Sync {
    /// Name of the test.
    fn name(&self) -> &str;
    /// Runs the test.
    fn run(&self, info: TestInfo);
}

impl<T: Fn(TestInfo) + Any + Send + Sync> Testable for T {
    fn name(&self) -> &str {
        type_name::<Self>()
    }

    fn run(&self, info: TestInfo) {
        self(info);
    }
}

/// Runs tests
///
/// do not call - this function is called automatically in lib.rs
///
/// however, you may be able to find alternative uses elsewhere
pub fn run_tests(tests: &'static [&(dyn Testable + 'static)]) -> ! {
    serial_println!("Now Running {} Tests.", tests.len());
    let mut fail_names = vec![];
    let mut pass_count = 0;
    let mut ignore_count = 0;
    for (i, test) in tests.iter().enumerate() {
        let mut options = TestOptions {
            ignored: false,
            should_panic: false,
        };

        let name = test.name();
        serial_print!("[{}] {name}: ", i + 1);

        let options_ptr = unsafe { (&raw mut options).as_mut() }.unwrap();

        let thread = ThreadBuilder::new()
            .with_name(format!("test <{name}>"))
            .with_entry(move || {
                test.run(TestInfo {
                    ord: i,
                    type_id: test.type_id(),
                    options: options_ptr,
                })
            })
            .with_stack(64 * 1024)
            .spawn();

        match thread.join() {
            Ok(()) => {
                if options.ignored {
                    ignore_count += 1;
                    continue;
                }

                if options.should_panic {
                    serial_println!("[FAILED (no panic)]");
                    fail_names.push(name);
                    continue;
                }
                serial_println!("[OK]");
                pass_count += 1;
            }
            Err(e) => match e.kind() {
                JoinErrorKind::Panicked => {
                    if options.ignored {
                        ignore_count += 1;
                        continue;
                    }
                    if options.should_panic {
                        serial_println!("[OK]");
                        pass_count += 1;
                        continue;
                    }
                    serial_println!("[FAILED]");
                    fail_names.push(name);
                }
                JoinErrorKind::Unfinished => {
                    unreachable!("`join` should return `Unset`, not `Unfinished`")
                }
                JoinErrorKind::Unset => {
                    if !options.ignored {
                        error!("Thread exited prematurely")
                    }
                    serial_println!("[IGNORED]");
                    ignore_count += 1;
                }
            },
        }
    }
    serial_print!("Ran Tests: ");
    if !fail_names.is_empty() {
        serial_println!("[FAILED]");
    } else {
        serial_println!("[OK]");
    }
    serial_println!("=> {} Passed", pass_count);
    serial_println!("=> {} Ignored", ignore_count);
    serial_println!("=> {} Failed", fail_names.len());
    for x in &fail_names {
        serial_println!("===> {x} failed");
    }
    if !fail_names.is_empty() {
        exit(QemuExitCode::Failed)
    } else {
        exit(QemuExitCode::Passed)
    }
}

// QEMU exiting.

/// Represents a Qemu Exit Code
///
/// This is used when ending tests, which is why prints must be serial.
///
/// # Example
/// in run_tests...
/// ```rust,no_run
/// use crate::test::{QemuExitCode, exit};
///
/// exit(QemuExitCode::Passed);
/// ```
#[derive(Debug, Clone, Copy)]
pub enum QemuExitCode {
    /// Tests Passed
    Passed = 0x10,
    /// Tests Failed
    Failed = 0x11,
}

/// Exits QEMU using the code
///
/// see [`QemuExitCode`] for more info
pub fn exit(code: QemuExitCode) -> ! {
    exit_raw(code);

    panic!(
        "Attempted to write `{code:?}` ({:#x}) to port `0xf4`, but it did not exit.",
        code as u32
    );
}

/// exits without a panic.
pub fn exit_raw(code: QemuExitCode) {
    use x86_64::instructions::port::Port;

    unsafe {
        let mut port = Port::new(0xf4);
        port.write(code as u32);
    }
}
