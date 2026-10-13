#!/usr/bin/env python3
"""Test the production scheduler FIFO lock and its interrupt-mask wrapper.

Host threads/adapters exercise real lock and process-table code. This does not
claim guest dispatch fairness when a hypervisor never schedules a peer vCPU.
"""

from pathlib import Path
import hashlib
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
LOCK = (ROOT / "kernel/src/ticket_lock.rs").read_text()
PROCESS = (ROOT / "kernel/src/aarch64_process.rs").read_text()


def item(source: str, marker: str) -> str:
    start = source.index(marker)
    opening = source.index("{", start)
    depth = 1
    end = opening + 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[start:end]


wrapper = item(PROCESS, "fn with_state<R>(")
ordered = [
    "crate::arch::LocalInterruptMask::acquire()",
    "core::sync::atomic::compiler_fence(Ordering::SeqCst)",
    "PROCESSES.lock.lock()",
    "function(unsafe { &mut *PROCESSES.state.get() })",
    "drop(guard)",
    "core::sync::atomic::compiler_fence(Ordering::SeqCst)",
    "drop(interrupts)",
]
position = 0
for token in ordered:
    position = wrapper.index(token, position) + len(token)
assert "lock: crate::ticket_lock::TicketLock" in PROCESS
assert "lock: crate::ticket_lock::TicketLock::new()" in PROCESS
assert "self.next.fetch_add(1, Ordering::Relaxed)" in LOCK
assert "self.serving.load(Ordering::Acquire) == ticket" in LOCK
assert ".store(self.ticket.wrapping_add(1), Ordering::Release)" in LOCK

# The repair must not modify the boot workload, bounds, release rendezvous or
# evidence accounting in order to manufacture balancing evidence.
load = item(PROCESS, "pub fn run_smp_load_balancing_self_test()")
assert hashlib.sha256((load + "\n").encode()).hexdigest() == (
    "a08836a19f78e2a1b4b013c8477e70a38de14328ffbcb7abf2b31a2fd7a1ca60"
), "boot proof must remain byte-identical to fc9338bbef4c"
for assertion in (
    "|| min_dispatches < 32", "|| total_dispatches < 288",
    "|| max_dispatches > min_dispatches.saturating_mul(3)",
    "|| !free_balance", "statuses != [80, 81, 82, 83, 84, 85]",
    "counter_deadline_millis(2_000)", "counter_deadline_millis(20_000)",
):
    assert assertion in load, assertion
workload = (ROOT / "user/aarch64_smp_load_probe.S").read_bytes()
assert hashlib.sha256(workload).hexdigest() == (
    "452bbb9435972e043860c278a321b189005d426e5b334447b7033f2d0f0d350b"
), "six-task guest workload must remain byte-identical to fc9338bbef4c"

# Test-only inspection calls the production ticket reservation/admission
# methods, including boundary values, without replacing their implementation.
inspection = r"""
#[cfg(test)]
impl TicketLock {
    pub(crate) fn test_reserve(&self) -> u64 { self.reserve_ticket() }
    pub(crate) fn test_has_turn(&self, ticket: u64) -> bool { self.has_turn(ticket) }
    pub(crate) fn test_enter(&self, ticket: u64) -> TicketGuard<'_> {
        assert!(self.has_turn(ticket), "only the oldest ticket may enter");
        TicketGuard { lock: self, ticket }
    }
    pub(crate) fn test_at(ticket: u64) -> Self {
        Self { next: AtomicU64::new(ticket), serving: AtomicU64::new(ticket) }
    }
    pub(crate) fn test_counters(&self) -> (u64, u64) {
        (self.next.load(Ordering::Acquire), self.serving.load(Ordering::Acquire))
    }
}
"""

with tempfile.TemporaryDirectory(prefix="makos-scheduler-lock-") as temporary:
    directory = Path(temporary)
    library = directory / "libmakos_process_table.rlib"
    subprocess.run(
        ["rustc", "--edition=2024", "--crate-name", "makos_process_table",
         "--crate-type", "rlib", str(ROOT / "crates/process-table/src/lib.rs"),
         "-o", str(library)], check=True,
    )
    (directory / "test.rs").write_text(
        (ROOT / "scripts/test_aarch64_scheduler_lock.rs").read_text()
    )

    def compile_fixture(name: str, source: str = LOCK, entry: str = wrapper) -> Path:
        (directory / "ticket_production.rs").write_text(source + inspection)
        (directory / "wrapper_production.rs").write_text(entry)
        binary = directory / name
        subprocess.run(
            ["rustc", "--edition=2024", "--test", "-D", "warnings",
             str(directory / "test.rs"), "--extern", f"makos_process_table={library}",
             "-o", str(binary)], check=True,
        )
        return binary

    subprocess.run([str(compile_fixture("scheduler-lock-tests")), "--test-threads=1"],
                   check=True, timeout=60)

    def reject(name: str, test: str, diagnostic: str,
               source: str = LOCK, entry: str = wrapper) -> None:
        result = subprocess.run(
            [str(compile_fixture(name, source, entry)), "--exact", test],
            capture_output=True, text=True, timeout=15,
        )
        output = result.stdout + result.stderr
        if result.returncode == 0 or diagnostic not in output:
            raise SystemExit(f"{name} failed to reject the restored defect:\n{output}")

    # Generated copies only: admit newer tickets before an older queued AP.
    reject("ignore-queue-order", "fifo_reservations_prevent_barging",
           "a newcomer must not pass a queued owner",
           LOCK.replace("self.serving.load(Ordering::Acquire) == ticket",
                        "self.serving.load(Ordering::Acquire) <= ticket"))
    reject("saturating-ticket-release", "ticket_wrap_releases_the_next_owner",
           "ticket release must wrap to the next reservation",
           LOCK.replace("self.ticket.wrapping_add(1)", "self.ticket.saturating_add(1)"))
    reject("interrupt-mask-removed", "scheduler_wrapper_restores_prior_mask",
           "scheduler state must be protected from local interrupt re-entry",
           entry=wrapper.replace("crate::arch::LocalInterruptMask::acquire()", "Box::new(())"))
    early_restore = wrapper.replace("drop(guard);", "drop(interrupts);")
    start = early_restore.index("drop(interrupts);") + len("drop(interrupts);")
    early_restore = early_restore[:start] + early_restore[start:].replace(
        "drop(interrupts);", "drop(guard);", 1
    )
    reject("early-interrupt-restore", "scheduler_wrapper_restores_prior_mask",
           "restore IRQs only after scheduler-lock release", entry=early_restore)

print("MAKOS_AARCH64_SCHEDULER_LOCK_HOST_OK lock=production-fifo "
      "process_table=production-code irq=host-adapter "
      "negative_controls=queue-order,wrap,interrupt-mask,early-restore "
      "scope=queued-contenders-not-host-vcpu-service")
