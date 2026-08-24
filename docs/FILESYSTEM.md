# MakFS and VFS

MakOS uses a sparse 1 GiB data disk through ATA or virtio-blk. Sector size is
512 bytes. Existing images extend in place without changing stored sectors.
Current VFS remains native MakFS, not FAT or host-backed storage.

Both x86_64 and AArch64 support legacy raw images and single-disk GPT layout.
The GPT image contains a UEFI ESP at LBA 2,048 and a MakOS data partition at
LBA 133,120. Filesystem LBAs are relative to that partition. Primary/backup GPT
headers and entry-array CRCs are validated before writes; invalid GPT never
falls back to whole-disk raw writes.

## Stable metadata

- LBA 1: primary `MAKFS001` superblock.
- LBA 2: backup superblock.
- LBA 3: checksummed root record for `/boot-count.txt`.
- LBA 4: root-file data.
- LBA 5: update-generation record.
- LBA 6: legacy fixed `/home/user/note.txt` record.

Superblocks contain format version, generation, boot count, and CRC-32. Mount
chooses the newest valid copy. With `makfs.recover=auto`, one valid copy repairs
the other before VFS exposure.

## Dynamic allocation format v2

- LBA 7: `MAKALC02` allocation header and 80-bit data-sector bitmap.
- LBA 8–23: sixteen `MAKINOD2` inode records.
- LBA 32–111: eighty data sectors.

Each inode stores active flag, slot, name, mode/UID/GID, byte length, whole-file
CRC-32, and up to four 16-bit block indexes. Names are at most 32 validated ASCII
bytes. Files are at most 2,048 bytes. Data blocks need not be contiguous.

Write replacement uses this order:

1. Allocate different free blocks and write zero-padded sectors.
2. Persist bitmap with new and old blocks reserved.
3. Persist CRC-protected inode pointing at new blocks.
4. Release old blocks and persist bitmap again.

Mount scans every valid inode, rejects duplicate/out-of-range blocks, rebuilds
the bitmap, and verifies whole-file CRCs. Thus interrupted replacement may leak
blocks temporarily but cannot make them available while referenced; next mount
reclaims leaks. Bitmap corruption is repaired from inodes.

If no v2 allocator exists, mount reads up to four legacy `MAKDYN02` records from
LBA 7–10, initializes v2 metadata, and rewrites active files. Automated boot
testing exercises this migration.

## VFS behavior

Mounted namespace currently contains `/`, `/home`, and `/home/user`. Native
syscalls provide process-owned FDs, open/read/write/close, shared-offset
duplication, bounded seek, create/unlink/rename, stat/readdir, ownership, modes,
size, timestamp, and inode number. Arbitrary
leaf names under `/home/user` accept ASCII letters, digits, `.`, `_`, and `-`.
Nested directories, canonical cwd-relative paths, `O_RDWR`, positional I/O,
durability flushes, and process byte-range record locks are supported.

## MakFS4 region

MakFS4 metadata occupies logical 4 KiB blocks 14 onward, before package header
at block 256. Read-only package payload must end before block 98,304 (384 MiB).
Durable package transactions own blocks 98,304–131,071 as two 64 MiB A/B slots.
MakFS4 extent data begins at block 131,072 and runs to disk end. Blank/legacy disks below
1 GiB remain v3-only; run scripts sparsely extend persistent images first.

Mount validates redundant CRC superblocks and empty COW catalog/bitmap/inode
geometry, formatting only when both roots are absent. Package overlap is fatal.
On AArch64, each logical 4 KiB MakFS4 block uses one checked eight-sector
virtio request. x86_64 uses ATA sector transfers. Shared partition translator
validates complete range before dispatch. See `docs/MAKFS4.md`. VFS large-file
read/write routing remains incomplete.

## Verified and missing

Two-boot QEMU test migrates one legacy record, creates ten files, persists and
validates a 1,024-byte two-block file, corrupts primary superblock plus bitmap,
repairs both, validates all ten files, unlinks them, and proves absence.
`scripts/test_makfs4_block_io.py` structurally gates whole-block AArch64
read/write dispatch and partition-aware bounds; MakFS4 unit tests gate exact
block-to-sector geometry, overflow rejection, and two-flush commit ordering.

Still missing: MakFS4 migration, links, symlinks, page cache, offline checker,
generic partition enumeration/mounting, and runtime fault-injection proof.

## Package transaction engine

`makos-package-store` now provides host-tested A/B sector snapshots for real
payload bytes, exact/minimum semantic-version dependencies, atomic
install/replace/remove, and corrupt/interrupted-generation fallback. Kernel
`DataDisk` implements its sector/flush interface. Layout v1 reserves
LBA 786,432–1,048,575; signed package syscalls route there after authentication
when full geometry and static-package non-overlap checks pass. Small images use
legacy RAM slots. Invalid/overlapping existing regions fail closed without
writes. Active payloads are exposed read-only as
`/packages/<name>/payload`; live activation refreshes after mutations without
shadowing immutable image paths. See `docs/PACKAGE-STORE.md` for format,
migration, dependency wire format, and remaining gaps.
