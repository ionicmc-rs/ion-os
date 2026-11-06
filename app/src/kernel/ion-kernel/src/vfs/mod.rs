#![doc = include_str!("ionfs.md")]

use alloc::{format, string::String, vec::Vec};

use crate::{
    disk::{self, AsyncBlockDevice, DiskId},
    io, log,
    vfs::{
        fs::parse::{read_inode, read_superblock},
        structures::{Inode, InodeKind},
    },
};
pub mod fd;
pub mod fs;
pub mod structures;

#[derive(Clone, Copy, Debug)]
pub struct Mount {
    pub id: usize,
    pub disk: DiskId,
}

const MAX_MOUNTS: usize = 16;

static mut MOUNTS: [Option<Mount>; MAX_MOUNTS] = [None; MAX_MOUNTS];
static mut MOUNT_COUNT: usize = 0;
static mut ROOT_MOUNT: Option<Mount> = None;

#[allow(static_mut_refs)]
pub fn init_mounts() {
    unsafe {
        MOUNTS = [None; MAX_MOUNTS];
        MOUNT_COUNT = 0;
        ROOT_MOUNT = None;
    }

    for disk_index in 0..disk::disk_count() {
        let Some(disk_id) = DiskId::new(disk_index) else {
            continue;
        };

        let is_ionfs = crate::asyncio::run_future(async {
            read_superblock(disk::get_disk(disk_id)).await.is_ok()
        });

        if !is_ionfs {
            continue;
        }

        unsafe {
            if MOUNT_COUNT >= MAX_MOUNTS {
                log::error!(
                    "vfs: mount table full, ignoring additional IonFS partition on {:?}",
                    disk_id
                );
                break;
            }

            let mount = Mount {
                id: MOUNT_COUNT,
                disk: disk_id,
            };

            MOUNTS[MOUNT_COUNT] = Some(mount);
            MOUNT_COUNT += 1;

            if ROOT_MOUNT.is_none() {
                ROOT_MOUNT = Some(mount);
                log::info!("vfs: mounted root filesystem from {:?}", disk_id);
            } else {
                log::info!(
                    "vfs: mounted IonFS partition as #{} from {:?}",
                    mount.id,
                    disk_id
                );
            }
        }
    }
}

pub fn root_mount() -> Option<Mount> {
    unsafe { ROOT_MOUNT }
}

pub fn mount(id: usize) -> Option<Mount> {
    unsafe { MOUNTS.get(id).copied().flatten() }
}

pub fn resolve_virtual_path(path: &str) -> io::Result<(DiskId, String)> {
    if let Some(rest) = path.strip_prefix("@/") {
        let root = root_mount().ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "root filesystem is not mounted")
        })?;

        return Ok((root.disk, format!("@/{rest}")));
    }

    let Some(path) = path.strip_prefix('#') else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path must start with @/ or #<mount-id>/",
        ));
    };

    let Some((mount_id, rest)) = path.split_once('/') else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "mount paths must use the form #<mount-id>/...",
        ));
    };

    let mount_id = mount_id
        .parse::<usize>()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid mount identifier"))?;

    let mount = mount(mount_id)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "mount not found"))?;

    Ok((mount.disk, format!("@/{rest}")))
}

pub async fn lookup_in_dir(
    dir: &Inode,
    disk: &mut dyn AsyncBlockDevice,
    name: &str,
) -> io::Result<Option<u64>> {
    if dir.kind() != InodeKind::Directory {
        return Err(io::Error::new(
            io::ErrorKind::NotADirectory,
            "inode is not a directory",
        ));
    }

    let entries = dir.read_dir(disk).await?;

    for (entry_name, inode_idx) in entries {
        if entry_name == name.as_bytes() {
            return Ok(Some(inode_idx));
        }
    }

    Ok(None)
}

fn split_path(path: &str) -> io::Result<Vec<&str>> {
    if !path.starts_with("@/") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path must be absolute",
        ));
    }

    Ok(path
        .split('/')
        .filter(|s| !s.is_empty())
        .filter(|s| s != &"@" && s != &".")
        .collect())
}

pub async fn resolve_path(disk: &mut dyn AsyncBlockDevice, path: &str) -> io::Result<Inode> {
    let parts = split_path(path)?;

    // root inode is always 0
    let mut current = read_inode(disk, 0).await?;

    for part in parts {
        let next_idx = lookup_in_dir(&current, disk, part).await?.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("path component not found: {}", part),
            )
        })?;

        let next = read_inode(disk, next_idx).await?;

        current = next;
    }

    if current.kind() == InodeKind::Directory && !path.ends_with('/') {
        return Err(io::Error::new(
            io::ErrorKind::NotADirectory,
            "path refers to a directory but does not end with '/'",
        ));
    }

    if current.kind() != InodeKind::Directory && path.ends_with('/') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path refers to a file but ends with '/'",
        ));
    }

    Ok(current)
}

pub async fn resolve_path_to_index(disk: &mut dyn AsyncBlockDevice, path: &str) -> io::Result<u64> {
    let inode = resolve_path(disk, path).await?;
    Ok(inode.index())
}
