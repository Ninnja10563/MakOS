#!/usr/bin/env python3
"""Offline invariants for the not-yet-enabled AArch64 SMP EL0 scheduler."""

from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TABLE = (ROOT / "crates/process-table/src/lib.rs").read_text()
ARCH = (ROOT / "kernel/src/arch/aarch64.rs").read_text()
PROCESS = (ROOT / "kernel/src/aarch64_process.rs").read_text()
DESIGN = (ROOT / "docs/AARCH64-SMP-SCHEDULER.md").read_text()

for token in (
    "current: [Option<usize>; MAX_SCHEDULER_CPUS]",
    "pub fn activate_on(",
    "pub fn schedule_next_on(",
    "pub fn schedule_next_where_on(",
    "pub fn block_current_on(",
    "pub fn exit_current_on(",
    "pub fn current_pid_on(",
    "pub fn running_cpu(",
    "multicore_selection_never_runs_one_context_twice",
    "affinity_selection_leaves_rejected_tasks_ready_and_unowned",
):
    assert token in TABLE, token

for token in (
    "ACTIVE_USER_ROOTS",
    "pub(crate) fn cpu_index()",
    ".space 1024",
    "add x10, x10, x11, lsl #8",
    '"tlbi vae1is, {page}"',
    "root_active_on_any_cpu(root)",
    "static SMP_USER_SCHEDULER_ENABLED: AtomicBool = AtomicBool::new(false);",
    "fn secondary_scheduler_idle() -> !",
    "fn init_secondary_timer_on_current_cpu()",
    "if cpu_index() == 0",
):
    assert token in ARCH, token

for token in (
    "fn scheduler_cpu() -> usize",
    "current_pid_on(scheduler_cpu())",
    "activate_on(scheduler_cpu(),",
    "schedule_next_for_cpu(scheduler_cpu())",
    "block_current_on(scheduler_cpu())",
    "exit_current_on(scheduler_cpu(),",
    "replace_current_resource_on(scheduler_cpu(),",
    "pub(crate) fn run_secondary_scheduler() -> !",
    "schedule_next_where_on(cpu, |info|",
    "slot.pid != slot.group_pid",
    "slot.role == ProcessRole::Firefox",
    'asm!("dsb ish", "sev", options(nostack))',
):
    assert token in PROCESS, token

# Any CPU0 compatibility wrapper here would silently mutate CPU0 ownership
# when same syscall/exception path executes on an AP.
for cpu0_wrapper in (
    ".current_pid()",
    ".activate(",
    ".schedule_next()",
    ".block_current()",
    ".exit_current(",
    ".replace_current_resource(",
):
    assert cpu0_wrapper not in PROCESS, cpu0_wrapper

assert "SMP_USER_SCHEDULER_ENABLED.store(true" not in ARCH

assert "Until every gate passes, MakOS reports `userspace_scheduler_cpus=1`." in DESIGN
assert "Firefox first paint/navigation" in DESIGN

print(
    "MAKOS_AARCH64_SMP_SCHED_FOUNDATION_OK process_table=per-cpu-current "
    "exception_paths=per-cpu kernel_return=per-cpu ttbr_cache=per-cpu "
    "tlbi=inner-shareable "
    "runtime_enabled=0 truthful_marker=userspace_scheduler_cpus=1"
)
