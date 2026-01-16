use alloc::{
    boxed::Box,
    collections::{LinkedList, VecDeque},
    rc::Rc,
    string::String,
    vec,
    vec::Vec,
};

use crate::test::TestInfo;

/// Tests allocation Tools.
pub fn test_alloc_tools(inf: TestInfo) {
    // use variable data for better testing.
    let boxed = Box::new(inf.ord);
    assert_eq!(inf.ord, *boxed);

    // Reason: We are testing allocation here.
    #[allow(clippy::useless_vec)]
    let num_vec = vec![0usize, 1, 2, 3];
    for (i, num) in num_vec.iter().enumerate() {
        assert_eq!(i, *num);
    }

    let reference_counted = Rc::new(vec![1, 2, 3]);
    let cloned_reference = reference_counted.clone();
    assert_eq!(Rc::strong_count(&cloned_reference), 2);

    drop(reference_counted);

    assert_eq!(Rc::strong_count(&cloned_reference), 1);

    let string = String::from("Hello, World!");
    assert_eq!(string.as_str(), "Hello, World!");

    let mut buf = VecDeque::new();
    buf.push_back(1);
    buf.push_back(3);
    assert_eq!(3, *buf.back().unwrap());
    let mut list = LinkedList::new();
    list.push_back(0usize);
    list.push_back(1);
    list.push_back(2);
    list.push_back(3);
    for (i, it) in list.iter().enumerate() {
        assert_eq!(i, *it);
    }
}
/// Test large allocations.
pub fn test_large_alloc(_: TestInfo) {
    let n = 1000;
    let mut vec = Vec::new();
    for i in 0..n {
        vec.push(i);
    }
    assert_eq!(vec.iter().sum::<u64>(), (n - 1) * n / 2)
}

/// Tests memory re-usability
pub fn test_freed_mem_used(_: TestInfo) {
    // The way this test works is that if the asertion fails, it means the heap is not being reused
    // when there is free memory.

    for i in 0..10000 {
        let x = Box::new(i);
        assert_eq!(*x, i);
    }
}
