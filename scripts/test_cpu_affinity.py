#!/usr/bin/env python3
"""Structural guard for the native CPU-affinity ABI and Firefox runtime proof."""

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PROCESS = (ROOT / "kernel/src/aarch64_process.rs").read_text()
AARCH64 = (ROOT / "kernel/src/arch/aarch64.rs").read_text()
X86 = (ROOT / "kernel/src/syscall.rs").read_text()
MUSL = (ROOT / "ports/musl/patches/0065-makos-cpu-affinity.patch").read_text()
PROBE = (ROOT / "ports/musl/pthread-probe.c").read_text()
RUNTIME = (ROOT / "scripts/boot_test_aarch64_production_smp.py").read_text()
SDK = (ROOT / "sdk/include/makos.h").read_text()
DOC = (ROOT / "docs/SYSCALLS.md").read_text()


def require(source: str, fragment: str) -> None:
    if fragment not in source:
        raise AssertionError(f"missing CPU-affinity invariant: {fragment}")


for fragment in (
    "affinity_mask: u8",
    "automatic_cpu: u8",
    "affinity_user_set: bool",
    "pub fn task_affinity(tid: u64)",
    "pub fn set_task_affinity(tid: u64, mask: u64)",
    "mask & !ONLINE_CPU_MASK",
    "slot.affinity_mask & (1u8 << cpu)",
    "ProcessRole::Firefox | ProcessRole::Native | ProcessRole::Python",
    "notify_idle_cpus();",
):
    require(PROCESS, fragment)
for source in (AARCH64, X86):
    require(source, "SYS_THREAD_AFFINITY")
    require(source, "= 148;")
    require(source, "ABI_FEATURE_CPU_AFFINITY: u64 = 1 << 22")
require(AARCH64, "yield_from_exception(frame)")
require(X86, "thread_in_current_process(registers.rsi)")
require(MUSL, "L_sched_setaffinity = 122")
require(MUSL, "M_thread_affinity = 148")
require(MUSL, "memcpy((void *)c, &mask, sizeof mask)")
require(PROBE, "sched_setaffinity(0, sizeof requested, &requested)")
require(PROBE, "sched_getaffinity(0, sizeof observed, &observed)")
require(
    PROBE,
    "singleton=0x2,0x4,0x8 restored=0xe get=kernel-owned placement=least-reserved-ap",
)
require(PROBE, "migrations=automatic:load,forced:3 caller_selected_automatic=0")
require(RUNTIME, "expected_affinity_gets = {(0x1, 0), (0x2, 1), (0x4, 2), (0x8, 3)}")
require(RUNTIME, "if len(forced_migrations) < 3")
require(RUNTIME, "AUTOMATIC_MIGRATION_MARKER")
require(RUNTIME, "automatic_migrations < 1")
require(SDK, "MAKOS_FEATURE_CPU_AFFINITY")
require(SDK, "makos_thread_set_affinity")
require(DOC, "`thread_affinity`")

print(
    "MAKOS_CPU_AFFINITY_TEST_OK abi=148 masks=kernel-owned "
    "validation=online,same-process migration=automatic-load,forced "
    "placement=least-reserved-ap runtime=firefox,native,python-musl"
)
