#!/usr/bin/env python3
"""Structural guard for bounded Firefox surface-key scheduling priority."""

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PROCESS = (ROOT / "kernel/src/aarch64_process.rs").read_text()
INPUT = (ROOT / "kernel/src/aarch64_virtio_input.rs").read_text()
ARCH = (ROOT / "kernel/src/arch/aarch64.rs").read_text()
SECURITY = (ROOT / "kernel/src/security.rs").read_text()
GRAPHICS = (ROOT / "kernel/src/graphics.rs").read_text()
PTHREAD_PROBE = (ROOT / "ports/musl/pthread-probe.c").read_text()
PRODUCTION_RUNTIME = (ROOT / "scripts/boot_test_aarch64_production_smp.py").read_text()


def function_body(source: str, signature: str) -> str:
    start = source.index(signature)
    brace = source.index("{", start)
    depth = 0
    for index in range(brace, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return source[brace + 1 : index]
    raise AssertionError(f"unterminated function: {signature}")


route = function_body(INPUT, "fn route_surface_key")
poll = function_body(INPUT, "pub fn poll()")
prioritize = function_body(PROCESS, "pub(crate) fn prioritize_firefox_surface_thread")
handoff = function_body(PROCESS, "pub(crate) fn arm_firefox_process_leader_handoff")
set_priority = function_body(PROCESS, "fn set_surface_priority")
active_priority = function_body(PROCESS, "fn active_surface_priority_tid")
priority_affinity = function_body(PROCESS, "fn surface_priority_cpu_eligible")
input_block = function_body(PROCESS, "pub(crate) fn block_current_for_input")
futex_wake = function_body(PROCESS, "fn wake_futex_in_state")
activate_wakes = function_body(PROCESS, "fn activate_futex_wakes")
dispatch = function_body(ARCH, "fn handle_svc")
timer_irq = function_body(ARCH, "fn handle_irq")
select = function_body(PROCESS, "fn select_surface_priority")
schedule = function_body(PROCESS, "fn schedule_from_exception")
secondary = function_body(PROCESS, "pub(crate) fn run_secondary_scheduler")

assert "SURFACE_KEY_QUEUED.store(true" in route
assert "if routed" in route
assert "SURFACE_KEY_QUEUED.swap(false" in poll
assert "prioritize_firefox_surface_thread" in poll
assert "slot.role == ProcessRole::Firefox" in prioritize
assert "slot.input_wait" in prioritize
assert prioritize.index("slot.input_wait") < prioritize.index("slot.pid == slot.group_pid")
assert "FIREFOX_INPUT_WATCHER_TID.load" in prioritize
assert prioritize.index("FIREFOX_INPUT_WATCHER_TID.load") < prioritize.index(
    "slot.pid == slot.group_pid"
)
assert ".or_else(||" in prioritize
assert "set_surface_priority(tid)" in prioritize
assert "saturating_add(SURFACE_PRIORITY_TICKS)" in set_priority
assert "now > deadline" in active_priority
assert "slot.role == ProcessRole::Firefox" in priority_affinity
assert "slot.affinity_mask & (1u8 << cpu) != 0" in priority_affinity
assert "FIREFOX_INPUT_WATCHER_BLOCK_REPORTED" in input_block
assert "MAKOS_AARCH64_FIREFOX_INPUT_WATCHER_BLOCKED_OK" in input_block
assert "SURFACE_MAIN_HANDOFF_PENDING.store(true" in handoff
assert "SURFACE_PRIORITY_TID.store(0" in handoff
assert "slot.pid == slot.group_pid" in handoff
assert "set_surface_priority(leader)" in handoff
assert "source=watcher-dequeue-fallback" in handoff
assert "activate_futex_wakes(state" in futex_wake
assert "slot.pid == slot.group_pid" in activate_wakes
assert "SURFACE_MAIN_HANDOFF_PENDING.swap(false" in activate_wakes
assert "set_surface_priority(task.thread)" in activate_wakes
assert "MAKOS_AARCH64_SURFACE_MAIN_HANDOFF_OK" in activate_wakes
assert "const SURFACE_PRIORITY_TICKS: u64 = 1_000;" in PROCESS
assert "FIREFOX_INPUT_WATCHER_TID.store(tid" in PROCESS
wait_event = dispatch[dispatch.index("SYS_SURFACE_WAIT_EVENT =>") :]
wait_event = wait_event[: wait_event.index("SYS_SURFACE_BLIT =>")]
assert "if event.kind == 1" in wait_event
assert "arm_firefox_process_leader_handoff" in wait_event
assert "service_input_on_owner_cpu();" in timer_irq
assert timer_irq.index("service_input_on_owner_cpu();") < timer_irq.index(
    "crate::aarch64_process::preempt_from_timer(frame);"
)
assert "ProcessState::Ready" in select and "state.table.activate_on(cpu, tid)" in select
assert "ProcessState::Running if tid == prior_pid => Some(info)" in select
assert "ProcessState::Blocked => None" in select
assert "ProcessState::Running => None" in select
assert "surface_priority_cpu_eligible(slot, cpu)" in select
assert "select_surface_priority(state, prior_pid, cpu)" in schedule
assert ".or_else(|| state.schedule_next_for_cpu(cpu))" in schedule
assert "select_surface_priority(state, prior_pid, cpu)" in secondary
assert "report_surface_priority_dispatch(pid, cpu, pid == group_pid)" in secondary
assert "MAKOS_AARCH64_SURFACE_PRIORITY_AP_OK" in PROCESS
assert "MAKOS_AARCH64_SURFACE_MAIN_DISPATCH_OK" in PROCESS
assert "compare_exchange(tid, 0, Ordering::AcqRel" in PROCESS
assert "yields can outpace timer ticks" in PROCESS
assert INPUT.count("route_surface_key(") == 5  # helper plus 4 keyboard routes
for token in (
    "production_input_watcher",
    "makos_call4(140, production_input_surface",
    "event.key != 132",
    "MAKOS_FIREFOX_SMP_INPUT_PRIORITY_OK",
):
    assert token in PTHREAD_PROBE, token
assert "SessionProcessRole::FirefoxProbe" in PROCESS
assert "SessionProcessRole::FirefoxProbe" in SECURITY
assert "CAP_SERVICE_PUBLISH" in SECURITY
assert "const MAX_SURFACES: usize = 7;" in GRAPHICS
assert "const DESKTOP_SURFACES: usize = 6;" in GRAPHICS
assert "makos_call4(8, 96, 64, 7, 0)" in PTHREAD_PROBE
for token in (
    "WATCHER_BLOCKED_MARKER",
    'common.send_key(stream, "ctrl-a")',
    "WATCHER_AP_MARKER",
    "LEADER_DISPATCH_MARKER",
    "input_priority=watcher-ap,leader-cpu0",
):
    assert token in PRODUCTION_RUNTIME, token

print(
    "MAKOS_AARCH64_SURFACE_PRIORITY_TEST_OK "
    "trigger=queued-key target=firefox-input-watcher blocked,active=retain-hint fallback=process-leader "
    "ready=next-schedule boost=one-shot,stale-deadline handoff=watcher-ap,leader-cpu0,futex-refresh "
    "timer_poll=100hz owner=cpu0 expiry=ticks "
    "fallback=round-robin pointer=unchanged"
)
