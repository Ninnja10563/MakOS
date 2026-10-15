# Firefox VM first-fit search repair

## Preserved diagnostic qualification

Exact clean commit `40db5c225d7129ac7990704516e41acd00293037` matched
HEAD/main/origin/main/live remote on Apple M3/macOS 26.6.2, QEMU 11.0.3/HVF.
Unit/check, image, all twelve runtime gates, preserved image identity and
all-five-artifact/61-patch provenance preflight passed. Strict real Firefox
then failed; the sequence stopped without retry or visible login:

```text
MAKOS_AARCH64_DESKTOP_IDLE_OK accepted_presents=0 cpu_ratio=0.013 wait=wfi dirty_present=1
MAKOS_FIREFOX_FIRST_PAINT_OK process_start_to_verified_paint_ms=92030
AssertionError: Firefox Ctrl-A QMP-to-widget latency exceeded bound observed_ms=24215 limit_ms=10000
```

Make exited 2, observer 0, real time 131.94 seconds. QEMU PID37661 and
harness PID37652 exited. Session was `build/makos-aarch64-test-kd5pzmp1`,
QMP inherited socketpair fd4. Final raw serial, screenshots, private
boot/data/vars retained before cleanup, after-exit hashes, full logs and
host evidence are in `build/logs/macos-interaction-trace-20261015-FV7TML/`.
Its `FINAL-REPORT.txt`, `RESULTS.json`, `EXACT-RESULT-MARKERS.txt` and
`MASTER-IDENTITIES.txt` retain exact results and capture-tail caveats.
Launch pressure1, idle94.71%/95.57%; during pressure1, swap195.25MiB,
sole QEMU. No guest remains. Later browser assertions were not reached.

Master boot remained
`eec2fa4856ceab0a93ed2a28c018f049343b34a311e7938ad9246ec8d4b7f331`
before/after every runtime. Preserved Firefox image remains
`5baa9467ca510b8a1a207365abed12fe8b03f228432a80b2238b3b46042d00ce`;
manifest `4cb2b6ef3a7d4c4d852755ce33ebd0cbbbb4de5291b53a3d6dfc7a68fe2a6f6a`.
No browser rebuild, reintegration, restamp or profile edit.

## Evidence and limits

The bounded diagnostic captured exactly64 complete spans, including61
CPU0/TID4 `SYS_VM_MAP_RANGE` (53) calls of23–27ms each, totaling1436ms.
Three AP spans overlap these intervals. They must not be summed into a
disjoint elapsed-time total. The diagnostic then reached its64-record cap;
the overflow marker explicitly does not claim a final dropped-record count.
Later kernel phases are unobserved. No syscall arguments or return values
were recorded, so anonymous/fixed/file/shared branch identity is unresolved.

Source inspection finds a genuine first-fit algorithmic problem: each
overlap advances the candidate by one region, then rescans all8192 slots
for the next overlap. One allocation can inspect O(N times occupied regions)
entries while the VM lock masks local interrupts. Ordinary anonymous maps
perform only this metadata work, not physical allocation or TLB invalidation;
nonfixed file/shared maps also use first-fit. Fixed-map paths differ.
This evidence supports improving first-fit search, not claiming it explains
the entire24215ms delay or excluding other kernel/browser costs.

## Repair contract

Keep earliest-gap first-fit, per-owner isolation, sparse reservations,
protection/W^X policy, capacities, VM locking and fault-generation transactions.
Use allocation-free in-place address ordering followed by one gap traversal,
not next-fit/high-water allocation. Region slots are private; no external
stable-slot identity exists. No heap, region-sized stack scratch or persistent
index/cache requiring invalidation. Preserve checked bounds/overflow and
capacity failure behavior. Existing tests remain; add old-algorithm
differential cases and deterministic work-count coverage, not timing limits.
Register the VM-space crate tests in both `make unit` and `make check`.

The implementation's total bound is O(N log N) for sorting plus linear
vacancy/gap scans. Its deterministic8192-visit test measures the actual gap
helper only, not sorting or whole-syscall latency. Four new cases retain the
six existing tests:512 differential mutation steps against the frozen old
algorithm, explicit earliest holes after splits, capacity/bounds/overflow,
and counted traversal. Independent review found no correctness blocker.

Development validation on this Mac: `cargo test -p makos-vm-space` exits0
(10tests,0.06s), `make unit check` exits0 (160.21s), and
`make image-aarch64` exits0 (9.10s). Logs and independent pre-build artifact
clones: `build/logs/vm-first-fit-repair-20261015-B6CDUx/`. These host/build
results are not browser timing evidence or a runtime qualification.

This is a bounded kernel metadata repair, not a Firefox success claim.
Fresh exact-commit qualification must retain every original gate, workload
and threshold, including the600-second probe, first-character<500ms and
Ctrl-A<10000ms. Reuse the same Firefox image after unchanged hash/provenance
preflight. Stop at first failure, preserve it, and do not retry unchanged.
Only strict Firefox success permits the private-clone visible login.
Audit Partial/Missing rows remain; historical Pi evidence stays separate.
