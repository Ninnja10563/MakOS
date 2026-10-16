# Firefox accepted-load/channel diagnostics

## Preserved Mac/HVF result

Exact commit `b57cfe2bf31a5910a1cdaf9363a1627cbde554e9` passes unit/check,
image, all twelve pre-Firefox runtime gates, image hash and unchanged
62-patch/five-artifact provenance preflight. Firefox passes verified first
paint at 42315 ms, Ctrl-A at 4749 ms and first character at 110 ms. The next
assertion fails at the unchanged first-navigation deadline:

```text
AssertionError: Firefox navigation did not complete with real page pixels loaded=False changed=False tls_verified=False visual=not checked
```

The pixel comparison was not reached: `changed=False` is not a measured
unchanged-pixel result. No target page-stop/TLS proof or final raw fatal/
failure-detail record appears. No later gate, retry or visible login ran.
QEMU exited. Full evidence, all 39 private boot/data/vars files, post-exit
hashes, complete raw serial and available pre-navigation screenshots remain:

`build/logs/macos-navigation-diagnostic-20261016-y1zlrf/`

Its `FINAL-REPORT.txt` records every command, timing, status and retention
limitation. Firefox make exits 2; observer exits 0; QEMU numeric exit status
was not independently captured. Harness PID 98745, QEMU PID 98749, session
`build/makos-aarch64-test-luw1cx82`, QMP inherited socketpair fd 4, not a
filesystem socket. Required complete EL0 records were validated; incomplete
shutdown tails elsewhere were preserved verbatim, never reconstructed.

Final serial records the typed-navigation sequence followed by actual JS
open-call, native fixup-allowed/created/load-return and JS open-return. Native
results are NS_OK and the latter two have a load state. Earlier startup
records are separate. These records establish synchronous dispatch/return,
not channel completion, target URI identity, networking or pixels. A prior
search telemetry exception did not prevent this observed dispatch. SQLite
IOERR and failed shared-file mappings remain real but causally unattributed.
Missing user-idle service is a separate source-confirmed port gap, not a
proven cause of this navigation failure.

## Diagnostic implementation, not a browser fix

Append-only patch `0063-makos-channel-diagnostics.patch` follows real
LoadURI/InternalLoad/DoURILoad, channel initialization, URILoader OpenURI,
AsyncOpen, and request start/stop callbacks. Original branches, asynchronous
scheduling, status normalization, forwarded statuses and return values remain.
One original return expression is stored in a local solely to log its result.
WONT_HANDLE_CONTENT/NO_CONTENT and unknown-protocol results are captured
before their original normalization. Stop-entry and forwarded status are
separate because existing content-length handling can change that status.

`MAKOS_FIREFOX_LOAD_DIAGNOSTIC` contains a fixed stage, actual numeric result,
real load/channel IDs when available (zero otherwise), flags and
`scope=diagnostic-only`. Flags: bit 0 top-content, bit 1 HTTP, bit 2 HTTPS,
bit 3 top-content known. Loader-only callbacks do not infer top-content.
Entry/call NS_OK is neutral metadata, not completed success. No URI, user
input, credentials or exception message is logged.

Transport admits at most 128 attempts per process through an independent
atomic cap, uses a 256-byte stack record and stages of at most 32 bytes,
attempts one checked fd-2 write, never retries short/error writes and restores
errno after metadata getters and COM releases. Existing evidence admission
limits remain unchanged. Diagnostics can affect timing, exhaust their cap,
fail or fragment; absent output does not prove a path never executed. Never
reconstruct records or promote diagnostics into qualification proof.

`ports/firefox/test-runtime-channel.py --source-dir build/ports/firefox/source`
privately applies patches to pinned upstream source. Twenty host cases and
eight negative controls exercise the actual emitter/helpers, OpenURI and
request start/stop/content-length bodies using explicit host adapters. Exact
erasure of diagnostics restores the prior whole docshell/URILoader sources,
guarding policy changes not dynamically exercised. Tests also validate the
real interface's IDL/header ownership. These are not full Gecko compilation.
Default unit/check selects this mode when source is present. Source-free
`--patch-only` runs four emitter/helper cases and five controls with a
truthful patch-fragment guard; callbacks are explicitly not executed. Explicit
`--source-dir` never silently falls back. Original compiler warnings,
fortification and -Werror remain.

## Build-cache correction

The errno 0.3.8 preparer still verifies/copies upstream bytes and applies the
actual patch every invocation, now in a private candidate. Publication copies
changed bytes/modes/links and removes stale files without importing candidate
mtimes for identical bytes. Identical published files retain inode/mtime;
changed files receive fresh mtimes so real dependent builds invalidate.
Preparation failure leaves the prior stage intact. Publication I/O failure
propagates but is not transactionally rolled back. No trust-by-marker shortcut,
source/provenance bypass, timestamp restoration or forced build skip exists.

Private host tests cover identical/timestamp-only inputs, same-size byte
changes, destination drift, modes, symlinks, stale files, preparation failure,
publication error propagation, a real tiny make dependency and negative
controls. These tests do not claim Firefox build or guest qualification.

## Release authority and next qualification

Preservation before canonical source changes is recorded in
`build/logs/firefox-channel-diagnostic-20261016-5NfTUX/00-preservation.log`.
Its independent `preserved-firefox-62-patches/` tree has matching hashes for
all five previous binaries and the previous stamp. Old images/accounts/
profiles and earlier evidence remain untouched. Development review caught
nonexistent `nsIIdentChannel.h` includes before release; the real interface
comes from existing `nsIChannel.h`. Full development attempts remain recorded.

Actual ordered 63-patch series SHA-256:
`4cf457045160e993a6e66a78b8901971719c34a63ce4fff351520225778c85f9`.
All historical 59/60/61 rejection checks remain and 62 is now rejected too.
Build through the supported nondeveloper release path, all five artifact
audits and private integration from the preserved October10 source master.
Record the newly published image and manifest hashes, then run unchanged
runtime preflight. Historical 61/62 images are preservation/integration inputs,
not current 63-patch runtime authority; never restamp them.

Development unit/check passes exit0, real218.59/user112.23/sys28.78.
Supported widget/source verification passes exit0 in911.09s; package-manifest,
runtime-evidence, navigation and channel tests also pass exit0. Their complete
logs/statuses remain in the development evidence directory above. Actual
nondeveloper release started at UTC2026-10-16T10:30:28.140518 with one job.
Supported release completes exit0 at UTC2026-10-16T11:02:49.946714,
real1941.82/user230.58/sys387.98; recorder monotonic1941.864920s.
Actual Gecko compile/link reports success at2:10.19 and191 compiler warnings
with unchanged compiler settings. All five ELF/binary audits and truthful
63-patch provenance pass. Source tree:
`b21ed8765615bdd1db206c17f8598c6782fbfe6c`.
New libxul build SHA-256:
`3aef7386a6bd5d5d79b88bb1f1bcce0728de848c4a9aace73688af2414ad10c8`.
The other four build artifacts retain the previous hashes. Preserved exact
build-stamp SHA-256:
`cec26ea97ccecbf12190293e3b135470c141f951a66c6f3742f36641082a3de4`.
Actual errno preparation retains its prior bytes/inode/mtime; this is not
an attribution of the entire build duration to that change.

Supported private integration starts at UTC2026-10-16T11:03:26.216856,
from the unchanged October10 source master into
`build/makos-firefox-channel-s25zw5qr/`. CPython cache was independently
preserved before integration and Python3.14 packaging is used. Integration
passes exit0, real4180.64/user484.10/sys802.41; recorder4180.669242s.
Final unchanged runtime preflight passes exit0, real0.80s. Controller exits0
at UTC2026-10-16T12:13:09.321907; its scoped caffeinate and host monitor exit0.

Supported publication:
`build/makos-firefox-channel-s25zw5qr/makos-integrated-efc352303ed6888c.img`.
Actual image SHA-256:
`efc352303ed6888c0cce8f33098791350869b9456da3c7d8b4b954a9159ebf57`.
Sibling manifest SHA-256:
`2592fa40ec5289a9e6ac17b82b2b39a24121ed7e216d316205ea4c02b1be35ab`.
All five full-source/build-provenance checks pass. Package has45 files,
262523743 payload bytes, valid metadata/payload CRCs, nano9.1, ncurses6.5 and
CPython3.14.7. Runtime libxul SHA-256:
`7c6e956037918cc92998d6a48d6b104b231bb9465cce51891deaa83da23f5c31`.
Exact source image/manifest hashes remain5baa9467ca510b8a/4cb2b6ef3a7d4c4d
as recorded in the protocol. Preserved filesystem metadata SHA-256:
`0a0818e15ec884b2ee421dbfba9cc1ce3da21d8e3f1bf86d63f3eaa5f07c4b17`;
account/profile SHA-256:
`24c7909ea54cacabb8a62b7150d5cc146d96824428f3ddf4dc506b8316034cc2`.

Before normal package cleanup, an independent APFS candidate copy and exact
runtime record were retained in the development evidence directory. Candidate
bytes equal the final published image; that equality did not bypass supported
publication. Runtime-record SHA-256:
`43da259ff98433a29b174570d328fceebed99428717f0523e993929de43d8da3`.
Supported cleanup removes its private package tree and verification indexes;
not all transient index paths were individually recorded. Old master images,
full source/release backups and account/profile evidence remain. No app or
service was closed or paused; T3 Code remains open.

Fresh exact-commit qualification remains pending for this increment. Prepared
evidence directory: `build/logs/macos-channel-diagnostic-20261016-FfcY3B/`.
Release/preflight success is not a browser pass. Keep Apple clang,
fortification, -Werror, every original
proof and deadline, 600-second Firefox probe, first character <500 ms and
Ctrl-A <10000 ms. Runtime requires normal pressure and two CPU samples at
least 90% idle, no concurrent QEMU/heavy build. Stop the qualification at its
first failure; no later gate or visible login. All audit Partial/Missing
rows remain; this is not a Firefox fix or full OS completion.
