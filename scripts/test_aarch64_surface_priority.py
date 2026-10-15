#!/usr/bin/env python3
"""Production-code and structural guards for bounded surface-key priority.

Host adapters provide a mutex, clock and serial sink; selection and hint
lifecycle routines plus ProcessTable are real production code. These tests
do not qualify guest input latency or establish a past failure's interleaving.
"""

from pathlib import Path
import re
import subprocess
import tempfile


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


def function_item(source: str, signature: str) -> str:
    body = function_body(source, signature)
    start = source.index(signature)
    brace = source.index("{", start)
    return source[start:brace + 1] + body + "}"


route = function_body(INPUT, "fn route_surface_key")
poll = function_body(INPUT, "pub fn poll()")
prioritize = function_body(PROCESS, "pub(crate) fn prioritize_firefox_surface_thread")
handoff = function_body(PROCESS, "pub(crate) fn arm_firefox_process_leader_handoff")
handoff_ready = function_body(
    PROCESS, "pub(crate) fn complete_firefox_process_leader_handoff"
)
set_priority = function_body(PROCESS, "fn set_surface_priority")
set_priority_locked = function_body(PROCESS, "fn set_surface_priority_in_state")
active_priority = function_body(PROCESS, "fn active_surface_priority_tid")
priority_affinity = function_body(PROCESS, "fn surface_priority_cpu_eligible")
input_block = function_body(PROCESS, "pub(crate) fn block_current_for_input")
input_wake = function_body(PROCESS, "pub(crate) fn wake_input_waiters")
event_ready = function_body(GRAPHICS, "pub fn event_wait_ready")
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
assert "unwrap_or_else(||" in prioritize
assert "set_surface_priority(tid)" in prioritize
assert "saturating_add(SURFACE_PRIORITY_TICKS)" in set_priority_locked
assert "now > deadline" in active_priority
assert "slot.role == ProcessRole::Firefox" in priority_affinity
assert "slot.affinity_mask & (1u8 << cpu) != 0" in priority_affinity
assert "FIREFOX_INPUT_WATCHER_BLOCK_REPORTED" in input_block
assert "MAKOS_AARCH64_FIREFOX_INPUT_WATCHER_BLOCKED_OK" in input_block
assert "SURFACE_MAIN_HANDOFF_WATCHER_TID.store(watcher" in handoff
assert "SURFACE_MAIN_HANDOFF_PENDING_GROUP.store(group_pid" in handoff
assert "MAKOS_AARCH64_SURFACE_MAIN_HANDOFF_ARM_OK" in handoff
assert "with_state" in handoff and "clear_surface_priority" in handoff
assert "slot.pid == slot.group_pid" in handoff
assert "set_surface_priority(leader)" in handoff
assert "source=watcher-dequeue-fallback" in handoff
assert "SURFACE_MAIN_HANDOFF_WATCHER_TID.load" in handoff_ready
assert "SURFACE_MAIN_HANDOFF_PENDING_GROUP" in handoff_ready
assert ".compare_exchange(group_pid, 0" in handoff_ready
assert "set_surface_priority(leader)" in handoff_ready
assert "notify_idle_cpus();" in handoff_ready
assert "MAKOS_AARCH64_SURFACE_MAIN_HANDOFF_READY_OK" in handoff_ready
assert "MAKOS_AARCH64_SURFACE_MAIN_HANDOFF_READY_REJECT" in handoff_ready
assert "source=watcher-post-enqueue wake=sgi" in handoff_ready
assert "activate_futex_wakes(state" in futex_wake
assert "slot.pid == slot.group_pid" in activate_wakes
assert "SURFACE_MAIN_HANDOFF_PENDING_GROUP.load" in activate_wakes
assert "set_surface_priority_in_state(state, task.thread)" in activate_wakes
assert "MAKOS_AARCH64_SURFACE_MAIN_HANDOFF_OK" in activate_wakes
assert "const SURFACE_PRIORITY_TICKS: u64 = 1_000;" in PROCESS
assert "FIREFOX_INPUT_WATCHER_TID.store(tid" in PROCESS
assert "input_wait_handle: u64" in PROCESS
assert "state.contexts[index].input_wait_handle = surface_handle" in input_block
assert "crate::graphics::event_wait_ready(handle, owner_pid)" in input_wake
assert "slot.group_pid != group_pid" in input_wake
assert "slot.input_wait_handle != handle" in input_wake
assert "surface_woken" in input_wake and "surface_skipped" in input_wake
assert "MAKOS_AARCH64_INPUT_TARGET_WAKE_OK" in input_wake
assert "INPUT_TARGET_WAKE_REPORTED.swap(true" in input_wake
assert "!surface.created" in event_ready
assert "surface.owner_pid != owner_pid" in event_ready
assert "state.surface_event_tails[index] != state.surface_event_heads[index]" in event_ready
assert "crate::graphics::event_wait_ready(*handle, *owner_pid)" in prioritize
assert "MAKOS_AARCH64_FIREFOX_INPUT_TARGET_OK" in prioritize
assert "FIREFOX_INPUT_TARGET_REPORTED.swap(true" in prioritize
wait_event = dispatch[dispatch.index("SYS_SURFACE_WAIT_EVENT =>") :]
wait_event = wait_event[: wait_event.index("SYS_SURFACE_BLIT =>")]
assert "if event.kind == 1" in wait_event
assert "arm_firefox_process_leader_handoff" in wait_event
assert "service_input_on_owner_cpu();" in timer_irq
assert timer_irq.index("service_input_on_owner_cpu();") < timer_irq.index(
    "crate::aarch64_process::preempt_from_timer(frame);"
)
assert "let input = crate::aarch64_virtio_input::owns_interrupt(intid);" in timer_irq
assert "let direct = kind == 9;" in timer_irq
assert "record_input_irq(intid, direct, activity);" in timer_irq
assert "MAKOS_AARCH64_INPUT_IRQ_OK" in ARCH
assert "ProcessState::Ready" in select and "state.table.activate_on(cpu, tid)" in select
assert re.search(
    r"ProcessState::Running\s+if tid == prior_pid && "
    r"surface_priority_cpu_eligible\(slot, cpu\) => Some\(info\)", select
)
assert "ProcessState::Blocked => None" in select
assert "ProcessState::Running => None" in select
assert "surface_priority_cpu_eligible(slot, cpu)" in select
assert "select_surface_priority(state, prior_pid, cpu)" in schedule
assert ".or_else(|| state.schedule_next_for_cpu(cpu))" in schedule
assert "select_surface_priority(state, prior_pid, cpu)" in secondary
assert "report_surface_priority_dispatch(pid, cpu, pid == group_pid)" in secondary
assert "MAKOS_AARCH64_SURFACE_PRIORITY_AP_OK" in PROCESS
assert "MAKOS_AARCH64_SURFACE_MAIN_DISPATCH_OK" in PROCESS
assert "yields can outpace timer ticks" in PROCESS
assert INPUT.count("route_surface_key(") == 5  # helper plus 4 keyboard routes
for token in (
    "production_input_watcher",
    "makos_call4(140, surface",
    "event.key != 132",
    "makos_call(149, 0, 0) != 1",
    "handoff=post-enqueue-syscall:149",
    "MAKOS_FIREFOX_SMP_INPUT_PRIORITY_OK",
):
    assert token in PTHREAD_PROBE, token
assert "SessionProcessRole::FirefoxProbe" in PROCESS
assert "SessionProcessRole::FirefoxProbe" in SECURITY
assert "CAP_SERVICE_PUBLISH" in SECURITY
assert "const MAX_SURFACES: usize = 8;" in GRAPHICS
assert "const DESKTOP_SURFACES: usize = 6;" in GRAPHICS
assert "makos_call4(8, 96, 64, 7, 0)" in PTHREAD_PROBE
assert "makos_call4(8, 96, 64, 8, 0)" in PTHREAD_PROBE
assert "production_input_decoy_surface" in PTHREAD_PROBE
assert "decoy=blocked-until-destroy" in PTHREAD_PROBE
for token in (
    "WATCHER_BLOCKED_MARKER",
    "HANDLE_WAITERS_MARKER",
    "TARGET_SELECTION_MARKER",
    "TARGET_WAKE_MARKER",
    'common.send_key(stream, "ctrl-a")',
    "WATCHER_AP_MARKER",
    "LEADER_DISPATCH_MARKER",
    "HANDOFF_READY_MARKER",
    "handoff=watcher-post-enqueue-syscall:149",
):
    assert token in PRODUCTION_RUNTIME, token


def check_locked_publication(source: str) -> None:
    """Guard the real lock boundary, not merely adapter behavior."""
    setter = function_body(source, "fn set_surface_priority(")
    assert re.fullmatch(
        r"\s*with_state\(\|state\| set_surface_priority_in_state\(state, tid\)\)\s*",
        setter,
    ), "priority publication must use one scheduler-lock transaction"
    assert "static SURFACE_PRIORITY_TID:" not in source
    assert "static SURFACE_PRIORITY_DEADLINE:" not in source
    assert "surface_priority_tid: u64" in source
    assert "surface_priority_deadline: u64" in source
    assert "surface_priority_tid: 0" in source
    assert "surface_priority_deadline: 0" in source
    active = function_item(source, "fn active_surface_priority_tid(")
    assert "state: &mut SchedulerState" in active
    locked_setter = function_item(source, "fn set_surface_priority_in_state(")
    assert "state: &mut SchedulerState" in locked_setter
    report = function_body(source, "fn report_surface_priority_dispatch(")
    assert "with_state" not in report and "clear_surface_priority" not in report
    assert "SURFACE_PRIORITY_TID" not in report and "SURFACE_PRIORITY_DEADLINE" not in report
    assert "surface_priority_tid" not in report and "surface_priority_deadline" not in report
    selected = function_body(source, "fn select_surface_priority(")
    assert "if selected.is_some()" in selected
    assert "clear_surface_priority(state)" in selected
    assert "active_surface_priority_tid(state)" in selected
    schedule_next = function_body(source, "fn schedule_next_for_cpu(")
    assert "active_surface_priority_tid(self)" in schedule_next
    assert "if selected.is_some_and(|info| info.pid == surface_priority_tid)" in schedule_next
    consume = schedule_next.index("clear_surface_priority(self)")
    assert schedule_next.index("self.table.schedule_next_where_on") < consume
    assert consume < schedule_next.index(
        "report_surface_priority_dispatch(surface_priority_tid, cpu, leader)"
    ) < schedule_next.index("self.cpu_dispatches[cpu]")
    assert consume < schedule_next.index("self.cpu_dispatches[cpu]")


check_locked_publication(PROCESS)
# This restored torn-publication shape is rejected structurally: compiling a
# deliberately unlocked replacement with a host mutex would hide the defect.
unlocked = PROCESS.replace(
    "with_state(|state| set_surface_priority_in_state(state, tid))",
    "set_surface_priority_in_state(unlocked_state, tid)",
    1,
)
assert unlocked != PROCESS
try:
    check_locked_publication(unlocked)
except AssertionError as error:
    assert "priority publication must use one scheduler-lock transaction" in str(error)
else:
    raise AssertionError("unlocked publication negative control was accepted")

production_functions = {
    name: function_item(PROCESS, f"fn {name}(")
    for name in (
        "set_surface_priority", "set_surface_priority_in_state",
        "clear_surface_priority", "active_surface_priority_tid",
        "surface_priority_cpu_eligible", "select_surface_priority",
        "report_surface_priority_dispatch",
        "startup_dispatch_allowed", "application_worker_role",
        "production_ap_worker", "production_role_code", "tracked_production_worker",
    )
}
priority_constant = re.search(
    r"^const SURFACE_PRIORITY_TICKS:.*;$", PROCESS, re.MULTILINE
).group(0)
# Extract the complete ordinary selection predicate and its consume branch.
# Later boot-probe/dispatch telemetry is outside this fixture, not replaced
# by a parallel scheduling model. Rebalance and ProcessTable are production.
ordinary_body = function_body(PROCESS, "fn schedule_next_for_cpu(")
ordinary_selection = ordinary_body[:ordinary_body.index(
    "        // Blocking/exit paths can dispatch")]
assert ordinary_selection.count("schedule_next_where_on") == 1
production_functions["ordinary_selection"] = (
    "impl SchedulerState {\n"
    "fn schedule_next_for_cpu(&mut self, cpu: usize) "
    "-> Option<makos_process_table::ProcessInfo> {\n"
    + ordinary_selection
    + "self.consume_ordinary_selection(selected, surface_priority_tid, cpu);\n"
    + "selected\n}\n}\n"
)
production_functions["rebalance"] = (
    "impl SchedulerState {\n"
    + function_item(PROCESS, "fn rebalance_application_on_timer(") + "\n}\n"
)
production_functions["migration_type"] = (
    "#[derive(Clone, Copy)]\n" + function_item(PROCESS, "struct ComputeMigration {")
    + "\n" + function_item(PROCESS, "impl ComputeMigration {")
)
for name in ("APPLICATION_REBALANCE_DISPATCH_DELTA", "MAX_APPLICATION_MIGRATIONS"):
    production_functions[name] = re.search(
        rf"^const {name}:.*;$", PROCESS, re.MULTILINE
    ).group(0)
selection_start = schedule.index("let priority = select_surface_priority(state, prior_pid, cpu);")
selection_end = schedule.index("if migration.is_some()", selection_start)
assert schedule.index("state.rebalance_application_on_timer(cpu, prior_pid)") < selection_start
production_functions["timer_selection"] = (
    "fn select_after_rebalance(state: &mut SchedulerState, prior_pid: u64, cpu: usize) "
    "-> Option<makos_process_table::ProcessInfo> {\n"
    + schedule[selection_start:selection_end] + "next\n}\n"
)
ordinary_consume = function_item(
    ordinary_body,
    "if selected.is_some_and(|info| info.pid == surface_priority_tid)",
)
production_functions["ordinary_consume"] = (
    "impl SchedulerState {\n"
    "fn consume_ordinary_selection(&mut self, "
    "selected: Option<makos_process_table::ProcessInfo>, surface_priority_tid: u64, "
    "cpu: usize) {\n"
    + ordinary_consume + "\n}\n}\n"
)

host_fixture = r'''
use makos_process_table::{ProcessState, ProcessTable};
use std::sync::{Mutex, atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering}};

#[derive(Clone, Copy, PartialEq)]
#[allow(dead_code)] // Production predicate references roles not created in this fixture.
enum ProcessRole { Firefox, Native, Python, SmpProbe, SmpLoad }
#[derive(Clone, Copy)]
struct ContextSlot {
    pid: u64, group_pid: u64, role: ProcessRole, affinity_mask: u8,
    startup_ready: bool, automatic_cpu: u8, automatic_load_baseline: [u64; 3],
    automatic_migrated: bool, affinity_user_set: bool,
}
struct SchedulerState {
    table: ProcessTable<8>,
    contexts: Vec<ContextSlot>,
    surface_priority_tid: u64,
    surface_priority_deadline: u64,
    cpu_dispatches: [u64; 4],
    compute_placement_cursor: u8,
    application_migration_records: [ComputeMigration; MAX_APPLICATION_MIGRATIONS],
    application_migration_count: usize,
}
static STATE: Mutex<Option<SchedulerState>> = Mutex::new(None);
static CLOCK: AtomicU64 = AtomicU64::new(0);
static SERIAL: Mutex<Vec<String>> = Mutex::new(Vec::new());
static SURFACE_PRIORITY_REPORTED: AtomicBool = AtomicBool::new(false);
static SURFACE_PRIORITY_AP_REPORTED: AtomicBool = AtomicBool::new(false);
static SURFACE_MAIN_DISPATCH_REPORTED: AtomicBool = AtomicBool::new(false);
static REMOTE_GROUP_STOP_PID: AtomicU64 = AtomicU64::new(0);
static SMP_PROBE_AFFINITY: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];
static PRODUCTION_WORKER_GROUP_PID: AtomicU64 = AtomicU64::new(0);
static PRODUCTION_WORKER_ROLE: AtomicU8 = AtomicU8::new(0);
static APPLICATION_MIGRATION_EVIDENCE_DROPS: AtomicU64 = AtomicU64::new(0);
static APPLICATION_MIGRATIONS: AtomicU64 = AtomicU64::new(0);
static APPLICATION_MIGRATION_SOURCE_MASK: AtomicU64 = AtomicU64::new(0);
static APPLICATION_MIGRATION_TARGET_MASK: AtomicU64 = AtomicU64::new(0);
mod aarch64_application_balance;
fn with_state<R>(f: impl FnOnce(&mut SchedulerState) -> R) -> R {
    f(STATE.lock().unwrap().as_mut().unwrap())
}
mod arch {
    pub fn monotonic_ticks() -> u64 {
        super::CLOCK.load(std::sync::atomic::Ordering::Acquire)
    }
}
#[macro_export]
macro_rules! serial_println {
    ($($value:tt)*) => {{ crate::SERIAL.lock().unwrap().push(format!($($value)*)); }};
}
include!("priority_production.rs");

fn setup() -> (u64, u64) {
    CLOCK.store(10, Ordering::Release);
    SURFACE_PRIORITY_REPORTED.store(false, Ordering::Release);
    SURFACE_PRIORITY_AP_REPORTED.store(false, Ordering::Release);
    SURFACE_MAIN_DISPATCH_REPORTED.store(false, Ordering::Release);
    SERIAL.lock().unwrap().clear();
    REMOTE_GROUP_STOP_PID.store(0, Ordering::Release);
    for counter in [&APPLICATION_MIGRATIONS, &APPLICATION_MIGRATION_SOURCE_MASK,
                    &APPLICATION_MIGRATION_TARGET_MASK, &APPLICATION_MIGRATION_EVIDENCE_DROPS] {
        counter.store(0, Ordering::Release);
    }
    let mut table = ProcessTable::new();
    let leader = table.spawn(0, 0x1000).unwrap();
    let watcher = table.spawn(leader, 0x2000).unwrap();
    PRODUCTION_WORKER_GROUP_PID.store(leader, Ordering::Release);
    PRODUCTION_WORKER_ROLE.store(1, Ordering::Release);
    *STATE.lock().unwrap() = Some(SchedulerState {
        table,
        contexts: vec![
            ContextSlot { pid: leader, group_pid: leader,
                          role: ProcessRole::Firefox, affinity_mask: 1,
                          startup_ready: true, automatic_cpu: 0,
                          automatic_load_baseline: [0; 3], automatic_migrated: false,
                          affinity_user_set: false },
            ContextSlot { pid: watcher, group_pid: leader,
                          role: ProcessRole::Firefox, affinity_mask: 0xe,
                          startup_ready: true, automatic_cpu: 0,
                          automatic_load_baseline: [0; 3], automatic_migrated: false,
                          affinity_user_set: false },
        ],
        surface_priority_tid: 0, surface_priority_deadline: 0,
        cpu_dispatches: [0; 4], compute_placement_cursor: 1,
        application_migration_records: [ComputeMigration::EMPTY; MAX_APPLICATION_MIGRATIONS],
        application_migration_count: 0,
    });
    (leader, watcher)
}
fn hint() -> (u64, u64) {
    with_state(|s| (s.surface_priority_tid, s.surface_priority_deadline))
}
fn select(prior: u64, cpu: usize) -> Option<u64> {
    with_state(|s| select_surface_priority(s, prior, cpu).map(|info| info.pid))
}

#[test]
fn ready_selection_consumes_once_before_telemetry() {
    let (_, watcher) = setup();
    assert!(set_surface_priority(watcher));
    assert_eq!(hint(), (watcher, 1010));
    assert_eq!(select(0, 2), Some(watcher));
    assert_eq!(hint(), (0, 0), "selected priority must be consumed under lock");
    assert_eq!(select(watcher, 2), None);
    with_state(|s| assert_eq!(s.table.running_cpu(watcher), Some(2)));
}

#[test]
fn ordinary_selection_consumes_only_matching_hint() {
    let (leader, watcher) = setup();
    assert!(set_surface_priority(watcher));
    with_state(|s| {
        let hint = active_surface_priority_tid(s);
        let selected = s.table.schedule_next_where_on(0, |info| info.pid == leader);
        assert_eq!(selected.unwrap().pid, leader);
        s.consume_ordinary_selection(selected, hint, 0);
        assert_eq!(s.surface_priority_tid, watcher,
            "unrelated ordinary selection must retain the hint");
        assert!(SERIAL.lock().unwrap().is_empty(),
            "unrelated ordinary selection must not claim priority evidence");
        let selected = s.table.schedule_next_where_on(1, |info| info.pid == watcher);
        assert_eq!(selected.unwrap().pid, watcher);
        s.consume_ordinary_selection(selected, hint, 1);
        assert_eq!((s.surface_priority_tid, s.surface_priority_deadline), (0, 0),
            "ordinary selection must consume its matching one-shot hint");
    });
    let serial = SERIAL.lock().unwrap();
    assert!(serial.iter().any(|line| line.starts_with(&format!(
        "MAKOS_AARCH64_SURFACE_PRIORITY_AP_OK tid={watcher} cpu=1 "))),
        "ordinary watcher selection must emit its AP dispatch evidence");
    assert!(!serial.iter().any(|line| line.starts_with(
        "MAKOS_AARCH64_SURFACE_MAIN_DISPATCH_OK")));
}

#[test]
fn ordinary_leader_selection_reports_main_once() {
    let (leader, _) = setup();
    for _ in 0..2 {
        assert!(set_surface_priority(leader));
        with_state(|s| {
            let hint = active_surface_priority_tid(s);
            let selected = s.table.schedule_next_where_on(0, |info| info.pid == leader);
            assert_eq!(selected.unwrap().pid, leader);
            s.consume_ordinary_selection(selected, hint, 0);
            assert_eq!((s.surface_priority_tid, s.surface_priority_deadline), (0, 0));
        });
    }
    let serial = SERIAL.lock().unwrap();
    let expected = format!(
        "MAKOS_AARCH64_SURFACE_MAIN_DISPATCH_OK tid={leader} cpu=0 \
source=bounded-handoff affinity=leader-cpu0");
    assert_eq!(serial.iter().filter(|line| *line == &expected).count(), 1,
        "ordinary leader selection must emit exactly one main dispatch record");
    assert!(!serial.iter().any(|line| line.starts_with(
        "MAKOS_AARCH64_SURFACE_PRIORITY_AP_OK")));
}

#[test]
fn old_report_does_not_clobber_same_tid_refresh() {
    let (leader, _) = setup();
    assert!(set_surface_priority(leader));
    assert_eq!(select(0, 0), Some(leader));
    // The AP publishes syscall-149 priority after CPU0's old selection but
    // before CPU0 emits its delayed telemetry. Both publications name TID1.
    CLOCK.store(20, Ordering::Release);
    assert!(set_surface_priority(leader));
    report_surface_priority_dispatch(leader, 0, true);
    assert_eq!(hint(), (leader, 1020),
        "late dispatch report must not erase refreshed same-TID priority");
    assert_eq!(select(leader, 0), Some(leader));
    assert_eq!(hint(), (0, 0));
}

#[test]
fn expiry_clears_pair_and_refresh_survives_old_report() {
    let (leader, watcher) = setup();
    assert!(set_surface_priority(leader));
    CLOCK.store(1010, Ordering::Release);
    assert_eq!(with_state(active_surface_priority_tid), leader);
    CLOCK.store(1011, Ordering::Release);
    assert_eq!(with_state(active_surface_priority_tid), 0);
    assert_eq!(hint(), (0, 0), "expiry must clear the coherent tuple");
    assert!(set_surface_priority(watcher));
    report_surface_priority_dispatch(leader, 0, true);
    assert_eq!(hint(), (watcher, 2011));
    assert_eq!(select(0, 3), Some(watcher));
}

#[test]
fn blocked_priority_remains_until_real_wake() {
    let (_, watcher) = setup();
    with_state(|s| {
        s.table.activate_on(1, watcher).unwrap();
        assert_eq!(s.table.block_current_on(1), Some(watcher));
    });
    assert!(set_surface_priority(watcher));
    assert_eq!(select(0, 2), None);
    assert_eq!(hint(), (watcher, 1010));
    with_state(|s| {
        assert_eq!(s.table.get(watcher).unwrap().state, ProcessState::Blocked);
        assert!(s.table.wake(watcher));
    });
    assert_eq!(select(0, 2), Some(watcher));
}

#[test]
fn foreign_running_owner_retains_hint_until_owner_selects() {
    let (_, watcher) = setup();
    with_state(|s| s.table.activate_on(1, watcher).unwrap());
    assert!(set_surface_priority(watcher));
    assert_eq!(select(0, 2), None);
    assert_eq!(hint(), (watcher, 1010));
    assert_eq!(select(watcher, 1), Some(watcher));
    assert_eq!(hint(), (0, 0));
    with_state(|s| assert_eq!(s.table.running_cpu(watcher), Some(1)));
}

#[test]
fn leader_and_worker_affinity_are_authoritative() {
    let (leader, watcher) = setup();
    assert!(set_surface_priority(leader));
    for cpu in 1..4 {
        assert_eq!(select(0, cpu), None, "leader must remain CPU0-only");
        assert_eq!(hint(), (leader, 1010));
    }
    assert_eq!(select(0, 0), Some(leader));
    assert!(set_surface_priority(watcher));
    assert_eq!(select(leader, 0), None, "worker must remain AP-only");
    with_state(|s| s.contexts[1].affinity_mask = 0x4);
    assert_eq!(select(0, 1), None);
    assert_eq!(select(0, 3), None);
    assert_eq!(select(0, 4), None);
    assert_eq!(select(0, 2), Some(watcher));
}

#[test]
fn invalid_targets_clear_without_dispatch() {
    let (_, watcher) = setup();
    assert!(!set_surface_priority(0));
    assert_eq!(hint(), (0, 0));
    assert!(set_surface_priority(99));
    assert_eq!(select(0, 1), None);
    assert_eq!(hint(), (0, 0));
    with_state(|s| s.contexts[1].role = ProcessRole::Native);
    assert!(set_surface_priority(watcher));
    assert_eq!(select(0, 1), None);
    assert_eq!(hint(), (0, 0));
    with_state(|s| { s.contexts.pop(); });
    assert!(set_surface_priority(watcher));
    assert_eq!(select(0, 1), None);
    assert_eq!(hint(), (0, 0));
}

#[test]
fn zombie_target_cannot_dispatch() {
    let (_, watcher) = setup();
    with_state(|s| {
        s.table.activate_on(1, watcher).unwrap();
        s.table.exit_current_on(1, 42).unwrap();
    });
    assert!(set_surface_priority(watcher));
    assert_eq!(select(0, 1), None);
    assert_eq!(hint(), (0, 0));
}

#[test]
fn zero_tid_does_not_erase_existing_hint_and_deadline_saturates() {
    let (leader, _) = setup();
    CLOCK.store(u64::MAX - 10, Ordering::Release);
    assert!(set_surface_priority(leader));
    assert_eq!(hint(), (leader, u64::MAX));
    assert!(!set_surface_priority(0));
    assert_eq!(hint(), (leader, u64::MAX));
}

#[test]
fn rebalance_running_priority_releases_source_before_unique_target_dispatch() {
    for source in 1..4 {
        let (leader, watcher) = setup();
        assert!(set_surface_priority(watcher));
        with_state(|s| {
            s.contexts[1].automatic_cpu = source as u8;
            s.cpu_dispatches[source] = APPLICATION_REBALANCE_DISPATCH_DELTA;
            s.table.activate_on(source, watcher).unwrap();
            let migration = s.rebalance_application_on_timer(source, watcher).unwrap();
            let target = migration.target_cpu;
            assert_ne!(target, source);
            assert_eq!(migration.pid, watcher);
            assert_eq!(migration.group_pid, leader);
            assert_eq!(migration.application_role, 1);
            assert_eq!(migration.source_cpu, source);
            assert_eq!(migration.loads[source - 1], APPLICATION_REBALANCE_DISPATCH_DELTA);
            assert_ne!(migration.idle_mask & (1 << target), 0);
            assert_eq!(s.contexts[1].automatic_cpu, target as u8);
            assert!(s.contexts[1].automatic_migrated);
            assert_eq!(s.application_migration_count, 1);
            assert_eq!(s.application_migration_records[0].target_cpu, target);
            assert_eq!(s.table.running_cpu(watcher), Some(source),
                "rebalance publishes placement before changing ProcessTable ownership");
            // Exact production priority-or-ordinary expression, after the real
            // timer rebalance. Ordinary ProcessTable preemption releases owner.
            assert_eq!(select_after_rebalance(s, watcher, source).map(|i| i.pid), None,
                "migrating priority must not retain source ownership");
            assert_eq!(s.table.get(watcher).unwrap().state, ProcessState::Ready);
            assert_eq!(s.table.running_cpu(watcher), None);
            assert_eq!(s.table.current_pid_on(source), None);
            assert_eq!(s.surface_priority_tid, watcher);
            assert_eq!(select_after_rebalance(s, 0, target).unwrap().pid, watcher);
            assert_eq!(s.table.running_cpu(watcher), Some(target));
            assert_eq!((0..4).filter(|cpu|
                s.table.current_pid_on(*cpu) == Some(watcher)).count(), 1);
            assert_eq!((s.surface_priority_tid, s.surface_priority_deadline), (0, 0));
            assert!(s.rebalance_application_on_timer(target, watcher).is_none());
        });
        assert_eq!(APPLICATION_MIGRATIONS.load(Ordering::Acquire), 1);
        assert_eq!(APPLICATION_MIGRATION_SOURCE_MASK.load(Ordering::Acquire), 1 << source);
        assert_eq!(APPLICATION_MIGRATION_EVIDENCE_DROPS.load(Ordering::Acquire), 0);
    }
}

#[test]
fn ordinary_priority_rejects_automatic_source_without_priority_selector() {
    for initially_running in [false, true] {
        let (_, watcher) = setup();
        assert!(set_surface_priority(watcher));
        with_state(|s| {
            s.contexts[1].automatic_cpu = 2;
            if initially_running { s.table.activate_on(1, watcher).unwrap(); }
            assert!(s.schedule_next_for_cpu(1).is_none(),
                "ordinary priority must respect automatic target");
            assert_eq!(s.table.get(watcher).unwrap().state, ProcessState::Ready);
            assert_eq!(s.table.running_cpu(watcher), None);
            assert_eq!(s.surface_priority_tid, watcher);
            assert!(SERIAL.lock().unwrap().is_empty());
            assert_eq!(s.schedule_next_for_cpu(2).unwrap().pid, watcher);
            assert_eq!(s.table.running_cpu(watcher), Some(2));
            assert_eq!((s.surface_priority_tid, s.surface_priority_deadline), (0, 0));
            assert!(s.schedule_next_for_cpu(3).is_none());
            assert_eq!(s.table.running_cpu(watcher), Some(2));
        });
    }
}

#[test]
fn automatic_routing_exceptions_preserve_affinity_for_both_selectors() {
    for ordinary in [false, true] {
        for (leader_target, explicit, automatic) in [(false, true, 3), (false, false, 0), (true, false, 3)] {
            let (leader, watcher) = setup();
            let tid = if leader_target { leader } else { watcher };
            let allowed = if leader_target { 0 } else { 2 };
            let denied = if leader_target { 1 } else { 0 };
            assert!(set_surface_priority(tid));
            with_state(|s| {
                let index = if leader_target { 0 } else { 1 };
                s.contexts[index].automatic_cpu = automatic;
                s.contexts[index].affinity_user_set = explicit;
                s.contexts[index].affinity_mask = 1 << allowed;
                // Keep the unrelated context out of the denied-CPU check.
                s.contexts[1 - index].startup_ready = false;
                if ordinary {
                    assert!(s.schedule_next_for_cpu(denied).is_none());
                    assert_eq!(s.surface_priority_tid, tid);
                    assert_eq!(s.schedule_next_for_cpu(allowed).unwrap().pid, tid);
                } else {
                    assert!(select_surface_priority(s, 0, denied).is_none());
                    assert_eq!(s.surface_priority_tid, tid);
                    assert_eq!(select_surface_priority(s, 0, allowed).unwrap().pid, tid);
                }
                assert_eq!(s.table.running_cpu(tid), Some(allowed));
                assert_eq!((s.surface_priority_tid, s.surface_priority_deadline), (0, 0));
            });
        }
    }
}

#[test]
fn running_prior_rejects_wrong_cpu_and_retains_hint() {
    let (_, watcher) = setup();
    assert!(set_surface_priority(watcher));
    with_state(|s| {
        s.contexts[1].automatic_cpu = 2;
        s.table.activate_on(2, watcher).unwrap();
        assert!(select_surface_priority(s, watcher, 1).is_none(),
            "Running prior cannot bypass automatic CPU eligibility");
        assert_eq!(s.surface_priority_tid, watcher);
        assert_eq!(s.table.running_cpu(watcher), Some(2));
        assert_eq!(select_surface_priority(s, watcher, 2).unwrap().pid, watcher);
        assert_eq!((s.surface_priority_tid, s.surface_priority_deadline), (0, 0));
    });
}

#[test]
fn ordinary_priority_preserves_startup_and_remote_stop_guards() {
    let (leader, watcher) = setup();
    assert!(set_surface_priority(watcher));
    with_state(|s| {
        s.contexts[1].startup_ready = false;
        assert!(s.schedule_next_for_cpu(1).is_none());
        assert_eq!(s.surface_priority_tid, watcher);
        s.contexts[1].startup_ready = true;
        REMOTE_GROUP_STOP_PID.store(leader, Ordering::Release);
        assert!(s.schedule_next_for_cpu(1).is_none());
        assert_eq!(s.surface_priority_tid, watcher);
        REMOTE_GROUP_STOP_PID.store(0, Ordering::Release);
        assert_eq!(s.schedule_next_for_cpu(1).unwrap().pid, watcher);
    });
}
'''

with tempfile.TemporaryDirectory(prefix="makos-surface-priority-") as temporary:
    directory = Path(temporary)
    library = directory / "libmakos_process_table.rlib"
    subprocess.run(
        ["rustc", "--edition=2024", "--crate-name", "makos_process_table",
         "--crate-type", "rlib", str(ROOT / "crates/process-table/src/lib.rs"),
         "-o", str(library)], check=True,
    )
    (directory / "test.rs").write_text(host_fixture)
    # Compile the actual balance policy, excluding its separately registered
    # unit module so this fixture's case count remains explicit.
    (directory / "aarch64_application_balance.rs").write_text(
        (ROOT / "kernel/src/aarch64_application_balance.rs").read_text().split("#[cfg(test)]")[0]
    )

    def compile_fixture(name: str, **changes: str) -> Path:
        functions = production_functions | changes
        (directory / "priority_production.rs").write_text(
            priority_constant + "\n" + "\n".join(functions.values())
        )
        binary = directory / name
        subprocess.run(
            ["rustc", "--edition=2024", "--test", "-D", "warnings",
             str(directory / "test.rs"), "--extern", f"makos_process_table={library}",
             "-o", str(binary)], check=True,
        )
        return binary

    subprocess.run(
        [str(compile_fixture("surface-priority-tests")), "--test-threads=1"],
        check=True, timeout=30,
    )

    def reject(name: str, test: str, diagnostic: str, **changes: str) -> None:
        result = subprocess.run(
            [str(compile_fixture(name, **changes)), "--exact", test],
            capture_output=True, text=True, timeout=15,
        )
        output = result.stdout + result.stderr
        if result.returncode == 0 or diagnostic not in output:
            raise AssertionError(f"{name} did not reject restored defect:\n{output}")

    old_report = production_functions["report_surface_priority_dispatch"]
    old_report = old_report[:-1] + r'''
    with_state(|state| {
        if state.surface_priority_tid == tid { clear_surface_priority(state); }
    });
}'''
    reject("old-report-clobber", "old_report_does_not_clobber_same_tid_refresh",
           "late dispatch report must not erase refreshed same-TID priority",
           report_surface_priority_dispatch=old_report)
    retained = production_functions["select_surface_priority"].replace(
        "if selected.is_some()", "if false && selected.is_some()", 1
    )
    assert retained != production_functions["select_surface_priority"]
    reject("unconsumed-priority", "ready_selection_consumes_once_before_telemetry",
           "selected priority must be consumed under lock",
           select_surface_priority=retained)
    unrestricted = production_functions["surface_priority_cpu_eligible"].replace(
        "slot.affinity_mask & (1u8 << cpu) != 0", "slot.affinity_mask != 0", 1
    )
    assert unrestricted != production_functions["surface_priority_cpu_eligible"]
    reject("ignored-affinity", "leader_and_worker_affinity_are_authoritative",
           "leader must remain CPU0-only", surface_priority_cpu_eligible=unrestricted)
    partial_clear = production_functions["clear_surface_priority"].replace(
        "state.surface_priority_deadline = 0;", "", 1
    )
    assert partial_clear != production_functions["clear_surface_priority"]
    reject("partial-expiry-clear", "expiry_clears_pair_and_refresh_survives_old_report",
           "expiry must clear the coherent tuple", clear_surface_priority=partial_clear)
    ordinary_unconsumed = production_functions["ordinary_consume"].replace(
        "clear_surface_priority(self);", "let _ = self;", 1
    )
    assert ordinary_unconsumed != production_functions["ordinary_consume"]
    reject("ordinary-unconsumed", "ordinary_selection_consumes_only_matching_hint",
           "ordinary selection must consume its matching one-shot hint",
           ordinary_consume=ordinary_unconsumed)
    ordinary_unreported = production_functions["ordinary_consume"].replace(
        "report_surface_priority_dispatch(surface_priority_tid, cpu, leader);",
        "let _ = (surface_priority_tid, cpu, leader);", 1,
    )
    assert ordinary_unreported != production_functions["ordinary_consume"]
    reject("missing-ordinary-report", "ordinary_leader_selection_reports_main_once",
           "ordinary leader selection must emit exactly one main dispatch record",
           ordinary_consume=ordinary_unreported)
    running_bypass = production_functions["select_surface_priority"].replace(
        "if tid == prior_pid && surface_priority_cpu_eligible(slot, cpu) => Some(info)",
        "if tid == prior_pid => Some(info)", 1,
    )
    assert running_bypass != production_functions["select_surface_priority"]
    reject("running-placement-bypass",
           "rebalance_running_priority_releases_source_before_unique_target_dispatch",
           "migrating priority must not retain source ownership",
           select_surface_priority=running_bypass)
    ignored_automatic = production_functions["surface_priority_cpu_eligible"].replace(
        "|| usize::from(slot.automatic_cpu) == cpu)", "|| true)", 1,
    )
    assert ignored_automatic != production_functions["surface_priority_cpu_eligible"]
    reject("ignored-automatic-routing",
           "ordinary_priority_rejects_automatic_source_without_priority_selector",
           "ordinary priority must respect automatic target",
           surface_priority_cpu_eligible=ignored_automatic)
    ordinary_bypass = production_functions["ordinary_selection"].replace(
        "surface_priority_cpu_eligible(slot, cpu)",
        "slot.role == ProcessRole::Firefox && slot.affinity_mask & (1u8 << cpu) != 0", 1,
    )
    assert ordinary_bypass != production_functions["ordinary_selection"]
    reject("ordinary-placement-bypass",
           "ordinary_priority_rejects_automatic_source_without_priority_selector",
           "ordinary priority must respect automatic target",
           ordinary_selection=ordinary_bypass)

print(
    "MAKOS_AARCH64_SURFACE_PRIORITY_HOST_OK priority=production-code "
    "process_table=production-code rebalance,ordinary-predicate=production-code "
    "lock,clock,serial=host-adapters cases=16 "
    "negative_controls=unlocked-publication,old-report-clobber,unconsumed-priority,"
    "ignored-affinity,partial-expiry-clear,ordinary-unconsumed,missing-ordinary-report,"
    "running-placement-bypass,ignored-automatic-routing,ordinary-placement-bypass "
    "scope=hint-coherence-and-migration-ownership-not-guest-latency"
)

print(
    "MAKOS_AARCH64_SURFACE_PRIORITY_TEST_OK "
    "trigger=queued-key target=firefox-input-watcher wait=exact-handle decoy=not-woken blocked,active=retain-hint fallback=process-leader "
    "ready=next-schedule boost=one-shot,stale-deadline handoff=watcher-ap,post-enqueue-syscall:149,leader-cpu0,futex-refresh "
    "delivery=gicv2-spi recovery_poll=100hz owner=cpu0 expiry=ticks "
    "fallback=round-robin pointer=unchanged"
)
