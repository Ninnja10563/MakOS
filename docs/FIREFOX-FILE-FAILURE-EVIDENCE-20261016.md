# Firefox file-syscall failure evidence

## Preserved qualification

Exact clean `10eb3684e3ca35205e0978e8882918179128a96f` matched
HEAD/main/origin/main/live remote on Apple M3/macOS26.6.2, QEMU11.0.3/HVF.
Unit/check, image, all twelve runtime gates and unchanged61-patch/five-artifact
Firefox preflight passed. Firefox then produced:

```text
MAKOS_FIREFOX_FIRST_PAINT_OK process_start_to_verified_paint_ms=28145
MAKOS_FIREFOX_SELECTION_LATENCY_OK raw=132 enqueue_to_widget_ms=5176 limit_ms=10000
MAKOS_FIREFOX_INPUT_LATENCY_OK raw=104 enqueue_to_widget_ms=110 limit_ms=500
AssertionError: Firefox navigation did not complete with real page pixels loaded=False changed=False tls_verified=False visual=not checked
```

The unchanged90s initial navigation to `https://example.com` did not obtain
successful matching page-stop/HTTP/TLS proof. Pixel comparison is conditional
on those prerequisites and was not reached; `changed=False` does not establish
physical pixel equality. Later interaction/browser SMP checks and login did
not run. Sequence stopped immediately without unchanged retry. QEMU87333 and
harness87328 exited; make2/observer0, real141.18s/user485.88s/sys18.46s.

Evidence: `build/logs/macos-makfs4-prefix-20261015-clEgkG/`, including
`FINAL-REPORT.txt`, complete logs, final raw serial, all retained private
boot/data/vars, screenshots, exact markers/hashes, source/host identities.
Session `build/makos-aarch64-test-jyy3swd9`, QMP socketpairfd4. Required
Firefox readiness pressure1 with idle97.34%/97.25%; during pressure1,
swap195.25MiB, sole QEMU/no heavy build. No diagnostic fatal reason observed.

Master boot unchanged before/after every runtime:
`ff01b9b277de4d7ab5d71e816f6ecae1c940f744f46b5619d3aa3d7b82ffc835`.
Integrated image unchanged:
`5baa9467ca510b8a1a207365abed12fe8b03f228432a80b2238b3b46042d00ce`.
Manifest unchanged:
`4cb2b6ef3a7d4c4d852755ce33ebd0cbbbb4de5291b53a3d6dfc7a68fe2a6f6a`.
No Firefox rebuild/reintegration/restamp or master profile edit.

## Known evidence, unresolved cause

Complete widget sequences5–23 spell `https://example.com`; Enter sequence24
dispatches with0ms guest enqueue-to-main delay. No lost key demonstrated.
SearchService initialization error follows, but UrlbarController's telemetry
record method catches that exception. It is not proof that URL loading aborts.
TLS successes concern Mozilla settings, not the requested example.com host.

Repeated `760be972.sqlite` connection errors are `NS_ERROR_STORAGE_IOERR`.
Firefox masks extended SQLite result bits, losing the distinction between
read/stat/lock/shared-map/other failures. Profile datastore opens its
`${storeID}.sqlite` with `openNotExclusive:true`, then selects WAL. SQLite's
nonexclusive WAL shared index normally maps a regular `-shm` file; kernel
`map_shared` currently accepts only the dedicated shared-memory descriptor
type, not regular MakFS4 files. That is a source-proven compatibility gap,
not yet the proven syscall that caused this connection-open failure. Default
exclusive SQLite connections can use a heap shared index instead. Do not
change profile locking/preferences or infer the missing extended result.

Read-only inspection with production MakFS4 decoders confirms the preserved
master and failed private image already contain `760be972.sqlite` with
SQLite read/write header versions2/2 (WAL), size32768/page_size32768, plus a
regular32768-byte `-shm` and zero-byte `-wal`. First100 shared-index bytes
are zero in both; remaining bytes were not examined. No SQLite engine was
opened and no image/profile bytes changed. The report is
`SQLITE-READONLY-INSPECTION.txt` in the evidence directory; it is not full
fsck/database integrity proof. This strengthens the mapping lead without
identifying the actual rejected syscall or proving navigation causality.

The older timing diagnostic records inclusive slow spans, not return values,
arguments or FD identity. Its64-record cap exhausts early. These observations
motivate a bounded error-result diagnostic, not a guessed behavioral repair.

## Observation contract

Observe only negative returns from selected normal-dispatch AArch64 file
syscalls:11,12,17,36,53,87,89,92,96,97,100,101,132,133. Copy raw x0–x5
before x0 becomes the result; emit after handler guards are dropped and before
signal delivery can rewrite the frame. Earlier blocking/socket/scheduling
branches returning before normal dispatch remain outside this boundary.
In particular, native FD_CONTROL record-lock operations6–8 take an earlier
branch, so this diagnostic does not observe their fcntl lock failures.
No claim of complete syscall, POSIX errno or userspace-failure coverage.

Filter to Firefox role. Each operation has an independent four-record budget,
then exactly one first-overflow record. Saturated operations do no subsequent
identity/clock work. Maximum56 failure records plus14 first-overflow records
per boot; overflow does not claim a final dropped total. Successful/unknown
calls do no role/identity/clock work. No rearm, timed deadline or changed gate.

Record operation, signed raw return, six raw entry arguments, current TID/CPU,
root and architectural-counter time. Pointers are values only: never
dereference user buffers, read file contents, acquire VFS locks or add disk
I/O for diagnostics. For map53, arguments include length/protection/flags/FD/
offset; other operations retain their own ABI meanings. A value of-1 may be
the historical generic MakOS sentinel, not a reconstructed POSIX error.

The serial sink retains its existing lock/transport behavior; record count
is bounded but existing PL011 transmission is not newly time-bounded.
No syscall results, permissions, mapping policy, locking, workload, test
assertions or deadlines change. Host tests use the actual diagnostic module
with explicit role/clock/identity/serial adapters and negative controls;
they do not establish runtime browser success.

Development validation on this Mac: focused diagnostic host test exits0
(2.88s), `make unit check` exits0 (179.54s), `make image-aarch64` exits0
(9.15s). Five behavioral cases and four negative controls cover real module
logic, concurrent admission, raw arguments/results and fast paths; structural
checks bind the actual normal-dispatch hook and architectural clock. Logs and
independent pre-build boot/kernel/loader clones remain in
`build/logs/file-failure-trace-20261016-7AQF3i/`. Independent review found no
blocking diagnostic defect within the explicit coverage limits above.

After diagnostic validation and commit, run the full unchanged Mac protocol
in a fresh evidence directory. Keep600s probe, first-character<500ms and
Ctrl-A<10000ms, original image/provenance and every rendering/network/SMP
assertion. Stop first failure; preserve raw evidence. Only strict Firefox
success permits private-clone visible login. No audit upgrade or OS completion.
