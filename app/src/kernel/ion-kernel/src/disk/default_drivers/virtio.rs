#![allow(dead_code)]
#![allow(unsafe_op_in_unsafe_fn)]

use core::ptr::{self, read_volatile, write_volatile};
use core::sync::atomic::{compiler_fence, Ordering};

use alloc::boxed::Box;
use linked_list_allocator::{align_up, align_up_size};

use crate::arch::defs::PAGE_SIZE;
use crate::arch::frame_allocator::alloc_2mib_frame;
use crate::arch::mmu::{PageTable, PageTableFlags, map_2mib, virt_to_phys};
use crate::arch::{PhysAddr, VirtAddr, defs};
use crate::address_space::{kernel_address_space, map_2mib_into_space};
use crate::c_lib::libc::{self, OsError};
use crate::io::{Error, ErrorKind, Result};
use crate::lib_alloc::user::USER_DATA_BASE;

/* ============================ PCI ============================ */

fn pci_read_u8(bus: u8, slot: u8, func: u8, off: u8) -> u8 {
    let addr = 0x8000_0000u32
        | ((bus as u32) << 16)
        | ((slot as u32) << 11)
        | ((func as u32) << 8)
        | ((off as u32) & 0xFC);
    unsafe {
        crate::arch::port::outl(0xCF8, addr);
        ((crate::arch::port::inl(0xCFC) >> ((off & 3) * 8)) & 0xFF) as u8
    }
}

fn pci_read_u16(bus: u8, slot: u8, func: u8, off: u8) -> u16 {
    let lo = pci_read_u8(bus, slot, func, off) as u16;
    let hi = pci_read_u8(bus, slot, func, off + 1) as u16;
    lo | (hi << 8)
}

fn pci_read_u32(bus: u8, slot: u8, func: u8, off: u8) -> u32 {
    let addr = 0x8000_0000u32
        | ((bus as u32) << 16)
        | ((slot as u32) << 11)
        | ((func as u32) << 8)
        | ((off as u32) & 0xFC);
    unsafe {
        crate::arch::port::outl(0xCF8, addr);
        crate::arch::port::inl(0xCFC)
    }
}

fn pci_write_u16(bus: u8, slot: u8, func: u8, off: u8, val: u16) {
    let addr = 0x8000_0000u32
        | ((bus as u32) << 16)
        | ((slot as u32) << 11)
        | ((func as u32) << 8)
        | ((off as u32) & 0xFC);
    unsafe {
        crate::arch::port::outl(0xCF8, addr);
        let mut cur = crate::arch::port::inl(0xCFC);
        let shift = (off & 2) * 8;
        cur &= !(0xFFFF << shift);
        cur |= (val as u32) << shift;
        crate::arch::port::outl(0xCFC, cur);
    }
}


/* ========================== VirtIO =========================== */

const VIRTIO_VENDOR: u16 = 0x1AF4;
const VIRTIO_BLK_DEVICE: u16 = 0x1042;

const VIRTIO_PCI_CAP_COMMON: u8 = 1;
const VIRTIO_PCI_CAP_NOTIFY: u8 = 2;
const VIRTIO_PCI_CAP_ISR: u8    = 3;
const VIRTIO_PCI_CAP_DEVICE: u8 = 4;
const VIRTIO_PCI_CAP_PCI_CFG: u8 = 5;

const VIRTQ_DESC_F_NEXT: u16 = 1;
const VIRTQ_DESC_F_WRITE: u16 = 2;

const VIRTIO_BLK_T_IN: u32 = 0;
const VIRTIO_BLK_T_OUT: u32 = 1;

const VIRTIO_STATUS_ACKNOWLEDGE: u8 = 1;
const VIRTIO_STATUS_DRIVER: u8      = 2;
const VIRTIO_STATUS_DRIVER_OK: u8   = 4;
const VIRTIO_STATUS_FEATURES_OK: u8 = 8;
const VIRTIO_STATUS_FAILED: u8      = 128;

const VIRTIO_F_VERSION_1: u32 = 32;

#[repr(C)]
struct VirtioPciCommonCfg {
    device_feature_select: u32,
    device_feature: u32,
    driver_feature_select: u32,
    driver_feature: u32,
    _msix: u16,
    num_queues: u16,
    device_status: u8,
    config_generation: u8,
    queue_select: u16,
    queue_size: u16,
    _queue_msix: u16,
    queue_enable: u16,
    _queue_notify_off: u16,
    queue_desc: u64,
    queue_avail: u64,
    queue_used: u64,
}

#[repr(C)]
struct VirtioBlkReq {
    type_: u32,
    reserved: u32,
    sector: u64,
}

#[repr(C)]
struct VirtqDesc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

#[repr(C)]
struct VirtqAvail {
    flags: u16,
    idx: u16,

    // TEMPORARY(debug-hack): this should be changed to dynamically size based on
    // queue length
    ring: [u16; 128], 
}

#[repr(C)]
struct VirtqUsedElem {
    id: u32,
    len: u32,
}

#[repr(C)]
struct VirtqUsed {
    flags: u16,
    idx: u16,
    ring: [VirtqUsedElem; 128],
}

/* ======================= Driver Struct ======================= */

const MAX_CONCURRENT: usize = 4;

pub struct VirtioBlk {
    common: *mut VirtioPciCommonCfg,
    notify: *mut u8,
    notify_off_mul: u32,

    desc: *mut VirtqDesc,
    avail: *mut VirtqAvail,
    used: *mut VirtqUsed,

    last_used: u16,
    capacity: u64,

    // DMA Scratchpad
    dma_virt: *mut u8,
    dma_phys: u64,
    
    // NEW: Slot tracking
    // true = in use, false = free
    slot_in_use: [bool; MAX_CONCURRENT], 
}

/* ====================== MMIO Mapping ========================= */

use core::sync::atomic::{AtomicUsize};
static MMIO_BASE: AtomicUsize = AtomicUsize::new(0);
static MMIO_NEXT: AtomicUsize = AtomicUsize::new(0);

// Maps a 2MiB physical region to virtual memory. 
// Expects phys to be 2MiB aligned.

/// Maps a frame to the MMIO region
#[track_caller]
pub unsafe fn map_mmio_frame(phys: usize) -> *mut u8 {
    assert!(phys % (2 * 1024 * 1024) == 0, "map_mmio_frame received unaligned address");

    let base = MMIO_BASE.load(Ordering::Acquire);
    let base = if base == 0 {
        let computed =
            align_up_size(1024 * 1024 * 1024, 2 * 1024 * 1024);
        MMIO_BASE.store(computed, Ordering::Release);
        computed
    } else {
        base
    };

    let virt = base + MMIO_NEXT.fetch_add(2 * 1024 * 1024, Ordering::Relaxed);

    serial_println!("Mapping MMIO frame: phys={:#x} -> virt={:#x}", phys, virt);

    map_2mib_into_space(
        &mut kernel_address_space(),
        VirtAddr::new(virt),
        PhysAddr::new(phys),
        PageTableFlags::PRESENT
            | PageTableFlags::WRITABLE
            | PageTableFlags::NO_EXECUTE
            | PageTableFlags::NO_CACHE,
    );

    virt as *mut u8
}

/* ========================= PROBE ============================== */

pub fn probe() -> Option<VirtioBlk> {
    for bus in 0u8..=255 {
        for slot in 0u8..32 {
            if pci_read_u16(bus, slot, 0, 0x00) != VIRTIO_VENDOR {
                continue;
            }
            let did = pci_read_u16(bus, slot, 0, 0x02);
            if did != VIRTIO_BLK_DEVICE {
                continue;
            }
            serial_println!("virtio-blk found at {}:{}:{}", bus, slot, 0);
            unsafe { return Some(init(bus, slot)); }
        }
    }
    None
}

/* ====================== INITIALIZATION ======================= */

unsafe fn init(bus: u8, slot: u8) -> VirtioBlk {
    // 1. Reset device
    let status_cmd = pci_read_u16(bus, slot, 0, 0x04);
    pci_write_u16(bus, slot, 0, 0x04, status_cmd | 0x3); // Enable IO + Bus Master

    let mut common: *mut VirtioPciCommonCfg = core::ptr::null_mut();
    let mut notify = core::ptr::null_mut();
    let mut notify_off_multiplier = 0;

    let mut cap = pci_read_u8(bus, slot, 0, 0x34);

    // Cache mapped BARs to avoid re-mapping the same 2MiB region multiple times
    let mut bar_virt_cache = [ptr::null_mut::<u8>(); 6];

    while cap != 0 {
        let cfg_type = pci_read_u8(bus, slot, 0, cap + 3);
        let bar_idx  = pci_read_u8(bus, slot, 0, cap + 4);
        let offset   = pci_read_u32(bus, slot, 0, cap + 8);
        #[allow(unused)]
        let length   = pci_read_u32(bus, slot, 0, cap + 12); // Useful for debug
        let next     = pci_read_u8(bus, slot, 0, cap + 1);

        if bar_idx < 6 {
            // Read 32-bit BAR address
            let bar_reg = 0x10 + (bar_idx as u8 * 4);
            let bar_low = pci_read_u32(bus, slot, 0, bar_reg);
            
            let is_64bit = (bar_low & 0x4) != 0;
            let is_io = (bar_low & 0x1) != 0;

            let bar_addr = if is_64bit {
                let bar_high = pci_read_u32(bus, slot, 0, bar_reg + 4);
                ((bar_high as u64) << 32) | ((bar_low & !0xF) as u64)
            } else {
                (bar_low & !0xF) as u64
            };

            if !is_io && bar_addr != 0 {
                // Calculate alignment
                let page_mask: u64 = 2 * 1024 * 1024 - 1; // 2MiB
                let phys_frame_base = bar_addr & !page_mask;
                let intra_frame_offset = bar_addr & page_mask;

                // Map if not already cached
                let base_ptr = if bar_virt_cache[bar_idx as usize].is_null() {
                    let ptr = map_mmio_frame(phys_frame_base as usize);
                    bar_virt_cache[bar_idx as usize] = ptr;
                    ptr
                } else {
                    bar_virt_cache[bar_idx as usize]
                };

                // Calculate final pointer: VirtualBase + IntraFrameOffset + CapabilityOffset
                let cap_ptr = base_ptr.add((intra_frame_offset + offset as u64) as usize);

                match cfg_type {
                    VIRTIO_PCI_CAP_COMMON => {
                        common = cap_ptr as *mut VirtioPciCommonCfg;
                        serial_println!("Common CFG found at {:p}", common);
                    }
                    VIRTIO_PCI_CAP_NOTIFY => {
                        let mult = pci_read_u32(bus, slot, 0, cap + 16);
                        notify = cap_ptr;
                        notify_off_multiplier = mult;
                        serial_println!("Notify CFG found at {:p}, mult: {}", notify, mult);
                    }
                    _ => {}
                }
            }
        }
        cap = next;
    }

    if common.is_null() || notify.is_null() {
        panic!("virtio: failed to find common or notify capabilities");
    }

    /* === Device Initialization Sequence (VirtIO 1.0) === */

    // 1. Reset
    write_volatile(&mut (*common).device_status, 0);
    compiler_fence(Ordering::SeqCst);

    // 2. Set ACKNOWLEDGE and DRIVER
    write_volatile(
        &mut (*common).device_status,
        read_volatile(&(*common).device_status) | VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER
    );

    // 3. Negotiate Features
    negotiate_features(common);

    // 4. Setup Queues
    write_volatile(&mut (*common).queue_select, 0); // Select Queue 0
    let q_size = read_volatile(&(*common).queue_size);
    
    if q_size == 0 {
        libc::set_os_error(OsError::HardwareFail);
        panic!("virtio: queue size is 0 (mapping issue or device error)");
    }
    serial_println!("Queue 0 size: {}", q_size);

    // Allocate memory for queue (Descriptors + Avail + Used)
    // For simplicity, we stick to 128 (your original size) if device allows, or take the device max
    let q_len = if q_size >= 128 { 128 } else { q_size };
    write_volatile(&mut (*common).queue_size, q_len); // Tell device our size

    let queue_phys = alloc_2mib_frame();
    // Map the queue memory for CPU access
    let queue_virt = map_mmio_frame(queue_phys);

    // Calculate offsets
    // Spec requires alignment. 2MiB frame is definitely aligned enough.
    let size_desc = core::mem::size_of::<VirtqDesc>() * q_len as usize;
    let size_avail = core::mem::size_of::<VirtqAvail>() + 2 * q_len as usize; // +2 for padding if needed, but the struct has padding
    
    let desc_ptr = queue_virt as *mut VirtqDesc;
    let avail_ptr = queue_virt.add(size_desc) as *mut VirtqAvail;
    
    // Used ring must be aligned to 4 bytes
    let used_off = align_up((size_desc + size_avail) as *mut u8, 4096) as usize;
    let used_ptr = queue_virt.add(used_off) as *mut VirtqUsed;

    // Zero out the queue memory
    ptr::write_bytes(queue_virt, 0, 2 * 1024 * 1024);

    // Write physical addresses to device
    let phys_avail = queue_phys as u64 + size_desc as u64;
    let phys_used = queue_phys as u64 + used_off as u64;

    write_volatile(&mut (*common).queue_desc, queue_phys as u64);
    write_volatile(&mut (*common).queue_avail, phys_avail);
    write_volatile(&mut (*common).queue_used, phys_used);

    // 5. Enable Queue
    // CRITICAL FIX: Do NOT write 0 to queue_enable. It is one-way 0->1.
    write_volatile(&mut (*common).queue_enable, 1);

    // 6. Set DRIVER_OK
    write_volatile(
        &mut (*common).device_status,
        read_volatile(&(*common).device_status) | VIRTIO_STATUS_DRIVER_OK
    );
    compiler_fence(Ordering::SeqCst);

    // Verify it stuck
    if read_volatile(&(*common).device_status) & VIRTIO_STATUS_FAILED != 0 {
        panic!("virtio: device set FAILED status");
    }

    const SCRATCH_REGION: usize = USER_DATA_BASE - PAGE_SIZE; 

    let scratch_phys = alloc_2mib_frame();

    map_2mib_into_space(
        &mut kernel_address_space(), 
        VirtAddr::new(SCRATCH_REGION), 
        PhysAddr::new(scratch_phys), 
        PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE
    );

    VirtioBlk {
        common,
        notify,
        notify_off_mul: notify_off_multiplier,
        desc: desc_ptr,
        avail: avail_ptr,
        used: used_ptr,
        last_used: 0,
        capacity: 0, // TODO: Read capacity from Device Config (struct at VIRTIO_PCI_CAP_DEVICE)

        dma_phys: scratch_phys as u64,
        dma_virt: SCRATCH_REGION as *mut u8,

        slot_in_use: [false; MAX_CONCURRENT]
    }
}

#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn negotiate_features(common: *mut VirtioPciCommonCfg) {
    let mut device_features: u64 = 0;

    // Read Device Features (Bank 0)
    write_volatile(&mut (*common).device_feature_select, 0);
    device_features |= read_volatile(&(*common).device_feature) as u64;

    // Read Device Features (Bank 1)
    write_volatile(&mut (*common).device_feature_select, 1);
    device_features |= (read_volatile(&(*common).device_feature) as u64) << 32;

    // Negotiate (We want VIRTIO_F_VERSION_1)
    let driver_features: u64 = (1 << VIRTIO_F_VERSION_1) & device_features;

    // Write Driver Features
    write_volatile(&mut (*common).driver_feature_select, 0);
    write_volatile(&mut (*common).driver_feature, driver_features as u32);
    
    write_volatile(&mut (*common).driver_feature_select, 1);
    write_volatile(&mut (*common).driver_feature, (driver_features >> 32) as u32);

    // Set FEATURES_OK
    let status = read_volatile(&(*common).device_status);
    write_volatile(&mut (*common).device_status, status | VIRTIO_STATUS_FEATURES_OK);
    
    // Re-read status to ensure device accepted it
    compiler_fence(Ordering::SeqCst);
    let new_status = read_volatile(&(*common).device_status);
    if new_status & VIRTIO_STATUS_FEATURES_OK == 0 {
        panic!("virtio: device rejected features");
    }
}

use crate::disk::{AsyncBlockDevice, register_disk};
use crate::lib_alloc::{heap_end};
use crate::serial_println;

pub fn probe_and_register() {
    if let Some(dev) = probe() {
        let dev: &'static mut VirtioBlk = Box::leak(Box::new(dev));
        register_disk(dev);
        crate::log::info!("disk/virtio: registered disk");
    }
}


/* ====================== IO CORE ============================== */

impl VirtioBlk {
    /// Dispatches a request to the device and returns the slot index used.
    /// This does NOT wait for the device to finish.
    unsafe fn submit_async(&mut self, sector: u64, buf_phys: usize, write: bool) -> Result<usize> {
        // 1. Find a free slot
        let slot = self.slot_in_use.iter().position(|&usage| !usage)
            .ok_or_else(|| Error::new(ErrorKind::Other, "No free VirtIO slots"))?;

        self.slot_in_use[slot] = true;

        // 2. Setup DMA memory for Header and Status
        let offset = slot * 64; 
        let req_ptr = self.dma_virt.add(offset) as *mut VirtioBlkReq;
        let stat_ptr = self.dma_virt.add(offset + 32);

        (*req_ptr).type_ = if write {
            libc::set_os_error(OsError::Unsupported);
            assert!(offset % 512 == 0, "Cross sector writes are not supported");
            VIRTIO_BLK_T_OUT 
        } else { VIRTIO_BLK_T_IN };
        (*req_ptr).sector = sector;
        *stat_ptr = 0xFF; // Device will overwrite this with 0 on success

        // 3. Chain 3 Descriptors (Header -> Data -> Status)
        let base_desc = (slot * 3) as u16;
        
        // Header
        *self.desc.add(base_desc as usize) = VirtqDesc {
            addr: self.dma_phys + offset as u64,
            len: core::mem::size_of::<VirtioBlkReq>() as u32,
            flags: VIRTQ_DESC_F_NEXT,
            next: base_desc + 1,
        };
        // Data
        *self.desc.add(base_desc as usize + 1) = VirtqDesc {
            addr: buf_phys as u64,
            len: 512,
            flags: if write { VIRTQ_DESC_F_NEXT } else { VIRTQ_DESC_F_NEXT | VIRTQ_DESC_F_WRITE },
            next: base_desc + 2,
        };
        // Status
        *self.desc.add(base_desc as usize + 2) = VirtqDesc {
            addr: self.dma_phys + offset as u64 + 32,
            len: 1,
            flags: VIRTQ_DESC_F_WRITE,
            next: 0,
        };

        // 4. Update Avail Ring
        let avail_idx = (*self.avail).idx as usize % 128;
        (*self.avail).ring[avail_idx] = base_desc;
        compiler_fence(Ordering::Release);
        (*self.avail).idx = (*self.avail).idx.wrapping_add(1);

        // 5. Kick the device
        let notify_offset = (*self.common)._queue_notify_off;
        let notify_addr = self.notify.add(notify_offset as usize * self.notify_off_mul as usize) as *mut u16;
        write_volatile(notify_addr, 0); // Notify Queue 0

        Ok(slot)
    }

    /// Checks if a specific slot has completed.
    unsafe fn poll_slot(&mut self, slot: usize) -> bool {
        let stat_ptr = self.dma_virt.add(slot * 64 + 32);
        // If the status is no longer 0xFF, the device has written to it.
        read_volatile(stat_ptr) != 0xFF
    }
    
    /// Releases the slot for reuse and returns if the operation was successful.
    unsafe fn complete_slot(&mut self, slot: usize) -> Result<()> {
        let stat_ptr = self.dma_virt.add(slot * 64 + 32);
        let status = read_volatile(stat_ptr);
        
        self.slot_in_use[slot] = false;
        
        // We also need to advance the internal 'last_used' tracker
        // In a real async driver, you'd process the Used Ring properly here.
        self.last_used = self.last_used.wrapping_add(1);

        if status == 0 { Ok(()) } else { Err(Error::new(ErrorKind::Other, "Disk IO Error")) }
    }
}

impl VirtioBlk {

    /// Returns a Future that resolves when the read is complete.
    /// This is the primary entry point for async disk I/O.
    fn read_async(&mut self, sector: u64, buf_phys: u64) -> Result<VirtioFuture> {
        unsafe {
            // Reuses the established async submission logic
            let slot = self.submit_async(sector, buf_phys as usize, false)?;
            Ok(VirtioFuture { dev: self, slot })
        }
    }
}

impl VirtioBlk {
    /// synchronously does block I/O
    unsafe fn do_block_io(
        &mut self,
        block: u64,
        buf_phys: usize,
        write: bool,
    ) -> Result<()> {
        let slot = self.submit_async(block, buf_phys, write)?;
        while !self.poll_slot(slot) {
            crate::arch::cpu::relax();
        }
        self.complete_slot(slot)
    }
}

/* ====================== Trait Impl =========================== */

unsafe impl Send for VirtioBlk {}
unsafe impl Sync for VirtioBlk {}

impl VirtioBlk {
    fn check_block_buf(&self, len: usize) -> Result<()> {
        if len != self.block_size() {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "buffer size must equal block size",
            ));
        }
        Ok(())
    }

    unsafe fn submit_block_async(
        &mut self,
        block: u64,
        phys: usize,
        write: bool,
    ) -> Result<VirtioFuture<'_>> {
        let slot = self.submit_async(block, phys, write)?;
        Ok(VirtioFuture {
            dev: self,
            slot
        })
    }
}

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

pub struct VirtioFuture<'a> {
    dev: &'a mut VirtioBlk,
    slot: usize,
}

impl<'a> Future for VirtioFuture<'a> {
    type Output = Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        unsafe {
            let slot = self.slot;
            // 1. Check if the device marked this slot as done
            if self.dev.poll_slot(slot) {
                // 2. Clean up and return result
                Poll::Ready(self.dev.complete_slot(slot))
            } else {
                // TODO: add waker to some type of queue for waking.
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }
}

impl AsyncBlockDevice for VirtioBlk {
    fn block_size(&self) -> usize {
        512
    }

    fn block_count(&self) -> u64 {
        self.capacity
    }

    fn read_block_async<'a>(
        &'a mut self,
        block: u64,
        buf: &'a mut [u8],
    ) -> Pin<Box<dyn Future<Output = Result<()>> + 'a>> {
        Box::pin(async move {
            self.check_block_buf(buf.len())?;

            let virt = VirtAddr::new(buf.as_ptr().addr());
            let phys = unsafe {
                virt_to_phys(virt)
                    .ok_or_else(|| Error::new(ErrorKind::Other, "buffer not mapped"))?
            };

            unsafe {
                self.submit_block_async(block, phys.as_inner(), false)?
            }
            .await
        })
    }

    fn write_block_async<'a>(
        &'a mut self,
        block: u64,
        buf: &'a [u8],
    ) -> Pin<Box<dyn core::future::Future<Output = Result<()>> + 'a>> {
        Box::pin(async move {
            self.check_block_buf(buf.len())?;

            let virt = VirtAddr::new(buf.as_ptr().addr());
            let phys = unsafe {
                virt_to_phys(virt)
                    .ok_or_else(|| Error::new(ErrorKind::Other, "buffer not mapped"))?
            };

            unsafe {
                self.submit_block_async(block, phys.as_inner(), true)?
            }
            .await
        })
    }
}
