//! Tools for loading ELFs and parsing Ion OS-specific requests.
//!
//! Uses PT_LOAD segments and the userspace allocator.
//! All user memory lives above USER_BASE.

#![allow(dead_code)]

use core::alloc::Layout;
use core::mem::{size_of, MaybeUninit};

use alloc::alloc::Allocator;
use elf::endian::AnyEndian;
use elf::ElfBytes;

use crate::{address_space, log};
use crate::arch::{
    ctx::Context,
    PhysAddr,
    VirtAddr,
};
use crate::arch::mmu::{map_4k, PageTableFlags};
use crate::arch::frame_allocator::alloc_4k_frame;
use crate::lib_alloc::user::{init_user_data, map_user_data};

/* ============================
 * ELF loader errors
 * ============================ */

#[derive(Debug)]
pub enum ElfLoadError {
    ElfParseError(elf::parse::ParseError),
    RequestParseError(RequestParseError),
    MissingRequests,
    NoLoadableSegments,
    InvalidOffset,
}

/* ============================
 * Raw ELF header structures for manual parsing
 * ============================ */

#[repr(C)]
struct Elf64_Ehdr {
    e_ident: [u8; 16],
    e_type: u16,
    e_machine: u16,
    e_version: u32,
    e_entry: u64,
    e_phoff: u64,
    e_shoff: u64,
    e_flags: u32,
    e_ehsize: u16,
    e_phentsize: u16,
    e_phnum: u16,
    e_shentsize: u16,
    e_shnum: u16,
    e_shstrndx: u16,
}

#[repr(C)]
struct Elf64_Shdr {
    sh_name: u32,
    sh_type: u32,
    sh_flags: u64,
    sh_addr: u64,
    sh_offset: u64,
    sh_size: u64,
    sh_link: u32,
    sh_info: u32,
    sh_addralign: u64,
    sh_entsize: u64,
}

/* ============================
 * Public entry point
 * ============================ */

pub struct LoadedElf {
    pub context: Context,
    pub stack_size: usize,
    pub address_space: address_space::AddressSpace,
}

/// Find the .ion_os.requests section manually by iterating through section headers
/// 
/// # Safety
/// This function performs raw pointer operations and should only be called
/// with valid ELF data that has been verified to contain the required headers.
unsafe fn find_requests_section(elf_data: &[u8], ehdr: &Elf64_Ehdr) -> Result<*const u8, ElfLoadError> {
    let shstrndx = ehdr.e_shstrndx as usize;
    let shnum = ehdr.e_shnum as usize;
    
    if shnum == 0 || shstrndx == 0 {
        return Err(ElfLoadError::MissingRequests);
    }

    // Get the string table section header
    let shstrtab_offset = ehdr.e_shoff as usize + shstrndx * size_of::<Elf64_Shdr>();
    if shstrtab_offset + size_of::<Elf64_Shdr>() > elf_data.len() {
        return Err(ElfLoadError::InvalidOffset);
    }
    
    let shstrtab_shdr = unsafe {
        &*(elf_data.as_ptr().add(shstrtab_offset) as *const Elf64_Shdr)
    };
    
    // Get the string table data
    let strtab_offset = shstrtab_shdr.sh_offset as usize;
    let strtab_size = shstrtab_shdr.sh_size as usize;
    if strtab_offset + strtab_size > elf_data.len() {
        return Err(ElfLoadError::InvalidOffset);
    }
    
    let strtab_data = &elf_data[strtab_offset..strtab_offset + strtab_size];
    
    // Search for ".ion_os.requests" in the string table
    let target = b".ion_os.requests";
    
    // Find the position of the target string in the string table
    let mut target_pos: Option<usize> = None;
    for i in 0..strtab_data.len().saturating_sub(target.len() - 1) {
        if &strtab_data[i..i + target.len()] == target {
            target_pos = Some(i);
            break;
        }
    }
    
    let Some(name_offset) = target_pos else {
        return Err(ElfLoadError::MissingRequests);
    };
    
    // Now find which section has this name offset
    for sh_idx in 0..shnum {
        let offset = ehdr.e_shoff as usize + sh_idx * size_of::<Elf64_Shdr>();
        if offset + size_of::<Elf64_Shdr>() > elf_data.len() {
            continue;
        }
        
        let shdr = unsafe {
            &*(elf_data.as_ptr().add(offset) as *const Elf64_Shdr)
        };
        
        if shdr.sh_name as usize == name_offset {
            // Found the section - return its data pointer
            let req_offset = shdr.sh_offset as usize;
            let req_size = shdr.sh_size as usize;
            if req_offset + req_size > elf_data.len() {
                return Err(ElfLoadError::InvalidOffset);
            }
            log::debug!("found .ion_os.requests at offset {}, size {}", req_offset, req_size);
            return Ok(unsafe { elf_data.as_ptr().add(req_offset) });
        }
    }
    
    Err(ElfLoadError::MissingRequests)
}

pub fn load_elf(elf: ElfBytes<AnyEndian>, elf_data: &[u8]) -> Result<LoadedElf, ElfLoadError> {
    /* ----------------------------
     * Parse .ion_os.requests - manual lookup
     * ---------------------------- */

    log::debug!("parsing .ion_os.requests section");

    // Get the ELF header for manual parsing
    let ehdr = unsafe { &*(elf_data.as_ptr() as *const Elf64_Ehdr) };
    
    let req_data_ptr = unsafe { find_requests_section(elf_data, ehdr) }?;
    let req_data = unsafe { core::slice::from_raw_parts(req_data_ptr, 24) };

    let requests = parse_requests(req_data)
        .map_err(ElfLoadError::RequestParseError)?;

    log::debug!("parsed .ion_os.requests: stack size = {}, align = {}",
        requests.stack.size, requests.stack.align);

    /* ----------------------------
     * Create address space
     * ---------------------------- */

    let mut space = unsafe {
        address_space::create_address_space(
            2 * 1024 * 1024,
            2 * 1024 * 1024,
        )
    };
    

    log::debug!("created new address space for ELF");

    let user_alloc = space.allocator().unwrap();

    /* ----------------------------
     * Scan PT_LOAD segments
     * ---------------------------- */

    let phdrs = elf
        .segments()
        .ok_or(ElfLoadError::NoLoadableSegments)?;

    let mut min_vaddr = usize::MAX;
    let mut max_vaddr = 0;
    let mut found = false;

    for ph in phdrs.iter() {
        if ph.p_type != elf::abi::PT_LOAD {
            continue;
        }

        found = true;

        let start = ph.p_vaddr as usize;
        let end   = start + ph.p_memsz as usize;

        min_vaddr = min_vaddr.min(start);
        max_vaddr = max_vaddr.max(end);
    }

    if !found {
        return Err(ElfLoadError::NoLoadableSegments);
    }

    let total_size = align_up(max_vaddr, 0x1000) - align_down(min_vaddr, 0x1000);

    /* ----------------------------
     * Allocate VA range via userspace allocator
     * ---------------------------- */

    let load_base = unsafe {
        user_alloc
            .allocate(
                Layout::from_size_align(total_size, 0x1000).unwrap()
            )
    }.unwrap().as_ptr().addr();

    let delta = load_base.wrapping_sub(min_vaddr);

    /* ----------------------------
     * Map + populate PT_LOAD segments
     * ---------------------------- */

    for ph in phdrs.iter() {
        if ph.p_type != elf::abi::PT_LOAD {
            continue;
        }

        let src_vaddr = ph.p_vaddr as usize;
        let dst_vaddr = load_base + (src_vaddr - min_vaddr);

        let memsz  = ph.p_memsz  as usize;
        let filesz = ph.p_filesz as usize;

        let seg_data = elf
            .segment_data(&ph)
            .map_err(ElfLoadError::ElfParseError)?;

        // Copy file-backed bytes
        unsafe {
            core::ptr::copy_nonoverlapping(
                seg_data.as_ptr(),
                dst_vaddr as *mut u8,
                filesz,
            );

            // Zero BSS
            if memsz > filesz {
                core::ptr::write_bytes(
                    (dst_vaddr + filesz) as *mut u8,
                    0,
                    memsz - filesz,
                );
            }
        }
    }

    /* ----------------------------
     * Stack allocation
     * ---------------------------- */

    let stack_base = unsafe {
        user_alloc
            .allocate(
                Layout::from_size_align(
                    requests.stack.size,
                    requests.stack.align,
                ).unwrap()
            )
    }.unwrap().as_ptr().addr();

    let stack_top = stack_base + requests.stack.size;

    /* ----------------------------
     * Entry point
     * ---------------------------- */

    let entry = load_base + (elf.ehdr.e_entry as usize - min_vaddr);

    log::info!("ELF loaded: entry={:#x}, load_base={:#x}, stack_base={:#x}",
        entry, load_base, stack_base);

    let mut ctx = Context::user(entry, /* tls */ 0, stack_top);
    ctx.set_addr_space(space.clone());

    Ok(LoadedElf {
        context: ctx,
        stack_size: requests.stack.size,
        address_space: space,
    })
}

/* ============================
 * Request parsing
 * ============================ */

#[derive(Debug)]
pub enum RequestParseError {
    MagicInvalid,
    InvalidFormat,
    StackAlignInvalid,
    StackAlignTooSmall,
}

const REQ_MAGIC: u32 = u32::from_le_bytes(*b"IONR");
const REQ_VERSION: u16 = 1;

#[repr(C)]
#[derive(Copy, Clone)]
struct RequestsHeader {
    magic: u32,
    version: u16,
    size: u16,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct StackRequest {
    size: u64,
    align: u64,
}

struct Requests {
    stack: StackRequests,
}

struct StackRequests {
    size: usize,
    align: usize,
}

fn parse_requests(data: &[u8]) -> Result<Requests, RequestParseError> {
    if data.len() < size_of::<RequestsHeader>() + size_of::<StackRequest>() {
        return Err(RequestParseError::InvalidFormat);
    }

    let header = read::<RequestsHeader>(&data[..size_of::<RequestsHeader>()])?;

    if header.magic != REQ_MAGIC {
        return Err(RequestParseError::MagicInvalid);
    }

    if header.version != REQ_VERSION {
        return Err(RequestParseError::InvalidFormat);
    }

    if header.size as usize != data.len() {
        return Err(RequestParseError::InvalidFormat);
    }

    let offset = size_of::<RequestsHeader>();
    let stack = read::<StackRequest>(&data[offset..offset + size_of::<StackRequest>()])?;

    if stack.align < 16 {
        return Err(RequestParseError::StackAlignTooSmall);
    }

    if !stack.align.is_power_of_two() {
        return Err(RequestParseError::StackAlignInvalid);
    }

    Ok(Requests {
        stack: StackRequests {
            size: stack.size as usize,
            align: stack.align as usize,
        },
    })
}

fn read<T: Copy>(bytes: &[u8]) -> Result<T, RequestParseError> {
    if bytes.len() < size_of::<T>() {
        return Err(RequestParseError::InvalidFormat);
    }

    let mut value = MaybeUninit::<T>::uninit();
    unsafe {
        core::ptr::copy_nonoverlapping(
            bytes.as_ptr(),
            value.as_mut_ptr() as *mut u8,
            size_of::<T>(),
        );
        Ok(value.assume_init())
    }
}

/* ============================
 * Helpers
 * ============================ */

#[inline]
fn align_down(x: usize, a: usize) -> usize {
    x & !(a - 1)
}

#[inline]
fn align_up(x: usize, a: usize) -> usize {
    (x + a - 1) & !(a - 1)
}

