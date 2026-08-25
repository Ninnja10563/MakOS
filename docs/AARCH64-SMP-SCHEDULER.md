# AArch64 multicore userspace scheduler design

## Current verified boundary

QEMU `virt` starts four PEs through PSCI `CPU_ON_64`. Every PE has a
private 1 MiB EL1 stack, VBAR, GICC interface, logical ID in `TPIDR_EL1`, and
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
recovery. The gate closes between the qualification fixtures. After driver and
login-UI initialization, the desktop opens a bounded production gate:
process leaders, the shell, UI services, and every non-Firefox role remain on
CPU0, while non-leader Firefox-role threads are AP-eligible on the shared Ready
queue. Device MMIO remains CPU0-owned. The production scheduler scope remains
bounded; this is not general desktop SMP scheduling.

The production gate now records actual simultaneous execution, not only the
set of CPUs used over a process lifetime. Each AP publishes the currently
selected Firefox TID before its active bit; every in-exception yield/block
switch refreshes that TID at the scheduler selection point. The first snapshot
with at least two AP bits validates distinct nonzero owners and is retained
through process reap. A production-only three-pthread rendezvous qualifies the
mechanism on Pi/QEMU TCG: all APs dispatch workers, AP1/AP3 overlap with TIDs
6/5 (`overlap_mask=0xa`), the live snapshot matches the final aggregate, and
the process exits 42. The strict real-Firefox Make target requires equivalent
evidence from the launched Firefox group while preserving every existing
latency and interaction limit. Until that target passes on idle macOS/HVF, the
fixture is not real-Firefox qualification. Because several PEs can log during
this interval, PL011 formatted and raw writes use an IRQ-masked cross-PE lock;
the final repeat emits intact records.

Surface-key priority follows the same affinity split. A Firefox thread blocked
in `surface_wait_event` publishes its TID; AP1-3 may select that non-leader
watcher even while CPU0 is busy, and CPU0 alone selects the leader after the
watcher dequeues a key. A racing PE that observes the selected TID Running on
another PE preserves the hint. Each watcher or leader hint is consumed after
one successful dispatch; the 1,000-tick deadline is only stale-hint cleanup,
not a priority time slice. This one-shot rule prevents a yielding CPU0 leader
from starving a newly forked same-affinity Firefox child. The focused Pi/TCG
runtime creates a real process-owned overflow surface, blocks a non-leader
upstream-musl pthread in syscall 140, injects QMP Ctrl-A, and requires the same
watcher TID on AP1-3 plus the group leader on CPU0 before full status-42 reap.
Its final run selected watcher TID 8 on AP2. This is deterministic wake and
affinity evidence, not macOS/HVF Firefox latency qualification.

Production thread affinity is now explicit kernel state rather than an
adapter-reported constant. Native syscall 148 gets or replaces the mask of the
caller or a same-thread-group TID. Masks must be nonempty subsets of the four
online CPUs. Firefox and native non-leader threads may choose those CPUs;
process leaders retain mask `0x1` because desktop and device service remains
CPU0-owned. Clone gives Firefox workers the production default `0xe`, while
forked/new process leaders begin at `0x1`. Every normal scheduler selection,
including surface priority, consults the stored mask. If a caller removes its
current CPU, the exception path captures its complete context, publishes it
Ready/unowned, returns an AP with no eligible successor to its dispatcher, and
sends the scheduler SGI only after publication so an idle target AP rechecks
its Ready queue. A running remote target also rechecks at its next bounded
timer preemption. The musl Linux-number adapter maps
`sched_setaffinity(122)` and `sched_getaffinity(123)` to this native ABI and
fully clears/copies the caller's bounded `cpu_set_t`.

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

A fourth fixture proves the safe-return case for a sibling already executing
in EL1. Its AP1 worker enters the real yield syscall; an armed boot-probe hook
publishes a user-visible release flag only after EL1 entry and holds that
syscall until CPU0 publishes `exit_group(56)`. The normal yield ABI is
unchanged outside this bounded probe. SVC and resolved page-fault returns now
recheck the stop contract before EL0 return. The AP detaches under the
scheduler lock, switches to the kernel root, rewrites the outer exception
frame, executes a `DSB`, and then acknowledges. An exception that entered the
scheduler just before publication uses the same locked boundary and contributes
an early-stop bit to the exact target/ack contract. Runtime requires
`entered_el1_mask=0x2`, `deferred_ack_mask=0x2`, target/ack masks `0x2`, status
56, single address-space reap, and exact frame recovery.

A fifth fixture starts two independent EL0 processes on CPU0 and AP1. Both
rendezvous inside syscall 119 before either may acquire the group-stop
coordinator. A dedicated atomic coordinator, separate from the process-table
lock, publishes status before group identity and bounds contention at 20
seconds. Both callers subsequently acquire it one at a time, exit with their
own statuses 57 and 58, and reap their distinct roots. Runtime requires
`rendezvous_mask=0x3`, `serialized_acquire_mask=0x3`, a maximum of one
coordinator owner, and exact frame recovery.

This concurrent deep teardown initially exposed a real AP kernel-stack
overflow: the prior 64 KiB AP1 stack grew into the adjacent `KERNEL_ROOT`
atomic, which QMP physical-memory inspection found cleared. AP stacks now
match the BSP at 1 MiB, and the boot contract requires
`stack_bytes=1048576`. This is necessary before general AP syscalls, not merely
for the fixture.

A sixth fixture clones a shared-address-space sibling and rendezvous-holds the
leader on CPU0 and worker on AP1 inside syscall 119 with distinct requested
statuses 59 and 60. Exactly one caller owns the coordinator; the other joins
the already-published group teardown. The joiner transitions its own task to
Zombie under the process lock, switches to the kernel root, publishes the
early-stop/ack bits after an inner-shareable barrier, and returns through its
per-CPU kernel frame without running duplicate cleanup. Runtime proves
`cpu_mask=0x3`, one owner bit, the complementary joined bit, first-owner status
for both callers, one shared-root reap, and exact frame recovery.

A forced-migration fixture then starts one immutable EL0 task on AP1. Its armed
yield requests AP2. Under the scheduler lock the kernel captures the live
context, removes AP1 ownership, changes Running to Ready/unowned, publishes AP2
affinity and only then permits target selection. AP1 returns through its private
kernel frame; AP2 selects the same TID and resumes after the SVC. The task
verifies callee-saved GPRs, SP-backed data, `TPIDR_EL0`, and SIMD `v0`, then
exits 71. The focused Pi/QEMU TCG repeat requires source/target masks
`0x2`/`0x4`, exactly one migration, the same TID on both PEs, exclusive
ownership, frame balance, and status 71. An initial run already migrated and
exited correctly but exposed a target-mask observer race because AP2 could run
before the source incremented a later counter; publishing the intended target
inside the locked transition made the evidence race-free. This qualifies one
forced migration, not load balancing or unrestricted desktop migration.

A shared-Ready-queue fixture then creates six immutable EL0 tasks for AP1-AP3.
Every task executes 48 real yield syscalls, so the three per-CPU round-robin
selectors contend for one locked queue across at least 288 Ready publications.
Selection verifies that the chosen task has exactly one CPU owner before each
dispatch; per-task CPU masks and per-CPU counters are recorded independently.
The focused Pi/QEMU 10.0.11 TCG gate requires all six exact statuses 80-85,
worker mask `0xe`, at least 32 dispatches per AP, bounded max/min skew, exact
frame recovery, and no duplicate ownership. The passing run records perfectly
even counters `99,99,99` (297 selections total). During qualification, longer
absolute sleeps exposed two real wake-path defects: session liveness incorrectly
ignored Blocked/Ready tasks, and CPU0 stopped its sole global timer while
waiting in EL1. Liveness now includes Ready/Running/Blocked tasks; Ready
publication uses the scheduler SGI; AP idle acknowledges interrupts around
`WFI`; and CPU0 explicitly keeps its scheduler timer armed during the bounded
completion wait. This qualifies shared-queue AP load execution under repeated
yield contention. The gate closes again before later boot fixtures and only
reopens under the bounded production policy after desktop startup.

An opt-in seventh fixture runs only with `test.smp-input=required`, after the
virtio keyboard/tablet and graphics service are initialized. AP1 enters the
real EL0 `read_key` syscall while a CPU0-affined Ready sentinel prevents the
last-runnable WFI shortcut. With no AP1-eligible successor, the waiter now
returns to the AP idle dispatcher instead of undoing its block. The host
harness sends a real QEMU Ctrl-K event; CPU0 drains the virtio used ring, wakes
the input wait class, and sends the scheduler SGI. Virtio-input MMIO and its
deferred compositor work now have an explicit CPU0 owner. AP syscall and TTY
paths record a deferral instead of polling the device, while the low-level
driver fails closed if called from a non-owner CPU. Runtime requires nonzero
CPU0 ring activity and AP deferrals, matching
`input_idle_mask=0x2`/`input_resume_mask=0x2`, exact key delivery, status 61,
frame balance, and subsequent boot completion. The ordinary boot config never
arms or waits for external test input.

The same opt-in image first runs an eighth fixture against real virtio-net and
QEMU slirp DNS. AP1 copies transaction `0x4d4c` into an eight-slot, 1,400-byte
bounded UDP service queue; CPU0 owns the actual transmit ring, completes the
request, and wakes the requester. AP1 then blocks in `recvfrom` and returns to
its idle dispatcher. CPU0 exclusively drains and demultiplexes the RX ring,
wakes the exact network wait source, and sends the scheduler SGI. Low-level TX,
RX, and socket-pump entry points fail closed on the wrong CPU. Runtime requires
a validated DNS response, nonzero CPU0 TX completions, AP TX requests, CPU0 RX
frames and AP RX deferrals, `io_idle_mask=0x2`/`io_resume_mask=0x2`, status 63,
and exact frame balance. This qualifies copied UDPv4/v6 TX; stateful AP TCPv4
is qualified separately below. The AP waits for its UDP completion in a bounded
EL1 `WFE` loop, so only the following receive phase proves scheduler
block/idle/wake behavior.

A dedicated image armed by `test.smp-tcp=required` qualifies stateful TCPv4.
AP1 creates a socket, connects through QEMU slirp to `10.0.2.2:18080`, sends
exact `MAKOS_AP_TCP_TX\n`, receives exact `MAKOS_CPU0_TCP_RX\n`, and closes.
Connect and segment operations copy remote addresses, sequence/acknowledgement
state, flags, window and payload through the eight-slot owner queue. CPU0 alone
resolves the route, performs SYN/SYN-ACK/ACK, submits data/ACK/FIN and drains
RX. Socket send/receive mutate the live table entry under its lock, so an AP
never publishes an obsolete whole-socket snapshot over CPU0 receive state.
The host fixture delays its reply by 0.5 seconds: AP1 must return from the real
blocking receive to its idle dispatcher before CPU0's timer pump ingests the
frame and sends its scheduler SGI. Pi/QEMU 10.0.11 TCG requires exact statuses
69/70, request/response bytes, one owner RX frame and AP deferral, four AP
requests/four owner completions (connect, data, ACK, FIN),
`io_idle_mask=0x2`/`io_resume_mask=0x2`, and exact frame balance. TCPv6 uses the
same copied service structure but still requires a focused guest runtime.

The scheduler fixture forces its fixed-affinity SmpProbe AP through the normal
Blocked-to-idle path even when host-vCPU ordering would otherwise make it the
last runnable task and take the EL1 WFI shortcut. This is confined to the
opt-in boot probe; production last-runnable behavior is unchanged.

A ninth opt-in fixture qualifies the production virtio-blk owner service. The
kernel creates a private mode-0600 uid/gid-1000 fixture inode and binds the
immutable AP1 probe to the minimum file-write capability while no login session
exists. AP1 uses normal VFS/MakFS4 syscalls to write and `fsync` 4 KiB, close,
reopen, read and byte-verify 4 KiB, then the kernel removes the fixture. Every
AP read/write/FLUSH is copied through an eight-slot queue. CPU0's ordinary 100
Hz timer bottom half alone submits requests; if the IRQ interrupted a direct
CPU0 block operation, the service observes the owner lock and defers one tick
instead of recursing. Low-level ring submission still fails closed off CPU0.
Pi/TCG runtime reports 33 requests/completions: 18 reads, 10 writes and 5
flushes, all 33 serviced by the timer, status 65, exact content, inode cleanup,
and frame balance. The AP request wait remains bounded EL1 `WFE`, not scheduler
block/idle/wake evidence.

An additional opt-in fixture qualifies the production virtio-GPU owner path.
The immutable AP1 process receives only `CAP_GRAPHICS` and uses the ordinary
native surface create/fill/present ABI. Any `compose` requested off CPU0 retains
the new scene and publishes one coalesced deferred action; it never takes the
device lock or touches MMIO. CPU0's ordinary timer bottom half consumes that
action. Low-level control and cursor submission entry points fail closed on an
AP. The first Pi/TCG runtime reports one AP deferral, one CPU0 owner compose,
two real control-queue completions (one transfer and one resource flush),
status 67, surface reap, exact frame balance, and continued block/network/input
and boot success.

The offline scheduler foundation adds:

- `ProcessTable::*_on(cpu, ...)` transitions with one current task per CPU and
  a test proving one saved context cannot run on two CPUs;
- AArch64 process/syscall/exception paths select `current_pid_on(cpu)` and use
  CPU-indexed activate, schedule, block, exit, and exec-resource transitions;
- per-CPU EL0-to-kernel return records selected through `TPIDR_EL1`;
- per-CPU active TTBR0 caches, cross-CPU active-root teardown rejection, and
  inner-shareable page invalidation (`TLBI VAE1IS`) for shared process roots.

These changes preserve existing CPU0 wrappers. The boot probe is genuine
parallel EL0 evidence, but it is not evidence of real Firefox overlap. A
separate production-role fixture uses the
upstream musl pthread ELF under the exact Firefox scheduler role to exercise
clone, futex, pipe, signal, AP block/wake, join, exit, wait, and reap after the
desktop has initialized. It is explicitly not a substitute for real Firefox.

## Next enablement blockers

- Initial and exception-time AP selectors restrict candidates to non-leader
  Firefox workers after desktop startup. Leaders and all other roles remain on
  CPU0. Eligible threads may select kernel-owned masks through syscall 148;
  broader production roles and automatic load balancing remain gated until
  device-owning and PID1/UI paths are qualified.
- One AP1-to-AP2 forced migration preserves GPR/SP/TLS/SIMD state and exclusive
  ownership through a Ready/unowned publication. Six load tasks also contend
  through 288 yields on the shared Ready queue and receive 99 dispatches on
  each AP with exclusive ownership. The musl production fixture also forces
  three caller-selected cross-AP migrations and restores its AP-pool masks.
  Broader production policy/priorities, automatic load-driven migration, and
  Firefox/desktop contention remain open.
- `sleep_until`, timed poll/I/O, timed futex, and event waits with no AP-eligible
  successor now return through the per-CPU saved kernel record into the AP idle
  loop and have timer- or cross-CPU-event-wake/resume proof. Thread-only exit
  also returns to its calling CPU's kernel record when no local successor
  exists. Input no-successor behavior now has a real virtio-keyboard
  device-triggered AP idle/wake/resume proof.
- CPU0-initiated `exit_group` now stops and acknowledges both a remote-running
  EL0 sibling and a sibling inside a returning SVC/page-fault EL1 path before
  reap. Simultaneous unrelated groups now serialize without holding the
  process-table lock, while simultaneous callers within one group cooperate
  through a first-owner-wins join path. A permanently non-returning EL1 driver
  path would still require a cancellable safe point. Administrative `terminate`
  correctly continues to reject a task owned by another CPU.
- AP banked virtual-timer PPI enable/programming and CPU0-only global tick
  servicing pass the bounded probe. Virtio input now has exclusive CPU0 MMIO
  ownership plus measured AP deferral. Virtio-net now has CPU0-only low-level
  TX/RX ownership, copied AP UDPv4/v6 service, and a real AP DNS receive wake.
  Stateful TCPv4 now has copied CPU0-owned connect, segment, ACK and FIN service
  plus a real AP blocking-receive/SGI-wake proof. Virtio-blk now has CPU0-only
  ring submission and a production timer-bottom-
  half service proof for real AP 4 KiB reads, 4 KiB writes, and `fsync`/FLUSH
  through VFS/MakFS4. Virtio-GPU now has CPU0-only low-level submission and a
  production timer-bottom-half proof for composition requested through the AP
  native surface ABI. TCPv6 remains to be qualified.
- Ready publication now sends the scheduler SGI after the process lock's
  Release unlock; AP idle acknowledges IRQs around `WFI`, then consumes queue
  state after Acquire lock acquisition. This has boot-probe coverage and a
  production-role pthread fixture; genuine Firefox overlap and performance on
  the intended idle macOS/HVF host remain required.

Before desktop startup and between bounded boot fixtures, APs remain in
closed-gate WFI at EL1. After desktop startup, they stay available for eligible
Firefox workers and return to WFI when the shared queue has no eligible task.
Routine BSP `SEV` traffic cannot open the gate. Enablement and Ready publication
issue a GICv2 SGI after their Release stores.

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
   - GICC acknowledge/EOI is per PE. Virtio-input MMIO and virtio-net low-level
     TX/RX ring service are explicitly CPU0-owned and guarded against AP entry.
     AP UDPv4/v6 and TCPv4 use bounded copied-request services; TCPv6 still
     needs equivalent guest runtime qualification. Virtio-blk ring submission is also
     CPU0-only through a bounded copied-request service. CPU0's timer bottom
     half passes real AP VFS/MakFS4 read, write, and FLUSH traffic, while
     recursively interrupted CPU0 ownership defers one tick. Retained graphics
     composition requested by an AP is coalesced and serviced by that same CPU0
     timer path; low-level virtio-GPU submissions fail closed off-owner.

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

The post-desktop marker reports `userspace_scheduler_cpus=4` only for this
bounded policy. The original audit remains Partial until genuine Firefox
threads overlap on multiple guest CPUs under the unchanged idle-macOS/HVF
runtime gate and broader production roles are safely scheduled.
