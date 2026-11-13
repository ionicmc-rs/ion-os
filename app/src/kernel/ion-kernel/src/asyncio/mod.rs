//! Tools for executing futures using thread blocking for efficiency

use core::marker::PhantomPinned;
use core::pin::Pin;
use core::sync::atomic::{AtomicBool, Ordering};
use core::task::{RawWaker, RawWakerVTable, Waker};
use core::{
    future::Future,
    task::{Context, Poll},
};

use alloc::boxed::Box;
use pc_keyboard::KeyEvent;

use crate::event::{self, reg_kbd_waker};
use crate::log;
use crate::thread::{self, JoinHandle};

#[track_caller]
/// Runs the given future, blocking when it is pending.
pub fn run_future<F, T>(future: F) -> T
where
    F: Future<Output = T>,
{
    let prev = thread::kill_safe();
    thread::set_kill_safe(false);
    let mut future = Box::pin(future);

    let awoken = AtomicBool::new(false);

    let waker = thread_waker(&awoken);
    let mut cx = Context::from_waker(&waker);

    loop {
        awoken.store(false, Ordering::SeqCst);
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(v) => {
                thread::set_kill_safe(prev);
                break v;
            }
            // if this thread is killed the pointer becomes invalid - so there should be some warning
            Poll::Pending => {
                if awoken.load(Ordering::SeqCst) {
                    continue;
                }
                thread::block_until(awoken.as_ptr());
            }
        }
    }
}

/// Runs the future on a new thread.
pub fn thread<F: Future<Output = T> + Send + 'static, T: Send + 'static>(
    future: F,
    stack: usize,
) -> JoinHandle<T> {
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
    drop_waker(data);
}

fn wake_by_ref(data: *const ()) {
    let waker = unsafe { &*(data.cast::<TWaker>()) };
    unsafe { &*waker.flag }.store(true, Ordering::Release);
}

fn drop_waker(data: *const ()) {
    unsafe { drop(Box::from_raw(data.cast::<TWaker>().cast_mut())) }
}

// helper types

/// a future which returns key input
///
/// see [`key_input`]
#[derive(Debug)]
pub struct KeyInput {
    state: Option<KeyEvent>,
    registered: bool,
    _pinned: PhantomPinned,
}

impl Future for KeyInput {
    type Output = KeyEvent;

    fn poll(self: core::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // SAFETY: `state` is not moved, so its address remains stable; only the
        // unpinned `registered` field is mutated directly.
        let this = unsafe { self.get_unchecked_mut() };

        if this.state.is_some() {
            Poll::Ready(
                this.state.as_ref().cloned().unwrap(), // this is ugly, but efficient
            )
        } else {
            if this.registered {
                return Poll::Pending;
            }

            // note: the following implementation requires `self` is not moved until completion.

            reg_kbd_waker(cx.waker().clone());
            this.registered = true;

            let ptr = &raw const this.state;
            let refr = unsafe { ptr.cast_mut().as_mut().unwrap() };
            event::add_key_listener_once(|ev| *refr = Some(ev));
            Poll::Pending
        }
    }
}

/// returns a future that returns key input.
///
/// due to its implementation - it cannot be moved, thus it is pinnned
pub fn key_input() -> Pin<Box<KeyInput>> {
    Box::pin(KeyInput {
        state: None,
        registered: false,
        _pinned: PhantomPinned,
    })
}
