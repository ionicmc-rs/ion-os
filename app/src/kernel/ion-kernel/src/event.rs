use core::task::Waker;

use alloc::{boxed::Box, vec::Vec};
use pc_keyboard::KeyEvent;

use crate::thread::sync::Mutex;

/// keyboard hooks
pub static KEYBOARD_HOOKS: Mutex<Vec<Box<dyn Fn(KeyEvent) + Sync>>> = Mutex::new(Vec::new());

/// keyboard listeners that only run once
pub static KEYBOARD_ONCE_HOOKS: Mutex<Vec<Box<dyn FnOnce(KeyEvent) + Sync>>> =
    Mutex::new(Vec::new());
/// wakers that run on key input
pub static KEYBOARD_WAKERS: Mutex<Vec<Waker>> = Mutex::new(Vec::new());

/// returns the index of the hook
pub fn add_key_listener(f: impl Fn(KeyEvent) + Sync + 'static) -> usize {
    let mut lock = KEYBOARD_HOOKS.lock();
    lock.push(Box::new(f));
    lock.len() - 1
}

/// remove the nth index from the keyboard listeners
pub fn rem_keyboard_index(i: usize) -> impl Fn(KeyEvent) + Sync + 'static {
    let mut lock = KEYBOARD_HOOKS.lock();
    lock.remove(i)
}
/// add a keyboard listener that only runs once
pub fn add_key_listener_once(f: impl FnOnce(KeyEvent) + Sync + 'static) {
    KEYBOARD_ONCE_HOOKS.lock().push(Box::new(f));
}
/// take keyboard once hooks
pub fn take_kbd_once_hooks() -> Vec<Box<dyn FnOnce(KeyEvent) + Sync + 'static>> {
    let mut hooks = KEYBOARD_ONCE_HOOKS.lock();
    core::mem::take(&mut *hooks)
}

/// register a keyboard waker
pub fn reg_kbd_waker(waker: Waker) {
    KEYBOARD_WAKERS.lock().push(waker);
}
/// take all keyboard wakers
pub fn take_kbd_wakers() -> Vec<Waker> {
    let mut wakers = KEYBOARD_WAKERS.lock();
    core::mem::take(&mut *wakers)
}