# MakFS4 extent/COW storage

MakFS4 replaces MakFS v3's sixteen in-memory 2 KiB files. It is native MakOS
storage, not a host filesystem, Linux mount, archive overlay, or browser-only
shim. `crates/makfs4` contains allocation and validated on-disk metadata code;
kernel block/VFS integration now mounts it as `/home/user`'s scalable backing
while fixed MakFS v3 compatibility files remain readable during migration.

## Geometry

- Logical allocation block: 4096 bytes; block device sectors remain 512 bytes.
- Disk addresses and file sizes: 64-bit.
- File mapping: fourteen ordered extents per inode; contiguous neighbors merge.
- Name: 1..255 bytes in each inode. Catalog establishes parent/child hierarchy.
- Allocation bitmap: caller-sized; allocator checks bounds, overlap, exhaustion,
  and never partially reserves or releases a range.
- Metadata record: 512 bytes with version, little-endian fields, CRC32.
- Two superblock locations: mount validates both and selects highest valid
  generation. Corrupt/newer-invalid copy never masks older valid root.

Fixed profile-volume placement will be outside packaged Firefox payload and
will be recorded in the partition/volume table. Sparse host image allocation is
only build storage optimization; guest sees a normal block device. No profile
sector may overlap read-only package sectors.

## Crash-consistent commit

Every transaction follows enforced `CommitSequencer` order:

1. Write replacement file data extents.
2. Write replacement inode records.
3. Write copy-on-write allocation bitmap.
4. Write copy-on-write catalog root.
5. Flush all new metadata/data.
6. Write older/inactive superblock with next generation.
7. Flush root commit.

Old extents remain reachable until step 7 succeeds. Mount chooses last CRC-valid
generation; later reclamation frees blocks no selected generation references.
This implements architecture's COW/atomic-root design without journal replay.

## Current gate

Source integration now includes sparse 1 GiB volume discovery/format/mount,
redundant validated roots, three rotating metadata sets, extent-backed partial
I/O, create/mkdir/unlink/rmdir/rename, VFS descriptors, `O_RDWR`, positional
I/O, truncate, seek/stat, 255-byte directory names, 4096-byte paths, persistent
directory cursors, and process-scoped POSIX byte-range locks. New user files use
MakFS4; fixed v3 files remain compatibility entries.

Still required before claiming Firefox profile persistence complete:

- atomic replace-rename and POSIX unlink-while-open orphan handling;
- page cache plus block-granular/batched COW (current full-file COW is correct
  but slow for large, frequently updated databases);
- offline checker/recovery and v3 data migration;
- constrained runtime fault-injection, remount, SQLite, and Firefox-profile
  tests after low-resource mode is lifted.
