# Firefox first-channel continuation state diagnostics

## Preserved exact Mac/HVF result

Commit `f666a8a5cb849427f6d89d9ef60375a5bcb82da0` passes unit/check,
image, all twelve pre-Firefox runtime gates, exact65-patch image/manifest and
unchanged all-five-artifact provenance preflight. Strict Firefox then fails
first navigation. No retry, later browser assertion or visible login follows.
QEMU exits; all39 private boot/data/vars files remain, hashed after exit.
Evidence: `build/logs/macos-network-boundary-20261017-UvBwf1/`.

```text
MAKOS_FIREFOX_FIRST_PAINT_OK process_start_to_verified_paint_ms=46312
MAKOS_FIREFOX_SELECTION_LATENCY_OK raw=132 enqueue_to_widget_ms=6391 limit_ms=10000
MAKOS_FIREFOX_INPUT_LATENCY_OK raw=104 enqueue_to_widget_ms=110 limit_ms=500
AssertionError: Firefox navigation did not complete with real page pixels loaded=False changed=False tls_verified=False visual=not checked
```

Make exits2, observer0; real161.58/user551.90/sys23.12,
recorder163.561064s. Pixel comparison is not reached; `changed=False` is not a
measured unchanged-page result. Final raw serial is102160 bytes, SHA-256
`8b0697bc520507736d27bde17f8aa7cd10b652f62bf0a793c6e9924a04a54ade`,
including final SIGTERM cleanup tail. No captured `MAKOS_FATAL` or
`MAKOS_FAILURE_DETAIL`; no missing fatal reason inferred. Sealed report SHA-256
`07a1b2e9ebbb433d420a7479b35aa7b86837aa0feac560a2487d73aa18ba85eb`.

Firefox harness PID53991, QEMU PID54006, session
`build/makos-aarch64-test-g15tt04x`, inherited QMP socketpair fd4, not a reusable
filesystem endpoint. Retained private files under
`17-firefox-runtime-private/makos-aarch64-test-g15tt04x/`:

- boot: `7004e312eb2ff50f6a80bdf8bebef8eb0bd947cea94fa1784d5cc0e94e85b169`
- data: `8ad2e3fcdb1493c47755566f9df1b3f0d2cc3e9f7f9f684375ee43ab2f137d2b`
- vars: `75924f12f373799a3c83cbb593af4833634a0b19869273dd9883266e4fd984ec`

All13 runtime master boot pre/post hashes match. Self-host20 builds/21 processes,
ordered graphs4,3,2,2,3,2,2,3 and exact generated headers pass; one locked
parallel snapshot has CPUs1,2,3, distinct PIDs/groups14,15,13, roots
`0x4018f000,0x401f0000,0x4012e000`, three status42 children. Migrations22,
drops0, owner compositions37/AP deferrals48, pending0, GPU errors/timeouts0.
Native/Python-role migrations2/1, Firefox-role3, all zero evidence drops.
Role fixtures are not application execution. Cursor positions7/changed pixels0.
Older focused harness tail limits are recorded in `EARLY-GATE-REVIEW.txt`;
unavailable LF/shutdown bytes are not reconstructed. Required contiguous EL0,
VM, user-write, BTYPE, generated-header and locked-overlap records are retained.

Final Firefox launch accepted91.30%/94.74% idle samples with pressure1.
No apps/services were paused or closed; T3 remained open. Pre-navigation
screenshot shows New Tab and empty focused address field, not target pixels.
`VISUAL-REVIEW.txt` records inspection after original report sealing.

## What native boundary evidence establishes

Six complete native connect attempt/result pairs, traces0..5, each result1,
all before target typed navigation. No captured connect/poll denial or
saturation notice. These native IDs/IPs do not identify a Gecko channel or
hostname; absence of a later record does not prove nonexecution.

The real bridge identifies load0x17/channel0x1f in this run, not prior0x1a.
128 complete load diagnostics, zero malformed records, establish target
classifier lookup/completion/main callback, proxy callback and Gecko `Connect`
entry, then actual nested NS_OK returns. They do not distinguish cache wait,
tail wait, DNS promise or accepted transaction dispatch. The128-record budget
is consumed by target plus unattributed channels; missing later callbacks are
not evidence of permanent callback loss. No behavioral network/cache repair
is justified by this run alone. Separate socket ABI limitations remain
uncorrelated, not its root-cause diagnosis.

## Next bounded observation, not a browser fix

Append supported patch0066 for first-channel continuation state. Latch only
the first nonzero channel supplied by existing known top-content HTTP(S)
metadata; never reset/rearm on another load. New state diagnostics use a
separate bounded32-attempt budget and one saturation notice, leaving existing
128-attempt load diagnostics unchanged. Failed/short writes consume attempts;
one native write, errno preserved, no retry or final invented dropped count.

Observe actual cache-open return/wait bits, cache callback and wait clearing,
network trigger, proxy/DNS waits, suspension and transaction setup/initiation/
read returns. Record immutable channel ID, raw statuses and scalar state;
entry-neutral metadata is explicitly distinct from actual results. No URI,
hostname, preference change, forced callback or predicate bypass. After-state
access is allowed only where original code retains a live object. Selection
is first-channel-only, not redirect/subchannel or whole-browser coverage.

Patch0066 is implemented and independently reviewed, not yet browser-qualified.
Its SHA-256 is `8f60c24ddc49b14d4b4df25853eaea2b43b5d2498913aa1be9c99afe128eb4f9`;
the exact66-patch ordered series is
`b57023ccff08bcf379f8085a1a6492e8a5fff5fc7aa5b8a60f4a879696584efe`.
An independent full65-patch source/release copy succeeded before any suffix
application; all five binary hashes and build stamp match the originals.
Preservation evidence: `build/logs/firefox-http-boundary-20261017-FYObrM/`.

Initial host-development attempt there correctly rejected a malformed final
patch hunk, exit1 before compilation. Its missing blank context line was fixed,
not bypassed. Next attempt in
`build/logs/firefox-http-boundary-fixed-20261017-EkG7rx/` passes eight helper
cases and two negative controls, then Apple clang rejects a tautological
expression in the third deliberate mutant under unchanged `-Werror`, exit1.
Both failures and tested bytes remain preserved; within each failed attempt,
no later build or guest ran.
Neither is a guest/browser failure. Correcting only the deliberate mutant
to consume flags without a tautological expression preserves its intended
assertion and all compiler flags. Focused patch-only regression then passes
8cases/13negative controls (real14.72s); full pinned-private-source regression
passes33cases/21controls (real24.61s), both exit0. Exact source snapshots,
commands and logs: `build/logs/http-boundary-focused-20261017-SVW1QA/`.
Temporary compiler fixtures auto-cleaned; their exact source generator remains.

Sequential complete development/release checks passed in
`build/logs/firefox-http-state-release-20261017-jV3KHo/` with independent
preservation referenced from FYObrM, not duplicated or overwritten.
Fresh patch-only and full private-source checks pass again; `make unit check`
passes exit0, real422.96/user223.61/sys54.57 (observer0, elapsed423.004418s).
Supported nondeveloper release passes exit0, real1971.88/user276.54/sys341.93
(observer0, elapsed1971.923133s), including all five ELF/artifact audits and
truthful66-patch provenance. Actual compile/link completed in2:38.85 with191
reported compiler warnings. Source tree is
`d2e09c4c86ac820d986361353ca871a19c279560`; build libxul SHA-256 is
`c1bf319db4104b918a6e357199eb6c0fc08e4281055a3be184d64433c450534f`.
The other four measured build-artifact hashes match the retained65 release;
the new record still audits all five, not only the changed library.

Supported integration passes exit0 in unique output
`build/makos-firefox-state-djoburnx/`, with CPython3.14/-j1 and a preserved
pre-integration CPython cache. Source5baa image/manifest identities match
before and after integration. Exact integration timing is
real4183.25/user483.71/sys718.19 (elapsed4183.246618s); final unchanged
preflight passes exit0, real0.78/user0.20/sys0.13 (elapsed0.796948s).
Controller, observer and scoped caffeinate all exit0.
The private package candidate passes unchanged all-five
runtime-artifact preflight and is independently retained before cleanup,
SHA-256 `2482999bc8233d3e424575a58b4145036d90d7d4cab44a3cda7501bca5524fd8`.
Its runtime libxul hash is
`79e8764be52598fa82fd1c046e97f5ded6229c5b32b529c4249f37072ef5d7c0`;
retained runtime-record SHA-256 is
`62d7be4e461764a14cc78f10fa5b28c1614e92c439a7c66e5c11f97902f7541b`.
The retained candidate matches the supported final publication exactly:

```text
build/makos-firefox-state-djoburnx/makos-integrated-2482999bc8233d3e.img
image SHA-256: 2482999bc8233d3e424575a58b4145036d90d7d4cab44a3cda7501bca5524fd8
manifest SHA-256: 5a7f5ac96d21dbfbdfb98b1d8be949e1e8b54afa077f81367cc9e25aeaa7c4d7
```

Image size1073741824 bytes; manifest3013 bytes; package45 files,
payload262529823 bytes, metadata/payload CRCs pass. Preserved filesystem
metadata SHA-256 is
`0a0818e15ec884b2ee421dbfba9cc1ce3da21d8e3f1bf86d63f3eaa5f07c4b17`;
account/profile region SHA-256 is
`24c7909ea54cacabb8a62b7150d5cc146d96824428f3ddf4dc506b8316034cc2`.
These are the intended October fresh-state source regions, not recovery of
lost older accounts. Post-integration identity/retention check exits0.
Supported packaging removed/renamed observed private paths
`.makos-firefox-image.hzR9xS`, `.makos-integrated-_uywk09d.img.tmp` and
`/private/tmp/makos-firefox-package.H3Wpjk`; candidate bytes and runtime record
remain independently retained. No claim every ephemeral index was retained.
Fresh exact-commit qualification remains pending; packaging is not a browser pass.
Host fixtures use explicit channel/callback/transport adapters; five
complete production functions execute, other branches are structural-only.
No Gecko lifetime, guest networking or browser qualification claim follows.
Keep all five artifact audits, truthful new patch/source/build/runtime
provenance, and supported content-addressed integration from preserved
October10 source image. Old65-patch binaries must never be restamped as66.

Preserve but do not qualify new browser source with the historical master:
`build/makos-firefox-http-yd85u8ir/makos-integrated-2b11393804661aa8.img`,
SHA-256 `2b11393804661aa88b5b36bb1c8db73b4a8c57aeffc3b1d77b831298bd91c6cb`,
manifest `0e109cbf5f5da900f5da92d3e5f85227f10eedd3600b0483f1a55ffc49496aba`.

Keep Apple clang, fortification, `-Werror`, Firefox600-second probe,
first-character<500ms, Ctrl-A<10000ms and all original rendering, interaction,
networking, survival and SMP assertions. Stop qualification at first failure.
Only strict Firefox pass plus QEMU exit permits private-clone visible login.
No full OS completion claim; audit Partial/Missing rows remain.
