//! This module is not for tests, but instead the test frame-work
//! 
//! It includes the test runner, and other related items.
#![cfg_attr(not(feature = "test"), allow(dead_code))]
use core::{any::{Any, TypeId, type_name}, convert::Infallible, ops::{FromResidual, Try}};

use crate::{hlt_loop, serial_print, serial_println};

/// Info Passed to Tests
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TestInfo {
    /// The index at which the test is ran
    pub ord: usize,
    /// TypeID of the Test.
    /// 
    /// Usually a function's
    pub type_id: TypeId
}

/// A Testable Object
/// 
/// This allows for any type to be a test.
pub trait Testable: Any {
    /// This should print the test name using the `print` macro.
    fn run(&self, info: TestInfo) -> TestResult;
}

impl<T: Fn(TestInfo) -> TestResult + Any> Testable for T {
    fn run(&self, info: TestInfo) -> TestResult {
        serial_print!("{}: ", type_name::<T>());
        self(info)
    }
}

/// The result of a test
/// 
/// A test can: 
/// - pass (Ok)
/// - fail: (Failure(/* err */))
/// - be ignored (Ignored)
#[derive(Debug, Clone)]
pub enum TestResult {
    /// The Test Has Passed
    Ok,
    /// The Test has Failed
    /// 
    /// The inner value is a description of why
    Failure(&'static str),
    /// This test was ignored, for whatever reason.
    Ignored,
}

impl TestResult {
    /// Returns a fail
    /// 
    /// useful for map functions
    pub fn fail(err: &'static str) -> Self {
        Self::Failure(err)
    }

    /// asserts the first argument, failing with `err` if it is false
    pub fn assertion(assert: bool, err: &'static str) -> Self {
        if assert {
            Self::Ok
        } else {
            Self::Failure(err)
        }
    }
}

impl FromResidual<&'static str> for TestResult {
    fn from_residual(residual: &'static str) -> Self {
        Self::Failure(residual)
    }
}

impl FromResidual<Result<Infallible, &'static str>> for TestResult {
    fn from_residual(residual: Result<Infallible, &'static str>) -> Self {
        let Err(e) = residual;
        Self::Failure(e)
    }
}

impl Try for TestResult {
    type Output = ();
    type Residual = &'static str;
    fn branch(self) -> core::ops::ControlFlow<Self::Residual, Self::Output> {
        match self {
            Self::Failure(e) => core::ops::ControlFlow::Break(e),
            _ => core::ops::ControlFlow::Continue(())
        }
    }

    fn from_output(_: Self::Output) -> Self {
        Self::Ok
    }
}

/// Runs tests
/// 
/// do not call - this function is called automatically in lib.rs
/// 
/// however, you may be able to find alternative uses elsewhere
pub fn run_tests(tests: &'static [&(dyn Testable + 'static)]) -> ! {
    // TODO: Use Serial Prints, and Exit QEMU, as this is planned in CONTRIBUTING.md

    serial_println!("Now Running {} Tests.", tests.len());
    let mut fail_count = 0;
    let mut pass_count = 0;
    let mut ignore_count = 0;
    for (i, test) in tests.iter().enumerate() {
        serial_print!("[{}] ", i + 1); // run should print test name
        match test.run(TestInfo {
            ord: i,
            type_id: test.type_id()
        }) {
            TestResult::Ok => { 
                serial_println!("[OK]");
                pass_count += 1;
            },
            TestResult::Failure(e) => {
                serial_println!("[FAIL]");
                serial_println!(" => {}", e);
                fail_count += 1;
            },
            TestResult::Ignored => { 
                serial_println!("[IGNORED]");
                ignore_count += 1;
            }
        }
    }
    serial_print!("Ran Tests: ");
    if fail_count > 0 {
        serial_println!("[FAILED]");
    } else {
        serial_println!("[OK]");
    }
    serial_println!("=> {} Passed", pass_count);
    serial_println!("=> {} Failed", fail_count);
    serial_println!("=> {} Ignored", ignore_count);
    if fail_count > 0 {
        exit(QemuExitCode::Failed)
    } else {
        exit(QemuExitCode::Passed)
    }
}

/// Asserts the passed in value, with an optional, Statically set message
pub macro test_assert {
    ($test:expr_2021 $(,)?) => {{
        $crate::test::TestResult::assertion($test, concat!("Assertion `", stringify!($test), "` Failed"))
    }},
    ($test:expr_2021, $msg:literal) => {{
        $crate::test::TestResult::assertion($test, concat!("Assertion `", stringify!($test), "` Failed: ", $msg))
    }}
}

/// Asserts the passed in values are equal, with an optional, Statically set message
pub macro test_assert_eq {
    ($a:expr_2021, $b:expr_2021 $(,)?) => {
        $crate::test::test_assert!($a == $b)
    },
    ($a:expr_2021, $b:expr_2021, $msg:literal) => {
        $crate::test::test_assert!($a == $b, $msg)
    }
}

/// Asserts the passed in values are not equal, with an optional, Statically set message
pub macro test_assert_ne {
    ($a:expr_2021, $b:expr_2021 $(,)?) => {
        $crate::test::test_assert!($a != $b)
    },
    ($a:expr_2021, $b:expr_2021, $msg:literal) => {
        $crate::test::test_assert!($a != $b, $msg)
    }
}


/// Asserts the passed in value matches the pattern, with an optional, Statically set message
pub macro test_assert_matches {
    ($a:expr_2021, $pat:pat $(,)?) => {
        $crate::test::test_assert!(matches!($a, $pat))
    },
    ($a:expr_2021, $b:expr_2021, $msg:literal) => {
        $crate::test::test_assert!(matches!($a, $pat), $msg)
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
/// # let fails = 0
/// use crate::test::{QemuExitCode, exit};
/// 
/// exit(QemuExitCode::Passed);
/// ```
#[derive(Debug)]
pub enum QemuExitCode {
    /// Tests Passed
    Passed = 0x10,
    /// Tests Failed
    Failed = 0x11
}

/// Exits QEMU using the code
/// 
/// see [`QemuExitCode`] for more info
pub fn exit(code: QemuExitCode) -> ! {
    use x86_64::instructions::port::Port;

    unsafe {
        let mut port = Port::new(0xf4);
        port.write(code as u32);
    }
    hlt_loop();
}