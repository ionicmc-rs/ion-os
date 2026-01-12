//! Tools for executing futures using thread blocking for efficiency

use core::sync::atomic::{AtomicBool, Ordering};
use core::task::{RawWaker, RawWakerVTable, Waker};
use core::{
    future::Future,
    task::{Context, Poll},
};

use alloc::boxed::Box;

use crate::thread::{self, JoinHandle};

/// Runs the given future, blocking when it is pending.
pub fn run_future<F, T>(future: F) -> T
where
    F: Future<Output = T> + 'static,
{
    let mut future = Box::pin(future);

    let awoken = AtomicBool::new(false);

    let waker = thread_waker(&awoken);
    let mut cx = Context::from_waker(&waker);

    loop {
        awoken.store(false, Ordering::SeqCst);
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(v) => break v,
            Poll::Pending => thread::block_until(awoken.as_ptr()),
        }
    }
}

/// Runs the future on a new thread.
pub fn thread<
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
>(future: F, stack: usize) -> JoinHandle<T> {
    thread::spawn(|| run_future(future), stack)
}

// =======================
// WAKER
// =======================

fn thread_waker(flag: &AtomicBool) -> Waker {
    unsafe { Waker::from_raw(raw(flag as *const AtomicBool)) }
}

/// IMPORTANT:
/// - no lifetimes
/// - no ownership
/// - just a raw pointer
struct TWaker {
    flag: *const AtomicBool,
}

fn raw(flag: *const AtomicBool) -> RawWaker {
    let waker = Box::new(TWaker { flag });
    RawWaker::new(
        Box::into_raw(waker).cast(),
        &RawWakerVTable::new(clone, wake, wake_by_ref, drop_waker),
    )
}

fn clone(data: *const ()) -> RawWaker {
    let waker = unsafe { &*(data.cast::<TWaker>()) };
    raw(waker.flag)
}

fn wake(data: *const ()) {
    let waker = unsafe { &*(data.cast::<TWaker>()) };
    unsafe { &*waker.flag }.store(true, Ordering::Release);
}

fn wake_by_ref(data: *const ()) {
    let waker = unsafe { &*(data.cast::<TWaker>()) };
    unsafe { &*waker.flag }.store(true, Ordering::Release);
}

fn drop_waker(data: *const ()) {
    unsafe {
        drop(Box::from_raw(data.cast::<TWaker>().cast_mut()))
    }
}
