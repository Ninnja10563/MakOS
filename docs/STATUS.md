# Implementation status

Last updated: 2026-08-26.

## Implemented

- 2026-08-26 the bounded AArch64 guest preprocessor now evaluates the next
  genuine C expression tiers with checked signed 32-bit semantics: unary
  `+ - ! ~`, `* / %`, `+ -`, `<< >>`, equality/relations, `& ^ |`, and
  short-circuit `&& ||`, using C precedence and left associativity. Active
  zero divisors, invalid shift counts, negative or overflowing left shifts,
  and signed overflow fail closed; unevaluated logical operands are still
  parsed without triggering those semantic traps. Literal/object-macro
  magnitude remains capped at 65,535, intermediate results at `int32_t`, and
  each expanded header at 1,024 bytes. The real two-header guest build covers
  every new tier, short-circuit behavior, and separate active zero-divisor,
  shift-range, and overflow denials before building, loading, and reaping
  status 42. Focused Pi/QEMU 10.0.11 TCG evidence ran all 15 toolchain
  processes with placements `3,4,8`, dispatches `178,178,185`, 39 migrations,
  zero evidence drops, 238 CPU0 compositions for 247 AP deferrals, and no
  pending handoff. Full `make unit check`, Firefox-role production SMP
  (`9695,13737,10228`, exact Ctrl-A, status 42), Native SMP
  (`10154,13198,9708`, status 42), and the seven-position/zero-scanout-pixel
  cursor runtime pass on the Pi. This remains a bounded seed: ternary
  expressions, general function-like/text macros, token operations, system
  headers, and substantial in-guest MakOS builds remain absent, so the SDK and
  self-hosting audit rows remain Partial. A fresh visible login from this exact
  image is active as the sole QEMU, PID 850875, under
  `makos-visible-selfhost-if-arithmetic-final3.service`, with private session
  `build/makos-pi-visible-selfhost-if-arithmetic-final3-IgEL0cQl`. Its
  read-only boot clone SHA-256 is
  `7c7be2f22ef732de8e898f960df0ea1108ef023e642cd854decc2840eadb9e05`;
  the QMP 800x600 login capture SHA-256 is
  `53179ecad66d43194bfc58a93a3f8bbb3d1d11bda432e1110c385f5cd59d8382`.
- 2026-08-26 the bounded AArch64 guest preprocessor now evaluates active
  `#if` and `#elif` expressions instead of supporting only name-defined
  conditionals. Its fail-closed recursive parser implements `defined(NAME)`
  and `defined NAME`, signed numeric literals and object macros, unknown-name
  zero, unary `!`, comparison/equality, `&&`, and `||` with C-like precedence.
  Branch state records whether an earlier arm was selected, so exactly one
  `#if`/`#elif`/`#else` arm expands and inactive missing includes remain
  untouched. The two-header fixture selects a genuine `#elif`, expands four
  macros, proves relational-before-equality precedence with `1 == 2 < 3`,
  preserves include-guard deduplication and expanded-source cache identity,
  and executes the resulting ELF with status 42. Malformed
  expressions and `#elif` after `#else` fail closed; the per-header expansion
  buffer was explicitly bounded at 768 bytes in that increment. The first
  precedence-correct runtime exposed a real AP-exit race: CPU0 could observe
  and reap a Toolchain zombie before the source AP retired its active TTBR0,
  causing fail-fast address-space destruction. The exit path now switches to
  the kernel root while still holding the scheduler lock, before parent wake
  can lead to cleanup; a structural ordering guard and the complete 15-process
  guest sequence cover the fix. Structural validation,
  release image/artifact checks, full `make unit && make check`, focused
  Pi/QEMU 10.0.11 TCG self-host runtime, unchanged Firefox-role production
  SMP, and the seven-position cursor runtime pass. This is not a general C
  preprocessor: function-like/text macros, token operations, system includes,
  and unbounded include
  graphs remain absent, so SDK/self-hosting remain Partial. Final focused
  self-host evidence reports placements `4,8,3`, dispatches `182,175,178`, 42
  migrations, zero evidence drops, and all 15 processes reaped with status 42.
  The unchanged Firefox-role regression reports dispatches
  `10683,13709,9922`, two automatic migrations, exact Ctrl-A delivery, and
  status 42; Native reports `10224,13213,9569` and status 42. A fresh visible
  login was active as PID 841525 under
  `makos-visible-selfhost-if-lifecycle-precedence-final.service`, using private
  session
  `build/makos-pi-visible-selfhost-if-lifecycle-precedence-final-SIcwrHk7`;
  its boot
  clone SHA-256 is
  `02a6520d560c5ba595386b57dce7ab6e8a9ca2a71ee81dfeb87b41b7301b6818`;
  it was stopped cleanly through QMP before the arithmetic-expression gates.
- 2026-08-26 default AArch64 Firefox and Native non-leader workers now receive
  kernel-owned least-reserved AP preferences while preserving their public
  `0xe` affinity. Normal scheduler selection honors that preference; timer
  preemption moves a default worker once when its AP is at least 64 cumulative
  dispatches above the least-loaded AP, after capturing GPR/SP/TLS/SIMD and
  publishing Ready/unowned under the scheduler lock. A real
  `sched_setaffinity` request clears the automatic preference and remains
  authoritative. The upstream-musl pthread fixture creates an imbalance by
  yielding one default worker while two peers sleep, without selecting a CPU.
  Fresh Raspberry Pi/QEMU 10.0.11 TCG Firefox-role runtime passed with AP mask
  `0xe`, placements `4,2,14`, dispatches `10376,13208,9671`, three automatic
  migrations, zero evidence drops, distinct-TID overlap on AP1/AP2, direct
  keyboard INTID 78, watcher TID 8 on AP2, and status 42. The Native twin
  passed with placements `3,2,13`, dispatches `10067,13298,9605`, two
  automatic migrations, zero drops, and status 42. Full `make unit && make
  check`, release image/artifact validation, both focused runtimes, the full
  self-host regression (15 Toolchain processes, 40 migrations, zero drops),
  and the seven-position/zero-scanout-pixel cursor runtime pass.
  This is functional Pi/TCG evidence, not strict real-Firefox latency evidence;
  the scheduler row remains Partial until unchanged idle-macOS/HVF Firefox and
  broader built-in/service contention qualify.
- 2026-08-26 AArch64 `Toolchain` leaders now migrate automatically after
  initial placement instead of remaining on one AP for life. At each timer
  preemption the kernel captures the complete user context under the scheduler
  lock, compares cumulative dispatch load, and, at an eight-dispatch imbalance,
  changes the singleton affinity to an idle lower-load AP before publishing the
  task Ready/unowned and sending the scheduler SGI. CPU0 emits bounded evidence
  only after child exit, preventing AP telemetry from splitting guest stdout;
  buffer saturation is counted, never fatal. The final Pi/QEMU 10.0.11 TCG
  self-host run naturally made 42 migrations across source and target masks
  `0xe`, corrected initial placements `4,4,7` to dispatch totals
  `180,184,180`, preserved GPR/SP/TLS/SIMD state and exclusive ownership, and
  recorded zero evidence drops. All 15 compiler/assembler/linker processes
  still exited 42; CPU0 handled 242 deferred compositions, APs made 247
  deferrals, and no GPU handoff remained pending. Full `make unit check`,
  Firefox-role production SMP (`9582,9289,10673` dispatches, watcher AP3,
  INTID 78), ordinary Native SMP (`13706,12466,12616`), and cursor runtime
  pass.
  This is genuine dynamic load-driven migration for one safe role, not general
  Firefox/Native/service balancing; the scheduler audit row remains Partial.
- 2026-08-26 AArch64 guest-native compiler, assembler, and linker process
  leaders now use a distinct `Toolchain` scheduler role and kernel-owned
  automatic placement instead of inheriting CPU0-only `Native` leader policy.
  At each spawn the scheduler snapshots AP1-3 dispatch load, prefers an idle
  AP, selects the least-dispatched candidate, rotates equal-load ties, and
  assigns a singleton affinity without caller selection. The Pi/QEMU 10.0.11
  TCG self-host gate ran all 15 real toolchain processes on AP1-3 with
  `cpu_mask=0xe`, placements `4,6,5`, dispatches `188,171,182`, and every
  process reaped status 42. This uncovered a real ownership fault when AP
  compiler output reached the graphics console; retained console updates now
  defer composition to CPU0. The same run proves 247 AP deferrals, 241 CPU0
  owner compositions, no pending handoff, and no off-owner GPU MMIO. Full
  `make unit` and `make check`, focused self-host, Firefox-role production SMP,
  ordinary Native SMP, and cursor runtime pass. This is automatic load-aware
  placement for bounded single-threaded toolchain leaders, not general dynamic
  balancing of every process/service role; the scheduler audit row remains
  Partial and real Firefox still requires the unchanged idle macOS/HVF gate.
- 2026-08-26 MakOS now guest-builds the first exact tracked repository-native
  component instead of only synthetic inline fixtures. Canonical
  `user/aarch64_selfhost_probe.c` (440 bytes) and
  `user/aarch64_selfhost_probe.S` (53 bytes) are read by `kernel/build.rs`;
  the same C file is compiled by the host AArch64 clang into a reference
  object, while exact generated byte arrays and FNV-1a identities are embedded
  into the sandboxed EL0 toolchain. Fixture mode verifies those identities and
  persists the exact sources plus a two-input `MAKBUILD1` graph in MakFS.
  Authenticated guest `makbuild` proves cold `0/2` and warm `2/0` cache
  results, and shell `run` loads and reaps the guest-produced ELF with status
  42. Pi/QEMU 10.0.11 TCG focused runtime, structural guard, AArch64 artifact
  validation, full `make unit` and `make check`, Firefox-role production SMP,
  ordinary Native SMP, and cursor runtime all pass. This is genuine source
  identity and guest compilation of a repository component, but it is not a
  substantial MakOS build: the SDK and self-hosting audit rows remain Partial.
- 2026-08-26 the AArch64 guest-native build driver resolves bounded recursive
  absolute quoted headers on exact directive lines anywhere in each C
  translation unit through the guest MakFS/VFS. Resolution is capped at four
  nested headers and eight unique dependencies, detects include cycles, and
  fingerprints the fully expanded source bytes. A bounded preprocessing pass
  additionally supports up to eight empty or signed-integer object macros and
  four nested `#ifdef`/`#ifndef` conditionals per source/header, with
  exact `#else`/`#endif` balancing,
  so the existing state-last `MAKSTATE2` cache now invalidates a dependent C
  object when its header changes while retaining unrelated objects. The
  deterministic Pi/QEMU 10.0.11 TCG gate seeds a separate two-input build
  graph whose C unit defines one function before including the same guarded
  root header twice. The root conditionally includes a guarded leaf that
  defines and uses an integer macro; inactive missing-header branches are not
  resolved and the final definition is emitted once. It proves cold `0/2`,
  warm `2/0`, leaf-header edit
  `1/1`, and rewarm `2/0`
  hit/miss results, then launches the resulting
  `/home/user/generated-header.elf` through the authenticated shell `run`
  command and reaps status 42. Missing, relative, cyclic, and over-depth
  includes and malformed define/conditional forms fail closed. The focused runtime, structural guard, AArch64 release/artifact
  check, full `make unit check`, unchanged Firefox-role production SMP,
  ordinary Native SMP, and cursor regressions pass. The production run reports
  dispatches `9906,10296,10964`, overlap TIDs 5/6 on AP1/AP3, watcher TID 8
  on AP3, input INTID 78, and status 42; Native reports
  `9636,9543,10685`, overlap TIDs 5/6 on AP1/AP3, and status 42. This is
  genuine bounded transitive dependency discovery and preprocessing. The
  subsequent expression increment adds bounded `#if`/`#elif`; this entry's
  earlier runtime remains evidence for the recursive-include foundation, not
  for a general preprocessor.
- 2026-08-26 the sandboxed AArch64 guest-native C compiler now accepts six
  typed parameters and six call arguments through AAPCS64 `x0`-`x5`. Values
  are retained in callee-saved `x23`-`x28`; four- through six-parameter
  functions use a 112-byte frame with explicit pair saves/restores while the
  existing one- through three-parameter layouts remain stable. A real
  `sum6`/`invoke6` translation unit emits 196 code bytes in a parsed 808-byte
  ELF64 `ET_REL`, carries one same-object `R_AARCH64_CALL26`, links with
  `invoke6` selected, transitions through writable/NX to RX, and executes both
  `sum6(10,5,6,7,8,6)` and `invoke6(37)` as 42. Seven parameters or arguments
  fail closed. Focused Pi/QEMU 10.0.11 TCG runtime, AArch64 release/artifact
  checks, full `make unit check`, unchanged Firefox-role and Native SMP
  regressions, and cursor runtime pass. This remains a bounded compiler seed.
- 2026-08-26 the sandboxed AArch64 guest-native C compiler now accepts up to
  six function definitions per translation unit instead of three, with a
  bounded eight-entry call-relocation table. A new source defines `stage1`
  through `stage6`; the compiler verifies six ordered non-overlapping
  definitions, emits five genuine same-object `R_AARCH64_CALL26` relocations,
  wraps them in a parsed ELF64 `ET_REL` with seven symbols, links `stage6` as
  the entry, enforces writable/NX then RX mappings, and executes
  `stage6(36)=42`. Seven definitions still fail closed. Focused Pi/QEMU
  10.0.11 TCG self-host runtime, structural guard, AArch64 release/image
  artifact validation, unchanged Firefox-role production SMP, ordinary Native
  SMP, and cursor regressions pass. This advances but does not complete the
  SDK/self-hosting rows: parameters remain capped at six, build graphs at
  six objects, linked code at 512 bytes, and no substantial in-guest MakOS
  build exists yet.
- 2026-08-26 AArch64 post-desktop production SMP is no longer restricted to
  Firefox-role workers. Non-leader ordinary `Native` application threads now
  share AP1-3 while leaders plus shell, UI, service, and all device MMIO work
  remain on CPU0. A separate exact-role upstream-musl pthread gate prevents
  Native evidence from satisfying Firefox markers. The Pi/QEMU 10.0.11 TCG
  Native run passed all three APs with `cpu_mask=0xe`, dispatches
  `11119,10254,11130`, simultaneous AP1/AP3 TIDs 5/6 (`overlap_mask=0xa`),
  kernel-owned affinity migration/restoration, exclusive ownership, AP
  block/wake/join, and status-42 reap. The unchanged Firefox regression also
  passed with dispatches `9857,11153,9945`, `overlap_mask=0xa`, exact surface
  watcher TID 8 on AP2, and status 42. Full `make unit check`, release/image
  artifact validation, combined network/input-IRQ runtime, and cursor runtime
  pass. This remains functional Pi/TCG evidence: automatic load balancing and
  additional built-in/service roles remain Partial, and real Firefox still
  needs the unchanged strict gate on an idle macOS/HVF host.
- 2026-08-26 AArch64 virtio-net receive now uses a genuine QEMU `virt`
  GICv2 interrupt in the normal path. Network slot 28 maps to SPI INTID 76,
  configured Group 1/edge-rising and targeted exclusively at CPU0. A lower-EL
  entry runs the bounded CPU0 network bottom half before EOI; an EL1 entry only
  acknowledges transport status to avoid recursively taking socket locks, and
  the unchanged 100 Hz owner pump remains recovery-only. The focused 4-PE
  Pi/QEMU 10.0.11 TCG gate sends a real AP1 UDP DNS request, blocks AP1 in
  receive, keeps CPU0 in a syscall-free EL0 counter loop, then proves INTID 76
  drained one RX frame and woke AP1 by SGI (`irq_frames=1`,
  `irq_el1_deferrals=0`, status 63, exact frame balance). The later real QMP
  Ctrl-K input phase and final boot also pass. The Firefox-role production gate
  continues to pass with three AP workers, a simultaneous AP1/AP3 interval,
  exact Ctrl-A watcher wake on AP1, and status 42; the cursor gate passes seven
  positions with zero changed scanout pixels. AArch64 release/artifact checks,
  structural guards, and full `make unit check` pass. This is functional
  Raspberry Pi/TCG evidence only; strict Firefox timings remain unchanged and
  still require the missing integrated image on idle macOS/HVF.
- 2026-08-26 AArch64 virtio keyboard/tablet input now uses genuine QEMU `virt`
  GICv2 interrupts in the normal path. Device slot `n` derives INTID `48+n`;
  discovered slots 29/30 configure shared Group 1, edge-rising SPIs 77/78 and
  target CPU0 exclusively. Lower-EL IRQ entry drains input and publishes
  exact-handle wakes before EOI. EL1 entry acknowledges the transport without
  recursively taking syscall-held locks, leaving the unchanged 100 Hz owner
  poll as a recovery drain. The focused upstream-musl Firefox-role Pi/TCG
  runtime proves QMP Ctrl-A on keyboard INTID 78, direct lower-EL dispatch,
  exact surface 7/TID 8 execution on AP1, one waiter woken/three skipped,
  all three AP workers (`9557,10824,9509` dispatches), simultaneous AP1/AP3
  TIDs 5/6, and status-42 reap. Structural guards, release artifact validation,
  and full `make unit check` pass. The audit remains Partial and unchanged
  strict Firefox timing still requires the missing integrated image on an idle
  macOS/HVF host.
- 2026-08-26 AArch64 surface-event blocking is now handle-specific instead of
  a global input thundering herd. Each blocked task records its surface handle;
  the wake path snapshots scheduler state, checks queue/owner state outside the
  scheduler lock, and wakes only a task whose exact owned surface has data.
  Destroyed or invalidated handles remain wakeable so teardown retries fail
  closed rather than stranding a join. Firefox key priority uses the same
  queued-handle selection. The upstream-musl production fixture now owns
  surfaces 7 and 8 and blocks target/decoy pthreads concurrently. QMP Ctrl-A
  selects handle 7/TID 8 on AP2, wakes exactly one surface waiter while three
  unrelated surface waiters remain blocked, dispatches the CPU0 leader, and
  leaves the decoy blocked until surface 8 is destroyed. Final Pi/QEMU 10.0.11
  TCG evidence reports `cpu_mask=0xe`, dispatches `9954,9924,11186`,
  `overlap_mask=0x6` with TIDs 5/6, `surface_woken=1`,
  `surface_skipped=3`, and status 42. Release artifact validation, focused
  production runtime, structural guard, and full `make unit check` pass. This
  is functional Pi/TCG evidence only; strict Firefox thresholds are unchanged
  and still require the missing package on idle macOS/HVF.
- 2026-08-26 AArch64 now exposes kernel-owned per-thread CPU affinity through
  target syscall 148 and ABI feature bit 22. Get/set are restricted to the
  caller's thread group, reject empty/offline masks, keep process leaders on
  CPU0, and force a scheduling boundary when a caller excludes its current
  PE. Official-musl patch 65 translates Linux AArch64
  `sched_getaffinity`/`sched_setaffinity`; CPython configuration now detects
  the real API. The upstream-musl Firefox-role fixture verifies leader mask
  `0x1`, each worker's singleton masks, at least three actual cross-PE
  migrations, restored AP-pool mask `0xe`, concurrent distinct-TID ownership,
  all joins, the AP input watcher, and status-42 reap. Final Raspberry
  Pi/QEMU 10.0.11 TCG evidence reports `cpu_mask=0xe`, dispatches
  `9867,11100,9833`, `overlap_mask=0x6` with TIDs 5/6, and watcher TID 8 on
  AP2. Full `make unit check`, both architecture checks, artifact validation,
  and the focused runtime pass. This remains Pi/TCG functional evidence; the
  scheduler row remains Partial and unchanged strict Firefox qualification is
  still pending on an idle macOS/HVF host.
- 2026-08-26 Firefox surface-key scheduling is now role-affine across the
  production AArch64 SMP policy. A blocked non-leader Firefox native-event
  watcher is selected only on AP1-3, while its bounded main-thread handoff is
  selected only on CPU0. A valid priority hint is retained when another PE
  owns the target, but is consumed after one successful dispatch; its
  1,000-tick deadline now only expires stale hints. This fixed a real
  starvation found by the focused fixture: retaining the CPU0 leader hint
  across fast yields prevented a newly forked Firefox child from reaching its
  typed-IPC service before the parent timed out. The production-only
  `FirefoxProbe` credential profile adds service publication for the existing
  full pthread/IPC fixture without broadening ordinary Firefox privileges.
  The compositor has two bounded owned overflow surfaces for fixtures while
  preserving the six stable launcher/taskbar slots and their geometry. The
  final Raspberry Pi/QEMU 10.0.11 TCG run reports `cpu_mask=0xe`, dispatches
  `9826,11253,9695`, `overlap_mask=0x6` with TIDs 5/6, watcher TID 8 on AP2,
  one CPU0 leader dispatch, and final status 42 after the full upstream-musl
  pthread workload. Structural scheduler/priority guards, artifact checks,
  and full `make unit check` pass. This is Pi/TCG functional evidence only:
  unchanged strict Firefox Gate 3 still requires an idle macOS/HVF rerun, and
  the scheduler/graphics audit rows remain Partial.
- 2026-08-25 AArch64 opened a bounded production scheduler gate after
  driver and login-UI initialization. At that milestone process leaders,
  shell/UI tasks, and every non-Firefox role remained on CPU0; non-leader
  Firefox-role threads were eligible on the shared AP1-3 Ready queue. All device MMIO remains
  CPU0-owned, Ready publication sends a scheduler SGI, and an AP with no local
  successor publishes Blocked/unowned state and returns to its WFI dispatcher
  for sleep, I/O, input, IPC, and futex waits. Firefox's TaskController request
  is restored from one to three workers. The focused `firefox-smp` command runs
  the upstream musl pthread workload under the exact production Firefox role.
  A production-only three-worker rendezvous now keeps distinct pthreads Ready
  long enough to prove a simultaneous AP execution interval rather than merely
  accumulating per-CPU dispatch counters. The final Pi/QEMU 10.0.11 TCG pass
  reports `cpu_mask=0xe`, `overlap_mask=0xa`, distinct live TIDs 6/5 on AP1/AP3,
  exclusive ownership, final status 42, and dispatch counts
  `9787,11024,9493`. This fixture exercises
  clone/futex/pipe/signal/block/wake/join/exit/wait/reap but is explicitly not
  real Firefox or macOS/HVF performance evidence. A pre-fix repeat exposed an
  IRQ window inside the EL1-to-EL0 restore trampoline at `0x48000164`; EL1 IRQs
  now stay masked until the target SPSR is installed atomically by `ERET`.
  Subsequent load gates complete all 297 selections and the focused production
  runtime passes. The strict real-Firefox Make target now sets
  `MAKOS_AARCH64_FIREFOX_SMP_REQUIRED=1` and rejects a run unless its launched
  Firefox group publishes at least two simultaneous AP owners with distinct,
  nonzero TIDs; all paint, Ctrl-A, input, TLS, URI, interaction, resource, and
  survival thresholds remain unchanged. Qualification also exposed byte-level
  PL011 record interleaving. AArch64 formatted/raw serial writes now hold an
  IRQ-masked cross-PE lock, and the repeated overlap run emits intact live and
  final evidence lines. Release image/artifact validation and final
  `make unit check` pass. The broad Pi/TCG harness reaches later desktop/libc/application gates
  but retains the pre-existing Settings resize mismatch (`560x360` observed,
  exact `450x290` required), so it is not recorded as a full broad-gate pass.
  Real Firefox overlap plus the unchanged idle-macOS/HVF Gate 3 remain required,
  and the scheduler audit row stays Partial.
- 2026-08-25 AArch64 has its first genuine four-PE EL0 scheduling proof. After
  PSCI/GIC bring-up, the kernel publishes a bounded boot-probe gate and sends a
  GICv2 SGI to three WFI APs. Each AP enables its banked virtual-timer PPI; four
  independent ELF processes rendezvous immediately before EL0 entry and then
  overlap across CPU0-3 with distinct TIDs and exit statuses 40-43. The final
  Pi/QEMU 10.0.11 TCG run reports TIDs `1,2,3,4`, `overlap_mask=0xf`, reaps all
  roots, restores exact free-frame balance, closes the AP gate, and reaches the
  visible login. A subsequent probe has every AP block in `sleep_until`, return
  through its per-CPU kernel record to the idle dispatcher, receive CPU0's
  timer wake, and resume in EL0 with `resume_mask=0xe`. The same fixed-affinity
  processes then execute timed futex waits: CPU0 idles in the syscall, all APs
  return to idle, and the 20 ms timeout resumes them with
  `futex_idle_mask=0xe`/`futex_resume_mask=0xe`. A zero-descriptor 200 ms `poll`
  additionally proves the retry-PC I/O path with
  `io_idle_mask=0xe`/`io_resume_mask=0xe`. A separate embedded EL0 program now
  creates a process-owned auto-reset event and clones a shared-VM thread. The
  leader blocks on AP1 with no eligible successor; CPU0 runs the child on its
  validated RW/NX stack, signals the event, and returns through thread-only
  exit with no local successor. AP1 resumes and the parent exits 44. Runtime
  reports `ipc_idle_mask=0x2`/`ipc_resume_mask=0x2` with exact frame balance.
  A third EL0 fixture places a cloned busy worker on AP1 and has its CPU0 leader
  invoke syscall-119 group exit. The scheduler publishes a dying-group
  exclusion, sends a GICv2 SGI, detaches the worker on AP1, switches AP1 off
  the shared TTBR0, and receives acknowledgement mask `0x2` before worker reap;
  status 55, single-root cleanup, frame balance, and subsequent login pass.
  A fourth fixture holds the AP1 worker inside its real yield SVC after a
  release-ordered EL1-entry publication. CPU0 exits the group with status 56;
  AP1 detaches at the safe outer-exception return, changes to the kernel root,
  and acknowledges only after the barrier. Runtime reports matching
  `entered_el1_mask=0x2`/`deferred_ack_mask=0x2`, target/ack masks `0x2`, exact
  frame balance, and subsequent login. Scheduler-entry races before target-mask
  publication are folded into the same locked stop/ack contract.
  A fifth fixture rendezvous-holds two independent processes inside syscall 119
  on CPU0/AP1, then proves both acquire a separate max-one teardown coordinator
  and exit with statuses 57/58 (`rendezvous_mask=0x3`,
  `serialized_acquire_mask=0x3`) without scheduler-lock deadlock. This test
  exposed the former 64 KiB AP stack overwriting the adjacent kernel-root word;
  every AP now has a runtime-reported 1 MiB EL1 stack, and the full fixture plus
  visible login pass with exact frame balance.
  A sixth fixture concurrently enters group exit from a shared-root leader on
  CPU0 and worker on AP1 with distinct statuses 59/60. One caller owns teardown;
  the other transitions itself to Zombie, switches to the kernel root, joins
  the acknowledgement contract, and does not duplicate cleanup. Runtime proves
  complementary owner/join masks, first-owner-wins status, one shared-root
  reap, exact frame balance, and subsequent visible login.
  An opt-in seventh fixture starts after real virtio-input initialization. AP1
  blocks in EL0 `read_key`, returns to its idle dispatcher with no eligible
  local successor, then resumes only after the focused harness sends QEMU
  Ctrl-K and CPU0 drains the virtio ring and sends an SGI. Virtio-input MMIO
  and deferred compositor input work now have an exclusive CPU0 service
  wrapper; AP syscall/TTY paths record a deferral, and the low-level driver
  fails closed on any non-owner call. Two repeated Pi/TCG runs report nonzero
  CPU0 activity/AP deferrals and require
  `input_idle_mask=0x2`/`input_resume_mask=0x2`, status 61, exact frame balance,
  and subsequent boot completion. The ordinary image never arms the
  external-input wait.
  Ordinary AArch64 keyboard/tablet delivery is no longer timer-polled in the
  normal case. QEMU `virt` slot `n` maps to GICv2 INTID `48+n`; MakOS programs
  discovered input INTIDs 77/78 as Group 1 edge-rising SPIs targeted only at
  CPU0. A lower-EL IRQ drains both used rings and publishes exact-handle wakes
  before EOI. If the IRQ interrupted EL1, it acknowledges transport status
  without recursively entering graphics/scheduler locks; the retained 100 Hz
  owner poll drains that queue on the next safe tick. The Firefox-role focused
  Pi/TCG run proves QMP Ctrl-A arrived directly on keyboard INTID 78, woke only
  surface 7/TID 8 on AP1, skipped three unrelated waiters, and reaped status
  42. This is functional Pi evidence, not macOS/HVF latency qualification.
  The same opt-in image first runs a real virtio-net UDP/DNS fixture. AP1 copies
  its UDPv4 request into a bounded eight-slot service queue; CPU0 alone mutates
  the transmit ring and completes the request. AP1 then blocks in receive and
  returns to idle; CPU0 exclusively drains/demultiplexes the RX ring and sends
  the wake SGI. Two pre-hardening passes and one post-hardening repeat pass on
  Pi/TCG validate one AP TX request/CPU0 completion, the DNS response, nonzero
  CPU0 RX frames/AP deferrals, I/O idle/resume masks `0x2`, status 63, and exact
  frame balance. One intervening run completed the entire network fixture but
  later missed the independently injected Ctrl-K and hit the unchanged input
  completion timeout; the immediate unchanged repeat passed. Low-level TX now
  fails closed off CPU0 and copied UDPv4/v6 is qualified. The AP UDP completion
  wait is a bounded EL1 `WFE` loop, not a scheduler-idle proof.
  A separate opt-in TCPv4 fixture now has AP1 create a real socket, connect to
  QEMU slirp host `10.0.2.2:18080`, send exact `MAKOS_AP_TCP_TX\n`, block in
  receive, verify exact `MAKOS_CPU0_TCP_RX\n`, and close with FIN. Connect and
  segment requests copy immutable state through the same bounded service queue;
  CPU0 alone resolves the route and mutates the virtio-net rings, while socket
  state is published under the socket-table lock. A delayed host reply proves
  AP1 returns to its idle dispatcher and is resumed after CPU0 drains RX and
  sends the wake SGI. Pi/QEMU 10.0.11 TCG passes status 69/70, exact request and
  response, one owner RX frame/AP deferral, four owner completions/four AP
  requests (connect, data, ACK, FIN), I/O idle/resume masks `0x2`, and exact
  frame balance. TCPv6 retains structural service support but has no equivalent
  guest runtime proof yet.
  A ninth fixture creates a private mode-0600 uid/gid-1000 inode and gives its
  immutable AP1 probe the minimum file-write capability before login. Through
  normal VFS/MakFS4 calls, AP1 writes and `fsync`s 4 KiB, closes, reopens, reads
  and byte-verifies 4 KiB; the kernel then removes the fixture. An eight-slot
  copied-request queue lets CPU0 exclusively submit virtio-blk operations;
  low-level submission fails closed off CPU0. CPU0's production 100 Hz timer
  bottom half services the queue and defers one tick if it interrupted direct
  CPU0 I/O with the owner lock held. The passing combined Pi/TCG gate reports
  33 requests/completions: 18 reads, 10 writes, 5 flushes, and all 33 timer
  serviced, with status 65 and exact frame balance. Two initial runs with an
  unnecessary fixed five-guest-second CPU0 evidence window exhausted the
  unchanged 90-second Pi/TCG harness window: the first after the block marker,
  the repeat during the block phase. Reducing that evidence window to one guest
  second retained 100 timer opportunities and every counter requirement, then
  passed block, network, input, and boot. The AP request wait is bounded EL1
  `WFE`, not scheduler-idle proof.
  An additional immutable AP1 fixture receives only `CAP_GRAPHICS` and uses
  ordinary syscalls 8/9/10 to create, fill, and present a 96x64 retained
  surface. Any composition requested off CPU0 now publishes one coalesced
  deferred action rather than touching virtio-GPU state. CPU0's production
  timer bottom half consumes it, and every low-level control/cursor queue
  submission fails closed off-owner. The first Pi/QEMU 10.0.11 TCG pass reports
  one AP deferral, one CPU0 deferred composition, two real control-queue
  completions (one transfer and one resource flush), status 67, surface reap,
  exact frame balance, and all later block/network/input/boot gates passing.
  After the marker was tightened to report the AP syscall as
  `scanout=0 deferred=1`, two unchanged 90-second Pi/TCG repeats again passed
  the complete GPU proof but exhausted the combined harness window before the
  later input-ready phase. Neither reported a guest fatal/panic; thresholds
  were not changed. The earlier same-behavior combined run remains the full
  block/network/input/boot regression evidence.
  A further bounded scheduler fixture now forces one immutable EL0 TID from
  AP1 to AP2. Under the process lock it captures the live context, changes the
  task from Running to Ready/unowned, publishes the new affinity, then kicks
  idle PEs; AP2 alone may select it. The first runtime exposed only a target-
  evidence ordering race: the same TID ran on both PEs and exited 71, but AP2
  resumed before a later counter publication. Publishing the intended target
  under the scheduler lock fixes that observer race. The unchanged repeat on
  Pi/QEMU 10.0.11 TCG passes exact source/target masks `0x2`/`0x4`, one
  migration, exclusive ownership, GPR/SP/TLS/SIMD preservation, status 71 and
  frame balance. This qualifies forced migration.
  A following shared-Ready-queue fixture runs six immutable tasks across AP1-3.
  Each task performs 48 real yields; selection checks exactly one CPU owner and
  records per-task/per-CPU evidence. The unchanged focused Pi/QEMU 10.0.11 TCG
  gate passes statuses 80-85, worker mask `0xe`, exact frame balance, and even
  dispatch counters `99,99,99` (297 total) under 288 yield contention points.
  Longer absolute sleeps exposed and fixed a real wake path: session liveness
  now retains Ready/Blocked tasks, Ready publication sends the scheduler SGI,
  AP idle acknowledges IRQs around `WFI`, and CPU0 keeps its sole global timer
  armed while waiting for AP sleep deadlines.
  The current AArch64 release image/artifact check, full `make check` and
  `make unit`, and both SMP structural guards pass. The later desktop gate now
  admits non-leader Firefox and ordinary Native application threads to AP1-3;
  leaders plus shell, UI, service, and device MMIO work remain on CPU0.
  Separate exact-role/group pthread fixtures record simultaneous distinct TIDs
  without cross-crediting their evidence. Automatic load balancing, additional
  built-in/service roles, and the same overlap/contention from the genuine
  Firefox process are still open, so the scheduler row stays Partial.
- 2026-08-26 the AArch64 guest-native toolchain now crosses a genuine variable
  build-graph boundary within an explicit two-to-six-input limit. Its primary
  fixture writes and rereads an assembly startup, a C translation unit
  containing `answer` and `adjust`, a second C unit defining `combine`, and an
  independent C unit defining `helper` through MakFS. A parsed `MAKBUILD1`
  manifest supplies all four source/object paths, the final output path, and
  `_start` entry symbol; these fields drive source reads, object persistence,
  linking, and the final write. It accepts one leading `asm` input plus one
  through five `c` inputs, absolute non-colliding paths, and one terminal link
  record. Bad version, relative path, collision, missing-link, wrong-order, and
  seven-input manifests fail closed. The
  authenticated `makbuild <manifest>` terminal command now validates a
  `/home/user/` path in the kernel, copies it into the EL0 toolchain's real SysV
  `argv[1]`, and uses `MODE=build` to consume existing MakFS inputs without
  seeding or overwriting them. `selfhost-aarch64` explicitly selects the
  separate deterministic `MODE=fixture` path. It also seeds a separate
  three-input manifest. Focused Pi/TCG runtime executes fixture mode once and
  build mode twelve times across the four-, three-, and two-input graphs, with every
  toolchain process reaped at status 42. Build mode derives a versioned
  120-byte `MAKSTATE2`
  record and commits it only after object writes and an always-relinked final
  ELF. Its 64-bit FNV-1a manifest/source/object fingerprints are
  non-cryptographic cache keys, not a security boundary; a hit also requires
  the persisted object to parse and pass symbol validation. The focused run
  proves four-input cold `0/4`, warm `4/0`, corrupt-object `3/1`, rewarm `4/0`,
  edited-source `3/1`, rewarm `4/0`, and corrupt-state full `0/4` hit/miss
  results, followed by three-input cold `0/3` and warm `3/0`, then quoted-header
  cold `0/2`, warm `2/0`, edited-header selective `1/1`, and rewarm `2/0`.
  The recursive resolver reads absolute quoted headers through MakFS, hashes
  fully expanded source bytes for cache identity, records up to eight unique
  dependencies across four nested headers, and rejects missing, relative,
  cyclic, or over-depth includes. It also expands up to eight empty or
  signed-integer object macros and processes four nested defined/undefined
  conditional levels per source/header; malformed or unbalanced directives
  fail closed. Stale, missing,
  old-version, or malformed state safely forces a full rebuild.
  Each bounded C
  translation unit accepts up to six
  AAPCS64 `int` functions, each with up to six typed parameters and up to four
  register locals, unsigned 16-bit constants, parentheses, unary `+`/`-`,
  precedence-correct `*`/signed `/`/`%`/`+`/`-`, mutable parameter/local
  assignments, signed
  `==`/`!=`/`<`/`<=`/`>`/`>=` comparisons, a conditional `if`, a bounded
  assignment-only `while`, and a
  one- through six-argument call within or across objects. Parameters may independently
  be `int` or `int *`; the compiler also accepts `int *pointer = &local`, address expressions passed across
  the call boundary, dereference loads inside expressions, and
  `*pointer = expression` stores. Pointer locals and call arguments now also
  accept `pointer-or-array + constant-or-scalar`. Constants are restricted to
  0..3 elements and use a scaled 64-bit address `ADD`. Scalar `int` parameters
  or non-address-taken scalar locals use `ADD ... SXTW #2`, preserving negative
  32-bit C offsets, while pointee loads/stores remain 32-bit; parenthesized
  `*(pointer + offset)` works on either side of an assignment. Known
  local-array bounds reject one-past-end constants and all variable offsets
  whose range is unproved. Fixed local `int` arrays have one to four
  exactly initialized elements within the four-slot frame budget. Constant
  indexing emits bounded 32-bit loads/stores; known local-array bounds are
  checked at compile time, and a bare array argument decays to its preserved
  64-bit stack address in AAPCS64 `x0`.
  Address-taken `int` locals occupy bounded 32-bit stack slots and are reloaded
  from memory, while pointer locals use preserved 64-bit registers. Forward
  conditional and signed backward unconditional branches are range-checked
  before patching. Emitted non-leaf functions preserve FP/LR and x19-x24 in a
  96-byte frame. Up to six arguments use AAPCS64 `x0` through `x5` (`w0`
  through `w5` for integers) and are preserved in x23 through x28. The `x25`
  stack slot is emitted only for a three-parameter function; four through six
  parameters select a 112-byte frame with paired x25-x28 preservation,
  preserving the exact existing one- and two-parameter code sizes. The current linked call invokes
  `adjust(values + 1, 1)`; `adjust(int *pointer, int delta)` derives
  `next = pointer + delta`, computes the signed element count
  `distance = next - pointer` through 64-bit `SUB`/arithmetic shift-right two,
  updates element zero, loops while `count < distance`,
  and stores through `*(pointer + delta)`. The compiler concatenates a 140-byte
  `answer` and 168-byte `adjust` into one 308-byte `.text` with two definitions
  plus undefined `combine` in a 976-byte ELF64 `ET_REL`; the separate 60-byte
  `combine` definition occupies a 616-byte library object. The independent
  56-byte `helper` definition occupies a 608-byte object and direct RX
  execution proves `helper(40)=42`. The assembler produces 76 bytes in a
  688-byte object. All four persist and reopen. The bounded general linker concatenates two
  through six objects, discovers global definitions/undefined symbols, resolves
  relocations against either same-object definitions or external symbols,
  applies validated `R_AARCH64_CALL26` relocations
  (`_start`→`answer` and `adjust`→`combine` externally, with
  `answer`→`adjust` internally), includes `helper`, and emits 500 code
  bytes in an 815-byte two-`PT_LOAD` `ET_EXEC`. It rejects an out-of-range BL
  site, relocation type 282, a nonzero CALL26 addend, an unresolved `adjust`, a
  missing `combine` object, and duplicate `answer` definitions. A separate
  two-function source compiles `sum3(int,int,int)` and `invoke3(int)` into 140
  code bytes and a 752-byte ELF64 `ET_REL`; the linker resolves its same-object
  `CALL26`, selects entry offset 80, and RX execution requires both calls to
  return 42. A separate `sum6`/`invoke6` unit emits 196 code bytes in an
  808-byte parsed object, resolves its internal call with entry offset 124,
  and executes both paths as 42. Duplicate parameter names, more than six parameters, more than
  six call arguments, unsupported bitwise syntax, direct literal-zero
  division/remainder,
  and a non-total conditional function, loop without a terminal return,
  assignment to an undefined variable, address-of an undefined local, and
  untyped pointer reassignment, returning a pointer/address as an `int`, indexing a
  known two-element array at index two, deriving `values + 2` or a variable
  offset from that known array, pointer-minus-scalar, duplicate functions and
  a seventh function in one
  translation unit also fails closed. RX execution of the
  fully linked C graph proves `answer(20)=42`, `answer(0)=86`,
  `adjust(forty,1)=42`, `adjust(scaled,2)=44`, and `adjust(zero,1)=2`; the latter
  three also prove the arrays change to `41:42:0`, `42:0:44`, and `1:2:0`.
  Separate RX probes exercise all four signed ordering relations and prove a
  `pointer + -1` load returns 42 plus pointer differences of `3` and `-3`.
  A separate three-definition unit emits 168 code bytes in a parsed 784-byte
  ELF64 `ET_REL`; 32-bit `SDIV`, quotient-based `MSUB` remainder, and unary
  `SUB` execute positive/negative results `6`/`-6`, `2`/`-2`, and `-42`/`42`
  from RX memory.
  Same-array provenance remains a caller obligation. The linked `answer`→`adjust` call passes the
  stack-backed `values[3] + 1` address; that internal call plus the external
  `adjust`→`combine` call require real relocations, scaled pointer addition, and
  callee loads/stores into the final two elements of caller-owned memory. Focused
  Pi/QEMU 10.0.11 TCG then
  executes/reaps the final ELF twice with status 42 through syscalls 56/57.
  The library function is executed directly as `combine(40,2)=42`; the linked
  mutation paths also require the external C-to-C call. Release artifact validation,
  focused runtime, structural guard, full `make unit check`, unchanged
  Firefox-role and Native SMP regressions, cursor runtime, and a fresh visible
  Pi/TCG login pass. That preprocessing guest (PID 766987) was stopped cleanly
  through QMP before the repository-source runtime. The repository-source
  milestone was PID 775104 under the user service
  `makos-visible-selfhost-repository-final.service`, with
  private boot/data/variables and QMP in
  `build/makos-pi-visible-selfhost-repository-final-JJyWajUO`; its boot clone
  exactly matches the current release image SHA-256
  `3b03a276d5c6f6ab1c0654860088f1054087c82b9b5698a3b7fb9d2aa9d9e4b6`;
  it was later stopped cleanly through QMP.
  QMP capture `login.ppm` has SHA-256
  `53179ecad66d43194bfc58a93a3f8bbb3d1d11bda432e1110c385f5cd59d8382`.
  This is a real but deliberately bounded seed, not a
  general function-like/expression/system-header preprocessor or unbounded dependency
  engine, general C/Rust compiler/linker,
  arbitrary graph beyond six inputs, parallel build system, debugger, or substantial
  in-guest MakOS build, so self-hosting remains Partial.
- 2026-08-25 AArch64 syscall 57 has parity with the versioned normative
  startup-vector ABI. The kernel requires the exact 336-byte version-1
  descriptor, copies and validates up to eight arguments, eight environment
  entries and 256 string bytes, rejects malformed/unused offsets and invalid
  strings, and builds child-owned SysV stack vectors before scheduling. The
  guest-native two-pass assembler supports labels, compare, conditional
  branches, 64-bit loads and byte loads in addition to move/SVC/return. Its
  linked ELF inspects startup registers itself: Pi/QEMU 10.0.11 TCG
  passes syscall 56 with `argc=1`, syscall 57 with `argc=3`/`envc=1`, three
  malformed-descriptor denials, two status-42 wait/reaps, and truthful ABI bit
  19. `make unit`, `make check`, release image/artifact validation, and focused
  structural guards pass. The broad Pi/TCG boot continued through later musl
  and MicroPython gates, then stopped at the unchanged Settings resize mismatch
  (`560x360` observed versus required `450x290`); it is not recorded as a full
  broad-gate pass.
- 2026-08-25 repository imported and pushed to
  `https://github.com/Ninnja10563/MakOS.git` on `main` at commit `346b0df`.
  Source, docs, scripts, ports, SDK, tests, and manifests are tracked. Generated
  build/target/output images, logs, QEMU variable stores, and caches remain
  intentionally ignored.
- 2026-08-25 queued typed native IPC passes 12 core unit tests, structural ABI
  guard, full unit/check suite, and isolated full AArch64 HVF runtime. Versioned
  messages are FIFO and kernel-stamp sender PID/UID; channel-handle transfer is
  generation-safe, rights-attenuated, lifetime-retained, and cycle-collected.
  Capability-gated service routes enforce same UID/session. Provider/process exit
  unregisters services and closes handles before reap. Runtime marker:
  `MAKOS_AARCH64_TYPED_IPC_RUNTIME_OK service=same-domain fifo=1 transfer=attenuated cleanup=process-exit-before-reap`.
- 2026-08-25 fresh cursor rerun passes seven HVF QMP positions with zero scanout
  changes, virtio-GPU cursor plane, and hidden host cursor. Two latest unchanged
  Firefox Gate 3 reruns painted real browser chrome in 248,584/255,543 ms but
  failed strict Ctrl-A latency at 10,971/14,363 ms against 10,000 ms. Host was
  unsuitable for performance qualification: load 7.66, 163 MiB free, 6.6 GiB
  compressed, multiple browser/WindowServer cores busy. Thresholds remain
  unchanged; rerun on an idle host before calling latest Gate 3 green.
  An idle Raspberry Pi retry of the unchanged Make target stopped at preflight
  before QEMU launch because the exact integrated image
  `build/makos-integrated-a9c604254f094de2.img` and staged Firefox package are
  not present on this development host. No release asset supplies that artifact.
  This is neither a Firefox runtime result nor macOS/HVF performance evidence.
- 2026-08-25 AArch64 installer now shares the fail-closed installer core with
  x86 and supports exact `install disk1 resume-disk1`. Full HVF gate SIGKILLs
  QEMU after first verified payload progress. Installer serializes a source
  flush with a disk0 write freeze and keeps successful source frozen until
  shutdown; disk1 writes remain active. Gate proves LBA0 blank and two nonzero
  partial blocks source-identical, guest-tests conflict/committed-MBR refusal,
  resumes to complete source/target SHA-256 equality, detaches live source, and
  passes two installed-only persistence boots.
- 2026-08-25 current `make unit && make check` and focused cursor runtime pass.
  Latest AArch64 artifact check passes. Verified integrated package is
  `a9c604254f094de2` (SHA-256
  `a9c604254f094de24ed2668da74cbcd48f48ae0f111e8b182a7b3dedfeda2824`).
  Focused cursor runtime moves through seven positions with zero changed
  scanout pixels, virtio-GPU cursor plane, and hidden host cursor. Prior visible
  HVF PID 19919 has exited; no visible QEMU remained at handoff.
- 2026-08-25 sustained Firefox Gate 3 passes exact Make-target defaults on
  package `a9c604254f094de2`. Patch `0056` gives MakOS wheel input valid line
  delta mode; down/up dispatch changes 65,599 document pixels then recovers to
  13,270 differing pixels without crash. Firefox types `makos42` in httpbin's
  Customer field, pointer-selects/copies it, builds exact
  `https://example.com/?customer=makos42`, and completes two cycles/four
  repeated top-level example/IANA pages. Current pass paints in 169,697 ms,
  handles Ctrl-A in 5,727 ms, accepts first character in 111 ms, bounds host
  CPU ratio at 1.053 and RSS at 325,140,480 bytes, reports 54,254 guest Firefox
  resident pages, then survives 531 seconds.
- 2026-08-25 AArch64 task reports include mapped user resident pages/KiB from
  actual L3 descriptors. `test-aarch64-firefox-runtime` now enables sustained
  interactions with two cycles by default. Full unit/check, package/binary,
  cursor, strict interaction, and resource gates pass.
- 2026-08-25 strict Firefox document-selection gate passes on package
  `1b55d512904b8c2a`. Real held-button pointer dispatch selects rendered IANA
  text; changed selection pixels are required. Ctrl-C writes MakOS clipboard,
  raw key 136 maps Ctrl-L, and Ctrl-V composes exact `https://example.com`
  before built-in-root TLS/HTTP 200 reload. Current pass paints in 164,388 ms,
  handles Ctrl-A in 9,373 ms, accepts first character in 110 ms, completes link
  navigation in 25,496 ms and selection/reload in 34,400 ms, then survives 329 s.
- 2026-08-25 Firefox packaging always refreshes Mozilla `stage-package` before
  reading `dist/firefox`; incremental Gecko builds cannot silently package an
  older staged runtime. Make and package-runtime defaults now use `a9c604...`.
- 2026-08-25 strict Firefox interaction gate now proves real document mouse
  routing. After clipboard reload, QMP injects a left click over the rendered
  `example.com` link; MakOS reports Firefox surface hit-testing, then Firefox
  reaches exact `https://www.iana.org/help/example-domains` with built-in-root
  TLS and changed pixels. Current pass paints in 169,164 ms, handles Ctrl-A in
  7,435 ms, accepts first character in 110 ms, completes clicked navigation in
  36,993 ms, and survives 292 seconds.
- 2026-08-25 post-interaction `make unit && make check` passes. Obsolete
  integrated package `5a00ea782577c499`, stale clipboard-only serial evidence,
  Python bytecode caches, and temporary screenshot conversion were removed;
  active package/runtime fixtures and latest audit evidence were retained.
- 2026-08-25 two-boot security-audit content proof now validates loaded records
  before early-boot merge. Current HVF run finds two prior-boot severity-4
  records: authentication accepted and account created, both with nonzero PID
  attribution. This closes count-only ambiguity while preserving bounded
  32-record CRC/COW journal semantics.
- 2026-08-25 fixed AArch64 IPv6 runtime gate ordering: AF_INET6 and UDPv6
  markers originate in post-login musl probe, not pre-login boot. Focused full
  guest run now proves validated RA/SLAAC EUI-64 configuration, 28-byte native
  sockaddr_in6, NDP resolution, and checksum-valid UDPv6 transmit. Reproducible
  target: `test-aarch64-ipv6-runtime`; receive/TCPv6 runtime remains open.
- 2026-08-25 sparse anonymous VM decommit now has current guest execution
  proof for both supported release operations. Fresh 1,350-object upstream-musl
  build writes anonymous pages, calls `MADV_DONTNEED` and immediate-decommit
  `MADV_FREE`, verifies zero refault after each, unmaps, and completes full
  AArch64 runtime. Structural guard tracks frame-unmap/free semantics.
- 2026-08-25 strict Firefox interaction gate now exercises real system
  clipboard integration after HTTPS completion: select URL, copy, clear field,
  paste, reload, and require a second exact-URI page-stop marker. Current
  `example.com` run passes changed pixels and built-in-root TLS, paints in
  177,985 ms, handles Ctrl-A in 7,201 ms, accepts first character in 109 ms,
  and survives 258 seconds. `test-aarch64-firefox-runtime` includes this proof.
- 2026-08-25 AArch64 TCP receive flow control now tracks each connection's
  advertised window against its pooled 32 KiB RX capacity. Accepted packets
  advertise remaining bytes, saturated buffers emit duplicate zero-window
  ACKs without sequence advance, and userspace drains emit reopening ACKs.
  IPv4/IPv6 packet tests verify encoded windows and checksums; structural gate,
  release kernel, full AArch64 boot, and strict Wikipedia runtime pass. Latest
  pass paints in 178,412 ms, handles Ctrl-A in 7,479 ms, accepts first character
  in 101 ms, verifies exact URI/built-in-root TLS/changed pixels, and survives
  271 seconds. Broader IPv6 guest runtime and modern-site coverage remain open.
- 2026-08-25 broader Firefox runtime exposed TCP burst loss: sockets advertised
  32 KiB while storing 4 KiB inline, and the timer pump could consume 16 frames
  before userspace drained them. RX storage now uses a bounded external pool of
  128 x 32 KiB, keeping copied socket records off 16 KiB kernel stacks. Full
  two-boot HVF passes. Strict `https://www.wikipedia.org/` now paints chrome in
  169,084 ms, handles Ctrl-A in 6,138 ms, accepts the first character in 110 ms,
  and passes exact URI, built-in-root TLS, changed pixels, and 270 s survival.
  Broader sites remain open.
- 2026-08-25 x86 install recovery gate now uses a prepared live-media base and
  per-boot qcow2 overlays. It SIGKILLs QEMU after the first verified 4 KiB
  payload block, proves LBA0 remains zero and every nonzero partial block equals
  source, boots again with `resume-disk1`, verifies final target SHA-256, then
  detaches source and passes two installed-only persistence boots. Installer
  emits an initial durable progress event plus 32 MiB periodic checkpoints;
  `make release` now includes `test-x86_64-install`.
- 2026-08-25 `test-makfs4-guest-fsck` runs the full AArch64 two-boot workload,
  waits for QEMU exit, exports its real MakFS4 data volume, then runs read-only
  fsck before temporary cleanup. Runtime passes generation 257/root slot 1,
  5 inodes, 4 files, 1 directory, and 4 allocated blocks. Release uses this
  gate instead of duplicating the plain AArch64 test.
- 2026-08-24 current runtime gates: focused AArch64 cursor gate passes seven
  QMP moves with zero scanout pixel changes; five-boot durable package gate
  passes signed install/replace/remove/rollback, live VFS reads, reboot
  persistence, open-FD generation pinning, and corrupt-newest fallback. Latest
  full first boot passes musl extended stat/symlink/timestamps, exact-TID
  signals, timed/robust futex, SCM_RIGHTS, shmem, and MakFS4 64-sibling/name255
  create phase. Browser sandbox structured-log read denial passes with untouched
  output buffers. Full network-isolated second boot now passes structured-log
  merge plus MakFS4 64-sibling/name255 remount verification and cleanup;
  Firefox Gate 3 separately proves external DNS/TCP/TLS/HTTP.
- Historical Firefox strict Gate 3 passed three consecutive clean runs on integrated package
  `makos-integrated-4dcdfcc16c362584.img`. Fresh full rebuild passes
  shared-musl/NSS/Gecko binary audit. Two earlier clean strict probes reproduced
  the no-paint branch after surface/XRE
  event-loop startup. Missing `FUTEX_REQUEUE` stranded musl private-condvar
  relay waiters; AArch64 now supports FIFO wake/requeue and compare-requeue
  while preserving handles/deadlines. Three-waiter `pthread_cond_broadcast`
  relay and full two-boot HVF pass. Full Firefox rebuild passes binary audit;
  clean package passes exact SHA-256 manifest, CRC, and ELF verification.
  Diagnostic task snapshots exposed another cause: blocking erased ProcessTable's
  round-robin origin, so low-slot futex churn starved later Ready workers. Per-CPU
  persistent cursors now preserve fairness across block/wake. Registered Firefox
  watcher identity also retains key priority while processing an earlier event,
  before explicit leader handoff. Final strict runs paint real `browser.xhtml` in
  181,859/167,216/169,122 ms; Ctrl-A takes 6,417/7,587/7,315 ms and first URL
  character 110/110/110 ms. Each exact `https://example.com/` completes
  built-in-root TLS, HTTP 200, changed pixels, and 213/229/232-second guest survival.
  Supported
  `MOZ_TASKCONTROLLER_THREADCOUNT=1`, MakOS IndexedDB limit 2, and stream-pool
  limit 4/idle 1 reduce BSP-only contention. Watcher dequeue immediately arms
  the Firefox leader, while a later futex wake refreshes the bounded 1,000-tick
  window. Runtime emits `MAKOS_AARCH64_SURFACE_MAIN_HANDOFF_OK` with
  `source=watcher-dequeue-fallback`. Earlier pre-fix probes stalled without
  paint beyond 240 and 348 seconds. Diagnosed cold-start gate now reproduces;
  broader site requirements remain incomplete.
- QEMU 11.0.3 usernet PCAP proves native UDP6 TX has valid checksum and NDP,
  then slirp returns ICMPv6 no-route for documented DNS endpoint `fec0::3`.
  Runtime marker now states UDP6 RX is backend-dependent; offline RX/demux tests
  pass, guest UDP6/TCP6 receive proof remains pending.

- AArch64 per-task monotonic sleep deadlines save EL0 context, block the task,
  and wake from generic-timer IRQ without userspace busy-spinning. Syscalls 102
  and 103 expose clock resolution and absolute sleep; musl `clock_getres`,
  `nanosleep`, relative realtime/monotonic `clock_nanosleep`, and absolute
  monotonic `clock_nanosleep` pass the two-boot HVF runtime probe.
- AArch64 pipes support blocking and nonblocking operation, 512-byte atomic
  `PIPE_BUF` writes, CLOEXEC, EOF/HUP/broken-pipe transitions, and timed or
  infinite poll. Kernel saves and retries blocked syscalls; pipe state/timer IRQ
  wakes tasks without busy-spinning. Upstream musl pthread runtime covers read,
  write, readiness wake, timeout, and close paths under HVF.
- AArch64 process-owned epoll supports ADD/DEL/MOD, level/edge/one-shot modes,
  finite/infinite scheduler-blocked waits, and automatic process cleanup.
  Upstream musl runtime covers pipe transition wakeups plus connected UDP DNS
  socket readiness over real virtio-net. Core readiness table has eight host
  tests and passes no_std AArch64 check.
- AArch64 network RX drains at most 16 virtio frames per bounded bottom-half
  invocation, routes TCP segments into pooled per-socket 32 KiB buffers,
  advances sequence/FIN state, emits ACKs, and wakes poll/epoll. On QEMU `virt`,
  lower-EL GICv2 INTID 76 is the normal receive trigger; the timer invocation is
  retained for recovery. HVF runtime resolves `example.com`, sends a real HTTP
  request, blocks in epoll, wakes from async RX, then validates HTTP/1.x.
- AArch64 now shares x86's bounded 32-record structured log ring and syscalls
  28/29. Parent EL0 appends a severity-5 record, reads payload plus monotonic
  timestamp/PID/severity metadata, and emits `MAKOS_AARCH64_LOG_OK`. ABI feature
  discovery now truthfully includes log bit 7 and package bit 16. `MAKLOG01`
  stores exact ring snapshots at `/.makos-system-log` through MakFS4 COW, loads
  prior records, then merges pre-mount boot records. Whole-image CRC, canonical
  ring validation, corruption preservation, three codec tests, both kernel
  checks, first-boot persistence, real Browser sandbox denial, and second-boot
  durable merge proof pass.
  Log reads require `CAP_CONSOLE`; persistent severity-4 audit records cover
  authentication decisions, account creation, sign-out, and package transaction
  decisions. Browser access-control denial is guest-proven; two-boot audit merge
  remains pending.
- MakOS clang now defaults C/C++ target compilation to
  `-fstack-protector-strong`; explicit `-fno-stack-protector` remains available
  for musl bootstrap. Toolchain gate proves protected objects reference
  musl's guard/failure runtime while opt-out objects do not. SysV startup already
  supplies virtio-RNG-backed `AT_RANDOM`. Rebuilt deployed CRT probe corrupts a
  real protected stack canary, enters musl `__stack_chk_fail`, and faults at
  lower EL. AArch64 converts lower-EL instruction/data aborts into process-group
  status 139 while retaining EL1 fatal behavior; parent wait/reap, shell
  survival, and the remaining two-boot gate pass. Broader app rebuild remains.

- Reproducible Rust workspace and x86_64 freestanding/UEFI targets.
- Separate UEFI loader and ELF64 kernel.
- ELF validation: identity, class, endian, ISA, header sizes, table/segment
  bounds, loadable segment size, physical span overflow.
- Exact physical allocation and zero/copy of `PT_LOAD` image.
- GOP framebuffer and ACPI 1/2 RSDP discovery.
- `ExitBootServices` using final memory-map key through uefi-rs.
- Versioned loader/kernel boot ABI.
- FAT-loaded `MAKOS.CFG` crosses BootInfo ABI v2 inline; kernel validates root,
  serial logging, and automatic MakFS recovery policy before initialization.
- Kernel COM1 initialization/output and framebuffer status screen.
- Dependency-free FAT32 disk construction.
- ELF parser unit tests, image/ELF artifact checks, QEMU serial boot test.
- Bitmap physical-frame allocator covering up to 64 GiB; boot-map ingestion,
  reserve/allocate/free/accounting, host tests, boot allocation self-test.
- Kernel bump heap with `alloc` support and live `Box`/`Vec` test.
- Kernel-owned GDT/TSS/IST, IDT, breakpoint/page-fault handlers, NX support,
  owned CR3 and four-level tables with early 4 GiB identity map.
- PIT 100 Hz interrupts and preemptive round-robin stack switching across three
  kernel tasks.
- Checksummed ACPI RSDP/XSDT/MADT parsing; local APIC enablement.
- SMP INIT/SIPI trampoline: real mode -> protected mode -> long mode; four QEMU
  CPUs online with separate AP stacks.
- AArch64 QEMU `virt`/HVF SMP: ACPI MPIDRs feed genuine PSCI 0.2+
  `CPU_ON_64` HVC calls. Three secondaries enter EL1 through an identity-mapped
  assembly trampoline, install private 1 MiB stacks plus shared MMU and
  per-PE VBAR/GICC state, then participate in a coherent parallel-work
  rendezvous. Runtime rejects CPU_ON success without all APs online and counter
  progress during BSP work. APs enter closed-gate, masked-interrupt WFE
  afterward; system-wide SEV only causes a gate recheck. EL0 scheduler remains
  BSP-only.
- Ring-3 isolation, ELF userspace init loading, executable/read-only code pages,
  16 KiB writable/NX user stacks, 64 KiB ring-0 syscall stacks, DPL3 `int 0x80`
  ABI, and live-PTE pointer validation.
  Copyin accepts readable RX pages; copyout requires every page writable and
  rejects unmapped holes without kernel faults.
- Eight-entry preemptive scheduler with five reusable dynamic task slots. It
  concurrently runs two path-loaded ring-3 ELF children with distinct CR3
  roots, ring-0 stacks, PIDs, exit records, and parent waits.
- Wait reaps scheduler slot, all user leaf/page-table frames, FDs, handles,
  sockets, and surfaces; integration trace proves frame reuse and stale-window
  removal.
- Process-local typed handles, bounded bidirectional channels, live ring-3 IPC
  sentinel round trip, yield and process exit.
- Native ABI v1.0 discovery/feature mask; documented syscall table.
- Shared-address-space user threads with distinct stacks, pointer argument,
  join/exit, typed auto-reset event, true scheduler block/wake, handle close.
- Process-owned anonymous VM manager: sixteen regions/process in a 1 MiB arena,
  up to sixteen pages/region, zero-fill, first-fit hole reuse, exact-region
  unmap, partial-range protection, W^X, and teardown reclamation. Legacy
  one-page mmap ABI remains compatible.
- Legacy ATA PIO block driver on secondary channel with identify/read/write/
  cache flush and sparse 1 GiB data disk; old images extend without data loss.
- MakFS v1 redundant checksummed superblocks with generation/boot transactions,
  root directory, persistent regular file, mode/UID/GID/size/timestamp/data CRC.
- Two-boot test proving persistent generation and file data readback.
- Fault-injection test corrupts primary superblock and allocation bitmap between
  boots; MakFS selects valid backup, reconstructs bitmap from CRC-valid inodes,
  repairs metadata, advances generation, and preserves user data.
- Mounted VFS root with process-local open/read/close file descriptors; ring-3
  reads `/boot-count.txt`, while owner/mode policy rejects write-open.
- VFS directory tree for `/`, `/home`, `/home/user`; `stat`/`readdir` expose
  inode, type, mode, UID/GID, size, and monotonic modification time.
- Capability/mode-checked `create`/`unlink` backed by sixteen CRC-protected MakFS
  inodes, an 80-block bitmap, copy-on-write replacement, and 2 KiB file limit.
  Boot test migrates a legacy v1 record, creates/writes ten names including a
  1,024-byte two-block file, then remounts/reads/unlinks all ten and proves absence.
- Backward-compatible MakFS v3 dynamic inodes persist file/directory kind and
  canonical nested paths. `mkdir`/`rmdir`, nested stat/open/chdir/readdir,
  nonempty rejection, cwd-busy rejection, and recursive path lookup pass in EL0.
- PCI config-space enumeration and RTL8139 bus-master driver.
- Locked RTL8139 C-mode transmit ring cycles all four hardware descriptors in
  device order and uses one DMA buffer per descriptor; completion/error status
  is checked before reuse.
- Real Ethernet, DHCP offer parsing, ARP, IPv4 checksums, ICMP echo/reply, UDP,
  DNS A-query/response, and complete checksum-checked TCP HTTP exchange.
- IPv6 packet construction/checksums, NDP neighbor solicitation/advertisement,
  and ICMPv6 echo/reply through QEMU user networking.
- Process-owned, generation-tagged AF_INET socket objects expose create,
  connect, send, receive, and close for connected UDP/TCP. Ring 3 resolves DNS
  over UDP, then fetches HTTP over TCP; stale handles and unprivileged creation
  are boot-tested as denied.
- IRQ-driven PS/2 keyboard/mouse driver with 64-byte keyboard queue, 16-event
  button-edge queue, latest-position motion coalescing, and adaptive pointer
  acceleration. Native ring-3 interactive shell exposes real status, directory,
  file-read, identity, echo, uptime, clear, and exit commands.
- Two process-owned graphical surfaces, clipped software composition, framed
  windows, z-order, and scanout. x86_64 renders a bounded pointer from an
  immutable cursor-free scene shadow. AArch64 uses virtio-GPU's dedicated
  64x64 cursor resource/queue: pointer motion submits `CMD_MOVE_CURSOR` and
  performs no scanout writes/transfers. QEMU's macOS host pointer remains
  hidden. QMP scanout captures intentionally exclude the hardware cursor
  plane; boot verification therefore requires every scanout frame to remain
  byte-identical across repeated cursor moves and an exact return trip.
  Compositor retains mouse input, hit testing, click-to-front focus, taskbar
  minimize/restore,
  close buttons, Start-menu reopening with retained app state, and low-cost
  XOR-outline title-bar dragging. Taskbar tray renders PL031 UTC time plus DHCP
  status; Settings reports wired network online and truthfully disables Wi-Fi
  setup when no wireless device exists. Focused System Monitor refreshes real
  PMM/scheduler data at 1 Hz using content-only damage; hidden/background state
  costs no redraws. Terminal retains
  58x22 cells and renders live input/output in its process-owned window.
- Native framebuffer login form displays explicit Username/Password fields,
  active-field focus rectangle/caret, instructions, and live masked input before
  session authentication. Native widget renderer ports 95.css visual grammar:
  system palette, raised/sunken bevels, navy title bars, buttons, and taskbar.
  Isolated native Browser fetches HTTP over guest virtio-net and renders bounded
  readable HTML; no CSS engine, JavaScript, or host WebView exists.
- PCI AC97 mixer/PCM setup and bus-master stereo 48 kHz DMA progression.
- Capability-gated ring-3 s16le PCM syscall copies validated samples into AC97
  DMA buffer; hardware progression is integration-tested.
- PCI UHCI controller, port reset, queue-head/TD engine, USB control transfers,
  descriptor/configuration enumeration, HID keyboard interrupt-IN reports.
- UID/GID credentials, capabilities for console/graphics/IPC, kernel-pointer
  denial, permission-aware syscall copyin/copyout, mode-bit file access, and A/B
  update metadata with CRC/hash rollback.
- Interactive login for `marcus`: unauthenticated PID1 receives only
  console/input; salted PBKDF2-HMAC-SHA256 (100,000 iterations) verification
  creates uid/gid 1000 session and scoped capabilities; wrong-password denial
  is boot-tested.
- Static C17 SDK/libc wrappers and native C application; RSA-2048/SHA-256
  authenticated disk-backed A/B package install/upgrade/remove/rollback on
  layout-compatible 1 GiB volumes (RAM fallback on small legacy disks), with
  signed SHA-256 content authentication, CRC-protected durable snapshots, a
  checked built-in `libc` dependency, boot-tested tamper rejection on fallback
  path, and capability-gated mutation; durable routing is host/structurally
  tested, not guest-tested. Active durable payloads map read-only below
  `/packages`, versioned exact/minimum dependency graphs are signed, and
  Settings reports backing/generation/count. Structured 32-record log ring/clock API.
- Linux x86_64 personality fixture in isolated PID3 translates and tests syscall
  numbers for write/getpid/uname/clock_gettime/exit via current int80 adapter.
- PE32+ x86_64 parser/loader validates image/section bounds, maps W^X/NX
  sections, and enters isolated PID4; Microsoft x64 fixture tests eight Win32
  thunks backed by kernel console/time/event/process facilities.
- Native init supervises isolated `demo` service with on-failure policy: detects
  deliberate user #PF as exit 142, reaps it, restarts once, then observes clean
  completion. Ring-3 fault cannot halt kernel.
- Isolated PID6 toolchain seed parses an arithmetic source expression, emits a
  native x86_64 function, transitions its page from writable/NX to read/execute,
  executes result 42, and proves simultaneous write/execute permission denial.
- Toolchain also constructs a valid 1,150-byte/three-block x86_64 ELF64 image
  and persists it as `/home/user/generated.elf`. Generic syscalls 56/57 snapshot a
  readable VFS path, rejects malformed/non-ELF and overlapping layouts,
  validates up to four nonoverlapping `PT_LOAD` segments under W^X, maps split
  RX code plus read-only/NX data and stack in fresh CR3 roots, launches bounded
  PID7 and PID8 concurrently, then wait/reap destroys all child frames. ABI57
  copies versioned bounded argv/env into child stack, supplies SysV
  argc/argv/envp/auxv plus direct entry registers, rejects malformed vectors,
  and preserves ABI56 default startup. Generated ring-3 code validates all
  fields; boot2 repeats after remount.

## Partial

- Architecture abstraction: x86_64 has broadest current feature set. AArch64
  UEFI/HVF executes native ARM code with PL011, BootInfo validation, PMM, heap,
  owned MMU tables, exception vectors, GICv2 timer IRQs, isolated EL0 ELF/SVC,
  virtio keyboard/tablet, virtio-blk, virtio-net, persistent MakFS/VFS, login,
  compositor, EL0 terminal, native Settings/user management, Files, isolated
  Text Edit, isolated native Browser, upstream MicroPython fallback, and
  package-selected official CPython 3.14.7 runner.
  Browser uses DHCP/ARP/IPv4, UDP DNS, TCP/HTTP, owner-checked surface events,
  and sandbox credentials. Portable eight-slot process table provides preemptive
  register/address-space switching plus PID/FD/socket/surface/frame cleanup.
  SMP userspace scheduling, audio/USB, compatibility, and package/service
  parity remain.
- Memory management: physical allocator, owned tables, per-process CR3 roots,
  user mappings, NX, W^X range `mprotect`, faults, multi-region mmap/unmap,
  fragmentation reuse, frame reclaim, package-backed demand paging, sparse
  demand-zero, and guest-proven `MADV_DONTNEED`/`MADV_FREE` decommit work;
  writable/shared file maps, COW, ASLR, higher-half kernel, swap, and kernel-heap
  reclaim remain absent.
- Scheduling: preemptive kernel/user tasks and event block/wake work on BSP.
  Four AArch64 PEs execute coherent EL1 code with private stacks. Current-task,
  kernel-return, and active-TTBR state are CPU-indexed. A bounded shared Ready
  queue balances six EL0 load tasks over AP1-3 through 288 yields with even
  `99,99,99` dispatch counts and
  exclusive ownership. After desktop startup, Firefox and ordinary Native
  application workers are AP-eligible while leaders plus shell, UI, service,
  and device-owner roles stay on CPU0. Syscall 148 now provides
  explicit per-thread masks and forced migration for eligible workers;
  automatic load-driven balancing, additional built-in/service roles and
  priorities, and
  real Firefox/desktop contention remain open.
  CPU0-only virtio input, virtio-net TX/RX,
  virtio-blk, and virtio-GPU submission now have focused AP runtime evidence
  for keyboard wake, copied UDPv4 DNS send/receive, timer-serviced 4 KiB
  filesystem read/write plus `fsync`/FLUSH, and deferred retained-surface
  transfer/resource-flush completion. A separate TCPv4 gate now proves an AP
  connect/send/blocking-receive/FIN lifecycle with CPU0-owned TX/RX and exact
  I/O idle/wake/resume. A bounded migration gate also moves the same saved TID
  from AP1 to AP2 with exclusive ownership and GPR/SP/TLS/SIMD preservation.
- Processes/userspace: isolated ELF processes, spawn/wait/exit, user threads,
  static C libc, shell, login, package/log APIs, and two-slot static-ELF
  exec-by-path with bounded argv/env; no fork/COW, complete signals, general PID
  allocator, declarative service units/dependencies, or full utility suite.
  AArch64 additionally embeds upstream MicroPython 1.28.0 fallback. When
  `/usr/bin/python3` is present, `python FILE` loads official CPython 3.14.7
  PIE from disk package, uses upstream musl PT_INTERP and deterministic stored
  stdlib ZIP, reads source under per-session VFS identity, then parent-waits and
  reclaims full address space. Host Python performs build generation only.
  AArch64 also boots genuine upstream musl 1.2.6 static programs through both
  custom entry and upstream `crt1.o`/`__libc_start_main`; real SysV startup,
  TLS, main-thread TID, VFS/TTY calls, per-process `dup`/`dup3`/`fcntl`,
  shared-offset `lseek`, blocking/nonblocking atomic `pipe2`, timed pipe/file
  `poll`, path/FD metadata, persistent nested directory streams,
  per-process cwd/relative lookup,
  true non-truncating `O_RDWR` open-file descriptions and bounded POSIX
  byte-range record locks,
  zero-fill/shrink `ftruncate`, virtio/ATA durability-barrier `fsync`, positional
  `pread`/`pwrite` with shared-offset preservation and sparse zero-fill, pthread
  create/join, per-task signal-mask inheritance, atomic masked
  `pselect`/`ppoll`/`epoll_pwait`, handler return, return/exit, wait, and reap pass under
  HVF. Robust-list owner-death/wake-one, exact-TID signals, relative timed futex
  expiry, and bounded `pthread_mutex_timedlock` ETIMEDOUT all pass current guest
  runtime; cancellation and full signal breadth remain incomplete.
- Filesystem: legacy small-volume fallback retains sixteen slots, while mounted
  MakFS4 uses 512 inode records, 255-byte components, 4 KiB extents, CRC/COW
  metadata, redundant roots, persistent symlinks, and Unix timestamps. Current
  source adds a 1,024-bucket collision-chained child index rebuilt from
  authoritative inode metadata plus resumable raw-inode directory cursors.
  Forced-collision/80-entry unit coverage and a two-boot 64-sibling/name255
  create/remount/random-lookup/cleanup guest probe pass. Geometry is
  still bounded; read-only offline fsck exists, but no repair mode,
  common-filesystem driver, or on-disk tree index.
- Networking: guest-tested IPv4 plus process-owned connected UDP/TCP socket
  objects and ICMPv6 packet probes. AArch64 native AF_INET6 sockets now preserve
  `SOCK_NONBLOCK`/`SOCK_CLOEXEC`, support read/write plus fcntl flag changes,
  expose 16-byte IPv4 and 28-byte IPv6 local/peer endpoints, type/error/buffer/domain/protocol queries,
  TCP_NODELAY, SHUT_RDWR, wildcard UDP bind, addressed datagrams, and bounded
  scatter/gather messages. Virtio-net implements validated RA /64 SLAAC,
  EUI-64 link-local/global addresses, NDP, checksum-required UDPv6, and TCPv6.
  Patched
  musl resolver source reads DHCP-provided DNS through MakOS network config
  syscall 61 when `/etc/resolv.conf` is absent, requests A+AAAA only for
  genuinely configured families, and closes flagged sockets across exec.
  Offline wire tests and kernel/static/shared-libc builds pass; constrained
  IPv6 guest proof is pending. AArch64 confines low-level virtio-net TX/RX to
  CPU0; AP UDPv4/v6 sends use a bounded copied-request queue, with a real AP1
  UDPv4 DNS transaction qualified under Pi/TCG. Stateful AP TCPv4 connect,
  segment send, ACK/window update, receive, and FIN now use copied owner-service
  requests and pass an exact AP1/CPU0-host exchange under Pi/TCG. TCPv6 has the
  same copied service structure but still lacks guest runtime qualification.
  AArch64 uses bounded interrupt-driven TCP RX with timer recovery and
  poll/epoll wake; no
  listen/accept, DAD, IPv6 extension headers/scoped link-local socket API,
  broad options, routing policy/firewall, or physical-device IRQ qualification.
- Graphics: 95.css-inspired native framebuffer theme, six-slot software
  compositor, Start launcher, app taskbar, drag/close/minimize/reopen, bounded
  resizing, per-surface input events, retained terminal, and architecture-
  specific cursor isolation preventing pointer damage; no scalable fonts,
  multi-display, GPU acceleration, or full desktop application suite. AArch64
  Text Edit provides drag selection plus a session-isolated 64 KiB clipboard.
  AArch64 cursor uses virtio-GPU's cursor plane; pure motion never mutates or
  transfers scanout pixels, and QEMU's host pointer stays hidden.
  Retained-surface fill/blit/text paths mark damage only when pixels change;
  unchanged presents skip composition and virtio-GPU scanout flush. Files and
  Text Edit discard hover-only motion when no drag/selection state changes.
  If sleeping task is scheduler's last runnable task, kernel waits in EL1
  `WFI` until monotonic deadline; libc/Firefox `nanosleep` cannot fall into an
  `EAGAIN` busy loop merely because every other task is blocked on input/I/O.
  Last-runnable poll/epoll/socket waits use one-interrupt `WFI`, pump net/input,
  preserve timeout across retries, then re-enter original syscall; no-successor
  scheduling cannot turn blocking I/O into host-CPU spin.
- AArch64 virtio-rng MMIO consumes host `/dev/urandom`; native SysV process
  startup supplies unpredictable 16-byte `AT_RANDOM`, bounded
  `argc/argv/envp`, standard auxv, aligned SP, and matching x0/x1/x2. EL0
  validation plus scheduler exit/wait/reap runs in the HVF boot suite.
- AArch64 scheduler saves/restores `TPIDR_EL0`; native calls 78-82 implement
  real TIDs, shared-VM thread clone, distinct stack/TLS, FIFO futex wait/wake,
  per-thread exit, clear-child-tid join, and task-only reap. Upstream musl
  `pthread_create`/`pthread_join` passes HVF. Task masks, clone inheritance,
  atomic masked readiness waits, `EINTR`, and restoration pass HVF. Per-task
  robust-list register/query plus bounded thread/group-exit owner-death cleanup
  pass a blocked-waiter raw-exit guest probe. Credential-checked `kill` plus
  exact-task `tkill` and `tgkill` pass process and exact-handler-TID assertions.
  Relative timed futex expiry passes a contended `pthread_mutex_timedlock` probe
  requiring `ETIMEDOUT` in 50 ms..1 s. Broader signal semantics and
  cancellation breadth remain.
- AArch64 loader executes PIEs through official musl 1.2.6 `PT_INTERP`; HVF
  verifies upstream `_dlstart`, libc resolution, separate VFS `DT_NEEDED` DSO
  lookup/open/fstat/eager-private-map, RELA/PLT/GOT, exported symbol call,
  RELRO, plus runtime `dlopen(RTLD_NOW)`/`dlsym`/call/`dlclose`, exit/wait/reap.
  Generic read-only system-package descriptors also map 920 KiB `libc.so`,
  exceeding mutable MakFS's 2 KiB file cap. Same-PID AArch64 `execve` now
  replaces a trusted system-package PT_INTERP image, copies bounded argv/env,
  applies FD_CLOEXEC, preserves process/session identity, and reclaims the old
  root under HVF. Arbitrary validated VFS executables, multithreaded exec,
  disk-backed scalable/recursive/TLS-versioned dependencies and broad
  writable/shared mappings remain. Package-backed demand paging executes
  Firefox; sparse anonymous demand-zero and decommit have current guest proof.
- Official Firefox ESR shared-musl PIE/DSO outputs now complete linking with
  exact `/lib/ld-musl-aarch64.so.1` and `libc.so` audits. Mozilla
  `stage-package` plus MakOS integration yields 42 checksummed package files and
  260,052,147 payload bytes, including Firefox, CPython, nano, licenses, font,
  and terminfo. VFS package descriptors stream sector ranges from virtio-blk
  instead of embedding them in kernel RAM. Guest Firefox now passes JIT/XPCOM,
  creates a 700x400 native window, paints real browser chrome through retained
  Skia blits, and runs a blocking input watcher. Exact typed input reaches the
  URL bar; native DNS/TCP plus NSS validate `https://example.com/` and complete
  HTTP 200. Strict Gate3 proves real central document paint: 138,000 changed
  pixels, 324 colors, visible heading/body/link, and exact final URI. QMP-to-
  first-character latency is 104 ms and every URL character is at most 300 ms.
  Current package paints in 164,388 ms; Ctrl-A takes 9,373 ms and first URL
  character 110 ms. It also passes URL clipboard reload, real link click,
  rendered document drag-selection, clipboard copy, Ctrl-L, and exact pasted
  reload. A
  persistent watcher hint, watcher-dequeue leader handoff, and futex refresh
  keeps input processing inside a bounded 10-second priority window. MakOS now
  caps IndexedDB at 2 connection workers and stream transport at 4 workers/1
  idle worker, while preserving upstream limits on other targets. Performance
  remains incomplete for broader modern-site and interaction coverage. Widget source provides pointer,
  wheel, resize/close, DOM keyboard accelerators, and UTF-8/UTF-16 clipboard;
  broad runtime interaction coverage remains incomplete.
- AArch64 VM syscall 114 plus official-musl syscall 233 translation now provide
  real `MADV_DONTNEED`/`MADV_FREE` decommit: resident frames are released while
  region/file metadata remains for later zero-fill or package reload. Advisory
  access/huge-page/dump policies remain validated hints; decommit runtime passes.
- MakFS4 source now stores Unix-epoch atime/mtime/ctime and persistent symlink
  targets. Official-musl patch 62 translates `symlinkat`, `readlinkat`, extended
  stat/fstat, `AT_SYMLINK_NOFOLLOW`, and `DT_LNK`; clean 1,350-object musl build,
  strict probe link, full unit suite, AArch64 release, and two-boot guest
  create/readlink/lstat/follow/readdir/unlink/remount proof pass.
- MakFS4 child lookup now uses a 1,024-bucket collision-chained cache index over
  512 authoritative inode records; directory iteration resumes from raw inode
  cursor instead of rescanning prior entries. Unit coverage forces every one of
  80 entries into one bucket. Current musl probe creates 64 siblings plus one
  255-byte name, validates complete `readdir`/lookup after remount, then removes
  them. Clean musl link, unit suite, dual-arch checks, AArch64 release, and
  two-boot guest execution pass.
- AArch64 AF_UNIX stream `socketpair` and musl `SCM_RIGHTS` translation now have
  a strict probe: one prefix byte must not receive queued rights; associated
  byte transfers an open-file description whose payload remains readable after
  sender closes original FD. Kernel refcount/queue cleanup guards, C probe
  warnings-as-errors link, full unit suite, AArch64 release, and exact-byte
  guest execution pass.
- New read-only `makos-makfs4-fsck` accepts raw data images or validated MakOS
  partitions from redundant GPT, then checks both filesystem roots, exact
  metadata-set geometry, catalog/inode CRCs and counts, parent types/cycles,
  duplicate child names, extent bounds/overlap, and allocation bitmap agreement.
  Six sparse host tests pass, including GPT offset, root fallback, and corruption
  rejection. Exported quiescent two-boot guest volume passes; repair mode remains.
- MakOS process-identity operation 3 plus official-musl `getppid(173)` now
  resolves every thread through its group leader to the real process parent.
  Genuine pthread probe source asserts main/worker consistency; execution is pending.
- Input/USB: IRQ-driven PS/2 keyboard/mouse and UHCI USB keyboard; no hubs, hotplug,
  USB mass storage, xHCI, event queue, layouts, or USB mouse.
- Audio: AC97 DMA and ring-3 fixed-format write API work; no buffered mixer,
  multi-client streams, volume service, notifications, or HDA.
- Security/update: login, credentials/capabilities/mode checks, sandbox denial,
  RSA-signed package manifests, redundant A/B record, default strong stack
  protection, and guest-proven AArch64 lower-EL fault containment; no executable
  signatures, general repository, ASLR, MAC, Secure Boot policy, rotating
  repository keys, memory-hard password KDF, crash restart, or recovery UI.

## Planned / not implemented

- Full desktop apps, GPU acceleration, package repository/solver/key rotation.
- Stronger sandboxing, transactional recovery boot selection.
- Broad dynamic-linker semantics, end-to-end self-hosting toolchain, broad
  POSIX/Linux compatibility, broad Win32 compatibility.
- Real-hardware qualification and full AArch64 service/driver/userspace parity.
- Full POSIX libc breadth and complete native Firefox window/navigation remain
  incomplete. Native GNU nano build/package and two-boot save/reopen runtime
  integration pass.

## Tested platform

- Partial 2026-08-25 Raspberry Pi 4 / Debian 13 host proof: user-local QEMU
  10.0.11 TCG with AAVMF 2025.02 initially trapped at the kernel ELF entry
  because the loader used execute-never `LOADER_DATA`. The loader now uses
  executable `LOADER_CODE`; the unchanged AArch64 harness passes UEFI handoff,
  four-PE PSCI/SMP, EL0 scheduler/process/VM probes, virtio block/net/rng/GPU/
  input, MakFS4, authenticated login/desktop, typed IPC, upstream musl threads,
  signals, futexes, dynamic linking, and Python execution. The run is not a
  full gate pass. A later unchanged run additionally passed the new guest
  assembler, persisted-ELF loader/execution/status-42 lifecycle, typed IPC,
  musl and Python before slow TCG again produced Settings resize `560x360`
  instead of the required `450x290` marker. A later focused run passed the
  three-object ELF64 `ET_REL` linker, both cross-object calls, both linked C
  paths, and both final-ELF executions. Thresholds
  and expected geometry remain unchanged. This is functional Pi evidence only,
  not Apple-HVF performance
  qualification; `/dev/zram0` supplied 1.8 GiB swap and KVM was unavailable to
  the unprivileged user. An earlier focused run passed the two-function C
  translation unit, 872-byte multi-definition object, same-object CALL26, exact
  array mutation, and both final-ELF executions. A still newer focused run
  passes bounded scaled pointer addition across the call and inside `adjust`,
  a 272-byte two-definition `.text`, 880-byte object, exact direct-array
  mutations, known one-past-end denial, and both final-ELF executions. The
  latest focused run adds signed scalar-variable element offsets and all four
  signed ordering relations: it proves two dynamic positive additions, a
  `SXTW #2` negative-one load, three exact three-element mutations, all four
  signed relations, then signed pointer differences of `3`/`-3` used directly
  and by the linked program, with sixteen malformed-C denials,
  persistence/reopen and both status-42 final-ELF executions. The latest
  three-function run added a later-defined 60-byte
  `combine`, a second same-object relocation, 2 KiB object capacity, and a
  four-function fail-closed case. Its exact artifacts are 76/140/168/60 code
  bytes, 688/1,032 object bytes, 444 linked bytes and an 815-byte ELF. The
  latest three-object run moves `combine` into its own persisted C source and
  616-byte object; the program object is 976 bytes, its `adjust`→`combine`
  reference resolves externally, linking without the library fails closed,
  and the exact 444-byte linked program still executes twice with status 42.
  The subsequent manifest-build run routes the same graph through a real
  versioned MakFS build description and denies four malformed manifests before
  preserving every artifact size, relocation, execution, and loader result.
  The latest focused run first seeds that fixture, then invokes the authenticated
  `makbuild` CLI against the persisted manifest. Kernel-built SysV `argc=2`/
  `argv[1]`, distinct fixture/build modes, and `seeded=0` for CLI processes all
  pass. The following focused cache run executes six CLI builds and proves
  cold, warm, object-corrupt, source-edited, and state-corrupt invalidation
  outcomes on that fixed three-input graph. The newest variable-graph run adds
  a 56-byte `helper` translation unit in a 608-byte fourth object, emits 500
  linked bytes, executes `helper(40)=42`, and uses the 120-byte `MAKSTATE2`
  format. Eight authenticated CLI builds prove four-input `0/4`, `4/0`, `3/1`,
  `4/0`, `3/1`, `4/0`, `0/4` and a distinct three-input graph's `0/3`, `3/0`;
  all toolchain processes reap with status 42.
  A fresh private TCG boot then
  reached and visibly captured the native 800x600 login dialog. This remains Pi
  functional evidence, not macOS/HVF timing qualification. The
  focused four-vCPU TCG SMP input/network image also
  passed three runs with CPU0-owned UDP TX, DNS RX wake, and exact frame
  balance (two before and one after the final malformed-length guard). One
  intervening post-guard run completed the network proof but missed the later
  QMP-injected Ctrl-K; its immediate unchanged repeat passed the full gate. The
  later block-service image passed twice with a real AP1 `fsync`/CPU0 FLUSH.
  One intervening run failed in the earlier unchanged EL1 exit-group
  rendezvous and never reached block initialization; the exact immediate
  repeat passed block, network, input, and boot completion. The subsequent
  production timer-service gate completed 33 AP filesystem requests (18 reads,
  10 writes, 5 flushes). The first five-guest-second evidence run reached the
  block marker but consumed the focused harness window before the later input
  readiness marker; its exact repeat entered the block phase but exhausted the
  same window before the marker. A one-guest-second window retained exact
  timer-owner counter proof and passed the complete unchanged 90-second gate.
  The next focused Pi/TCG run added AP1 native surface create/fill/present and
  passed one CPU0 deferred composition, two real virtio-GPU submissions, one
  transfer, one resource flush, exact surface/frame cleanup, all existing
  device phases, and boot. The later dedicated TCPv4 gate passed an exact AP1
  connect/send/receive/FIN lifecycle through a delayed host fixture, including
  four copied owner completions and I/O idle/resume masks `0x2`. A fresh
  normal-config visible QEMU then reached and visibly captured the 800x600
  native login dialog.
- Passed 2026-08-14: QEMU 11.0.3 `pc` + bundled OVMF x86_64,
  Apple Silicon M3 host, TCG emulation.
- Test uses four vCPUs, 256 MiB RAM, RTL8139, two ATA disks, PS/2 keyboard,
  GOP 1280x800, QEMU user networking, AC97, UHCI USB keyboard. Full marker chain passes twice;
  second boot proves persisted ten-file/multi-block MakFS data, persisted ELF
  execution, plus superblock and allocation-map recovery.
- Real hardware, VMware, VirtualBox: untested; not claimed.
- Passed 2026-08-17: QEMU 11.0.3 `virt` + AArch64 edk2 on Apple Silicon M3,
  Apple HVF, native AArch64 ISA, four vCPUs, 1 GiB, UEFI handoff, PL011,
  PMM/MMU, exceptions/GIC/timer, isolated EL0, virtio input/block, two-boot
  MakFS persistence, authenticated login, lowercase/punctuation, history/Tab,
  framebuffer pixels, drag, resize, close, Start-menu reopen, virtio-gpu dirty
  transfers, live 800x600/1024x768/1280x800 mode switching, and Text Edit VFS
  save/reopen behavior, timer-preemptive register restoration, virtio-net
  DHCP/ARP/DNS/TCP/HTTP, isolated Browser render/reopen/close, and exact
  cursor-scene pixel stability. Real upstream MicroPython parser/compiler/VM
  executes a VFS script containing comprehension, multiplication, `range`, and
  `sum`, exits status 0, and reaps 115 frames. Cocoa `zoom-to-fit` permits live host-window
  scaling.
- Passed 2026-08-23: official GNU nano 9.1 AArch64 PIE plus official ncurses
  6.5 under Apple HVF. Focused probe types punctuation-bearing content, writes
  through Ctrl-O, exits Ctrl-X/status 0, reopens, reboots with the same MakFS4
  package/profile disk, verifies 22 persisted bytes, and reopens again.
- Passed 2026-08-23: official CPython 3.14.7 AArch64 MakOS PIE under Apple
  HVF. Guest proof reports `(3, 14, 7)`, parses/compiles/executes VFS source,
  computes 190, uppercases text, rereads its own file, imports upstream `json`
  from 556-module stored stdlib ZIP, exits status 0, and reclaims 1,838 frames.
  Artifact links only target `libc.so`; fake/host delegation are absent.
- Passed 2026-08-23: guest-side AArch64 installer copies live GPT system to a
  distinct blank virtio-blk disk only after exact administrator confirmation.
  Automated safety gates prove wrong token, nonblank media, unequal geometry,
  and absent target refuse; target hashes remain unchanged for host-observable
  refusal cases. Installed disk boots with live source detached, then preserves
  newly created VFS file across second installed-only boot.
- Passed 2026-08-24: rebuilt AArch64 musl CRT probe deliberately corrupts its
  strong stack canary. Kernel records the lower-EL data abort, terminates the
  process group with status 139, returns to the parent shell, reaps the child,
  and completes the full network-isolated two-boot suite.
- Added 2026-08-23 regression gates: once desktop clients finish startup,
  accepted-present count stays fixed during idle; Apple-HVF guest CPU time is
  bounded to 35% of sampled wall time; Start click feedback arrives within
  500 ms end-to-end; three Files hover moves cause zero accepted presents.
- x86_64-on-Apple-Silicon still uses cross-ISA TCG. AArch64/HVF removes ISA
  translation for completed ARM path; full OS parity is not yet claimed.

## Known constraints

- Kernel initially loads at 64 MiB, then installs owned identity page tables.
- COM1 and 32-bit RGB/BGR GOP modes only.
- x86_64 and AArch64 have single-disk GPT ESP+MakFS targets. x86_64 uses
  secondary ATA master plus shared CRC-validating GPT partition translation.
  x86 guest installer source selects secondary slave, requires administrator
  capability plus exact confirmation, refuses nonblank/wrong-size/conflicting
  resume targets, verifies writes, flushes, commits MBR last. Host safety tests
  pass. QEMU runtime passes refusal guards, SIGKILL before MBR commit, blank-MBR
  partial validation, source-matching resume, final SHA-256 equality,
  source-detached boot, and two-boot persistence.
  AArch64 guest Terminal installer supports fail-closed fresh/resume on an
  equal-sized virtio-blk target. Its runtime gate hard-kills before MBR commit,
  proves blank LBA0 plus source-identical partial blocks, resumes to exact
  source SHA-256, detaches source, and passes two persistence boots. Installers
  still lack graphical selection/partitioning, resizing, upgrades, and physical
  qualification.
- Loader uses audited third-party `uefi` crate for firmware protocol bindings;
  kernel has no third-party runtime dependency.
