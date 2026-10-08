#!/usr/bin/env python3
"""Host-execute production copied-TX dispatch and socket/epoll-lock progress.

Only hardware events/counters, the low-level packet transport and immutable
guest-probe observations are adapted. This does not execute virtio or qualify
Mac/HVF. All queue state, owner/safety checks, timeout and socket/epoll lock
code are extracted; negative controls must detect missing safeguards. The
epoll block-owner hook has structural coverage only in this network fixture.
"""

from __future__ import annotations

from pathlib import Path
import re
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[1]
NET = (ROOT / "kernel/src/aarch64_virtio_net.rs").read_text()
SOCKET = (ROOT / "kernel/src/aarch64_socket.rs").read_text()
EPOLL = (ROOT / "kernel/src/aarch64_epoll.rs").read_text()
ARCH = (ROOT / "kernel/src/arch/aarch64.rs").read_text()


def item(source: str, declaration: str) -> str:
    start = source.index(declaration)
    opening = source.index("{", start)
    depth = 1
    end = opening + 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[start:end]


constants = "\n".join(re.findall(r"^const TX_\w+:.*;$", NET, re.M))
atomics = "\n".join(re.findall(r"^static (?:TX_|TCP_TX_)\w+:.*;$", NET, re.M))
queue_types = NET[
    NET.index("#[derive(Clone, Copy)]\nstruct TxServiceRequest"):
    NET.index("#[derive(Clone, Copy, Debug, Eq, PartialEq)]\npub struct NetConfig")
]
connections = "\n".join(
    "#[derive(Clone, Copy)]\n" + item(NET, declaration)
    for declaration in ("pub struct TcpConnection", "pub struct Tcp6Connection")
)
functions = "\n".join(item(NET, declaration) for declaration in (
    "fn queue_tx_request(",
    "fn report_tx_timeout(",
    "pub fn service_tx_requests()",
    "pub fn service_tx_requests_from_timer()",
    "pub fn service_tx_requests_while_waiting()",
    "fn service_tx_requests_inner(",
    "pub fn reset_tx_affinity_evidence()",
    "pub fn progress_evidence()",
    "pub fn tcp_tx_affinity_evidence()",
    "pub fn tx_affinity_evidence()",
    "pub fn tx_request_publication_pending()",
    "fn tcp4_service_result(",
    "fn tcp6_service_result(",
))
socket_lock = item(SOCKET, "fn with_state<R>")
epoll_lock = item(EPOLL, "fn with_table<R>")
progress_call = "crate::aarch64_virtio_net::service_tx_requests_while_waiting();"
for lock in (socket_lock, epoll_lock):
    assert progress_call in lock
    assert "compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)" in lock
    assert lock.index(progress_call) < lock.index("let result = function(")
    assert "STATE.lock.store(false, Ordering::Release)" in lock
assert "crate::aarch64_virtio_blk::service_requests_while_waiting();" in epoll_lock
# Keep the actual transitive lock dependency visible: production epoll table
# callbacks inspect socket ownership/readiness while the table lock is held.
assert "with_table(" in item(EPOLL, "pub(crate) fn control(")
assert "valid_target," in item(EPOLL, "pub(crate) fn control(")
assert "target_readiness)" in item(EPOLL, "pub(crate) fn collect(")
assert "crate::aarch64_socket::is_owned(" in item(EPOLL, "fn valid_target(")
assert "crate::aarch64_socket::poll_events(" in item(EPOLL, "fn target_readiness(")
assert "counter_deadline_millis(5_000)" in functions
assert "wait_for_tx_event();" in functions and "notify_tx_waiters();" in functions
assert '"dsb ish", "sev"' in item(NET, "fn notify_tx_waiters()")
assert '"wfe"' in item(NET, "fn wait_for_tx_event()")
diagnostic = item(NET, "fn report_tx_timeout(")
for field in ("cpu", "slot", "kind", "state", "device_locked", "owner_active",
              "requests", "completions", "timer_completions", "lock_wait_completions"):
    assert f"{field}={{}}" in diagnostic, field
assert "request.payload" not in diagnostic
assert "aarch64_process" not in diagnostic and "aarch64_socket" not in diagnostic
dispatcher = item(NET, "fn service_tx_requests_inner(")
assert "aarch64_process" not in dispatcher and "aarch64_socket" not in dispatcher
assert "TX_OWNER_ACTIVE.store(false, Ordering::Release)" in dispatcher

# The same low-level service must be reachable on an EL1 timer, not only the
# lower-EL RX path which cannot break a kernel-mode owner dependency.
irq = item(ARCH, "fn handle_irq(")
timer_tx = irq.index("crate::aarch64_virtio_net::service_tx_requests_from_timer();")
timer_block = irq.rindex("if timer {", 0, timer_tx)
assert "if kind == 9" not in irq[timer_block:timer_tx]
assert "if kind == 9" in irq[timer_tx:]

queue = (
    "use core::cell::UnsafeCell;\n"
    "use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};\n"
    + constants + "\n" + atomics + "\n" + queue_types + "\n"
    + connections + "\n" + functions
)


def replace_once(source: str, original: str, replacement: str) -> str:
    assert source.count(original) == 1, original
    return source.replace(original, replacement, 1)


def regex_once(source: str, pattern: str, replacement: str) -> str:
    rewritten, count = re.subn(pattern, replacement, source, count=1)
    assert count == 1, pattern
    return rewritten


with tempfile.TemporaryDirectory(prefix="makos-net-owner-") as directory:
    output = Path(directory)
    fixture = output / "test.rs"
    fixture.write_text((ROOT / "scripts/test_aarch64_net_owner.rs").read_text())

    def build(name: str, queue_source: str, lock_source: str,
              epoll_source: str = epoll_lock) -> Path:
        (output / "net_queue.rs").write_text(queue_source)
        (output / "socket_lock.rs").write_text(lock_source)
        (output / "epoll_lock.rs").write_text(epoll_source)
        binary = output / name
        subprocess.run([
            "rustc", "--edition=2024", "--test", str(fixture), "-o", str(binary),
        ], check=True)
        return binary

    binary = build("net-owner-test", queue, socket_lock)
    subprocess.run([str(binary), "--test-threads=1"], check=True, timeout=40)

    owner_guard = '''if crate::arch::cpu_index() != 0 {
        crate::fatal("AArch64 network TX service attempted from non-owner CPU");
    }'''
    negative_cases = (
        (
            "without-socket-progress", queue,
            replace_once(socket_lock, progress_call, ""),
            epoll_lock, "socket_contention_completes_ap_tx_without_timer",
            "CPU0/AP socket-TX circular wait",
        ),
        (
            "without-epoll-progress", queue, socket_lock,
            replace_once(epoll_lock, progress_call, ""),
            "epoll_socket_chain_completes_ap_tx_without_timer",
            "CPU0/AP epoll-socket-TX circular wait",
        ),
        (
            "without-device-deferral",
            regex_once(queue, r"STATE\.lock\.load\(Ordering::Acquire\)\s*\|\|\s*", ""),
            socket_lock, epoll_lock, "locked_network_device_defers_without_consuming_request",
            "device lock entered recursively",
        ),
        (
            "without-active-service-deferral",
            regex_once(queue, r"TX_OWNER_ACTIVE\s*\.compare_exchange\(false, true, Ordering::AcqRel, Ordering::Acquire\)\s*\.is_err\(\)", "false"),
            socket_lock, epoll_lock, "active_service_defers_between_device_acquisitions",
            "recursive TX service must defer",
        ),
        (
            "without-owner-guard", replace_once(queue, owner_guard, ""),
            socket_lock, epoll_lock, "nonowner_service_is_rejected_without_device_access",
            "non-owner direct service was accepted",
        ),
        (
            "without-payload-bound",
            replace_once(queue, "length > TX_SERVICE_PAYLOAD", "false"),
            socket_lock, epoll_lock, "invalid_request_length_completes_failed_without_transport",
            "out of range",
        ),
    )
    for name, source, lock, epoll_source, test, expected_failure in negative_cases:
        negative = build(name, source, lock, epoll_source)
        failed = subprocess.run(
            [str(negative), "--exact", test], capture_output=True, text=True, timeout=12,
        )
        if failed.returncode == 0 or expected_failure not in failed.stdout + failed.stderr:
            raise SystemExit(
                f"network owner negative control {name} did not fail as required:\n"
                f"{failed.stdout}{failed.stderr}"
            )

print("MAKOS_AARCH64_NET_OWNER_HOST_OK locks=production-socket,epoll queue=production-tx "
      "requests=udp4,udp6,tcp4-connect,tcp6-connect,tcp4-segment,tcp6-segment "
      "owner=cpu0 timeout_ms=5000 timer=el1-safe guards=device-lock,active-service "
      "negative_controls=6 block_hook=structural-only runtime=not-claimed")
