# Firefox runtime evidence repair

## Current: cold syscall read buffers

The October 10 Mac/HVF qualification of
`73da7fc65cbc54dea4ff215f39c50c7190a1248a` supersedes the original report
below. All twelve pre-Firefox commands passed, followed by the supported
61-patch release build, normal packaging/integration and unchanged provenance
preflight. On Apple M3/16 GiB, macOS 26.6.2, QEMU 11.0.3/HVF, the immediate
prelaunch sample was 95.37% CPU idle with normal memory pressure. No concurrent
QEMU or heavy build ran. The strict Firefox command exited 2 after 617.47 s
(recorder 617.956 s) on
`AssertionError: missing Firefox paint markers: [b'MAKOS_JIT_POOL_OK']`.

The final raw serial has **one browser-document submission**, eight blits,
seven TLS successes and zero JIT-pool records. There is no fatal, failure
detail, network-owner timeout or duplicate mapping. The browser-document
record proves the new submission emitter ran, not completed rasterization or
first paint. No Firefox screenshot was taken because the unchanged harness
waits for the JIT record first. No first-character, Ctrl-A, later interaction
or final visible login result exists. The 600-second probe and every other
gate limit remain unchanged; no retry occurred.

The ZIP `MakOS-73da7fc65cbc-Mac-HVF-evidence-20261010.zip` is rooted at
`macos-hvf-73da7fc65cbc-20261010/`. Its
`19-firefox-runtime-artifacts/firefox-runtime-latest-serial.log` is 59,516
bytes, SHA-256
`b0deb01b649090e58eb3026f7d13df3e8cbed041b4694e6ecd1cb72becd8be6b`.
The archive contains logs/manifests/provenance, not executable or disk bytes.

### Defect and scope

Patch0061 calls `write(2, record, length)` with a static read-only literal.
Musl forwards the pointer to native syscall 17 without first reading its
contents. The kernel previously required every page to have a resident user
PTE before the copy. That rejects a valid but untouched file-backed mapping;
the actual VM fault resolver is not called. A one-shot emitter then consumes
its attempt without writing anything. This is a source-proven syscall defect
with a deterministic host regression, not proof that the particular Mac
literal page was absent: that run did not log its address or write result.

Enabled SpiderMonkey startup must successfully create its JIT runtime and
trampolines through `ExecutableAllocator::createPool` before self-hosted JS
initialization completes. Repository defaults, launch environment and local
generated ARM64 configuration do not disable JIT. However, the Mac ZIP does
not contain effective profile preferences or generated configuration; a
profile override cannot be ruled out by these logs alone. Do not report an
observed JIT allocation failure or generated-code execution from absence or
presence of this record.

`kernel/src/arch/aarch64.rs` now resolves lazy read buffers before the normal
syscall-17 file/TTY, pipe and socket copies and the syscall-63 TTY copy. The
shared helper uses the current process's existing VM resolver, checks an
active root and retains the resolver's overflow, user-address and 16 MiB
copy limits. Every route still checks actual resident EL0 read permission
after population and before constructing the input slice. Population alone
is not authorization: a resident `PROT_NONE` page must remain denied.
File backing I/O occurs outside VM/page-table and descriptor/TTY/serial locks.
The existing generation-checked fault commit, cleanup, current-root isolation,
W^X, capability checks, error conventions and complete-write serial boundary
are unchanged. Concurrent user-buffer unmap after validation remains a broader
existing copy-lifetime limitation; this increment does not claim to solve it.

No Firefox patch, emitter, profile preference, provenance contract, existing
runtime parser, assertion, deadline or latency limit changes. In particular,
there is no retry, assembled fragmented record, substitute JIT marker or host
API fallback. The seven `signal-action-denied` records match profile-lock
handlers, not Wasm's differently flagged signal setup. Resident-only signal
handler validation is a separate known compatibility gap and is not repaired
or established as the cause here. Missing idle service, SQLite errors and the
2,920 ms graphics flush-event delay also remain unqualified follow-up work.

### Regression and next qualification

`scripts/test_aarch64_user_write_fault.py` compiles the production VM,
region table, population helper and resident permission validator with the
existing host physical-page/package adapters. Fourteen cases cover cold
immutable and RX buffers, exact cross-page copying, rejected suffixes without
prefix output, cold/resident `PROT_NONE`, bounds, isolation, I/O failure and
unmap/protect during population. Three compiled negative controls restore the
old resident-only path, omit the final permission check and remove the early
size bound; each must fail its corresponding regression. These are host
adapter results, not guest or real Firefox execution.

The additive dynamic-musl guest fixture passes untouched immutable mmap
buffers directly to musl/native-17 and native-63 TTY writes, writes a cold
cross-page library span to its own exclusively created temporary file and
verifies exact readback, and requires ten invalid-buffer rejections with the
existing native errors. This statically selected built-in fixture has an
explicit `MuslDynamicProbe` credential role: console and file-write only,
still subject to session ownership and VFS permissions. Ordinary Native
applications retain console-only credentials; no caller-supplied executable
can select the fixture's role. It removes only its own file and mappings. The new
`make test-aarch64-user-write-runtime` wrapper retains every original EL0/VM
proof/deadline plus complete new evidence; it cannot manufacture a Firefox
pass. It is included in the sequential Mac protocol after VM-fault.

The existing Mac integrated master is reusable, without Firefox rebuild,
repackaging or reintegration:

`build/makos-firefox-evidence-ymp9Om/makos-integrated-5baa9467ca510b8a.img`

SHA-256 `5baa9467ca510b8a1a207365abed12fe8b03f228432a80b2238b3b46042d00ce`;
matching manifest SHA-256
`4cb2b6ef3a7d4c4d852755ce33ebd0cbbbb4de5291b53a3d6dfc7a68fe2a6f6a`.
Its source/61-patch identity remains exactly as recorded below. Require the
unchanged all-five-ELF, loaded-literal and provenance preflight before reuse.
Preserve the master, all prior release outputs and private profile/evidence
disks; never restamp or change preferences to force the missing record.

Mac boot master/private boot SHA-256 remained
`977185b5793be36c2e68ad867c31cba10eb6872eef2b50a42f8518e662d64496`.
QEMU PID 69471, harness PID 69451, original session
`build/makos-aarch64-test-5lzp0ykz`, inherited QMP fd 4. The harness terminated
QEMU with SIGTERM; numeric QEMU exit status was not captured. Its normal
temporary cleanup removed the original directory, but pre-cleanup hard links
retained boot/data/vars at
`build/logs/macos-hvf-73da7fc65cbc-20261010/19-firefox-runtime-private/makos-aarch64-test-5lzp0ykz/`.
Private data SHA-256 is
`3588374602d911b93975b6cd89ee9e3929d4d1b6cc3c6aed5ff6d92622ed08e1`.
These are Mac paths, not Pi files. No Mac guest remains running. The master
preserves October 9 account/profile state, not recovery of earlier lost data.

Follow the exact pushed commit and [sequential Mac protocol](MACOS-HVF-TEST-AGENT-PROMPT.md).
Rebuild the kernel/guest fixture only. Stop on first failure; only after strict
Firefox passes may a private-clone visible login be launched. Pi validation
is separate from Mac/HVF qualification. All audit Partial/Missing rows remain.

### Local development evidence

Evidence is retained at `build/firefox-cold-write-20261010-PeQpHj/` on the
Pi/Debian development host. The original boot image, kernel, shared libc and
dynamic probe have independent preserved copies. No Mac image is present in
the attached ZIP and no real Firefox runtime is inferred from these fixtures.

Two local setup failures are retained: `02-unit-check.log` passed unit but
stopped at the x86 cross-build because `clang` was absent from `PATH`
(180.842 s); `03-unit-check.log` reached the AArch64 musl audit but its
`llvm-objdump` default still named a Homebrew path (847.324 s). These were Pi
tool-path errors, not guest or Darwin failures. The corrected environment
uses the staged LLVM 19 `bin` in `PATH` and explicit `MAKOS_REAL_CLANG`,
`MAKOS_REAL_CLANGXX`, `MAKOS_LLD`, `MAKOS_AR`, `MAKOS_RANLIB`, `MAKOS_NM`,
`MAKOS_OBJDUMP` and `MAKOS_READELF` from that same installation. Host `cc`/`c++`
remain Debian's defaults; no compiler flags, fortification or assertions change.
The completed static build passes its unchanged audit in 5.031 s
(`04-musl-static-audit.log`). Unrelated host build/I/O pressure contributed to
the development elapsed times; none is a Mac/HVF performance measurement.
The supported shared-musl rebuild/audit also passes in 393.931 s
(`05-musl-shared-build.log`), including the updated dynamic fixture. Rebuilt
libc is byte-identical to the independently preserved prior library:
SHA-256 `757881fd959bc7739496520e074db4327423fda6f2f1398b0fde8e7dbad261e6`.

Before the fixture-credential correction, full `make unit check` passed in
423.400 s (`06-unit-check.log`) and `make image-aarch64` in 115.637 s
(`07-image-build.log`). The first runtime attempt failed before guest boot:
the staged QEMU module directory omitted its final `/qemu` component.
`08-qemu-module-check.log` verifies the corrected virtio-GPU device setup.
The next Pi/TCG run (`qualified/runtime-el0-entry.log`, 52.567 s) passed the
original EL0/VM proofs and both new untouched-buffer TTY writes, then correctly
denied the new fixture's file creation. Its ordinary Native credentials had
only console authority. The process exited 138 and the unchanged gate failed;
later gates were not run. Session `build/makos-el0-entry-ako8hbl_/` and its
private disks/raw serial are retained. The dedicated fixed-fixture role
described above corrects this test setup without broadening ordinary Native
permissions. Both failed attempts and their process/hash records remain
preserved, not overwritten or presented as passes.

Final repaired-source `make unit check` passes in 303.686 s
(`09-unit-check.log`); `make image-aarch64` and its artifact audit pass in
56.631 s (`10-image-build.log`). The new host suites include fourteen
production-VM/read-validation cases with three compiled negative controls,
and twelve evidence/emission/capability-scope tests, including two compiled
emitter controls and five source-mutation permission controls. No existing
assertion, compiler warning policy, deadline or threshold is relaxed.

The sequential final Pi/Debian QEMU 10.0.11/TCG run is **not an all-pass
qualification**. Immediately before launch, sampled CPU idle was 97–99%,
available RAM about 1 GiB, and active swap-in/out zero. Results and private
boot/data/vars hard links are retained under `qualified2/`:

- `make test-aarch64-el0-entry-runtime`: exit 0, 34.424 s. Original three
  AP entries, 96 calls, status-42 joins/reap and VM proof pass. Its raw guest
  serial also contains both real cold TTY writes and the complete new
  `MAKOS_MUSL_USER_WRITE_OK` record: exact cross-page file readback, ten
  denied invalid calls and cleanup. The new strict parser independently
  validates these existing raw bytes (`11-retained-user-write-evidence.log`);
  this is not a second guest run or a standalone new-target pass.
- `make test-aarch64-vm-fault-runtime`: exit 2, 97.036 s, before desktop or
  the dynamic-musl fixture. The unchanged early-boot SMP load proof reports
  `dispatches=48,145,113 total=306 min=48 max=145 statuses=80,81,82,83,84,85
  task_masks_valid=1 free_balance=1`, then
  `MAKOS_FATAL: AArch64 SMP load-balancing proof failed`. The failing bound
  is `145 > 3 * 48`. The unchanged load workload issues yield and exit,
  not writes; this observation does not establish a causal link to the
  cold-buffer repair or prove a host-only cause. No threshold is changed.
- The sequence stops there. The standalone user-write, BTYPE, network-owner,
  SMP TCP/input, self-host, native/production-SMP and cursor targets are
  **not run** in this final sequence. No retry, real Firefox or final visible
  login is performed. No QEMU remains. Mac/HVF qualification is still required.

Successful EL0 session: `build/makos-el0-entry-k3f61c8h/`, QEMU PID 3383130,
harness PID 3383109. Failed VM session:
`build/makos-vm-fault-9jpbt3r6/makos-el0-entry-mselq7jx/`, QEMU PID 3383403,
harness PID 3383389. Both use inherited QMP socketpair fd 4; both original
sessions and private disks are retained. Full argv, exits, hashes and clone
paths are in `qualified2/runtime-results.json`. Serial SHA-256 values are
respectively `f829b1eb60762464c8cdd57274a70e942e8e822c406e721fd6bb991fd279925f`
and `d0a6b081afe86d2050477ef57be22fcb57388c521751e5ae0a3284383da5f93d`.
The rebuilt master boot hash remains unchanged across both runs:
`583d3316ba1f187e959887ad231562daf52c5e38c4b1e89c38f4e939070cc408`;
kernel SHA-256 is
`012b7168d7764804ff5f496e3bea55b07516679deb9909c9107fe8dda48e90c8`.

## Historical: original emitter repair

Everything in this section records the earlier `73da7fc65cbc` emitter
increment and its then-required retest. Those release-build instructions
were completed by the latest Mac run and are superseded by the current
kernel-only repair and verified-image reuse instructions above.

The Mac/HVF run at `44174449e899dfab57625c6fbd14563652d407c7` exhausted
the strict first-paint probe. The supported Firefox patch series contained
neither of the two production emitters required by that gate. Patch0061
repairs this evidence path without changing the parser, pixel checks,
deadlines or latency limits. It does not establish that browser chrome
rendered correctly, or resolve every error in the trace.

A supported Firefox release rebuild, new provenance, packaging and integrated
image are required before the next Mac qualification. Preserve the prior
master, accounts/profiles, build outputs and evidence; do not restamp them.

### Original authoritative Mac result

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

### Historical production changes

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

### Historical regression and build checks

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

#### Earlier Raspberry Pi validation

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

### Historical remaining diagnosis and qualification

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
