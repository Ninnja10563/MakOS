# Firefox surface-priority atomicity: Mac evidence and bounded repair

## Preserved first failure

Apple M3, macOS 26.6.2 (25G83), QEMU 11.0.3/HVF, 16 GiB RAM and
Apple clang 21.0.0: exact clean commit
`7baa2c2ce03d4c4757c39e2c0c8e9b3a1f9e7ee8` matched local main,
origin/main and live remote before each command. Unit/check, image and all
twelve sequential runtime gates pass. The exact commands, complete outputs,
timings, hashes and statuses remain in
`build/logs/macos-development-7baa2c2ce03d-V0Oqi3uE/` (`RESULTS.json`,
`EXACT-RESULT-MARKERS.txt`, `FINAL-REPORT.txt`). No assertions changed.

The original integrated image SHA-256 remains
`5baa9467ca510b8a1a207365abed12fe8b03f228432a80b2238b3b46042d00ce`.
Unchanged all-five-artifact/61-patch preflight passes. The real Firefox
command returns make exit 2, real 132.74 seconds, with first failure:

```text
MAKOS_AARCH64_DESKTOP_IDLE_OK accepted_presents=0 cpu_ratio=0.013 wait=wfi dirty_present=1
MAKOS_FIREFOX_FIRST_PAINT_OK process_start_to_verified_paint_ms=89167
AssertionError: Firefox Ctrl-A QMP-to-widget latency exceeded bound observed_ms=26503 limit_ms=10000
```

The raw serial contains JIT, browser.xhtml paint, watcher enqueue,
syscall149 post-enqueue acceptance, CPU0 leader dispatch and raw key132.
It is not a missing-handoff failure. No fatal reason is inferred. The next
first-character and later networking/interaction/survival proofs were not
reached. No visible login was launched, and no QEMU remains.

Prelaunch memory pressure was normal (1); two CPU intervals were 93.50%
and 97.78% idle. During the saved browser sample pressure remained normal,
swap 195.25 MiB, with one QEMU and no observed heavy compiler/build work.
Sampling cannot exclude unobserved brief host activity. QEMU PID 81520,
harness PID 81507, original session `build/makos-aarch64-test-uufv1ykg`,
QMP inherited socketpair fd4. Final raw serial and screenshots remain under
`17-firefox-runtime-artifacts/`; all private boot/data/vars were retained
before harness cleanup under `17-firefox-runtime-private/` and hashed
after QEMU exited. All earlier runtime private disks were also retained.
Older focused harnesses do not drain shutdown serial tails; missing bytes
are not reconstructed. The Firefox harness does drain its stopped pipe.

## Source-proven defects, not a reconstructed failure interleaving

The old priority setter published TID and deadline through separate atomics.
A selector could see the new TID with an old zero/expired deadline, clear
both, and lose the hint before the setter published its deadline. Expiry
also cleared a concurrent refresh without checking its identity.

Separately, selection happened under the scheduler lock, but telemetry
after unlock consumed the hint through a TID-only compare-exchange. A
fresh post-enqueue hint for the same leader could be erased by the previous
dispatch's delayed report. The raw trace is compatible with this ordering
but does not prove it occurred or explains the entire 26,503 ms delay.

The repaired TID/deadline tuple belongs to `SchedulerState`. Publication,
expiry and clear run under its existing IRQ-masked FIFO lock. Both the
explicit priority selector and ordinary scheduler selection consume a
successfully selected hint while locked. Reporting is telemetry-only.
Ordinary blocking/exit dispatch has no common post-unlock hook, so it emits
the same bounded once-only dispatch markers within the locked selection;
otherwise consuming the hint there could lose required evidence entirely.
The reporter never mutates hints or recursively acquires the scheduler lock.
Futex wake already holds the lock and uses the in-state setter to avoid
recursive acquisition. Pending watcher/group identity atomics are unchanged;
this repair does not claim to redesign that separate protocol.

The 1000-tick stale deadline, exact surface targeting, leader/AP affinity,
blocked and foreign-running retention, one-shot dispatch and child fairness
remain. The deadline is not a time slice. No priority is retained across
successful dispatch merely to obtain a passing latency result.

Guest enqueue-to-main values are based on delivered timer ticks; the strict
harness uses host monotonic time. They cannot be subtracted to infer an
unmeasured input or scheduler phase. The pointer-down/release trace also
shows substantial time during or interleaved with earlier pointer handling.
No userspace defect sufficient to justify a Firefox rebuild was established.

## Validation contract

`scripts/test_aarch64_surface_priority.py` retains structural policy guards
and exercises extracted production hint/selection/report code with the real
process-table implementation, explicit host lock/clock/context adapters,
and negative controls. Host checks do not replace runtime qualification.

Final development validation on this Mac passes: focused host regression
11 cases/seven negative controls (exit 0, real 4.03 s), full `make unit check`
(exit 0, real 151.30 s), `make image-aarch64` (exit 0, real 8.55 s).
Complete logs are in `build/logs/surface-priority-repair-20261015-S0Kb7P/`
as `04-final-surface-priority-host.log`, `05-final-unit-check.log` and
`06-final-image-aarch64.log`. Earlier development iterations remain retained
and separately labeled. No QEMU ran during these checks.

Qualify the repaired exact pushed commit in a new evidence directory using
the complete unchanged Mac protocol. Stop at the first failed gate, preserve
raw diagnostics and private disks, then diagnose before any changed-candidate
run. Reuse the original image after exact hash/provenance verification;
do not rebuild or restamp Firefox for this kernel repair. Keep the 600-second
probe, first-character <500 ms, Ctrl-A <10000 ms and every remaining proof.
Only a complete strict Firefox pass permits private-clone visible login.
Historical Pi evidence remains separate; no audit status is upgraded.
