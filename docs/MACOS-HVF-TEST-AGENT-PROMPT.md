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

1. Read AGENTS.md, docs/T3-CONTINUATION.md, docs/ORIGINAL-SPEC-AUDIT.md,
   docs/STATUS.md, docs/BUILD.md, and docs/INTEGRATED-DATA-IMAGE.md.
2. Record `git status --short --branch`, local HEAD, origin/main, remote main,
   macOS version, Apple chip/model, QEMU version, accelerator, CPU count, load
   average, free/used memory, swap/compressor state, and every QEMU process.
   Pull main only if the worktree is clean. Install QEMU with Homebrew if it is
   absent. Do not overwrite an existing data image.
3. Confirm no QEMU is running. Run these gates in order and save complete logs:

       make unit check
       make test-aarch64-native-smp-runtime
       make test-aarch64-production-smp-runtime
       make test-aarch64-cursor-runtime

   The Native gate must report all of the following without borrowing Firefox
   evidence: `MAKOS_AARCH64_NATIVE_SMP_RUNTIME_OK`, `cpu_mask=0xe`, nonzero
   dispatch counts on AP1/AP2/AP3, a live/final overlap match containing at
   least two distinct nonzero TIDs, kernel-owned affinity get/set/migration and
   restoration, `device_mmio_owner=cpu0`, and status 42. The Firefox production
   gate must retain its exact-role, exact-group, surface-wake, IRQ, and CPU0
   device-ownership assertions. The cursor gate must retain seven positions,
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
   survival, exact-URI, TLS/HTTP, or multi-TID overlap assertion. If preflight
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
