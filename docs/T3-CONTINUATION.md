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

- Active visible Pi/QEMU 10.0.11 TCG pointer-capable self-host milestone:
  PID 365270, user service `makos-visible-selfhost-pointer.service`, VNC
  `127.0.0.1:5901`, session
  `build/makos-pi-visible-selfhost-pointer-VRJSOOUV`, private boot clone
  `build/makos-pi-visible-selfhost-pointer-VRJSOOUV/boot.img`, private data clone
  `build/makos-pi-visible-selfhost-pointer-VRJSOOUV/data.img`, private variables
  `build/makos-pi-visible-selfhost-pointer-VRJSOOUV/vars.fd`, QMP
  `build/makos-pi-visible-selfhost-pointer-VRJSOOUV/qmp.sock`, serial
  `build/makos-pi-visible-selfhost-pointer-VRJSOOUV/serial.log`, PID file
  `build/makos-pi-visible-selfhost-pointer-VRJSOOUV/qemu.pid`, and QMP framebuffer
  captures `login.ppm`/`login.png`. Its boot
  clone SHA-256 is
  `1de63df19c3f40740f089b1f04ed50b6a859a3fba51ca18149bb4c2f51095563`,
  exactly matching `build/makos-aarch64.img`. It is the sole QEMU
  process and the ordinary config reports `smp_input_probe=0`,
  `smp_tcp_probe=0`, four online PEs,
  initial boot-probe `userspace_scheduler_cpus=1`, post-desktop
  `userspace_scheduler_cpus=4` under the bounded Firefox-worker policy,
  `MAKOS_LOGIN_UI_OK`, and `MAKOS_AARCH64_BOOT_OK`, plus shared-queue load
  counters `100,102,98`, with no fatal/panic. The 800x600 capture was visually
  inspected and shows the native login with username focus. VNC required QEMU's bundled
  data path via `-L build/host-tools/qemu-root/usr/share/qemu`. Keep it running
  for user testing; the framebuffer capture visibly shows the native login
  dialog. Use QMP `quit` before any later runtime gate.
  Prior PID 358340/session `build/makos-pi-visible-selfhost-loop-Zlx4NpuL`
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
  admits only Firefox workers; general roles, automatic load balancing, and
  genuine Firefox contention remain pending.
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
  login pass. Production priorities/affinity, automatic migration and real
  Firefox/desktop contention remain open.
- The desktop now opens a bounded production scheduler policy after driver and
  login-UI initialization. Leaders, non-Firefox roles, and all device MMIO stay
  on CPU0; only non-leader Firefox-role threads are AP-eligible. AP sleep, I/O,
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
- 2026-08-25 the guest-native AArch64 toolchain now builds three source files
  into three persisted ELF64 `ET_REL` objects. The assembler emits 76 bytes of
  `_start` code in a 688-byte object. The bounded C compiler emits a 120-byte
  `answer` and 152-byte `adjust` in 728/704-byte objects, including a 96-byte
  AAPCS64 non-leaf frame, mutable parameter/local assignments,
  equality/inequality comparisons, a signed backward-branch assignment-only
  `while`, bounded `int *pointer = &local`, real dereference loads/stores, and
  a real `answer`→`adjust` call. The bounded
  linker discovers definitions/undefined symbols across all three, applies
  `_start`→`answer` and `answer`→`adjust` `R_AARCH64_CALL26` relocations, and
  emits 348 linked bytes in an 815-byte `ET_EXEC`. The fully linked C graph
  executes `answer(20)=42`, `answer(0)=86`, `adjust(40)=42`, and
  `adjust(0)=2`; the normal loader executes the final ELF twice with status 42.
  The adjust outcomes require real stack address formation, dereference memory
  writes and an address-taken reload. Unsupported division, missing all-path
  return, undefined-variable assignment/address target, pointer reassignment,
  an out-of-range BL site, relocation type 282, a nonzero CALL26 addend,
  unresolved `adjust`, and duplicate `answer` fail closed. Exact source
  passes release artifact checks, `make unit check`, structural guard, and
  focused Pi/QEMU 10.0.11 TCG runtime.
  Reproducer: `make test-aarch64-selfhost-runtime`. The audit rows remain
  Partial: this is not a full C/Rust compiler, general linker/build system/
  debugger, or substantial in-guest MakOS build.
- At this handoff PID 365270 is the sole QEMU and no runtime-test harness is
  active. Check process state before every runtime gate and stop the visible
  guest through its recorded QMP socket; never start concurrent QEMU.
- The shared-Ready-queue milestone is the current implementation state; forced
  migration, stateful TCP owner service and CPU0-owned device services remain
  its foundations. Generated
  `build/`, `target/`, nested targets, `outputs/`, logs, QEMU variable stores,
  Python caches, and `.DS_Store` are intentionally ignored rather than uploaded.
- Cursor uses virtio-GPU hardware cursor plane. Marker:
  `cursor=virtio-gpu-plane move=cursorq scanout_damage=none host-cursor=hidden`
- Focused cursor runtime harness: `scripts/boot_test_aarch64_cursor.py`
- Make target: `test-aarch64-cursor-runtime`
- Focused cursor runtime passes on current image after 100 Hz timer input polling: seven QMP positions, zero changed scanout pixels, virtio-GPU cursor plane, host cursor hidden.
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
3. Expand the bounded guest C compiler into pointer arithmetic/parameters,
   arrays/structs and nested/general blocks, then add multiple functions per translation unit, broader relocation/
   object support, and a real build driver before a substantial in-guest build. Preserve real implementation
   requirements—no fake/spoofed apps.

## Operating constraints

- Use `apply_patch` for source edits.
- Prefer `rg`/`rg --files` for search.
- Avoid destructive git/filesystem commands.
- Keep user informed during long builds; avoid >60 s silent work.
- Heavy builds currently allowed.
- Do not overwrite user changes or old images unexpectedly.
