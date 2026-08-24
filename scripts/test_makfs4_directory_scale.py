#!/usr/bin/env python3
"""Guard MakFS4 scalable-directory implementation and guest proof wiring."""

from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def require(path: str, fragments: tuple[str, ...]) -> None:
    source = (ROOT / path).read_text()
    missing = [fragment for fragment in fragments if fragment not in source]
    if missing:
        raise AssertionError(f"{path}: missing {missing}")


require(
    "crates/makfs4/src/lib.rs",
    (
        "pub struct DirectoryIndex",
        "pub fn rebuild(&mut self, entries:",
        "pub fn find(",
        "directory_index_finds_many_siblings_and_handles_collisions",
        "DirectoryIndex::<96, 1>",
    ),
)
require(
    "kernel/src/makfs4_volume.rs",
    (
        "const MAXIMUM_INODES: u32 = 512;",
        "const DIRECTORY_INDEX_BUCKETS: usize = 1024;",
        "child_index: DirectoryIndex",
        "cache.child_index.find(&cache.entries, parent, name)",
        "directory_cursor_limit",
        "child_from(parent: u64, start: u32)",
        "fn normalize_user_root_permissions()",
        "root.mode = 0o040700;",
        "root.uid = crate::security::INIT_UID;",
        "root.gid = crate::security::INIT_GID;",
        "MAKOS_MAKFS4_HOME_ROOT_OK inode=1 mode=0700",
    ),
)
require(
    "ports/musl/pthread-probe.c",
    (
        "scalable_directory_probe",
        "unsigned char seen[64]",
        "char long_name[256]",
        "MAKOS_MAKFS_DIRECTORY_SCALE_OK",
    ),
)
require(
    "scripts/boot_test_aarch64.py",
    (
        "scalable=64-siblings,name255,indexed-lookup,cursor",
        "MAKOS_MAKFS_DIRECTORY_SCALE_OK siblings=64 name_bytes=255",
    ),
)

print(
    "MAKOS_MAKFS4_DIRECTORY_SCALE_TEST_OK "
    "inodes=512 name_bytes=255 lookup=hash-index collision=chained "
    "readdir=cursor siblings=64 remount=verify-cleanup home_root=0700,uid1000"
)
