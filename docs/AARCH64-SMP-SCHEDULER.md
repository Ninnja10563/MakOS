# AArch64 multicore userspace scheduler design

## Current verified boundary

QEMU `virt` starts four PEs through PSCI `CPU_ON_64`. Every PE has a
private 64 KiB EL1 stack, VBAR, GICC interface, logical ID in `TPIDR_EL1`, and
coherent identity-mapped kernel tables. A boot rendezvous proves all APs make
parallel EL1 progress. A bounded boot probe then sends a GICv2 SGI, enables each
AP's banked virtual-timer PPI, and runs four independent EL0 processes at once.
The three AP dispatchers publish their active bits and wait immediately before
EL0 transition; CPU0 joins that rendezvous and releases all four PEs together.
Pi/QEMU TCG requires four distinct TIDs, `overlap_mask=0xf`, unique statuses
40-43, complete address-space reap, and exact frame recovery. Each AP process
also calls `sleep_until`; with no local successor it remains Blocked, returns
through the per-CPU kernel record to the idle dispatcher, and resumes after a
CPU0 timer wake. Runtime requires `resume_mask=0xe`. It then performs a timed
futex wait. Stable per-PE probe affinity prevents a woken context being stolen
while CPU0 leaves WFI; CPU0 idles inside the syscall and APs return to their
dispatchers until the 20 ms timer expiry. Runtime requires
`futex_idle_mask=0xe` and `futex_resume_mask=0xe`. This explicit release
barrier keeps the correctness fixture independent of host emulation speed. A
zero-descriptor 200 ms `poll` also blocks each AP with its PC rewound to the SVC,
returns to idle, wakes on the shared deadline, retries, and observes timeout;
runtime requires `io_idle_mask=0xe` and `io_resume_mask=0xe`. A second embedded
EL0 program then creates an auto-reset event and clones a shared-VM thread. Its
leader is fixed to AP1 and blocks in `event_wait` with no local successor; AP1
returns through its private kernel record to idle. CPU0 enters the child on its
validated RW/NX thread stack, signals the process-owned handle, and executes
thread-only exit with no CPU0 successor. AP1 resumes the exact leader, which
closes the handle and exits 44. Runtime requires `ipc_idle_mask=0x2`,
`ipc_resume_mask=0x2`, status-0 child return, parent reap, and exact frame
recovery. The gate then closes and APs return to interrupt-masked WFI. General
desktop userspace still runs on CPU0, so `userspace_scheduler_cpus=1` remains
the truthful scope marker.

A third embedded EL0 program proves remote-running group teardown. Its leader
clones a shared-VM worker, CPU0 and AP1 execute them concurrently, and the
worker publishes a release-ordered running flag before spinning in EL0. The
CPU0 leader invokes process-exit/`exit_group(55)`. Under the scheduler lock the
kernel publishes the dying group and exact remote-owner mask, excludes every
group task from new selection, then sends a GICv2 SGI. AP1 atomically detaches
the worker, switches to the kernel root, rewrites the exception return to its
private kernel record, and publishes its acknowledgement only after a `DSB`.
CPU0 waits for that acknowledgement before robust-list cleanup, worker reap,
and shared-resource handling. Runtime requires target/ack masks `0x2`, absent
worker state, one status-55 leader zombie, single address-space reap, and exact
frame recovery.

The offline scheduler foundation adds:

- `ProcessTable::*_on(cpu, ...)` transitions with one current task per CPU and
  a test proving one saved context cannot run on two CPUs;
- AArch64 process/syscall/exception paths select `current_pid_on(cpu)` and use
  CPU-indexed activate, schedule, block, exit, and exec-resource transitions;
- per-CPU EL0-to-kernel return records selected through `TPIDR_EL1`;
- per-CPU active TTBR0 caches, cross-CPU active-root teardown rejection, and
  inner-shareable page invalidation (`TLBI VAE1IS`) for shared process roots.

These changes preserve existing CPU0 wrappers. The boot probe is genuine
parallel EL0 evidence, but it is not evidence of parallel Firefox execution or
safe general process migration.

## Next enablement blockers

- Initial and exception-time AP selectors now restrict candidates to
  non-leader Firefox workers. Broader affinity/load balancing remains gated
  until device-owning and PID1/UI paths are qualified.
- `sleep_until`, timed poll/I/O, timed futex, and event waits with no AP-eligible
  successor now return through the per-CPU saved kernel record into the AP idle
  loop and have timer- or cross-CPU-event-wake/resume proof. Thread-only exit
  also returns to its calling CPU's kernel record when no local successor
  exists. Input no-successor behavior and device-triggered I/O still need
  runtime coverage.
- CPU0-initiated `exit_group` now stops and acknowledges a remote-running EL0
  sibling before reap. Teardown of a sibling interrupted in an EL1 critical
  section needs a deferred-return acknowledgement, and simultaneous unrelated
  group exits still need serialization without deadlock. Administrative
  `terminate` correctly continues to reject a task owned by another CPU.
- AP banked virtual-timer PPI enable/programming and CPU0-only global tick/device
  servicing pass the bounded probe. General AP syscalls still require a complete
  device/service ownership audit.
- Ready publication needs an idle-CPU kick (`SEV`/SGI) after the process lock's
  Release unlock; idle selection must consume after Acquire lock acquisition.

Outside the bounded boot proof, APs remain in closed-gate WFI at EL1. Routine
BSP `SEV` traffic cannot wake this gate and consume host CPU. The probe issues
a GICv2 SGI only after publishing its enabled state.

## Required runtime architecture

1. Scheduler ownership
   - All task lifecycle and selection remain under `LockedProcesses` initially.
   - Every exception path obtains `cpu_index()` and uses
     `current_pid_on(cpu)`, `schedule_next_on(cpu)`, `block_current_on(cpu)`,
     and `exit_current_on(cpu)`.
   - A Ready context becomes Running on exactly one CPU. Context save occurs
     before releasing scheduler lock; context restore occurs after selection.
   - CPU0 retains session ownership. APs returning with no Ready task enter an
     idle WFI dispatcher; they never set `session_active=false` merely because
     another CPU owns the last Running task.

2. AP dispatch
   - After desktop/session startup enables SMP scheduling, AP WFI dispatchers
     select Ready Firefox worker threads. They do not run PID1/UI threads until
     process and device paths are qualified for parallel use.
   - Clone/wake/spawn publishes the context with Release ordering then sends an
     SGI/SEV to idle dispatchers. Selection consumes it with Acquire ordering.
   - A BSP-only boot option/fallback remains available until full regression
     coverage passes.

3. Timer and interrupt state
   - Each AP enables its banked GICv2 virtual-timer PPI and programs
     `CNTV_CVAL_EL0` before EL0 entry.
   - CPU0 alone advances global monotonic ticks and services deadlines/device
     polling. AP timer IRQs only preempt/select; otherwise four timer streams
     would make wall time advance fourfold.
   - GICC acknowledge/EOI is per PE. Device bottom halves remain CPU0-owned
     until their locks and interrupt affinity are audited.

4. Address spaces and TLBs
   - TTBR0 is per PE. Same-process Firefox threads may concurrently use one
     root; different processes may use different roots.
   - Mapping mutation holds the VM/page-table owner lock, publishes PTE writes
     with `DSB ISHST`, issues `TLBI ...IS`, then `DSB ISH; ISB`.
   - Address-space destruction is forbidden while any per-CPU active-root slot
     references it. Exit-group first evicts/stops every sibling and waits for
     an acknowledgement mask before teardown.

5. Blocking, futexes, signals, exit
   - Blocking transitions only the calling CPU's current task. No-successor on
     an AP returns to its idle dispatcher rather than reactivating the task.
   - Futex/readiness wake changes Blocked to Ready once and kicks one idle CPU;
     the existing global process lock provides correctness before sharded wait
     queues are attempted.
   - `exit_group` marks a group dying, sends reschedule IPIs, prevents new
     scheduling, waits until no CPU owns a sibling, then reaps shared VM/files.
   - `clear_child_tid`, TTY, VFS, sockets, graphics, credentials, and VM cleanup
     execute once after task ownership is detached.

## Proof gates before claiming parallel EL0

- Four distinct guest TIDs simultaneously report four distinct logical CPU
  IDs from EL0 while advancing independent counters.
- Timer preemption/context restore covers GPRs, SIMD/FP, TLS, SP, PC, TTBR0 on
  every PE; no task appears on two CPUs.
- Shared-root concurrent map/protect/unmap plus broadcast TLB test passes.
- Futex wait/wake, pipe/epoll, clone/exit/exit_group and two-boot persistence
  pass under forced migrations and repeated contention.
- Firefox first paint/navigation passes with at least two Firefox TIDs observed
  running in overlapping intervals on different CPUs.
- Idle CPU, cursor integrity, pointer latency, and CPU0 fallback suites remain
  green.

Until every gate passes, MakOS reports `userspace_scheduler_cpus=1`.
