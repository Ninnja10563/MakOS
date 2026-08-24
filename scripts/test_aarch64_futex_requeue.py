#!/usr/bin/env python3
"""Structural guard for AArch64 futex requeue and musl condvar relay proof."""

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CORE = (ROOT / "crates/futex/src/lib.rs").read_text()
PROCESS = (ROOT / "kernel/src/aarch64_process.rs").read_text()
ARCH = (ROOT / "kernel/src/arch/aarch64.rs").read_text()
PROBE = (ROOT / "ports/musl/pthread-probe.c").read_text()
HARNESS = (ROOT / "scripts/boot_test_aarch64.py").read_text()

assert "pub enum RequeueError" in CORE
assert "pub fn requeue(" in CORE
assert "self.slots[index].key = target" in CORE
assert "requeue_preserves_global_fifo_and_deadlines" in CORE
assert "FUTEX_REQUEUE: u32 = 3" in PROCESS
assert "FUTEX_CMP_REQUEUE: u32 = 4" in PROCESS
assert "requeue_futex_in_state(" in PROCESS
assert "timeout_address as usize" in PROCESS
assert "compare_value" in PROCESS
assert "frame.registers[4]" in ARCH
assert "frame.registers[5] as u32" in ARCH
assert "pthread_cond_broadcast(&requeue_condition)" in PROBE
assert "requeue_completed != 3" in PROBE
marker = (
    "MAKOS_AARCH64_FUTEX_REQUEUE_OK libc=pthread_cond_broadcast "
    "waiters=3 wake=relay requeue=mutex fifo=1 joins=bounded"
)
assert "MAKOS_AARCH64_FUTEX_REQUEUE_OK libc=pthread_cond_broadcast " in PROBE
assert "waiters=3 wake=relay requeue=mutex fifo=1 joins=bounded" in PROBE
assert marker in HARNESS

print(
    "MAKOS_AARCH64_FUTEX_REQUEUE_TEST_OK "
    "core=fifo,handles,deadlines kernel=wake,requeue,cmp "
    "libc=pthread_cond_broadcast waiters=3"
)
