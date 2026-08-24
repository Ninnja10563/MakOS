#!/usr/bin/env python3
"""Structural guard for process and thread-directed signal runtime proof."""

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TTY = (ROOT / "kernel/src/aarch64_tty.rs").read_text()
DISPATCH = (ROOT / "kernel/src/arch/aarch64.rs").read_text()
PATCH = (ROOT / "ports/musl/patches/0064-makos-directed-signals.patch").read_text()
APPLY = (ROOT / "ports/musl/apply-patches.sh").read_text()
PROBE = (ROOT / "ports/musl/pthread-probe.c").read_text()
HARNESS = (ROOT / "scripts/boot_test_aarch64.py").read_text()

for function in ("pub fn kill(", "pub fn kill_task("):
    assert function in TTY
assert "may_signal_process" in TTY
assert "SYS_SIGNAL: u64 = 142" in DISPATCH
assert "3 => SYS_TYPED_CHANNEL_RECEIVE" in DISPATCH
yield_dispatch = DISPATCH[DISPATCH.index("SYS_YIELD =>") : DISPATCH.index("SYS_EXIT =>")]
assert "finish_signal_delivery(frame);" in yield_dispatch
for fragment in (
    "L_kill = 129",
    "L_tkill = 130",
    "L_tgkill = 131",
    "M_signal = 142",
    "case L_kill:",
    "case L_tkill:",
    "case L_tgkill:",
):
    assert fragment in PATCH
assert "patches/0064-makos-directed-signals.patch" in APPLY
assert APPLY.count("patches=64") == 4
for fragment in (
    "kill(getpid(), SIGWINCH)",
    "pthread_kill(thread, SIGWINCH)",
    "syscall(SYS_tgkill, getpid(), target_signal_tid, SIGWINCH)",
    "winch_tid != target_signal_tid",
    "signals=task-mask,inherit,pselect-atomic,ppoll-atomic,epoll-pwait-atomic,eintr,restore,kill,tkill,tgkill",
):
    assert fragment in PROBE
assert "restore,kill,tkill,tgkill" in HARNESS

print(
    "MAKOS_AARCH64_DIRECTED_SIGNALS_TEST_OK "
    "musl=patch64 process=kill thread=tkill,tgkill permission=credentials "
    "delivery=exact-tid"
)
