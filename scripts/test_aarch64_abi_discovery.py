#!/usr/bin/env python3
"""Static companion gate for runtime-tested AArch64 ABI discovery."""

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
KERNEL = (ROOT / "kernel/src/arch/aarch64.rs").read_text()
INIT = (ROOT / "user/aarch64_init.c").read_text()
DOC = (ROOT / "docs/SYSCALLS.md").read_text()


def require(source: str, fragment: str) -> None:
    if fragment not in source:
        raise AssertionError(f"missing AArch64 ABI discovery invariant: {fragment}")


require(KERNEL, "3 => SYS_THREAD_AFFINITY,")
require(KERNEL, "2 => ABI_FEATURES,")
require(KERNEL, "const SYS_LOG_APPEND: u64 = 28;")
require(KERNEL, "const SYS_LOG_READ: u64 = 29;")
require(KERNEL, "SYS_LOG_APPEND => {")
require(KERNEL, "SYS_LOG_READ => {")
for bit in (0, 1, 2, 3, 4, 5, 6, 7, 8, 11, 14, 15, 16, 17, 18, 19, 20, 21, 22):
    require(KERNEL, f"1 << {bit};")
require(INIT, "syscall2(SYS_ABI_INFO, 2, 0) != AARCH64_ABI_FEATURES")
require(INIT, "syscall2(SYS_ABI_INFO, 3, 0) != SYS_THREAD_AFFINITY")
require(INIT, "(UINT64_C(1) << 7)")
require(INIT, "(UINT64_C(1) << 16)")
require(INIT, "(UINT64_C(1) << 18)")
require(INIT, "(UINT64_C(1) << 19)")
require(INIT, "syscall4(SYS_LOG_APPEND")
require(INIT, "syscall4(SYS_LOG_READ")
require(INIT, "MAKOS_AARCH64_LOG_OK structured=1 ring=32 pid=1 severity=5")
require(INIT, "MAKOS_AARCH64_ABI_OK version=1.0 normative_max=57 target_extension_max=148")
require(DOC, "`148` both targets")

print(
    "MAKOS_AARCH64_ABI_DISCOVERY_TEST_OK normative_max=57 "
    "target_extension_max=148 feature_mask=truthful runtime_probe=init"
)
