#!/usr/bin/env python3
"""Structural guard for AArch64 guest-driven durable package runtime proof."""

from pathlib import Path
import re


ROOT = Path(__file__).resolve().parents[1]
SHELL = (ROOT / "user/aarch64_shell.c").read_text()
X86_INIT = (ROOT / "user/init.rs").read_text()
DESKTOP = (ROOT / "kernel/src/aarch64_desktop.rs").read_text()
VFS = (ROOT / "kernel/src/vfs.rs").read_text()
FS = (ROOT / "kernel/src/fs.rs").read_text()
HARNESS = (ROOT / "scripts/boot_test_aarch64_package.py").read_text()


def signature(source: str, name: str) -> str:
    if source is SHELL:
        pattern = rf"static const char {name}_HEX\[\] =\s*\n\s*\"([0-9a-f]+)\";"
    else:
        pattern = rf"const {name}: \[u8; 256\] = decode_signature\(\s*b\"([0-9a-f]+)\""
    match = re.search(pattern, source)
    assert match, f"missing {name}"
    assert len(match.group(1)) == 512
    return match.group(1)


for fixture in ("FIRST_PACKAGE_SIGNATURE", "SECOND_PACKAGE_SIGNATURE"):
    assert signature(SHELL, fixture) == signature(X86_INIT, fixture)

for syscall in (
    "SYS_PACKAGE_INSTALL",
    "SYS_PACKAGE_QUERY",
    "SYS_PACKAGE_ROLLBACK",
    "SYS_PACKAGE_REMOVE",
):
    assert syscall in SHELL

for command in (
    "pkg-probe-install",
    "pkg-probe-query-v1",
    "pkg-probe-query-v2",
    "pkg-probe-remove",
    "pkg-probe-rollback",
):
    assert SHELL.count(f'"{command}"') >= 2

assert 'if name.starts_with(b"/")' in DESKTOP
assert "MAKOS_AARCH64_SHELL_CAT_ERROR reason=not-found path={}" in DESKTOP
assert "MAKOS_AARCH64_SHELL_CMD cat bytes={} path={}" in DESKTOP
assert "struct PackageSnapshot" in VFS
assert "package_snapshot = PackageSnapshot::capture(package);" in VFS
assert VFS.count("description.package_snapshot.mounted()") >= 4
assert "Open file descriptions retain their captured package backing" in VFS
assert "package_transaction_sector_pinned" in VFS
assert "description.references != 0" in VFS
assert "package_file_by_path(state, path)" in VFS
assert "crate::fs::read_package_file(&package, 0, &mut output[..length])" in VFS
assert "crate::vfs::package_transaction_sector_pinned(sector)" in FS
assert "MAKOS_PACKAGE_PINNED_SLOT_BLOCKED" in FS
assert "MAKOS_PACKAGE_LIVE_REFRESH_OK result={}" in FS
assert "open_fd_pin=replace" in SHELL
assert "open_fd_pin=remove" in SHELL
assert "pinned_reuse_denied=1" in SHELL
assert "pinned_rollback_denied=1" in SHELL
assert "open_fd_generation_pin=replace,remove" in HARNESS

for generation in (2, 3, 4, 5):
    assert f"generation={generation} packages=" in HARNESS
assert "corrupt_newest_header" in HARNESS
assert "os.fsync(image.fileno())" in HARNESS
assert "MAKOS_AARCH64_PACKAGE_RUNTIME_OK" in HARNESS

print(
    "MAKOS_AARCH64_PACKAGE_PROBE_TEST_OK "
    "fixtures=canonical-rsa2048 commands=install,query,remove,rollback "
    "vfs=absolute-package-path,open-fd-generation-pin "
    "runtime=multi-boot,corrupt-newest-fallback"
)
