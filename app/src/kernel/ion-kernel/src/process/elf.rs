//! Tools for loading ELFs and parsing Ion OS–specific requests.
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
}

/* ============================
 * Public entry point
 * ============================ */

pub struct LoadedElf {
    pub context: Context,
    pub stack_size: usize,
}

pub fn load_elf(elf: ElfBytes<AnyEndian>) -> Result<LoadedElf, ElfLoadError> {
    /* ----------------------------
     * Parse .ion_os.requests
     * ---------------------------- */

    let shdr = elf
        .section_header_by_name(".ion_os.requests")
        .map_err(ElfLoadError::ElfParseError)?
        .ok_or(ElfLoadError::MissingRequests)?;

    let (req_data, _) = elf
        .section_data(&shdr)
        .map_err(ElfLoadError::ElfParseError)?;

    let requests = parse_requests(req_data)
        .map_err(ElfLoadError::RequestParseError)?;

    /* ----------------------------
     * Create address space
     * ---------------------------- */

    let mut space = unsafe {
        address_space::create_address_space(
            2 * 1024 * 1024,
            2 * 1024 * 1024,
        )
    };

    unsafe { map_user_data(&mut space); };

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

    let total_size = max_vaddr - min_vaddr;

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
        let dst_vaddr = src_vaddr + delta;

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

    let entry = elf.ehdr.e_entry as usize + delta;

    log::info!("ELF loaded: entry={:#x}, load_base={:#x}, stack_base={:#x}",
        entry, load_base, stack_base);

    let ctx = Context::user(entry, /* tls */ 0, stack_top);

    Ok(LoadedElf {
        context: ctx,
        stack_size: requests.stack.size,
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