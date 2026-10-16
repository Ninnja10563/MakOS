# Firefox navigation-boundary diagnostics

## Follow-up

The release described below was qualified at exact `b57cfe2bf31a`.
Earlier gates pass; Firefox paint42315ms, Ctrl-A4749ms and first-character110ms
pass, then first navigation fails. Synchronous JS/native load returns are now
observed; asynchronous channel completion remains unresolved. See the
[next channel diagnostic and preserved results](FIREFOX-CHANNEL-DIAGNOSTICS-20261016.md).
Historical pending-runtime/reuse instructions below describe62-patch state,
not authority for the new63-patch increment.

## Preserved Mac/HVF failure

Exact `d762db55d5cf5c0992bf6592151bcf401e6b84b4` passes unit/check,
image, all twelve runtime gates, and unchanged 61-patch/five-artifact
Firefox preflight. Real Firefox reports verified paint at 36540 ms,
Ctrl-A at 5242 ms (limit 10000 ms), and first character at 101 ms (limit
500 ms). The next assertion fails at the original first-navigation deadline:

```text
AssertionError: Firefox navigation did not complete with real page pixels loaded=False changed=False tls_verified=False visual=not checked
```

No requested example.com page-stop/TLS proof appears; the gated pixel
comparison is not reached. `changed=False` is not a measured unchanged-pixel
result. Final raw serial contains no fatal/failure-detail record. No retry,
later browser assertion or visible login runs; QEMU exits. Complete evidence,
final raw serial, available pre-failure screenshots and all 39 private disks
remain in `build/logs/macos-priority-migration-20261016-S3Y5tR/`.

Three negative mmap results describe 32 KiB, read/write, `MAP_SHARED`,
fd 48/63 near SQLite storage IO errors. They do not identify the FD pathname
or prove navigation causality. The separate failed 16 GiB anonymous reservation
is not that shared-file mapping. Supporting regular-file shared mappings
would require coherent backing, lifetime, truncate and writeback semantics;
accepting the descriptor or substituting private pages is not a valid fix.

## Bounded diagnostic change

Patch `0062-makos-navigation-diagnostics.patch` adds actual dispatch-boundary
diagnostics, not synthetic page/interaction success markers:

- URL bar: before `openTrustedLinkIn`, after return, and the actual caught
  numeric `ex.result` when available. The original single property read,
  error-page comparison, revert behavior, exceptions and view-close order
  remain. Only MakOS attempts output; no URL, typed input or exception message
  is logged. At most 32 attempts per module, including failed attempts.
- Native `FixupAndLoadURIString`: allowed/denied navigation, actual
  `CreateFromLoadURIOptions` result and load-state presence, original malformed
  URI/error returns, and actual `LoadURI` return. Original branches, calls and
  return values remain; native output is guarded by `XP_MAKOS`.
- Native transport: at most 64 admitted attempts per process, shared atomic
  counter, one checked fd-2 write per attempt, no retry after short/error
  writes, and caller errno preserved. Fixed stage names and numeric results
  fit the fixed stack record. No filesystem, networking or preference changes.

These are diagnostics, not qualification proofs. Added bounded I/O can affect
timing; original strict latency assertions remain mandatory. Native records
can be lost or shortened. JS `dump` is best-effort and can be disabled,
buffered, fragmented or lost. Do not reconstruct fragments, infer that a stage
did not execute solely from missing output, or promote diagnostics to passes.

`ports/firefox/test-runtime-navigation.py` privately applies the relevant
patches to pinned source, compares original/patched real C++ method and JS
boundary behavior, and tests native cap/concurrency/short/error writes plus
JS exception/getter/transport paths. It uses host adapters for Gecko services,
atomics and transport, not browser execution. Thirteen native cases, ten JS
behavior cases and five negative controls run in both unit and check. Apple
clang, fortification and `-Werror` remain. Node.js is needed for host JS tests.

## Release and qualification contract

The initial development attempt was storage-blocked. Supported release source
verification and Rust compilation completed; remaining build was interrupted
by the agent before disk exhaustion (minimum sampled free0.703GiB, about
1.5GiB remaining versus the old2.1GiB libxul). Command exit241,
real1346.93/user887.30/sys228.87; no new release authority or integrated image
was produced. No guest ran in this attempt. All interruption/process/host
records remain beside the complete release log. Existing canonical outputs
are partially rebuilt; do not treat the surviving historical stamp as current
authority. This is not a compiler or browser diagnosis.

User subsequently freed about53GiB. The resumed attempt reverified the original
master/manifest and all five preserved release binaries, then passed full
unit/check including private-copy wiring in210.20s. Its supported release
passes (exit0, all-five audits, truthful62-patch stamp) under
`build/logs/firefox-navigation-resume-20261016-GYJod4/`.
Source tree is157444175e6d07f4860a4dbcb4ea83915249361b. Keep time-p
real31059.07/user5110.88/sys827.00 separately from recorder monotonic7533.546231s;
neither is guest timing. Supported integration passes exit0,
real3804.61/user424.12/sys691.93, and final runtime-image preflight passes
exit0, real0.57/user0.18/sys0.11. Published image:

`build/makos-firefox-navigation-iis0begx/makos-integrated-b5e45cf171c5bc44.img`

Image SHA-256:
`b5e45cf171c5bc44c211fc9b4dfb1d521237ce4ac86fadc362d0010206c47316`.
Sibling manifest SHA-256:
`9bde1add278f3ac0dc95f2f17d8dfb7a56e5c7e6cc6ca1a0bc18ee6a3be70e38`.
All five build/runtime artifacts, package CRCs and current62-patch authority
pass. Full source master/manifest hashes remain unchanged. Preserved metadata
region SHA-256 is0a0818e15ec884b2ee421dbfba9cc1ce3da21d8e3f1bf86d63f3eaa5f07c4b17;
account/profile region is24c7909ea54cacabb8a62b7150d5cc146d96824428f3ddf4dc506b8316034cc2.
Reuse this exact newly published image after unchanged preflight; no redundant
rebuild or restamp. This is release/integration evidence, not a runtime pass.
No QEMU has launched for this increment. Supported packaging removed its
invocation-private temporary staging files on success; an independent candidate
image clone and exact runtime provenance were preserved beforehand in GYJod4.
Original images, old source/release backups and earlier private disks remain.

Exact ordered62-patch series SHA-256:
`3bc2756c89e093968e99481bd390e78d0f8e6aabb8d141b52c65d6117d558326`.
The explicit provenance regression pin is updated to this actual identity;
all previous59/60 rejection checks remain, with61 now also required to fail
build/runtime authority validation. No assertion is removed or relaxed.

This browser-source change requires a new supported release; old 61-patch
artifacts cannot be restamped as 62 patches. Preserve original source, release
outputs/provenance, all images and profiles. Independent APFS copy clones are
recorded in `build/logs/firefox-navigation-release-20261016-UxPYJQ/`; symlinks
are preserved as links. No old artifact is deleted.

Use `ports/firefox/apply-patches.sh`, `ports/firefox/test-widget.sh`, package
manifest/runtime-evidence/navigation host tests and nondeveloper
`ports/firefox/build-makos.sh -j1`. Firefox uses host Python 3.12 and documented
LLVM cross-build prerequisites; OS host tests retain Apple `/usr/bin/cc`.
Require all five build artifact audits, new truthful 62-patch series identity,
and supported `make integrated-data-aarch64` into a unique output directory.
Source image is the preserved `5baa9467ca510b8a` master; its writable account/
profile regions and complete source hash must remain unchanged. Integration
uses host Python 3.14 for CPython, then validates every runtime artifact and
provenance record. Record the newly published image and manifest hashes; do
not manufacture the historical identity or manually populate staging.

Full new exact-commit Mac/HVF qualification remains required: original
unit/check, image and twelve sequential runtime gates, new image hash and
provenance preflight, then unchanged strict Firefox. Stop on first failure.
Require normal memory pressure and two CPU intervals >=90% idle, no concurrent
QEMU/heavy builds. Keep every original proof and limit, including 600-second
probe, 90-second first navigation, first-character <500 ms, Ctrl-A <10000 ms,
all pixels/rendering/network/interaction/survival/SMP/migration assertions.
Preserve final raw serial and private disks before automatic cleanup. Only a
strict Firefox pass followed by QEMU exit permits the private-clone visible
login. This diagnostic is not a Firefox fix, Mac qualification, or an upgrade
of any Partial/Missing audit row.

## Private packaging copies on the space-constrained Mac

Preserving the old release and then constructing ordinary unstripped artifact
snapshots plus another private package tree can consume several additional
GiB. Do not delete old images, caches or evidence to make the next release fit.
`ports/firefox/copy-private.sh` requests Darwin copy-on-write cloning only for
explicit mode-preserving `-p`/`-Rp` copies into absent destinations or empty real
directories. Ordinary forms and existing contents retain ordinary `cp`.
Darwin `cp -c` falls back to copying on unsupported/cross-filesystem targets;
space savings are not guaranteed. Keep disk checks before integration.

This restriction is deliberate: actual Darwin tests found that cloning
without `-p` bypasses the ordinary new-file umask, and cloning onto an existing
destination symlink replaces it instead of following ordinary `cp` semantics.
Both development failures remain negative controls, alongside hardlink and
swallowed-error controls. File/tree hash equality, independent bidirectional
write/truncation, modes, timestamps and symlink behavior are covered. APFS
block sharing is not inferred from inode/size counters.

Use this helper only for the private stamped-binary snapshots and private
package-tree copy. Explicit preservation retains source modes; the outer
invocation-private directory remains mode0700 outside that copied tree.
All canonical-path/alias guards, five-artifact comparisons, source and runtime
provenance checks, strips and atomic publication remain unchanged. A clone
is an independent file, never a hardlink to a mutable input. This host storage
optimization is not an OS/browser substitute or a qualification pass.
