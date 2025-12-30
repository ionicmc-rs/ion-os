use core::{fmt::Debug, marker::PhantomData, ptr::NonNull};

use crate::c_lib::bit_flags::BitFlags;

/// module containing tools for handling Bit Flags
pub mod bit_flags;
/// module for handling bits.
pub mod bit;

/// The Actual BootInfo used, in raw numbers
/// 
/// see [`BootInfo`] for Rust Types.
#[repr(C)]
#[derive(Clone, Debug)]
pub struct BootInfoInput {
    /// Multiboot magic value
    /// 
    /// equal to 0x36d76289
    pub multiboot_magic: u32,
    /// multiboot info
    pub multiboot_info: u32,
    /// The edx register after querying the `cpuid` command
    pub cpuid_edx: u32,
    /// The ecx register after querying the `cpuid` command
    pub cpuid_ecx: u32,
    /// address of the page table base
    pub page_table_base: u64,
    /// stack's top
    pub stack_top: u64,
    /// Frame Buffer Address, currently always set to 0 due to lack of implementation
    // TODO: impl
    pub framebuffer_addr: u64,
    /// Memory Map Address, currently always set to 0 due to lack of implementation.
    // TODO: impl
    pub memory_map_addr: u64,
    /// Address for C's kernel entry
    pub kernel_entry: u64,
}

impl BootInfoInput {
    /// converts the [`BootInfoInput`] into a rust type
    pub fn into_rust(self) -> BootInfo {
        use core::{ptr::{without_provenance_mut, without_provenance, null}, mem};
        BootInfo { 
            cpuid_ecx: BitFlags::new(self.cpuid_ecx),
            cpuid_edx: BitFlags::new(self.cpuid_edx),
            frame_buffer: null(),
            // Safety: kernel entry is always set.
            kernel_entry: unsafe { mem::transmute::<usize, unsafe extern "C" fn(BootInfoInput) -> !>(self.kernel_entry as usize) },
            mem_map_addr: null(),
            // Safety: We cast a u32 to a usize, which means the address is always valid
            multiboot_info: unsafe { SmallPtr::new_unchecked(without_provenance(self.multiboot_info as usize)) },
            multiboot_magic: self.multiboot_magic,
            page_table_base: NonNull::new(without_provenance_mut(self.page_table_base as usize)).unwrap(),
            stack_top: NonNull::new(without_provenance_mut(self.stack_top as usize)).unwrap()
        }
    }
}

/// A Pointer from the 32 bit stage
/// 
/// used for multiboot info.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct SmallPtr<T: ?Sized> {
    ptr: u32,
    phantom: PhantomData<T>
}

impl<T: ?Sized> Debug for SmallPtr<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:#x}", self.ptr)
    }
}

impl<T> SmallPtr<T> {
    /// # Safety
    /// the pointer must be to a 32 bit address
    pub unsafe fn new_unchecked(ptr: *const T) -> Self {
        Self { ptr: ptr as usize as u32, phantom: PhantomData }
    }

    /// Convert the SmallPtr to its inner value.
    pub fn into_inner(self) -> *const T {
        self.ptr as *const T
    }
}

#[derive(Debug, Clone)]
/// Boot info, with proper Rust types.
pub struct BootInfo {
    /// Multiboot magic value
    /// 
    /// equal to 0x36d76289
    pub multiboot_magic: u32,
    /// multiboot info, stored as a 32bit pointer.
    pub multiboot_info: SmallPtr<()>, // this should be a ptr, but it is 32 bits, and we must keep the
                                      // the memory layout the same as BootInfoInput
    /// The edx register after querying the `cpuid` command
    /// 
    /// this is stored as bit flags.
    pub cpuid_edx: BitFlags,
    /// The ecx register after querying the `cpuid` command
    /// 
    /// this is stored as bit flags
    pub cpuid_ecx: BitFlags,
    /// pointer to page table base
    pub page_table_base: NonNull<()>,
    /// pointer to stack top
    pub stack_top: NonNull<()>,
    /// pointer to frame buffer, always null for now.
    /// 
    /// WARNING: We will change this to a `NonNull` eventually, so plan accordingly.
    pub frame_buffer: *const (),
    /// pointer memory map, always null for now.
    /// 
    /// WARNING: We will change this to a `NonNull` eventually, so plan accordingly.
    pub mem_map_addr: *const (),
    /// C kernel entry, as a function pointer
    /// 
    /// note: one of the unsafe preconditions to call this function is that nothing is initialized yet, however,
    /// by calling this function at any point after the init call in [`rust_kernel_entry`](crate::rust_kernel_entry), 
    /// we have violated this.
    /// 
    /// TLDR: do not call this.
    pub kernel_entry: unsafe extern "C" fn(BootInfoInput) -> !
}

impl BootInfo {
    /// Exits rust, and calls the C Kernel Entry function.
    /// 
    /// # Safety
    /// This must only be done before anything is initialized, violating this contract is
    /// ***undefined behavior***
    pub unsafe fn call_kernel_entry(self) -> ! {
        // Safety: The caller guarantees safety
        unsafe { (self.kernel_entry)(core::mem::transmute::<Self, BootInfoInput>(self)) }
    }
}


/// C BootInfo, passed in to the main function.
#[repr(C)]
#[derive(Debug)]
pub struct BootInfoC {
    input_ptr: *const BootInfoInput,
    valid: bool
}

impl BootInfoC {
    /// Returns the inner value
    /// # Errors
    /// This Function returns the value either way, but if it is in the [`Err`] variant, it means the
    /// [`BootInfo`] was not valid.
    pub fn into_inner(self) -> Result<BootInfoInput, BootInfoInput> {
        let f = if self.valid {
            Ok
        } else {
            Err
        };
        if self.input_ptr.is_null() {
            panic!("MEMORY ERROR: BOOT INFO POINTER IS INVALID  ({:?}) (aligned: {})", self.input_ptr, self.input_ptr.is_aligned());
        }

        f(unsafe { self.input_ptr.read_unaligned() })
    }
}