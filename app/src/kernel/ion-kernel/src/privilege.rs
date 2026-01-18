//! Privilege levels within IonOS.
//! 
//! There are 2 "special" levels of privilege:
//! - Os: The Kernel
//!   - No restrictions
//!   - Not exposed to users, kernel internal
//! - Atom: System Admin
//!   - similar to linux's `root`
//!   - can do mostly anything
//! 
//! And then, there are users.
//! 
//! Users' privilege depends on settings set by them, realistically, a user can simply emulate `atom`'s permissions.

pub enum PrivilegeLevel {
    /// Operating System, internal to kernel
    Os,
    /// System Admin
    Atom,
    /// Just a regular user
    User {
        // permissions...
    }
}

