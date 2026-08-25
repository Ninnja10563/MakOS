#!/usr/bin/env python3
"""Structural guard for musl symlink and Unix timestamp runtime proof."""

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PATCH = (ROOT / "ports/musl/patches/0062-makos-symlinks-timestamps.patch").read_text()
APPLY = (ROOT / "ports/musl/apply-patches.sh").read_text()
PROBE = (ROOT / "ports/musl/pthread-probe.c").read_text()
HARNESS = (ROOT / "scripts/boot_test_aarch64.py").read_text()
KERNEL = (ROOT / "kernel/src/arch/aarch64.rs").read_text()
VFS = (ROOT / "kernel/src/vfs.rs").read_text()

assert not (ROOT / "ports/musl/drafts/0060-makos-symlinks-timestamps.patch").exists()
for symbol in (
    "L_symlinkat = 36",
    "M_symlink = 130",
    "M_readlink = 131",
    "M_stat_extended = 132",
    "M_fstat_extended = 133",
    "struct makos_extended_metadata",
    "case L_symlinkat:",
    "case L_readlinkat:",
    "AT_SYMLINK_NOFOLLOW",
    "DT_LNK",
):
    assert symbol in PATCH

assert 'patches/0062-makos-symlinks-timestamps.patch' in APPLY
assert APPLY.count("patches=65") == 4
for operation in (
    "symlink(metadata_target, metadata_link)",
    "readlink(metadata_link, link_output, sizeof link_output)",
    "lstat(metadata_link, &link_metadata)",
    "stat(metadata_link, &path_metadata)",
    "directory_entry->d_type == DT_LNK",
    "unlink(metadata_link)",
    "target_metadata.st_atim.tv_sec < 1577836800",
):
    assert operation in PROBE

metadata_marker = "metadata=stat,fstat,regular,fifo,tty,timestamps-unix"
symlink_marker = "symlink=create,readlink,lstat,follow,readdir,unlink"
assert metadata_marker in PROBE
assert symlink_marker in PROBE
assert f"{metadata_marker} {symlink_marker}" in HARNESS
for syscall in ("SYS_SYMLINK", "SYS_READLINK", "SYS_STAT_EXTENDED", "SYS_FSTAT_EXTENDED"):
    assert syscall in KERNEL
extended_fstat = KERNEL[KERNEL.index("SYS_FSTAT_EXTENDED =>") : KERNEL.index("SYS_READ_DIR =>")]
assert "if fd <= 2" in extended_fstat
assert "mode: 0o020620" in extended_fstat
assert "crate::vfs::metadata_extended_for_fd(fd)" in extended_fstat
for implementation in ("create_symlink", "read_link", "stat_extended", "metadata_extended_for_fd"):
    assert f"pub fn {implementation}" in VFS
for virtual_parent in ("ROOT_PATH", "HOME_PATH", "USER_DIRECTORY_PATH"):
    assert virtual_parent in VFS[VFS.index("pub fn read_link") : VFS.index("pub fn unlink")]
assert "if virtual_node_exists" in VFS
assert "Err(DescriptorError::Invalid)" in VFS[VFS.index("pub fn read_link") : VFS.index("pub fn unlink")]

print(
    "MAKOS_AARCH64_SYMLINK_TIMESTAMPS_TEST_OK "
    "musl=patch62 symlink=create,readlink,lstat,follow,readdir,unlink "
    "metadata=atime,mtime,ctime,unix-epoch virtual_nonlink=EINVAL"
)
