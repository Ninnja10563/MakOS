# MakOS macOS/HVF milestone test-agent prompt

Copy the prompt below to the agent that will test this milestone on the target
Mac. Do not relax, skip, or reinterpret a failed threshold.

```text
Test the current MakOS main branch on an idle Apple Silicon macOS host using
AArch64 QEMU with HVF. This is qualification work only: preserve repository
changes and test data, do not change source or thresholds, do not run two QEMU
instances concurrently, and stop every visible guest through QMP before the
next runtime. Stop on the first failed command; do not retry or run later
gates, including the final visible login, after a failure.

Repository: https://github.com/Ninnja10563/MakOS.git
Branch: main
Previous exact Mac-tested baseline: 73da7fc65cbc54dea4ff215f39c50c7190a1248a

Use the exact full cold syscall-buffer repair commit supplied in chat,
descending from this baseline; do not test the baseline again or choose an
arbitrary future descendant. The last run passed all twelve ordered commands
through cursor, a supported 61-patch Firefox release, integration and image
preflight. The full 600-second Firefox probe then failed on missing
`MAKOS_JIT_POOL_OK`: eight blits, seven TLS successes and one browser-document
submission, but no fatal or Firefox screenshot/interaction result. Kernel
write validation incorrectly rejected untouched demand-backed readable pages.
Populate authorized buffers before the unchanged resident-permission and
whole-record serial checks. The missing record's exact page residency was
not captured on the Mac; this source-proven defect is not yet a browser pass.
Firefox source, all 61 patches, provenance and runtime gates are unchanged.
Reuse the verified October 10 integrated image below; do NOT rebuild Firefox,
repackage, reintegrate, change profile preferences or restamp artifacts for
this kernel-only repair. Preserve all historical images and private profiles.

Pi source/host/cross-build checks do not qualify Firefox on this Mac.
The final Pi run passed full unit/build and one EL0 runtime with complete
cold-write evidence, then stopped at an unchanged boot SMP load-ratio failure
(dispatches 48,145,113; 145 > 3 * 48) before the VM fixture. Later Pi gates were
not run. This outstanding result is not waived or called a clean sweep;
require the full Mac sequence below and retain any recurrence unchanged.
Do not change fatal assertions, timeouts, thresholds, SDK
macros, fortification or -Werror. Keep /usr/bin/cc (Apple clang); do not set
HOST_CC to substitute another compiler. Preserve all preceding kernel,
whole-record emission, executable-context and futex repairs. Require
checked-out HEAD, local main, origin/main, and
remote main to match, and record the exact tested commit.

1. Read AGENTS.md, docs/T3-CONTINUATION.md, docs/ORIGINAL-SPEC-AUDIT.md,
   docs/STATUS.md, docs/BUILD.md, docs/INTEGRATED-DATA-IMAGE.md, and
   docs/FIREFOX-PACKAGING-20260908.md, docs/FIREFOX-EL0-ENTRY-20260910.md,
   docs/EL0-EVIDENCE-ATOMICITY-20260911.md and
   docs/EL0-DARWIN-ADAPTER-20260911.md and
   docs/FIREFOX-FATAL-CAPTURE-20260915.md and
   docs/FIREFOX-VM-FAULT-20261008.md, docs/FIREFOX-BTYPE-20261009.md and
   docs/FIREFOX-NET-OWNER-20261009.md and
   docs/FIREFOX-RUNTIME-EVIDENCE-20261010.md.
   Also read docs/FIREFOX-FRESH-BASELINE.md if the preserved image is absent.
2. Record `git status --short --branch`, local HEAD, origin/main, remote main,
   macOS version, Apple chip/model, QEMU version, accelerator, CPU count, load
   average, free/used memory, swap/compressor state, and every QEMU process.
   Pull main only if the worktree is clean. Install QEMU with Homebrew if it is
   absent (`brew install qemu`). Do not overwrite an existing data image. Save
   the output of `sw_vers`, `system_profiler SPHardwareDataType`, `uptime`,
   `vm_stat`, `sysctl vm.swapusage`, `memory_pressure`, and
   `pgrep -fl 'qemu-system|boot_test_aarch64' || true`. Confirm the last command
   reports no QEMU or runtime harness before each runtime below.
3. Run these gates in order, never in parallel, and save complete logs plus
   each exit status:

       make unit check
       make image-aarch64
       make test-aarch64-el0-entry-runtime
       make test-aarch64-vm-fault-runtime
       make test-aarch64-user-write-runtime
       make test-aarch64-btype-runtime
       make test-aarch64-net-owner-runtime
       make test-aarch64-smp-tcp-runtime
       make test-aarch64-smp-input-runtime
       make test-aarch64-selfhost-runtime
       make test-aarch64-native-smp-runtime
       make test-aarch64-production-smp-runtime
       make test-aarch64-cursor-runtime

   The new EL0-entry gate must report
   `MAKOS_AARCH64_EL0_ENTRY_RUNTIME_OK accel=hvf fixture=dynamic-musl-pthread`,
   three distinct TIDs observed by the kernel on AP1/AP2/AP3 with masks
   0x2/0x4/0x8, loader pthread_create in 0x28000000..0x30000000, high mmap RX
   code, resume on the next page, 96 calls with `block=sleep-until`, three
   `MAKOS_AARCH64_HIGH_EL0_ENTRY_OK` records matching those exact TIDs/CPUs,
   a common nonzero aligned root and next-page PC with
   `proof=validated-before-eret`, three status-42 joins, and the
   ordinary status-42 dynamic-process reap. Require unchanged boot hashes.
   The result must be one contiguous record including its terminating line
   ending; preserve the raw serial bytes. Never join fragments around kernel
   diagnostics or suppress those diagnostics to satisfy the existing parser.
   Preserve its printed session directory, session.json/PID/inherited QMP fd,
   private boot/data/vars, and serial.log on success or failure. This fixture
   is not Firefox and cannot replace its unchanged strict gate.

   The added VM-fault gate must report
   `MAKOS_AARCH64_VM_FAULT_RUNTIME_OK accel=hvf fixture=dynamic-musl-pthread`,
   the same three AP workers as its own unchanged EL0 proof,
   `rounds=16 same_page_rounds=16 distinct_pages=48 table_stride=2097152`,
   `coherent_checks=288 first_touch=barrier-released`, status-42 joins,
   `cleanup=unmapped boot=unchanged`, and exactly one complete guest
   `MAKOS_MUSL_VM_FAULT_OK` record. Preserve its printed nested session,
   private boot/data/vars, session.json/PID/QMP descriptor and final serial.
   This exercises genuine concurrent first touches but deliberately reports
   `kernel_loser_interleaving=not-counted firefox=not-tested`; do not claim a
   deterministic losing kernel interleaving or browser pass from the marker.
   Deterministic host tests force that race separately. The added wrapper
   launches one EL0 harness, with its original deadlines and checks intact.

   The added user-write gate must report
   `MAKOS_AARCH64_USER_WRITE_RUNTIME_OK accel=hvf`. Require two complete
   immutable-source lines written from untouched read-only mappings through
   musl/native syscall 17 and native TTY syscall 63, a cold cross-page file
   write with exact readback, and ten denied invalid-buffer calls retaining
   the existing errors (17:-1,63:-22). Require exactly one complete guest
   `MAKOS_MUSL_USER_WRITE_OK` record, cleanup, the unchanged EL0/VM proofs and
   status-42 reap. This is a syscall-buffer fixture, not JIT execution or
   Firefox. Retain its nested session, private disks, raw serial, PID/QMP and
   unchanged boot hashes. No output from a rejected buffer may be accepted.

   The added BTYPE gate must report `MAKOS_AARCH64_BTYPE_RUNTIME_OK accel=hvf`.
   Require exactly one complete ordered SOURCE_OK, ENTRY_OK, TARGET_OK and
   final MAKOS_AARCH64_BTYPE_OK record for the same immutable probe TID/root,
   source CPU1 and target CPU2, SPSR=0x80000400, BTYPE=1, unchanged SP/TLS,
   and PCs in its two-instruction BR loop. Entry PC must equal capture PC.
   Require an actual status-42 reap after target IRQ and before final result,
   one migration and balanced frames. Termination is kernel-controlled only
   after the resumed EL0 IRQ, not a userspace exit. The target loop regenerates
   BTYPE; do not claim that IRQ alone proves its first post-ERET value. The
   original saved context must match before ERET, and host tests separately
   reject state-clearing mutations. UNSUPPORTED is not a qualifying pass.
   Preserve the nested private boot/data/vars, session.json/PID/QMP and final
   raw serial. The wrapper also requires every unchanged EL0 harness check.

   The new network-owner gate must prove three real AP1 UDP socket sends
   received byte-exactly by its host fixture and emit
   `MAKOS_AARCH64_NET_OWNER_RUNTIME_OK accel=hvf` with
   `timer_completions=2 lock_wait_completions=1`, positive `busy_deferrals`,
   `udp_host_received=3 payloads=exact` and `statuses=42,42,42`.
   Require IRQ-masked CPU0 socket
   contention completed by production lock-wait TX service, a separate
   current-EL timer completion with no manual owner service, and a timer
   deferral while the actual network-device lock is held with a request READY,
   followed by completion after unlock. Require three exact status-42 reaps,
   balanced frames, CPU0-only virtio-net ownership, and unchanged master and
   private boot hashes (the private clone has only its opt-in config changed).
   Preserve the printed session, private boot/data/vars, session.json/PID/QMP,
   full final serial and exact host datagrams. This is a driver/scheduler
   fixture, not Firefox. The unchanged SMP TCP gate must retain its real
   SYN/SYNACK/ACK, exact request/response, FIN, AP idle/SGI wake, locked socket
   publication, owner-only rings and frame-balance proofs. No gate may be
   replaced by host-only tests or manually drained test queues.
   The unchanged SMP-input gate also retains its real AP UDP/DNS RX interrupt
   and wake proof alongside its existing input, GPU and block assertions.

   Record the boot-image SHA-256 both before and immediately after the
   self-host run, because its phony image prerequisite rebuilds that file;
   require equality and treat the post-run hash as the identity of the bytes
   used by the harness. Preserve
   the complete harness output and `build/makos-selfhost-focused-serial.log`;
   record each path, byte size, SHA-256, command exit status, QEMU version, and
   accelerator. From the final host marker record exact graph/CLI/process
   counts, placements, dispatches, migration count and source/target masks,
   evidence drops, parallel PIDs/roots/CPUs, CPU0 owner compositions, AP
   deferrals, pending handoff, and GPU delayed/timeouts/errors.

   The self-hosting gate must report
   `MAKOS_AARCH64_C_BRANCH_BLOCK_OK forms=if,if-else,nested-if,nested-loop body=bounded-control-assignment continuation=return max_depth=4 object=elf64-et-rel symbols=choose,bump,nested,accumulate linked=1 wx=denied results=42,2,5,8,42,2,1,6 malformed=empty-else,branch-declaration-denied,depth-5-denied`.
   Its final host marker must include
   `branch_blocks=if,if-else,nested-if,nested-loop`,
   `branch_block_body=bounded-control-assignment`, `branch_block_max_depth=4`,
   `branch_block_results=42,2,5,8,42,2,1,6`,
   `branch_block_object=elf64-et-rel`, and
   `malformed_branch_blocks=empty-else,branch-declaration-denied,depth-5-denied`.
   It must also report
   `MAKOS_AARCH64_C_SIX_FUNCTION_OK functions=6 calls=5` with ELF64 ET_REL
   emission, five `R_AARCH64_CALL26` relocations, one-object guest linking,
   W^X execution result 42, `max_functions=6`, and a denied seventh function.
   Its final host marker must include `max_functions_per_unit=6`,
   `six_function_calls=5`, and `six_function_result=42`. It must also report
   `MAKOS_AARCH64_C_SIX_ARGUMENT_OK parameters=6 call_arguments=6`, registers
   `x0-x5`, callee-saved `x23-x28`, frame 112, parsed ELF64 ET_REL object size
   808, one `R_AARCH64_CALL26`, direct and same-object-call results 42, and a
   denied seventh parameter/argument. The final host marker must retain
   `max_parameters=6`, `max_call_arguments=6`, `nonleaf_frame=96,112`, and
   `six_argument_object=elf64-et-rel:808`. It must also report the exact
   generated-header readback marker before any preprocessor success marker:
   `MAKOS_AARCH64_GENERATED_HEADERS_OK inline=/home/user/generated-inline.h inline_bytes=164 inline_fnv1a=bbbc9068d3d73e49 leaf=/home/user/generated-leaf.h leaf_bytes=1215 leaf_fnv1a=ccf73fc02c2f9ceb identity=guest-readback-exact`.
   The complete self-host output must not contain
   `MAKOS_AARCH64_GENERATED_HEADER_ERROR`,
   `MAKOS_AARCH64_C_INCLUDE_ERROR`, or
   `MAKOS_AARCH64_C_PREPROCESSOR_ERROR`. It must also report the exact
   quoted-header/preprocessor guard and dependency markers:
   `MAKOS_AARCH64_C_PREPROCESSOR_GUARD_OK headers=2 max_depth=2 macros=6 conditional_depth=2 include_guard=deduplicated missing=denied relative=denied cycle=denied overdepth=denied macro_expansion=text,function-like parameters=4 expansion_depth=8 if_expression=defined,numeric,arithmetic,shift,comparison,bitwise,not,and,or,short-circuit,conditional elif=selected malformed=define,endif,unterminated,duplicate-else,expression,elif-after-else,zero-divisor,shift-range,overflow,conditional-syntax,conditional-selected-trap,macro-parameters,macro-arity,macro-recursion,macro-token-op-denied depth_limit=4`
   and
   `MAKOS_AARCH64_C_HEADER_DEP_OK source=/home/user/generated-header.c root=/home/user/generated-inline.h leaf=/home/user/generated-leaf.h headers=2 max_depth=2 resolver=quoted-absolute-recursive depth_limit=4 preprocessor=bounded-macro-if-expressions macros=6 conditional_depth=2 macro_expansion=text,function-like parameters=4 expansion_depth=8 if_expression=defined,numeric,arithmetic,shift,comparison,bitwise,not,and,or,short-circuit,conditional elif=selected include_guard=deduplicated fingerprint=expanded-source`.
   Prove the separate two-input
   graph cold `0/2`, warm `2/0`, edited-header selective `1/1`, and rewarm
   `2/0`, and execute `/home/user/generated-header.elf` with status 42 through
   `MAKOS_AARCH64_RUN_OK`. The dependency marker must report root
   `/home/user/generated-inline.h`, leaf `/home/user/generated-leaf.h`, two
   headers, depth two, recursive absolute-quoted resolution, six macros,
   bounded text/function-like expansion with four parameters and depth eight,
   conditional depth two, selected `#elif`, the exact bounded expression
   feature set, include-guard deduplication, and an expanded-source
   fingerprint. Missing, relative, cyclic, and over-depth
   headers must remain denied at depth limit four; the final host marker must
   contain `runtime_graphs=4,3,2,2,3,2,2,3`,
   `invalidations=object,source,state,header`, and
   `header_dependency=quoted-absolute-recursive headers=2 max_depth=2 depth_limit=4 preprocessor=bounded-macro-if-expressions macros=6 conditional_depth=2 macro_expansion=text,function-like parameters=4 expansion_depth=8 if_expression=defined,numeric,arithmetic,shift,comparison,bitwise,not,and,or,short-circuit,conditional elif=selected include_guard=deduplicated fingerprint=expanded-source`
   plus
   `malformed_preprocessor=define,endif,unterminated,duplicate-else,expression,elif-after-else,zero-divisor,shift-range,overflow,conditional-syntax,conditional-selected-trap,macro-parameters,macro-arity,macro-recursion,macro-token-op-denied`.
   It must additionally prove the exact repository-source marker
   `MAKOS_AARCH64_REPOSITORY_SOURCE_OK c=user/aarch64_selfhost_probe.c asm=user/aarch64_selfhost_probe.S c_bytes=440 asm_bytes=53 c_fnv1a=5d0b854c29106f84 asm_fnv1a=7ad8871bd0e68af4 identity=build-generated-exact host_reference=compiled`.
   Require the separate `/home/user/makos-repo-probe.build` graph to report
   cold `0/2`, warm `2/0`, and
   `MAKOS_AARCH64_RUN_OK path=/home/user/makos-repo-probe.elf status=42`.
   The final host marker must contain `cli_builds=20`,
   `runtime_graphs=4,3,2,2,3,2,2,3`, and
   `repository_source=user/aarch64_selfhost_probe.c,user/aarch64_selfhost_probe.S c_bytes=440 asm_bytes=53 c_fnv1a=5d0b854c29106f84 asm_fnv1a=7ad8871bd0e68af4 identity=build-generated-exact host_reference=compiled guest_execution=42`.
   The eight graph input counts are the current four-input primary graph,
   three-input graph, two-input quoted-header graph, two-input exact
   repository-source graph, three-input production global-data graph,
   two-input const-only graph, two-input mutable-only graph, and three-input
   nested-control graph. Require
   `MAKOS_AARCH64_C_GLOBAL_DATA_OK source=/usr/src/makos/ports/musl/shared-demo.c`
   with the exact read-only source and `/usr/include/stdint.h`, denied
   write/truncate, `.text,.rodata,.data`, `STT_FUNC,STT_OBJECT`, paired
   `R_AARCH64_ADR_PREL_PG_HI21,R_AARCH64_ADD_ABS_LO12_NC` relocations,
   `R-X,R--,RW-NX`, and rejection of malformed relocation pairs, unresolved
   data, duplicate data, and out-of-range data. The production graph must
   cold-build `0/3`, warm-reuse `3/0`, reap its CLI process with status 42,
   then execute `/home/user/makos-shared-demo.elf` through the ordinary loader
   with status 42. The const-only and mutable-only two-input graphs must each
   cold-build `0/2`, reap with status 42, and execute through the ordinary
   loader with status 42. The final host marker must retain
   `production_source=/usr/src/makos/ports/musl/shared-demo.c`,
   `source_identity=exact-read-only`, `header=/usr/include/stdint.h`,
   `global_data=rodata,rwdata`, `segments=R-X,R--,RW-NX`, and `execution=42`.
   It must also prove the nested graph's cold `0/3`, warm `3/0`, two identical
   `MAKOS_AARCH64_MAKBUILD_OUTPUT_OK` records with `linked_bytes=564`,
   `output_bytes=1583`, `linked_capacity=1024`, `image_capacity=2048`, and
   `data_offset=1536`, followed by
   `MAKOS_AARCH64_RUN_OK path=/home/user/generated-nested.elf status=42`.
   In the fixed parallel phase, require exactly one complete, contiguous cold
   build record for each of `/home/user/generated-three.build` (`0/3`),
   `/home/user/generated-header.build` (`0/2`), and
   `/home/user/generated-nested.build` (`0/3`), plus exactly one complete
   header-dependency record and nested-output record. Do not accept fragmented
   substrings or infer cold-cache evidence only from exit status. Require
   `MAKOS_AARCH64_MAKBUILD_PARALLEL_OK spawn_before_wait=3
   statuses=42,42,42`, exactly three matching status-42 reaps, and exactly one
   `MAKOS_AARCH64_TOOLCHAIN_PARALLEL_OK` with three distinct PIDs/group PIDs,
   three distinct nonzero TTBR0 roots, CPUs `1,2,3`, `cpu_mask=0xe`, running
   state, singleton ownership, scheduler-lock snapshot evidence, and CPU0
   emission.
   It must prove kernel-owned SMP placement for all 21 real Toolchain
   processes. Require exactly 21
   `MAKOS_AARCH64_TOOLCHAIN_PLACEMENT_OK` decisions, each with singleton AP
   affinity, the selected AP at the minimum recorded load, an idle AP selected
   whenever `idle_mask` is nonzero,
   `policy=least-dispatched-idle-ap caller_selected=0`, and CPU0 device
   ownership. Require dispatch markers covering AP1, AP2, and AP3. Require
   nonzero `MAKOS_AARCH64_TOOLCHAIN_MIGRATION_OK` records emitted by CPU0 only
   after child exit. Every record must move between distinct singleton AP
   affinities, select an idle target, prove source load is at least eight
   dispatches above target load, retain GPR/SP/TLS/SIMD context and
   Ready/unowned source publication, use SGI wake, preserve exclusive
   ownership, and report no caller-selected affinity. The final
   `MAKOS_AARCH64_TOOLCHAIN_SMP_OK` must report `cpu_mask=0xe`, 21 total
   placements with every AP nonzero, every AP dispatch count nonzero,
   nonzero migrations, nonzero source/target masks contained in `0xe`,
   `migration_policy=timer-safe-dispatch-imbalance migration_delta=8`, and
   `migration_evidence_drops=0`,
   `leader=ap kernel_placement=least-dispatched-idle caller_selected=0`,
   exclusive ownership, and
   `console_gpu_handoff=ap-defer,cpu0-compose` with positive owner compositions
   and AP deferrals, `pending=0`, and status 42. Require exactly 21 cumulative
   `MAKOS_AARCH64_TOOLCHAIN_SMP_OK` summaries, with the final summary covering
   all 21 processes. The final host marker must
   retain `toolchain_smp=kernel-least-loaded-ap`, `cpu_mask=0xe`,
   `processes=21`, the same migration count/masks/policy with zero evidence
   drops, `caller_selected=0`, `ownership=exclusive`,
   `device_mmio_owner=cpu0`, and the drained console/GPU handoff evidence.
   The Native gate must
   report all of the following without borrowing Firefox
   evidence: `MAKOS_AARCH64_NATIVE_SMP_RUNTIME_OK`, `cpu_mask=0xe`, nonzero
   dispatch counts on AP1/AP2/AP3, a live/final overlap match containing at
   least two distinct nonzero TIDs, automatic placements covering AP1/AP2/AP3,
   at least one `MAKOS_AARCH64_APPLICATION_MIGRATION_OK role=native` with a
   64-dispatch imbalance, Ready/unowned full-context migration, no caller CPU
   selection, zero evidence drops, kernel-owned affinity get/set/migration and
   restoration, explicit-affinity authority, `device_mmio_owner=cpu0`, and
   status 42. The same host marker must contain `builtin_role=python`, nonzero
   `python_dispatches` on AP1/AP2/AP3, Python placements covering AP1/AP2/AP3,
   at least one automatic Python-role migration, and `python_status=42`. Treat
   that as scheduler-role evidence, not proof that Python executed. The Firefox
   production gate must retain its exact-role,
   exact-group, surface-wake, IRQ, and CPU0 device-ownership assertions and add
   the equivalent `role=firefox` automatic-placement/migration proof before its
   explicit affinity phase. It must also report
   `MAKOS_AARCH64_SURFACE_MAIN_HANDOFF_ARM_OK` for the exact watcher/group,
   `MAKOS_AARCH64_SURFACE_MAIN_HANDOFF_READY_OK` with
   `source=watcher-post-enqueue wake=sgi bounded_ticks=1000`, the same watcher,
   group, and CPU0 leader, then `MAKOS_AARCH64_SURFACE_MAIN_DISPATCH_OK`. Its
   final marker must contain `handoff=watcher-post-enqueue-syscall:149`.
   Rejection or fallback-only evidence is not a pass. Both production gates
   must observe
   `MAKOS_AARCH64_PRODUCTION_SMP_READY userspace_scheduler_cpus=4 policy=interactive-leaders-cpu0,application-workers-shared-ap,toolchain-leaders-least-loaded-ap roles=firefox,native,python,toolchain device_mmio_owner=cpu0 wake=sgi block=ap-idle`.
   The cursor gate must retain seven positions,
   zero changed scanout pixels, the virtio-GPU cursor plane, hidden host
   cursor, `completion=fast-plus-bounded-recovery`, and zero GPU timeouts or
   errors. Every delayed completion, if any, must have a matching recovered
   record with the same queue and command.
4. Reuse the supported October 10 Mac release/integrated master:

       INTEGRATED_IMAGE=build/makos-firefox-evidence-ymp9Om/makos-integrated-5baa9467ca510b8a.img
       shasum -a 256 "$INTEGRATED_IMAGE"
       python3 scripts/verify_firefox_runtime_image.py "$INTEGRATED_IMAGE"

   Require exact image SHA-256
   `5baa9467ca510b8a1a207365abed12fe8b03f228432a80b2238b3b46042d00ce`
   and matching manifest SHA-256
   `4cb2b6ef3a7d4c4d852755ce33ebd0cbbbb4de5291b53a3d6dfc7a68fe2a6f6a`.
   The existing supported release/package markers report 61 patches with exact ordered
   series SHA-256
   `770b7659493b6b6545c64e37243f41982ede10f007f0f03a197aa552e40b3aab`,
   pinned Firefox 140.13.0esr source
   `90ad18aabeaa9cbd63a1f749a57f266e758e50da`, and all five audited build
   and stripped-runtime artifacts. Require the unchanged all-five-ELF,
   loaded-literal, hash and provenance preflight, not just an image hash.
   No Firefox source, patch, package or provenance change is part of this
   kernel repair; no release rebuild or integration is needed. Never restamp
   or transplant artifacts. The older 60-patch `3b68500032eec2d4` image remains
   historical evidence, not a substitute input.

   Preserve the current master, manifest, build/runtime provenance, release
   cache and all private account/profile/evidence disks. The previous private
   session was retained at
   `build/logs/macos-hvf-73da7fc65cbc-20261010/19-firefox-runtime-private/makos-aarch64-test-5lzp0ykz/`;
   its data SHA-256 is
   `3588374602d911b93975b6cd89ee9e3929d4d1b6cc3c6aed5ff6d92622ed08e1`.
   Do not boot forensic originals writable or edit their preferences. The
   integrated master preserves October 9 account/profile state; it is not
   recovery of the earlier lost account. A manifest or compact evidence ZIP
   cannot restore missing image bytes. If this exact master is absent or its
   hash/preflight fails, stop and report the input blocker. Do not silently
   create a different test baseline or run another historical image.

5. Run the strict real-Firefox gate only against that verified current image,
   only when `uptime`, `vm_stat`, `sysctl vm.swapusage`, and `memory_pressure`
   show an idle host with low memory pressure, and only after confirming no
   QEMU is running. Set `INTEGRATED_IMAGE` to the exact content-addressed path
   and run exactly:

       AARCH64_FIREFOX_PACKAGE_IMAGE="$INTEGRATED_IMAGE" make test-aarch64-firefox-runtime

   Do not add, remove, or override any gate variable. The Make target fixes a
   600-second Firefox probe, 90-second first navigation, first-character
   latency strictly below 500 ms, Ctrl-A latency strictly below 10,000 ms,
   120-second clipboard/link/document-selection windows, two sustained cycles,
   a 120-second sustained-navigation window, exact IANA link URI, and required
   real-Firefox SMP proof. It also retains the existing paint, exact-URI,
   TLS/HTTP, selection, scrolling, form, survival, CPU, RSS, and resident-page
   assertions. Do not weaken or reinterpret any of them.

   Before QEMU, the target must print `MAKOS_FIREFOX_RUNTIME_IMAGE_OK` with the
   pinned source, 61-patch series identity above,
   `artifacts=build-audited,runtime-sha256-matched`, and
   `elf=aarch64-pie,libxul
   all_five_elf=aarch64-et-dyn,interp-and-deps-by-kind`. Missing provenance,
   a malformed ELF, an old patch identity, or any
   packaged-runtime hash mismatch is a preflight refusal, not permission to
   bypass the check.

   Require `MAKOS_WIDGET_MAIN_HANDOFF_OK source=post-enqueue syscall=149`,
   `MAKOS_AARCH64_SURFACE_MAIN_HANDOFF_READY_OK`,
   `MAKOS_FIREFOX_SELECTION_LATENCY_OK`, `MAKOS_FIREFOX_INPUT_LATENCY_OK`,
   `MAKOS_FIREFOX_SUSTAINED_INTERACTION_OK`,
   `MAKOS_FIREFOX_SMP_OVERLAP_OK`, `MAKOS_FIREFOX_SMP_AUTOBALANCE_OK`, and
   `MAKOS_FIREFOX_GUEST_PROBE_OK`. Fallback-only handoff is a failure. The
   first-paint phase requires a registered executable pool, the actual browser
   document's successful WebRender submission, blit and client pixels before the first Firefox
   key; the post-enqueue syscall-149 markers are required only after the timed
   Ctrl-A produces raw 132. Do not move them back ahead of the event that
   creates them or move them after the latency decision. The
   overlap must belong to the launched Firefox PID, contain at least two
   distinct nonzero TIDs concurrently owning different APs, and retain
   exclusive ownership. Default placements must cover AP1/AP2/AP3. The same
   live Firefox group must make a kernel-owned 64-dispatch migration between
   distinct APs with GPR/SP/TLS/SIMD context, Ready/unowned publication,
   `caller_selected=0`, and explicit-affinity authority.

   If the unchanged idle-host run fails, preserve
   `build/firefox-runtime-latest-serial.log`, the complete harness output,
   every `build/makos-firefox-*.ppm`, timing/resource lines, package and image
   provenance, the first failing assertion, host load/memory/swap evidence,
   and every QMP/session/PID path. Record an absent required package as not run,
   not as a pass or runtime failure.

   On a fatal, preserve the final raw serial file after the harness exits,
   not just the exception's earlier snapshot. Include the complete
   MAKOS_FAILURE_DETAIL: record that precedes the unchanged MAKOS_FATAL:
   marker, any MAKOS_AARCH64_NET_TX_TIMEOUT record (CPU/slot/kind/state,
   length/device lock/owner-active and service counters), any
   MAKOS_AARCH64_DUPLICATE_MAPPING record (CPU/root/VA/candidate
   physical/existing entry), and the exact preceding thread/CPU/root records. Do not infer a
   missing reason, join fragments into successful evidence, retry after the
   first failure, or launch the final visible login. Capture the temporary
   session paths/PID/QMP descriptors while the harness is running. Before
   its normal cleanup, retain the exact private boot/data/vars files in a new
   evidence directory (same-filesystem hard links are acceptable); do not
   alter the harness or boot those forensic files. Hash retained files after
   QEMU exits. If retention fails, explicitly record missing bytes rather
   than claiming a manifest restores them. Preserve all master images/data.

   For any earlier self-host failure, preserve
   `build/makos-selfhost-focused-serial.log`, complete harness stdout/stderr,
   the first failing assertion, exact exit status, boot-image identity, host
   load/memory/swap evidence, every retained QMP/session/PID path, and raw bytes
   surrounding any missing or duplicated parallel marker. Do not reinterpret
   three child status-42 reaps as proof of a cold marker that is absent.

6. Only after strict Firefox passes and its QEMU has exited, confirm no QEMU remains, then boot
   the visible login as the sole QEMU from private clones. Do not start another
   runtime while it is active. Create the session and sparse clones without
   modifying either source image:

       SESSION=$(mktemp -d "$PWD/build/makos-macos-visible-firefox-XXXXXX")
       python3 - "$PWD/build/makos-aarch64.img" "$INTEGRATED_IMAGE" "$SESSION" <<'PY'
import pathlib
import sys
sys.path.insert(0, str(pathlib.Path.cwd() / "scripts"))
from boot_test_aarch64 import copy_sparse

boot, data, session = map(pathlib.Path, sys.argv[1:])
copy_sparse(boot, session / "boot.img")
copy_sparse(data, session / "data.img")
PY

   Launch the repository's supported visible Cocoa/HVF path and record its
   actual QEMU PID:

       MAKOS_AARCH64_BUILD_DIR="$SESSION" \
       MAKOS_AARCH64_QMP_SOCKET="$SESSION/qmp.sock" \
       ./scripts/run-qemu-aarch64.sh "$SESSION/boot.img" "$SESSION/data.img" \
         >"$SESSION/serial.log" 2>&1 &
       QEMU_PID=$!
       printf '%s\n' "$QEMU_PID" >"$SESSION/qemu.pid"

   Wait while verifying that PID remains alive until serial reports
   `MAKOS_LOGIN_UI_OK framebuffer=800x600` and
   `MAKOS_AARCH64_BOOT_OK ... desktop=login`. Query the private QMP socket and
   capture the login scanout with:

       python3 - "$SESSION/qmp.sock" "$SESSION/login.ppm" "$SESSION/qmp-status.json" <<'PY'
import json
import pathlib
import socket
import sys
sys.path.insert(0, str(pathlib.Path.cwd() / "scripts"))
from boot_test_aarch64 import qmp_command

qmp_path = pathlib.Path(sys.argv[1])
screenshot = pathlib.Path(sys.argv[2]).resolve()
record = pathlib.Path(sys.argv[3])
client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
client.settimeout(10)
client.connect(str(qmp_path))
with client, client.makefile("rwb", buffering=0) as stream:
    greeting = json.loads(stream.readline())
    capabilities = qmp_command(stream, "qmp_capabilities")
    status = qmp_command(stream, "query-status")
    capture = qmp_command(stream, "screendump", {"filename": str(screenshot)})
evidence = {
    "greeting": greeting,
    "capabilities": capabilities,
    "status": status,
    "screendump": capture,
}
record.write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n")
if any("error" in item for item in (capabilities, status, capture)):
    raise SystemExit("QMP visible-login evidence failed")
status_result = status.get("return")
if (
    not isinstance(status_result, dict)
    or status_result.get("status") != "running"
):
    raise SystemExit("QMP visible-login status is not running")
PY

   Require QMP status `running`. Record the absolute session path, PID, private `boot.img`,
   `data.img`, copied `edk2-arm-vars-makos.fd`, `qmp.sock`, `serial.log`,
   `qemu.pid`, `qmp-status.json`, `login.ppm`, source/integrated image
   identities, and SHA-256 of every image/capture. Confirm
   `pgrep -fl 'qemu-system|boot_test_aarch64'` shows exactly this one QEMU and
   no runtime harness.

   Leave that sole visible login running only for the requested user test and
   state that explicitly. Before any later runtime, connect to the recorded QMP
   socket, negotiate `qmp_capabilities`, send `quit`, wait for the recorded PID
   to exit, and confirm no QEMU remains. Do not use SIGKILL for an ordinary
   visible-login shutdown.

7. Return a concise report containing the tested commit hash and proof that the
   required baseline is its ancestor, clean/dirty status, QEMU/HVF and host
   evidence, every command and exit status, exact OK marker lines, current
   20-build/eight-graph/21-process self-host evidence, Firefox timing lines or
   exact preflight blocker, release/package/image identities, all artifact/log
   paths, and screenshot/image SHA-256 hashes. Include the visible-login
   PID/session/private-data/QMP record and explicitly state whether any QEMU
   remains running. Do not claim MakOS complete: the original-spec audit still
   contains Partial rows.
```

The Pi/TCG evidence establishes functionality, not macOS/HVF performance. A
successful unchanged strict Firefox run on an idle Mac is the required next
Firefox qualification result.
