# Firefox interaction timing diagnostics

## Preserved Mac failure after the priority repair

Exact clean commit `04bbe5fbf1ab3e190a3ce85a1000a86e46a67d22` matched
HEAD/main/origin/main/live remote on Apple M3, macOS 26.6.2 (25G83), QEMU
11.0.3/HVF. Unit/check, image and all twelve unchanged runtime gates pass;
the preserved image SHA-256 and all-five-artifact/61-patch preflight pass.
The strict real Firefox command then stops the sequence with exit 2:

```text
MAKOS_AARCH64_DESKTOP_IDLE_OK accepted_presents=0 cpu_ratio=0.000 wait=wfi dirty_present=1
MAKOS_FIREFOX_FIRST_PAINT_OK process_start_to_verified_paint_ms=90073
AssertionError: Firefox Ctrl-A QMP-to-widget latency exceeded bound observed_ms=19139 limit_ms=10000
```

Real time is 124.80 seconds; recorder elapsed 125.928753 seconds. Launch
pressure is normal (1), final CPU idle intervals 95.61%,93.19%; during-run
pressure remains normal. No concurrent guest/heavy build was launched.
QEMU PID12464 and harness PID12459 have exited. Session was
`build/makos-aarch64-test-we8w_emy`, QMP inherited socketpair fd4.
All private boot/data/vars were retained before normal cleanup and hashed
after exit. Complete logs, final drained raw serial, screenshots, host
metrics and command/status indices remain in
`build/logs/macos-surface-priority-20261015-BJztz6/`. See its
`FINAL-REPORT.txt`, `RESULTS.json`, `EXACT-RESULT-MARKERS.txt` and
`MASTER-IDENTITIES.txt`. No unchanged retry or visible login occurred.

Master boot stayed
`29b5eacef288b062cb26e4e1477f939feaa6da81596964038eacc33f2a74ffc0`
before/after every runtime. The Firefox master remains
`5baa9467ca510b8a1a207365abed12fe8b03f228432a80b2238b3b46042d00ce`,
with manifest
`4cb2b6ef3a7d4c4d852755ce33ebd0cbbbb4de5291b53a3d6dfc7a68fe2a6f6a`.
No Firefox rebuild, reintegration, restamp or profile edit was performed.

All original self-host proofs pass, including exact generated headers,
20 builds/21 processes, ordered graphs, three status-42 children and one
complete locked distinct-root overlap. Self-host migrations22; Native2,
Python-role1 and Firefox-role2; all evidence drops0. Cursor positions7,
changed scanout pixels0 and GPU errors/timeouts/delays0. These fixtures do
not qualify real Firefox. Earlier SMP-load/TCP focused logs end after their
complete semantic result fields without the final line ending; the unchanged
harnesses do not drain shutdown tails. Missing bytes are not reconstructed.

## What the trace establishes, and what it does not

The browser has real JIT/paint evidence and the complete watcher ARM,
leader DISPATCH, post-enqueue READY and syscall149 acknowledgment. The
priority repair fixed source-proven races but did not qualify interaction.
Do not attribute the change from the earlier 26503 ms to this 19139 ms
measurement causally from two runs.

Widget event delays for move/down/up/Ctrl-A are 360/600/5580/5480 guest-ms.
FIFO enqueue order establishes at least 4980 guest-ms between pointer-down
and release dispatch starts. That interval can include synchronous pointer
handling, intervening Gecko tasks/paints, descheduling or kernel work. Two
accepted blits and a socket connect appear between those records, but lack
thread/duration attribution. Accepted blits do not prove changed pixels or
a compositor flush. No GPU timeout/error/fatal reason was observed.

Guest widget time uses delivered CPU0 timer ticks; host gate time is monotonic
wall time. Masked/delayed IRQs can make those clocks advance differently.
Subtracting the guest value from host latency cannot identify an unobserved
phase. No userspace cause has been established that warrants rebuilding the
browser. Existing CPU0 network-owner service can synchronously wait for a
TCP response, but the trace does not establish that it caused this delay.

## Bounded kernel observation, not a performance fix

The next increment records completed kernel syscall and CPU0 timer-service
spans after the first queued Firefox key. It uses architectural-counter
`uptime_millis`, not serviced-tick time. Records identify the operation,
CPU, available caller TID/root, start/end and inclusive elapsed time. IRQ
spans do not acquire scheduler state to guess a TID; unavailable identity
is labeled explicitly. RAII captures ordinary early returns as well as
successful calls. Spans that never return cannot emit completion evidence.

Observation lasts 30000 ms, records only spans of at least 10 ms, and caps
output at 64 records with explicit overflow evidence. These are diagnostic
sampling parameters, not changed test thresholds or guest deadlines.
After window expiry, later entries stop reading the counter and cannot
rearm; already-admitted spans may still complete. Host regression executes
the production module with clock/CPU/TID/serial adapters: eight cases,
five defect-restoring negative controls, including early-return completion,
concurrent output admission, truthful overflow and no IRQ scheduler lookup.
Inclusive/nested spans must not be added as disjoint CPU time. Instrumentation
has overhead; short, pre-arm, out-of-window or dropped spans are unobserved,
not evidence of absence. No raw proof fragments are joined or synthesized.

Keep the complete Mac qualification sequence unchanged, with a fresh exact
commit and evidence directory. Preserve all first failures and private disks;
no unchanged retry. Reuse the exact Firefox image after normal hash/provenance
preflight. Keep the 600-second probe, first-character <500 ms, Ctrl-A <10000 ms
and all paint/network/interaction/SMP assertions. Only strict Firefox success
permits private-clone visible login. Audit Partial/Missing rows remain.

Development validation on this Mac, before exact-commit qualification:
`python3 scripts/test_aarch64_interaction_trace.py` exits 0 (2.77 seconds),
`make unit check` exits 0 (170.62 seconds), and `make image-aarch64` exits 0
(8.60 seconds). Complete logs are in
`build/logs/interaction-trace-20261015-55393S/`. This validates host logic
and compilation, not guest timing or Firefox success.
