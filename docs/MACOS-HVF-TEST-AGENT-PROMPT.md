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
Required implementation baseline: ffbce4f6179a7fef03c5cd2b32341ffa7498a7a0

Verify that the checked-out `main` contains this exact implementation commit.
Do not test an older commit. A later documentation-only handoff commit is
acceptable if this baseline is its ancestor.

1. Read AGENTS.md, docs/T3-CONTINUATION.md, docs/ORIGINAL-SPEC-AUDIT.md,
   docs/STATUS.md, docs/BUILD.md, and docs/INTEGRATED-DATA-IMAGE.md.
2. Record `git status --short --branch`, local HEAD, origin/main, remote main,
   macOS version, Apple chip/model, QEMU version, accelerator, CPU count, load
   average, free/used memory, swap/compressor state, and every QEMU process.
   Pull main only if the worktree is clean. Install QEMU with Homebrew if it is
   absent. Do not overwrite an existing data image.
3. Confirm no QEMU is running. Run these gates in order and save complete logs:

       make unit check
       make test-aarch64-selfhost-runtime
       make test-aarch64-native-smp-runtime
       make test-aarch64-production-smp-runtime
       make test-aarch64-cursor-runtime

   The self-hosting gate must report
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
   contain `runtime_graphs=4,3,2,2`,
   `invalidations=object,source,state,header`, and
   `header_dependency=quoted-absolute-recursive headers=2 max_depth=2 depth_limit=4 preprocessor=bounded-macro-if-expressions macros=6 conditional_depth=2 macro_expansion=text,function-like parameters=4 expansion_depth=8 if_expression=defined,numeric,arithmetic,shift,comparison,bitwise,not,and,or,short-circuit,conditional elif=selected include_guard=deduplicated fingerprint=expanded-source`
   plus
   `malformed_preprocessor=define,endif,unterminated,duplicate-else,expression,elif-after-else,zero-divisor,shift-range,overflow,conditional-syntax,conditional-selected-trap,macro-parameters,macro-arity,macro-recursion,macro-token-op-denied`.
   It must additionally prove the exact repository-source marker
   `MAKOS_AARCH64_REPOSITORY_SOURCE_OK c=user/aarch64_selfhost_probe.c asm=user/aarch64_selfhost_probe.S c_bytes=440 asm_bytes=53 c_fnv1a=5d0b854c29106f84 asm_fnv1a=7ad8871bd0e68af4 identity=build-generated-exact host_reference=compiled`.
   Require the separate `/home/user/makos-repo-probe.build` graph to report
   cold `0/2`, warm `2/0`, and
   `MAKOS_AARCH64_RUN_OK path=/home/user/makos-repo-probe.elf status=42`.
   The final host marker must contain `cli_builds=14`,
   `runtime_graphs=4,3,2,2`, and
   `repository_source=user/aarch64_selfhost_probe.c,user/aarch64_selfhost_probe.S c_bytes=440 asm_bytes=53 c_fnv1a=5d0b854c29106f84 asm_fnv1a=7ad8871bd0e68af4 identity=build-generated-exact host_reference=compiled guest_execution=42`.
   It must also prove kernel-owned SMP placement for all 15 real Toolchain
   processes. Require exactly 15
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
   `MAKOS_AARCH64_TOOLCHAIN_SMP_OK` must report `cpu_mask=0xe`, 15 total
   placements with every AP nonzero, every AP dispatch count nonzero,
   nonzero migrations, nonzero source/target masks contained in `0xe`,
   `migration_policy=timer-safe-dispatch-imbalance migration_delta=8`, and
   `migration_evidence_drops=0`,
   `leader=ap kernel_placement=least-dispatched-idle caller_selected=0`,
   exclusive ownership, and
   `console_gpu_handoff=ap-defer,cpu0-compose` with positive owner compositions
   and AP deferrals, `pending=0`, and status 42. The final host marker must
   retain `toolchain_smp=kernel-least-loaded-ap`, `cpu_mask=0xe`,
   `processes=15`, the same migration count/masks/policy with zero evidence
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
   status 42. The Firefox production gate must retain its exact-role,
   exact-group, surface-wake, IRQ, and CPU0 device-ownership assertions and add
   the equivalent `role=firefox` automatic-placement/migration proof before its
   explicit affinity phase. Both production gates must observe
   `MAKOS_AARCH64_PRODUCTION_SMP_READY userspace_scheduler_cpus=4 policy=interactive-leaders-cpu0,application-workers-shared-ap,toolchain-leaders-least-loaded-ap roles=firefox,native,toolchain device_mmio_owner=cpu0 wake=sgi block=ap-idle`.
   The cursor gate must retain seven positions,
   zero changed scanout pixels, the virtio-GPU cursor plane, and hidden host
   cursor.
4. Run the strict real-Firefox gate only when the host is idle and memory
   pressure is low, and only if the exact integrated image and staged Firefox
   package required by the Make target are present. The currently documented
   image is `build/makos-integrated-a9c604254f094de2.img`; verify its identity
   from the repository documentation and target preflight. Do not substitute a
   different image or Pi/TCG timing. Run exactly:

       make test-aarch64-firefox-runtime

   Do not weaken the 10,000 ms Ctrl-A limit or any paint, first-input,
   navigation, clipboard, selection, scrolling, form, CPU, RSS, resident-page,
   survival, exact-URI, TLS/HTTP, multi-TID overlap, automatic-placement, or
   load-migration assertion. If preflight
   reports that the required image/package is absent, record that as not run,
   not as a failure or pass. If the unchanged idle-host run fails, preserve the
   serial log, harness output, QMP/session paths, screenshots, timing evidence,
   host load/memory evidence, and the first failing assertion for scheduler or
   input-wake diagnosis.
5. Return a concise report containing the tested commit hash, clean/dirty
   status, QEMU/HVF and host evidence, every command and exit status, exact OK
   marker lines, Firefox timing lines or exact preflight blocker, artifact/log
   paths, and any screenshots with SHA-256 hashes. Explicitly state whether
   any QEMU remains running. Do not claim MakOS complete: the original-spec
   audit still contains Partial rows.
```

The Pi/TCG evidence establishes functionality, not macOS/HVF performance. A
successful unchanged strict Firefox run on an idle Mac is the required next
Firefox qualification result.
