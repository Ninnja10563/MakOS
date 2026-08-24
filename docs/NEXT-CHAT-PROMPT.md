# MakOS next-chat prompt

Continue MakOS from `/Users/marcushuang/Documents/Codex/2026-08-12/mak` and
`docs/T3-CONTINUATION.md`. First read `docs/ORIGINAL-SPEC-AUDIT.md`,
`docs/STATUS.md`, `docs/BUILD.md`, and original specification at
`/Users/marcushuang/.codex/attachments/4ef18f50-cd19-419b-93ed-d509edff0836/pasted-text.txt`.

Preserve current work. Repository is `https://github.com/Ninnja10563/MakOS.git`,
branch `main`; verify `git status`, local/remote HEAD, source/test state, and all
QEMU/process state before acting. Never restart project, overwrite user changes,
run concurrent QEMU, or claim full completion while audit has Partial/Missing rows.

Current verified milestone: GitHub import `346b0df`; current-source cursor runtime
passes seven positions with zero scanout pixel changes, virtio-GPU cursor plane,
host cursor hidden. Typed IPC is complete at core scope: 12/12 unit tests,
structural guard, full `make unit && make check`, and isolated full HVF marker
`MAKOS_AARCH64_TYPED_IPC_RUNTIME_OK service=same-domain fifo=1 transfer=attenuated cleanup=process-exit-before-reap`.
No QEMU was running at handoff.

Highest priority: rerun unchanged `make test-aarch64-firefox-runtime` only when
no visible QEMU runs and host load/memory pressure is low. Two latest runs painted
in 248584/255543 ms but missed unchanged Ctrl-A bound at 10971/14363 ms while host
load was 7.66 with 163 MiB free and 6.6 GiB compressed. Do not weaken thresholds.
If idle-host rerun still fails, diagnose scheduler/input wake path from evidence.
Then boot a meaningful visible login milestone for user testing and record its
PID/session/private data clone/QMP in handoff.

After Firefox, advance highest-impact verified Partial/Missing audit requirement,
preferably AArch64 userspace SMP scheduling or first genuine guest self-hosting
step. Implement real behavior, add proportionate tests, run relevant unit/static/
runtime gates, update audit/status/handoff precisely, commit, and push `main`.
