//! This module contains tools for linking with C code.
//! 
//! currently, we only link with main.c, but other than that, thats the only file.
//! 
//! but this is subject to change in the future, so we built this module.
use core::{ffi::CStr, fmt::Debug, marker::PhantomData, ptr::NonNull};

use crate::{c_lib::bit_flags::BitFlags, serial_println};

pub mod bit_flags;
pub mod bit;
pub mod libc;

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
        use core::{ptr::{without_provenance_mut, without_provenance}, mem};
        BootInfo { 
            cpuid_ecx: BitFlags::new(self.cpuid_ecx),
            cpuid_edx: BitFlags::new(self.cpuid_edx),
            // Safety: kernel entry is always set.
            kernel_entry: unsafe { mem::transmute::<usize, unsafe extern "C" fn(BootInfoInput) -> !>(self.kernel_entry as usize) },
            // Safety: We cast a u32 to a usize, which means the address is always valid
            multiboot_info: { 
                let ptr: SmallPtr<MultibootTag> = unsafe { SmallPtr::new_unchecked(without_provenance(self.multiboot_info as usize)) };
                let inner = unsafe { ptr.into_inner().as_ref().unwrap() };
                serial_println!("{:#?}", inner);
                ptr
            } ,
            multiboot_magic: {
                if self.multiboot_magic == 0x36d76289 {
                    MultibootMagic::Multiboot2
                } else {
                    // always called with valid boot info,
                    // so we can have cleaner `else` case
                    MultibootMagic::Multiboot1
                }
            },
            page_table_base: NonNull::new(without_provenance_mut(self.page_table_base as usize)).unwrap(),
            stack_top: NonNull::new(without_provenance_mut(self.stack_top as usize)).unwrap(),
            frame_buffer: NonNull::new(without_provenance_mut(self.framebuffer_addr as usize)).unwrap(),
            mem_map_addr: {
                let data_ptr = self.memory_map_addr as *const MultibootMemoryIntermediate;
                let header = unsafe {
                    data_ptr.as_ref().unwrap()
                };
                serial_println!("Header: {:#?}", header);
                let entries_len = {
                    (header.size.strict_sub(size_of::<MultibootMemoryIntermediate>() as u32))
                        / header.entry_size
                } as usize;
                let full_ptr: *const MultibootMemory =
                core::ptr::slice_from_raw_parts(data_ptr as *const MemoryMapEntry, entries_len)
                as *const MultibootMemory;  
                NonNull::new(full_ptr as *mut MultibootMemory).unwrap()
            },
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

/// The Multiboot Magic value
#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum MultibootMagic {
    /// Value for Multiboot1
    Multiboot1 = 0x2badb002,
    #[default]
    /// Value for Multiboot2
    Multiboot2 = 0x36d76289
}

impl Debug for MultibootMagic {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:#x}", *self as u32)
    }
}

#[derive(Debug, Clone)]
/// Boot info, with proper Rust types.
pub struct BootInfo {
    /// Multiboot magic value
    /// 
    /// equal to 0x36d76289 on multiboot2
    pub multiboot_magic: MultibootMagic,

    /// multiboot info, stored as a 32bit pointer.
    // this should be a ptr, but it is 32 bits, and we 
    // must keep the the memory layout the same as 
    // BootInfoInput
    pub multiboot_info: SmallPtr<MultibootTag>, 
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
    /// pointer to frame buffer.
    pub frame_buffer: NonNull<()>,
    /// pointer to memory map.
    pub mem_map_addr: NonNull<MultibootMemory>,
    /// C kernel entry, as a function pointer
    /// 
    /// note: one of the unsafe preconditions to call this function is that nothing is initialized yet, however,
    /// by calling this function at any point after the init call in [`rust_kernel_entry`](crate::rust_kernel_entry), 
    /// we have violated this.
    /// 
    /// TLDR: do not call this.
    pub kernel_entry: unsafe extern "C" fn(BootInfoInput) -> !
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

        // Safety: We check the pointer is non-null, and we ensure in C this pointer is never invalid.
        f(unsafe { self.input_ptr.read_unaligned() })
    }
}

/// The Physical Memory offset for pages.
/// 
/// currently always 0.
pub const PHYSICAL_MEMORY_OFFSET: usize = 0;

/// Multiboot Tag
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MultibootTag {
    /// Tag Type
    pub typ: u32,   // tag type
    /// Total Size of the tag.
    pub size: u32,  // total size of this tag (including header)
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Kind of MemoryMapEntry
pub enum EntryKind {
    /// This entry is Usable
    Usable,
    /// This is entry is Reserved
    Reserved,
    /// Entry can be reclaimed by APIC
    APICReclaimable,
    /// This entry is for NotVolatile (Optimizable) Storage.
    NonVolatileStorage,
    /// This entry was ruined by bad hardware, or other errors.
    BadMemory,
    // others are to be added in the future...
}

/// MMap Entry.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MemoryMapEntry {
    /// Address.
    pub addr: u64,
    /// Length
    pub len: u64,
    /// Entry Kind
    pub entry_type: u32,
    /// Reserved Memory.
    pub reserved: u32,
}

impl MemoryMapEntry {
    /// Starting address for MemoryMapEntry
    pub fn start_addr(&self) -> usize {
        self.addr as usize
    }

    /// Size of entry
    pub const fn size(&self) -> usize {
        self.len as usize
    }

    /// Ending address
    /// 
    /// simple `self.start_addr` + `self.size()`
    pub fn end_addr(&self) -> usize {
        self.start_addr() + self.size()
    }
}

#[allow(missing_docs)]
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Type for a Multiboot Tag
pub enum MultibootTagType {
    End                = 0,
    CommandLine        = 1,
    BootLoaderName     = 2,
    Module             = 3,
    BasicMemInfo       = 4,
    BootDevice         = 5,
    MemoryMap          = 6,
    VbeInfo            = 7,
    FramebufferInfo    = 8,
    ElfSections        = 9,
    ApmTable           = 10,
    Efi32SystemTable   = 11,
    Efi64SystemTable   = 12,
    SmbiosTables       = 13,
    AcpiOldRsdp        = 14,
    AcpiNewRsdp        = 15,
    NetworkInfo        = 16,
    EfiMemoryMap       = 17,
    EfiBsNotTerminated = 18,
    Efi32ImageHandle   = 19,
    Efi64ImageHandle   = 20,
    LoadBaseAddr       = 21,
    // /// Catch‑all for unknown or future values
    // Unfortunately, we must keep this enum Transmute Safe.
    // Tuple Variants fail to transmute.
    // Unknown(u32),
}

/// Intermediate stage to convert a memory address to NonNull<[`MultibootMemory`]>
#[repr(C)]
#[derive(Debug)]
pub struct MultibootMemoryIntermediate {
    /// Type (6)
    pub typ: MultibootTagType,
    /// Size
    pub size: u32,
    /// Size of one entry
    pub entry_size: u32,
    /// Version (0 until Multiboot updates it)
    pub entry_version: u32,
    // no entries here
}

/// Multiboot Memory Data.
#[repr(C)]
#[derive(Debug)]
pub struct MultibootMemory {
    /// Type (6)
    pub typ: MultibootTagType, // = 6 (MemoryMap)
    /// Size
    pub size: u32,         // total size of this tag
    /// Entry Size
    pub entry_size: u32,   // size of each entry
    /// Entry Version (0)
    pub entry_version: u32,
    /// All [`MemoryMapEntry`]s
    pub entries: [MemoryMapEntry]
}

/// FB Type
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum FBType {
    /// Indexed
    Indexed,
    /// RGB (Red-Green-Blue)
    Rgb,
    /// Text Only.
    Text
}

/// Frame Buffer Tag
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MultibootFramebufferTag {
    /// Type (8)
    pub typ: MultibootTagType, // = 8
    /// Size of Tag
    pub size: u32,
    /// Physical Address to Frame Buffer.
    pub addr: NonNull<()>,      // physical address of framebuffer
    /// Bytes per scanline
    pub pitch: u32,     // bytes per scanline
    /// Screen Width
    pub width: u32,
    /// Screen Height
    pub height: u32,
    /// BitsPerPixel
    pub bpp: u8,        // bits per pixel
    /// FB Type
    /// 
    /// see [`FBType`]
    pub fb_type: FBType,    // 0 = indexed, 1 = RGB, 2 = text
    /// Reserved Memory.
    pub reserved: u16,
}

/// Module Tag
#[repr(C)]
#[derive(Debug)]
pub struct Multiboot2ModuleTag {
    /// Type (3)
    pub typ: MultibootTagType,       // = 3
    /// Size
    pub size: u32,
    /// Start of module
    pub mod_start: u32,
    /// end of Module
    pub mod_end: u32,
    /// 0 Terminated C Str.
    pub zstr: CStr
}

// MemoryMap entry types

/// Number for a Usable Entry
pub const USABLE_ENTRY: u32 = 1;
/// Number for a APIC Reclaimable Entry.
pub const ACPI_RECLAIMABLE: u32 = 3;
/// Number for a Non Volatile Storage Entry.
pub const NON_VOLATILE_STORAGE: u32 = 4;
/// Number for a Bad Memory Storage Entry.
pub const BAD_MEM: u32 = 5;