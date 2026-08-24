#!/usr/bin/env python3
"""Structural guard for AArch64 robust-futex owner-death handling."""

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PROCESS = (ROOT / "kernel/src/aarch64_process.rs").read_text()
DISPATCH = (ROOT / "kernel/src/arch/aarch64.rs").read_text()
PATCH = (ROOT / "ports/musl/patches/0063-makos-robust-futex.patch").read_text()
APPLY = (ROOT / "ports/musl/apply-patches.sh").read_text()
PROBE = (ROOT / "ports/musl/pthread-probe.c").read_text()
HARNESS = (ROOT / "scripts/boot_test_aarch64.py").read_text()

for fragment in (
    "robust_list_head: u64",
    "robust_list_length: u64",
    "pub fn set_robust_list",
    "pub fn get_robust_list",
    "fn robust_list_on_current_exit",
    "fn robust_lists_on_group_exit",
    "MAX_ROBUST_NODES: usize = 2048",
    "FUTEX_WAITERS: u32 = 0x8000_0000",
    "FUTEX_OWNER_DIED: u32 = 0x4000_0000",
    "wake_futex_in_state(state, FutexKey::new(root, address), 1)",
):
    assert fragment in PROCESS

assert "SYS_ROBUST_LIST: u64 = 141" in DISPATCH
for fragment in (
    "L_set_robust_list = 99",
    "L_get_robust_list = 100",
    "M_robust_list = 141",
    "case L_set_robust_list:",
    "case L_get_robust_list:",
):
    assert fragment in PATCH
assert "patches/0063-makos-robust-futex.patch" in APPLY
assert APPLY.count("patches=64") == 4

for fragment in (
    "pthread_mutexattr_setrobust(&robust_attribute, PTHREAD_MUTEX_ROBUST)",
    "syscall(SYS_set_robust_list, &robust_head, sizeof robust_head)",
    "syscall(SYS_futex, &robust_node.futex, 0, robust_observed, 0, 0, 0)",
    "robust_node.futex != 0xc0000000U",
    "robust=owner-death,wake-one",
):
    assert fragment in PROBE
assert "robust=owner-death,wake-one" in HARNESS

print(
    "MAKOS_AARCH64_ROBUST_FUTEX_TEST_OK "
    "musl=patch63 register=query owner_death=waiters-preserved wake=one "
    "exit=thread,group bounded_nodes=2048"
)
