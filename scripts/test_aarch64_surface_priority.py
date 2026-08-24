#!/usr/bin/env python3
"""Structural guard for bounded Firefox surface-key scheduling priority."""

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PROCESS = (ROOT / "kernel/src/aarch64_process.rs").read_text()
INPUT = (ROOT / "kernel/src/aarch64_virtio_input.rs").read_text()
ARCH = (ROOT / "kernel/src/arch/aarch64.rs").read_text()


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
futex_wake = function_body(PROCESS, "fn wake_futex_in_state")
activate_wakes = function_body(PROCESS, "fn activate_futex_wakes")
dispatch = function_body(ARCH, "fn handle_svc")
timer_irq = function_body(ARCH, "fn handle_irq")
select = function_body(PROCESS, "fn select_surface_priority")
schedule = function_body(PROCESS, "fn schedule_from_exception")

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
assert "crate::aarch64_virtio_input::poll();" in timer_irq
assert timer_irq.index("crate::aarch64_virtio_input::poll();") < timer_irq.index(
    "crate::aarch64_process::preempt_from_timer(frame);"
)
assert "ProcessState::Ready" in select and "table.activate_on(scheduler_cpu(), tid)" in select
assert "ProcessState::Running if tid == prior_pid => Some(info)" in select
assert "ProcessState::Blocked => None" in select
assert "now > deadline" in select
assert "select_surface_priority(state, prior_pid)" in schedule
assert ".or_else(|| state.schedule_next_for_cpu(cpu))" in schedule
assert INPUT.count("route_surface_key(") == 5  # helper plus 4 keyboard routes

print(
    "MAKOS_AARCH64_SURFACE_PRIORITY_TEST_OK "
    "trigger=queued-key target=firefox-input-watcher blocked,active=retain-hint fallback=process-leader "
    "ready=next-schedule boost=bounded-window handoff=watcher-dequeue-fallback,futex-refresh "
    "timer_poll=100hz expiry=ticks "
    "fallback=round-robin pointer=unchanged"
)
