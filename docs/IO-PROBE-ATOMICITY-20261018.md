# AArch64 I/O probe-to-registration atomicity

## Status and scope

Subsequent exact6c4ec7e Mac/HVF qualification completed once: all preliminary
commands exit0, but real Firefox still fails first navigation after
paint36164ms/Ctrl-A5837ms/first-character107ms. No retry or login; all QEMU
exited. See [latest evidence and separate RX investigation](NETWORK-RX-RETENTION-20261018.md)
and `build/logs/macos-transaction-boundary-20261018-eD66nH/` for retained raw
records, capture limitations and39 private disks. The original race remains
source/host-proven, not established as Firefox's cause. Pending wording below
records the earlier publication state and is no longer current guest status.

The pinned pre-fix reproduction and focused repair regression pass on the Mac
host. The first full unit/check stopped at a fixed architecture-policy digest
in the network diagnostic regression. After explicit baseline review, fresh
full unit/check and image build pass; new guest qualification is not run.
Host interleavings are not Mac/HVF guest or Firefox qualification.
Dates in this evidence directory refer to UTC 2026-10-18.

Working material is preserved under
`build/logs/io-probe-atomicity-20261018-QFamvI/`. The two original kernel files
are retained in `private-candidate/baseline/`; proposed copies are in
`private-candidate/proposed/`. Preparing these copies did not apply the repair
to tracked production source. After the original contract failure was reproduced
and fixed artifacts independently preserved, the reviewed repair was applied.
Its kernel/test inputs are pinned in `candidate-inputs.json` before validation.
Parent commit is `5958c2990e9d2fb86d90fac58f795a1a4703f1bb`, whose supported
Firefox diagnostic release/integration passed separately. Reuse its measured
67-patch image from the Mac protocol; no browser rebuild, restamp or profile
change is part of this kernel repair.

The source review identifies an admissible cross-CPU lost-notification ordering.
It does **not** establish that ordering occurred in the failed Firefox navigation
attempt. A queued socket event, an ignored `PollableEvent::Signal()` result, or
an absent diagnostic record does not establish this as Firefox's cause.

## Original source ordering

The native AArch64 ABI uses `90` for pipe creation, `91` for poll, and `17` for
descriptor writes. In `kernel/src/arch/aarch64.rs`, scheduled `SYS_POLL` calls
`poll_descriptors`, then calls `block_current_for_io` if no descriptor is ready
and the timeout is nonzero. The latter uses wildcard `WaitSource::Any`.

`kernel/src/vfs.rs::poll_events` observes pipe state under the VFS lock. A
successful pipe write publishes bytes under that lock, releases it, and its
syscall path calls `wake_vfs_source`. Both pipe endpoints have a shared object
key from `vfs::io_wait_key`.

`kernel/src/aarch64_process.rs::block_current_for_io_on` originally sets
`io_wait` and publishes Blocked only later, under the scheduler lock.
`wake_io_source` takes that same scheduler lock but skips slots whose `io_wait`
is false. Thus the following source-supported ordering is possible:

1. Reader observes an empty pipe; its VFS lock is released.
2. A different CPU publishes pipe bytes and invokes the matching wake.
3. Reader is still Running with `io_wait=false`; the wake skips it.
4. Reader registers its wait and becomes Blocked, although the pipe is readable.

The separate locks serialize each local operation, not this entire sequence.
Masking local interrupts does not prevent the other CPU from writing. A finite
deadline or another matching/wildcard event can later recover the task; neither
proves the original notification was delivered. Infinite waits have no timeout
deadline. Existing AP idle interrupt handling addresses a different boundary:
sleeping with a task already Ready, not this unregistered notification.

## Bounded repair

Each task gains a separate optional probe source and a boolean notification
latch. The existing wait source and persistent deadline remain separate. There
is no numeric generation counter and therefore no generation-wrap policy.

`begin_io_probe` arms the source under the scheduler lock before the operation
that may find no progress. `wake_io_source` marks matching armed probes even
while their tasks are Running, then retains its existing registered-waiter
wake/count/notification behavior. `consume_io_probe` consumes this admission
under the same scheduler lock, before Blocked publication. An absent or
mismatched admission returns the existing Failed disposition rather than
silently allowing an unprotected wait.

The `#[must_use]` guard owns the entry TID. Exactly one probe may be active per
TID. Success, error, and nonblocking paths drop the guard before signal delivery
or frame-switching exits. Before calling a blocking helper, the caller hands
cleanup ownership to registration. Its later destructor is consequently inert
and cannot erase a newly armed probe after the same task resumes on another CPU.

All twelve potentially blocking call sites are covered:

- Descriptor-targeted: rights receive/send, local socket receive/send, pipe
  read/write.
- Network-targeted: socket `receive_from` and descriptor socket `receive`.
- Wildcard: blocking record lock, poll, epoll wait, and pselect.

Existing object-key lookups are moved before the operations rather than
duplicated. Poll and epoll arm only after their unchanged argument validation,
but before temporary-mask installation and the first interruption check.
Constructors, EMPTY-based reset/reap paths, completion, and exec reset clear
probe state.

No VFS, socket, epoll, TTY, device, or user-buffer operation is added inside the
scheduler critical section. Rechecking descriptors while holding that lock
would risk recursion or lock-order inversion because existing readiness paths
can obtain scheduler identity or invoke device/wake services.

## Deadline and retry contract

Both ordinary/AP registration and the sole-CPU0 path consume the latch under
the scheduler lock. The original deadline initialization and expiry test occur
before a notified-probe retry. A finite deadline is initialized once, not
restarted by repeated notifications; an infinite wait retains deadline zero.

On a notification before registration, the task remains Running and the saved
ELR-minus-four context retries the original SVC through the existing Switched
disposition. No synthetic EAGAIN or success is returned to userspace. If no
notification was recorded, the original blocking path continues. The sole-CPU0
post-admission timer/WFI retry behavior is not redesigned by this increment.

No syscall validation predicate, workload, qualification assertion, compiler,
deadline, or performance threshold is weakened. In particular, this candidate
does not change Firefox's probe, interaction limits, or rendering/network/SMP
requirements. Concurrent descriptor close/rebinding semantics are not newly
qualified by this source review.

## Preserved pre-fix reproduction

`reproduce.py` and `fixture.rs` pin the original process source and the
`makos-process-table` and `makos-readiness` sources. The runner extracts
the byte-identical ordinary/AP registration closure from
`block_current_for_io_on`, together with the complete `wake_io_source` function.
It compiles those actual crates and extracted code with warnings
denied. Host adapters provide a scheduler mutex, selected AP number, context
values, notification count, and a readable bit representing pipe publication.

The controls cover registration before publication, preservation and
expiry of a finite deadline, and an infinite deadline. Two adversarial cases
place publication and wake between an empty probe and registration, for finite
and infinite waits. The contract assertion is deliberately expected to fail on
the original implementation with `CONTRACT_BLOCKED_WITH_READABLE`; the wrapper
requires that assertion and its Rust failure status. A compiler error, timeout,
unrelated assertion, or unexpected successful contract cannot count as a
reproduction.

The runner retains input snapshots, extracted source, binaries,
full stdout/stderr, statuses, timings, and host/compiler details in a unique
run directory. Its exclusive-process guard rejects concurrent guest/build/
integration activity. One run is retained in `prefx-run-v4v3omtb/`:
all compilations exit0; three controls exit0; finite/infinite gap cases each
exit101 at the required `CONTRACT_BLOCKED_WITH_READABLE` assertion. Both
capture Running/ready=1/wake_count=0 before registration and Blocked afterwards.
The wrapper and observer exit0, real5.28/user0.43/sys0.32 seconds.

The readable-bit adapter does not execute VFS byte copying. The deterministic
ordering is not actual concurrent guest execution. This fixture does not execute
the sole-CPU0 WFI path, hardware interrupts, timer service loop, address-space
switching, the SVC context-capture prefix, or Firefox. It cannot establish guest
latency, fix completeness, or browser success.

## Focused repair regression and remaining qualification

`python3 scripts/test_aarch64_io_probe_atomic.py` passes17 deterministic host
cases and six negative controls, exit0/observer0,
real3.94/user1.68/sys0.46 seconds. The two real crates and complete extracted
guard/admission/block/wake/completion bodies compile with warnings denied.
The fixture covers pre-arm readiness; Running-gap notifications; CPU0/AP
registration; post-registration wake; exact/wildcard/unrelated source matching;
repeated notifications; unchanged finite/infinite deadlines and expiry;
cleanup; ownership handoff versus an old guard after rearm; and fail-closed
missing/mismatched admission. All twelve syscall arms and lifecycle resets
receive structural coverage, including drop-before-signal-delivery on the
network-receive endpoint-error path.

Four compile-clean behavioral mutants must each exit101 at their named
assertion: missing Running latch, ignored notification, stale handed-off guard
cleanup, and restarted deadline. Two structural mutants must fail their exact
ordering checks: operation-before-arm and endpoint-error guard surviving signal
delivery. Commands, full captured outputs, statuses and timings are logged.
Host temporary regression binaries are removed by TemporaryDirectory; unlike
the preserved pre-fix run, those ephemeral binaries are not retained.

Clock, CPU/dispatch selection, frame/address-space operations, serial and WFI
remain explicit host adapters. No real interrupts, concurrent CPUs, VFS/device
progress, signal delivery or browser execution is inferred from these tests.
The new regression is added to unit/check; no existing assertion is removed.

First full `make unit check`: **FAILED**, exit2/observer0,
real36.20/user20.61/sys3.41 seconds. It stopped at
`scripts/test_aarch64_network_diagnostics.py:80`:
`AssertionError: original arch policy changed`. The exact whole-file digest
predates this intentional I/O admission repair. No later test/image/QEMU ran.
Preserve `03-unit-check.log` and its stop sentinel. Review must retain exact
whole-file pinning, all existing network assertions/cases/controls, and report
the new architecture baseline separately from unchanged socket parent policy.
The reviewed fixed architecture baseline is now
`e60de711cc5d86631eb3d37dc5c485214a8b034232cc0106af5eb026fc115593`.
The historical erased baseline
`9b0cc57201d224a86569edaeb3e946fa6c3add7ca13df22c702419f452e1d860`
remains recorded, and socket erasure still pins the original parent/hash.
Only the reviewed twelve-arm I/O admission changes distinguish the architecture
baseline; no arbitrary policy edits are ignored. Expected digests remain fixed
literals, not values derived dynamically from the file under test. The existing
connect-policy checks, eight host cases and nine negative controls remain;
an added structural mutant rejects changing the first wildcard probe source.
Network output now reports socket-parent restoration separately from the
reviewed architecture baseline instead of implying both restore the old parent.

New evidence: `build/logs/io-probe-policy-20261018-Gocufi/` retains the original
erasure helpers' measurement, old/new erased-source diff and six exact input
snapshots. Focused network regression passes8cases/10controls, exit0/observer0,
real5.61/user2.40/sys0.66 seconds. Focused I/O regression passes17cases/6controls,
exit0/observer0, real3.82/user1.68/sys0.46 seconds. The historical failed attempt
is not erased, retried unchanged, or relabeled a pass.

Fresh full `make unit check`: **PASS**, exit0/observer0,
real458.43/user258.82/sys55.28 seconds. Cross-target builds report warnings;
they are retained in the log, not described as warning-free.

Candidate kernel image build: **PASS**, exit0/observer0,
real15.05/user12.10/sys0.43 seconds. Artifact checks pass. SHA-256:

- Boot: `25a99dc0dd131709c582153dc59295158e81586a85ee8c496f7a3592e796bd96`.
- Kernel: `642261693965d3eeaa90cc75f7051c0e1848b0f141d1b19c62eac942e5069a0f`.
- Loader: `d42177fd0223ff7262191925ba447f955f4c61d00958ac7dd206d9f8c3cb6761`.

Candidate Mac/HVF qualification and Firefox: **NOT RUN**.
After host/image validation and commit publication, run the complete unchanged
Mac/HVF sequence from a fresh evidence directory, stopping at its first failure.

No visible-login or full-OS-completion claim follows from this work. Existing
audit Partial/Missing rows and historical Pi/Mac evidence retain their scope.
