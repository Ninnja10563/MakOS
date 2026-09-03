# Original specification audit

Status uses only runtime/static evidence in this workspace. `Implemented` means
tested required core exists; `Partial` means real implementation exists but
spec breadth remains; `Missing` means no qualifying implementation.

Last audit: 2026-09-03. Primary interactive target: AArch64 QEMU/HVF on Apple
Silicon. Original initial x86_64 target remains built/tested separately.

Current self-host implementation baseline
`5a49af108452983bf4809c12a2a8307582fa5955` corrects full quoted-include path
scanning, immediately rereads and byte-compares the generated 164-byte root and
1,215-byte leaf headers, and emits complete build, nested-output, and
header-dependency records through one checked `SYS_WRITE` each. The unchanged
focused Raspberry Pi Debian/QEMU 10.0.11
(`1:10.0.11+ds-0+deb13u1`)/TCG gate exits zero across eight graphs, 20
authenticated CLI builds, and 21 Toolchain processes. Three
disjoint cold graphs overlap as distinct singleton leaders on AP1-3 with
unique PIDs and TTBR0 roots; final placements are `9,4,8`, dispatches
`178,175,174`, with 29 migrations, source/target masks `0xe`/`0xe`, and zero
evidence drops. The same run has zero GPU delayed recoveries, timeouts, or
errors. These are Pi/TCG functional results, not a Firefox browser pass or
macOS/HVF qualification. They narrow but do not complete the Partial
self-hosting, scheduler, SDK/developer-tools, or graphics rows.

Scheduler-row evidence now also includes opt-in AP1 UDP/DNS and TCPv4 TX/RX
fixtures. AP1 copies bounded network requests to CPU0, which exclusively owns
both virtio-net rings. The TCP fixture performs connect, exact payload send,
blocking receive and FIN; a delayed exact host response makes AP1 return to
idle before CPU0 drains RX and wakes it. Another AP1 fixture performs a real
4 KiB VFS/MakFS4 write, `fsync`, reopen, 4 KiB read and byte verification.
CPU0's production timer bottom half completes all copied
virtio-blk requests; the latest Pi/TCG run reports 18 reads, 10 writes and 5
flushes, exact content/inode cleanup, and balanced frames. Low-level block
submission fails closed off CPU0 and recursively interrupted CPU0 ownership
defers one tick. A third AP1 fixture uses the normal native surface ABI to
create, fill, and present a retained surface. Its composition is deferred to
CPU0's production timer bottom half, which completes one real virtio-GPU
transfer and one resource flush; every low-level GPU submission now fails
closed off CPU0. A further Pi/TCG gate migrates one live EL0 TID from AP1 to
AP2 through a locked Running-to-Ready/unowned transition and verifies exclusive
ownership plus GPR/SP/TLS/SIMD preservation. A six-task shared-Ready-queue gate
then records 288 real yields across AP1-3, exclusive ownership on every
selection, statuses 80-85, exact frame balance, and even `99,99,99` dispatch
counters. After desktop initialization, a bounded production policy now keeps
leaders plus shell, UI, service, and device work on CPU0 while admitting
non-leader Firefox and ordinary Native application threads to AP1-3. Separate
focused upstream-musl pthread workloads under those exact roles pass on all
three APs (`cpu_mask=0xe`) with exclusive ownership and status 42. Their
production-only three-worker rendezvous proves simultaneous distinct-TID AP
intervals; evidence is isolated by exact launched group and role so the Native
fixture cannot satisfy Firefox qualification. Default workers retain public
affinity `0xe` while the kernel assigns least-reserved AP preferences. At a
64-dispatch imbalance, timer preemption can move a default worker once through
Ready/unowned without caller selection. Fresh Pi/TCG Firefox-role and Native
runs cover every AP and each record multiple such migrations with zero evidence
drops; a later `sched_setaffinity` remains authoritative.
The Firefox-role process owns overflow surfaces 7/8 and blocks target/decoy
watchers in `surface_wait_event`. Handle-specific readiness selects surface
7/TID 8 for QMP Ctrl-A, wakes exactly that watcher while three unrelated
surface waiters remain blocked, dispatches it on AP2, and then dispatches its
leader once on CPU0. Key dequeue records the exact Firefox watcher/group;
target syscall 149 is accepted only when that watcher reports the bounded
widget event and Gecko-main drain runnable are ready, then refreshes leader
priority and sends a scheduler SGI. Fresh Pi/TCG fixture evidence records the
arm, accepted post-enqueue acknowledgement, and CPU0 leader dispatch. The decoy
remains blocked until surface 8 destruction
wakes its fail-closed retry. Role-affine priority hints
are consumed after one successful dispatch; the longer deadline only removes
stale hints and no longer starves same-affinity fork children. The strict
real-Firefox gate requires equivalent launched-group overlap without changing
any latency threshold. It also requires three bounded live placement records
covering AP1-3 and a live 64-dispatch migration record tied to the launched
Firefox PID while the browser remains alive.
The keyboard/tablet transports now use their real QEMU `virt` GICv2 SPIs:
slot-derived INTIDs 77/78 are Group 1 edge-rising interrupts targeted only at
CPU0. Lower-EL IRQs drain and publish input directly; EL1 IRQs acknowledge and
defer to the retained 100 Hz recovery poll to avoid recursive syscall locks.
The focused Ctrl-A run proves direct keyboard INTID 78 delivery before the
exact-handle wake and status-42 reap.
AArch64 serial records are protected by an IRQ-masked cross-PE lock, so the
live and final SMP evidence lines cannot be byte-spliced. This does not close
the Partial row: the Firefox-role fixture is not real Firefox, and genuine
Firefox overlap and automatic-balancing evidence on idle macOS/HVF, additional
built-in/service roles, and broader repeated contention remain unqualified. AP
UDP/block completion waits are bounded EL1 `WFE`; they are not scheduler-idle
proof.

Firefox developer-link qualification note (2026-09-03): the protected build
from MakOS base HEAD `5827f228744c936b4091de93323d277fb4b4dcda` completed
Firefox 140.13.0esr source
`90ad18aabeaa9cbd63a1f749a57f266e758e50da` with all 59 patches (series
SHA-256 `c922d619398e64b6a162046efde105bc19152a9d868e9a2254ffa701874cc974`)
and patched tree `e6a918f00399df70a73e710a798c3e500e2b0a11` in `881:10`.
The all-five ELF parser, regenerated object-directory/backend selection, real
print-settings compile, and errno source/libc/object evidence pass. The final
audit printed `MAKOS_FIREFOX_BINARY_OK target=aarch64-unknown-makos
elf=firefox,plugin-container,xpcshell,libxul gecko=linked nss=linked
runtime=shared-musl interp=/lib/ld-musl-aarch64.so.1` followed by
`MAKOS_FIREFOX_DEVELOPER_BUILD_OK binary_audit=passed
release_provenance=withheld`. The five audited artifacts are `firefox`
(2,002,648 bytes,
`b897a56500ec0be81ad14b504c89e8a0594b77662a16e5368713f833c5b772b2`),
`plugin-container` (1,961,832,
`8611075f36124f7371e4291774227329b6f6ac86df76cd3de2966f86e076dd63`),
`xpcshell` (1,961,240,
`c43a9fc6307374bbd4da65d10094207396b8491ed9b19358202ff8f1c9cd30c4`),
`libxul.so` (734,482,952,
`71d5af3f4dee2ddf17bcb3803ee3aa42cd17aef7ce465a6fddde23dbf86b5fd5`),
and `libnspr4.so` (337,656,
`fb2aafe6f5a73c9555f115a45f8a192a249b6ea66c6041d0ba1ac6b5082a69f0`).
The 3,084,268-byte log
`build/logs/firefox-developer-linkfix-20260902-retry.log` has SHA-256
`77ddae030b0a5b223fcaf27c58552dd8785d2c9c5558e00720c232d1ddbf7a0a`.
This closes the observed developer compile/link blockers only. Provenance is
absent, and this evidence cannot authorize a release build stamp, package,
integrated image, QEMU/browser runtime, or macOS/HVF result. All affected rows
below therefore remain Partial.

Firefox package-coherence qualification note (2026-09-03): reviewed code
commit `817602513ccae985f1ca1d1159587520dfba7529` preserves the exact tree of
reviewed `f8a0ebf3932de5a5e77ffd21f6e87341f6cc0129`. Five stamp-authorized build
inputs are isolated in a private mode-0700 snapshot, independently stripped,
directly compared with the private staged tree, and used as the authority for
all five candidate runtime payloads. A same-directory temporary image passes
the actual package and Firefox preflights, including all-five ELF parsing,
before publication. Canonical-path and alias rejection protects source,
object, DIST, BIN, stamp, and output paths. Adversarial coverage interrupts
after each of six individually atomic auxiliary publications: `firefox`,
`plugin-container`, `xpcshell`, `libnspr4.so`, runtime provenance, and stripped
`libxul.so`. The auxiliary set is deliberately nontransactional and may be a
mix of complete old and candidate files until an unchanged rerun completes.
The candidate image moves last, leaving the prior image authoritative across
all earlier failures. This is code/focused-test evidence, not evidence of a
new release package, integrated image, QEMU/browser runtime, or macOS/HVF
qualification. No Partial or Missing label is upgraded by this increment.

| Requirement group | Status | Current evidence | Missing proof/work |
|---|---|---|---|
| Real OS, no host fallback | Implemented | UEFI loader, freestanding kernels, EL0/ring-3 userspace, own drivers; `scripts/boot_test.py`, `scripts/boot_test_aarch64.py` | Real-hardware qualification |
| Architecture/roadmap | Implemented | `docs/ARCHITECTURE.md`, `docs/ROADMAP.md`, `docs/SYSCALLS.md`, `docs/SDK.md` | Keep parity matrix current |
| UEFI boot chain | Implemented | Loader consumes memory map/config, loads the kernel into executable `LOADER_CODE`, exits boot services, and enters it; both automated boot suites plus Debian AAVMF execute-protection handoff proof | VMware/VirtualBox and physical UEFI tests |
| Bootable/installable media | Partial | Both architectures have redundant-GPT ESP+MakFS targets. x86 guest installer selects ATA `disk1`, enforces admin/exact confirmation/equal geometry/blank target, verified copy/flush/MBR-last, and source-matching resume. Six host safety tests plus QEMU refusal gates pass. Runtime SIGKILLs QEMU after first verified payload progress, proves LBA0 stayed blank and every partial block matches source, resumes with `resume-disk1`, verifies complete target SHA-256, detaches source, and passes two installed-only persistence boots. AArch64 uses the same fail-closed fresh/resume core through virtio-blk. It serializes a source flush/write-freeze before copy, denies all disk0 writes through commit, thaws on error, and retains freeze until successful shutdown. HVF gate guest-tests committed/conflicting resume refusal, hard-kills QEMU after first progress, proves blank LBA0 plus two source-identical partial blocks, resumes through exact `install disk1 resume-disk1`, verifies source/target SHA-256 equality, detaches source, and passes two persistence boots | Graphical selection, variable-size layouts, upgrades, physical qualification |
| ACPI/CPU/interrupts/timers | Partial | x86 APIC/IOAPIC/SMP; AArch ACPI/GICv2/generic timer plus genuine PSCI `CPU_ON_64` secondary entry under QEMU `virt`/HVF. Four PEs use private 1 MiB EL1 stacks, per-PE VBAR/GICC state, coherent MMU state, and a runtime rendezvous requiring every AP counter to advance during BSP work. Bounded EL0 gates prove AP timer PPIs, scheduler SGIs, CPU0-owned monotonic deadlines, shared-Ready execution, and post-desktop AP eligibility for Firefox, ordinary Native, and Python-role application workers. QEMU `virt` network/keyboard/tablet slots derive GICv2 SPI INTIDs 76/77/78, configure Group 1 edge-rising delivery to CPU0, and have focused real-IRQ runtime proof. Network INTID 76 directly drains one real DNS RX frame from lower EL and wakes a blocked AP waiter; the 100 Hz pump is recovery-only | Additional built-in/service scheduling roles, remaining device IRQs, hardware tables, power management, non-QEMU PSCI conduit/firmware and physical-machine qualification |
| Physical memory/kernel heap | Implemented | Frame allocator accounting/reclaim tests; owned heap self-tests | Pressure/reclaim policies |
| Virtual memory | Partial | Per-process roots, user isolation, mmap/unmap/protect/brk, NX/W^X, guard pages, teardown balance; package-backed demand paging is guest-exercised. Sparse anonymous reservations and demand-zero faults support modern JIT address spaces. Fresh upstream-musl guest probe writes anonymous pages, executes both `madvise(MADV_DONTNEED)` and MakOS immediate-decommit `MADV_FREE`, verifies resident-frame release through zero refault, then unmaps cleanly; structural guard and full AArch64 boot pass | COW, swap, ASLR, memory pressure/accounting UI |
| Scheduler/processes | Partial | Preemptive RR, saved full contexts, spawn/exit/wait/reap, timer-blocked sleep/deadline wake, and same-PID AArch64 `execve`; process ownership, exception return records and active TTBR0 state are CPU-indexed. The bounded four-PE Pi/TCG gate runs independent fixed-affinity EL0 tasks concurrently (`overlap_mask=0xf`) and proves AP sleep, poll, futex and IPC block/idle/timer-or-SGI wake with exact reap/frame balance. A forced-migration fixture moves one live saved TID AP1→AP2 through a locked Running→Ready/unowned→Running transition and verifies exclusive ownership plus GPR/SP/TLS/SIMD context. A shared-Ready-queue fixture runs six tasks over AP1-3 through 288 yields, checks exclusive ownership at every selection, and records even `99,99,99` dispatch counts, statuses 80-85 and exact frame recovery. Group-exit fixtures cover active EL0 interruption, deferred EL1 return, concurrent independent cleanup and same-group owner/join semantics; all PEs use 1 MiB kernel stacks. Opt-in real-device phases prove CPU0-owned virtio-input with AP keyboard wake; CPU0-owned virtio-net copied AP UDP/DNS plus a stateful TCPv4 connect/exact-send/blocking-receive/FIN lifecycle; CPU0 timer-serviced virtio-blk traffic from an AP VFS/MakFS4 write/`fsync`/reopen/read/verify/remove lifecycle (18 reads, 10 writes, 5 flushes); and CPU0 timer-serviced virtio-GPU composition after an AP native surface present (one transfer and one flush). Low-level device entry fails closed off-owner. After desktop startup, Firefox/Native/Python leaders plus shell/UI/service/device work stay on CPU0 while their non-leader application workers are AP-eligible. Distinct single-threaded `Toolchain` leaders are kernel-placed on a singleton AP using idle-first least-dispatched selection with rotating ties. The current real 21-process guest compiler/assembler/linker run uses AP1-3 with placements `9,4,8` and dispatches `178,175,174`, proves no caller-selected affinity, and makes 29 natural timer-boundary migrations across source/target masks `0xe` with zero drops. Its fixed parallel phase captures three simultaneous singleton Toolchain leaders on CPUs 1,2,3 with PIDs `13,15,14` and distinct TTBR0 roots `0x4012f000,0x401f3000,0x40191000`; an eight-dispatch imbalance moves full GPR/SP/TLS/SIMD context through Ready/unowned. AP telemetry is drained by CPU0 after child exit; AP console composition is deferred to CPU0 with 43 owner compositions, 48 AP deferrals, no pending handoff, and no off-owner GPU MMIO. A reproduced AP-exit/CPU0-reap race is closed by retiring the exiting PE's active TTBR0 under the scheduler lock before parent wake exposes the Zombie; structural ordering guards and all process reaps cover it without weakening active-root destruction checks. Separate exact-role pthread fixtures pass AP1-3 with exclusive ownership, AP block/wake/reap, and simultaneous distinct-TID intervals; markers are tied to the exact launched group and role. Overlap evidence is sampled from the scheduler's locked Running-owner table so stale source-AP entry state after migration cannot masquerade as concurrent ownership. Default workers are kernel-placed by least-reserved AP preference while retaining affinity `0xe`; a 64-dispatch timer imbalance can migrate each once with full context through Ready/unowned. Current Pi/TCG Native evidence has dispatches `6855,7066,10391`, placements `13,3,2`, and two migrations; Python-role evidence has dispatches `7136,10934,6846`, placements `1,1,1`, and one migration. Target syscall 148 supplies kernel-owned per-thread masks with same-group/online-mask validation and a locked migration boundary when the caller excludes its current PE; an explicit request disables automatic preference and remains authoritative. Official-musl patch 65 exposes real `sched_getaffinity`/`sched_setaffinity`; the fixtures verify CPU0 leader/AP worker masks, migration, restoration, join, and status-42 reap. The Firefox fixture additionally blocks real surface-event watchers on distinct handles in syscall 140; a genuine lower-EL keyboard SPI (INTID 78), not the timer fallback, delivers QMP Ctrl-A and wakes the exact surface waiter while unrelated waiters stay blocked. Target syscall 149 accepts only that watcher after its bounded post-enqueue acknowledgement, refreshes CPU0 leader priority, and sends a scheduler SGI. Current Pi/TCG production Firefox-role evidence records dispatches `7014,10845,7361`, placements `14,2,4`, three migrations, watcher TID 8 on CPU2, and status 42. This is a role fixture, not a Firefox browser pass; strict Firefox Gate 3 still rejects missing genuine-browser overlap evidence | Real Firefox TIDs must still supply overlap and automatic-balancing evidence under unchanged idle-macOS/HVF Gate 3. Additional built-in/service roles, broader priorities and repeated Firefox/desktop contention remain; `fork`/COW, multithreaded exec sibling termination, cancellable safe points for permanently non-returning EL1 driver paths, broader group-exit completeness, and TCPv6 runtime qualification remain |
| Threads/synchronization | Partial | AArch shared-VM threads, distinct TLS/SP/TIDs, futex FIFO WAIT/WAKE with relative deadlines/timer expiry, FIFO `FUTEX_REQUEUE`/`FUTEX_CMP_REQUEUE` preserving waiter handles/deadlines, clear-child-tid, per-task signal masks with clone inheritance; upstream musl pthread HVF probe; x86 threads/events. Current first-boot HVF run passes robust owner-death/wake-one, exact-task `kill`/`tkill`/`tgkill`, `pthread_mutex_timedlock` ETIMEDOUT, and a bounded three-waiter private `pthread_cond_broadcast` relay with all joins completing. Yield now delivers pending task signals before scheduler handoff | Cancellation breadth and broader signal set/semantics |
| Stable syscall ABI | Partial | Versioned descriptors/features, `docs/SYSCALLS.md`, bounds/capability checks. Both targets implement normative calls 56/57 and target call 148 with feature bit 22 for kernel-owned thread affinity; x86 truthfully exposes only CPU0 while AArch64 validates online masks and same-group targets. AArch64 additionally reports target max 149 and feature bit 23 for an exact Firefox watcher post-enqueue main-thread handoff; x86 remains at target max 148. AArch64 copies and validates the exact 336-byte version-1 descriptor, bounded argv/env offsets and strings before allocation, builds child-owned SysV vectors, and advertises startup-vector bit 19. Pi/TCG guest-built code validates default and explicit startup registers; three malformed forms fail closed | Unify errno conventions; freeze the remaining cross-arch ABI; fill POSIX/device/security calls |
| IPC | Implemented | Both targets retain process-owned scalar channels/events and now expose queued typed native IPC through syscalls 143-147. Versioned 64-byte messages are bounded FIFO and kernel-stamp sender PID/UID. Atomic handle transfer uses generation-tagged channel objects, rights attenuation, queued lifetime retention, stale/spoof denial, and reachability collection for transfer cycles. Service publish/accept requires `CAP_SERVICE_PUBLISH` plus `CAP_IPC`; connect requires `CAP_IPC`; routes enforce matching UID/session. Process exit removes handles/routes before reap. Twelve core unit tests, structural ABI guard, full unit/check, and upstream-musl child/provider HVF runtime pass. Existing pipes, timed poll, epoll, POSIX shared memory, AF_UNIX `socketpair`, and `SCM_RIGHTS` remain guest-tested | Broader broker/discovery policy and richer object classes are future breadth, not missing core IPC |
| Native filesystem/VFS | Partial | Persistent MakFS, block allocation, FDs, metadata/modes/UID/GID, mount, recovery/reconcile, Files app; musl directory streams plus per-process cwd/relative lookup are guest-tested; checksummed sector-backed package format mounts and streams Firefox payload without kernel embedding; MakFS4 source supplies 4 KiB extents, CRC/COW metadata, redundant roots, sparse 1 GiB geometry, Unix-epoch atime/mtime/ctime, and persistent symlink resolution. Current MakFS4 uses 512 inode records, 255-byte components, a 1,024-bucket collision-chained in-memory child index rebuilt from authoritative metadata, and resumable raw-inode directory cursors. Two-boot HVF passes timestamps, symlinks, 64-sibling/name255 create/readdir/random lookup, remount verification, and cleanup. Read-only `makos-makfs4-fsck` accepts raw or validated redundant-GPT data volumes and checks roots, metadata geometry/CRCs, inode graph/cycles/names, extents, and bitmap agreement. Six sparse-volume tests pass. Reproducible gate now exports the real two-boot guest volume only after QEMU closes; fsck accepts generation 257/root slot 1 with 5 inodes and 4 allocated blocks | Repair mode, on-disk tree indexing beyond bounded 512-inode geometry, migration fault injection, broader cache, common FS formats |
| Storage/drivers | Partial | x86 ATA/PCI/UHCI; AArch virtio-blk; real persistent images. AArch64 low-level ring submission fails closed off CPU0. A bounded copied-request queue carries 512-byte/4 KiB reads/writes and FLUSH. A real AP1 VFS/MakFS4 runtime writes/`fsync`s/reopens/reads/verifies/removes a 4 KiB file through CPU0's non-recursive timer service: 18 reads, 10 writes, 5 flushes and 33/33 timer completions with balanced frames | NVMe, AHCI, xHCI, USB mass storage, hotplug, driver isolation |
| Graphics/window system | Partial | Real framebuffer/virtio-GPU scanout, surfaces, compositor, focus, drag/resize/minimize/close/reopen, x86 cursor-free scene shadow, AArch64 dedicated virtio-GPU cursor plane, per-user 64 KiB clipboard. Six desktop slots plus two bounded overflow slots support handle-specific blocking: only an owned surface with queued data wakes, while destroyed handles wake for a fail-closed retry. Pure pointer motion uses only the cursor queue; focused runtime moves through seven positions with zero changed scanout pixels and hidden host cursor. Retained backing damage skips unchanged presents. Package `a9c604254f094de2` includes Firefox held-button patch `0054`, Ctrl-L/raw-136 patch `0055`, and valid MakOS line-mode wheel patch `0056`; binary, manifest, CRC, ELF, and package checks pass. Source patch `0057` adds a post-enqueue Gecko-main handoff acknowledgement but is not in that historical package, so a new package/integrated image and idle-macOS/HVF qualification are explicitly pending. The ordered source series now contains 59 patches including `0046a`, PDF print-settings patch `0058`, and Rust errno ABI patch `0059`. The print source defines Gecko's hidden platform factory with PDF output and no native-printer claim; errno 0.3.8 selects MakOS musl's real exported `__errno_location`. Structural, staging, Cargo-routing, provenance, focused source/libc/object symbol evidence, regenerated backend selection, real print compilation, and a protected developer full link now pass. The developer audit covers all five AArch64 ET_DYN artifacts and proves a complete compile/link, but deliberately withholds release provenance; a default release build, package, and runtime remain pending. The build/package pipeline binds pinned source plus the ordered patch-series digest to five audited build hashes and five exact stripped runtime hashes; integration and the strict target reject missing, stale, or mismatched provenance before QEMU. Release and pre-QEMU gates bounded-parse all five artifacts, enforce executable entry/interpreter ordering, DSO interpreter absence, mapped dynamic metadata, per-artifact dependencies, and exact libxul/libnspr SONAMEs. Strict first paint keeps JIT/blit/client-pixel proof; input-dependent syscall-149 handoff markers are required immediately after the first timed Ctrl-A/raw-132 wait, with elapsed time captured first and the unchanged 10000 ms limit retained. Focused adversarial tests pass, but no new release package or runtime evidence is claimed. Strict Gate 3 retains exact-URI TLS/HTTP, clipboard, rendered-link click, document drag-selection/copy, and location reload proofs. Sustained runtime additionally proves down/up scrolling, real form entry plus pointer selection/clipboard exact-query composition, four repeated example/IANA top-level navigations, bounded host CPU/RSS, and actual guest resident-page reporting. Final default pass paints in 169697 ms, handles Ctrl-A in 5727 ms and first input in 111 ms, bounds CPU ratio at 1.053 and RSS at 325140480 bytes, reports 54254 Firefox resident pages, and survives 531 seconds. Two later high-pressure macOS/HVF runs painted in 248584/255543 ms but missed the unchanged 10000 ms Ctrl-A limit at 10971/14363 ms; they remain recorded failures pending a fresh provenance-bearing package and unchanged idle-host rerun | Acceleration, scalable fonts, multi-display, DPI, public mature GUI API, and broader modern-site coverage remain |
| Native desktop/apps | Partial | Login/session, taskbar/start with minute-refreshed UTC/DHCP tray, Terminal, Settings/users/resolution/network status, Files, Text Edit with selection/copy/cut/paste, System Monitor, Browser, Python runner, two-boot-tested upstream GNU nano | Notifications, real Wi-Fi setup once hardware/driver exists, richer configuration, production app/service model |
| Input | Partial | x86 PS/2/UHCI keyboard; AArch virtio keyboard/tablet; punctuation/modifiers/absolute pointer tests. AArch64 now routes real slot-derived GICv2 SPIs to CPU0: lower-EL IRQs immediately drain queues and exact-handle wake the Firefox surface watcher, while EL1 entries acknowledge/defer safely and the 100 Hz poll remains recovery-only. Pi/TCG QMP Ctrl-A proves direct keyboard INTID 78 delivery | Layouts/IME, generic HID, hotplug, touch/accessibility, physical-hardware IRQ qualification |
| Networking | Partial | Ethernet, ARP, DHCP, IPv4, ICMP, UDP, TCP, native DNS, asynchronous RX and poll/epoll readiness are guest-tested. TCP keeps 32 KiB RX buffers in a bounded external pool, advertises actual free capacity after each accepted segment, sends duplicate zero-window ACKs rather than dropping over-capacity bursts, and sends pure window-update ACKs when userspace drains buffered data. IPv4/IPv6 packet tests verify requested window fields with valid checksums; structural flow-control guards, full AArch64 boot, and strict Wikipedia HTTPS runtime pass. Firefox resolves both `example.com` and `www.wikipedia.org`, connects TCP/443, validates built-in roots, and completes exact-URI HTTPS with changed page pixels without host proxying. Focused guest gates validate RA/SLAAC EUI-64 configuration, native 28-byte AF_INET6 socket ABI, NDP route resolution, checksum-valid UDPv6 transmit, CPU0-only virtio-net TX/RX, and a real AP1 UDPv4 DNS request/response through a bounded copied-request queue. QEMU `virt` slot 28 now routes GICv2 SPI INTID 76 exclusively to CPU0; a syscall-free CPU0 EL0 interval proves lower-EL direct RX drains the real DNS response, wakes blocked AP1 by SGI, and leaves the unchanged 100 Hz pump as recovery only. A separate Pi/TCG gate proves copied CPU0 service for AP1 TCPv4 connect, exact send, delayed blocking receive/wake, ACK and FIN with exclusive ring ownership. UDPv6/TCPv6 checksums/demux, IPv6 endpoint names/bind, and musl AAAA/AI_ADDRCONFIG semantics also pass offline wire tests plus kernel/static/shared-libc builds | Guest-test UDPv6 receive and TCPv6 connect/send/receive, DAD, extension headers, scoped link-local endpoints, multicast policy, broad options/server calls, Wi-Fi, VPN, routing/firewall/namespaces, and physical-device IRQ qualification |
| Audio | Partial | x86 AC97 DMA + userspace fixed-format API | AArch audio, mixer, volume, multi-client streams, HDA/USB audio |
| Security/accounts | Partial | Persistent multi-user DB, PBKDF2-HMAC-SHA256, per-PID credentials/caps, file modes, address isolation, sandbox tests, NX/W^X. Structured-log reads require `CAP_CONSOLE`; real Browser sandbox denial leaves output/metadata buffers untouched. PID-attributed persistent severity-4 audit records cover authentication, account, session, and package decisions. Current two-boot HVF gate summarizes the loaded journal before early-record merge and proves prior-boot authentication-accepted plus account-created decisions survived with nonzero PID attribution. MakOS clang defaults C/C++ target builds to `-fstack-protector-strong`; object-symbol gate proves instrumentation and RNG-backed `AT_RANDOM` startup exists. A rebuilt deployed musl CRT probe performs a real protected-buffer overwrite; full HVF runtime proves `__stack_chk_fail`, lower-EL fault containment, status-139 wait/reap, shell survival, and continued tests | Rebuild broader deployed apps; ASLR, executable signatures, secure-boot policy, MAC, broader secure IPC/security event audit |
| Init/services/userspace | Partial | Native init/login/shell, utilities, TTY/ANSI, Files/Text Edit/diagnostics/config, upstream MicroPython plus guest-tested official CPython 3.14.7 package | Dependency-aware service manager, broader utilities/network tools, CPython extension/package breadth, crash supervision |
| Native libc/POSIX | Partial | Official musl 1.2.6 static/shared libc; upstream crt1/main/pthread probes; upstream `PT_INTERP` resolves libc and loads a separate VFS `DT_NEEDED` DSO through open/fstat/eager-private-map, RELA/PLT/GOT, symbols, RELRO; runtime `dlopen(RTLD_NOW)`/`dlsym`/call/`dlclose`; musl `execve` caller→new dynamic target with argv/env and same PID; exit/wait/reap under HVF. Patch 62 translates symlink and extended stat ABIs; patch 65 translates Linux AArch64 `sched_getaffinity`/`sched_setaffinity` to real target syscall 148. Fresh patch replay, all-1,350-object build, structural checks, and separate Pi/TCG Firefox/Native role pthread runtimes pass kernel-owned get/set/migration/restoration | Documented remaining semantic gates, recursive/TLS/versioned DSO breadth and unload stress, full signals/files/readiness/process/network APIs |
| Native executable/ELF | Partial | Validated ELF64 loading, multiple unaligned PT_LOAD mappings, SysV argv/envp/auxv, PT_TLS/TLS, permissions; upstream musl `PT_INTERP`, external `DT_NEEDED` VFS lookup, RELA/PLT/GOT, symbol resolution, RELRO, PIE entry, basic runtime `dlopen`, 920 KiB multi-page system ELF mapping, and same-PID system-path `execve` guest-tested. AArch64 calls 56/57 safely snapshot untrusted MakFS ELF64, reject wrong type/machine, out-of-file segments, overlaps, invalid ranges/flags and W+X before allocation, then map and execute the immutable snapshot with default or explicit child-owned startup vectors. Genuine Firefox ESR browser/plugin/xpcshell/libxul passes JIT initialization, creates a 700x400 native window, paints browser chrome and a nonblank document, accepts the exact URL key stream, and completes HTTPS HTTP 200 | Writable/shared file mappings, broad TLS/versioned relocations, general ASLR, broader untrusted-ELF error recovery, modern-site breadth, and acceptable cold-start performance |
| Linux compatibility | Partial | Own syscall translation fixtures and musl Linux-number adapter; genuine Firefox ESR executes on MakOS musl/C++ runtime without Linux kernel or Linux target identity, through JIT, native window/chrome, keyboard, DNS/TCP/TLS, and HTTP completion | Complete document rendering plus broad Linux ABI, `/proc`, signals, fork/exec, device/ioctl semantics, real app suite |
| Windows compatibility | Partial | Local PE64 parser/loader and bounded Win32 translation fixture; no Wine/Windows kernel | Broad Win64 loader/runtime, ntdll/kernel32/user32, registry/config, filesystem/process semantics, real apps |
| Other ABIs | Partial | Modular role/loader structure and native/MicroPython/CPython/musl paths | Stable plugin ABI and another independently tested format/runtime |
| Package/update/recovery | Partial | Signed legacy and `MAKDEP1` exact/minimum dependency manifests route install/upgrade/query/remove/rollback to reserved no_std A/B store; active payloads mount under `/packages`, refresh live, and Settings reports backing/generation/count; overlap/corrupt migration fails closed. Five-boot HVF runtime passes signed install/replace/remove/rollback, live payload reads, reboot persistence, open-FD A/B slot pinning, tamper denial, and corrupt-newest fallback | Repositories, key rotation, richer solver, boot rollback/recovery UI |
| SDK/developer tools | Partial | Headers, syscall wrapper, cross-build docs, host toolchain and genuine ports. A sandboxed AArch64 EL0 toolchain implements a bounded two-pass assembler, source-driven C subset compiler, and static linker. Translation units support up to six AAPCS64 functions and parameters, four locals, signed arithmetic/relations, bounded arrays and pointers, same/external calls, and nested assignment/`if`/`else`/`while` control to depth four. Genuine ELF64 `ET_REL` fixtures cover two-, three-, four-, and six-function units, up to five `R_AARCH64_CALL26` relocations, six arguments, W^X execution, signed division/remainder, and malformed/overflow rejection. `MAKBUILD1` accepts two through six inputs and persists validated state-last `MAKSTATE2` cache records. Exact absolute quoted includes are recursively resolved through guest MakFS to four nested headers and eight unique dependencies. The corrected parser consumes every valid path byte through the closing delimiter rather than stopping after one byte. Fixture startup immediately rereads and byte-compares `/home/user/generated-inline.h` (164 bytes, FNV-1a `bbbc9068d3d73e49`) and `/home/user/generated-leaf.h` (1,215 bytes, `ccf73fc02c2f9ceb`) before preprocessing. The pass supports eight bounded object/function macros, four parameters, rescanning depth eight, four conditional levels, and checked signed `#if`/`#elif` expressions with selected-arm-only evaluation; malformed, missing, relative, cyclic, protected, over-depth, overflow, bad-arity/recursion, and token-operation forms fail closed with bounded diagnostics. Build, nested-output, and dependency evidence is assembled in a 768-byte buffer and emitted by one checked `SYS_WRITE` per complete record. Exact tracked repository C/assembly and bounded read-only production C/`stdint.h` are compiled with generated identities, typed objects, `.rodata`/`.data`, ADRP+ADD relocations, and R-X/R--/RW-NX output. Fresh Pi/TCG runtime and full `make unit check` pass | Expand beyond six parameters/arguments and add pointer-provenance analysis and broader expression/lvalue arithmetic, variable-length/general global/multidimensional arrays, structs and general blocks; more than six functions or objects; broader assembler/directive/relocation/object support; native full C/Rust compiler, general-purpose linker, debugger, variadic/multiline macros, token pasting/stringification, function-like expression macros, general system-header preprocessing and include graphs beyond current bounds, arbitrary build graphs/general parallel scheduling, and package dev tools |
| Self-hosting | Partial | Focused Raspberry Pi Debian/QEMU 10.0.11 TCG persists and rereads eight bounded `MAKBUILD1` graphs and runs 20 authenticated CLI builds through real `argc=2`/`argv[1]` startup without build-mode seeding. Manifest paths drive guest MakFS reads, object writes/reopens, static linking, state-last `MAKSTATE2` updates, and final ELF writes; malformed versions, paths, collisions, ordering, missing links, and seven-input graphs fail closed. The primary graph resolves external and same-object `R_AARCH64_CALL26`; separate three-input, quoted-header, exact repository-source, production-data, const-only, mutable-only, and nested-control graphs prove the count is not fixed. Cold/warm, corrupt-object, edited-source/header, and corrupt-state results are authenticated, and cached objects still pass ELF/symbol validation. The corrected quoted-include parser consumes full paths; fixture startup byte-compares its generated headers before recursive include/macro/conditional tests. A fixed command spawns the three disjoint cold three-input/header/nested graphs before any wait. Complete child evidence records are single-write, all three children reap status 42, and a locked snapshot proves simultaneous singleton Toolchain leaders on CPUs 1,2,3 with PIDs `13,15,14` and distinct roots `0x4012f000,0x401f3000,0x40191000`. All 21 Toolchain processes use AP1-3 with placements `9,4,8`, dispatches `178,175,174`, 29 migrations, masks `0xe`/`0xe`, zero drops, and status 42. The production data and nested-control outputs execute through the ordinary loader as 42. Full `make unit check`, Native/Python SMP, production Firefox-role, and cursor gates pass; these are Pi/TCG functional results, not a Firefox browser pass or macOS/HVF qualification | This remains a bounded seed, not a general C/Rust compiler/linker: no pointer-provenance analysis, broader expressions, general global/multidimensional arrays, structs or general blocks, more than six parameters/functions/objects, aggregate linked output beyond 1,024 code bytes, variadic/multiline macros, token pasting/stringification, function-like expression macros, general system-header preprocessing, include graphs beyond four nested headers/eight dependencies, arbitrary build graphs or general parallel scheduling, debugger, or substantial in-guest MakOS build; Milestone 10 still requires end-to-end MakOS builds inside MakOS |
| Crash/log/debug | Partial | Both targets expose the same bounded 32-record structured log ring through syscalls 28/29 with sequence, monotonic timestamp, PID, severity, and readback. `MAKLOG01` persists the ring in a CRC-protected fixed image through MakFS4 COW, merges pre-mount records after reboot, and preserves malformed journals rather than overwriting them; three codec tests plus fresh two-boot AArch64 durable-merge runtime pass. AArch64 lower-EL instruction/data aborts terminate the offending process group and resume its parent; a real stack-canary failure proves status-139 reap and shell survival while EL1 faults remain fatal. AArch64 PL011 formatted and raw records now use an IRQ-masked cross-PE lock; repeated production-SMP runtime proves concurrent markers remain intact. x86 and AArch64 EL0 payload/metadata probes, serial fatal/markers, deterministic recovery tests, and QEMU debug config remain available | Crash dumps, debugger protocol, service restart/recovery UX |
| Test mandate | Partial | Unit/kernel/syscall/FS/net/VM/scheduler/driver/userspace/security/boot/recovery coverage across scripts/crates | Coverage map for every claimed API, real hardware matrix, fault injection/fuzzing |
| Reproducible fresh build | Partial | Make/Cargo image targets, documented prerequisites, QEMU scripts | Clean-machine CI proof, pinned complete toolchain/bootstrap, release artifact signing |

## Current critical path

Qualification note (2026-09-03): implementation through
`5a49af108452983bf4809c12a2a8307582fa5955` includes exact read-only
`ports/musl/shared-demo.c`, the bounded self-host `/usr/include/stdint.h`,
`STT_OBJECT`, `.rodata`/`.data`, paired AArch64 ADRP+ADD relocations,
cross-object data resolution, and final R-X/R--/RW-NX regions. It also corrects
complete quoted-path parsing, verifies the exact generated headers by guest
readback before expansion, and emits parallel child evidence as complete
one-write records. ET_REL construction uses a 4 KiB in-memory work buffer but
persistent files remain bounded by MakFS's 2 KiB limit.

The same baseline retains the fixed authenticated
`makbuild-parallel` phase for three disjoint cold graphs. The shell performs all
three spawns before any wait, safely distinguishes Pending from NoChild, drains
launched siblings after partial spawn failure, requires three status-42 reaps,
and emits its success record only afterward. The scheduler captures a locked,
one-shot proof that distinct singleton Toolchain leaders with unique TTBR0
roots simultaneously own AP1-3, emits that proof later on CPU0, and restricts
migration destinations to idle APs. The harness correlates exact process,
placement, reap, group, root, and ordered migration evidence while permitting
nondeterministic child output and legal pre/post-snapshot migration. The
unchanged Pi Debian/QEMU 10.0.11/TCG gate exits zero with eight graphs, 20 CLI
builds, 21 Toolchain processes, three simultaneous cold graphs, placements
`9,4,8`, dispatches `178,175,174`, 29 migrations, masks `0xe`/`0xe`, and zero
drops. The locked snapshot has PIDs `13,15,14`, unique roots
`0x4012f000,0x401f3000,0x40191000`, and CPUs `1,2,3`. This does not qualify
macOS/HVF or a Firefox browser. The 12,408-byte harness log is
`build/logs/aarch64-selfhost-parallel-atomic-20260903.log`, SHA-256
`32ef4df36570702594cd15237247e61efe8e934ea93b1b24e24f2992293394da` and the
75,738-byte serial log is `build/makos-selfhost-focused-serial.log`, SHA-256
`118d4e1f3a2f55eb342671c38a2cb0acec944703056644a9742e30103f73c84c`.
Full unchanged `make unit check` also passes; its 79,717-byte log is
`build/logs/full-unit-check-20260903-atomic-final.log`, SHA-256
`23bdbb78d0bb547d1d70fbe5ff55ea1f34fa698d9a4243f029abb5f4690f8113`.
General globals/initializers, aggregates, common objects, TLS,
general system headers, a self-hosted DSO, arbitrary build graphs, and
substantial in-guest builds remain missing, so the Scheduling, SDK/developer
tools, and Self-hosting rows stay Partial.
The older row-gap shorthand `arbitrary/parallel build graphs` is therefore
superseded only for this fixed three-graph command; arbitrary graphs and
general parallel build scheduling remain missing.

The unchanged strict Firefox target was attempted on the idle Pi, but exited 2
before QEMU because
`build/makos-integrated-firefox-handoff149.img` is absent. This is a fail-closed
missing release-package prerequisite, not a browser runtime failure or pass;
no macOS/HVF evidence exists and all strict thresholds remain unchanged. The
7,629-byte preflight log SHA-256 is
`637bc5815537a755b93243ee3f02de20837890c3e9bf0ce5b02a7390ab41fc06`.

1. Requalify strict Firefox Gate 3 on an idle host, then widen upstream apps.
2. Add package repositories, key rotation, richer dependency solving, and
   transaction-recovery UI.
3. Hardware breadth: Wi-Fi, AArch64 audio/USB, multi-display, real machines.
4. Expand the self-hosting seed into compiler/linker/build-system support and an
   end-to-end substantial in-guest build.

Goal remains incomplete until every `Partial`/`Missing` row reaches tested
required scope. Narrow markers never prove broader rows.
