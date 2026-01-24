//! Types and structures for concurrency.
//!
//! The types in this module allow for safe **global**[^1] variables safely without data races
//! # Mutexes
//! a [`Mutex`] allows for obtaining a lock to a value, and using that lock to mutate and read that value.
//! ```
//! static mut GLOBAL_VAR: i32 = 10;
//!
//! let first = thread::spawn(|| {
//!     GLOBAL_VAR += 1;
//!
//!     // context switch can occur here!
//!
//!     assert_eq!(GLOBAL_VAR, 11)
//! });
//! let second = thread::spawn(|| {
//!     println!("{GLOBAL_VAR}"); // this value cannot be predicted now, and this causes unsafety.
//! });
//!
//! first.join();
//! second.join();
//! ```
//!
//! a [`Mutex`] prevents this.
//!
//! ```
//! // does not need `mut`
//! static GLOBAL_VAR: Mutex<i32> = Mutex::new(10);
//!
//! // ...
//!
//! let locked = GLOBAL_VAR.lock();
//! ```
//! this ensure the current thread is the only one to be able to access the variable.
//!
//! [^1]: using these structures on (thread) local variables has no effect.

use core::{
    cell::UnsafeCell, fmt::{Debug, Display}, ops::{Deref, DerefMut}, sync::atomic::{AtomicBool, Ordering}
};

use crate::{panic, thread};

pub struct Mutex<T: ?Sized> {
    locked: AtomicBool,
    poisoned: AtomicBool,
    value: UnsafeCell<T>,
}

impl<T: Debug> Debug for Mutex<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        struct DebugTag(&'static str);

        impl core::fmt::Debug for DebugTag {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.write_str(self.0)
            }
        }

        let mut ds = f.debug_struct(core::any::type_name::<Self>());

        match self.try_lock() {
            Ok(v) => {
                ds.field("data", &*v);
            }
            Err(TryLockError::Locked) => {
                ds.field("data", &DebugTag("{{ locked }}"));
            }
            Err(TryLockError::Poisoned) => {
                ds.field("data", &DebugTag("{{ poisoned }}"));
            }
        }

        ds.finish()
    }
}

#[derive(Debug, Clone)]
pub struct MutexGuard<'mutex, T: ?Sized> {
    lock: &'mutex AtomicBool,
    poisoned: &'mutex AtomicBool,
    cell: &'mutex UnsafeCell<T>,
}

impl<T: Display + ?Sized> Display for MutexGuard<'_, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.deref())
    }
}

impl<T: PartialEq + ?Sized> PartialEq<T> for MutexGuard<'_, T> {
    fn eq(&self, other: &T) -> bool {
        self.deref().eq(other)
    }

    fn ne(&self, other: &T) -> bool {
        self.deref().ne(other)
    }
}

impl<T: PartialEq + ?Sized> PartialEq for MutexGuard<'_, T> {
    fn eq(&self, other: &Self) -> bool {
        self.deref().eq(other)
    }

    fn ne(&self, other: &Self) -> bool {
        self.deref().ne(other)
    }
}

impl<T: Eq> Eq for MutexGuard<'_, T> {}

impl<T: PartialOrd> PartialOrd<T> for MutexGuard<'_, T> {
    fn partial_cmp(&self, other: &T) -> Option<core::cmp::Ordering> {
        self.deref().partial_cmp(other)
    }
}

impl<T: PartialOrd> PartialOrd for MutexGuard<'_, T> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.deref().partial_cmp(other)
    }
}

impl<T: Ord> Ord for MutexGuard<'_, T> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.deref().cmp(other)
    }
}

impl<'mutex, T: ?Sized> MutexGuard<'mutex, T> {
    fn new(
        lock: &'mutex AtomicBool,
        cell: &'mutex UnsafeCell<T>,
        poisoned: &'mutex AtomicBool,
    ) -> Self {
        Self {
            lock,
            cell,
            poisoned,
        }
    }

    pub fn add_hook_for(guard: Self)
    where
        'mutex: 'static, // 'mutex = 'static
    {
        panic::add_hook(|| {
            drop(guard);
        });
    }
}

impl<T: ?Sized> Deref for MutexGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { self.cell.get().as_ref().unwrap() }
    }
}

impl<T: ?Sized> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.cell.get().as_mut().unwrap() }
    }
}

impl<'mutex, T: ?Sized> Drop for MutexGuard<'mutex, T> {
    fn drop(&mut self) {
        if thread::panicking() {
            self.poisoned.store(true, Ordering::Relaxed);
        }
        self.lock.store(false, Ordering::Relaxed);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TryLockError {
    Poisoned,
    Locked,
}

impl Display for TryLockError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let fmt = match self {
            TryLockError::Locked => "held by another thread",
            TryLockError::Poisoned => "poisoned by another thread",
        };
        write!(f, "attempted to lock a mutex, but it was {fmt}.")
    }
}

impl core::error::Error for TryLockError {}

impl<T> Mutex<T> {
    pub const fn new(val: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            poisoned: AtomicBool::new(false),
            value: UnsafeCell::new(val),
        }
    }

    pub fn lock(&'static self) -> MutexGuard<'static, T> {
        if self.is_locked() {
            thread::atomic_block_until(&self.locked);
        }
        if self.poisoned.load(Ordering::Acquire) {
            // thread holding mutex panicked, we must panic too.
            panic!("attempted to lock a poisoned mutex");
        }

        self.locked.store(true, Ordering::Release);

        let lock = MutexGuard::new(&self.locked, &self.value, &self.poisoned);
        lock
    }

    pub fn try_lock(&'_ self) -> Result<MutexGuard<'_, T>, TryLockError> {
        if self.locked.swap(true, Ordering::Acquire) {
            return Err(TryLockError::Locked);
        }

        if self.poisoned.load(Ordering::Relaxed) {
            return Err(TryLockError::Poisoned);
        }

        Ok(MutexGuard::new(&self.locked, &self.value, &self.poisoned))
    }

    pub fn is_poisoned(&self) -> bool {
        self.poisoned.load(Ordering::Relaxed)
    }

    pub fn is_locked(&self) -> bool {
        self.locked.load(Ordering::Relaxed)
    }
}

unsafe impl<T: ?Sized> Sync for Mutex<T> {}
unsafe impl<T: ?Sized> Send for Mutex<T> {}
