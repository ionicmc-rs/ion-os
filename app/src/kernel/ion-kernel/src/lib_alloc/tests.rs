use alloc::{boxed::Box, collections::{LinkedList, VecDeque}, rc::Rc, string::String, vec, vec::Vec};

use crate::test::{TestInfo, TestResult, test_assert_eq};

/// Tests allocation Tools.
pub fn test_alloc_tools(inf: TestInfo) -> TestResult {
    // use variable data for better testing.
    let boxed = Box::new(inf.ord);
    test_assert_eq!(inf.ord, *boxed)?;

    // Reason: We are testing allocation here.
    #[allow(clippy::useless_vec)]
    let num_vec = vec![0usize, 1, 2, 3];
    for (i, num) in num_vec.iter().enumerate() {
        test_assert_eq!(i, *num)?;
    }

    let reference_counted = Rc::new(vec![1, 2, 3]);
    let cloned_reference = reference_counted.clone();
    test_assert_eq!(Rc::strong_count(&cloned_reference), 2)?;

    drop(reference_counted);

    test_assert_eq!(Rc::strong_count(&cloned_reference), 1)?;

    let string = String::from("Hello, World!");
    test_assert_eq!(string.as_str(), "Hello, World!")?;

    let mut buf = VecDeque::new();
    buf.push_back(1);
    buf.push_back(3);
    test_assert_eq!(3, *buf.back().unwrap())?;

    let mut list = LinkedList::new();
    list.push_back(0usize);
    list.push_back(1);
    list.push_back(2);
    list.push_back(3);
    for (i, it) in list.iter().enumerate() {
        test_assert_eq!(i, *it)?;
    }

    TestResult::Ok
}

/// Test large allocations.
pub fn test_large_alloc(_: TestInfo) -> TestResult {
    let n = 1000;
    let mut vec = Vec::new();
    for i in 0..n {
        vec.push(i);
    }
    test_assert_eq!(vec.iter().sum::<u64>(), (n - 1) * n / 2)
}

/// Tests memory re-usability
pub fn test_freed_mem_used(_: TestInfo) -> TestResult {
    // The way this test works is that if the assertion fails, it means the heap is not being reused
    // when there is free memory.

    for i in 0..super::HEAP_SIZE {
        let x = Box::new(i);
        test_assert_eq!(*x, i)?;
    }

    TestResult::Ok
}