#!/usr/bin/env python3
"""Guard AArch64 interrupted-install resume and fail-closed policy."""

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
INSTALLER = (ROOT / "kernel/src/aarch64_installer.rs").read_text()
BLOCK = (ROOT / "kernel/src/aarch64_virtio_blk.rs").read_text()
DESKTOP = (ROOT / "kernel/src/aarch64_desktop.rs").read_text()
HARNESS = (ROOT / "scripts/boot_test_aarch64_install.py").read_text()

for fragment in (
    "impl Disk for VirtioDisk",
    "InstallMode::Fresh",
    "InstallMode::Resume",
    "makos_installer::install(",
    "InstallError::ResumeCommitted",
    "InstallError::ResumeConflict",
    "commit=protective-mbr-last",
    "freeze_source_writes()",
    "keep_until_shutdown()",
):
    assert fragment in INSTALLER, fragment

for fragment in (
    "SOURCE_WRITES_FROZEN",
    "pub fn freeze_source_writes()",
    "submit(state, REQUEST_FLUSH, 0, 0)",
    "device == 0 && SOURCE_WRITES_FROZEN.load(Ordering::Acquire)",
    "pub fn keep_until_shutdown",
):
    assert fragment in BLOCK, fragment

for fragment in (
    'b"install disk1 erase-disk1"',
    'b"install disk1 resume-disk1"',
    "makos_installer::InstallMode::Fresh",
    "makos_installer::InstallMode::Resume",
):
    assert fragment in DESKTOP, fragment

for fragment in (
    "process.kill()",
    "verify_interrupted_copy(source, target)",
    'target_file.read(512) != bytes(512)',
    'send_command(stream, "install disk1 resume-disk1")',
    'b"MAKOS_INSTALL_BEGIN source=disk0 target=disk1 mode=resume"',
    'b"MAKOS_INSTALL_ERROR error=ResumeConflict"',
    "digest(target) != digest(source)",
    "except subprocess.TimeoutExpired:",
    "source_detached=1 persistence=two-boot",
):
    assert fragment in HARNESS, fragment

print(
    "MAKOS_AARCH64_INSTALL_RESUME_TEST_OK "
    "interrupt=process-kill mbr=blank source=flush,write-frozen partial=source-identical "
    "resume=exact-command digest=match detached=two-boot"
)
