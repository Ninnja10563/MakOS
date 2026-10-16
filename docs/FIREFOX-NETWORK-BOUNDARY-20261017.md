# Firefox native network boundary diagnostics

## Preserved failure, not a browser repair

Exact Mac/HVF source `f3cfdfea28311811aa54f4dcf02a169886466df2` passes
unit/check, image, all twelve pre-Firefox runtime gates, exact image/manifest
hashes and unchanged65-patch/all-five-artifact runtime preflight. Strict Firefox
then fails its first navigation. No retry, later browser assertion or visible
login is run. QEMU exits; all39 private boot/data/vars files are retained and
hashed after exit. Evidence:
`build/logs/macos-http-continuation-20261017-366oqF/`.

```text
MAKOS_FIREFOX_FIRST_PAINT_OK process_start_to_verified_paint_ms=41937
MAKOS_FIREFOX_SELECTION_LATENCY_OK raw=132 enqueue_to_widget_ms=6213 limit_ms=10000
MAKOS_FIREFOX_INPUT_LATENCY_OK raw=104 enqueue_to_widget_ms=142 limit_ms=500
AssertionError: Firefox navigation did not complete with real page pixels loaded=False changed=False tls_verified=False visual=not checked
```

Harness make exits2, observer0; real157.63/user535.31/sys23.32,
recorder159.279113s. Pixel comparison is not reached: `changed=False` is not
a measured unchanged-page result. No target page-stop/TLS proof or final raw
`MAKOS_FATAL`/`MAKOS_FAILURE_DETAIL` appears. Final raw serial is100626 bytes,
SHA-256 `6f2d2939a0a441bec0700a3c30a9523948ae362fabe094456f9abc16631ab7a0`;
it retains the final harness SIGTERM cleanup tail. Report SHA-256:
`bc403f3e49b5ef01a0fa6f445baf892a411ad4311db17fe7818c94f4c8c30dd4`.
Available screenshot is pre-navigation New Tab, not a failure-time or target
page capture. Separate `VISUAL-REVIEW.txt` records that inspection without
changing original artifacts or ledgers.

QEMU PID19316, harness/session details are in the report and observed argv;
session `build/makos-aarch64-test-dl9l4tsj`, inherited QMP socketpair fd4,
not a reusable filesystem socket. Retained private files are under
`17-firefox-runtime-private/makos-aarch64-test-dl9l4tsj/`:

- boot: `d856fe708440ac4fcbefb7a5f032d98f425374f096eeb84802738e02d3442ac5`
- data: `ca6ea183bbc9ec30012a66da0760d64e1e612cbe62effd7df826bad18b580e92`
- vars: `a8591fc9ca7e7ccaedcb4dbf94191892ac5855cef301b29fb5127eb5e83d8404`

Every runtime's master boot pre/post hash matches. Self-host proves20 builds,
21 processes, ordered graphs4,3,2,2,3,2,2,3, exact generated headers and one
locked parallel snapshot with CPUs1,2,3, distinct PIDs/groups13,15,14 and roots
`0x4012e000,0x401f0000,0x4018f000`; statuses42,42,42. It records23 migrations,
zero drops,35 owner compositions/48 AP deferrals, pending0 and zero GPU
errors/timeouts/delayed recoveries. Native/Python-role migrations2/1 and
Firefox-role migrations3 pass their unchanged assertions; these are role
fixtures, not Python/Firefox application execution. Cursor positions7,
changed scanout pixels0, errors/timeouts/delayed recoveries0.
The final Firefox launch samples93.88% and93.85% idle with normal pressure.
No applications were paused/closed or system services disabled; T3 remained open.

## What the new HTTP records establish

128 complete LF-terminated load diagnostics, zero malformed records, all raw
results NS_OK. This fills the existing process cap; later silence cannot prove
nonexecution. IDs0 mean unavailable. Other nonzero channels lack a load/URI
link in this capture and remain unattributed, not assumed background requests.

Typed-load chain identifies load0x17/channel0x1a. Forty records for that channel
show actual classifier worker dispatch, lookup/return, main dispatch, completion
and callback, proxy callback, `BeginConnect`, preparation and `Connect` entry,
then nested actual NS_OK returns and classifier callback return. This excludes
never-reaching those recorded boundaries, not downstream cache/DNS/socket
completion. Source allows tail/cache waits, DNS promises and accepted asynchronous
transaction dispatch to return NS_OK. Missing network-link service disables
optional cache/network racing; normal cache callback fallback remains. No
source-proven channel/cache defect or navigation cause is established.

## Next bounded kernel observation

Existing `MAKOS_FIREFOX_SOCKET_CONNECT` appears only after transport setup;
earlier validation, handshake or publication failures are not covered by that
marker. Poll's existing invalid-argument return also lacks a corresponding
bounded Firefox diagnostic. Observing these real native boundaries is the next
increment; it does not change network semantics or claim a repair.

Capture real connect attempt/result identity and unchanged poll/connect denial
returns without reading invalid user buffers or reevaluating short-circuited
validation solely for logging. Snapshot task identity before socket/device locks.
Keep separate bounded budgets with explicit saturation, no retries or fabricated
final dropped counts. A connect boolean is the existing native ABI result, not
a Gecko channel result, errno or TLS/HTTP proof. No URL/payload/hostname is emitted;
native task/socket identities do not themselves identify a Gecko channel.

Host regression extracts the actual admission/wrapper/denial functions and
connect syscall arm. Eight cases cover role filtering, paired actual booleans,
entry identity, denial metadata, independent budgets, concurrent admission,
saturating counters and unchanged short-circuit validation. Seven compiled
behavioral mutants and two structural policy mutants are rejected. Identity,
serial and inner transport are explicit host adapters, not guest/network proof.
Exact parent-restoration digests separately retain the original socket and
architecture policy; poll control flow is protected structurally, not executed
by this host fixture. Output budgets are32 connect pairs and8 denials of each
kind, plus one saturation notice per budget. Role lookup still occurs after
saturation; bounded output does not mean zero steady-state overhead.

The first focused development attempt exited1 because a deliberate identity
mutant left a binding unused under `-Dwarnings`; its intended runtime assertion
was not reached. Logs and pre-build clones remain in
`build/logs/firefox-network-boundary-20261017-uq238J/`. Temporary host fixture
files were automatically removed on exception, not retained. Correcting only
that mutant to explicitly consume the old binding keeps `-Dwarnings` intact.
The corrected focused regression passes exit0, real5.68s, in
`build/logs/firefox-network-boundary-fixed-20261017-HL8Rid/`.
Full `make unit check` passes exit0, real347.47/user190.51/sys45.37,
monotonic347.518553s. `make image-aarch64` and artifact checks pass exit0,
real15.11/user12.26/sys0.44, monotonic15.129445s. The development controller and
scoped caffeinate both exit0; no guest launched, no apps/services changed.
Pre-build boot/kernel artifacts have independent verified clones, and the
65-patch master/manifest and all five release artifact hashes remain unchanged.
Fresh exact-commit Mac/HVF qualification remains pending.
No existing assertion, workload, compiler, deadline or threshold changes. Do not
infer that separately observed EOF/nonblocking/poll-capacity limitations caused
this failure without target-correlated evidence. Preserve all failed attempts.

## Unchanged Firefox input

This kernel-only increment reuses the completed65-patch image; no Firefox
rebuild, reintegration, restamp or profile preference edit:

```sh
INTEGRATED_IMAGE=build/makos-firefox-http-yd85u8ir/makos-integrated-2b11393804661aa8.img
shasum -a 256 "$INTEGRATED_IMAGE"
python3 scripts/verify_firefox_runtime_image.py "$INTEGRATED_IMAGE"
```

Image SHA-256 `2b11393804661aa88b5b36bb1c8db73b4a8c57aeffc3b1d77b831298bd91c6cb`;
manifest SHA-256 `0e109cbf5f5da900f5da92d3e5f85227f10eedd3600b0483f1a55ffc49496aba`.
Keep65-patch series `465b7e7a990c1421241629bb32f1bcb08c0fa57b0aa733a6a1b90f1105d4c83c`,
all five artifact audits and unchanged preflight. Preserve the master; guests
use private clones. Run the full committed Mac protocol, stopping at its first
failure. Firefox probe600s, first-character<500ms, Ctrl-A<10000ms and every
remaining pixel/interaction/network/SMP proof remain mandatory. No full OS
completion claim; audit Partial/Missing rows remain.
