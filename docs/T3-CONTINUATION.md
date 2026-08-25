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

1. Near-native mouse performance; guest cursor must remain visible without pixel corruption or host-cursor duplication.
2. Real Firefox/browser support, modern websites/protocols, usable performance.
3. Functional terminal: punctuation/lowercase, commands, selection, copy/paste, real nano.
4. Normal desktop behavior: draggable/resizable/closable/reopenable windows, taskbar apps, pressed button feedback, clear login focus/tab behavior.
5. Text editor save correctness, file explorer, settings/resolution/users/sign-out, system monitor, Python runner, Wi-Fi/clock UI.
6. Future fully bootable/installable OS beyond macOS hosting; AArch64 and x86 paths.
7. Boot QEMU for user testing at meaningful milestones.

## Current verified state

- Active visible Pi/QEMU 10.0.11 TCG milestone for core commit `d5c0b0b`:
  PID 177966, VNC `127.0.0.1:5901`, session
  `build/makos-pi-visible-uy1pn5`, private data clone
  `build/makos-pi-visible-uy1pn5/data.img`, private variables
  `build/makos-pi-visible-uy1pn5/vars.fd`, QMP
  `build/makos-pi-visible-uy1pn5/qmp.sock`, serial
  `build/makos-pi-visible-uy1pn5/serial.log`, and PID file
  `build/makos-pi-visible-uy1pn5/qemu.pid`. It is the sole QEMU process and
  passes the four-PE EL0 marker, both remote group-stop markers, the concurrent
  independent-group serialization marker, the simultaneous same-group join
  marker, ordinary-config `smp_input_probe=0`, `MAKOS_LOGIN_UI_OK`, and
  `MAKOS_AARCH64_BOOT_OK`. Keep it running for user testing; use QMP `quit`
  before any later runtime gate.
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
  The gate closes before the desktop; general desktop/Firefox AP scheduling
  remains pending device affinity and contention gates.
  An opt-in seventh fixture runs after real virtio-input initialization. AP1
  blocks in EL0 `read_key` and returns to its idle dispatcher; the focused QMP
  harness sends a genuine Ctrl-K through virtio-keyboard, CPU0 drains the used
  ring, and an SGI resumes AP1. Two repeated Pi/TCG passes require matching
  input idle/resume masks `0x2`, status 61, exact frame balance, and boot
  completion. Normal `boot/MAKOS.CFG` never arms this external-input wait.
- 2026-08-25 AArch64 normative syscall 57 startup-vector parity is implemented.
  The exact 336-byte version-1 descriptor is copied and validated before child
  allocation. The guest-native two-pass assembler emits code that validates
  syscall-56 `argc=1` and syscall-57 `argc=3`, `argv[1]`, and `envp[0]`; Pi
  QEMU 10.0.11 TCG passes both status-42 executions plus three malformed-form
  denials. Full `make unit`, `make check`, release image/artifact checks and
  structural guards pass. The broad Pi/TCG harness later hit the preserved
  Settings resize mismatch (`560x360` versus exact `450x290`), so it is not a
  full broad-gate pass.
- No QEMU/test process was running at the 2026-08-25 handoff. Stale visible-test
  clone `build/makos-visible-data-1787604571.img` remains available, but PID
  `19919` and its QMP/session are gone. Check process state before every runtime
  gate; never start concurrent QEMU.
- Repository import commit `346b0df` is pushed to GitHub `main`. Generated
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

1. When no visible QEMU runs and host load/memory pressure is low, rerun unchanged
   `make test-aarch64-firefox-runtime`; diagnose code only if strict Ctrl-A still
   exceeds 10000 ms under an idle host. Never weaken Gate 3 thresholds.
2. Boot a current visible login milestone for user testing after the next verified
   behavior change; record PID/session/data clone/QMP before handoff.
3. Continue the highest-impact Partial/Missing original-spec row. AArch64
   userspace SMP scheduling remains the strongest candidate after syscall-57
   parity and the first genuine guest self-hosting seed. Preserve real
   implementation requirements—no fake/spoofed apps.

## Operating constraints

- Use `apply_patch` for source edits.
- Prefer `rg`/`rg --files` for search.
- Avoid destructive git/filesystem commands.
- Keep user informed during long builds; avoid >60 s silent work.
- Heavy builds currently allowed.
- Do not overwrite user changes or old images unexpectedly.
