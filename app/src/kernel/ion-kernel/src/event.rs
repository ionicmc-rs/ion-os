use alloc::{boxed::Box, vec::Vec};
use core::task::Waker;

use pc_keyboard::KeyEvent;
use spin::Mutex;

/// Keyboard hooks (run every key event)
static KEYBOARD_HOOKS: Mutex<Vec<Box<dyn Fn(KeyEvent) + Send + Sync>>> =
    Mutex::new(Vec::new());

/// Keyboard hooks that run once
static KEYBOARD_ONCE_HOOKS: Mutex<Vec<Box<dyn FnOnce(KeyEvent) + Send + Sync>>> =
    Mutex::new(Vec::new());

/// Keyboard wakers
static KEYBOARD_WAKERS: Mutex<Vec<Waker>> =
    Mutex::new(Vec::new());

/// Register a keyboard hook (persistent)
pub fn add_key_listener(f: impl Fn(KeyEvent) + Send + Sync + 'static) -> usize {
    let mut hooks = KEYBOARD_HOOKS.lock();
    hooks.push(Box::new(f));
    hooks.len() - 1
}

/// Remove the hook at index `i`
pub fn rem_keyboard_index(i: usize) -> Option<Box<dyn Fn(KeyEvent) + Send + Sync>> {
    let mut hooks = KEYBOARD_HOOKS.lock();
    if i < hooks.len() {
        Some(hooks.remove(i))
    } else {
        None
    }
}

/// Register a keyboard hook that runs once
pub fn add_key_listener_once(f: impl FnOnce(KeyEvent) + Send + Sync + 'static) {
    KEYBOARD_ONCE_HOOKS.lock().push(Box::new(f));
}

/// Run all persistent keyboard hooks
///
/// Safe to call from thread context.
/// No allocation, no blocking.
pub fn run_kbd_hooks(ev: KeyEvent) {
    let hooks = KEYBOARD_HOOKS.lock();
    for hook in hooks.iter() {
        hook(ev.clone());
    }
}

/// Run and consume all one-shot keyboard hooks
pub fn run_kbd_once_hooks(ev: KeyEvent) {
    let mut hooks = KEYBOARD_ONCE_HOOKS.lock();

    // Drain explicitly so ownership and drop are obvious
    while let Some(hook) = hooks.pop() {
        hook(ev.clone());
    }
}

/// Register a waker to be woken on next key event
pub fn reg_kbd_waker(waker: Waker) {
    KEYBOARD_WAKERS.lock().push(waker);
}

/// Wake and clear all keyboard wakers
pub fn wake_kbd_wakers() {
    let mut wakers = KEYBOARD_WAKERS.lock();

    // Wake first, then drop
    for waker in wakers.iter() {
        waker.wake_by_ref();
    }

    wakers.clear();
}
