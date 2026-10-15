# Firefox input priority must respect timer migration

## Preserved Mac/HVF failure

Exact clean `36a291a24189ae35a267cb9c0180bff1f83fd5bc` matched
HEAD/main/origin/main/live remote on Apple M3/macOS26.6.2, QEMU11.0.3/HVF.
Unit/check, image, all twelve runtime gates and the unchanged61-patch/
five-artifact Firefox image preflight passed. Firefox then reported:

```text
MAKOS_FIREFOX_FIRST_PAINT_OK process_start_to_verified_paint_ms=48336
AssertionError: missing new interactive marker b'MAKOS_WIDGET_KEY raw=132'
```

The final raw serial, drained after harness cleanup, contains:

```text
MAKOS_FAILURE_DETAIL: AArch64 timer migration retained source ownership
MAKOS_FATAL: AArch64 timer migration retained source ownership
```

This is a real kernel fatal, not an inferred reason for high QEMU CPU usage.
It follows pointer focus, interaction-trace arming and a watcher enqueue.
The diagnostic does not identify the exact failing TID/source/target CPU;
do not invent them. The first harness assertion is missing Ctrl-A delivery,
not a measured latency above the limit. No later browser assertions or
visible login ran. The sequence stopped without unchanged retry.

Evidence: `build/logs/macos-file-failure-20261016-haXs3v/`, full report,
logs/statuses, final raw serial, available PPMs, all39 retained private disks,
source/provenance/hashes and host measurements. Harness16472/QEMU16479,
session `build/makos-aarch64-test-r7a2_rmp`, QMP socketpairfd4, both exited.
Make2/observer0; real244.29s/user904.56s/sys7.81s. Recorder readiness:
pressure1/1, CPU93.95%/95.65% idle. During readings pressure1, swap195.25MiB,
sole QEMU, no heavy build. Screenshots are available harness captures, not
a claimed post-fatal capture. Original private session cleanup is recorded;
retained links preserve all private boot/data/vars bytes after guest exit.

Boot master/private boot, unchanged before/after every runtime:
`64707f751c93ce203599b1ed8491503827374533a90fa76127486e8693897809`.
Firefox master:
`5baa9467ca510b8a1a207365abed12fe8b03f228432a80b2238b3b46042d00ce`.
Manifest:
`4cb2b6ef3a7d4c4d852755ce33ebd0cbbbb4de5291b53a3d6dfc7a68fe2a6f6a`.
Private Firefox data:
`ca3d04012d42ae018142da6a4efaa95f0618c74ec2a28ae24a948c016b1505fa`.
Private vars:
`8b0f58a7457874110d1b2ac5f7ae4509810ed61213bcf29dbf4aaf3cb4eb0aa8`.

The earlier `10eb368` navigation failure remains separate evidence. That
run passed input latency; this fatal does not retrospectively explain its
missing target navigation. This run observes no failed regular-file shared
mapping and does not establish the earlier SQLite IOERR's cause.

## Source-proven transition and bounded repair

While holding the scheduler lock, application timer rebalance selects an
idle target and updates `automatic_cpu`. The process table still marks the
interrupted worker Running on its source until subsequent scheduling releases
that owner. This ordering is intentional: context capture, target publication
and source release occur within one lock transaction.

Two input-priority paths violated that transaction:

1. `select_surface_priority` retained a Running hinted prior task without
   testing CPU eligibility, bypassing ordinary source-owner release.
2. Ordinary selection's hinted-task predicate checked the allowed affinity
   mask but ignored the newly selected automatic CPU. Fixing only the first
   path could still reselect the migrated task on its source.

The existing ownership fatal detects that real contradiction and remains
unchanged. ProcessTable correctly releases the prior owner and selects only
eligible Ready tasks; no core process-table ownership defect is demonstrated.

Input priority now changes ordering, not automatic placement. The common
predicate requires Firefox role, allowed affinity and the selected automatic
CPU for a default non-leader worker. Explicit user affinity remains
authoritative; leaders and workers with no automatic assignment retain their
existing affinity behavior. Both Running retention and Ready activation use
this predicate; the ordinary priority branch already calls the same helper.

When migration makes the source ineligible, the priority shortcut declines
without consuming the hint. Ordinary scheduling releases the old owner and
cannot immediately select that worker again there. An eligible target can
then activate it uniquely and consume the one-shot hint. Blocked/foreign-owner
retention,1000-tick stale bound, FIFO admission, startup barrier, migration
imbalance/count limits, telemetry and every runtime assertion remain intact.

This narrows the former policy that let input hints bypass automatic CPU
preference. That bypass is incompatible with the already-required migration
ownership invariant. It does not disable balancing, suppress its fatal,
pin all workers to one CPU or change any runtime workload/threshold.

Host regression executes production selection and migration logic with the
real ProcessTable. It retains all11 earlier cases and adds5 cases covering
all source APs, source release, unique target dispatch, hint consumption,
automatic/explicit/leader routing and ordinary startup/remote-stop guards.
Ten negative controls retain all7 earlier controls and independently restore
the Running, shared automatic-routing and ordinary-only placement bypasses.
Host adapters supply lock/clock/serial and exclude unrelated post-selection
telemetry/context-switch machinery; this is not guest performance proof.

Development validation on this Mac exits0: focused regression5.65s,
`make unit check`185.41s, `make image-aarch64`9.21s. Full logs, command/status
records, independent prior boot/kernel/loader clones and independent review:
`build/logs/surface-migration-repair-20261016-ZvkXwr/`. Review found no blocking
production/regression issue. No QEMU ran during development validation.

After publishing the bounded repair, run a fresh full
exact-commit Mac/HVF sequence. Keep the original master/profile/provenance,
600s probe, Ctrl-A<10000ms, first-character<500ms and all rendering/network/SMP
proofs. Only strict Firefox success permits private-clone visible login.
No Firefox qualification or original-spec audit upgrade is claimed here.
