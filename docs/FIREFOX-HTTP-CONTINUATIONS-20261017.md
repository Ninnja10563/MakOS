# Firefox HTTP continuation diagnostics

## Preserved Mac/HVF failure

Exact commit `d671da168a79c1b89b77c78c66fb7e07ce0208bd` passes unit/check,
image generation, all twelve pre-Firefox runtime gates, exact image/manifest
hashes and unchanged 64-patch/all-five-artifact provenance preflight.
Strict Firefox then reports:

```text
MAKOS_FIREFOX_FIRST_PAINT_OK process_start_to_verified_paint_ms=41938
MAKOS_FIREFOX_SELECTION_LATENCY_OK raw=132 enqueue_to_widget_ms=6461 limit_ms=10000
MAKOS_FIREFOX_INPUT_LATENCY_OK raw=104 enqueue_to_widget_ms=110 limit_ms=500
AssertionError: Firefox navigation did not complete with real page pixels loaded=False changed=False tls_verified=False visual=not checked
```

Make exits 2, observer exits 0; real157.31/user534.33/sys24.05,
recorder158.998938s. Pixel comparison is not reached: `changed=False` is not
a measured unchanged-page result. Target page-stop/TLS proof is absent.
Other Mozilla-host TLS records are not target-navigation proof. No final raw
`MAKOS_FATAL` or `MAKOS_FAILURE_DETAIL` appears. The qualification stops at
this first failure, with no retry, later browser assertion or visible login.
QEMU PID46230 exits; its inherited QMP socketpair is fd4, not a named socket.

Complete evidence: `build/logs/macos-channel-admission-20261017-56tIBi/`.
`FINAL-REPORT.txt` SHA-256:
`b1c47ed7dbc9a73cf3f18a9b6b9f8653d5c480c0bfec12622232a9dba60339af`.
All 39 private boot/data/vars files are retained and hashed after exit, along
with final raw serial, complete harness logs, host samples, provenance and
available pre-navigation captures. Raw serial ends with harness SIGTERM
cleanup. Older focused harnesses do not drain shutdown tails; unavailable
bytes are not reconstructed. No guest remains.

Admission repair yields 15 complete LF-terminated load records, zero malformed
records. Raw lines732–747 identify load0x17 and channel0x1c. Eleven records
have flags0xd (known top-content HTTPS), four have flags4 (HTTPS, top status
unknown). All results are NS_OK. The path reaches real channel creation,
content-loader acceptance and `AsyncOpen` return, but captures no request
start/stop callback. These records do not establish asynchronous completion
or the reason it is missing. Only15 complete records were captured; attempt
exhaustion is not established. Failed or short writes can also consume attempts.

## Narrow next observation

Append-only `0065-makos-http-continuations.patch` observes existing parent and
child HTTP implementations, accepted-load continuations, classifier dispatch
and callback, proxy resolution, suspend/resume and connection entry. It does
not force continuation, retry dispatch, change a preference or bypass content
security, tracking classification, safe browsing or proxy handling.

Use the existing `WriteLoadDiagnostic` admission and128-attempt process cap,
single checked write, no retry, bounded stage/record and errno preservation.
New stages report actual channel IDs and raw `nsresult`; flags0 means metadata
unavailable, not a negative scheme/top-content classification. Entry and void
boundary NS_OK values are neutral observations, not request success.

Parent/child methods snapshot scalar IDs before callbacks, so return traces
do not add post-callback object-lifetime assumptions. Classifier tasks capture
their real ID on the main thread before publication; workers read only that
immutable scalar, never channel COM getters. Query/release preserves errno.
The existing ignored main-thread dispatch-result policy remains ignored; the
new record observes it without inventing a repair. Existing RefPtrs and
callback ownership remain unchanged.

Source inspection finds several possible accepted waits: upload normalization,
tail queue, socket-process startup, tracker-classifier callback, proxy callback
and suspend/resume. No direct SessionSaver/user-idle-service dependency was
found in the inspected HTTP/classifier path. Missing idle service and SQLite
errors remain separate observed gaps, not a proven navigation diagnosis.
Missing diagnostics still cannot establish nonexecution; capture loss, short
writes and bounded admission remain possible. Never reconstruct records.

## Release authority and pending qualification

Required ordered65-patch series SHA-256:
`465b7e7a990c1421241629bb32f1bcb08c0fa57b0aa733a6a1b90f1105d4c83c`.
Keep pinned Firefox140.13.0esr source, all five build/runtime audits and the
unchanged runtime preflight. Production-derived host regression passes
23cases/14negative controls; patch-only emitter mode passes1case/3controls.
Exact whole-file policy restoration, extracted complete HTTP functions and
classifier constructor/completion/dispatch suffix run with explicit adapters.
Parent AsyncOpen/Connect and remaining branches are structural-only coverage;
host adapters do not qualify real threading, refcount destruction or guests.
All five focused attempts are retained, including corrected fixture-only
unused-constant/-Werror and expected-record buffer-truncation failures.

Development evidence: `build/logs/firefox-http-continuation-20261017-QloAyq/`.
Before canonical source mutation, independent APFS preservation of the full
64-patch source/release completes exit0; all five binary/stamp hashes match.
Historical64-patch image remains:
`build/makos-firefox-admission-5ttx11ow/makos-integrated-e0752622edd1667b.img`,
SHA-256 `e0752622edd1667ba5e221847d39a79fd8f67f07ba1420830e51f465f68257f3`,
manifest `ea88fc387142af444cb4ee5cd5c6a3b42496c60c832b3996b6ebd8d7d0f1e551`.
Do not restamp or use it as a65-patch qualification input. Build through the
supported nondeveloper release path; integrate from the unchanged October10
source master into a unique directory and record actual published identities.
Supported65-patch release and integration now pass; fresh exact-commit
qualification remains pending.

Full development `make unit check` exits0: real350.54/user188.13/sys47.00,
recorder350.575297s. Supported widget/source verification exits0:
real990.33/user107.56/sys206.38, recorder990.362882s. Package-manifest,
runtime-evidence, navigation, channel and HTTP regressions all exit0,
recorder0.652409/15.215261/7.571196/17.448725/18.787908s respectively.
HTTP retains23cases/14negative controls after supported canonical application.
Single-job release completes UTC2026-10-16T16:52:16.698257 (2026-10-17 local),
exit0, real1938.28/user272.62/sys374.60, recorder1938.317854s. Actual Gecko
build succeeds2:07.42 with191 recorded warnings under unchanged settings.
All five build ELF audits and truthful65-patch stamp pass, source tree
`5a9c732fd8794c1f7fd4d15359f84b8598427a34`, build libxul SHA-256
`1b4ff9be84e30c2c8e331bf699a065322f45e3a7d83a7b51d1a2bc8f8dec4c38`.
Other four build artifact identities remain unchanged. Development controller
and its scoped caffeinate both exit0. No runtime result is implied. Review
preserves host-fixture limits and the distinction between records and attempts.

Supported integration starts UTC2026-10-16T16:52:29.374539 into unique
`build/makos-firefox-http-yd85u8ir/`, from the exact preserved October10 source
and manifest after their full hashes match. CPython cache is independently
cloned before packaging. Controller86498/scoped caffeinate86499, host monitor
and owned-process candidate observer are recorded; no QEMU is launched.
Release-stamp copy SHA-256:
`55d5788be197182b3242415a87498890e2806c55961c4e1c39b21c6a2f87702e`.
Supported integration completes exit0, real4260.36/user498.33/sys827.31,
recorder4260.347070s. Final unchanged runtime preflight exits0,
real0.71/user0.27/sys0.12, recorder0.729271s. Controller and scoped caffeinate
both exit0 at UTC2026-10-16T18:03:32.017977. No QEMU is launched.

Published image:
`build/makos-firefox-http-yd85u8ir/makos-integrated-2b11393804661aa8.img`.
Image SHA-256:
`2b11393804661aa88b5b36bb1c8db73b4a8c57aeffc3b1d77b831298bd91c6cb`.
Sibling manifest SHA-256:
`0e109cbf5f5da900f5da92d3e5f85227f10eedd3600b0483f1a55ffc49496aba`.
All five runtime artifacts pass; stripped libxul SHA-256:
`f4d3a1b4b87252fece4fa0bd84e413539c7cddd5cd6cce85d1f34224206439a0`.
Other four runtime identities remain unchanged. Package has45 files,
262525375 payload bytes, valid metadata/payload CRCs and required artifacts.
Source image5baa and manifest4cb2 full hashes match before/after integration;
filesystem metadata and account/profile region hashes remain unchanged.

Independent unpublished candidate and runtime JSON are retained under the
development evidence directory and verified equal to final image/manifest.
Runtime record SHA-256:
`2aa01114f1a0d37669af5ff5694b6d15079212a0f61cb815eabab166d16ee3d2`.
Supported cleanup removes or renames the observed private paths
`/private/tmp/makos-firefox-package.7ER1ev`, `.makos-firefox-image.oapbNw`
and `.makos-integrated-3ghmq4yc.img.tmp` in the unique output directory.
Not every ephemeral verification index was inventoried; no claim all temporary
files were retained. Post-integration identity/retention checks pass exit0.
Fresh exact-commit runtime qualification remains pending; these release checks
do not establish browser success. Old64 report has a separately hashed
`REPORT-ERRATA.txt` for an omitted excerpt section; original report/raw evidence
and their identities are unchanged.

Keep Apple clang, fortification, -Werror, all existing proofs and thresholds:
600-second Firefox probe, first character<500ms, Ctrl-A<10000ms. Require normal
memory pressure and two CPU intervals>=90% idle. Never concurrent QEMU or heavy
builds during runtime; stop each qualification at its first failure. No browser
fix or full OS completion is claimed; audit Partial/Missing rows remain.
