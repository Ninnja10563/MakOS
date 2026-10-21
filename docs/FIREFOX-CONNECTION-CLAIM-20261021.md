# Firefox claimed-connection continuation

## Established failure and preservation

The strict Mac/HVF run of `1d39533fe8f03794122ccd1527b27c12452a80f0`
passed all preceding gates, then failed first navigation after paint 34334 ms,
Ctrl-A 6386 ms and first character 110 ms. The latest complete evidence remains
in `build/logs/macos-tcp-eof-20261018-yXeqwu/`. No browser repair is established.

The selected transaction reaches successful connection claim, pending insertion
and an only-reused-connection dispatch attempt. `FindConnToClaim` can claim an
existing DNS/connect object or an active connection with a null transaction
driving TLS. The original DNS object's transaction is not replaced by claiming
it. Absence of selected-channel DNS records therefore does not prove DNS failed
to run. Connection kind and subsequent continuation remain unobserved.

On October 21 the user authorized deletion of obsolete MakOS artifacts. Cleanup
removed older runtime-private test clones and stale fixture directories, freeing
26 GiB. Historical text logs, master images, source/release caches and the latest
complete runtime were retained. `build/cleanup-20261021.json` enumerates deletions;
older documentation's retained-disk statements describe the original runs and
are superseded by this inventory. Missing old disk bytes cannot be recovered
from their hashes. No current account/profile image was changed.

Before this increment, the complete current Firefox source/release was copied
with APFS clones to
`build/logs/firefox-connection-20261021-79phshi0/preserved-firefox-67-patches/`.
All five binary/stamp hashes match, with distinct file inodes. Both the current
`8b191c84c05f1999` master and October integration source `5baa9467ca510b8a`
retain their pinned image/manifest identities. The existing >15 GiB preservation
guard was met unchanged.

## Diagnostic contract

Patch `0068-makos-connection-claim.patch` adds historical claim provenance:
selected channel plus a process-local token, capped at 32 admitted claims. These
scalars never replace transaction ownership, references or IDs. Socket-thread
objects retain the last admitted claim; a later unrelated transaction may use
the same connection. A claim token is not proof of current ownership.

Claim paths distinguish existing DNS/connect, new DNS/connect and active TLS.
Pending-reference classification and the actual dispatch return are recorded.
DNS callbacks, stream readiness/status and connection setup retain captured
claim identity. Setup copies that identity into the existing strong connection
before later callbacks. Connection close, transaction close, readable/writable,
NPN-wait, handshake callback and reclaim boundaries then remain observable even
after a null transaction has been removed. Incoming close/setup reasons are
explicitly neutral observations, not successful operation results.

All new records share the existing `MAKOS_FIREFOX_TRANSACTION` 32-record plus
one saturation-attempt budget. There is no extra transport budget. For `claim-*`
records `arg0` is the historical token; `arg1` is a state snapshot except setup
and stream-status stages, where it identifies the primary transport. DNS state
packs overall/primary/backup states in bits 0/4/8 and speculative/free/connected
flags in bits 12/13/14. HTTP state packs connection state in bits 0–7 and
experience flags above bit 7. `dispatch-on-connection` identifies the actual
transaction channel; its arguments are token and historical-channel equality.
Pending-reference records have zero arguments. Pending-dispatch return records
carry the actual return and the existing only-reused decision.

Repeated HTTP I/O stages get per-claim once bits, consumed even on failed writes.
Callbacks never depend on a diagnostic strong reference or read an object after
its lifetime solely for logging. Errno, first-channel admission, non-MakOS
argument erasure and all preexisting transport limits remain. Missing records
after saturation or failed transport remain inconclusive. No URI, hostname,
preference, retry, DNS/TLS policy or qualification assertion changes.

## Validation and release requirements

`ports/firefox/test-runtime-connection.py` executes production helpers/macros,
concurrent token admission, failed/short writes, shared saturation, transfer and
post-deletion snapshots. Pinned-source mode additionally checks full-file policy
restoration after diagnostic erasure and the complete three production claim
methods against explicit host adapters. Other Gecko sites receive source checks;
host adapters do not prove guest lifetime, networking or TLS progress.

Initial host compile caught a macro parameter colliding with a member name; its
log and source draft are retained. A negative-control expectation was corrected
to require the earlier unselected-claim rejection. Neither failure launched a
guest or changed the canonical Firefox cache. Source-free host checks pass
8 cases and 6 negative controls. Pinned-source mode passes 16 cases and 7
negative controls in 7.06 seconds, including all three complete claim methods
and exact full-file diagnostic erasure. The first full suite stopped on stale
handoff patch-count wording; the failure log remains preserved. Correcting that
wording and adding a stale-67 provenance rejection control leaves all existing
assertions intact. Fresh `make unit check` passes, exit 0, in 386.36 seconds.
Widget validation passes, exit 0, in 887.13 seconds. The subsequent package,
evidence, navigation, channel, HTTP, boundary, transaction and connection host
checks all pass. Supported release passes, exit 0, in 2123.61 seconds, including
all five ELF audits, binary audit and truthful 68-patch provenance. An additional
read-only audit finds five new stage strings in readable file-backed load
segments, exit 0. Supported integration passes, exit 0, in 6000.77 seconds;
final unchanged image preflight passes, exit 0, in 0.98 seconds. Guest
qualification remains pending.

The new publication is
`build/makos-firefox-connection-gjp2lha0/makos-integrated-8d4e097da8deae9c.img`.
Its SHA-256 is
`8d4e097da8deae9cdba03662a8343af42c5f0ba5d6006daf4d7001d1f5a9616f`;
the sibling manifest SHA-256 is
`164fd6dad1fa4889e9d1d29daa1074291b0690d386f438c042aa9679ab930e20`.
The complete source image and manifest match their original hashes after
integration. Filesystem metadata and account/profile regions are preserved.
An independent APFS candidate copy matches the published image; its separate
runtime record SHA-256 is
`b2b3ff72c57b67ea38bef5a0bec18270902dce9f616b93e8941dd0da9d5d18a0`.
All five build/runtime artifacts pass provenance and ELF preflight. Full logs,
statuses, source/release/CPython backups and candidate/runtime-record copies are
in `build/logs/firefox-connection-20261021-79phshi0/`. Two upstream compiler
warnings (include-path case and `nsHttpChannelAuthProvider` lifetime) remain
recorded; no compiler flag was changed.

The ordered 68-patch SHA-256 is
`561b3ec9adefecd4d3b0e4fe298be06a96c31042f426b01b2a1cfced80fb46df`.
Use the supported default release wrapper with Python 3.12, then supported
integration with host Python 3.14 and a unique output directory. Require all five
artifact audits, truthful new source/provenance and preserved source-account/
profile regions. Old 67-patch image bytes must not be restamped.

After publication, qualify the exact new commit with the complete unchanged
sequential Mac/HVF protocol. Keep normal pressure, two >=90% idle samples,
no concurrent QEMU/heavy build, 600-second Firefox probe, 90-second first
navigation, first character <500 ms, Ctrl-A <10000 ms and every other assertion.
Stop that qualification at first failure, preserve evidence and investigate the
next increment; do not retry unchanged failures into passes. Visible login only
follows a strict browser pass. No original-spec audit row is upgraded here.
