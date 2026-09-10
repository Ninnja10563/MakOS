# Dynamic-musl evidence emission repair — 2026-09-11

## Authoritative Mac report

The user tested `799354fd96a9a7db999acaafd05ea485fb5693b0` on Apple M3,
16 GB, macOS 26.6.2 (25G83), QEMU 11.0.3/HVF. Local and remote refs matched,
the worktree stayed clean, and no source/test/provenance edits or concurrent
QEMU/heavy builds occurred. Pre-runtime CPU was 93.32% idle with normal
memory pressure. These are supplied Mac results, not Pi runtime evidence or
Mac paths that are available locally.

- `make unit check`: exit 0, 150.49 s.
- `make image-aarch64`: exit 0, 21.22 s.
- `make test-aarch64-el0-entry-runtime`: exit 2, 15.01 s.
- First failure: `AssertionError: expected one complete dynamic-musl EL0-entry result`.

The exact result body was interrupted after its resume address:

```text
MAKOS_MUSL_EL0_ENTRY_OK loader=musl threads=3 singleton=0x2,0x4,0x8 tids=5,6,7 pthread_create=0x280afcd4 rx=0x80000000 resume=0x80001000MAKOS_AARCH64_THREAD_EXIT_OK pid=4 tid=7 status=0 reap=task-only shared_root=retained
MAKOS_CLEAR_CHILD_TID_OK pid=6 address=0x280e03e8 zeroed=1 wake=0
MAKOS_AARCH64_THREAD_EXIT_OK pid=4 tid=5 status=0 reap=task-only shared_root=retained
MAKOS_AARCH64_THREAD_EXIT_OK pid=4 tid=6 status=0 reap=task-only shared_root=retained
 calls=96 statuses=42,42,42 block=sleep-until
```

All three validated-before-ERET records existed: CPU/TID `1/5,2/6,3/7`,
root `0x4012e000`, resume PC `0x80001000`. The dynamic process reaped status
42, without fatal, panic, or EL0 rejection. This is useful execution evidence,
but the required contiguous record was absent: **the gate failed**. No later
runtime, Firefox preflight/runtime, or final visible login was run.

Master boot before/after and private boot SHA-256 matched:
`32f6cf012dee4b0a34066f55e87fc1aca40c3fefb9b88e39414a76e3721eadd3`.
The preserved integrated image still hashes to
`c23395ff4644b183991f2508bdd475ad2120110019f134ebd2b5af0c550a12dc`.
Mac QEMU PID 10863 and harness PID 10847 exited; session
`build/makos-el0-entry-2m4pa0lo`, inherited QMP fd 4. Private boot/data/vars,
full serial, session JSON and harness logs remain preserved on the Mac.
The gate captured no screenshot. No guest remains from that run.

## Two real emission boundaries

1. `ports/musl/dynamic-probe.c` used `printf` followed by `fflush`. Upstream
   musl's `__fwritex`/`__stdio_write` can emit the buffered formatted prefix
   and final newline-bearing literal as two vectors. MakOS's existing musl
   `L_writev` translation performs a separate native `M_write=17` for each
   vector. The observed seam exactly matches the final literal boundary.
   Flushing afterward cannot make earlier native writes atomic.
2. Even one normal `write` enters `aarch64_tty::write`, whose line discipline
   splits ONLCR output into body and CRLF sink calls. The old `TerminalSink`
   took the serial lock once per slice, allowing a kernel record between the
   complete body and its terminating line ending.

Thread-exit logging after `pthread_join` is not itself an error. The exit
path publishes clear-child-TID and wakes waiters before its cleanup log and
later thread-exit log. Delaying the probe or waiting for diagnostic lines
would not fix the serial-write boundary.

## Repair and preserved behavior

The guest now formats the unchanged marker with `snprintf` into a 512-byte
buffer, checks for nonpositive/error/truncated output, then performs exactly
one ordinary `write(STDOUT_FILENO, ...)`. Any error, zero, short or overlong
write result fails the probe; it does not retry a suffix. All three genuine
joins and workload validations still precede this emission. The existing
dynamic success marker and status-42 process reap are unchanged.

`serial::write_tty_bytes` holds the existing AArch64 IRQ-masked cross-CPU serial
guard across the complete TTY write and line-ending conversion. The TTY
graphics transformation runs afterward through a graphics-only sink, after
the serial guard is released, under the existing TTY state lock. The lock
order remains TTY → serial, with no graphics callback under the serial lock.
Input/echo still use the existing sink. File-descriptor checks, readable-user
range validation, syscall/ABI, output bytes and graphics translation remain
unchanged. In particular, the existing ONLCR-then-serial LF policy yields
CRCRLF when enabled; this repair deliberately does not silently alter it.

This is a producer/serial atomicity fix, not a parser accommodation. The
focused runtime harness, its parser and original parser tests are unchanged.
No fragmented records are joined. No sleeps, deadlines, work counts,
assertions, Firefox thresholds, patches or provenance rules change. General
musl `writev` still uses separate native writes; this does not claim new
atomic vector-I/O semantics.

## Regression and qualification boundary

`scripts/test_aarch64_el0_emission.py` executes the actual production C emitter
with controlled host format/write adapters: 11 cases cover exact one-write
success, maximum-width fields, formatting error/zero/overflow and every
non-full write result without retries. A generated split-write negative
control injects a kernel record at each write boundary. Both it and the exact
Mac fragment above remain rejected by the unchanged runtime parser.

`scripts/test_aarch64_tty_serial_atomic.py` executes production Rust serial
functions with host mutex/MMIO-byte adapters and the real TTY crate: six tests
cover whole-record locking, exact old bytes for both newline modes/all byte
values/multiline/empty output, and deterministic contention with a kernel
formatted record. Reintroducing old per-chunk locking or removing the serial
guard makes negative controls fail. Structural checks retain IRQ ordering,
normal fd/user validation, echo behavior and graphics outside the serial lock.
Both new suites are included in `make unit check`; they are host tests, not
AArch64 hardware or Firefox execution.

The C suite also passes with repository-staged LLVM clang 19 on the Pi
(`HOST_CC="$MAKOS_REAL_CLANG" python3 scripts/test_aarch64_el0_emission.py`),
in addition to the default host C compiler. That is Linux host-compiler
coverage, not evidence that Darwin has run the new tests.

Before new runtime runs, the prior fixed-name self-host/Native/Firefox-role/
cursor serial logs and all eight cursor captures were copied to
`build/el0-emission-prior-evidence-20260911-3XhPPr/`. Earlier session directories
and logs remain intact. These local artifacts are not repository source.

## Local Pi validation

Host: Raspberry Pi 4 Model B Rev 1.5, Debian 13.6/AArch64, approximately 2 GB
RAM; repository-staged LLVM 19 and QEMU 10.0.11. The current user lacks KVM
group access, so functional guest runs use QEMU/TCG. These are not Mac/HVF
qualification or interactive/performance measurements. No audit
Partial/Missing row is upgraded.

- `make image-aarch64`: exit 0, artifact checks passed. The first rebuild
  included static/shared musl and took about 15 minutes with `MUSL_JOBS=1`.
  Log: `build/logs/aarch64-image-el0-emission-20260911.log`.
- `make unit check`: exit 0, 235.401 s including x86_64/AArch64 cross-checks,
  all existing suites and both added host suites. Log:
  `build/logs/full-unit-check-el0-emission-20260911.log`.

`make test-aarch64-el0-entry-runtime` passed twice unchanged, exit 0:

| Run | Command wall time | Retained session | QEMU / harness PID |
| --- | --- | --- | --- |
| 1 | 175.746 s, including release prerequisite rebuild | `build/makos-el0-entry-pu9x1cd9` | `320511 / 320508` |
| 2 | 35.005 s, cached release prerequisite | `build/makos-el0-entry-4grz6o_g` | `320851 / 320822` |

Both runs report TIDs `5,6,7` on AP1/AP2/AP3, singleton masks `0x2,0x4,0x8`,
`pthread_create=0x280b0750`, RX `0x80000000`, resume `0x80001000`, 96 blocking
calls, three status-42 joins and status-42 process reap. Each validated entry
uses the same root `0x4012f000`. Direct inspection of each **raw** serial record
confirms one complete marker through its original `\r\r\n` ending, with no
embedded diagnostic and no fragment joining. There is no fatal/rejection.
Both QEMU processes exited 0. Each session retains private `boot.img`,
`data.img`, `vars.fd`, `serial.log` and `session.json`; QMP was inherited fd 4.
No screenshot was captured by this focused gate.

Both sessions' before/after master boot and after private boot hashes equal
`d487a9e8a8effcf6c11661aebd4cdbc95334a4b8ece785c0c30581b083cfc524`.
Kernel ELF SHA-256:
`598f28d115f2ef370769b8675d5c7db9365acff2df3de27a42f25333c526f4b6`.

Local log SHA-256 identities:

| File | SHA-256 |
| --- | --- |
| `build/logs/aarch64-image-el0-emission-20260911.log` | `de6e363ce5a7ca96ff269cedaa8a5cd21cfce8ae98580ba0981917834f5da0e8` |
| `build/logs/full-unit-check-el0-emission-20260911.log` | `f05b1c0347e20cdd24c1692c2537d6a614b80365efadb86714cc6c9fe5a9982a` |
| `build/logs/aarch64-el0-emission-runtime-20260911-run1.log` | `7d8e955607d01e35f3697125b8dff2f7d7e5e25e2a6e2ae3d891aaf58603e271` |
| `build/logs/aarch64-el0-emission-runtime-20260911-run2.log` | `7e6ccd59f90eddf465c52aa187d1c46a3f6a5b9cf84f9e6d5466047b53c38e73` |
| `build/makos-el0-entry-pu9x1cd9/serial.log` | `4a6c931c9707f9504e1cba3d4334ec0168d131d10a4cbd4a036724e825293461` |
| `build/makos-el0-entry-4grz6o_g/serial.log` | `e5c44430f580910d9c8ecf781c00c51190a7f45580b10c4f3a4105066237d438` |

The broader unchanged runtime gates also passed sequentially (all Pi/TCG,
exit 0; command times include their normal image prerequisite):

| Make target | Wall time | Required result |
| --- | --- | --- |
| `test-aarch64-selfhost-runtime` | 201.434 s | 20 CLI builds, 21 processes, graphs `4,3,2,2,3,2,2,3`, three parallel status-42 children, 29 migrations, zero evidence drops/GPU errors/timeouts |
| `test-aarch64-native-smp-runtime` | 44.771 s | Native migrations 3, Python-role migrations 1; AP1/AP2/AP3 dispatch and placement; both status 42 and zero evidence drops |
| `test-aarch64-production-smp-runtime` | 44.756 s | Firefox-role migrations 4, zero evidence drops, exact watcher/group syscall-149 handoff, status 42; this is not Firefox execution |
| `test-aarch64-cursor-runtime` | 31.896 s | Seven positions, zero changed scanout pixels, virtio-GPU cursor plane, host cursor hidden, zero delayed recoveries/errors/timeouts |

Self-host retains exact generated-header lengths/hashes `164/bbbc9068d3d73e49`
and `1215/ccf73fc02c2f9ceb`. Its locked overlap snapshot has PIDs/group PIDs
`13,15,14`, CPUs `1,2,3`, roots `0x4012f000,0x401f3000,0x40191000`, distinct
groups/address spaces and singleton ownership. Placements are `8,7,6`,
dispatches `174,176,174`, migration source/target masks `0xe/0xe`, CPU0 owner
compositions 41, AP deferrals 48, pending 0. Master boot before/after the full
self-host Make command matches the hash above, as does the final boot image.

Observed QEMU/harness PIDs were self-host `321089/321088`, Native/Python-role
`321941/321940`, Firefox-role `322325/322307`, and cursor `322521/322480`.
The existing broader harnesses remove their private temporary disks on exit;
they do not retain sessions like the EL0 gate. Observed temporary directories
included `build/makos-selfhost-focused-sdnrh_ax`,
`build/makos-native-smp-focused-y4x6pj4n`, and
`build/makos-cursor-focused-_8lnm1dx` (QMP inherited fd 4). Do not report those
removed directories as retained disk artifacts. Complete harness and raw
serial logs were retained separately:

| File under `build/logs/` | SHA-256 |
| --- | --- |
| `aarch64-selfhost-el0-emission-20260911.log` | `564ad7fba1367c6685165250c2b8c1906682c173b74524290b57c123827244d2` |
| `aarch64-selfhost-el0-emission-20260911-serial.log` | `7f15f203b4f5784ff7f4b0c1fb5f7c61bfa6b450342117056fe58c79b1ae3421` |
| `aarch64-native-smp-el0-emission-20260911.log` | `513b016f041c50dddaa21cdd19f3955b957e114ce646a7c55a7a9a481d84dc39` |
| `aarch64-native-smp-el0-emission-20260911-serial.log` | `e57aed75c6072853bfd45184e02b8fa4b9184e2a3fba697528528dde47490063` |
| `aarch64-production-smp-el0-emission-20260911.log` | `02bb0e798bb08335ecd36a2484867ff0270961e8bdfe958d3966848677bff054` |
| `aarch64-production-smp-el0-emission-20260911-serial.log` | `639a4e4be41eed34348581e9d4fcba8ab9c49ffa0b450781a75e30ff0b624733` |
| `aarch64-cursor-el0-emission-20260911.log` | `9ab1785e04bb50624319702c8b0be38238eb39735a4527e250cce075c08535ed` |
| `aarch64-cursor-el0-emission-20260911-serial.log` | `7ed9f299daab333d802087dcc34c78d7e6f7cec0d939b3e44f94d8c1b8fb6542` |

The eight current `build/makos-cursor-focused-*.ppm` scanout captures all hash
to `53179ecad66d43194bfc58a93a3f8bbb3d1d11bda432e1110c385f5cd59d8382`.
The local `build/logs/el0-emission-qualification-20260911.sha256` manifest
records individual file identities. Prior captures remain in the archive
listed above. No QEMU or MakOS build/test process remains running. No final
visible guest was launched: real Firefox and visible-login qualification
remain pending on the Mac.

All pre-existing runtime harnesses/parsers and Firefox sources/provenance are
byte-identical to `799354f`. Removing only the two new host-suite lines from
each of `unit` and `check` yields the exact baseline Makefile bytes; no existing
Make command, gate variable, assertion, threshold or deadline changed.

Use the exact pushed repair HEAD and the unchanged sequential commands in
[the Mac test-agent protocol](MACOS-HVF-TEST-AGENT-PROMPT.md). Rebuild the boot
image. Reuse the preserved integrated image only after its exact hash and
unchanged preflight pass. Do not rebuild/restamp Firefox solely for this
probe/TTY change. Strict real Firefox retains the 600-second probe,
first-character <500 ms, Ctrl-A <10,000 ms, and every other gate assertion.
Only after qualification passes may the sole final visible login be launched
from private clones with PID/session/data/QMP and capture evidence recorded.
