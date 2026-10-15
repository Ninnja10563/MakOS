# AArch64 multicore userspace scheduler design

## 2026-10-16: priority cannot override migration placement

The [preserved Mac fatal and repair](FIREFOX-PRIORITY-MIGRATION-20261016.md)
identifies a reachable conflict between timer rebalance and input priority.
After automatic CPU publication, both Running retention and ordinary hinted
selection must reject the old source. The common priority predicate now
honors automatic CPU assignment as well as allowed affinity. Explicit user
affinity remains authoritative; leaders stay CPU0-only. Priority changes
ordering, not placement. Rejected source selection preserves the hint until
an eligible CPU dispatches it. Existing owner-release invariant and fatal,
one-shot consumption, stale bound and migration policy limits remain.
This supersedes the historical preference-bypass policy described below;
it does not establish Firefox success or explain earlier navigation failures.

## 2026-10-15: coherent one-shot Firefox surface hints

The [Mac failure and repair report](FIREFOX-SURFACE-PRIORITY-20261015.md)
records all twelve passing Mac/HVF gates followed by a real Firefox Ctrl-A
latency failure. Surface hint TID/deadline now belong to `SchedulerState`:
publication, expiry, reset and successful selection are serialized by its
existing lock. Both explicit priority selection and ordinary blocking/exit
dispatch consume matching hints before unlock. Post-unlock telemetry cannot
erase a newer same-TID refresh. Futex wake uses the already-locked setter,
not recursive locking. The stale bound remains 1000 ticks; hints remain
one-shot, with blocked/foreign-owner retention and existing affinity rules.
This does not establish the measured browser delay's cause or a Firefox pass.

## 2026-10-14: FIFO scheduler lock and AP startup publication

The authoritative 2026-10-13 Apple M3/macOS 26.6.2, QEMU 11.0.3/HVF
qualification of `fc9338bbef4cb45c1c6a5548ca0511917d3ef10e` passes
`make unit check`, image construction, EL0-entry, VM-fault, cold user-write,
BTYPE migration and network-owner gates. The new user-write gate proves both
untouched file-backed TTY writes, exact 67-byte cross-page file readback,
ten invalid-buffer denials and cleanup, alongside the original EL0/VM/reap
checks. This is not Firefox or JIT execution evidence.

The first failed command is `make test-aarch64-smp-tcp-runtime`, but its guest
fails in the preceding unconditional boot load-balancing fixture, before any
TCP fixture work. Its complete final diagnostic is:

```text
MAKOS_AARCH64_SMP_LOAD_DIAGNOSTIC peak=0xe cpu_mask=0xe dispatches=49,59,186 total=294 min=49 max=186 statuses=80,81,82,83,84,85 task_masks_valid=1 free_balance=1
MAKOS_FAILURE_DETAIL: AArch64 SMP load-balancing proof failed
MAKOS_FATAL: AArch64 SMP load-balancing proof failed
```

The original ratio check rejects `186 > 3 * 49`. Exact statuses, task masks,
overlap and frame recovery pass. Earlier guests in the same sequence passed
that check with dispatches `107,95,99`, `88,115,96`, `118,106,79`,
`98,105,113` and `75,109,122`. The Mac therefore reproduces the same assertion
class as the prior Pi/TCG `48,145,113` failure; neither the host nor the
cold-buffer repair is established as its cause. The last prelaunch CPU
sample is 95.55% idle with normal memory pressure and no concurrent guest or
heavy build. Command exit is 2, shell real time 110.72 s and recorder elapsed
time 110.882025 s. The harness retains its 90-second marker wait and reports
its missing TCP-result assertion after the guest fatal.

The sequence stops immediately, without retry. SMP-input, self-host,
Native/Python-role, Firefox-role and cursor gates, current Firefox image
preflight/runtime, and final visible login are **not run**. Historical
results below do not qualify this commit for those later gates.

### Evidence preservation

The supplied compact evidence archive contains
`macos-hvf-fc9338bbef4c-20261013/08-smp-tcp-runtime-artifacts/makos-smp-tcp-focused-serial.log`:
9,280 bytes, SHA-256
`0f318ea022b475a49ec2564ce8ac9d94679f23d0c4ea49790181b7abcfea93da`.
The saved serial contains the full diagnostic, failure detail and fatal. The
unchanged TCP harness saves its accumulated serial before termination and
does not drain the shutdown tail; no additional bytes or numeric QEMU exit
status are claimed. QEMU PID is 76459, harness PID 76457, original session
`build/makos-smp-tcp-dp2vnc_n`, QMP inherited socketpair fd 5. The original
temporary directory was cleaned up, but all three private boot/data/vars
files were retained before cleanup under the Mac evidence directory's
`08-smp-tcp-runtime-private/makos-smp-tcp-dp2vnc_n/`. They are not in the
compact ZIP. No QEMU remains and no current screenshot was produced.

The existing Firefox integrated master remains
`build/makos-firefox-evidence-ymp9Om/makos-integrated-5baa9467ca510b8a.img`,
SHA-256 `5baa9467ca510b8a1a207365abed12fe8b03f228432a80b2238b3b46042d00ce`.
Its post-failure preservation hash is not a current preflight pass. There was
no Firefox rebuild, reintegration, provenance restamp or profile edit.

### Repair scope and validation boundary

The former scheduler-state test-and-set lock permits a fast reacquiring PE
or CPU0's completion poll to overtake PEs already waiting for the same lock.
The lock repair replaces it with the allocation-free FIFO ticket
core in `kernel/src/ticket_lock.rs`. Each waiter reserves one turn; an owner
publishes its completed scheduler-state changes with Release ordering, and
the next owner observes its turn with Acquire ordering. Queued callers
cannot be overtaken by subsequent arrivals. The guard releases the lock
before `with_state` returns.

`with_state` masks local interrupts **before** reserving a ticket, preventing
a local interrupt from queueing behind its own interrupted owner or waiter.
Compiler fences surround the masked memory-access interval because the
DAIF helpers use `nomem`; the ticket guard is dropped before the closing
fence and restoration of the caller's exact saved DAIF value. Interrupts are
not unconditionally enabled on return.

This repairs an actual starvation possibility, not a measured reconstruction
of the failed Mac interleaving. FIFO acquisition guarantees order among
queued requests; it neither proves which PE stalled in that run nor bounds
dispatch shares when a peer has not requested service. The six immutable
tasks, 48 yields each, task selection, affinity, dispatch accounting,
ownership checks, minimum 32 dispatches per AP, total at least 288,
`max <= 3 * min`, status/frame assertions, 2-second AP-start and 20-second
completion deadlines remain unchanged. No runtime gate or Firefox threshold
is relaxed.

Additive host coverage is provided through
`scripts/test_aarch64_scheduler_lock.py` in `make unit check`, exercising the
production ticket core and the extracted production `with_state` wrapper.
Nine tests cover a legal old-lock barging interleaving, queued AP/BSP order,
ticket wrap, exclusion/publication, guard release, saved interrupt masks and
the real process-table six-task workload. Four compiled negative controls
reject broken queue order, nonwrapping release, missing IRQ masking and early
IRQ restoration. Host adapters do not execute AArch64 DAIF instructions.
The boot proof and assembly workload are additionally byte-hash guarded
against the unchanged baseline. Both test targets retain warning-as-error
compilation for these generated Rust fixtures.

For the initial FIFO-only candidate on Pi/Debian, full `make unit check` passes in 336.509 s,
including the nine checks and four negative controls in both targets.
`make image-aarch64` and its artifact checks pass in 63.303 s. Logs are
retained under `build/smp-lock-repair-20261014-P1AVcM/`; old boot/kernel and
opt-in images and previous serial logs were copied there before testing.
The independent read-only review found no blocking lock-order or IRQ-order
issue. These host results alone do not qualify guest behavior.
The Mac protocol adds the existing, unchanged
`make test-aarch64-smp-load-runtime` as a focused gate while retaining every
previous sequential command and stop-on-first-failure rule. Pi/Debian
functional evidence must be recorded separately from Mac/HVF qualification.
Firefox remains the priority, but neither its runtime nor full OS completion
is established; original-specification audit Partial/Missing rows remain.

### Local runtime recording boundary

Functional testing uses Pi/Debian, QEMU 10.0.11/TCG, four guest CPUs and the
unchanged Make targets, one guest at a time. The initial FIFO-only boot SHA-256 is
`cdd1f289d4bb6f7782c3daff8ee5c3fd728901299cd64feeebad9af229861ecc`.
The local recorder retains separate command logs, the raw serial files saved
by the unchanged harnesses, PID/harness/QMP arguments, session paths and
live-linked private boot/data/vars; it hashes those files after guest exit.
Its records are in
`build/smp-lock-repair-20261014-P1AVcM/{qualified,continued}/runtime-results.json`.
No local result is Mac/HVF or Firefox performance evidence.

One **unqualified capture interruption** must remain visible: after the first
six gates passed, the local observer aborted during TCP launch with
`expected exactly three private guest disks`. Its rejected process argument
sample and numeric harness status were not retained; no guest result or
failure reason may be inferred from that incomplete attempt. The command log
ends at harness launch. Its original `build/makos-smp-tcp-7em5igv4/` disks
remain, also retained under `capture-interrupted-smp-tcp-private/` in the
evidence directory. No QEMU or harness remained before continuing.

Only the ignored local recorder was corrected to retain process samples and
skip samples without a QEMU drive argument list. A fresh unchanged TCP
command then completed successfully, followed by a passing SMP-input gate
and the failed self-host run described below.
The interrupted attempt is not counted as a pass, no guest assertion
was waived, and no production source or repository gate was changed between
those two launches. Preserve both evidence sets.

The first eight completed gates pass, but self-host **fails** after 98.535 s:
the first Toolchain process (PID 4) exits status 80, followed by
`AArch64 toolchain SMP evidence invalid`. Its load proof passes with
`120,117,175` dispatches. The sequence stops; Native/Python, Firefox-role,
cursor, real Firefox and visible login are not run on this candidate.
QEMU PID 3912049 and harness 3911912 exit; QMP was inherited fd 4 and the
original session was `build/makos-selfhost-focused-kgguhjj8`. Raw serial,
harness output and all private disks remain under `continued/`; their exact
hashes are in its result JSON. This failure is not waived or counted as a
passing sweep.

### Startup race exposed during local validation

`install_loaded_process` previously installed a Ready Toolchain context,
released the scheduler lock and notified APs before `spawn_toolchain` bound
that PID's session credentials. Toolchain leaders, unlike the ordinary
CPU0 session leaders, can execute immediately on another CPU. An early
`SYS_CREATE` therefore sees deny-by-default credentials and can fail the
fixture's initial file-seeding block with status 80. A read-only scan of the
first 1 MiB of the retained data image found no `generated` filename matches;
the exact failing syscall return was not logged, so this is a source-proven
race consistent with the failure, not a reconstructed syscall trace.

A scheduler-locked `startup_ready` admission bit now keeps new Toolchain
leaders ineligible until the original successful credential-registration
step completes. Every ordinary scheduler selection checks the bit before
role and affinity eligibility. A timer or unrelated SGI cannot bypass it;
merely postponing the creator's wake would not suffice. Publication verifies
a Ready, singleton Toolchain context, sets the bit under the same lock, and
then notifies APs. The original registration refusal and `discard_spawned`
cleanup remain unchanged and execute while the child is still unowned.
Existing boot fixtures, CPU0 leaders and thread/fork initialization retain
their prior eligibility behavior. No capabilities, affinity, dispatch counts,
workload, runtime assertions or thresholds change.

The additional `scripts/test_aarch64_spawn_publication.py` compiles the real
spawn, admission, publication and discard routines, production process table,
and actual credential registration/lookup/clearing code. Its six host tests
include a forced AP scheduling lookup before registration and require the
first admitted execution to see exact console/file-write credentials.
Inactive/full sessions, invalid requests and publication states, cleanup, and ordinary role policy
are covered. Three compiled negative controls remove admission, the fresh
wake or reclamation. ELF loading, wake delivery, host locks and resource
reclamation are explicit adapters, not guest execution. Existing structural
assertions and runtime tests remain unchanged.

### Combined-repair validation

Final Pi/Debian `make image-aarch64` and artifact checks pass in 56.289 s;
full `make unit check` passes in 332.499 s. Both new suites run in both
targets: 15 host cases and seven compiled negative controls, retaining all
prior tests. The boot image SHA-256 is
`4bea120305b11b1ad5889f53279d5971b7b816f857e6d7515bf9ba7eecfee963`;
`target/aarch64-unknown-none/release/makos-kernel` is
`758bc6ee63006813b3d74dcc718a7ee17c8ebebce1ddfa9ed66ca0b3fb99d232`.
The evidence directory above retains `06-startup-barrier-image.log`,
`07-final-unit-check.log` (SHA-256
`f77f498d9821e3811edf40ec5ad2d029c65e94f9b1ab53f9e6ff93a556a256b9`)
and `08-final-pre-runtime.txt`. Independent read-only review found no
blocking admission, refusal-cleanup or lock-order issue; review is not a
guest test. Runtime results use a fresh `final/` directory, never replacing
the earlier failed or incomplete evidence.
After the final documentation update, the existing scheduler/provenance
guards and both new host suites pass again; their output is retained in
`09-final-document-and-focused-checks.log`.

The combined-repair self-host run passes in 198.929 s with `cli_builds=20`,
`processes=21`, `runtime_graphs=4,3,2,2,3,2,2,3`, exact generated-header bytes
and hashes, and `spawn_before_wait=3 statuses=42,42,42`. Its locked overlap
record matches PIDs/groups `13,14,15` on CPUs `1,2,3`, roots
`0x4012f000,0x40191000,0x401f3000`, distinct address spaces and singleton
ownership. All 21 placements use APs (`6,6,9`); dispatches are `175,175,178`,
with 30 migrations, source/target masks `0xe`, zero evidence drops, 40 CPU0
compositions for 48 AP deferrals, no pending handoff, and zero GPU delayed
recoveries/timeouts/errors. The saved raw
`final/runtime-selfhost-artifacts/makos-selfhost-focused-serial.log` has
SHA-256 `428756396ddd62cf40b91c273d52b5b94fc6ad8cc40ce9e1871454ed22aa12c9`.
Its original session is `build/makos-selfhost-focused-f6x5axcn`; QEMU PID
3933650, harness PID 3933551, QMP inherited socketpair fd 4. Private files
survive the original cleanup under `final/runtime-selfhost-private/`.

All twelve final sequential Pi/TCG gates exit 0, with no concurrent QEMU,
retry or intervening source change. Times below are local command elapsed
times, not Mac/HVF performance evidence. Dispatch triples are the real boot
load proof from each gate's saved serial, not application-worker totals.

| `make test-aarch64-…-runtime` | Seconds | Load dispatches AP1/AP2/AP3 | QEMU PID | Harness PID |
| --- | ---: | --- | ---: | ---: |
| `smp-load` | 23.595 | 100,100,100 | 3930901 | 3930857 |
| `el0-entry` | 32.471 | 100,99,101 | 3931162 | 3931145 |
| `vm-fault` | 30.962 | 98,101,101 | 3931536 | 3931495 |
| `user-write` | 31.406 | 99,98,100 | 3931843 | 3931827 |
| `btype` | 31.772 | 100,100,100 | 3932187 | 3932155 |
| `net-owner` | 24.118 | 98,101,101 | 3932539 | 3932502 |
| `smp-tcp` | 25.039 | 99,101,100 | 3932753 | 3932711 |
| `smp-input` | 33.350 | 99,99,99 | 3932957 | 3932920 |
| `selfhost` | 198.929 | 92,101,105 | 3933650 | 3933551 |
| `native-smp` | 42.527 | 100,101,99 | 3935467 | 3935434 |
| `production-smp` | 43.760 | 112,104,84 | 3936127 | 3936100 |
| `cursor` | 28.042 | 63,131,109 | 3936613 | 3936570 |

The exact original session paths, command lines and all private image hashes
are retained in `final/runtime-results.json`, SHA-256
`9a9b337ce1b2e9d35b0f9a8aa1e8b160c4f96685df8124d1ce3cec8a7d33e854`.
For each row, command output is `final/runtime-<gate>.log`, raw saved serial
is under `final/runtime-<gate>-artifacts/`, and live-retained boot/data/vars
are under `final/runtime-<gate>-private/`. QMP is an inherited socketpair,
fd 5 for TCP and fd 4 for the other eleven guests. Every before/after master
boot hash equals the final boot hash above; opt-in private boot hashes are
recorded separately. No QEMU or runtime harness remains after the sequence.

The Native/Python-role gate reports three/one automatic migrations with zero
evidence drops and both status-42 reaps. The production Firefox-role fixture
reports four migrations, zero drops, exact syscall-149 post-enqueue handoff
and status 42. These roles do not establish CPython or Firefox execution.
Cursor reports seven positions, zero changed scanout pixels, virtio-GPU plane,
hidden host cursor, and zero delayed recoveries/timeouts/errors. All original
EL0, concurrent VM, cold-buffer, BTYPE, real UDP/TCP and input assertions pass.

The required Mac `5baa9467ca510b8a` Firefox image is absent on this Pi, so no
real Firefox preflight/runtime or post-qualification visible login is run
here. The twelve passes qualify this local repair increment only. Mac/HVF
must run the complete sequential protocol at the exact pushed commit; strict
Firefox's 600-second probe, first-character `<500 ms`, Ctrl-A `<10000 ms`
and all other assertions remain unchanged. Audit Partial/Missing rows remain.

The design and historical evidence below are retained; the dated report
above is the current qualification boundary.

## Saved EL0 instruction validation

The initial main-image placement interval is not the valid resumed-PC range.
Musl's interpreter is mapped at `0x28000000`, and DSOs/JIT text can occupy the
higher mmap arena. `enter_user_context` validates the selected root's mapping
before restoring registers: matching nonzero active/context root, aligned
four-byte user PC, read-only EL0 executable page with AF/PXN and no UXN, and
no parent table restriction denying EL0 access/execution. Root alignment and
non-kernel identity are required before walking tables. Stack/SPSR checks and
the IRQ-masked assembly restore remain unchanged.

An absent translation is distinguished from a denied resident or malformed
descriptor. Only a live process's RX VMA in that exact root can authorize an
absent PC; the ordinary instruction fault later populates it. This handles
post-SVC page crossing and discarded executable pages without scheduler-side
I/O. The metadata lookup takes the VM lock after the caller releases the
scheduler lock; it never faults, allocates, or acquires the scheduler lock.
Signal-handler executable validation remains resident-only and does not take
the VM lock under the TTY lock. Concurrent mapping changes remain subject to
the hardware's permissions and ordinary fault path after ERET.

`make test-aarch64-el0-entry-runtime` boots a fresh private guest and runs the
genuinely dynamic `musl-shared` pthread probe. Three affinity-bound workers
block in high RX code and resume at the next page through outer dispatch.
The first validated high page-boundary entry on each AP emits a bounded
CPU/TID/root/PC record; these are matched to all three workers and their exact
shared mapping before accepting 96 completed calls, status-42 joins and reap.
Because those records are once per AP per boot, use the focused fresh-guest
target for qualification, not a second invocation in the same guest. This is
dynamic-loader/scheduler/VM evidence, not a real-Firefox latency pass. See
[the Mac failure and repair report](FIREFOX-EL0-ENTRY-20260910.md).

Thread-exit clear-child-TID cleanup must notify idle CPUs after waking futex
waiters, just as ordinary futex wake and robust-owner cleanup do. Musl uses
this mechanism to release its thread-list lock; other exiting/joining threads
may be pinned to idle APs. The cleared word and Ready states are published
before the scheduler lock is released and the existing SGI/SEV notification
is sent. No notification is needed when zero waiters woke. Host ordering and
missing-notification negative-control tests cover this path.
Futex wait's word comparison and waiter registration share the scheduler lock
with wake. Sampling before acquiring that lock permits clear-child-TID to
clear/wake an empty queue and then miss a waiter enqueued from the stale
sample. A changed word instead returns the existing `EAGAIN` without blocking.

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
Firefox and ordinary Native process leaders, the shell, and UI/service roles
remain on CPU0, while non-leader Firefox and ordinary native-application
threads retain affinity `0xe` but receive kernel-owned least-reserved AP
preferences. A timer may move each default worker once when its AP exceeds the
least-loaded AP by 64 dispatches measured since that worker was cloned; an explicit affinity request
clears the preference and remains authoritative. Single-threaded guest
compiler, assembler, and linker leaders have a separate `Toolchain` role: the
kernel places each new process on one least-dispatched AP, preferring idle APs
and rotating equal-load ties, then gives it a singleton affinity. Timer
preemption also re-evaluates that process: when the current AP exceeds an idle
candidate by eight dispatches, the scheduler captures the full context, moves
the singleton affinity under the ownership lock, publishes Ready/unowned, and
wakes the target by SGI. Device MMIO remains CPU0-owned. The production
scheduler scope remains bounded; this is not unrestricted desktop SMP
scheduling.

The production gate now records actual simultaneous execution, not only the
set of CPUs used over a process lifetime. Each AP publishes the currently
selected tracked application TID before its active bit; every in-exception yield/block
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

A separate `native-smp` gate runs the same upstream-musl pthread workload as
an ordinary `Native` process rather than using the Firefox scheduler role.
Its leader remains fixed to CPU0, three cloned workers default to affinity
`0xe`, are kernel-placed across AP1-3, and create a natural load imbalance
before any affinity syscall. The timer migrates a default worker through
Ready/unowned with full context and no caller-selected CPU. Each worker then
forces and verifies cross-AP migrations through syscall 148, and
the kernel records exclusive ownership plus a simultaneous distinct-TID AP
interval. This proves the production policy is no longer Firefox-only; the
Firefox evidence counters and markers remain scoped to the launched Firefox
group and cannot be satisfied by the native fixture.

The self-host gate covers automatic leader placement and migration with real
guest work.
Fifteen authenticated toolchain processes compile, assemble, and link the
fixture and tracked repository probe. The current Pi/TCG run initially placed
them `4,8,3`, then made 42 natural timer-boundary migrations with source and
target masks `0xe` and converged to dispatch totals `182,175,178`. Each initial
decision selects a minimum-load AP and an idle AP whenever one exists; each
migration proves an eight-dispatch imbalance, singleton affinity transition,
full GPR/SP/TLS/SIMD context, and Ready/unowned source release. AP
console writes retain their bytes but never submit GPU MMIO: they coalesce a
composition request for CPU0, which the wait/reap path services. The final
proof requires positive owner compositions and AP deferrals, zero pending
handoff, zero dropped migration records, and status 42. APs record bounded
evidence while CPU0 emits it only after child exit, so telemetry cannot split
guest stdout. This policy is kernel-owned and caller-selected affinity is
absent. Firefox/Native workers now use a related preferred-AP policy and
qualify it in their exact-role fixtures. Dynamic balancing remains unqualified
for genuine Firefox on idle macOS/HVF and for built-in/service processes.

That workload also exposed an AP exit/reap ordering race. The process table
could publish a Toolchain leader as Zombie and wake CPU0 while the exiting AP
still advertised the process TTBR0; CPU0's fail-closed teardown correctly
rejected destruction of the active root. Ordinary single-thread exit now
switches the source PE to the kernel root under the scheduler lock immediately
after `exit_current_on`, before parent wake or lock release. A selected
successor installs its own root afterward. The structural gate asserts this
ordering, and the complete 15-process runtime proves every migrated leader is
reaped without active-root destruction.

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

Input delivery now reaches that priority path through the hardware interrupt
controller rather than waiting for the next timer tick. QEMU `virt` maps
virtio-mmio slot `n` to GICv2 INTID `48+n`; the discovered keyboard/tablet at
slots 29/30 therefore use INTIDs 77/78. CPU0 programs each as Group 1,
edge-rising, and CPU0-targeted. A lower-EL entry safely drains the queue and
publishes its wake before EOI. An EL1 entry acknowledges only the transport
edge to avoid recursively acquiring a syscall-held lock, then relies on the
unchanged 100 Hz recovery drain. Focused Pi/TCG QMP Ctrl-A records direct
keyboard INTID 78 delivery before exact surface 7/TID 8 wake and status-42
reap. This qualifies interrupt-driven input on QEMU, not Firefox timing.

Production thread affinity is now explicit kernel state rather than an
adapter-reported constant. Native syscall 148 gets or replaces the mask of the
caller or a same-thread-group TID. Masks must be nonempty subsets of the four
online CPUs. Firefox, native, and Python-role non-leader threads may choose those CPUs;
process leaders retain mask `0x1` because desktop and device service remains
CPU0-owned. Clone gives Firefox, native, and Python-role workers the production default
`0xe` plus a least-reserved AP preference, while forked/new process leaders
begin at `0x1`. Normal scheduler selection consults both the stored mask and
automatic preference; surface priority may bypass the preference but never
the allowed mask. If a caller removes its
current CPU, the exception path captures its complete context, publishes it
Ready/unowned, returns an AP with no eligible successor to its dispatcher, and
sends the scheduler SGI only after publication so an idle target AP rechecks
its Ready queue. A running remote target also rechecks at its next bounded
timer preemption. The musl Linux-number adapter maps
`sched_setaffinity(122)` and `sched_getaffinity(123)` to this native ABI and
fully clears/copies the caller's bounded `cpu_set_t`.

`make test-aarch64-native-smp-runtime` follows its ordinary Native phase with
a separately tagged `python-smp` phase. Both use the freshly built
upstream-musl pthread binary, but the latter is registered with the Python
security and scheduler role and must independently cover AP1-3, prove a locked
Running-owner overlap snapshot with distinct TIDs, migrate automatically,
join, exit, wait, and reap status 42. The current Pi/TCG pass reports Python
dispatches `12997,9286,9224`, placements `1,1,1`, and one automatic migration.
This proves the built-in Python role's scheduler behavior; it is not evidence
that MicroPython or CPython executed this fixture.

The September 4 M3/HVF run passed the Native phase but failed the following
Python phase's required automatic-migration evidence. Two timing assumptions
were unsafe: migration compared boot-lifetime dispatch totals (including the
finished Native group's skew), and the fixture could complete its 4096 yields
between 100 Hz timer interrupts before setting authoritative explicit affinity.
Application clone now records the three AP dispatch counters under the scheduler
lock; automatic migration uses saturating differences from that worker's epoch.
Initial placement/accounting keeps its lifetime totals, the migration threshold
remains 64, and the decision still runs only at the existing timer context safe
point. Each worker retains its one-migration bound; explicit affinity continues
to disable the automatic preference. Forks/new leaders/empty slots have no
inherited epoch. Migration-record `loads` are the worker-epoch dispatch deltas.

The pthread workload retains all 4096 yields and then computes in EL0 through
five guest clock ticks before releasing sleeping peers and setting affinity.
It never reads migration state or requests an automatic target. This gives the
timer-based policy a real preemption interval on HVF without changing any
runtime assertion, harness timeout, or Firefox threshold. The focused test
executes the exact production policy against prior-group load skew, the full
64-dispatch boundary, idle preference, rotated ties, and saturation; it also
executes the exact guest workload with a host clock fixture that advances only
after all yields, including unsigned counter wrap. Runtime requalification on
macOS/HVF is still required for this correction.

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
Hz timer bottom half submits requests; if the IRQ interrupted a direct
CPU0 block operation, the service observes the owner lock and defers one tick
instead of recursing. Low-level ring submission still fails closed off CPU0.
Pi/TCG runtime reports 33 requests/completions: 18 reads, 10 writes and 5
flushes, all 33 serviced by the timer, status 65, exact content, inode cleanup,
and frame balance. The AP request wait remains bounded EL1 `WFE`, not scheduler
block/idle/wake evidence.

CPU0 also drains that same copied-request queue while contending for the VFS,
MakFS4 inode-cache, or MakFS4 mutation lock. An AP can hold one of these locks
across synchronous block I/O; a CPU0 syscall waiting for that lock has IRQs
masked, so a timer-only owner service creates a circular wait. This contention
path enters only the block driver, never the filesystem or scheduler, retains
CPU0 MMIO ownership and the existing device-lock recursion guard, and leaves
the 5,000 ms AP request deadline unchanged. The separate timer completion
counter still distinguishes timer work from contention work. The host
regression executes the production queue/service and filesystem lock bodies
with counter/event/MMIO adapters, requires read/write/flush progress with no
timer, checks three simultaneous AP requests, owner affinity and recursive
device-entry deferral, and reproduces the VFS deadlock when only the contention
hook is removed in its negative control. This is host behavioral evidence;
the reported macOS/HVF parallel self-host failure still needs its unchanged
runtime gate rerun on the fix.

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
parallel EL0 evidence, but it is not evidence of real Firefox overlap.
Separate production-role fixtures use the upstream musl pthread ELF under the
exact Firefox, ordinary native, and Python scheduler roles to exercise clone,
futex, pipe, signal, AP block/wake, join, exit, wait, and reap after the desktop
has initialized. The Firefox-role fixture is explicitly not a substitute for
real Firefox, and the Python-role fixture is not Python execution.

## Next enablement blockers

- Initial and exception-time AP selectors admit non-leader Firefox/Native/Python
  application workers plus singleton-affinity `Toolchain` leaders after
  desktop startup. Firefox/Native/Python leaders, PID1, shell, UI, and service roles
  remain on CPU0. Eligible application threads may select kernel-owned masks
  through syscall 148. Toolchain spawn now performs least-dispatched,
  idle-first AP placement with rotating ties, and timer-safe Toolchain work now
  migrates automatically at a measured eight-dispatch imbalance. Default
  Firefox/Native/Python workers use least-reserved preferred APs and one timer-safe
  migration at a 64-dispatch imbalance. Genuine Firefox on idle macOS/HVF and
  additional built-in/service roles remain gated.
- One AP1-to-AP2 forced migration preserves GPR/SP/TLS/SIMD state and exclusive
  ownership through a Ready/unowned publication. Six load tasks also contend
  through 288 yields on the shared Ready queue and receive 99 dispatches on
  each AP with exclusive ownership. The musl production fixture also forces
  three caller-selected cross-AP migrations and restores its AP-pool masks.
  The Firefox-role and Native fixtures each add multiple kernel-selected
  default-worker migrations with zero evidence drops before those affinity
  calls.
  The real Toolchain workload now adds 42 kernel-selected migrations across
  all AP source/destination masks. Broader production policy/priorities,
  built-in/service balancing, and repeated Firefox/desktop contention remain
  open.
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
Firefox/Native workers and Toolchain leaders and return to WFI when the shared
queue has no eligible task.
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
     select Ready Firefox and ordinary Native application worker threads plus
     kernel-placed singleton-affinity Toolchain leaders. They do not run PID1,
     shell, UI, service, or device-owner threads until those paths are
     qualified for parallel use.
   - Clone/wake/spawn publishes the context with Release ordering then sends an
     SGI/SEV to idle dispatchers. Selection consumes it with Acquire ordering.
   - A BSP-only boot option/fallback remains available until full regression
     coverage passes.

3. Timer and interrupt state
   - Each AP enables its banked GICv2 virtual-timer PPI and programs
     `CNTV_CVAL_EL0` before EL0 entry.
   - CPU0 alone advances global monotonic ticks and services deadlines/device
     recovery polling. AP timer IRQs only preempt/select; otherwise four timer streams
     would make wall time advance fourfold.
   - GICC acknowledge/EOI is per PE. Virtio-input SPIs are edge-rising and
     CPU0-targeted; lower-EL entries drain directly, EL1 entries acknowledge
     and defer, and the 100 Hz poll remains recovery. Virtio-input MMIO and
     virtio-net low-level TX/RX ring service are explicitly CPU0-owned and
     guarded against AP entry.
     AP UDPv4/v6 and TCPv4 use bounded copied-request services; TCPv6 still
     needs equivalent guest runtime qualification. Virtio-blk ring submission is also
     CPU0-only through a bounded copied-request service. CPU0's timer bottom
     half passes real AP VFS/MakFS4 read, write, and FLUSH traffic, while
     recursively interrupted CPU0 ownership defers one tick. Retained graphics
     composition requested by an AP—including Toolchain console output—is
     coalesced and serviced by the CPU0 timer path or the CPU0 toolchain
     wait/reap path; low-level virtio-GPU submissions fail closed off-owner.

4. Address spaces and TLBs
   - TTBR0 is per PE. Same-process Firefox threads may concurrently use one
     root; different processes may use different roots.
   - Mapping mutation holds the VM/page-table owner lock, publishes PTE writes
     with `DSB ISHST`, issues `TLBI ...IS`, then `DSB ISH; ISB`.
   - Address-space destruction is forbidden while any per-CPU active-root slot
     references it. Exit-group first evicts/stops every sibling and waits for
     an acknowledgement mask before teardown.
   - Single-thread process exit retires the source PE's TTBR0 under the
     scheduler lock before the Zombie can become observable to a reaping CPU.
     This closes the AP-exit/CPU0-reap window without weakening active-root
     teardown checks.

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
runtime gate and dynamic balancing plus additional built-in/service roles are
safely scheduled. Toolchain-only automatic balancing does not close those
broader requirements.
