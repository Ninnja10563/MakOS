# MakOS — T3 Code Nightly continuation

## Objective

Continue building MakOS toward original attached specification and all later user requests. Do not claim full completion while `docs/ORIGINAL-SPEC-AUDIT.md` contains Partial/Missing items.

Original specification source:

`/Users/marcushuang/.codex/attachments/4ef18f50-cd19-419b-93ed-d509edff0836/pasted-text.txt`

Current development host: Raspberry Pi running Debian Linux. The primary
interactive/performance qualification target remains macOS Apple Silicon using
AArch64 QEMU/HVF. Pi QEMU/KVM or TCG results are functional Pi evidence only
and never substitute for required macOS/HVF timing evidence. Workspace is a Git
repository on `main`, tracking `https://github.com/Ninnja10563/MakOS.git`.
Preserve existing files and changes.

## User priorities

1. Real Firefox/browser support, modern websites/protocols, usable performance.
2. Near-native mouse performance; guest cursor must remain visible without pixel corruption or host-cursor duplication.
3. Functional terminal: punctuation/lowercase, commands, selection, copy/paste, real nano.
4. Normal desktop behavior: draggable/resizable/closable/reopenable windows, taskbar apps, pressed button feedback, clear login focus/tab behavior.
5. Text editor save correctness, file explorer, settings/resolution/users/sign-out, system monitor, Python runner, Wi-Fi/clock UI.
6. Future fully bootable/installable OS beyond macOS hosting; AArch64 and x86 paths.
7. Boot QEMU for user testing at meaningful milestones.

## Current verified state

- AArch64 post-desktop production SMP now admits non-leader ordinary Native
  application workers as well as Firefox-role workers to AP1-3. Leaders,
  shell, UI, service, and device MMIO work remain CPU0-only. Separate
  exact-group/exact-role upstream-musl pthread gates prevent the Native fixture
  from satisfying Firefox evidence. Pi/QEMU 10.0.11 TCG Native evidence is
  `cpu_mask=0xe`, dispatches `11119,10254,11130`, `overlap_mask=0xa`, TIDs
  5/6, kernel-owned affinity migration/restoration, exclusive ownership, and
  status 42. The unchanged Firefox production regression passes with
  dispatches `9857,11153,9945`, `overlap_mask=0xa`, watcher TID 8/AP2, and
  status 42. Full `make unit check`, release/image artifacts, combined
  network/input-IRQ runtime, and cursor runtime pass on the Pi. This does not
  replace unchanged real-Firefox qualification on idle macOS/HVF.
- Active visible Pi/QEMU 10.0.11 TCG Native-SMP milestone: PID 699985,
  user service `makos-visible-native-smp-final3.service`, VNC
  `127.0.0.1:5901`, session
  `build/makos-pi-visible-native-smp-final3-54Bfbyox`, private read-only boot
  clone `boot.img`, private sparse `data.img`, private `vars.fd`, QMP
  `qmp.sock`, serial `serial.log`, PID file `qemu.pid`, and captures
  `login.ppm`/`login.png`.
  Boot clone and `build/makos-aarch64.img` both have SHA-256
  `f194b48ee3be8a8b939d41f312896f5479fff795965f4fd16a6dcbb8101efd70`.
  It is the sole QEMU process and reports four online PEs,
  the GICv2 network route (INTID 76), both input routes (INTIDs 77/78),
  `MAKOS_LOGIN_UI_OK`, `MAKOS_AARCH64_BOOT_OK`, and post-desktop
  `userspace_scheduler_cpus=4` with Firefox and Native AP-worker roles, and no
  fatal/panic. The visually inspected
  800x600 login PNG has SHA-256
  `ef6b87edd8b54b2714f2c3ab735235001b1fa63ed4d8cfeb7adb9d24678398b6`
  and shows the native login with username focus. Keep it running for user
  testing; use QMP `quit` before any later runtime gate. Prior PID 668793 and
  session `build/makos-pi-visible-network-irq-final-D6pbvEPD` were stopped
  cleanly through QMP before the Native-SMP runtime; their files remain. Two
  corrected launch attempts for this visible milestone exited before guest
  execution because the first used display-backend syntax for VNC and the
  second omitted QEMU's extracted data directory; no concurrent guest ran and
  their private files remain. Prior PID 660636 and
  session `build/makos-pi-visible-firefox-input-irq-final2-CShq1yHn` were
  stopped cleanly through QMP before the network-IRQ runtime; their files
  remain. Prior PID 659568 and
  session `build/makos-pi-visible-firefox-input-irq-final-kecJty1A` were
  stopped cleanly through QMP before the focused cursor regression; their
  files remain. Prior PID 651079 and
  session `build/makos-pi-visible-firefox-exact-input-final-4JLnogUm` were
  stopped cleanly through QMP before the interrupt-driven runtime; their files
  remain. Prior PID 630079 and
  session `build/makos-pi-visible-selfhost-signed-arithmetic-final2-rB4hMDDS`
  were stopped cleanly through QMP before the exact-handle runtime; their files
  remain. Prior PID 624159 and
  session `build/makos-pi-visible-selfhost-signed-arithmetic-final-CZWWR9Iz`
  were stopped cleanly through QMP before the exact-source final runtime;
  their files remain. Prior PID 614416 and
  session `build/makos-pi-visible-selfhost-three-argument-final-gZuXvkd1` were
  stopped cleanly through QMP before the signed-arithmetic self-host runtime;
  their files remain. Prior PID 606789 and
  session `build/makos-pi-visible-cpu-affinity-final2-kd98tM5Q` were stopped
  cleanly through QMP before the three-argument self-host runtime; their files
  remain. Prior PID 549619 and
  session `build/makos-pi-visible-firefox-input-affinity-final-WGpssi0N` were
  stopped cleanly through QMP before the affinity runtime; their files remain.
  Intermediate PID 604562/session
  `build/makos-pi-visible-cpu-affinity-final-0Fni6SPd` was stopped cleanly
  through QMP before the bounded-trace qualification rerun; its files remain.
  Prior PID 504710/session
  `build/makos-pi-visible-makstate2-graph-final-5oeaMcSe` was stopped cleanly
  through QMP before the Firefox input-affinity runtime; its private files
  remain.
  Prior PID 491323/session
  `build/makos-pi-visible-makstate-cache-final-KHjut1RP` was stopped cleanly
  through QMP before the variable-graph runtime; its private files remain.
  Prior PID 480589/session
  `build/makos-pi-visible-makbuild-cli-final-TEqxtQE7` was stopped cleanly
  through QMP before the focused cache runtime; its private files remain. Three
  corrected visible-launch attempts in the current session exited before guest
  execution because the first firmware image was the wrong pflash size and the
  extracted QEMU module directory was initially absent. They ran no concurrent
  guest; `final4` uses the 64 MiB no-secboot code image plus the extracted
  module directory.
  Prior PID 472733/session
  `build/makos-pi-visible-selfhost-manifest-build-final-O9IghdKK` was
  stopped cleanly through QMP before the MakBuild-CLI runtime; its private
  files remain.
  Prior PID 464852/session
  `build/makos-pi-visible-selfhost-three-object-link-final-Y2MmpHwi` was
  stopped cleanly through QMP before the manifest-build runtime; its private
  files remain.
  Prior PID 454609/session
  `build/makos-pi-visible-selfhost-three-function-final-hce1ALSI` was stopped
  cleanly through QMP before the three-object runtime; its private files remain.
  Prior PID 445551/session
  `build/makos-pi-visible-selfhost-pointer-difference-final-NM4U3tiN` was
  stopped cleanly through QMP before the three-function runtime; its private
  files remain.
  Prior PID 432919/session
  `build/makos-pi-visible-selfhost-relational-pointer-final-4x5uh1Ey` was
  stopped cleanly through QMP before the pointer-difference runtime; its
  private files remain.
  Prior PID 420287/session
  `build/makos-pi-visible-selfhost-two-parameter-final-Q6tVYzrX` was stopped
  cleanly through QMP before the relational/variable-pointer runtime; its
  private files remain.
  Prior PID 408245/session
  `build/makos-pi-visible-selfhost-pointer-add-final-KH1kf1pv` was stopped cleanly
  through QMP before the two-parameter self-host runtime; its private files remain.
  Prior PID 402725/session
  `build/makos-pi-visible-selfhost-pointer-add-xVy0hK88` was stopped cleanly
  through QMP before the final typed-address denial/runtime; its private files
  remain.
  Prior PID 393984/session
  `build/makos-pi-visible-selfhost-multifunction-pur2xwJK` was stopped cleanly
  through QMP before the pointer-add self-host runtime; its private files remain.
  Prior PID 386723/session
  `build/makos-pi-visible-selfhost-array-RPpobHaU` was stopped cleanly through
  QMP before the multi-function self-host runtime; its private files remain.
  Prior PID 378165/session
  `build/makos-pi-visible-selfhost-pointer-param-4xT0zGVU` was stopped cleanly
  through QMP before the fixed-array self-host runtime; its private files
  remain. Prior PID 365270/session
  `build/makos-pi-visible-selfhost-pointer-VRJSOOUV` was stopped cleanly
  through QMP before the pointer-parameter self-host runtime; its private files
  remain. Prior PID 358340/session
  `build/makos-pi-visible-selfhost-loop-Zlx4NpuL`
  was stopped cleanly through QMP before the pointer-capable self-host runtime;
  its private files remain.
  Prior PID 351586/session
  `build/makos-pi-visible-selfhost-three-object-hardened-dMPn4hSC` was stopped
  cleanly through QMP before the loop-capable self-host runtime; its private
  files remain.
  Prior PID 345856/session
  `build/makos-pi-visible-selfhost-three-object-final-ISxMsAVL` was stopped
  cleanly through QMP before the final malformed-relocation runtime; its
  private files remain.
  Prior PID 339593/session
  `build/makos-pi-visible-selfhost-three-object-FwD0d462` was stopped cleanly
  through QMP before final linker hardening/runtime; its private files remain.
  Prior PID 331488/session
  `build/makos-pi-visible-selfhost-flow-Eqae6jLh` was stopped cleanly through
  QMP before the three-object self-host runtime; its private files remain.
  Prior PID 324662/session
  `build/makos-pi-visible-selfhost-c-ubEkb4qP` was stopped cleanly through QMP
  before the self-host control-flow runtime; its private files remain.
  Prior PID 314733/session
  `build/makos-pi-visible-firefox-overlap-XO3FP0e3` was stopped cleanly through
  QMP before the self-host compiler runtime; its private files remain.
  Prior PID 286742/session
  `build/makos-pi-visible-production-smp-nL4neGKM` was stopped cleanly through
  QMP before Firefox-overlap qualification; its private files remain.
  Prior PID 275664/session `build/makos-pi-visible-selfhost-ItzH84jx` was
  stopped cleanly through QMP before the production-SMP work; its files remain.
  Prior PID 261990/session `build/makos-pi-visible-load-LhlxSbON` was stopped
  cleanly through QMP before the self-hosting build/runtime work; its files remain.
  Prior PID 248288/session `build/makos-pi-visible-migration-wHSJuT` was stopped
  cleanly through QMP before the focused shared-queue work; its files remain.
  Prior PID 241019/session `build/makos-pi-visible-tcp-5o5pxp` was stopped
  cleanly through QMP before migration runtime; its files remain. Prior PID
  224308/session `build/makos-pi-visible-TMvEbm` was stopped cleanly
  through QMP before focused testing; its private session files remain.
  Earlier PID 214025/session `build/makos-pi-visible-JZAZKK` was stopped cleanly
  through QMP before this build; its private session files remain.
  Intermediate PID 221943/session `build/makos-pi-visible-M60s26` was likewise
  stopped through QMP before the final marker rebuild; its files remain.
  Prior PID 193461/session `build/makos-pi-visible-ncttcL` and PID
  203938/session `build/makos-pi-visible-w5JUu3` were stopped cleanly through
  QMP before focused testing/building; their private session files remain.
- A bounded four-PE AArch64 EL0 scheduler proof now passes on Pi/QEMU 10.0.11
  TCG: a GICv2 SGI wakes the AP gate, each AP enables its banked virtual-timer
  PPI, the four contexts rendezvous immediately before EL0 entry, and four
  independent ELF processes report TIDs `1,2,3,4`, statuses 40-43 and
  `overlap_mask=0xf`. All roots/frames reap to baseline. Current release/image
  artifact checks, full `make check`, both structural guards, and visible login
  pass. The next runtime additionally has every AP block in `sleep_until`,
  return to its idle dispatcher, receive CPU0's timer wake, and resume with
  `resume_mask=0xe`. A fixed-affinity timed-futex phase also idles CPU0 in the
  syscall, returns every AP to idle, and proves the 20 ms timer wake with
  `futex_idle_mask=0xe`/`futex_resume_mask=0xe`. A 200 ms timed zero-descriptor
  `poll` proves I/O retry after AP idle with
  `io_idle_mask=0xe`/`io_resume_mask=0xe`. A second embedded EL0 process creates
  an auto-reset event, clones a shared-VM child, blocks its leader on AP1, and
  returns AP1 to idle. CPU0 enters the child through a validated RW/NX stack;
  the child signals the event and thread-exits without a local successor. AP1
  resumes the parent, which closes the handle and exits 44. Runtime requires
  `ipc_idle_mask=0x2`/`ipc_resume_mask=0x2`, child status 0, exact reap/frame
  balance, and a subsequent visible boot/login.
  A third EL0 fixture clones a busy AP1 worker, then invokes syscall-119 group
  exit from its CPU0 leader. The kernel publishes a dying-group scheduler
  exclusion, sends a GICv2 SGI, detaches the worker on AP1, switches AP1 off
  the shared root, and waits for acknowledgement mask `0x2` before reap. The
  fixture passes parent status 55, single-root cleanup, exact frame balance,
  and subsequent visible boot/login.
  A fourth fixture publishes a user flag only after its AP1 worker has entered
  the real yield SVC, then holds the syscall until CPU0 publishes group exit
  status 56. The safe outer-exception return detaches AP1 under the scheduler
  lock, switches to the kernel root, and acknowledges after a barrier. Runtime
  requires target/ack, `entered_el1_mask`, and `deferred_ack_mask` all `0x2`,
  single-root cleanup, exact frame balance, and subsequent login. A scheduler
  exception that entered just before publication is handled by the same locked
  stop contract and contributes an early-stop bit rather than losing the
  target.
  A fifth fixture rendezvous-holds two independent CPU0/AP1 processes inside
  syscall 119 before either can acquire the new teardown coordinator. Both
  acquire it serially, exit with statuses 57/58, reap distinct roots, and
  report `rendezvous_mask=0x3`/`serialized_acquire_mask=0x3` with exact frame
  balance. This concurrent cleanup exposed an actual overflow of the former
  64 KiB AP1 kernel stack into the adjacent kernel-root atomic; QMP inspection
  confirmed the cleared word. All AP kernel stacks now match the BSP at 1 MiB,
  and runtime requires `stack_bytes=1048576`.
  A sixth fixture enters syscall 119 from a shared-root leader on CPU0 and
  worker on AP1 with distinct statuses 59/60. Exactly one owns teardown; the
  other switches to the kernel root, transitions itself to Zombie, joins the
  stop acknowledgement, and never duplicates cleanup. Runtime proves
  complementary owner/join masks, first-owner-wins status, single-root reap,
  exact frame balance, and subsequent login.
  The boot-fixture gate closes before the desktop. The later production gate
  admits Firefox and ordinary Native application workers; additional
  built-in/service roles, automatic load balancing, and genuine Firefox
  contention remain pending.
  An opt-in seventh fixture runs after real virtio-input initialization. AP1
  blocks in EL0 `read_key` and returns to its idle dispatcher; the focused QMP
  harness sends a genuine Ctrl-K through virtio-keyboard, CPU0 drains the used
  ring, and an SGI resumes AP1. Virtio-input MMIO and deferred compositor input
  work now have an exclusive CPU0 service wrapper; AP syscall/TTY paths record
  deferrals and the low-level driver fails closed on a non-owner call. Two
  repeated Pi/TCG passes report nonzero owner activity/AP deferrals and require
  matching input idle/resume masks `0x2`, status 61, exact frame balance, and
  boot completion. Full `make unit`, `make check`, normal release image/artifact
  checks, and the fresh visible login pass. Normal `boot/MAKOS.CFG` never arms
  this external-input wait.
  The same opt-in image first runs an eighth fixture. AP1 copies UDP transaction
  `0x4d4c` into an eight-slot/1,400-byte service queue; CPU0 alone mutates the
  virtio-net TX ring and completes it. AP1 then blocks in receive and returns to
  idle; CPU0 alone drains/demultiplexes RX and sends the wake SGI. Three Pi/TCG
  passes (two before and one after the final malformed-length guard) require
  nonzero AP requests/CPU0 TX completions, a validated DNS response, nonzero
  CPU0 RX frames/AP deferrals, matching I/O masks `0x2`, status 63, and exact
  frame balance. One intervening post-guard run passed the network fixture but
  missed the independently QMP-injected Ctrl-K; its immediate unchanged repeat
  passed the whole combined gate. The first RX attempt had exposed rejection of
  legal saved EL0 NZCV bits; only NZCV is now permitted. Copied UDPv4/v6 TX and
  RX ownership are qualified. Stateful TCPv4 is qualified by the separate gate
  below; the AP UDP completion wait is bounded EL1 `WFE`, not scheduler-idle
  proof. Full `make check`, `make unit`, release/image artifact checks, and the
  SMP structural guard pass for the UDP ownership work.
  The focused image runs a ninth fixture before networking. The kernel creates
  a private mode-0600 uid/gid-1000 inode and binds the immutable AP1 probe to
  the minimum file-write capability before login. AP1 uses normal VFS/MakFS4
  calls to write and `fsync` 4 KiB, close, reopen, read and byte-verify 4 KiB;
  the kernel removes the inode afterward. An eight-slot bounded copied-request
  queue hands every request to CPU0's production 100 Hz timer bottom half.
  Low-level ring submission fatals off CPU0; a timer interrupt over direct CPU0
  I/O observes the owner lock and defers one tick instead of recursing. The
  combined Pi/TCG pass reports 33/33 requests/completions: 18 reads, 10 writes,
  5 flushes, all 33 timer-serviced, status 65, content/inode cleanup, and exact
  frame balance. The marker is
  `MAKOS_AARCH64_SMP_BLOCK_OK requester_cpu=1 service_cpu=0 device=virtio-blk requests=read4k,write4k,fsync ring_activity=real mmio_owner=cpu0 transport=bounded-copy-queue service_point=cpu0-timer-bottom-half owner_completions=33 ap_requests=33 read_completions=18 write_completions=10 flush_completions=5 timer_completions=33 wait=bounded-el1-wfe status=65 file=/home/user/.smp-block-io lifecycle=create,write,fsync,reopen,read,verify,remove free_balance=1 scheduler_scope=opt-in-boot-probe desktop_gate=closed`.
  Two initial production-service runs used an unnecessary fixed five guest
  seconds of CPU0 evidence. The first reached the successful block marker but
  not the later input readiness marker; its exact repeat entered the block
  phase but exhausted the same unchanged 90-second TCG window before its
  marker. A one-guest-second window retains
  100 timer opportunities and the exact timer/owner equality requirement; its
  run passed block, network, injected input, and final boot. Full `make unit`
  and `make check`, release/image artifact checks, the updated structural
  guard, focused runtime, and ordinary visible login pass for core commit
  `0868b79`.
  A subsequent immutable AP1 graphics fixture receives only `CAP_GRAPHICS`
  and calls the normal surface create/fill/present ABI. Its retained compose is
  deferred to CPU0's production timer bottom half; low-level virtio-GPU
  control/cursor submission now fails closed off CPU0. The first focused
  Pi/TCG pass reports `owner_composes=1`, `ap_deferrals=1`, two real queue
  submissions, one transfer, one resource flush, status 67, surface reap, and
  exact frame balance. The same run then passes block with 22/22 requests
  (13 reads, 6 writes, 3 flushes), network DNS RX wake, injected Ctrl-K input,
  and final boot. Full `make unit check`, focused/normal image artifact checks,
  and the fresh visible login pass. GPU ownership is closed as a scheduler-row
  subitem; acceleration and broad graphics work remain Partial.
  The surface-present marker was then tightened to say
  `scanout=0 ... deferred=1` before the separate CPU0 completion marker. Two
  exact unchanged 90-second Pi/TCG repeats passed that final-source GPU proof
  (`1/1` compose/deferral and `2/1/1` submission/transfer/flush) but exhausted
  the combined harness window before input readiness under slower host
  throughput; neither emitted fatal/panic. Do not lengthen or weaken the
  harness. The prior same-behavior combined pass remains regression evidence.
- A separate `test.smp-tcp=required` image now qualifies stateful AP TCPv4.
  AP1 creates a real socket, connects through slirp to the host fixture at
  `10.0.2.2:18080`, sends exact `MAKOS_AP_TCP_TX\n`, blocks in receive, checks
  exact `MAKOS_CPU0_TCP_RX\n`, and closes with FIN. Connect and segment state
  crosses the bounded copied queue; CPU0 alone resolves the route and owns both
  virtio-net rings. The socket table is mutated under its live lock instead of
  whole-socket snapshot replacement. A 0.5-second host response delay forces
  AP1 through Blocked/idle and a CPU0 RX wake SGI. Final Pi/QEMU 10.0.11 TCG
  passes requester/owner statuses 69/70, exact bytes, one owner RX frame/AP
  deferral, four owner completions/four AP requests (connect, data, ACK, FIN),
  `io_idle_mask=0x2`/`io_resume_mask=0x2`, and exact frame balance. Marker:
  `MAKOS_AARCH64_SMP_TCP_RUNTIME_OK accel=tcg requester_cpu=1 service_cpu=0 protocol=tcp4 request=exact response=exact close=fin tx_mmio_owner=cpu0 rx_mmio_owner=cpu0 socket_state=locked-publication`.
  The opt-in SmpProbe additionally bypasses the last-runnable WFI shortcut so
  host vCPU ordering cannot evade the required scheduler-idle publication;
  production last-runnable behavior is unchanged. TCPv6 runtime and general
  desktop SMP remain open. Full `make unit check`,
  scheduler structural guard, and dedicated 90-second runtime gate pass.
- A new ordinary-boot fixture forces the same live EL0 TID from AP1 to AP2.
  Under the process lock the source captures GPR/SP/TLS/SIMD state, changes
  Running to Ready/unowned, publishes AP2 affinity, and relinquishes ownership;
  AP2 alone then resumes after the SVC. The first Pi/TCG run exited 71 with the
  same TID on both PEs but found an observer-ordering issue: AP2 resumed before
  the source incremented a later target-evidence counter. Publishing the
  intended target inside the locked transition fixes that proof race. The
  unchanged repeat passes source/target masks `0x2`/`0x4`, one migration,
  exclusive ownership, preserved GPR/SP/TLS/SIMD, status 71 and frame balance.
  Marker: `MAKOS_AARCH64_SMP_MIGRATION_RUNTIME_OK accel=tcg tid=same source_cpu=1 target_cpu=2 ownership=exclusive context=gpr,sp,tls,simd`.
  Reproducer: `make test-aarch64-smp-migration-runtime`. This closes the
  bounded forced-migration case only.
- The ordinary boot now follows migration with six immutable load tasks on the
  shared Ready queue. Each executes 48 real yields; AP selectors check exactly
  one CPU owner on every recorded dispatch. Focused Pi/QEMU 10.0.11 TCG passes
  all statuses 80-85, CPU mask `0xe`, exact reap/frame balance, 288 contention
  yields, and even `99,99,99` dispatch counters (297 total). The fresh visible
  boot independently records `99,100,98`, confirming bounded scheduling skew.
  Qualification exposed and fixed the real wake path: session liveness retains
  Ready/Blocked tasks, Ready publication sends the scheduler SGI, AP idle
  acknowledges IRQs around `WFI`, and CPU0 keeps its scheduler timer armed
  during the bounded AP-deadline wait. Marker:
  `MAKOS_AARCH64_SMP_LOAD_RUNTIME_OK accel=tcg tasks=6 worker_cpus=3 cpu_mask=0xe run_queue=shared-ready selection=per-cpu-round-robin ownership=exclusive`.
  Reproducer: `make test-aarch64-smp-load-runtime`. Full `make unit check`, the
  structural guard, release image/artifact build, focused runtime, and visible
  login pass. Explicit per-thread affinity and forced migration now pass as
  described below; automatic load-driven balancing, additional built-in/service
  roles and priorities, and real Firefox/desktop contention remain open.
- The desktop now opens a bounded production scheduler policy after driver and
  login-UI initialization. Leaders plus shell, UI, service, and all device MMIO
  stay on CPU0; non-leader Firefox and ordinary Native application threads are
  AP-eligible. AP sleep, I/O,
  input, IPC, and futex no-successor cases publish Blocked/unowned state and
  return to WFI. Firefox requests three TaskController workers. A focused
  upstream-musl pthread fixture runs under that exact role. Its production-only
  three-worker rendezvous proves a simultaneous distinct-TID interval and the
  final Pi/QEMU 10.0.11 TCG pass reports AP1-3 dispatch, `cpu_mask=0xe`,
  `overlap_mask=0xa`, live TIDs 6/5 on AP1/AP3, exclusive ownership, dispatch
  counters `9787,11024,9493`, and status 42. Reproducer:
  `make test-aarch64-production-smp-runtime`. This is not real Firefox evidence.
  Strict `make test-aarch64-firefox-runtime` now requires equivalent live
  overlap from the launched Firefox group while retaining every existing
  latency/interaction/resource threshold. The first repeat exposed stale TID
  telemetry across an in-exception yield; every scheduler selection now
  refreshes the AP owner. The next repeat exposed byte-spliced PL011 lines;
  AArch64 formatted/raw serial records now use an IRQ-masked cross-PE lock.
  Qualification also exposed an IRQ window in the EL1-to-EL0 restore
  trampoline; EL1 stays masked until target SPSR takes effect at `ERET`.
  Subsequent 297-selection load gates and focused production runtime pass.
  Final `make unit check`, release image/artifact checks, structural guards,
  and visible login pass. Two broad Pi/TCG attempts continued deep into the UI
  suite but retained the known exact Settings resize mismatch (`560x360`
  observed versus `450x290` required), so no full broad-gate pass is claimed.
- 2026-08-25 AArch64 normative syscall 57 startup-vector parity is implemented.
  The exact 336-byte version-1 descriptor is copied and validated before child
  allocation. The guest-native two-pass assembler emits code that validates
  syscall-56 `argc=1` and syscall-57 `argc=3`, `argv[1]`, and `envp[0]`; Pi
  QEMU 10.0.11 TCG passes both status-42 executions plus three malformed-form
  denials. Full `make unit`, `make check`, release image/artifact checks and
  structural guards pass. The broad Pi/TCG harness later hit the preserved
  Settings resize mismatch (`560x360` versus exact `450x290`), so it is not a
  full broad-gate pass.
- 2026-08-26 the guest-native AArch64 toolchain reads a versioned
  `/home/user/generated.build` plus four source files from MakFS and builds
  four persisted ELF64 `ET_REL` objects. `MAKBUILD1` accepts two through six
  inputs: one leading `asm` and one through five `c` records, with absolute
  source/object path pairs, absolute final
  output, and entry symbol. Parsed values drive every source read, object
  write/reopen, linker entry, and final write. Bad version, relative path,
  path collision, and missing-link manifests fail closed. The authenticated
  `makbuild <manifest>` command passes a kernel-validated `/home/user/` path in
  the EL0 toolchain's child-owned SysV `argv[1]`. Its `MODE=build` consumes the
  already-persisted MakFS manifest and sources without seeding or overwriting
  them; `selfhost-aarch64` alone selects `MODE=fixture`. Focused Pi/TCG runtime
  runs the fixture once and eight authenticated CLI builds across distinct
  four- and three-input manifests, reaping every toolchain process with status
  42. A derived 120-byte `MAKSTATE2` record is
  committed after object and final-ELF writes. Its non-cryptographic 64-bit
  FNV-1a manifest/source/object fingerprints are cache keys only; reuse also
  requires the object to pass ELF parsing and symbol validation. Runtime proves
  four-input cold `0/4`, warm `4/0`, corrupt-object `3/1`, rewarm `4/0`,
  edited-source `3/1`, rewarm `4/0`, and corrupt-state full `0/4` hit/miss
  results, plus three-input cold `0/3` and warm `3/0`. The
  assembler emits 76 bytes of
  `_start` code in a 688-byte object. The program C translation unit contains
  the 140-byte `answer` and 168-byte `adjust(int *pointer, int delta)` in a
  308-byte `.text` and 976-byte object. A separate library C translation unit
  defines the 60-byte `combine(int value, int delta)` in a 616-byte object. An
  independent fourth C unit defines 56-byte `helper(int value)` in a 608-byte
  object; direct RX execution proves `helper(40)=42`.
  Across the C units, code generation includes a 96-byte
  AAPCS64 non-leaf frame, mutable parameter/local assignments,
  signed equality/inequality and `<`/`<=`/`>`/`>=` comparisons, a signed backward-branch assignment-only
  `while`, bounded `int *pointer = &local`, fixed local `int` arrays with exact
  initializers, checked constant indexing, array decay, bounded constant or
  scalar-variable element pointer addition, plus up to three independently typed `int`/`int *`
  parameters and call arguments. AAPCS64 carries them in `x0` through `x2`
  (`w0` through `w2` for integers) and preserves them in x23 through x25. The
  x25 stack save/restore is emitted only for three-parameter functions, so
  existing one- and two-parameter code sizes remain exact. Unary `+`/`-` and
  precedence-correct signed `/`/`%` now join multiplication/addition/subtraction;
  division emits 32-bit `SDIV`, remainder uses `SDIV` plus `MSUB`, and direct
  literal-zero divisors fail closed. Dynamic pointer addition emits
  `ADD ... SXTW #2`, so a signed negative offset is preserved. Known local
  bounds reject one-past-end constants and unproved variable offsets. The real
  `answer`→`adjust(values + 1, 1)` call, `next = pointer + delta`, and
  `*(pointer + delta)` store mutate caller-owned array elements using the
  second parameter as delta. `adjust` also computes `distance = next - pointer`
  with 64-bit `SUB` and arithmetic shift-right two; direct RX probes prove
  signed element results `3` and `-3`, while pointer-minus-scalar fails closed.
  `adjust` obtains its updated pointee through external `combine`, creating a
  real C-object-to-C-object call and relocation. A fourth function fails closed; the bounded
  object buffer is now 2 KiB and the persisted source buffer is 768 bytes.
  The bounded
  linker discovers definitions/undefined symbols across all four primary
  fixture objects, applies external
  `_start`→`answer`, same-object `answer`→`adjust`, and external
  `adjust`→`combine` `R_AARCH64_CALL26`
  relocations, includes the independent 56-byte `helper` definition from its
  608-byte object, and emits 500 linked bytes in an 815-byte `ET_EXEC`. Direct
  RX execution proves `helper(40)=42`. A separate `sum3`/`invoke3` translation
  unit emits 140 code bytes in a 752-byte ELF64 `ET_REL`, resolves a same-object
  `R_AARCH64_CALL26` with entry offset 80, and executes both
  `sum3(40,1,1)` and `invoke3(40)` as 42 from RX memory. A separate
  three-definition arithmetic unit emits 168 code bytes in a parsed 784-byte
  `ET_REL` and executes positive/negative division `6`/`-6`, remainder
  `2`/`-2`, and negation `-42`/`42` from RX memory. The fully linked C graph
  executes `answer(20)=42`, `answer(0)=86`, `adjust(forty,1)=42`,
  `adjust(scaled,2)=44`, and `adjust(zero,1)=2`; direct-call arrays must become
  `41:42:0`/`42:0:44`/`1:2:0`; separate RX functions exercise all four signed
  relations and prove `pointer + -1` loads 42, and the
  normal loader executes the final ELF twice with status 42. The adjust
  outcomes require real stack address formation and dereference memory writes.
  Unsupported bitwise syntax, direct literal-zero division/remainder, duplicate
  parameter names, more than three parameters or
  call arguments, missing all-path return, undefined-variable assignment/address target, pointer reassignment,
  pointer/address-as-`int` return, a known two-element array indexed at two, a known
  two-element array advanced by two or an unproved variable amount,
  pointer-minus-scalar, duplicate or four functions in one translation unit,
  an out-of-range BL site, relocation type 282, a nonzero CALL26 addend,
  unresolved `adjust`, an omitted library object, and duplicate `answer` fail closed. Exact source
  passes release artifact checks, `make unit check`, structural guard, focused
  Pi/QEMU 10.0.11 TCG runtime.
  Reproducer: `make test-aarch64-selfhost-runtime`. The audit rows remain
  Partial: this is not a full C/Rust compiler/linker, transitive dependency/
  header engine, an arbitrary graph beyond six inputs, a parallel build
  system, debugger, or substantial
  in-guest MakOS build.
- At this handoff PID 699985 is the sole QEMU and no runtime-test harness is
  active. Check process state before every runtime gate and stop the visible
  guest through its recorded QMP socket; never start concurrent QEMU.
- Kernel-owned per-thread affinity is now target syscall 148/feature bit 22.
  Same-thread-group get/set validates online masks; a caller excluding its
  current PE crosses a real scheduling boundary. Official-musl patch 65 maps
  Linux AArch64 `sched_getaffinity`/`sched_setaffinity`. The final focused
  Pi/TCG fixture passes forced singleton migrations for all three Firefox-role
  workers, restored `0xe` masks, joins, typed IPC and input-priority handling:
  `cpu_mask=0xe`, dispatches `9867,11100,9833`, `overlap_mask=0x6`, TIDs 5/6,
  watcher 8/AP2, status 42. Full `make unit check` passes. This is functional
  Pi/TCG evidence only; unchanged strict Firefox Gate 3 remains first priority
  on an idle macOS/HVF host.
- Shared-Ready dispatch, stateful TCP owner service and CPU0-owned device
  services remain the affinity milestone's foundations. Generated
  `build/`, `target/`, nested targets, `outputs/`, logs, QEMU variable stores,
  Python caches, and `.DS_Store` are intentionally ignored rather than uploaded.
- Cursor uses virtio-GPU hardware cursor plane. Marker:
  `cursor=virtio-gpu-plane move=cursorq scanout_damage=none host-cursor=hidden`
- Focused cursor runtime harness: `scripts/boot_test_aarch64_cursor.py`
- Make target: `test-aarch64-cursor-runtime`
- Focused cursor runtime passes on the interrupt-driven input image: seven QMP
  positions, zero changed scanout pixels, virtio-GPU cursor plane, and hidden
  host cursor (`accel=tcg` on this Pi). The 100 Hz input poll remains only as
  the safe recovery path described above.
- Fresh 2026-08-25 cursor rerun passes:
  `MAKOS_AARCH64_CURSOR_RUNTIME_OK accel=hvf positions=7 changed_scanout_pixels=0 backend=virtio-gpu-plane host_cursor=hidden`.
- Clean integrated data image:
  `build/makos-integrated-a9c604254f094de2.img`
- Image SHA-256:
  `a9c604254f094de24ed2668da74cbcd48f48ae0f111e8b182a7b3dedfeda2824`
- Image has Firefox ESR 140.13, nano 9.1, ncurses 6.5, CPython 3.14.7. Integrated image verification passed.
- Firefox strict Gate 3 baseline: first paint 388702 ms; input `h` 104 ms; first Ctrl-A 8130 ms; central body 138000 changed pixels/324 colors.
- Firefox scheduler work reduces modeled serial traffic by 1,374 Gate 3 lines, polls input from the 100 Hz timer bottom half, limits selected worker pools for BSP-only userspace, supplies FIFO futex requeue, preserves per-CPU round-robin cursors, and retains bounded watcher priority. Historical package `4dcdfcc16c362584` passed three consecutive strict Gate 3 runs; current verified package and interaction metrics are recorded below. Runtime handoff marker remains `MAKOS_AARCH64_SURFACE_MAIN_HANDOFF_OK tid=4 source=watcher-dequeue-fallback bounded_ticks=1000`.
- Prior identical-package probes failed to paint within 240 seconds and were manually stopped after 348+ seconds; another strict run painted but missed Ctrl-A at 18,995 ms before active-watcher retention. Three consecutive final runs now satisfy every unchanged strict threshold. A first strict Wikipedia attempt painted in 147182 ms and passed input/TLS but never reached `PAGE_STOP`: TCP advertised 32 KiB while storing only 4 KiB inline. RX storage now uses a bounded external 128 x 32 KiB pool, avoiding burst loss and large `Socket` stack copies. TCP connections now advertise actual free capacity, issue duplicate zero-window ACKs on saturation without sequence advance, and send reopening ACKs after userspace drain. Packet/structural checks and full AArch64 boot pass. Latest strict Wikipedia rerun paints in 178412 ms, handles Ctrl-A in 7479 ms, accepts first character in 101 ms, and passes exact `https://www.wikipedia.org/`, built-in-root TLS, changed pixels, and 271-second survival. Broad modern-site coverage remains open.
- Firefox system-clipboard interaction is now strict-runtime tested. After real `example.com` HTTPS/page-pixel completion, harness selects/copies URL, clears field, pastes through MakOS clipboard, presses Enter, and requires a second exact-URI page stop plus raw shortcut sequence. Pass: paint 177985 ms, Ctrl-A 7201 ms, first character 109 ms, survival 258 seconds. `test-aarch64-firefox-runtime` includes opt-in clipboard proof by default.
- Firefox real document mouse navigation now passes same strict gate. After clipboard reload, harness left-clicks rendered `example.com` link, requires MakOS pointer hit-testing on Firefox surface 5, exact `https://www.iana.org/help/example-domains`, built-in-root TLS, and changed pixels. Pass: paint 169164 ms, Ctrl-A 7435 ms, first character 110 ms, clicked navigation 36993 ms, survival 292 seconds. `test-aarch64-firefox-runtime` enables this proof by default.
- Package `1b55d512904b8c2a` contains Firefox patch `0054` for held mouse-button dispatch plus patch `0055` and kernel raw key 136 for Ctrl-L. Strict runtime now drags over rendered IANA document text, proves changed selection pixels, copies through MakOS clipboard, selects the URL bar through Ctrl-L, composes exact `https://example.com`, reloads with built-in-root TLS/HTTP 200, and preserves the exact key sequence. Latest pass: paint 164388 ms, Ctrl-A 9373 ms, first character 110 ms, mouse link 25496 ms, document selection 34400 ms, survival 329 seconds. `test-aarch64-firefox-runtime` enables this proof by default.
- Package `a9c604254f094de2` adds Firefox patch `0056`, defining MakOS wheel events as line-mode deltas instead of the invalid fallback `deltaMode=-1`. Sustained Gate 3 proves down/up wheel dispatch with 65,599 changed pixels and recovery to 13,270 differing pixels, types `makos42` into the real httpbin Customer field, pointer-selects/copies it, composes exact `https://example.com/?customer=makos42`, and completes two cycles/four repeated top-level `example.com`/IANA navigations. Final Make-target pass: paint 169697 ms, Ctrl-A 5727 ms, first character 111 ms, host CPU ratio 1.053, host RSS 325140480 bytes, guest Firefox resident pages 54254, survival 531 seconds.
- AArch64 `ps` now reports actual mapped user resident pages/KiB by scanning user L3 descriptors. `test-aarch64-firefox-runtime` enables sustained interaction with two cycles by default. Full post-change `make unit && make check`, binary/package verification, and focused cursor runtime pass. Firefox packaging unconditionally refreshes Mozilla `stage-package`. Make/package defaults use verified `a9c604254f094de2`.
- Latest 2026-08-25 Firefox reruns reached verified browser paint in 248584 ms and
  255543 ms but failed unchanged Ctrl-A limit at 10971 ms and 14363 ms versus
  10000 ms. Host evidence at failure: load average 7.66, 163 MiB free RAM,
  6.6 GiB compressed, with Zen/WindowServer consuming multiple cores. Prior
  current-package strict pass remains valid evidence, but latest Gate 3 is not
  green. Do not relax thresholds. Rerun unchanged only after host pressure clears.
- 2026-08-26 Pi/QEMU 10.0.11 TCG production-SMP runtime now proves the real
  Firefox-role input wake path deterministically. The upstream-musl fixture
  owns overflow surface 7, blocks watcher TID 8 in syscall 140, receives QMP
  Ctrl-A, dispatches that TID on AP2, then dispatches the group leader once on
  CPU0. Priority hints are role-affine and one-shot after selection; the
  1,000-tick deadline only expires stale hints. This also fixed CPU0-leader
  starvation of a freshly forked Firefox child during the fixture's typed-IPC
  phase. The latest fixture also uses real musl affinity calls to verify the
  CPU0 leader mask, force three worker migrations through singleton AP masks,
  read them back from kernel state, and restore mask `0xe` before teardown.
  Final evidence is `cpu_mask=0xe`, dispatches `9867,11100,9833`,
  `overlap_mask=0x6`, TIDs 5/6, watcher 8/AP2, status 42. Full `make unit check`,
  structural guards, release image and artifact checks pass. This is Pi/TCG
  functional evidence only; it does not replace the unchanged idle-macOS/HVF
  strict Firefox gate.
- 2026-08-26 Pi/QEMU 10.0.11 TCG production SMP also qualifies ordinary
  Native application pthreads after desktop startup. A dedicated `native-smp`
  exact-role process runs all three APs (`cpu_mask=0xe`, dispatches
  `11119,10254,11130`), forces/reads/restores kernel affinity, records a live
  simultaneous AP1/AP3 interval (`overlap_mask=0xa`, TIDs 5/6), joins and
  reaps status 42. Role/group-scoped accounting prevents it from satisfying
  Firefox evidence. The unchanged Firefox production regression, combined
  network/input-IRQ runtime, cursor runtime, full `make unit check`, and
  release/image artifact validation remain green. Automatic balancing and
  additional built-in/service roles remain Partial.
- 2026-08-26 the surface input wait class is now exact-handle rather than a
  global surface-watcher wake. Scheduler snapshots carry TID, process-group
  owner PID, and handle; graphics readiness is checked outside the scheduler
  lock. Queued data wakes only its owning handle, while destroyed handles wake
  so retry fails closed and teardown joins cannot strand. Firefox priority
  uses the same queued-handle selection. The upstream-musl production fixture
  creates target surface 7 and decoy surface 8, blocks both pthreads, and waits
  for both exact-handle registrations before QMP Ctrl-A. Final Pi/TCG runtime
  selects handle 7/TID 8 on AP2, reports `surface_woken=1` and
  `surface_skipped=3`, leaves the decoy blocked until surface 8 destruction,
  hands off once to the CPU0 leader, and completes the unchanged timed-futex
  bounds plus status-42 reap. Dispatches are `9954,9924,11186`, with
  `cpu_mask=0xe` and live/final `overlap_mask=0x6` for TIDs 5/6. Release
  artifact checks, structural guard, focused runtime, and full
  `make unit check` pass. This does not qualify strict Firefox timing.
- 2026-08-26 AArch64 keyboard/tablet delivery is now genuinely
  interrupt-driven on QEMU `virt`. MakOS derives each virtio-mmio GIC INTID as
  `48 + slot`, configures the shared SPI bank as Group 1/edge-rising, targets
  it only at CPU0, and drains input immediately when the exception interrupted
  EL0. An IRQ that interrupted EL1 acknowledges transport status without
  recursively taking syscall locks; the unchanged 100 Hz owner poll remains a
  recovery drain. The focused production Firefox-role runtime observes QMP
  Ctrl-A on keyboard INTID 78 through `entry=lower-el dispatch=direct`, selects
  exact surface 7/TID 8 on AP1, wakes one surface waiter while skipping three,
  runs all AP workers (`cpu_mask=0xe`, dispatches `9557,10824,9509`), records
  simultaneous TIDs 5/6 on AP1/AP3 (`overlap_mask=0xa`), and reaps status 42.
  The release image/artifact check, structural guards, and full
  `make unit check` pass. This is Pi/TCG functional evidence, not a substitute
  for the unchanged idle-macOS/HVF strict Firefox gate.
- 2026-08-26 AArch64 virtio-net RX is now genuinely interrupt-driven on QEMU
  `virt`. Slot 28 derives GICv2 SPI INTID 76 and shares the CPU0-only Group 1,
  edge-rising registration used by input. Lower-EL entry runs the bounded
  network bottom half directly; EL1 entry acknowledges/defer safely because
  socket locks are non-recursive, and the 100 Hz CPU0 pump remains recovery.
  The focused Pi/QEMU 10.0.11 TCG gate sends a real AP1 DNS request, blocks AP1
  in receive, keeps CPU0 in a syscall-free EL0 loop, and records
  `MAKOS_AARCH64_NETWORK_IRQ_OK intid=76 cpu=0 entry=lower-el dispatch=direct source=virtio-mmio frames=1 timer_fallback=100hz`.
  Its final network marker has `owner_frames=1`, `ap_deferrals=2`,
  `irq_frames=1`, `irq_el1_deferrals=0`, exact idle/resume mask `0x2`, status
  63, and frame balance; the subsequent real Ctrl-K input and boot gates pass.
  The production Firefox-role regression passes with dispatches
  `10538,9670,9665`, simultaneous AP1/AP3 TIDs 5/6, watcher TID 8 on AP1, and
  status 42. Cursor runtime again passes seven positions and zero changed
  scanout pixels. Full `make unit check`, AArch64 compile/release/artifact, and
  structural guards pass. The strict integrated Firefox image remains absent
  on this Pi; this result does not qualify macOS/HVF timing.
- The Raspberry Pi was idle when the unchanged Firefox Make target was retried,
  but preflight stopped before QEMU launch: this host does not contain exact
  `build/makos-integrated-a9c604254f094de2.img` or the staged Firefox package,
  and the GitHub repository has no release asset from which to recover it. This
  is an artifact-prerequisite block, not a guest runtime or latency failure.
  Firefox remains first priority on the intended idle macOS/HVF host.
- AArch64 installer now uses shared `makos-installer` fresh/resume core through a virtio-blk adapter. Exact `install disk1 resume-disk1` accepts only blank MBR plus zero/source-identical partial sectors; committed or conflicting media fail closed. Source snapshot begins with a serialized flush/write-freeze; all disk0 writes are denied until error thaw or successful shutdown while disk1 remains writable. Full HVF gate guest-tests both resume refusals, hard-kills QEMU after first progress, proves LBA0 blank plus two source-identical partial blocks, resumes to exact SHA-256 equality, detaches live source, and passes two installed-only persistence boots. Marker: `MAKOS_AARCH64_INSTALL_BOOT_OK ... conflict_resume_refusal=1 ... power_interrupt=pre-mbr ... partial_blocks=2 resume=1 source_digest_match=1 ...`.
- Final post-freeze `make unit && make check`, AArch64 release/image artifact build, and current visible login boot pass. Active clone is recorded above.
- Sparse anonymous VM decommit now has fresh upstream-musl guest proof. Probe writes pages, calls `MADV_DONTNEED` and MakOS immediate-decommit `MADV_FREE`, verifies zero refault after each, then unmaps. Fresh 1,350-object static musl build, structural guard, release embed, and full AArch64 runtime pass.
- Focused IPv6 runtime gate ordering now checks userspace markers after login/musl probe. Full guest passes validated RA/SLAAC EUI-64, native AF_INET6 sockaddr28, NDP resolution, and checksum-valid UDPv6 transmit. `test-aarch64-ipv6-runtime` reproduces it; UDPv6 receive and TCPv6 runtime remain open under current QEMU usernet backend.
- Security-audit persistence content proof passes full two-boot HVF. `kernel/src/log.rs` summarizes only severity-4 prior-boot records before early-log merge; second boot proves two records, authentication accepted plus account created, both PID-attributed. Structural guard, release build, and full guest workload pass.
- 2026-08-24 continuation: full `make unit`, AArch64 kernel check, updated AArch64 release/image build, cursor runtime, and five-boot package runtime pass. Workspace-wide fmt check still reports many preserved pre-existing formatting diffs; no bulk reformat was applied.
- Firefox Gate 3 rejects `commonDialog.xhtml` as first browser paint, creates `/home/user/firefox-profile` before process allocation, normalizes MakFS4 home-root ownership to uid/gid 1000 mode 0700, and gives existing virtual non-symlink parents `readlinkat` EINVAL. Reproducible target: `test-aarch64-firefox-runtime`; latest-source strict pass completed 2026-08-24. Diagnostic `ps` now runs only when real browser chrome is absent, so it cannot obscure successful final screenshot proof.
- AArch64 EL0 package probes now pass a five-boot signed install/replace/query/remove/rollback, live `/packages/hello/payload`, reboot-persistence, open-FD generation-pin, and corrupt-newest fallback gate. Desktop `cat` package reads were fixed by adding immutable package/system backing to `vfs::snapshot`; live refresh now emits an explicit result marker.
- Official-musl patches 62-64 pass the full two-boot HVF runtime through extended stat/fstat, symlink/timestamps, exact-TID signals, timed futex, robust owner-death, SCM_RIGHTS, shmem, scalable-directory create, second-boot remount verification, and cleanup. Extended fstat handles stdio TTY FDs; `SYS_YIELD` delivers pending task signals before handoff. The persistence gate uses `MAKOS_AARCH64_SKIP_BROWSER_FETCH=1` to isolate it from external DNS/HTTP; Firefox strict Gate 3 separately proves native DNS/TCP/TLS/HTTP.
- Existing relative futex deadline/timer-expiry path has real musl guest proof: worker holds a mutex while `pthread_mutex_timedlock` returns `ETIMEDOUT` within 50 ms..1 s, then cleanly joins.
- MakFS4 current source replaces linear child-name cache scans with a tested 1,024-bucket collision-chained index over 512 authoritative inode records; directory FDs retain resumable raw-inode cursors and 255-byte components. The passing two-boot musl probe creates 64 siblings plus one name255 entry, validates complete `readdir`/random lookup after remount, then cleans up. Forced-collision unit test, clean 1,350-object musl build/link, full `make unit`, dual-arch kernel checks, AArch64 release build, and guest execution pass.
- Existing AF_UNIX stream `socketpair`/`SCM_RIGHTS` path has a passing strict musl runtime probe for stream-byte association and queued open-description lifetime: sender closes original FD before receiver reads transferred file payload.
- Read-only host `makos-makfs4-fsck` accepts raw or validated redundant-GPT MakOS data volumes and checks filesystem roots, metadata-set geometry, catalog/inode CRC/count/identity, parent graph/cycles, duplicate child names, extent bounds/overlap, and bitmap agreement. Six sparse tests pass, including GPT offset, redundant-root fallback, and corruption rejection. New `test-makfs4-guest-fsck` runs the full two-boot HVF workload, exports its data image only after QEMU closes, then passes fsck at generation 257/root slot 1 with 5 inodes and 4 allocated blocks before temporary cleanup. Repair mode remains pending.
- AArch64 implements structured-log syscalls 28/29 over the shared 32-record ring. Parent EL0 source validates append/read payload, sequence, monotonic timestamp, PID, and severity; ABI discovery includes log bit 7 and package bit 16. `MAKLOG01` stores a whole-image-CRC snapshot at `/.makos-system-log` through MakFS4 COW, loads prior records, and merges records emitted before storage mount. Three strict codec tests, structural guards, both kernel checks, AArch64 release build, and fresh two-boot guest merge markers pass.
- Structured-log reads require `CAP_CONSOLE`; real Browser sandbox runtime proves denial with output/metadata buffers untouched. First-boot persistence and second-boot merge pass.
- MakOS clang now adds `-fstack-protector-strong` by default for C/C++ target builds while preserving explicit musl-bootstrap opt-out. Toolchain gate proves protected/unprotected object symbol behavior; musl exports guard/failure runtime and SysV startup supplies RNG-backed `AT_RANDOM`. Rebuilt deployed musl CRT probe performs a real 32-byte overwrite of a protected 16-byte stack buffer. Full two-boot HVF runtime proves `__stack_chk_fail`, lower-EL data-abort containment as process-group status 139, parent wait/reap, shell survival, and continued guest tests. Broader deployed-app rebuild remains pending.
- x86 installer supports ATA disk0/disk1, exact admin confirmation, blank install, source-matching resume, and MBR-last commit. Six host tests pass. QEMU runtime uses a prepared source plus per-boot qcow2 overlays, SIGKILLs after the first verified 4 KiB payload block, proves LBA0 blank plus every nonzero partial block source-identical, resumes through `resume-disk1`, verifies final SHA-256 equality, detaches source, and passes two installed-only persistence boots. Marker: `MAKOS_X86_INSTALL_BOOT_OK ... power_interrupt=pre-mbr mbr_blank_after_interrupt=1 partial_blocks=1 resume=1 source_digest_match=1 ...`.
- Package manager has disk-backed A/B store, signed `MAKDEP1` dependency metadata, graph checks, live read-only `/packages/<name>/payload`, install/remove/rollback refresh, Settings status, AArch64/x86 syscalls and SDK wrappers. Host tests, kernel checks, and five-boot guest fault-injection runtime pass.
- Queued typed native IPC is implemented on both syscall paths while preserving
  legacy scalar channels/events. Versioned 64-byte messages stamp kernel-owned
  sender PID/UID, bounded FIFO channels atomically transfer generation-tagged
  channel handles with attenuated rights, and unreachable queued-transfer cycles
  are collected. Service routes require `CAP_IPC`, publishing/accept additionally
  require `CAP_SERVICE_PUBLISH`, and routes are limited to matching UID/session.
  Process exit closes routes/handles before reap. Fresh evidence: 12/12 IPC unit
  tests, `test_aarch64_typed_ipc.py`, full `make unit && make check`, and isolated
  full HVF boot marker `MAKOS_AARCH64_TYPED_IPC_RUNTIME_OK service=same-domain fifo=1 transfer=attenuated cleanup=process-exit-before-reap`.

## Important files

- `docs/ORIGINAL-SPEC-AUDIT.md` — source of truth for spec coverage.
- `docs/STATUS.md` — current project status.
- `docs/BUILD.md` — build/boot instructions.
- `scripts/boot_test_aarch64_cursor.py` — cursor corruption runtime gate.
- `scripts/test_aarch64_firefox_trace_budget.py` — Firefox trace budget regression.
- `kernel/src/aarch64_process.rs` and `kernel/src/arch/aarch64.rs` — AArch64 process/display/input work.

## Next actions

1. On the intended macOS/HVF host, when no visible QEMU runs and host
   load/memory pressure is low, rerun unchanged
   `make test-aarch64-firefox-runtime`; diagnose code only if strict Ctrl-A
   still exceeds 10000 ms under an idle host. Never weaken Gate 3 thresholds
   or substitute Pi/TCG timing evidence.
2. The strict target now requires overlapping distinct Firefox TIDs on multiple
   guest CPUs; inspect that evidence in the next genuine macOS/HVF run. Then continue
   automatic load balancing and repeated migration contention while retaining
   CPU0-exclusive device ownership. Stop the visible QEMU through QMP before
   any focused runtime.
3. Expand the bounded guest C compiler beyond its current three-function per
   translation-unit and two-parameter limits. The primary runtime graph now
   spans four objects (with one same-object call, two cross-object calls, and
   one independent helper) and the build driver accepts two through six
   inputs. Continue beyond signed typed-pointer arithmetic into
   provenance-aware/broader pointer and lvalue expressions,
   variable-length/global/multidimensional arrays, structs and nested/general
   blocks, then lift the function/parameter bounds, add broader relocation/
   object support, transitive dependency/header discovery, arbitrary/parallel
   input graphs beyond the six-input bound, and broader command-line build
   control before a substantial
   in-guest build. Preserve real implementation
   requirements—no fake/spoofed apps.

## Operating constraints

- Use `apply_patch` for source edits.
- Prefer `rg`/`rg --files` for search.
- Avoid destructive git/filesystem commands.
- Keep user informed during long builds; avoid >60 s silent work.
- Heavy builds currently allowed.
- Do not overwrite user changes or old images unexpectedly.
