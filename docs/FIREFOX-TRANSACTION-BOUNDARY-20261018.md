# Firefox transaction admission and DNS boundary diagnostics

## Preserved Mac/HVF failure

Exact commit `8bfb335631b1b1c67dffbe9d2e089e0611098e66` passed unit/check,
image, all twelve preceding runtime gates, exact image SHA-256 and unchanged
66-patch/all-five-artifact provenance preflight. Strict Firefox then failed
first navigation. No retry, later gate or visible login followed. All QEMU
processes exited. Evidence: `build/logs/macos-http-state-20261018-w2I2up/`.

```text
MAKOS_FIREFOX_FIRST_PAINT_OK process_start_to_verified_paint_ms=37602
MAKOS_FIREFOX_SELECTION_LATENCY_OK raw=132 enqueue_to_widget_ms=6845 limit_ms=10000
MAKOS_FIREFOX_INPUT_LATENCY_OK raw=104 enqueue_to_widget_ms=110 limit_ms=500
AssertionError: Firefox navigation did not complete with real page pixels loaded=False changed=False tls_verified=False visual=not checked
```

Firefox make exit2; observer0; real152.57/user520.44/sys21.71 seconds,
recorder153.897541 seconds. Pixel comparison was not reached: `changed=False`
does not establish unchanged page pixels. No final raw `MAKOS_FATAL` or
`MAKOS_FAILURE_DETAIL` was captured; do not infer an absent reason.
Final raw serial is102983 bytes, SHA-256
`c94dc3fe5180acc75ca111cffee3b8064db6c1eef338476246d9f771b83b5818`,
including QEMU's SIGTERM cleanup tail. All39 private boot/data/vars files
from13 guests were retained and hashed after exit. Firefox harness PID47496,
QEMU PID47501, session `build/makos-aarch64-test-tqoqogn2`, inherited QMP
socketpair fd4 (not a reusable filesystem endpoint). Numeric QEMU exit status
was not separately captured; harness exit2 is not QEMU's exit code.

Firefox private hashes:

- boot `7004e312eb2ff50f6a80bdf8bebef8eb0bd947cea94fa1784d5cc0e94e85b169`
- data `1324a14b1687339f5ec3dae4595a87b5265f441cedcb342061a686a65a2a6c6e`
- vars `07a88e0f591b924e9ef8ba96a7f22e27b3f69404628495e374e3022d032268e2`

Every runtime's master boot pre/post hash matches the boot hash above.
The accepted final recorder CPU intervals were95.41%/97.30% idle with normal
pressure1; its earlier97.64%/89.33% pair was rejected. No readiness threshold
was lowered. User-authorized Zen and Activity Monitor closures were normal;
T3 Code remained open. No process was suspended by this attempt.

Earlier gates retain all original proofs: SMP counts109/98/87, six exact
statuses; EL0 three AP entries/96 calls; VM16/48/288; cold TTY/file/denial
proofs; ordered BTYPE; real owner/TCP/input fixtures. Self-host20 builds,
21 processes, graphs4,3,2,2,3,2,2,3 and exact generated headers pass. One
locked parallel record has PIDs/groups15,14,13, CPUs1,2,3 and distinct roots
401f0000/4018f000/4012e000; statuses42,42,42. Self-host migrations23/drops0;
Native/Python-role/Firefox-role migrations2/1/2, drops0. Roles are not browser
or Python execution. Cursor positions7/changed scanout pixels0/GPU errors0.
The sealed report and supplementary manual review record focused-harness
tail truncation/line-ending limits; no fragmented records were reconstructed.

## Established boundary, not a root-cause diagnosis

Raw load0x17 binds to channel0x1a. Thirteen complete HTTP_STATE records have
seven actual NS_OK results and six neutral observations. Cache wait clears;
network continuation reaches transaction setup/init/dispatch/read returns.
The final three records reuse DispatchTransaction's entry state, not fresh
post-call state. Zero transaction/pump bits therefore do not prove absence
after initialization. AsyncRead NS_OK means response-pipe pump acceptance,
not response bytes. Dispatch NS_OK is acceptance by the selected manager API;
without manager-variant evidence it does not prove the local enqueue path.

Five native connect pairs succeed before target navigation, with no Gecko
channel/hostname association. Five TLS markers identify startup services,
none the target. Existing128 load-diagnostic attempts are consumed. Missing
later markers do not prove nonexecution. HTTP-state saturation notices0;
four file-failure overflow notices and one interaction-drop notice are
captured, with final omitted totals unavailable. Those notices are distinct
from migration evidence drops0. No diagnostic-absence claim is universal.

## Bounded next implementation

Patch0067 observes first-channel transaction admission, pending disposition,
connection creation and DNS submission/callback boundaries. One independent
startup manager record identifies local versus IPC selection and PID; it
does not prove initialization completed. Channel zero is startup configuration,
not request identity. The first-channel latch is process-local; a remote
socket process is not assumed to inherit an armed parent latch.

Transaction observations have a separate32-attempt budget plus one overflow
notice. Failed/short writes consume attempts without retry/refund. Records
use one native write, preserve errno and contain scalar channel/PID/result/
stage arguments, never URL/hostname/input text. IDs and flags are captured
before potentially reentrant calls. Actual-result labels occur after real
API calls; canceled, pending, paced and connection-limit branches are neutral.
In particular, a wildcard proxy limit must not pretend its locally assigned
NS_ERROR_NOT_AVAILABLE was a dispatch API return.

Stage arguments are raw observations, not additional success assertions:

| Stages | arg0 | arg1 |
| --- | --- | --- |
| `connection-limit` | Current active connection count | Current configured maximum |
| `dns-socket-create-call`, `dns-socket-create-return` | Active count captured before create call | Maximum captured before create call |
| `try-dispatch-enter` | Existing `onlyReusedConnection` boolean | Zero |
| `new-message`, `process-return` | Existing priority cast to uint32 | Zero |
| `dns-resolve-enter` | Base `mDnsFlags` at entry | Backup boolean captured at entry |
| `dns-submit-call`, `dns-submit-return` | Base `mDnsFlags` captured before each submission | Backup boolean captured at ResolveHost entry |
| `dns-service-missing` | Zero | Backup boolean captured at entry |
| `dns-transport-return` | One for primary callback, zero for backup | Zero |
| All other stages, including manager selection | Zero | Zero |

DNS submission still ORs the unchanged `RESOLVE_WANT_RECORD_ON_ERROR` bit
into its API argument. Logged base flags exclude that added bit unless it
already existed in `mDnsFlags`; they are not the full effective API flags.
Callback status is an actual incoming status, not the callback function's
return value. Create/dispatch return records retain captured entry scalars;
none claims a post-call object snapshot.

The local manager posts a ConnEvent through the socket-thread event target.
Successful queue insertion calls an installed dispatch observer before returning,
but nsSocketTransportService::OnDispatchedEvent ignores PollableEvent::Signal's
boolean result. Local dispatch NS_OK therefore does not prove a successful
wake-pipe write or event execution. Same-thread dispatch can skip signaling.
Also, OnMsgNewTransactionWithStickyConn has no `new-message` observation;
it still calls the instrumented ProcessNewTransaction. Missing `new-message`
cannot uniquely locate a queue/wake failure. These are source-path limits,
not findings from a new guest. Preserve process-local admission/budget/write
loss caveats and distinguish IPC manager selection before interpreting them.

No production network policy, DNS retry, cache, profile preference, assertion,
deadline or compiler setting changes. Exact diagnostic erasure must restore
all four preceding source files byte-for-byte. Existing diagnostic budgets
remain independent and unchanged. This is observation, not a browser fix.

An independent full66 source/release clone was verified before any supported
suffix application: `build/logs/firefox-transaction-boundary-20261018-2xbLiH/`.
Preservation exit0,107.99538958299672 seconds; all five binary hashes match.
Focused patch-only regression passes18cases/25negative controls, exit0,
real29.08/user13.91/sys2.39 seconds. Full pinned-private-source regression
passes28cases/30controls, exit0, real28.48/user17.90/sys2.99 seconds.
The subsequent complete development `make unit check` passes exit0,
real453.09/user253.44/sys54.36 seconds, observer0. All existing assertions
remain. Test/patch input snapshots are retained in the evidence directory.
Supported widget/source verification passes exit0,
real846.83/user106.90/sys163.20 seconds, observer0. Package-manifest and all
existing runtime-evidence/navigation/channel/HTTP host regressions pass;
the new full transaction regression passes again28cases/30controls.
Gecko compilation/linking completes successfully at mach3:10.89, with191
compiler warnings reported. All five build-ELF audits and the binary audit
pass. Supported release and final build-provenance generation pass exit0,
real1886.12/user338.15/sys335.50 seconds; observer0 and controller0.
Patched source tree is `7b61c0e12892c7061d3d2cc4038ddbc766345b1e`;
actual build libxul SHA-256 is
`08cd75fc74040878b9556444e34aeed5df2fd2f5c323da7c4dffde4a1a0c7807`.
All five measured artifact hashes are retained in
`release-build-provenance.json` in the evidence directory.
Supported private integration passes exit0, real4154.13/user524.65/sys755.01
seconds; final unchanged runtime preflight passes exit0,
real0.73/user0.27/sys0.12 seconds. Controller/observer/caffeinate exit0.
The actual new publication is
`build/makos-firefox-transaction-y4rak151/makos-integrated-8b191c84c05f1999.img`,
SHA-256 `8b191c84c05f199917981ecedbba2cc4bf295f7c7ed31c3a4a268412b21d7ef9`.
Its sibling manifest SHA-256 is
`450434abf2fc7f919f12e6ddb99ec1910d28ce356ac353a93b5e772cba19464c`.
Source master5baa and manifest4cb2 match before/after; filesystem metadata
and account/profile regions are preserved. Independent candidate/runtime-record
retention and post-publication identity checks pass. Runtime libxul SHA-256 is
`b458fc1fb39b79afba794c4c300dd1582784468602fcf43cfd2dc50a66444d0c`.
The supported packager removed its observed private package tree
`/private/tmp/makos-firefox-package.f0IeUS`, and renamed/removed its observed
`.makos-firefox-image.MBnITA` and `.makos-integrated-m0hg5_nl.img.tmp` paths.
Their candidate image and runtime provenance were retained independently;
not every ephemeral verification file or private staging file was retained.
Fresh guest qualification is not run. Packaging is not browser qualification.
Full regression
uses explicit host adapters for complete OnMsgNewTransaction/ResolveHost
bodies; other branches are structural-only, never full Gecko runtime proof.

Current new release contract requires67 patches, ordered series SHA-256
`c870df920805a85a37f829aea92fb7e51786a0b57d317e95e65aa0dce09fea71`.
Patch SHA-256 is
`4c1c49110feca0f7ba4dce29defa078b58ba580c0e3e2bd853891f804b4f673b`;
regression generator SHA-256 is
`1f5d8c180f5562d7a4586a08f583f0a1dbd51b5acbca05bf9f6536edd298ba12`.
Build/package/integrate only through supported paths from preserved October
fresh-state SOURCE_DATA_IMAGE. Record new measured image/manifest identities;
do not restamp66 bytes or overwrite accounts/profiles. All five artifact
audits and unchanged runtime preflight remain mandatory. Reuse the measured
new67 publication above; do not rebuild it for a subsequent kernel-only repair.
The66 image2482999 and its manifest remain historical evidence.

Keep Apple clang, fortification, -Werror,600-second Firefox probe,
first-character<500ms, Ctrl-A<10000ms and every original gate. Stop fresh
qualification at first failure. Only strict Firefox pass and QEMU exit allow
private-clone visible login. Audit Partial/Missing rows remain.

Compiler roles are unchanged, not interchangeable claims: host regressions
retain Apple's `/usr/bin/cc`/`/usr/bin/c++`. The already-committed Firefox
`host-tools.sh` selects its Homebrew LLVM22.1.8 host pair on Darwin/arm64;
target Gecko compilation uses the supported MakOS clang drivers. This
increment supplies no host compiler override and changes neither selector
nor fortification/-Werror. Firefox uses Python3.12.12; CPython packaging
requires the separate host Python3.14.7.
