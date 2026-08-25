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
zero-descriptor 20 ms `poll` also blocks each AP with its PC rewound to the SVC,
returns to idle, wakes on the shared deadline, retries, and observes timeout;
runtime requires `io_idle_mask=0xe` and `io_resume_mask=0xe`. The gate then
closes and APs return to interrupt-masked WFI. General
desktop userspace still runs on CPU0, so `userspace_scheduler_cpus=1` remains
the truthful scope marker.

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
- `sleep_until`, timed poll/I/O, and timed futex waits with no AP-eligible
  successor now return through the per-CPU saved kernel record into the AP idle
  loop and have timer-wake/resume proof. IPC and input no-successor paths still
  reactivate or reject the sole local task and need the same idle-return
  contract. Other I/O wait sources share the proved scheduler path but still
  need device-triggered runtime coverage.
- Exit/session teardown must distinguish no local successor from no live
  session. `exit_group` must first stop/ack remote-running siblings; current
  administrative `terminate` correctly rejects a task owned by another CPU.
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
