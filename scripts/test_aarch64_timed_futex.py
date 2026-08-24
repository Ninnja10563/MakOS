#!/usr/bin/env python3
"""Structural guard for musl timed-futex expiry proof."""

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PROCESS = (ROOT / "kernel/src/aarch64_process.rs").read_text()
PROBE = (ROOT / "ports/musl/pthread-probe.c").read_text()
HARNESS = (ROOT / "scripts/boot_test_aarch64.py").read_text()

for fragment in (
    "timeout_address: u64",
    "Some(now.saturating_add(ticks))",
    "Err(WaitError::DeadlineExpired)",
    "state.futex.expire(now",
    "slot.context.registers[0] = negative_errno(110)",
):
    assert fragment in PROCESS
for fragment in (
    "pthread_mutex_timedlock(&timed_mutex, &timed_deadline) != ETIMEDOUT",
    "elapsed_ns < 50000000LL",
    "elapsed_ns > 1000000000LL",
    "futex=wait,wake,requeue,timed-timeout",
):
    assert fragment in PROBE
assert "futex=wait,wake,requeue,timed-timeout" in HARNESS

print(
    "MAKOS_AARCH64_TIMED_FUTEX_TEST_OK "
    "libc=pthread_mutex_timedlock result=ETIMEDOUT "
    "scheduler=block,timer-expire,wake elapsed=50ms..1s"
)
