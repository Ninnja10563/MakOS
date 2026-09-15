# Firefox fatal evidence capture — 2026-09-15

## Authoritative user-reported Mac result

Commit `9614841ffb349cfa235fd7348a27b4c7f42843ad`, Apple M3, 16 GiB,
macOS 26.6.2, QEMU 11.0.3/HVF. HEAD/main/origin/main/remote main matched;
worktree clean, no concurrent QEMU. The pasted report is supplied evidence.
No separate raw attachment is accessible on this Pi; Mac paths must not be
treated as local paths. The missing fatal reason cannot be inferred.

Passed: `make unit check`, image build, EL0 (contiguous record, three AP
entries, 96 blocking calls, joins 42/42/42, reap and unchanged boot hashes),
self-host (20 builds, 21 processes, exact ordered graphs/headers and locked
distinct-root overlap), Native/Python-role and Firefox-role SMP fixtures
(required migrations, zero drops), cursor (seven positions, zero changed
scanout pixels/GPU errors/timeouts), and integrated-image/provenance preflight.
Scheduler-role fixtures are not real Firefox execution.

Failed unchanged command:

```sh
AARCH64_FIREFOX_PACKAGE_IMAGE=build/makos-integrated-c23395ff4644b183.img make test-aarch64-firefox-runtime
```

Exit 2; real 16.99 s, user 2.99 s, sys 1.11 s. First assertion:
`AssertionError: Firefox probe reached kernel fatal:`. Raw serial reportedly
ends exactly at `MAKOS_FATAL:` with no suffix, immediately after:

```text
MAKOS_AARCH64_THREAD_CREATE_OK pid=4 tid=8 root=0x4012e000 stack=0x10e43c3d0 tls=0x10e43c4d0 shared=vm,files,credentials signals=group
```

No fatal reason, latency result, retry or final visible-login qualification
exists. The test stopped on first failure; no QEMU remained. Preflight passed:
this is not a missing-provenance failure. Reported identities:

- Boot: `8c005734dba6d3bdd315ead26355709c5d42c100d88ccddb0d0d54211f718045`.
- Integrated: `c23395ff4644b183991f2508bdd475ad2120110019f134ebd2b5af0c550a12dc`.

## Confirmed capture defect and bounded repair

`kernel/src/main.rs::fatal` already formatted its fatal line through one
`serial::print` call, guarded on AArch64. That prevents guest record
interleaving, but cannot make host pipe reads correspond to complete records.
The Firefox harness reads at most 4,096 bytes and immediately rejects any
captured `MAKOS_FATAL:` prefix. A read ending at its colon therefore saves no
suffix. Its outer cleanup previously terminated QEMU without draining queued
stdout. This demonstrates an evidence-loss path compatible with the report,
not the reason the guest entered `fatal`.

The fatal producer now emits, in one formatted call/serial critical section:

```text
MAKOS_FAILURE_DETAIL: <actual reason>
MAKOS_FATAL: <same actual reason>
```

The actual reason precedes the existing immediate-stop marker in the ordered
serial stream. There is no new allocation, retry, delay, special Firefox
case, or synthetic reason. The original fatal line and hardware halt remain.
The added output occurs only on the fatal path. It does not alter executable
validation, scheduling, thread limits, isolation or successful workloads.

After the existing shutdown/5-second wait, the harness nonblockingly drains
pending stdout only from a stopped QEMU, appends those bytes unchanged, and
saves the final raw Firefox serial log even on an exceptional exit. It never
waits for a newline, joins fragmented success records, reruns success checks,
or suppresses the fatal assertion. Closed-pipe/file diagnostic errors are
reported without replacing that assertion. Existing terminate/wait behavior
is retained; this is not a new late-fatal validator.

All five existing fatal predicates, runtime assertions and Make targets are
unchanged, including the 600-second Firefox probe, first-character <500 ms,
Ctrl-A <10,000 ms, and all other deadlines/thresholds. Firefox source, the
60-patch series, package/image provenance and verification are unchanged.

## Regression scope

`scripts/test_aarch64_tty_serial_atomic.py` extracts the real fatal formatter,
serial macros and output routines. Only the final hardware halt returns in
the host fixture; existing host mutex/byte-capture adapters stand in for
DAIF/PL011. The suite checks every two-chunk split (including the exact colon),
reason-before-prefix ordering, the unchanged full fatal line, one guard for
both records, and two concurrent fatal reporters retaining their own reasons.
The old guarded prefix-first formatter must fail the same reason-preservation
assertion. Existing TTY translation, concurrency and negative controls remain.

`scripts/test_aarch64_firefox_serial_log.py` retains its Ctrl-A timeout checks
and adds real OS-pipe tests for pending suffix bytes, EOF at the colon, partial
reason/no newline, binary bytes, a subsequent synthetic success marker,
open writer descriptors, a still-live process, closed stdout, failed logfile
writes and no configured log. The same exception object remains a failure.
The old no-drain behavior must lose queued bytes and fail the same assertion.
No missing reason or success record is manufactured.

## Local validation and next qualification

Local validation is on Raspberry Pi/Debian AArch64, using the repository's
LLVM 19.1.7 host tools. Focused pipe/formatter tests pass, including eight
serial atomicity cases and the reproducing negative controls. Full
`make unit check` passes: exit 0, real 124.032 s, user 100.128 s,
sys 22.865 s. Both architecture cross-checks remain included.

Locally retained logs (not committed build artifacts):

- `build/logs/firefox-fatal-capture-20260915-focused.log`, SHA-256
  `f43e88ed800cb15c09043d4e388118c9efdc317797b255dd4f4e949f010c11fc`.
- `build/logs/firefox-fatal-capture-20260915-unit-check.log`, SHA-256
  `99132da319105b3db6de1cc3b304c4738c3b191cb35c3c10a2f24a6831c9aff6`.
- `build/logs/firefox-fatal-capture-20260915-image.log`, SHA-256
  `eae77017a8976b76ee08ee4a771523723943239c14ef2ae747b2b0e13128c510`.
- `build/logs/firefox-fatal-capture-20260915-el0-runtime.log`, SHA-256
  `f103780ee34cbfe30a57c16be8d4da541ce6683128215d1bedb03dcd6b481ff3`.

`make image-aarch64` passes (exit 0, real 55.334 s). The unchanged
`make test-aarch64-el0-entry-runtime` then passes on Pi/QEMU 10.0.11/TCG
(exit 0, real 31.709 s): three distinct AP/TID pairs 1/5, 2/6 and 3/7,
common root `0x4012f000`, loader `pthread_create=0x280b0750`, RX
`0x80000000`, resume PC `0x80001000`, 96 blocking calls, contiguous result,
status-42 joins and process reap. No fatal, panic or EL0 rejection occurred.
This successful guest run does not exercise a real Firefox fatal; that
failure path's ordering and capture are covered by the host regressions.

Runtime session: `build/makos-el0-entry-azwadxec`; harness PID 100502,
QEMU PID 100507, inherited QMP socketpair fd 4, QEMU exit 0. Retained:
private `boot.img`, `data.img`, `vars.fd`, `serial.log`, and `session.json`.
Serial SHA-256:
`0f8c921ec2e2e30168264780aa7814eea2de7732eae8d967a97a070a6475a831`.
Session JSON SHA-256:
`f90257e641604acd5509b85f6bc25ee39630d8d2871493cd4f80e6de8dede917`.
The runtime's master-before, master-after and private-boot hashes all equal
`69a82818c13e360176a0533f87f1d671be8e34995305d83b1beb62449c6b179e`.
Final kernel ELF SHA-256:
`f24993d2cb5b087ebe2aa44df591c716c9a1b79b8b149b3e4e023ac84502451e`.
No QEMU or build/test process remains. No screenshot was captured by this
focused gate. Other Pi runtime gates were not rerun for this diagnostic-only
change; their current Mac results above are user-reported at the baseline.

The pre-rebuild boot and kernel were preserved in
`build/firefox-fatal-before-20260915-FKzCm8/boot.img` and `kernel.elf`.
Their respective SHA-256 identities are
`d487a9e8a8effcf6c11661aebd4cdbc95334a4b8ece785c0c30581b083cfc524` and
`598f28d115f2ef370769b8675d5c7db9365acff2df3de27a42f25333c526f4b6`.
Existing sessions, logs, data images and Firefox artifacts were not deleted.
The Mac's qualified integrated image is absent on this Pi; no substitute
Firefox image, real-Firefox runtime or final visible-login result is claimed.

This repair addresses observability only. The guest fatal remains undiagnosed
until a new run supplies its actual reason. Rebuild the kernel/boot image and
follow the unchanged sequential [Mac test-agent protocol](MACOS-HVF-TEST-AGENT-PROMPT.md)
at the exact pushed commit supplied in chat. Preserve the integrated image,
require its exact hash and unchanged preflight, and stop on first failure.
Capture the final raw serial file as well as the earlier exception snapshot.
Only after strict Firefox passes may the sole visible login be launched from
private clones. Audit Partial/Missing rows remain; full OS completion is not
claimed.
