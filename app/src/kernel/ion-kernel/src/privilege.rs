//! User and privilege management for Ion OS.
//!
//! # Overview
//!
//! This module defines **user identities** and **privilege levels** within Ion OS.
//! A *user* represents an **authority / security principal**, not an execution
//! context and not an owner of resources.
//!
//! ## Core design principles
//!
//! - The **kernel owns all resources** (memory, threads, address spaces, devices).
//! - Users are given **references / handles**, never ownership.
//! - Privilege is a **policy decision**, enforced at kernel boundaries.
//! - Threads act *on behalf of* users.
//!
//! This module intentionally avoids:
//! - dynamic ownership models
//! - reference counting
//! - userspace-visible mutation
//!
//! The kernel remains the sole authority.

use alloc::vec::Vec;
use crate::thread::sync::Mutex;

/// Unique identifier for a user.
///
/// User IDs are kernel-assigned, stable, and never reused.
/// `UserId = 0` is always reserved for the `Os` user.
pub type UserId = u32;

/// Privilege levels within Ion OS.
///
/// This enum represents **authority**, not CPU rings or execution context.
/// It is used by the kernel to decide which operations are permitted.
///
/// Privilege does not imply ownership, memory access, or scheduling priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivilegeLevel {
    /// Operating System privilege.
    ///
    /// - Kernel-internal only
    /// - Full, unrestricted access
    /// - Never exposed to userspace
    Os,

    /// System administrator privilege.
    ///
    /// - Highest external authority
    /// - Comparable to Linux `root`
    /// - Still mediated by the kernel
    Atom,

    /// Regular user privilege.
    ///
    /// - Non-administrative userspace identity
    /// - Actual permissions are defined elsewhere
    User {
        // future: permission sets / capabilities
    },
}

/// Immutable user metadata.
///
/// This structure contains identity information only.
/// It deliberately avoids heap allocation and wide integer types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UserMetaData {
    /// User-visible name.
    ///
    /// Fixed-size to avoid heap allocation and ABI complexity.
    pub name: [u8; 16],

    /// Kernel-assigned user identifier.
    pub id: UserId,
}

/// Kernel representation of a user identity.
///
/// A `User` represents *who* an operation is performed on behalf of.
/// It does **not** own memory, threads, or address spaces.
#[derive(Debug)]
pub struct User {
    /// Immutable identity metadata.
    pub meta: UserMetaData,

    /// Privilege level associated with this user.
    pub privilege: PrivilegeLevel,
}

/// Kernel-owned registry of all loaded users.
///
/// This structure is global, authoritative, and never exposed directly.
/// Users are created by the kernel and referenced by `UserId`.
struct UserRegistry {
    users: Vec<User>,
}

/// Global user registry.
///
/// - Owned entirely by the kernel
/// - Protected by a lock
/// - Lives for the entire kernel lifetime
static USER_REGISTRY: Mutex<UserRegistry> = Mutex::new(UserRegistry {
    users: Vec::new(),
});

/// Initialize the user system.
///
/// This function must be called exactly once during early kernel
/// initialization.
///
/// It installs the mandatory `Os` user with `UserId = 0`.
pub fn init_users() {
    let mut reg = USER_REGISTRY.lock();

    // UserId 0 is always Os
    reg.users.push(User {
        meta: UserMetaData {
            name: *b"Os\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            id: 0,
        },
        privilege: PrivilegeLevel::Os,
    });
}

/// Create a new user.
///
/// This is a **kernel-only** operation.
///
/// Returns the assigned `UserId`.
pub fn create_user(
    name: [u8; 16],
    privilege: PrivilegeLevel,
) -> UserId {
    let mut reg = USER_REGISTRY.lock();

    let id = reg.users.len() as UserId;

    reg.users.push(User {
        meta: UserMetaData {
            name,
            id,
        },
        privilege,
    });

    id
}

/// Retrieve a user by ID.
///
/// Returns an immutable reference to a kernel-owned `User`.
///
/// # Safety model
///
/// - Users are never removed
/// - User storage is stable for the kernel lifetime
/// - Mutation is centralized inside the registry
pub fn get_user(id: UserId) -> Option<&'static User> {
    let reg = USER_REGISTRY.lock();
    let user = reg.users.get(id as usize)? as *const User;

    // SAFETY:
    // - registry lives forever
    // - users are never moved or removed
    unsafe { Some(&*user) }
}
