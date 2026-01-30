# IonFS

IonFS is a minimal yet feature-rich filesystem for IonOS, designed to maximize storage capacity while minimizing memory usage on both disk and RAM.

## Overview

IonFS provides:
- Simple inode-based file/directory structure
- Dynamic inode table allocation
- Efficient space utilization
- Support for files and directories
- Directory entry indexing via (name, inode_index) pairs

## Disk Layout

The filesystem is organized into fixed 512-byte blocks:

```
Block 0: Superblock (metadata)
Block 1: Allocation Block (free space tracking & inode table list)
Block 2+: Inode Tables and User Data
```

### Block 0: Superblock

Contains filesystem metadata and initialization info:

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0-3 | 4 bytes | Magic Number | `0xab87beef` (validation) |
| 4-7 | 4 bytes | Version | [total, major, minor_lo, minor_hi] |
| 8-15 | 8 bytes | Block Size | Size of each block (typically 512) |
| 16-23 | 8 bytes | Inode Count | Total number of allocated inodes |
| 24-31 | 8 bytes | User Data Start | Byte offset where user data region begins |
| 32-511 | 480 bytes | Reserved | Must be all zeros |

**Magic Number:** `0xab87beef` - Used to validate filesystem presence

**Version Format:** Currently supports `[0, 1, 0, 0]` only

### Block 1: Allocation Block

Tracks free space and inode table locations:

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0-3 | 4 bytes | Magic Number | `0xab87befd` (validation) |
| 4-7 | 4 bytes | Version | [total, major, minor_lo, minor_hi] |
| 8-15 | 8 bytes | Next Free Block | Block index for next allocation |
| 16-23 | 8 bytes | Inode Block Count | Number of inode table blocks |
| 24-N | 8 bytes each | Inode Blocks | List of block indices containing inodes |

**Next Free Block:** Points to the first unallocated block for data/inode tables

**Inode Blocks:** Variable-length list of block numbers that contain inode tables

### Blocks 2+: Inode Tables and User Data

Starting from block 2, the disk contains:
- Inode table blocks (containing inode metadata)
- User data blocks (file contents)

## Inode Table Block Format

Each inode table block contains inode metadata:

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0-1 | 2 bytes | Free Hint | Index of first free inode in this block (0xFFFF if full) |
| 2-3 | 2 bytes | Reserved | Must be zero |
| 4-511 | 508 bytes | Inode Entries | Up to 19 inode entries (508 / 26 bytes per inode) |

**Free Hint:** Optimization for fast inode allocation. When full, set to `0xFFFF`.

## Inode Structure

Each inode is exactly 26 bytes and describes a file or directory:

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0 | 1 byte | Used Flag | `0` = unused, `1` = used |
| 1 | 1 byte | Kind | `0` = File, `1` = Directory |
| 2-9 | 8 bytes | Data Size | Total allocated size for this inode's data |
| 10-17 | 8 bytes | Data Offset | Byte offset where inode's data begins |
| 18-25 | 8 bytes | Size | Actual used size (≤ Data Size) |

**Kind:** Determines how data is interpreted:
- **File (0):** Data is raw file contents
- **Directory (1):** Data contains directory entries (see format below)

**Data Size vs Size:**
- `Data Size`: Total allocated space
- `Size`: Actual bytes used (prevents over-allocation)

## Directory Entry Format

Directory data is a sequence of variable-length entries:

```
[name_len: u16 LE][name: bytes][inode_index: u64 LE]
```

Each entry:
1. `name_len` (2 bytes): Length of the filename
2. `name`: Filename bytes (variable length)
3. `inode_index` (8 bytes): Index of the inode this entry points to

Example: Directory entry for `"foo.txt"` pointing to inode 5:
```
07 00          // name_len = 7 (little-endian)
66 6f 6f 2e 74 78 74  // "foo.txt"
05 00 00 00 00 00 00 00  // inode_index = 5 (little-endian)
```

## Inode Allocation

Inodes are allocated from inode table blocks:

1. Search existing inode table blocks using the free hint
2. When free hint reaches the end of a block (0xFFFF), allocate a new inode table block
3. Update the allocation block with the new inode table location
4. New inode table blocks start with free hint = 0

## Supported Versions

Current supported version: **0.1.0.0**

Version format:
- Byte 0: Total version
- Byte 1: Major version
- Bytes 2-3: Minor version (little-endian u16)


### Inode Layout

the layout for inodes is:

- used (1 byte, `1` or `0`)
- kind (1 byte, `File (0)` or `Directory (1)`)
- data size (capacity) (8 bytes)
- offset (disk offset) (8 bytes)
- size (length) (8 bytes)

## Directory Layout (User Data)

Files have no fixed layout. Directories have the layout:

- name length (8 bytes)
- name (bytes of length `name length`)

