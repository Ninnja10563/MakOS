#!/usr/bin/env python3
"""Structural guard for the guest-native AArch64 assembler/exec path."""

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TOOLCHAIN = (ROOT / "user/aarch64_toolchain.c").read_text()
SHELL = (ROOT / "user/aarch64_shell.c").read_text()
PROCESS = (ROOT / "kernel/src/aarch64_process.rs").read_text()
ARCH = (ROOT / "kernel/src/arch/aarch64.rs").read_text()
SECURITY = (ROOT / "kernel/src/security.rs").read_text()
BUILD = (ROOT / "kernel/build.rs").read_text()
RUNTIME = (ROOT / "scripts/boot_test_aarch64.py").read_text()


def require(source: str, fragment: str) -> None:
    if fragment not in source:
        raise AssertionError(f"missing self-hosting guard: {fragment}")


for fragment in (
    '"mov x0, #42\\n"',
    '"mov x8, #5\\n"',
    '"svc #0\\n"',
    "static size_t assemble(",
    "EM_AARCH64",
    "PF_R | PF_X",
    "deliberately NX",
    "PROT_READ | PROT_WRITE | PROT_EXEC",
    "MAKOS_AARCH64_ASSEMBLER_OK",
    "/home/user/generated.s",
    "/home/user/generated-aarch64.elf",
):
    require(TOOLCHAIN, fragment)

for fragment in (
    "fn validate_static_process_image",
    "file_end > bytes.len() as u64",
    "segment.flags & 3 == 3",
    "crate::vfs::snapshot(path, &mut image)?",
    "pub fn spawn_path(path: &[u8])",
    "startup=sysv-default",
):
    require(PROCESS, fragment)

for fragment in (
    "const SYS_PROCESS_SPAWN_PATH: u64 = 56;",
    "const ABI_FEATURE_SELF_HOSTING_SEED: u64 = 1 << 14;",
    "const ABI_FEATURE_EXEC_BY_PATH: u64 = 1 << 18;",
    "crate::aarch64_process::spawn_path(path)",
    "16 => crate::aarch64_process::spawn_toolchain()",
):
    require(ARCH, fragment)

require(BUILD, "../user/aarch64_toolchain.c")
require(SHELL, "MAKOS_AARCH64_SELFHOST_SEED_OK")
require(SHELL, "SYS_PROCESS_SPAWN_PATH")
require(PROCESS, "SessionProcessRole::Toolchain")
require(SECURITY, "SessionProcessRole::Toolchain => CAP_CONSOLE | CAP_FILE_WRITE")
require(RUNTIME, 'send_command(stream, "selfhost-aarch64")')
require(RUNTIME, "MAKOS_AARCH64_SELFHOST_SEED_OK")

print("AArch64 guest self-hosting structural test passed")
