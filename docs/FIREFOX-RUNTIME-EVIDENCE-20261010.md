# Firefox runtime evidence repair

The Mac/HVF run at `44174449e899dfab57625c6fbd14563652d407c7` exhausted
the strict first-paint probe. The supported Firefox patch series contained
neither of the two production emitters required by that gate. Patch0061
repairs this evidence path without changing the parser, pixel checks,
deadlines or latency limits. It does not establish that browser chrome
rendered correctly, or resolve every error in the trace.

A supported Firefox release rebuild, new provenance, packaging and integrated
image are required before the next Mac qualification. Preserve the prior
master, accounts/profiles, build outputs and evidence; do not restamp them.

## Authoritative Mac result

The attached October 9 qualification used Apple M3, 16 GiB,
macOS 26.6.2 (25G83), QEMU 11.0.3/HVF, with matching clean Git refs and no
concurrent guest or heavy build. The final prelaunch sample was 97.86% CPU
idle with normal memory pressure. All twelve ordered commands through cursor
passed, including the network-owner regression and unchanged self-host,
EL0/VM/BTYPE, application-role SMP and cursor proofs. The old integrated image
and provenance preflight passed.

The single unchanged strict Firefox command exited 2 after 618.94 seconds;
the probe itself remained 600 seconds. Its first assertion was
`Firefox did not paint browser chrome within probe window`.
The final raw serial contains eight Firefox blits, five TLS successes and
61 accepted presents, but no `MAKOS_JIT_POOL_OK` or browser-document
`MAKOS_PRES_PAINT` record. There are zero fatal, failure-detail, network-TX
timeout and duplicate-mapping records. This is partial activity, not first
paint or an interaction pass. No Firefox screenshot was captured: the harness
waits for JIT-pool and blit evidence before its first Firefox screen capture.
No first-character, Ctrl-A, later interaction or final visible-login result
exists. The failure was not retried.

The archive is `MakOS-44174449-Mac-HVF-evidence-20261009.zip`, rooted at
`build/logs/macos-hvf-44174449e899-20261009/`. Its final serial is
`15-firefox-runtime-artifacts/firefox-runtime-latest-serial.log`, 57,140 bytes,
SHA-256 `fcf0630f2377037bf945df29177f0092ed12f639e0a51f977607f84e219535c7`.
The boot SHA-256 is
`977185b5793be36c2e68ad867c31cba10eb6872eef2b50a42f8518e662d64496`.
The preserved integrated image is
`build/makos-fresh-firefox-VMF8ae/makos-integrated-3b68500032eec2d4.img`,
SHA-256 `3b68500032eec2d4e0a225c44877a070f10de44e3f164cf0700f65bc827ecec3`.
These are Mac artifact identities, not Pi paths or new-release identities.

QEMU PID 24567, harness PID 24561, session
`build/makos-aarch64-test-_h1e_nx0`, inherited QMP socketpair fd 4. The harness
terminated QEMU with SIGTERM after the assertion; the numeric QEMU exit
status was not recorded. The original temporary session was removed, but the
Mac recorder retained boot/data/vars through pre-cleanup hard links. Those
private disks remain on the Mac and are excluded from the compact ZIP.
No QEMU remains and no final visible guest was launched.

## Production changes

`ports/firefox/patches/0061-makos-runtime-evidence.patch` is appended to the
unchanged prior 60 patches for pinned Mozilla source
`90ad18aabeaa9cbd63a1f749a57f266e758e50da` (Firefox 140.13.0esr).
Patch0061 SHA-256 is
`ee67209df6573407beba54326fe1a53784f9ba2fb0773d6012a7716f528c74c3`;
the exact ordered 61-patch series SHA-256 is
`770b7659493b6b6545c64e37243f41982ede10f007f0f03a197aa552e40b3aab`.

- `ExecutableAllocator::createPool` reports `MAKOS_JIT_POOL_OK` only after
  allocation-size validation, real executable allocation, pool-object
  creation and allocator registration succeed. Allocation and registration
  failures retain their original cleanup/return behavior. This proves a
  registered executable pool, not execution of generated code.
- `WebRenderLayerManager::EndTransactionWithoutLayer` reports the actual
  root document only after `WrBridge()->EndTransaction` succeeds. It requires
  a real nonempty display list, a window-paint builder, a nonempty on-screen
  target and an active, unsuppressed document. A call to the void
  `PresShell`/`PaintFrame` path alone is insufficient: downstream display-list
  submission can fail. Background-only, empty, offscreen and rejected
  transactions cannot emit this proof. Exact browser and common-dialog URI
  records are separate; a dialog never impersonates browser chrome.
- `mfbt/MakOSRuntimeEvidence.h` supplies a bounded atomic-once unbuffered
  write per record kind. The actual fd-2 console route validates user bytes
  and holds the kernel serial guard across the entire record and newline
  translation. No stdio fragmentation, retry or joining of partial writes
  is used. A synthetic short/error transport returns failure and consumes
  its attempt; it cannot retract bytes already delivered by that transport.
- The shared ELF audit now requires both literals in readable, file-backed
  `libxul.so` load segments. Release build, package, integration and pre-QEMU
  verification use that same contract. Strings in debug/unloaded data or
  another artifact cannot satisfy it. Presence is only a necessary build
  capability; source/provenance and all runtime checks remain mandatory.

The historical `MAKOS_PRES_PAINT` name is retained for the existing gate. Its
precise meaning is successful document display-list submission, not completed
rasterization, scanout or usable browser pixels. The unchanged blit and
framebuffer checks supply independent pixel evidence; no success marker is
emitted merely because the process started or a timeout expired.

## Regression and build checks

`ports/firefox/test-runtime-evidence.py`, included in `make unit check`,
checks the production bodies with host adapters and negative controls.
Use `--source-dir build/ports/firefox/source` to apply the new patch to exact
pinned source files in a private tree, without modifying the existing
checkout, object cache or provenance. `--evidence-dir` retains private test
inputs/results. Host models are not Mozilla or guest runtime execution.

`scripts/test_integrated_data.py` tests actual synthetic ELF bytes across
release-build audit, self-hashed package/image preflight and integration.
It rejects each absent literal, another artifact's strings, unloaded trailers,
unreadable loads and a string split across separate mappings. A contiguous
literal crossing the bounded reader's chunk boundary remains accepted.
Existing CRC, hash, ELF, dependency, provenance and preservation tests remain.
Synthetic binaries test validators only and are never used as Firefox.

For configured source/object inputs, the optional real-unit cross-build is:

```sh
python3 ports/firefox/test-runtime-evidence-compile.py \
  --source-dir build/ports/firefox/source \
  --obj-dir build/ports/firefox/obj-aarch64-makos-developer \
  --evidence-parent build/firefox-evidence-repair-20261010-wxsur4
```

Use the documented `MAKOS_REAL_CLANG`, `MAKOS_REAL_CLANGXX` and `MAKOS_LLD`
when the configured driver needs explicit host tools. This compiles the two
actual patched Gecko translation units with generated MakOS headers/backend
flags in a private directory, verifies AArch64 relocatable objects and their
emitter literals, and retains commands/hashes. It does not link a release,
authorize packaging, generate provenance or execute a guest.

### Raspberry Pi validation

Local checks run on Raspberry Pi/Debian Linux AArch64, not macOS. The focused
production-body suite passes 27 host cases and eight compiled behavioral
negative controls, with exact pinned-source private patch application.
Artifacts are retained at `build/firefox-runtime-evidence-host-20261010-002/`.
Final patch-context formatting is rechecked in
`build/firefox-runtime-evidence-host-20261010-003/`: 27 cases
and eight controls pass, and all four applied source/header files compare
byte-identically with the earlier host and actual cross-compile inputs.
The same 27 cases/eight controls also pass with Linux LLVM clang 19.1.7 in
33.794 seconds, retaining `-Wall -Wextra -Werror -O2`; log and artifacts are
`build/firefox-evidence-repair-20261010-wxsur4/05-clang-host.log` and
`clang-host/`. The ordinary host suite uses Debian GCC 14.2.0. Neither run
is an Apple clang/Darwin SDK result.
The central integrated-data and provenance regressions pass, including
explicit rejection of both old 59- and 60-patch stamps.
Final full `make unit check` exits 0 in 309.251 seconds, including both architecture
kernel/UEFI cross-checks and the new emitter suite in each target. Complete log:
`build/firefox-evidence-repair-20261010-wxsur4/07-final-unit-check.log`.
The preceding run also passed in 279.218 seconds; its `03-unit-check.log` is
retained. The offline `ports/firefox/test-package-coherence.sh` regression
passes with staged-byte mismatch, mutation, alias, interrupted-publication
and prior-image-preservation controls; log `08-package-coherence.log` is in
the same evidence directory. Its isolated mocked build fixtures are not a
real Firefox package or runtime.

The real Gecko unit cross-build passes with LLVM clang 19.1.7 and the existing
generated MakOS configuration in 88.180 seconds. Both objects are AArch64
ELF relocatables with the required emitter literals:

| Translation unit | Object bytes | SHA-256 |
| --- | ---: | --- |
| ExecutableAllocator | 117688 | `32fe391fd73388f653fa61505c439a67cab8a22b4f87678a01ec4b8c4040b521` |
| WebRenderLayerManager | 892984 | `f0e906d023b561b22d9d17bdf356cc90eecc515b5a56f07cef9f60a8cd2f4525` |

The compile log is
`build/firefox-evidence-repair-20261010-wxsur4/02-target-compile.log`;
commands, exact private sources, objects and hashes are in its
`gecko-units-po5uhkm1/` sibling. The original source/object caches were not
edited. An initial helper run failed to find the copied WebRender unit's
adjacent header; restoring the original quoted-header search path in the
helper fixed that compile setup. Its failed log/private tree are retained as
`01-target-compile.log` and `gecko-units-2ge01cuk/`. The existing libc++ warning
that it supports Clang 20 and later remains visible; no warning flags were
relaxed. Host-adapter tests retain global `-Werror` and default fortification.

The preserved old developer `libxul.so` (SHA-256
`71d5af3f4dee2ddf17bcb3803ee3aa42cd17aef7ce465a6fddde23dbf86b5fd5`)
now fails the real binary audit specifically for both absent emitter
literals. This is an expected negative control, not a new build failure or
permission to restamp it. Log: `04-old-binary-negative.log` in the same
evidence directory. Local boot and release-kernel hashes remain respectively
`f712c55db34855535e004ffc2890aee0c12f1e3dcae948020a9cdb07ff35022b`
and `5e42ef7cd3e3d359f895b320dfcb3616ecaf332a03b5c082bb29bd993b914b22`.

No QEMU, guest runtime, full Firefox release/link/package or integrated-image
publication was performed for this increment on the Pi. The kernel and
existing runtime harnesses are unchanged. The 61-patch browser still requires
the supported Mac release and full sequential qualification; neither these
object files nor an older developer binary is a substitute.

## Remaining diagnosis and qualification

The trace reports a 15,200 ms renderer-event delay, an 8,010 ms flush-event
delay, SQLite disk I/O error, missing `@mozilla.org/widget/useridleservice;1`
and IndexedDB `UnknownError`. None is attributed as the observed timeout's
cause. Source review also identifies follow-up questions about I/O readiness
registration, timer elapsed-time accounting and SQLite allocation capability.
Those paths are not changed in this bounded evidence repair. In particular,
this run does not prove the network-timeout class permanently fixed merely
because it did not recur.

Follow [the sequential Mac testing prompt](MACOS-HVF-TEST-AGENT-PROMPT.md)
at the exact pushed commit supplied in chat. Preserve the prior five release
outputs and package auxiliaries before supported tools refresh them. Build
with developer mode unset, require all five artifact checks, current 61-patch
provenance and normal Mozilla staging, then publish a new integrated clone
without modifying the intended source data image. Old 59/60-patch provenance
must remain rejected. No manual stamps, executable copies or preflight bypass.

Run the exact strict Firefox target on an idle Apple Silicon Mac with normal
memory pressure and no other QEMU or heavy build. Keep 600 seconds,
first-character <500 ms, Ctrl-A <10,000 ms and every other assertion/deadline.
Stop on the first failure, retain final raw evidence/private-session details,
and do not retry or launch final visible login. Only after all gates pass
launch the sole visible guest from private clones and record PID/session/data
clone/QMP and login capture. Pi evidence is not Mac/HVF qualification.
All audit Partial/Missing rows remain; no full OS completion is claimed.
