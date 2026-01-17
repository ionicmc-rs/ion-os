use x86_64::{VirtAddr, registers::model_specific::FsBase};

/// write the tls ptr
pub fn write_ptr(ptr: *mut u8) {
    FsBase::write(VirtAddr::from_ptr(ptr));
}

/// reads the tls ptr
pub fn read_ptr() -> *const u8 {
    FsBase::read().as_ptr::<u8>()
}