# Firefox MakFS4 prefix copy-on-write repair

## Preserved qualification

Exact clean `0f415ab5b30e282d092a88f7c08a0cea71de030b` matched
HEAD/main/origin/main/live remote on Apple M3/macOS 26.6.2, QEMU 11.0.3/HVF.
Unit/check, image, all twelve runtime gates, integrated image identity and
unchanged 61-patch/five-artifact Firefox preflight passed. Strict Firefox
then failed; no retry, later browser assertions or visible login:

```text
MAKOS_AARCH64_DESKTOP_IDLE_OK accepted_presents=0 cpu_ratio=0.013 wait=wfi dirty_present=1
MAKOS_FIREFOX_FIRST_PAINT_OK process_start_to_verified_paint_ms=33187
AssertionError: Firefox Ctrl-A QMP-to-widget latency exceeded bound observed_ms=15446 limit_ms=10000
```

Make exit2, observer0; real64.38s, user185.12s, sys11.63s. QEMU60754,
harness60751, session `build/makos-aarch64-test-kswl42r_`, QMP socketpair fd4.
Both exited. Full logs, final raw serial, screenshots, private boot/data/vars
retained before cleanup and hashed after exit, host evidence and provenance:
`build/logs/macos-vm-first-fit-20261015-he9KQb/`. See its `FINAL-REPORT.txt`,
`RESULTS.json`, `EXACT-RESULT-MARKERS.txt` and `MASTER-IDENTITIES.txt`.
Launch pressure1 and idle95.94%/98.70%; during pressure1, swap195.25MiB,
sole QEMU with no heavy build. No fatal diagnostic was observed; none inferred.

Master boot and all runtime pre/post hashes:
`c57714dae74516e208e3286277207783f35c2cb0d0f70bc954d7ea9dd8011f79`.
Firefox image unchanged:
`5baa9467ca510b8a1a207365abed12fe8b03f228432a80b2238b3b46042d00ce`;
manifest `4cb2b6ef3a7d4c4d852755ce33ebd0cbbbb4de5291b53a3d6dfc7a68fe2a6f6a`.
Private data `6adc947030f40ed84963c476805ea98ad4d3caf60ae0eb328cecea1d8eccc2d7`;
vars `0ac630bfcdaebf7719303e5ada69b0569d94133859b91f74328abe30fc38bd73`.
No master profile edit, Firefox rebuild, integration or restamp.

## Evidence and limits

The bounded trace contains64 complete spans then its explicit first-overflow
marker. Among them are28 CPU0/TID4 syscall17 spans of10–33ms, totaling501ms,
and one syscall96 span132ms. AP syscall spans overlap; do not sum them as
exclusive CPU time. Syscall17 also handles non-MakFS4 descriptors. No FD,
length or return value was captured. This does not attribute all writes or
the entire15446ms Ctrl-A delay to the filesystem, nor establish later absence
of slow VM maps. Different paint measurements are not controlled causal proof.

Read-only CRC-validated metadata comparison shows guest-created SQLite files
and WAL growth in the private image. It does not associate an inode with an
individual trace event, prove exact timing, or constitute full fsck. The
master remains unchanged; guest-created profile mutations are preserved.

Source inspection identifies independent write amplification: `rewrite_inode`
allocates, rereads and rewrites the entire file for each patch/truncate while
the caller holds the VFS lock. This is unnecessary for unchanged whole prefix
blocks. Existing commit publication and durability ordering must stay intact.

## Repair contract

Retain only whole blocks before both old/new EOF and the first patched byte.
An old partial EOF block is always copied, preserving zero-tail semantics
across append, holes, shrink and regrow. Allocate the changed suffix at/after
the retained physical prefix end, because the existing on-disk extent map
requires ascending, nonoverlapping physical extents. No format change.

If ordered tail space or the fourteen-extent capacity is unavailable, roll
back every new reservation and use the existing whole-file COW path. Never
free retained prefix blocks on rollback, including allocations merged with
the final retained extent. Release only replaced old suffix blocks in the
next bitmap. New data is written to separate blocks; full-block patches need
no old-block read. No change to VFS locking, metadata commit, either flush,
generation/root publication, allocation limits or browser/test deadlines.

Existing recovery limits remain: allocation uses the active root's bitmap,
not the union of all historical root references, so old fallback roots are
not indefinite data snapshots. A final root-flush error can leave uncertain
publication while in-memory ACTIVE/cache still describe the previous root.
Failure tests cover immediate crash/remount, not safe continued writes after
that uncertain error. This increment does not claim to repair those inherited
recovery limitations.

Core tests cover extent splits, lower-bound allocation, fragmented ordered
tails, partial-allocation rollback, prefix-merge rollback, fourteen-extent
capacity and full-rewrite fallback space. The host fixture extracts real
production rewrite, commit, cache, root validation and block helpers; it
compiles the actual format codecs and fsck. Host adapters supply a sparse
sector device and time. The modeled device keeps writes volatile until a
flush, and injected failed flushes either persist all pending sectors or
none. It does not model arbitrary torn sectors or all hardware reorderings.
Read/write/flush injection is followed immediately by crash/remount, with
old/new whole-file equality, fsck and original active prefix/bitmap checks.
The656-boundary sweep starts at generation1 with two identical roots;
failure injection after later rotating generations is not covered. Remount
uses real root validation/selection/cache loading, not full `mount_or_format`.
Separately, nine uninterrupted successful commits revisit all three metadata
targets at least twice, checking warm incremental cache/dirty tracking,
prefix immutability and exact contents before final crash/remount.

Functional coverage includes cold/warm caches, aligned/unaligned appends,
holes, cross-block and full-block overwrite, shrink/regrow/zero truncate,
backward-space and fourteen-extent fallbacks. A deterministic aligned append
to a5MiB file reads no old data and writes one4KiB data block; metadata I/O is
excluded, so this is not a whole-syscall timing claim. Negative controls
restore whole-file rewriting, omit the metadata flush, or release retained
prefix blocks; each must fail its intended assertion. These are host
regressions, not real-device durability or browser qualification.

Development validation on this Mac: `cargo test -p makos-makfs4` exits0
(14tests,1.23s); production prefix-COW fixture exits0 (7.12s,656 injected
boundaries, nine uninterrupted commits, three negative controls);
`make unit check` exits0 (182.41s); `make image-aarch64` exits0 (8.99s).
Complete logs and independently cloned/hashed prior boot/kernel/loader:
`build/logs/makfs4-prefix-repair-20261015-PAtfUK/`. Independent production
and fixture reviews found no new correctness blocker within the stated
model; inherited recovery limits above remain explicit.

This repair is not a Firefox success claim. Fresh complete exact-commit
Mac/HVF qualification must keep every proof, the600-second probe,
first-character<500ms and Ctrl-A<10000ms. Reuse the original integrated image
after unchanged identity/provenance preflight. Stop on first failure and
preserve it. Only strict Firefox success permits visible private-clone login.
Historical Pi evidence remains separate; all audit Partial/Missing rows stay.
