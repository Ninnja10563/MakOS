# HVF blocker repair — 2026-09-06

## Reported Mac baseline

User-reported, not logs inspected on this Pi: commit
`0fb7466822ad7c78e0a4e6ceabbb41fd227176b0`, Apple M3, macOS 26.6.2,
QEMU 11.0.3/HVF. HEAD, origin/main, remote main agreed; clean worktree,
no remaining Mac QEMU. Log directory on the Mac:
`build/logs/macos-hvf-0fb7466822ad-20260904/`.

Failures:

- `01e-make-unit-check.log`: host clang rejected guest ELF section syntax
  in the Mach-O capture harness.
- `06b-make-test-aarch64-selfhost-runtime.log`: fatal block-owner request
  timeout before the three-child parallel success marker, final graph,
  `cli_builds=20`, `processes=21`, and scheduler-lock overlap proof.
  Generated headers passed exact guest readback (164/1,215 bytes, FNV-1a
  `bbbc9068d3d73e49`/`ccf73fc02c2f9ceb`). Migration evidence drops were zero;
  no generated-header, include, preprocessor, or GPU error was reported.
  Boot-image pre/post SHA-256 matched
  `5d0bd6ccbf5370ad724a54054478e40bf0fd9351eaccdc6807ae2aff4fbaf62b`.
- `07-make-test-aarch64-native-smp-runtime.log`: Native completed status 42
  with one migration and zero drops; subsequent Python-role migration was
  absent, triggering the unchanged production balancing assertion.
- `19-cpython-build-makos.log`, `19b-cpython-patch-diagnostic.log`:
  CPython 3.14.7 archive SHA matched, host Python/LLVM prerequisites passed,
  but the obsolete `config.sub` patch rejected its new upstream context.

Passing Mac evidence: Firefox-role production SMP (not the actual browser)
with status 42, and cursor with seven positions, zero changed scanout pixels,
zero errors/timeouts. The fresh release Firefox build emitted
`MAKOS_FIREFOX_BUILD_ELF_OK`, `MAKOS_FIREFOX_BINARY_OK`,
`MAKOS_FIREFOX_PROVENANCE_OK`, and `MAKOS_FIREFOX_BUILD_OK developer=0`.
Identity: Firefox `140.13.0esr@90ad18aabeaa9cbd63a1f749a57f266e758e50da`,
59 patches, series SHA-256
`c922d619398e64b6a162046efde105bc19152a9d868e9a2254ffa701874cc974`,
five audited artifacts. CPython prevented a fresh integrated image, so strict
real Firefox and visible login were **not run**. No bypass occurred.

## Implemented repairs

Code baseline: `23c0842e25d3ee9e1e4883b7c0157b347c55bb1a`
(use the full committed handoff HEAD when testing).

1. `user/aarch64_toolchain.c` keeps `.text._start` on the guest ELF entry and
   omits only section placement under the existing host-test macro. The whole
   harness still executes. `scripts/test_aarch64_toolchain_freestanding.py`
   also compiles it as ARM64 Mach-O without an SDK and checks the production
   ELF symbol's exact entry section. The new compile reproduced the reported
   diagnostic before the fix on the Pi.
2. CPU0 now drains the existing copied block queue while contending for VFS,
   inode-cache, or MakFS4 mutation locks. An AP holding such a lock can need
   CPU0 I/O, while CPU0's waiting syscall has interrupts masked: relying solely
   on the timer creates a circular wait. The new path enters no filesystem or
   scheduler lock, preserves device-lock recursion deferral and CPU0-only MMIO,
   and retains the exact 5,000-ms request deadline. Five host behavioral tests
   execute the production queue/service/lock bodies with hardware adapters;
   removing only the new VFS hook reproduces the deadlock. This is a concrete
   source/test explanation of the reported failure, pending Mac rerun.
3. Each default application worker snapshots AP dispatch totals at clone under
   the scheduler lock. Automatic migration uses saturating deltas since that
   worker's creation, so completed Native work cannot hide new Python load.
   Timer-only safe points, the 64-dispatch requirement, one automatic migration,
   and authoritative explicit affinity remain. The role fixture retains its
   4,096 yields and then performs real EL0 arithmetic for five guest clock ticks
   before explicit affinity. It never reads migration state or chooses an
   automatic destination. Six exact policy tests cover previous-group skew,
   threshold boundaries, idle preference/ties, saturation, and invalid CPUs;
   extracted workload tests cover fast clocks and wraparound.
4. CPython patch context and hunk lengths match the pinned 3.14.7 source.
   A staging-first, zero-fuzz, explicit-direction replay verifies each whole
   file forward or reverse, then checks the actual `config.sub` target identity.
   The old configure-patched/config.sub-unpatched state recovers without source
   reset or deleting `.rej` files. Tests cover all eight per-file publication
   states, idempotence, modes, source drift refusal without partial writes,
   and symlink refusal. Both offline independent excerpts and the three complete
   SHA-verified archive files pass. The [official release archive](https://www.python.org/downloads/release/python-3147/)
   is still pinned to SHA-256
   `3b48dac8fb59f62eaa67ac83c1eb12bda1b7a08406dd286e252c11a66be27f81`.

No existing runtime harness, assertion, timeout, or latency threshold was
weakened. New regressions are added to both `make unit` and `make check`.
The original-spec Scheduler, SDK, Self-hosting, and browser-related rows
remain Partial.

## Pi validation and preservation

Development host: Raspberry Pi Debian AArch64 (`makus`), extracted LLVM 19,
QEMU 10.0.11 Debian `1:10.0.11+ds-0+deb13u1`, TCG (not HVF qualification).
Initial local HEAD, origin/main and remote main matched the reported baseline;
the worktree was clean. The Mac attachment path for the original specification
is not mounted on the Pi; its content remains provided in the user request.

One historical Pi login guest remained: PID 1023121, session
`build/makos-pi-visible-smp-atomic-ngRmkLhJ`, private `boot.img`, `data.img`,
`vars.fd`, `serial.log`, and `qmp.sock`. QMP `quit` stopped it before new tests;
all private files were preserved. No concurrent QEMU is authorized.

Focused CPython archive replay and `ports/cpython/test.sh` pass, reporting
`executable=not-built`. The supported target build still requires host Python
3.14 and target sysroots; the Pi's `python3` is 3.13.5 and no `python3.14` is
installed. This report does not claim a new interpreter binary.
`ports/cpython/test.sh` log:
`build/logs/cpython-patch-replay-hvf-fixes-20260906.log`, SHA-256
`a7109114a5fe547d20afa2b90de015f6b3b0dc6f4719d204a85b056a5b73602b`.
The unchanged `ports/cpython/build-makos.sh` passes fetch/patch and exits 2 at
the absent generator, logged in
`build/logs/cpython-build-hvf-fixes-20260906-pi-prerequisite.log`, SHA-256
`6a2cdc117ab5392c65b027587125c122caeeb2164fde4c8097fd1ba7c188eb20`.
The first `make unit check` invocation passed unit regressions but lacked the
Pi's explicit LLVM archive-tool environment at the AArch64 rebuild. Its failure
log remains `build/logs/full-unit-check-hvf-fixes-20260906.log`; the corrected
environment rerun is recorded separately, never by weakening the command.
The corrected full `make unit check` exits 0, including both architecture
checks and all new regressions. Its 83,206-byte log is
`build/logs/full-unit-check-hvf-fixes-20260906-final.log`, SHA-256
`b877e2224d143a918ced394ea8f6f19e7a3549dcfd3958d530cc99ba9baefe84`.

The unchanged self-host runtime exits 0 on Pi/TCG: eight ordered graph shapes,
20 CLI builds, 21 Toolchain processes, exact generated headers, and
`MAKOS_AARCH64_MAKBUILD_PARALLEL_OK spawn_before_wait=3 statuses=42,42,42`.
The locked parallel snapshot records AP1-3 PIDs `15,13,14` and distinct roots
`0x401f3000,0x4012f000,0x40191000`. Final placements `6,7,8`, dispatches
`178,175,175`, 30 migrations, source/target masks `0xe`, zero evidence drops,
40 owner compositions, 48 AP deferrals, pending zero, and no GPU delayed
recovery/timeout/error. Harness log
`build/logs/aarch64-selfhost-hvf-fixes-20260906.log` has SHA-256
`d7931923ec999aafdf0f06eaff1a779cff7c068173d2df2a1952a744278823da`;
preserved guest log `build/logs/aarch64-selfhost-hvf-fixes-20260906-serial.log`
has SHA-256 `426853b38c407ed9e06fb33ad136d90cad270df53334c854cb02828d31f78809`.

The unchanged Native/Python SMP gate exits 0 on Pi/TCG. Native dispatches are
`7446,12175,7530`, placements `13,2,3`, four migrations; Python dispatches are
`6867,11234,7113`, placements `1,1,1`, one migration. Both roles cover AP1-3,
retain the 64-dispatch requirement, report zero migration evidence drops, and
exit status 42. Recorded overlap masks are `0x6` and `0xa`, respectively;
these pass the existing role gates and are not a claim of three-way overlap.
Harness log `build/logs/aarch64-native-smp-hvf-fixes-20260906.log`, SHA-256
`e66c9ab002827fb068fbee86541dab83b19ff3f3b19d40f0e31317e859d22f82`;
guest log `build/logs/aarch64-native-smp-hvf-fixes-20260906-serial.log`, SHA-256
`6cfe884e5b118f37032f8f56867d353028df4919e0e3deed29a896c66a48fca4`.

The unchanged Firefox-role production SMP gate exits 0 on Pi/TCG, with
dispatches `8274,11387,7648`, placements `4,2,14`, three migrations, zero drops,
overlap mask `0x6`, and status 42. Real keyboard INTID 78 wakes exact watcher
TID 8 on CPU2, skips three unrelated surface waiters, and proves syscall-149
post-enqueue handoff before CPU0 leader execution. This is the upstream-musl
pthread role fixture, not real Firefox. Harness log
`build/logs/aarch64-production-smp-hvf-fixes-20260906.log`, SHA-256
`cd7262560d8338e891dea8ef397daed6f7131d3e2c51f0913d82e0f287b73a53`;
guest log `build/logs/aarch64-production-smp-hvf-fixes-20260906-serial.log`,
SHA-256 `a38c543500416d3d1c3bd0ea926e474082b064c1f230fe0ec059e0ed9d297464`.

The unchanged cursor gate exits 0 on Pi/TCG: seven positions, zero changed
scanout pixels, virtio-GPU cursor plane, hidden host cursor, zero delayed
recoveries/timeouts/errors. Harness log
`build/logs/aarch64-cursor-hvf-fixes-20260906.log`, SHA-256
`3cf09aa2f86550eb6841948e684c813101f58535fc2389b8424c7c79f71abb60`;
guest log `build/logs/aarch64-cursor-hvf-fixes-20260906-serial.log`, SHA-256
`87f9b095978278e9cb4eb5f2831d2f045c308e4e00a9fe199d6c0ea919c690ac`.

Final generated `build/makos-aarch64.img` SHA-256:
`43c713a48344566d7873aae766129bfe20f4eb9aae46adacd5b91ae0ca18b851`.
All four existing runtime harness sources are unchanged from the Mac baseline;
indeed no `scripts/boot_test_aarch64*.py` file changed. Previous serial logs
were copied to `build/logs/pre-hvf-fixes-20260906-*-serial.log` before running
the gates. No QEMU or runtime-test process remains at final local handoff.
These generated images and raw logs are local artifacts, not tracked source;
the committed report carries their paths and hashes. Mac/HVF requalification,
fresh CPython target build, integration, strict real Firefox, and visible Mac
login remain required. No audit row is promoted to complete.

## Required Mac requalification

Preserve the previous logs, release Firefox source/build outputs, accounts,
profiles, package provenance, and working-tree changes. Verify the exact new
handoff commit against origin/main/remote main. Record host/QEMU versions and
ensure there is no other QEMU or runtime test before each gate. Do not run
performance qualification under CPU load, memory pressure, or active builds.

Run unchanged, serially, saving separate logs and exit statuses:

```sh
make unit check
ports/cpython/fetch.sh
ports/cpython/test.sh
ports/cpython/build-makos.sh
make test-aarch64-selfhost-runtime
make test-aarch64-native-smp-runtime
make test-aarch64-production-smp-runtime
make test-aarch64-cursor-runtime
```

Require all existing self-host graph sequences/counts and three status-42
parallel reaps, distinct AP1-3 singleton leaders/roots under the scheduler lock,
all Native/Python/Firefox-role migration records, zero dropped migration
evidence, and all existing GPU/ownership checks. Preserve generated-header
identities and boot-image pre/post SHA. Repeat the self-host and Native gates
on a fresh guest to exercise the fast-HVF timing again.

Then package and integrate through the documented supported paths in
`ports/cpython/README.md`, `ports/firefox/README.md`, and
`docs/INTEGRATED-DATA-IMAGE.md`, preserving the existing Firefox release outputs
only if the unchanged source/provenance/ELF gates authorize them. No manual
stamp creation, preflight bypass, or unqualified image substitution. Once the
required `build/makos-integrated-firefox-handoff149.img` is genuinely available
and the Mac is idle with low memory pressure, run unchanged:

```sh
make test-aarch64-firefox-runtime
```

Preserve every full harness and guest serial log, QMP failure data, screenshots,
package/provenance identities, image pre/post hashes, and exact elapsed results
on failure. Stop the failed guest before another QEMU launch. After qualification,
boot one visible login using private boot/data/vars clones and record PID,
session/service, private data path, QMP endpoint, serial path, and screenshot.
Do not call the full OS complete while the audit contains Partial/Missing rows.
