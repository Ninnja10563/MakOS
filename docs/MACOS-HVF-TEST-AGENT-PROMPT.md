# MakOS macOS/HVF milestone test-agent prompt

Copy the prompt below to the agent that will test this milestone on the target
Mac. Do not relax, skip, or reinterpret a failed threshold.

```text
Test the current MakOS main branch on an idle Apple Silicon macOS host using
AArch64 QEMU with HVF. This is qualification work only: preserve repository
changes and test data, do not change source or thresholds, do not run two QEMU
instances concurrently, and stop every visible guest through QMP before the
next runtime.

Repository: https://github.com/Ninnja10563/MakOS.git
Branch: main
Required implementation baseline: da53d35a39b968887d5810780f4f9a6cd249b4da

Verify that the checked-out `main` contains this exact implementation commit.
Do not test an older commit. A later documentation- or test-only handoff commit
is acceptable if this baseline is its ancestor.

1. Read AGENTS.md, docs/T3-CONTINUATION.md, docs/ORIGINAL-SPEC-AUDIT.md,
   docs/STATUS.md, docs/BUILD.md, and docs/INTEGRATED-DATA-IMAGE.md.
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
       make test-aarch64-selfhost-runtime
       make test-aarch64-native-smp-runtime
       make test-aarch64-production-smp-runtime
       make test-aarch64-cursor-runtime

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
4. The historical `build/makos-integrated-a9c604254f094de2.img` predates
   Firefox patch `0059` and is not valid for this increment. A developer build
   is compile/link evidence only and cannot be packaged. With
   `MAKOS_FIREFOX_DEVELOPER_BUILD` unset, run the supported release path, never
   a bare make/relink inside the object directory:

       ports/firefox/clone.sh
       ports/musl/build-makos.sh
       ports/libcxx/build-makos.sh
       ports/rust/build-std.sh
       ports/firefox/test-widget.sh
       env -u MAKOS_FIREFOX_DEVELOPER_BUILD ports/firefox/build-makos.sh -j1

   Set the documented Python 3.11/3.12 and `MAKOS_*` LLVM/libclang variables
   when the tools are not installed in their default Homebrew locations. The
   supported wrapper must print `MAKOS_FIREFOX_HOST_TOOLS_OK` for a complete
   runnable host C/C++ pair and cbindgen 0.27.0 or newer before the expensive
   build. If a release cache was moved under the developer object directory,
   allow only the wrapper's journaled, recoverable
   `makos-moved-cargo-quarantine` operation. Do not delete or manually move
   Cargo or C++ caches.

   The full release must print
   `MAKOS_FIREFOX_BINARY_OK target=aarch64-unknown-makos
   elf=firefox,plugin-container,xpcshell,libxul gecko=linked nss=linked
   runtime=shared-musl interp=/lib/ld-musl-aarch64.so.1` and
   `MAKOS_FIREFOX_BUILD_OK developer=0`. It must then create the canonical
   release build stamp for Firefox 140.13.0esr source commit
   `90ad18aabeaa9cbd63a1f749a57f266e758e50da`. The release build/package
   markers must report 59 patches with exact ordered series SHA-256
   `c922d619398e64b6a162046efde105bc19152a9d868e9a2254ffa701874cc974`,
   and five audited build hashes. Do not manually create, copy from another
   build, or edit the provenance record.

   Set `SOURCE_DATA_IMAGE` to the intended existing MakOS data image. Never
   overwrite it. Build the package and a new content-addressed clone with:

       test -f "$SOURCE_DATA_IMAGE"
       make integrated-data-aarch64 SOURCE_DATA_IMAGE="$SOURCE_DATA_IMAGE"

   Require `MAKOS_FIREFOX_PACKAGE_OK` and `MAKOS_INTEGRATED_DATA_OK`. Record
   the new `build/makos-integrated-<identity>.img`, its matching
   `.manifest.json`, their SHA-256 values, preserved-region identities, five
   exact stripped runtime hashes, and package/image semantic identity. If any
   release, package, or integration prerequisite fails, return the exact
   blocker and do not run the historical image.

5. Run the strict real-Firefox gate only against that newly generated image,
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
   pinned source, 59-patch series identity above,
   `artifacts=build-audited,runtime-sha256-matched`, and
   `elf=aarch64-pie,libxul`. Missing provenance, an old patch identity, or any
   packaged-runtime hash mismatch is a preflight refusal, not permission to
   bypass the check.

   Require `MAKOS_WIDGET_MAIN_HANDOFF_OK source=post-enqueue syscall=149`,
   `MAKOS_AARCH64_SURFACE_MAIN_HANDOFF_READY_OK`,
   `MAKOS_FIREFOX_SELECTION_LATENCY_OK`, `MAKOS_FIREFOX_INPUT_LATENCY_OK`,
   `MAKOS_FIREFOX_SUSTAINED_INTERACTION_OK`,
   `MAKOS_FIREFOX_SMP_OVERLAP_OK`, `MAKOS_FIREFOX_SMP_AUTOBALANCE_OK`, and
   `MAKOS_FIREFOX_GUEST_PROBE_OK`. Fallback-only handoff is a failure. The
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

6. After the strict Firefox QEMU has exited, confirm no QEMU remains, then boot
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
