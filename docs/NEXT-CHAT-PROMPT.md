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
runtime. The guest-native AArch64 toolchain now reads a valid multi-statement
assembly startup and two C sources containing three functions from MakFS, compiles parameterized
arithmetic, register locals, mutable parameter/local assignments,
signed equality/inequality/ordering control flow, a backward-branch `while`, a 96-byte non-leaf
frame preserving x19-x24, bounded local address-of/dereference memory loads/stores,
one or two independently typed `int`/`int *` parameters and call arguments in
AAPCS64 x0/x1, fixed local `int` arrays, checked constant indexing, array decay,
bounded scaled `pointer-or-array + constant-or-scalar` expressions in pointer
initializers/calls, signed `SXTW #2` dynamic offsets, parenthesized
derived-pointer loads/stores, typed signed pointer difference, known-bound
one-past-end/unproved-variable and pointer-minus-scalar rejection, and a
same-object `answer`→`adjust(values + 1, 1)` call that mutates caller-owned
array elements using its second parameter, followed by an external
`adjust`→`combine` call. It compiles `answer` and `adjust` into a 976-byte
multi-definition object, `combine` into a separate 616-byte library object,
and independent `helper` into a 608-byte object, then persists/reopens those
plus the 688-byte assembly object. It reads a
versioned `/home/user/generated.build` whose bounded absolute
source/object/output paths and entry symbol drive the complete build. The
driver accepts two through six inputs (one leading assembly plus C inputs),
and rejects six malformed manifests. Authenticated `makbuild <manifest>` passes a
kernel-validated home path through a real child-owned SysV `argv[1]` and
rebuilds from existing MakFS files without fixture seeding; focused Pi/TCG
runtime executes/reaps one `MODE=fixture` process and eight `MODE=build`
processes across four- and three-input graphs with status 42. Its bounded
linker resolves external `_start`→`answer`, same-object `answer`→`adjust`, and
external `adjust`→`combine`,
applies three `R_AARCH64_CALL26` relocations, rejects malformed C, relocation,
unresolved-symbol, missing-library, and duplicate-definition inputs, rejects duplicate/over-limit
parameters and over-limit calls, rejects a fourth translation-unit function,
emits 500 linked bytes in an 815-byte `ET_EXEC`, directly executes
`helper(40)=42`,
executes both linked branch paths plus direct delta-1/delta-2 loop, exact
indexed-array outcomes (`41:42:0`, `42:0:44`, and `1:2:0`), all four signed
ordering relations, a `pointer + -1` load, and pointer differences `3`/`-3`,
and runs the final
ELF twice with status 42 in focused Pi/QEMU TCG runtime.
Build mode now persists a state-last 120-byte `MAKSTATE2` cache containing the
actual input count and six slots each of non-cryptographic source/object
fingerprints plus the manifest fingerprint; a cached object also must pass ELF
parsing and symbol validation. Focused runtime proves four-input cold `0/4`,
warm `4/0`, corrupt-object `3/1`, rewarm `4/0`, edited-source `3/1`, rewarm
`4/0`, corrupt-state full `0/4`, and separate three-input cold `0/3`/warm `3/0`
results across eight authenticated CLI builds, each linked, executed, and
reaped with status 42.
Full unit/check and release artifact checks pass. This remains a bounded compiler
seed, not a general C toolchain, transitive dependency/arbitrary-graph build
system, or substantial self-hosted build.
Verify GitHub HEAD rather than relying on a copied hash.

Firefox input scheduling now has a deterministic production-role Pi/TCG proof.
A real non-leader upstream-musl pthread blocks in surface syscall 140, QMP
Ctrl-A dispatches that exact watcher on AP1-3, and the group leader receives a
one-shot CPU0 handoff. The final run reports watcher TID 8 on AP2,
`cpu_mask=0xe`, `overlap_mask=0x6`, dispatches `9826,11253,9695`, and status 42
after the complete pthread/typed-IPC workload. The stale deadline no longer
acts as a priority time slice, fixing observed fork-child starvation. Full
`make unit check` passes. This is functional Pi/TCG evidence only; strict
Firefox timing still needs the unchanged idle macOS/HVF gate.

One visible Pi/QEMU TCG login milestone is running at handoff: PID 549619, user
service `makos-visible-firefox-input-affinity-final.service`, VNC
`127.0.0.1:5901`, session `build/makos-pi-visible-firefox-input-affinity-final-WGpssi0N`, private
boot/data/vars in that session, and QMP
`build/makos-pi-visible-firefox-input-affinity-final-WGpssi0N/qmp.sock`. Its
boot SHA-256 matches `build/makos-aarch64.img` at
`d52ef39b7d81d783f8093c0dbfe58eba001c8262709f204d11542e1aa710edd1`;
the inspected 800x600 login PNG is
`133b58664eaaeffb0a255ddb580ad09384db6334edc8612d2e6e3691bcd5ff4f`.
Stop it through QMP before
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
