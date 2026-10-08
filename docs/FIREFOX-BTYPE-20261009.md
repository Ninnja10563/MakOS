# AArch64 branch state repair

The Mac/HVF run of `2bafc9c779f95ea9a4675b8c8c3c72b52aa88481`
passes every pre-Firefox gate, including the concurrent VM regression, then
rejects a saved Firefox context before first paint. The entry guard mistakes
architectural branch-type state for forbidden processor state. This repair
recognizes that field without removing executable, isolation, stack, or
privilege checks. Strict real Firefox and final visible login still require
Mac/HVF qualification; no audit Partial/Missing row is upgraded.

## Authoritative Mac evidence

Apple M3, 16 GiB, macOS 26.6.2 (25G83), QEMU 11.0.3/HVF. Repository refs
matched and the worktree stayed clean. Pre-Firefox sampled CPU was 96.21%
idle, memory pressure was normal, and no other guest or heavy build ran.
Unit/check, image, EL0, VM-fault, self-host, Native/Python-role, Firefox-role,
cursor, exact integrated-image hash and unchanged provenance preflight passed.
Real Firefox exited 2 after 24.88 seconds, before paint or input measurements.

The attached ZIP's final raw serial, not its earlier truncated exception
snapshot, ends with:

```text
AArch64 EL0 entry rejected cpu=1 active_root=0x4012e000 context_root=0x4012e000 elr=0x8423cba8 sp=0x896b7ff0 stack_valid=1 spsr=0x80000400 instruction_mapping=Executable
MAKOS_FAILURE_DETAIL: AArch64 EL0 entry precondition failed
MAKOS_FATAL: AArch64 EL0 entry precondition failed
```

The 31,525-byte final serial SHA-256 was verified locally as
`2a65a7455ea541b6ce9d525d8837483b90446448a556c3f681014a1d96bea743`.
The exact rejected TID and instruction that established branch state are not
recorded. Earlier thread exits do not identify that TID. No duplicate mapping
fatal appears in this run; that alone does not prove absence of every VM race.

Failed QEMU PID 95612 and harness PID 95605 exited. The Mac session was
`build/makos-aarch64-test-8fer1x5m`, QMP inherited socketpair fd 4. External
hardlinks retained its private boot/data/vars under the Mac evidence directory
`build/logs/macos-hvf-2bafc9c779f9-20261009`; the ZIP excludes those disks.
The login capture is a headless harness artifact, not a final visible login.
Mac paths are not Pi paths.

## Processor state policy

`0x80000400` contains NZCV.N and BTYPE=1. ARM defines BTYPE at SPSR_EL1
bits 11:10 when FEAT_BTI exists. An indirect branch can establish that state
even on an unguarded code page; it must survive exception capture and return.
The [QEMU BR and BLR implementation](https://github.com/qemu/qemu/blob/v11.0.3/target/arm/tcg/translate-a64.c)
sets BTYPE=1 for BR X16/X17 and BTYPE=2 for BLR.
The [Arm feature register specification](https://documentation-service.arm.com/static/6526e1bd9e189a266cef8412)
defines ID_AA64PFR1_EL1.BT=1 as BTI support, 0 as absent, and other values
as reserved. This is an architecture policy correction, not a Mac-only
exception or a change to browser thresholds.

`kernel/src/arch/aarch64.rs` now permits NZCV plus BTYPE only when the
destination CPU advertises BT=1. It reads the local feature register only
when a context contains BTYPE; absent or reserved feature IDs keep the
NZCV-only rule. All other SPSR bits remain forbidden, including execution
mode, interrupt masks and illegal-state bits. Selected-root executable
mapping/reservation, PC alignment/range, stack permissions/alignment and
root checks are unchanged. The code does not clear or synthesize BTYPE.
Vector capture, scheduler context copies and assembly restoration remain
lossless. Failure diagnostics additionally identify TID and local PFR1.

This does not add guarded mappings or claim complete BTI enforcement for
applications. It preserves valid saved branch state on capable CPUs.

## Regression scope

Host tests execute extracted production predicates and context-copy methods.
The original no-BTI exhaustive NZCV test and mapping/isolation tests remain.
Additional tests cover all NZCV/BTYPE combinations, the exact Mac PC/SPSR,
absent/reserved capabilities, every other status bit, hostile mappings/stacks
and roots with BTYPE, and lossless capture/restore. Generated-only negative
controls must reject the old NZCV-only guard and unsafe feature/mask or
state-clearing mutations.

The additional guest fixture executes a two-instruction BR X16/X17 loop
after setting NZCV from EL0. A real timer exception captures branch-generated
SPSR `0x80000400` on AP1. The immutable probe migrates through the existing
Running-to-Ready/unowned path to AP2, whose outer entry guard validates the
same saved state before ERET. A subsequent AP2 timer exception proves EL0
resumption; only then does the kernel terminate and reap this bounded probe.
It does not claim voluntary userspace return or browser execution. The
post-return loop can regenerate BTYPE, so lossless restoration is also
checked directly in host context-copy tests and assembly wiring assertions.

The separate runtime gate reuses the unchanged EL0 harness, private disks,
deadlines and assertions. It additionally requires complete, ordered,
identity-matched hardware capture, validated-entry, resumed-IRQ and reap
records. Missing BTI is explicit unsupported evidence, never a BTYPE pass.
All existing runtime scripts, latency thresholds and provenance remain intact.

## Local Raspberry Pi validation

Development host: Raspberry Pi, Debian 13.7, Linux
6.18.39+rpt-rpi-v8/aarch64, Rust 1.98.0, host GCC 14.2 and staged LLVM 19.1.7.
Runtime evidence uses QEMU 10.0.11, TCG, CPU max, four vCPUs and 1 GiB guest
RAM. It is functional Pi evidence, not Mac/HVF or Firefox latency evidence.

The image build passed in 118.967 s and full `make unit check` passed in
254.101 s. The 21 entry-policy tests and six negative controls pass; the
14 additional runtime-evidence/wiring tests and comparator mutation control
pass. Existing assertions and no-BTI tests are retained. No production musl,
Firefox, existing runtime harness or provenance file changes are made.

Sequential Pi/TCG EL0 (31.132 s), VM-fault (27.229 s), and the added BTYPE
gate (30.197 s) pass. The BTYPE guest records actual AP1 capture, AP2
validated entry and later AP2 IRQ, all with SPSR `0x80000400`, TID 1 and
root `0x40000000`. Entry PC equals captured PC `0x10000084`; resumed loop PC
is `0x10000080`. Its status-42 reap reclaims 72 frames and total frame balance
is unchanged. The unchanged dynamic-musl proof retains AP1/AP2/AP3 workers,
96 blocking calls, three status-42 joins and process reap. VM proof retains
16 shared-page rounds, 48 distinct pages and 288 coherent checks.

Retained EL0 session: `build/makos-el0-entry-czpfkpel`, QEMU 3103948.
Retained VM session:
`build/makos-vm-fault-rz5ov2et/makos-el0-entry-u207wq7s`, QEMU 3104122.
Retained BTYPE session:
`build/makos-btype-hvq6exnq/makos-el0-entry-2j13bhf2`, QEMU 3104201.
Each has private boot/data/vars, session.json and full serial, QMP inherited
socketpair fd 4, QEMU exit 0, and matching before/after/private boot hashes.
BTYPE serial SHA-256:
`972f37083a0994ee4d7bccc99e8ba287b6226431ec04eef8d0c909dc321b8efe`.

Kernel ELF SHA-256:
`e8f22f35057aa80f73bcfb8b32b4cb41d9d1159c116f7d4393a831fea7bac13a`.
Boot image SHA-256:
`2cc552362162278866c7ca5fab69a10ecaa9e8801c9781d3f0532f88cf2ef645`.
The image target regenerates the FAT image: sample its identity after the
build step completes, not while that file is being written.

Logs and previous boot/kernel/fixed serial/cursor artifacts are preserved in
`build/btype-repair-20261009-h9ugqL/`. The previous boot hash is
`88d1891a3e5c1611b158d57734474642738ccaea0e077d81166fb15195627a85`.
The qualified Mac integrated Firefox image is not present on this Pi.

The unchanged self-host gate passes in 191.199 s: 20 CLI builds, 21
processes, graphs `4,3,2,2,3,2,2,3`, exact generated headers, three parallel
status-42 children and the locked distinct-root snapshot for PIDs 13/14/15
on CPUs 1/2/3. It reports 31 migrations, zero drops, and zero GPU
recoveries/timeouts/errors. Boot pre/post hashes match. QEMU 3104347,
harness 3104343, original session `build/makos-selfhost-focused-00e3bgvz`,
QMP inherited fd 4: hardlinks preserve boot/data/vars in
`build/btype-repair-20261009-h9ugqL/selfhost-private/` after normal harness
cleanup; full serial and process arguments are archived beside them.

Native/Python-role passes in 39.286 s, with 3/1 migrations respectively.
Firefox-role input/SMP passes in 35.210 s, with 3 migrations and exact
syscall-149 watcher-to-leader handoff. Both keep zero evidence drops, the
original role/ownership assertions and status 42; neither executes Firefox
or CPython. Private boot/data/vars are hardlinked under `native-private/`
and `production-private/` beside the complete serial/process logs.
Original sessions were `build/makos-native-smp-focused-7fyxty4c`
(QEMU 3104813, harness 3104794) and
`build/makos-production-smp-focused-79kn8nwq` (QEMU 3105012, harness 3105010),
both with inherited QMP fd 4. These guests exited before the next gate.

Cursor passes in 21.936 s: seven positions, zero changed scanout pixels,
virtio-GPU cursor plane, hidden host cursor, zero delayed recoveries,
timeouts or errors. QEMU 3105221, harness 3105220, original session
`build/makos-cursor-focused-y586xabh`, QMP inherited fd 4. Its normal harness
cleanup removed the new temporary disks before the external hardlink attempt;
they are not retained. The full serial and all eight PPM captures are
archived in the evidence directory. Earlier cursor artifacts remain preserved
separately. No QEMU/build/test remains. Real Firefox and final visible login
were not run locally; this is not full OS completion.

## Mac retest inputs

Use the exact pushed repair commit supplied in chat and the sequential
[Mac testing protocol](MACOS-HVF-TEST-AGENT-PROMPT.md). Rebuild the boot image;
do not rebuild or restamp Firefox for this kernel-only repair. Preserve and
verify the existing master:

```text
build/makos-fresh-firefox-VMF8ae/makos-integrated-3b68500032eec2d4.img
SHA-256 3b68500032eec2d4e0a225c44877a070f10de44e3f164cf0700f65bc827ecec3
```

The 600-second Firefox probe, first character below 500 ms, Ctrl-A below
10,000 ms, all other assertions and provenance remain mandatory. Stop on
first failure, preserve final raw serial and private disks, and never run
concurrent QEMU. Final visible login follows only a strict Firefox pass.
