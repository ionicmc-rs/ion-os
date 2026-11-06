use x86_64::{
    VirtAddr,
    registers::model_specific::{FsBase, GsBase},
};

/// Writes the kernel TLS pointer (ThreadSelf) to `GS.base`.
pub fn write_ptr(ptr: *mut u8) {
    GsBase::write(VirtAddr::from_ptr(ptr));
}

/// Reads the kernel TLS pointer (ThreadSelf) from `GS.base`.
pub fn read_ptr() -> *const u8 {
    GsBase::read().as_ptr::<u8>()
}

/// Writes the user TLS base (for FS-based TLS) into FS.base.
pub fn write_user_ptr(ptr: *mut u8) {
    FsBase::write(VirtAddr::from_ptr(ptr));
}
