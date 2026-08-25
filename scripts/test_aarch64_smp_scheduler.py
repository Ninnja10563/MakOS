#!/usr/bin/env python3
"""Offline invariants for bounded AArch64 SMP EL0 proof and gated desktop."""

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
    "SMP_USER_SCHEDULER_ENABLED.store(true, Ordering::Release);",
    "SMP_USER_SCHEDULER_ENABLED.store(false, Ordering::Release);",
    "user_stack_pointer_valid_in(context.ttbr0, context.sp_el0)",
    "pub(crate) fn send_scheduler_ipi()",
    "stop_remote_group_member_from_irq(frame)",
    "stop_remote_group_member_on_el0_return(frame)",
):
    assert token in ARCH, token

assert ARCH.count("crate::aarch64_process::ipc_control_allowed()") == 4

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
    "slot.role == ProcessRole::SmpProbe",
    "pub fn run_smp_userspace_self_test()",
    "SMP_PROBE_PEAK_MASK",
    "SMP_PROBE_IDLE_MASK",
    "SMP_PROBE_RESUME_MASK",
    "SMP_PROBE_FUTEX_IDLE_MASK",
    "SMP_PROBE_FUTEX_RESUME_MASK",
    "SMP_PROBE_AFFINITY",
    "FutexBlockResult::BspIdle",
    "SMP_PROBE_IO_IDLE_MASK",
    "SMP_PROBE_IO_RESUME_MASK",
    "SMP_PROBE_IPC_IDLE_MASK",
    "SMP_PROBE_IPC_RESUME_MASK",
    "pub fn run_smp_ipc_self_test()",
    "MAKOS_AARCH64_SMP_IPC_OK",
    "REMOTE_GROUP_STOP_TARGET_MASK",
    "REMOTE_GROUP_STOP_ACK_MASK",
    "REMOTE_GROUP_STOP_EARLY_MASK",
    "pub fn run_smp_exit_group_self_test()",
    "MAKOS_AARCH64_SMP_EXIT_GROUP_OK",
    "SMP_PROBE_EL1_ENTER_MASK",
    "SMP_PROBE_GROUP_STOP_DEFERRED_MASK",
    "hold_smp_exit_group_probe_in_el1",
    "pub fn run_smp_exit_group_el1_self_test()",
    "MAKOS_AARCH64_SMP_EXIT_GROUP_EL1_OK",
    "return_to_kernel(frame, 0)",
    "statuses != [40, 41, 42, 43]",
    "peak != 0b1111",
    "idle & 0b1110 != 0b1110",
    "resumed & 0b1110 != 0b1110",
    "futex_idle & 0b1110 != 0b1110",
    "futex_resumed & 0b1110 != 0b1110",
    "io_idle & 0b1110 != 0b1110",
    "io_resumed & 0b1110 != 0b1110",
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

assert "Until every gate passes, MakOS reports `userspace_scheduler_cpus=1`." in DESIGN
assert "Firefox first paint/navigation" in DESIGN

print(
    "MAKOS_AARCH64_SMP_SCHED_FOUNDATION_OK process_table=per-cpu-current "
    "exception_paths=per-cpu kernel_return=per-cpu ttbr_cache=per-cpu "
    "tlbi=inner-shareable "
    "runtime=boot-probe,4cpus desktop_enabled=0 truthful_marker=userspace_scheduler_cpus=1"
)
