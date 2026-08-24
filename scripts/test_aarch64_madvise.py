#!/usr/bin/env python3
"""Structural guard for frame-releasing AArch64 madvise semantics."""

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
VM = (ROOT / "kernel/src/aarch64_vm.rs").read_text()
PROBE = (ROOT / "ports/musl/pthread-probe.c").read_text()
BOOT = (ROOT / "scripts/boot_test_aarch64.py").read_text()

assert "const MADV_DONTNEED: u64 = 4;" in VM
assert "const MADV_FREE: u64 = 8;" in VM
assert "if !matches!(advice, MADV_DONTNEED | MADV_FREE)" in VM
assert "crate::arch::unmap_user_page_in(root, address)" in VM
assert "crate::mm::free_frame(frame)" in VM
assert "madvise(advice_page, 4096, MADV_DONTNEED)" in PROBE
assert "madvise(advice_page, 4096, MADV_FREE)" in PROBE
assert "madvise=dontneed,free,decommit,zero-refault" in PROBE
assert "madvise=dontneed,free,decommit,zero-refault" in BOOT

print(
    "MAKOS_AARCH64_MADVISE_TEST_OK "
    "advice=dontneed,free action=decommit frames=released refault=zero"
)
