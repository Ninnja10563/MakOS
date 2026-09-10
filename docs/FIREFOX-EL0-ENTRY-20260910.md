# Firefox dynamic-loader EL0 entry repair — 2026-09-10

## Mac/HVF evidence received

The user qualified `1b243548b937aaf8498581c1d7baf2a8eea5ab94` on Apple M3
(16 GB), macOS 26.6.2 (25G83), QEMU 11.0.3/HVF. HEAD/main/origin/main/remote
main matched, the worktree stayed clean, no assertions or thresholds changed,
and QEMU instances never overlapped. The Mac remains the primary interactive
and performance target. These are user-reported results, not logs inspected
on the Raspberry Pi development host.

The report passes full unit/check, CPython fetch/test/build/package, the fresh
release Firefox build with all five ELF audits and 60-patch provenance, and
supported package/integration. Both `plugin-container` and `xpcshell` staged
correctly: the September 8 manifest blocker is qualified on Mac.

Self-host passed twice (175.21/175.26 s): 20 CLI builds, 21 processes, graph
sequence `4,3,2,2,3,2,2,3`, exact generated headers, three status-42 children,
distinct locked overlap roots/CPUs, 23/25 migrations, zero drops/GPU errors,
and matching boot pre/post hashes. Native/Python-role passed twice
(16.77/16.94 s; Native migrations 2, Python-role 1, zero drops).
Firefox-role scheduler/input passed (15.69 s; migrations 2, zero drops,
exact syscall-149 handoff); **that fixture is not Firefox execution**.
Cursor passed (9.92 s; seven positions, zero changed scanout pixels and zero
GPU timeouts/errors).

Real Firefox preflight passed but the runtime exited make status 2 after
16.06 s, before first paint. Host load was approximately 1.3, sampled CPU
92% idle, and memory pressure normal. This was not a latency failure or a
shortened probe timeout. Both AP1 and AP2 reported the same rejected PC:

```text
AArch64 EL0 entry rejected cpu=1 active_root=0x4012e000 context_root=0x4012e000 elr=0x280adc14 sp=0x896463d0 stack_valid=1 spsr=0x0
MAKOS_FATAL: AArch64 EL0 entry precondition failed
```

AP2's stack was `0x896b83d0`; its roots, PC, stack-valid and SPSR fields agreed.
There are no first-character/Ctrl-A measurements or Firefox screenshot from
this run. The original 600-second probe and all latency limits remain intact.

Preserved Mac identities and artifacts:

- Integrated image `build/makos-integrated-c23395ff4644b183.img`, SHA-256
  `c23395ff4644b183991f2508bdd475ad2120110019f134ebd2b5af0c550a12dc`.
- Old tested boot SHA-256
  `c13c8fe8fe8882bedb23a6811c4182210d44cc0d665cb1b47aaddb2ed3acd58f`.
- Evidence directory
  `/Users/marcushuang/Desktop/Desktop/MakOS/build/logs/macos-hvf-1b243548b937-20260909/`:
  `QUALIFICATION-REPORT.md`, `RUNTIME-EXACT-MARKERS.txt`,
  `COMMAND-STATUS-FINAL.txt`, `ARTIFACT-SHA256-FINAL.txt`,
  `23-firefox-runtime.log`, `23a-firefox-live-processes.log`,
  `23-firefox-artifacts/serial.log`, `integrated-manifest.json`, and
  `release-runtime-provenance.json`.
- Failed QEMU PID 80148, harness PID 80136, session
  `build/makos-aarch64-test-1q_2ec48`, QMP inherited socketpair fd 4. Those
  processes exited; the harness removed temporary disks before preservation.
  Master images/provenance and the login-backspace capture remain preserved.
  No final visible login was launched and no QEMU remained running.

## Cause and repair

`USER_IMAGE_LIMIT` (`0x14000000`) is an initial main-ELF placement limit.
Applying it to every saved EL0 PC rejected legitimate pthread clone/resume
inside the dynamically loaded musl interpreter at `0x28000000`, as well as
potential DSO/JIT execution in the higher mmap arena. Root equality, stack,
and SPSR checks did not cause the reported rejection.

The architecture now validates instruction translations in the exact selected
root across the full user range, with four-byte PC alignment. Every page-table
level is inspected. A resident page must have the valid page descriptor,
access flag, EL0 read-only permissions, PXN set, and UXN clear. Privileged
blocks, malformed nonzero descriptors, and parent table restrictions denying
EL0 execution/access are rejected. Writable, execute-never and privileged
pages cannot qualify simply because their addresses are inside an arena.
Zero/kernel/unaligned roots and mismatched active/context roots are rejected
before unsafe context-page walks. Existing stack, SPSR and IRQ-masked restore
checks are retained.

A truly absent translation is distinct from a denied resident mapping. Only
a matching-root, live-process RX VMA can authorize an absent PC (for example,
a post-SVC PC crossing into an untouched executable page, or discarded code).
No allocation or device I/O is performed during validation: the existing EL0
instruction-fault handler checks permissions and populates the page normally.
VMA metadata cannot override an existing NX, writable or invalid descriptor.
Signal-handler validation remains resident-only and acquires no new VM lock.

The dynamically linked `musl-shared` regression retains its original marker
and status 42, and adds three genuine pthread children executing on AP1-3.
It also executes high-address anonymous code changed from RW to RX, blocking
for five ticks with the sleep syscall at a page boundary and returning 42
after resumption. A three-worker ready barrier completes affinity binding
first. Bounded per-AP kernel evidence records the exact TID/root/high-page PC
after successful outer-entry validation, not merely a direct exception ERET.
Each worker completes 32 calls (96 total) and all three joins return 42.
The fixture uses PIC/GOT function pointers so its `pthread_create` address is
the actual musl function, not a low canonical PLT stub. This tests the actual
musl loader, clone, scheduler and mprotect paths; it is not Firefox runtime
qualification. The focused gate is `make test-aarch64-el0-entry-runtime`.

The first Pi execution passed validated high-page resumption on all three APs
but stalled during pthread exit/join. Its last cleanup reported
`MAKOS_CLEAR_CHILD_TID_OK pid=7 ... zeroed=1 wake=2`: the exit path made futex
waiters runnable without notifying their idle CPUs. Unlike ordinary futex wake
and robust-owner cleanup, this path lacked `notify_idle_cpus`. The repair sends
the existing scheduler notification when at least one waiter is woken, after
the cleared word and Ready states are published and the scheduler lock is
released. It does not change affinity, wait policy, deadlines or thresholds.
The failed log `build/logs/aarch64-el0-entry-runtime-20260910.log` and private
session `build/makos-el0-entry-hh8p8esp` remain preserved; QEMU PID 69434 exited.

The subsequent unchanged Native/Python gate passed the Native workload but
stalled joining Python-role workers. Its tail showed TID 27 clearing the musl
thread-list lock with two wakes, then TID 25 clearing it with zero wakes; TID
26 never completed. Review found a second existing race: `FUTEX_WAIT` sampled
the word before acquiring the scheduler lock. A clear/wake of an empty queue
could finish between that sample and enqueue, leaving a waiter registered
against a stale value. The comparison now occurs inside the same locked
operation as registration, serialized against wake; a prior clear is rejected
with the existing `EAGAIN` result. The original failed full log and separate
serial snapshot are retained under `build/logs/aarch64-native-smp-runtime-20260910-el0-entry.log`
and `build/logs/aarch64-native-smp-runtime-20260910-pre-futexfix-serial.log`.
This race is consistent with that trace; the trace alone is not proof of the
precise instruction interleaving. No runtime deadline or assertion changes.

## Local regression coverage

The development host is Raspberry Pi Debian AArch64, Python 3.13.5, staged
LLVM 19, QEMU 10.0.11 (Debian `1:10.0.11+ds-0+deb13u1`)/TCG. It is not the
Mac/HVF qualification host. Initial worktree was clean at the reported
`1b24354`; local and remote main matched, and no QEMU/test process was running.
The original Mac attachment path is unavailable on the Pi; its specification
was supplied in the user request.

`scripts/test_aarch64_el0_entry.py` passes 15 host behavioral tests using the
exact production predicates, allocated four-level page tables, and the real
`vm-space` region implementation. This includes `0x280adc14`, high/last user
instructions, all denied descriptor/permission classes, parent restrictions,
wrong/zero/kernel/unaligned roots, cross-process isolation, lazy RX ownership,
retained stack/SPSR policy, and resident-only signal validation. A generated
negative control restores the old image-range guard and reproduces the
legitimate loader rejection. No production policy is replaced by a mock.

`scripts/test_aarch64_el0_entry_runtime.py` passes 17 evidence-parser tests,
rejecting incomplete calls/joins/reap, wrong loader/RX/resume, missing or
mismatched kernel TID/CPU/root/PC records, missing blocking proof, and fatal
output. Both scripts are included in unit/check. The ordinary full boot test
also requires the added dynamic-musl success marker without removing its old
dynamic loader marker or status-42 reap. The strict Firefox Make recipe is
byte-identical to `1b24354`.

`scripts/test_aarch64_clear_child_tid.py` adds six passing behavioral tests of
the exact production cleanup function with controlled host task/futex/IPI
adapters. They require zero-before-wake, same-root wake-all, consumed
registration, unlock-before-notify, notification only for positive wake
counts, and idempotent/zero/unmapped/missing-task behavior. Removing the
notification in a generated negative control fails as expected. This suite
also runs in both unit/check targets.

`scripts/test_aarch64_futex_wait_atomic.py` adds six passing cases using the
actual production registration prefix and real `makos-futex` queue. A
controlled pre-lock hook clears the word and wakes an empty queue: the fixed
operation must return `EAGAIN` without registering a waiter. Moving only the
read back before locking reproduces the lost wake in a negative control.
The reverse ordering must wake and consume the real registered handle;
root isolation and existing timeout/value/task error results remain covered.
This is deterministic host interleaving coverage, not guest execution.

After the notification fix (before the final futex compare/enqueue fix), the
focused runtime passes on Pi/TCG in private session
`build/makos-el0-entry-ci742d06` (QEMU PID 72286, harness PID 72283, QMP
inherited socketpair fd 4; both processes exited). TIDs 5/6/7 ran on AP1/2/3,
resolved `pthread_create=0x280b0750`, and resumed at `0x80001000` in shared root
`0x4012f000`. All 96 blocking RX calls, three status-42 joins, and the ordinary
status-42 process reap passed. Exit cleanup again observed two woken waiters;
they completed after the notification repair. Master boot before/after and
private boot after all matched SHA-256
`37c653327bfdd683d2e01de475ccf3384fbd45fe3b96d7afcb705cf8aa7df795`.
The session's `serial.log`, `session.json`, and private disks are retained.
Log: `build/logs/aarch64-el0-entry-runtime-20260910-wakefix.log`.

The first full `make unit check` passes (existing compiler warnings remain),
log `build/logs/full-unit-check-el0-entry-20260910.log`, 86,559 bytes, SHA-256
`bf35f96df8ffeacf4a6dad1640b9a437bb368d575b751a46b9509d24fa36fc45`.
Final full source-check evidence is recorded after the runtime results below.

One final-kernel cursor attempt failed before login in the unchanged boot-time
SMP load proof, not cursor rendering or EL0 validation: dispatches
`147,116,39`, total 302, all statuses `80..85`, full CPU/overlap masks, valid
task masks and balanced reclamation, but maximum/minimum exceeded the existing
3:1 bound. The failed log is
`build/logs/aarch64-cursor-runtime-20260910-final.log`; serial is preserved as
`build/logs/aarch64-cursor-runtime-20260910-boot-balance-failure-serial.log`.
QEMU PID 75256 exited. No bound or test was changed. A separate unchanged run
was started after host compilation completed (Pi sampled 96–97% idle,
943 MiB available, load 1.16); retain both results rather than hiding the
failed attempt. This does not establish its precise cause or Mac behavior.

### Final-kernel Pi/TCG runtime results

The following runs exit 0 with unchanged existing runtime gates, sequentially
with no concurrent QEMU. They are functional Pi evidence only:

- Native/Python: Native migrations 3, Python-role migrations 1, both status 42
  and zero evidence drops. Log `aarch64-native-smp-runtime-20260910-futexfix.log`.
- Firefox-role fixture: migrations 4, CPU mask `0xe`, exact syscall-149
  watcher-to-main handoff and status 42. This does not execute Firefox.
  Log `aarch64-production-smp-runtime-20260910-final.log`.
- Cursor rerun: positions 7, changed scanout pixels 0, delayed recoveries 0,
  GPU timeouts/errors 0, virtio-GPU plane and hidden host cursor.
  Log `aarch64-cursor-runtime-20260910-qualification.log`.
- Self-host: CLI builds 20, processes 21, graphs `4,3,2,2,3,2,2,3`, exact
  generated headers, three status-42 children, scheduler-locked parallel
  PIDs/groups `15,13,14` on CPUs `1,2,3` with distinct roots
  `0x401f3000,0x4012f000,0x40191000`; migrations 30, zero evidence drops,
  GPU delayed recoveries/timeouts/errors 0. QEMU PID 76068, harness 76051,
  session `build/makos-selfhost-focused-1qdbhspz`, inherited QMP fd 4; exited,
  temporary disks removed by the unchanged harness, serial snapshot retained.
  Log `aarch64-selfhost-runtime-20260910-qualification.log`.
- Final dynamic-musl repeat: all 96 blocking RX calls and three status-42
  joins pass again, TIDs 5/6/7 on AP1/2/3, loader `0x280b0750`, resumed PC
  `0x80001000`, shared root `0x4012f000`, status-42 process reap.
  QEMU PID 76987 exited 0; private session
  `build/makos-el0-entry-qud36q7r` retains boot/data/vars, serial and session
  JSON including inherited QMP fd 4 and before/after hashes.
  Log `aarch64-el0-entry-runtime-20260910-qualification.log`.

All listed logs are in `build/logs/`. Their SHA-256 identities in the same order:

```text
e3ab03008fb11f69f3a59774090e0915f679fb1701e5f577ee00273020ab878a
91a55b2e72dcfa60af5ec59227063d4c7d4f5ce26b918630d32c42c218253c06
d356c1f41f05a047bcd7f81a1774676c4be9395758f05c4892d91993db428aae
1dd8dcbbc7c311c75194e79d51898fda6c6520319ca27882e422d02d89da3b2f
c9ecabb2ffc6e135f3b0aec137678807d315035ded0b26ba072c9850eb98f455
```

Final Pi boot image SHA-256 (matching before/after self-host and the final
dynamic-musl session's master/private boot checks):
`50af6a7fcb194f75e750326831a70ed8d941467d17e03863b0b823a606e9c857`.
Final Pi kernel ELF SHA-256:
`00ffec753de7cb4bb83389f254ac5c66a2534f17921680933a9133201c83b674`.
The earlier notification-only boot hash is historical, not this final kernel.
No QEMU remained after these runs; no real Firefox or visible Mac login was
run on the Pi.

Final `make unit check` exits 0 after both wake fixes and all runtime runs,
including x86_64/AArch64 cross-checks and all four new suites in both targets.
Log `build/logs/full-unit-check-el0-entry-20260910-release.log`, 89,939 bytes,
SHA-256 `892b53c58ac840999d2d23a11dfb7f9379e99a0114bea2f8aa94c44f32485313`.
Existing compiler warnings and default optional-check skips remain visible;
no check was disabled for this repair. Git diff checks pass. Existing Firefox,
self-host, Native/Python, production-role and cursor Make recipes, the four
existing focused runtime harnesses, and Firefox image verifier are
byte-identical to `1b24354`. The common boot harness only adds the new dynamic
musl marker requirement; no existing assertion or timeout is changed.

## Release and qualification boundary

No Firefox source patch or package-authority rule changes. The qualified
60-patch identity remains
`4f6a84b2ec7c198b5e15b0273fe6931286c2836404ec231056e951d76d46d8fe`.
The existing integrated image above can be reused only after its exact hash
and the unchanged package/provenance/all-five-ELF preflight pass. Rebuild the
boot image with the kernel repair; do not boot the old kernel. A Firefox
rebuild/restamp is not required solely for this kernel/probe change.

Run the exact final handoff commit and the updated
[Mac test-agent protocol](MACOS-HVF-TEST-AGENT-PROMPT.md). Preserve all old
images and logs. Strict Firefox still requires first-character latency below
500 ms, Ctrl-A below 10,000 ms, and every existing paint/interaction/SMP/
survival assertion. Only after Firefox passes and its QEMU exits should the
sole visible login be launched from private boot/data clones with recorded
PID/session/QMP/capture evidence. No original-spec Partial/Missing row is
promoted by this fix.
