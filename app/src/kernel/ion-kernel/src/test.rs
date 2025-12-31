//! This module is not for tests, but instead the test frame-work
//! 
//! It includes the test runner, and other related items.
#![cfg_attr(not(feature = "test"), allow(dead_code))]
use core::any::{Any, TypeId, type_name};

use crate::text::{Color, print, println, reset_print_color, set_print_color};

/// Info Passed to Tests
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TestInfo {
    ord: usize,
    type_id: TypeId
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
        print!("{}: ", type_name::<T>());
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

/// Runs tests
/// 
/// do not call - this function is called automatically in lib.rs
/// 
/// however, you may be able to find alternative uses elsewhere
pub fn run_tests(tests: &'static [&(dyn Testable + 'static)]) {
    // TODO: Use Serial Prints, and Exit QEMU, as this is planned in CONTRIBUTING.md

    println!("Now Running {} Tests.", tests.len());
    let mut fail_count = 0;
    let mut pass_count = 0;
    let mut ignore_count = 0;
    for (i, test) in tests.iter().enumerate() {
        print!("[{}] ", i + 1); // run should print test name
        match test.run(TestInfo {
            ord: i,
            type_id: test.type_id()
        }) {
            TestResult::Ok => { 
                set_print_color(Color::LightGreen, Color::Black);
                println!("[OK]");
                reset_print_color();
                pass_count += 1;
            },
            TestResult::Failure(e) => {
                set_print_color(Color::LightRed, Color::Black);
                println!("[FAIL]");
                println!(" => {e}");
                reset_print_color();
                fail_count += 1;
            },
            TestResult::Ignored => { 
                set_print_color(Color::Yellow, Color::Black);
                println!("[IGNORED]");
                reset_print_color();
                ignore_count += 1;
            }
        }
    }
    println!("Ran Tests: ");
    set_print_color(Color::LightGreen, Color::Black);
    println!(" {pass_count} Passed");
    set_print_color(Color::LightRed, Color::Black);
    println!(" {fail_count} Failed");
    set_print_color(Color::Yellow, Color::Black);
    println!(" {ignore_count} Ignored");
    reset_print_color();
}

/// Asserts the passed in value, with an optional, Statically set message
pub macro test_assert {
    ($test:expr $(,)?) => {{
        $crate::test::TestResult::assertion($test, concat!("Assertion `", stringify!($test), "` Failed"))
    }},
    ($test:expr, $msg:literal) => {{
        $crate::test::TestResult::assertion($test, concat!("Assertion `", stringify!($test), "` Failed: ", $msg))
    }}
}

/// Asserts the passed in values are equal, with an optional, Statically set message
pub macro test_assert_eq {
    ($a:expr, $b:expr $(,)?) => {
        $crate::test::test_assert!($a == $b)
    },
    ($a:expr, $b:expr, $msg:literal) => {
        $crate::test::test_assert!($a == $b, $msg)
    }
}

/// Asserts the passed in values are not equal, with an optional, Statically set message
pub macro test_assert_ne {
    ($a:expr, $b:expr $(,)?) => {
        $crate::test::test_assert!($a != $b)
    },
    ($a:expr, $b:expr, $msg:literal) => {
        $crate::test::test_assert!($a != $b, $msg)
    }
}


/// Asserts the passed in value matches the pattern, with an optional, Statically set message
pub macro test_assert_matches {
    ($a:expr, $pat:pat $(,)?) => {
        $crate::test::test_assert!(matches!($a, $pat))
    },
    ($a:expr, $b:expr, $msg:literal) => {
        $crate::test::test_assert!(matches!($a, $pat), $msg)
    }
}