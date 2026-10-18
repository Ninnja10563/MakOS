# Retain unrelated RX frames across synchronous protocol waits

## Completed RX-repair Mac/HVF qualification

Exact `92554836596589a89ceb39737d40e05ae35af35e` completed one fresh sequence
on the same Apple M3/macOS26.6.2/QEMU11.0.3/HVF host. Evidence:
`build/logs/macos-rx-retention-20261018-2d7Wc5/`, including `FINAL-REPORT.txt`,
`MANUAL-REVIEW.txt`, full raw streams and exact per-command statuses/timings.
Unit/check467.52s, image0.86s, all twelve preliminary runtime commands,
image identity and unchanged67-patch/all-five-artifact preflight exit0.
Real Firefox exits2, real153.99s, observer0, at first navigation:

```text
MAKOS_FIREFOX_FIRST_PAINT_OK process_start_to_verified_paint_ms=39072
MAKOS_FIREFOX_SELECTION_LATENCY_OK raw=132 enqueue_to_widget_ms=5905 limit_ms=10000
MAKOS_FIREFOX_INPUT_LATENCY_OK raw=104 enqueue_to_widget_ms=110 limit_ms=500
AssertionError: Firefox navigation did not complete with real page pixels loaded=False changed=False tls_verified=False visual=not checked
```

Pixel comparison was not reached. No captured fatal/failure-detail, retry or
visible login; all QEMU exited. All39 private boot/data/vars were retained and
hashed after exit. The unchanged master/manifest and every runtime boot pre/post
hash match. No browser rebuild, restamp, profile change or weaker gate occurred.
Firefox harness69460/QEMU69468, session `build/makos-aarch64-test-ygm13jg5`,
QMP inherited socketpairfd4. Final raw105896bytes endsLF after shutdown;
SHA256 `31a2cdd7a9db65ab16ebbe7250d64e01fbbd717dfbf9bf036e1f6c078c0bd3d6`.
Numeric QEMU exit status is unavailable; make2 is not QEMU status.

Selfhost verifies20CLI/21processes, exact headers/graphs and three locked
distinct-root status42 children, migrations23/drops0. Native/Python-role/
Firefox-role migration counts3/1/3 all drop0; role fixtures are not browsers.
Cursor reports7positions/0changed pixels/0GPU errors/timeouts. Dedicated
SMP-load final record has all fields but lacks LF; production-role reap tail
truncates after status42. Other boot/prompt tails also remain incomplete as
documented in manual review. No fragments are reconstructed into proofs.

Current channel0x1a records actual successful transaction setup/init/dispatch/
AsyncRead and an after-cache-open state containing transaction/pump, not
downstream completion. Only manager-local startup transaction diagnostic was
captured; missing selected-channel records do not prove nonexecution. Native
connect/DNS/background Mozilla TLS observations lack target-channel correlation.
The RX repair did not resolve strict Firefox. Its original source/host contract
finding remains separate from unestablished browser causality. All assertions,
deadlines and audit Partial/Missing rows remain unchanged.

## Previous completed Mac/HVF qualification

Exact commit `6c4ec7e74be6a2579eb354071002a9647aaca88e` was qualified once
on Apple M3/macOS 26.6.2 with QEMU 11.0.3/HVF. Full evidence remains in
`build/logs/macos-transaction-boundary-20261018-eD66nH/`.
`make unit check` exits0 in450.50s; image exits0 in0.79s. All twelve
pre-Firefox runtime commands exit0, followed by image hash and unchanged
67-patch/all-five-artifact provenance preflight exit0. These are command
outcomes, not permission to reconstruct incomplete physical serial tails.
See `MANUAL-REVIEW.txt` for exact proof counts and capture limitations:
the final SMP-TCP record lacks LF and the production Firefox-role reap tail
is truncated after status42. Complete earlier records remain unmodified.

Real Firefox exits2, real151.23s, observer0, at its first navigation:

```text
MAKOS_FIREFOX_FIRST_PAINT_OK process_start_to_verified_paint_ms=36164
MAKOS_FIREFOX_SELECTION_LATENCY_OK raw=132 enqueue_to_widget_ms=5837 limit_ms=10000
MAKOS_FIREFOX_INPUT_LATENCY_OK raw=104 enqueue_to_widget_ms=107 limit_ms=500
AssertionError: Firefox navigation did not complete with real page pixels loaded=False changed=False tls_verified=False visual=not checked
```

Pixel comparison was not reached; `changed=False` does not measure pixel
equality. No `MAKOS_FATAL` or `MAKOS_FAILURE_DETAIL` was captured. There was
no retry or visible login, and every QEMU exited. All39 private boot/data/vars
files were retained and hashed after exit. Firefox harness PID31789,
QEMU PID31806, session `build/makos-aarch64-test-sv5uy4j2`; QMP used inherited
socketpair fd4, not a filesystem endpoint. Numeric QEMU exit status was not
captured. Final raw serial is108950bytes, including the QEMU shutdown tail,
SHA-256 `7ab4aa550d0cebb027ab816836e8f185c6e2d75c9670b0f9172aea2b90edfcb6`.
All runtime master boot hashes remained
`25a99dc0dd131709c582153dc59295158e81586a85ee8c496f7a3592e796bd96`.

Sixteen complete transaction diagnostics identify PID4/channel0x1a claiming
an existing initializing connection, successful connection admission, pending
insertion and six later only-reused dispatch entries. The returned0x80040111
is the existing `NS_ERROR_NOT_AVAILABLE` pending-queue path, not evidence of a
hard connection error. The claim can be a DnsAndConnectSocket or an active
NullHttpTransaction/TLS connection; records do not distinguish them. Existing
DNS ownership may differ from the selected channel, so missing selected-channel
DNS records do not prove missing DNS execution. A native successful connect
to104.20.23.154:443 is not bound to that channel by captured identity. The
I/O admission repair's source-proven race is not established as Firefox's cause.

## Separate source-supported RX loss

`aarch64_virtio_net.rs` has seven synchronous receive consumers: IPv6 router
configuration, DHCP, NDP, ARP, UDP, TCPv4 and TCPv6 waits. Their original
`poll_receive` advances the used ring, copies a frame and reposts its descriptor
before the caller decides whether the frame matches its protocol/tuple.
Nonmatching frames are discarded. CPU0's ordinary socket pump cannot recover
those bytes later. Socket callbacks cannot safely be added inside the network
state lock: an AP awaiting CPU0 TX service can itself own the socket lock.

Pinned pre-fix reproduction is preserved under
`build/logs/rx-wait-demux-20261018-Og8vSx/prefx-run-sezi4kcr/`.
The runner pins commit6c4ec7e, full network/wire sources, extracts unchanged
UDP/TCP4/TCP6 wait loops and actual RX-ring consumer, and uses the unchanged
wire parsers/builders. Allocated aligned ring/MMIO/buffers are explicit host
adapters. Existing `WAIT_SPINS=20_000_000` is unchanged.

Four positive controls exit0. Nine adversarial cases (three waiters by three
unrelated packet types) reach the desired reply, prove both real descriptors
advanced/reposted, then each exit101 with exactly `CONTRACT_UNRELATED_RX_LOST`.
Compile failure, timeout or another panic cannot qualify. Wrapper/observer
exit0, real1.93/user0.39/sys0.45s. Inputs, extraction, binaries, raw frames,
full outputs, statuses and timings remain preserved. This demonstrates a
production-source contract violation on host adapters, not its occurrence in
the failed guest and not Firefox causality.

Before applying a repair, source refs/worktree were checked, all current fixed
boot/kernel/loader/serial/capture artifacts independently APFS-cloned and
hashed, and the preserved browser master and manifest rehashed. See
`prior-preservation.json` and `preserve-before-kernel.log` in the same directory.

## Bounded repair

A non-Copy deferred queue retains up to16 supported nonempty frames with their
original byte lengths and arrival order. Removal shifts slot indices, not frame
arrays. Normal `poll_frame` drains deferred frames before raw descriptors; a
short output buffer leaves the oldest deferred frame intact. A synchronous wait
first searches saved frames for its unchanged acceptance predicate, then saves
unmatched raw frames rather than losing them. All seven consumers share this
path. Their original spin constants and acceptance conditions remain.

When the backlog is full and has no matching saved frame, the wait returns
`None` without consuming another raw descriptor. This is an early bounded-
resource failure, **not** an unchanged-timeout claim. Runtime normal RX can
drain after lock release; boot uses unpublished state and cannot do so. DHCP/
ARP boot setup can therefore fail at capacity. A full queue cannot guarantee
connection progress, even if a matching hardware completion is next. Existing
TX-request service can also delay normal pumping. This repair does not redesign
those scheduling or resource limits.

Queue storage is24,488bytes on a64-bit layout, excluding other State fields.
There is no new allocator, unsafe access, socket callback or lock ordering.
State is deliberately non-Copy; initialization may still generate stack
temporaries, which require emitted-code review. CPU0/AP stacks are1MiB.
Unsupported oversized/malformed descriptor handling is not redesigned. A
zero-length completion has no frame bytes to retain. Normal socket-level
buffer limits remain. This is not a whole-network losslessness guarantee.

The reviewed private candidate was applied unchanged, SHA-256
`199e4c5ca34dcae2a42456b0c80408e00bb00f66bae79c449686c7d1719de415`.
Its preserved focused regression passes40cases, six compile-clean behavioral
negative controls and one structural negative control, exit0/observer0,
real6.04/user3.32/sys0.75s. Evidence is `fixed-run-_i_fbu09/` under the repair
directory. The runner pins candidate/canonical equality before extraction.

Coverage includes the original four controls and nine loss cases, FIFO drain,
short-output preservation, interior selection/slot reuse, full backlog with
a saved match, full backlog leaving the raw head untouched, UDP output capacity,
unchanged zero/oversized completion handling, twelve independent tuple-field
rejections, and matching/exhausted DHCP/ARP/NDP/router-advertisement paths.
The exhausted-bootstrap fixture sets unpublished state and proves normal
polling cannot drain it. Its explicit host-only publication afterward inspects
retained bytes; it is not successful boot and does not execute configure_ipv4's
fatal caller. Acceptance predicates and low-level dequeue/result bodies have
additional exact structural comparisons against the pinned original source.

Behavioral mutants must compile and fail at their designated assertion with
exit101: lost save, broken insertion order, LIFO drain, wrong selection,
short-buffer dequeue and capacity-bypassing raw consumption. The structural
mutant must reject the changed UDP destination predicate. Compiler errors and
timeouts are not negative-control passes. Real locking, interrupts, TX
completion, guest packet delivery and Firefox causality remain unproved by
these explicit host adapters.

Canonical regression `python3 scripts/test_aarch64_rx_retention.py` also
passes the same40cases/6behavioral/1structural controls, exit0/observer0,
real6.18/user3.28/sys0.83s. Both Make `unit` and `check` invoke it without
removing any existing test. The canonical runner records current HEAD/source
hash rather than permanently requiring the first candidate's entire-file hash;
the historical source/wire pins and every unchanged-body, constant, predicate,
behavior and negative-control assertion remain. External `canonical-inputs.json`
pins all five implementation/test inputs before validation.

Canonical test temporary sources, raw fixture inputs, binaries and per-command
metadata are removed on exit by TemporaryDirectory; the enclosing log retains
full emitted command output/status/timing/provenance. The separately preserved
private pre-fix and fixed runs retain their binaries and full input artifacts.
No guest/private disk or user image is part of that temporary cleanup.

Full unit/check: **PASS**, exit0/observer0,
real466.52/user264.59/sys56.15s. Cross-target warnings remain in the complete
log; this is not a warning-free claim.

Kernel image build: **PASS**, exit0/observer0,
real14.89/user12.28/sys0.43s. Artifact checks pass. SHA-256:

- Boot: `4e59d145467662847fd8ae6b412e9950831590cbfebb012504e6c6a533b2ec4b`.
- Kernel: `f657e5f77543107da9bf1ea5f189d2a87443e44585e3c096802be90e804bad6c`.
- Loader: `d42177fd0223ff7262191925ba447f955f4c61d00958ac7dd206d9f8c3cb6761`.

Optimized-code review is retained in `optimized-code-review/`, including full
old/new symbols, disassembly, exact commands, statuses and binary hashes.
Boot main's frame grows from312,784 to337,680bytes (+24,896); static locked
network state grows from4,512 to29,000bytes (+24,488). Actual State payload is
28,992bytes. Emitted boot code initializes the backlog and makes two full-State
machine copies during construction/publication. Non-Copy source does not mean
zero machine copies. Normal packet paths do not copy the entire State.

TCP4/TCP6 owner frames are1,712/1,808bytes (previous1,696/1,728); DHCP is1,648
(previous1,680). TX service remains1,712, with_state32/48, raw poll32,
SVC71,152 and exception dispatch1,152bytes. Deferred push/take use32/64bytes,
copy at most1,514 packet bytes and at most120bytes of indices. CPU0/AP stacks
remain1MiB. This limited emitted-code inspection is **not** a full callgraph,
recursion or nested-interrupt stack bound.

At publication, exact-commit Mac/HVF qualification was pending. It has since
completed at92554836596589a89ceb39737d40e05ae35af35e with the failure above.
Host passes do not establish Firefox success. No audit row is upgraded.

## Preserved browser input and qualification

Reuse, without rebuilding, reintegrating, restamping or profile changes:

```text
build/makos-firefox-transaction-y4rak151/makos-integrated-8b191c84c05f1999.img
image SHA-256: 8b191c84c05f199917981ecedbba2cc4bf295f7c7ed31c3a4a268412b21d7ef9
manifest SHA-256: 450434abf2fc7f919f12e6ddb99ec1910d28ce356ac353a93b5e772cba19464c
67-patch series SHA-256: c870df920805a85a37f829aea92fb7e51786a0b57d317e95e65aa0dce09fea71
```

After host validation and publication, follow the complete unchanged Mac
protocol at the new exact commit in a fresh evidence directory. Require normal
memory pressure, two sampled intervals at least90% idle, no concurrent QEMU or
heavy builds, Apple clang, fortification and `-Werror`. Stop at first failure;
preserve final raw diagnostics and private disks. Keep the600-second Firefox
probe, first-character<500ms, Ctrl-A<10000ms and all other proofs. Only strict
Firefox success permits the documented private-clone visible login. Historical
Pi evidence and Firefox-role fixtures do not qualify the real browser.
