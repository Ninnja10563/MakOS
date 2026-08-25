# MakOS next-chat prompt

Continue MakOS from `/Users/marcushuang/Documents/Codex/2026-08-12/mak` and
`docs/T3-CONTINUATION.md`. First read `docs/ORIGINAL-SPEC-AUDIT.md`,
`docs/STATUS.md`, `docs/BUILD.md`, and original specification at
`/Users/marcushuang/.codex/attachments/4ef18f50-cd19-419b-93ed-d509edff0836/pasted-text.txt`.

Preserve current work. Repository is `https://github.com/Ninnja10563/MakOS.git`,
branch `main`; verify `git status`, local/remote HEAD, source/test state, and all
QEMU/process state before acting. Never restart project, overwrite user changes,
run concurrent QEMU, or claim full completion while audit has Partial/Missing rows.

Current verified milestones: cursor runtime passes seven positions with zero
scanout pixel changes, virtio-GPU cursor plane, and host cursor hidden. Typed IPC
passes 12/12 unit tests, structural guard, full unit/check, and isolated full HVF
runtime. The guest-native AArch64 toolchain now reads a valid multi-statement C
assembly startup and two C functions from MakFS, compiles parameterized
arithmetic, register locals, mutable parameter/local assignments,
equality/inequality control flow, a backward-branch `while`, a 96-byte non-leaf
frame, bounded local address-of/dereference memory loads/stores, typed `int *`
parameters, and a cross-object call that mutates caller-owned stack memory,
and emits/persists/reopens three genuine
ELF64 `ET_REL` objects. Its bounded linker resolves `_start`→`answer`→`adjust`,
applies two `R_AARCH64_CALL26` relocations, rejects malformed C, relocation,
unresolved-symbol and duplicate-definition inputs, emits an 815-byte `ET_EXEC`,
executes both linked branch paths plus direct loop and pointer/pointee-memory
outcomes,
and runs the final
ELF twice with status 42 in focused Pi/QEMU TCG runtime.
Full unit/check and release artifact checks pass. This remains a bounded compiler
seed, not a general C toolchain or substantial self-hosted build.
Verify GitHub HEAD rather than relying on a copied hash.

One visible Pi/QEMU TCG login milestone is running at handoff: PID 378165, VNC
`127.0.0.1:5901`, session `build/makos-pi-visible-selfhost-pointer-param-4xT0zGVU`, private
boot/data/vars in that session, and QMP
`build/makos-pi-visible-selfhost-pointer-param-4xT0zGVU/qmp.sock`. Stop it through QMP before
any runtime test; never run concurrent QEMU.

Highest priority: rerun unchanged `make test-aarch64-firefox-runtime` only when
no visible QEMU runs and host load/memory pressure is low. Two latest runs painted
in 248584/255543 ms but missed unchanged Ctrl-A bound at 10971/14363 ms while host
load was 7.66 with 163 MiB free and 6.6 GiB compressed. Do not weaken thresholds.
If idle-host rerun still fails, diagnose scheduler/input wake path from evidence.
After any runtime work, boot a meaningful visible login milestone for user
testing and record its PID/session/private data clone/QMP in handoff.

After Firefox, advance highest-impact verified Partial/Missing audit requirement,
preferably AArch64 userspace SMP scheduling or the next genuine guest compiler/
linker/build step. Implement real behavior, add proportionate tests, run relevant unit/static/
runtime gates, update audit/status/handoff precisely, commit, and push `main`.
