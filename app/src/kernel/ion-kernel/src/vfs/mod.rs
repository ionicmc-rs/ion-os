#![doc = include_str!("ionfs.md")]

use alloc::{format, vec::Vec};

use crate::{disk::AsyncBlockDevice, io, log, vfs::{fs::parse::read_inode, structures::{Inode, InodeKind}}};
pub mod structures;
pub mod fs;
pub mod fd;

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
            log::debug!("Found entry '{}' in directory", name);
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

pub async fn resolve_path(
    disk: &mut dyn AsyncBlockDevice,
    path: &str,
) -> io::Result<Inode> {
    log::debug!("Resolving path: {}", path);
    let parts = split_path(path)?;
    log::debug!("Path parts: {:?}", parts);

    // root inode is always 0
    let mut current = read_inode(disk, 0).await?;

    for part in parts {
        log::debug!("Looking up part '{}' in inode {:?}", part, current);
        let next_idx = lookup_in_dir(&current, disk, part)
            .await?
            .ok_or_else(|| io::Error::new(
                io::ErrorKind::NotFound,
                format!("path component not found: {}", part),
            ))?;
        log::debug!("Found inode index {} for part '{}'", next_idx, part);

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

pub async fn resolve_path_to_index(
    disk: &mut dyn AsyncBlockDevice,
    path: &str,
) -> io::Result<u64> {
    let inode = resolve_path(disk, path).await?;
    Ok(inode.index())
}