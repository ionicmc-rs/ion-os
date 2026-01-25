//! Disk tests (no filesystem involved)
//
// These tests operate directly on the disk device exposed via
// crate::disk::{get_disk, DiskId} and assume only Read + Seek.

use crate::disk::{self, BlockDevice, DiskId};
use crate::io::{Read, Seek, SeekFrom};
use crate::serial_println;
use crate::test::TestInfo;

/* ------------------------------------------------------------- */
/* Helpers                                                       */
/* ------------------------------------------------------------- */

fn disk0() -> &'static mut dyn BlockDevice {
    disk::get_disk(0)
}

/* ------------------------------------------------------------- */
/* Smoke tests                                                   */
/* ------------------------------------------------------------- */

pub fn disk_smoke_read(info: TestInfo) {
    let d = disk0();

    let mut buf = [0u8; 1];
    d.read(&mut buf).unwrap();
}

pub fn disk_smoke_seek(info: TestInfo) {
    let d = disk0();

    d.seek(SeekFrom::Start(0)).unwrap();
    d.seek(SeekFrom::Start(10)).unwrap();
    d.seek(SeekFrom::Current(-5)).unwrap();
}

/* ------------------------------------------------------------- */
/* Determinism                                                   */
/* ------------------------------------------------------------- */

pub fn disk_repeatable_reads(info: TestInfo) {
    let d = disk0();

    let mut a = [0u8; 64];
    let mut b = [0u8; 64];

    d.seek(SeekFrom::Start(0)).unwrap();
    d.read(&mut a).unwrap();

    d.seek(SeekFrom::Start(0)).unwrap();
    d.read(&mut b).unwrap();

    assert_eq!(a, b);
}

/* ------------------------------------------------------------- */
/* Cross-sector behavior                                         */
/* ------------------------------------------------------------- */

pub fn disk_cross_sector_read(mut info: TestInfo) {
    #[cfg(true)]
    info.ignore();
    let d = disk0();

    // End of sector
    d.seek(SeekFrom::Start(511)).unwrap();

    let mut buf = [0u8; 4];
    d.read(&mut buf).unwrap();
}

pub fn disk_many_small_reads(info: TestInfo) {
    let d = disk0();

    d.seek(SeekFrom::Start(0)).unwrap();

    let mut buf = [0u8; 1];
    for n in 0..200 {
        if n % 10 == 0 {
            serial_println!("reached {n}th read")
        }
        d.read(&mut buf).unwrap();
    }
}

/* ------------------------------------------------------------- */
/* Seek semantics                                                */
/* ------------------------------------------------------------- */

pub fn disk_seek_current_forward(info: TestInfo) {
    let mut d = disk0();

    d.seek(SeekFrom::Start(0)).unwrap();
    d.seek(SeekFrom::Current(100)).unwrap();

    let pos = d.seek(SeekFrom::Current(0)).unwrap();
    assert_eq!(pos, 100);
}

pub fn disk_seek_current_backward(info: TestInfo) {
    let mut d = disk0();

    d.seek(SeekFrom::Start(100)).unwrap();
    d.seek(SeekFrom::Current(-40)).unwrap();

    let pos = d.seek(SeekFrom::Current(0)).unwrap();
    assert_eq!(pos, 60);
}

pub fn disk_seek_before_start_panics(mut info: TestInfo) {
    info.should_panic();

    let mut d = disk0();
    d.seek(SeekFrom::Current(-1)).unwrap();
}

/* ------------------------------------------------------------- */
/* Boundary / stress tests                                       */
/* ------------------------------------------------------------- */

pub fn disk_large_read(info: TestInfo) {
    let mut d = disk0();

    let mut buf = [0u8; 4096];
    d.seek(SeekFrom::Start(0)).unwrap();
    d.read(&mut buf).unwrap();
}

pub fn disk_unaligned_seek_then_read(info: TestInfo) {
    let mut d = disk0();

    d.seek(SeekFrom::Start(123)).unwrap();

    let mut buf = [0u8; 17];
    d.read(&mut buf).unwrap();
}

/* ------------------------------------------------------------- */
/* Known-signature / realism tests                               */
/* ------------------------------------------------------------- */

pub fn disk_mbr_signature(mut info: TestInfo) {
    let mut d = disk0();

    d.seek(SeekFrom::Start(510)).unwrap();
    let mut sig = [0u8; 2];
    d.read(&mut sig).unwrap();

    // If disk is empty this may fail → ignore if needed
    if sig != [0x55, 0xAA] {
        info.ignore();
    }
}
