# Firefox channel diagnostic admission

Superseded runtime result and next diagnostic contract:
[HTTP continuations](FIREFOX-HTTP-CONTINUATIONS-20261017.md).
Exact d671da168a79 qualification passes all earlier gates and64-patch
preflight, then fails first navigation (paint41938ms/Ctrl-A6461ms/character110ms).
Fifteen complete admitted records are captured; attempt exhaustion is not
established and asynchronous completion remains unresolved. Historical identities remain
preserved, not65-patch runtime authority.

## Preserved first failure

Exact Mac/HVF commit `9388336974a4195c9b32e4311e18dc72ae486193` passes
unit/check, image, all twelve pre-Firefox runtime gates, image SHA-256 and
unchanged 63-patch/all-five-artifact preflight. Strict Firefox records:

```text
MAKOS_FIREFOX_FIRST_PAINT_OK process_start_to_verified_paint_ms=35145
MAKOS_FIREFOX_SELECTION_LATENCY_OK raw=132 enqueue_to_widget_ms=3822 limit_ms=10000
MAKOS_FIREFOX_INPUT_LATENCY_OK raw=104 enqueue_to_widget_ms=101 limit_ms=500
AssertionError: Firefox navigation did not complete with real page pixels loaded=False changed=False tls_verified=False visual=not checked
```

Pixel comparison was not reached; `changed=False` is not a measured unchanged
image. No target page-stop/TLS proof or final raw fatal/failure-detail appears.
The sequence stopped: no retry, later browser assertion or visible login.
QEMU exited. All 39 private boot/data/vars files, post-exit hashes, final raw
serial and available pre-navigation captures remain in
`build/logs/macos-channel-diagnostic-20261016-FfcY3B/`.
Its report/index retains exact commands, statuses, timings and cleanup limits.

There are exactly 128 complete channel diagnostics, zero malformed records.
All precede the typed URL-bar call at raw line 843; the last is line 563.
Flags are 0, 8 or 9: none identifies HTTP/HTTPS. All recorded results are
NS_OK, which does not prove completion. Seven AsyncOpen return records and
seven start/stop/forward triplets exist, but all callback channel IDs are
zero; temporal proximity cannot associate them with other channel objects.
Startup consumed the diagnostic budget before typed navigation. This is a
proven observation defect, not the cause of the browser navigation failure.
The later URL-bar/fixup records still show synchronous accepted dispatch and
return, not asynchronous completion or independent URI identity.

## Bounded observation correction

Append-only `0064-makos-channel-admission.patch` changes only admission to
`WriteLoadDiagnostic`. A process-local atomic latch arms once on actual
known top-content HTTP/HTTPS metadata (flags bit 3 and bit 0, plus bit 1 or
bit 2). Before arming, no attempt is consumed. Invalid stages cannot arm it.
After arming, all existing stages remain eligible, including callbacks with
unknown scheme/ID and error results. No whitelist, per-load reset or rearm
extends the original 128-attempt process cap. Failed/short writes still
consume attempts; each admitted record still attempts one checked write,
without retry. Stack/stage bounds and errno preservation remain unchanged.
The real Mozilla atomics default to sequential consistency.

The trigger is the first qualifying web load, not necessarily the user's
typed navigation. Subsequent unrelated work can exhaust the budget. Missing
output still cannot establish nonexecution; never reconstruct fragmented
records or use diagnostic records as page, TLS or pixel proof. No browser
policy, preference, callback status, test deadline or threshold changes.
The missing guest user-idle service remains a separately confirmed port gap;
its relationship to failed navigation is unproven and is not changed here.

Host regression uses the real patched emitter/helpers and production callback
bodies through explicit adapters. Existing callback/status/structural checks
remain, with startup-burst, HTTP/HTTPS arming, invalid-stage, unknown-callback,
concurrent cap and failed-write coverage. Host tests do not qualify Gecko or
guest behavior. Focused pinned-source regression passes 23 cases/13 negative
controls; source-free patch-only subset passes 7/10. Both exit0, retaining all
original20/8 full cases/controls. Complete logs and exact commands are in
`agent-channel-*` under the development evidence directory below. Release
and runtime qualification are not claimed from these host checks.

## Release and qualification authority

New ordered 64-patch series SHA-256:
`117c028eda1c76c6c001b7fed6a6fc320e280e810e9f01e4c0d36917efa080b7`.
Keep pinned Firefox source, all five build/runtime audits and provenance.
Historical 59/60/61/62 rejection coverage remains; 63 is now rejected too.
The previous master remains preserved:
`build/makos-firefox-channel-s25zw5qr/makos-integrated-efc352303ed6888c.img`,
SHA-256 `efc352303ed6888c0cce8f33098791350869b9456da3c7d8b4b954a9159ebf57`,
manifest `2592fa40ec5289a9e6ac17b82b2b39a24121ed7e216d316205ea4c02b1be35ab`.
It is historical evidence, not a current 64-patch input.

Development/preservation evidence:
`build/logs/firefox-channel-admission-20261016-aPxKo4/` (UTC-date directory).
The completed63-patch source/release was independently preserved before
supported suffix application. The nondeveloper release wrapper and supported
private integration from the unchanged October10 source master both pass.
Actual new image/manifest identities are recorded below; no old binary was
restamped or manually staged. Fresh exact-commit qualification is pending.

Independent APFS preservation completed successfully before build mutation:
`preserved-firefox-63-patches/` matches all five binary hashes and build stamp,
with independent inodes and preserved symlinks. Old master/manifest hashes
also match. About34GiB remained free afterward. Sequential development
unit/check passes exit0, real322.38/user169.37/sys42.49, recorder322.427790s.
Supported widget/source verification passes exit0,
real999.08/user106.44/sys206.86; recorder999.126038s. Package-manifest,
runtime-evidence, navigation and channel checks all pass exit0. Supported
single-job release started at UTC2026-10-16T13:26:14.502304 with Firefox
Python3.12 and developer mode unset. Actual Gecko compile/link succeeds at2:35.89 with191
compiler warnings under unchanged settings. All five ELF and binary audits
pass. Supported release completes exit0 at UTC2026-10-16T13:59:35.684009,
real2001.14/user284.84/sys382.45; recorder2001.198677s. Development controller
and scoped caffeinate both exit0. New source tree:
`814453fcfc37d8d6c42104e12086dbc2a38b881b`.
New libxul build SHA-256:
`6c7cc01e183cd498bb7b8485bfe7612c7cea18a191488ad4bc8188da16f180d9`.
The other four build hashes remain unchanged from63. Preserved build-stamp
SHA-256: `49814da6dd570ce7a25b7044d94acf28b236569f829df810dc450d46725fb117`.
These build results are not browser qualification. Supported integration uses
the unchanged October10 master and completes as recorded below.

Integration uses unique `build/makos-firefox-admission-5ttx11ow/`, with an
independent preserved CPython cache and supported Python3.14 packaging.
Canonical and private-snapshot source/build checks pass. Runtime-record
creation binds all five stripped artifacts; the candidate has45 files,
262523807 payload bytes and valid metadata/payload CRCs. Its all-five-ELF
runtime preflight passes before final publication. Runtime libxul SHA-256:
`c4b14088e1999e34f426a6dbb3aeb5f8f5ae57cbb0c3d58b1da7b27873492377`.

An independent APFS candidate copy and exact runtime record are retained in
the development evidence directory before normal cleanup. Candidate SHA-256:
`e0752622edd1667ba5e221847d39a79fd8f67f07ba1420830e51f465f68257f3`.
Runtime-record SHA-256:
`a662a4c75ef14dc42d3a930ae24f17b4ada93c68318427c8f6e35d28a2bfdd36`.
This is forensic retention, not authority to boot an unpublished candidate.
Final revalidation, source preservation and supported publication now pass.
Actual published image:
`build/makos-firefox-admission-5ttx11ow/makos-integrated-e0752622edd1667b.img`,
SHA-256 `e0752622edd1667ba5e221847d39a79fd8f67f07ba1420830e51f465f68257f3`.
Sibling manifest SHA-256:
`ea88fc387142af444cb4ee5cd5c6a3b42496c60c832b3996b6ebd8d7d0f1e551`.
The retained candidate bytes match the final publication, but only supported
publication and final preflight establish this qualification input.

`make integrated-data-aarch64` passes exit0,
real4273.49/user500.07/sys829.08; recorder4273.439213s. Final unchanged
runtime preflight passes exit0, real0.74/user0.27/sys0.13;
recorder0.759335s. Integration controller and scoped caffeinate exit0 at
UTC2026-10-16T15:11:08.524099, controller recorder4280.738783s.
Full source image SHA-256 stays
`5baa9467ca510b8a1a207365abed12fe8b03f228432a80b2238b3b46042d00ce`;
source manifest stays
`4cb2b6ef3a7d4c4d852755ce33ebd0cbbbb4de5291b53a3d6dfc7a68fe2a6f6a`.
Filesystem metadata hash stays
`0a0818e15ec884b2ee421dbfba9cc1ce3da21d8e3f1bf86d63f3eaa5f07c4b17`;
account/profile region stays
`24c7909ea54cacabb8a62b7150d5cc146d96824428f3ddf4dc506b8316034cc2`.
Post-publication accounting confirms the private package directory
`/private/tmp/makos-firefox-package.XLEscr` and original temporary image names
are gone through supported cleanup/publication. The independent candidate
image and exact runtime record remain; not every temporary verification-index
path was inventoried. No prior release, master or user data was deleted.
No guest has launched for this increment; no browser pass is claimed.

Keep Apple clang, fortification, -Werror, all original proofs, 600-second
Firefox probe, first character <500 ms and Ctrl-A <10000 ms. Require normal
memory pressure and two CPU intervals at least 90% idle; never concurrent
QEMU or heavy builds during runtime. Stop each qualification at its first
failure. All audit Partial/Missing rows remain; no Firefox fix or full OS
completion is claimed.
