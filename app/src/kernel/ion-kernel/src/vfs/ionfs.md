# IonFS Specification

IonFS is a **minimal, developer‑satisfaction–focused filesystem** designed for IonOS.

Its goals are:
- simplicity over cleverness
- predictable on‑disk layout
- low memory overhead
- easy debugging and inspection
- structures that are trivial to reason about in kernel code

IonFS is not POSIX‑compatible by design. It is opinionated, explicit, and intentionally small.

---

## Design Philosophy

- **Minimal**: no journaling, no extents, no indirect blocks
- **Explicit**: all metadata layouts are fixed and documented
- **Developer‑friendly**: easy to inspect with a hex editor
- **Kernel‑first**: optimized for kernel use, not user tooling
- **No hidden magic**: every byte has a clear purpose

---

## Top‑Level Virtual Paths

IonFS uses reserved prefixes for top‑level paths:

| Prefix | Meaning |
|------|--------|
| `@/` | Root filesystem |
| `#/` | Temporary files |
| `$<mnt>/` | Mount points (specifically under `<mnt>`) |

These prefixes are resolved by the VFS layer, not IonFS itself.

---

## Disk Layout Overview

The disk is divided into fixed‑size blocks (typically 512 bytes).

```
Block 0   → Superblock
Block 1   → Allocation Block
Block 2+  → Inode Blocks and User Data
```

There is no strict separation between inode blocks and data blocks beyond allocation tracking.

---

## Superblock (Block 0)

The superblock contains global filesystem metadata.

| Offset | Size | Field |
|------|------|------|
| 0–3 | 4 bytes | Magic number |
| 4–7 | 4 bytes | Version |
| 8–15 | 8 bytes | Block size |
| 16–23 | 8 bytes | Total inode count |
| 24–31 | 8 bytes | User data start offset |
| 32–end | remaining | Reserved (zeroed) |

The superblock is fixed‑location and always resides at block 0.

---

## Allocation Block (Block 1)

The allocation block tracks inode table blocks.

### Layout

| Field | Size |
|----|----|
| Magic number | 4 bytes |
| Version | 4 bytes |
| Inode block count | 8 bytes |
| Inode block entries | 8 bytes each |
| Remaining bytes | unused |

### Notes

- Each inode block entry is a **block index**
- There is no free bitmap
- Allocation is linear and explicit
- The allocation block grows only when inode blocks are added

---

## Inode Blocks

Each inode block contains multiple inodes.

### Inode Block Header

| Field | Size |
|----|----|
| Free hint | 2 bytes |
| Reserved | 2 bytes |

The **free hint** points to the first unused inode in this block.
If the block is full, the free hint is set to `0xFFFF`.

---

## Inode Layout

Each inode has a **fixed, explicit layout**.

| Field | Size | Description |
|----|----|----|
| `is_used` | 1 byte | `0` = free, `1` = allocated |
| `kind` | 1 byte | `0` = File, `1` = Directory |
| `capacity` | 8 bytes | Allocated data capacity |
| `data_offset` | 8 bytes | Absolute disk offset of data |
| `size` | 8 bytes | Actual used size |
| `index` | 8 bytes | Inode index (stored explicitly) |

### Notes

- `capacity` ≥ `size` always
- `index` is stored to avoid recomputation
- No indirect pointers or block lists
- Data is always contiguous on disk

---

## File Data

Files have **no internal structure**.

- File data is a raw byte range
- Stored at `data_offset`
- Length is defined by `size`
- Capacity limits writes

IonFS does **not** support sparse files.

---

## Directory Layout

Directories are stored as linear byte streams.

Each directory entry is encoded as:

```
[name_len: u16]
[name: u8[name_len]]  // UTF‑8
[inode_index: u64]
```

### Properties

- Variable‑length entries
- No `.` or `..`
- No hashing or tree structure
- Directory traversal is linear
- Entries are ordered by insertion

---

## Inode Allocation Rules

1. Search existing inode blocks using their free hints
2. If all inode blocks are full:
   - allocate a new inode block
   - append it to the allocation block
3. Update free hint on allocation and free
4. Freed inodes may be reused

There is no global inode bitmap.

---

## Error Handling & Recovery

IonFS assumes:
- clean shutdowns
- controlled kernel access
- no concurrent writers at the filesystem level

Corruption handling is intentionally minimal.

---

## Versioning

IonFS uses a simple version field:

- 4 bytes total
- interpretation is filesystem‑specific
- incompatible versions must be rejected at mount time

---

## Non‑Goals

IonFS explicitly does **not** aim to provide:

- journaling
- crash recovery
- permissions / ACLs
- timestamps
- symbolic links
- hard links

These are handled at higher layers or intentionally omitted.

---

## Summary

> *“The filesystem you can fully understand in one sitting.”*

Every structure is fixed, explicit, and directly mapped to disk.
There are no hidden layers, no implicit state, and no surprising behavior.

IonFS is ideal for kernel development, experimentation, and tightly controlled systems