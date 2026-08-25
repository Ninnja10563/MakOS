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
assembly startup and three C sources containing four functions from MakFS, compiles parameterized
arithmetic including unary negation and signed division/remainder, register
locals, mutable parameter/local assignments,
signed equality/inequality/ordering control flow, a backward-branch `while`, a 96-byte non-leaf
frame preserving x19-x24, bounded local address-of/dereference memory loads/stores,
up to three independently typed `int`/`int *` parameters and call arguments in
AAPCS64 x0-x2, with conditional x25 preservation for the third parameter,
fixed local `int` arrays, checked constant indexing, array decay,
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
`helper(40)=42`, and separately compiles `sum3(int,int,int)` plus
`invoke3(int)` into 140 bytes and a parsed 752-byte `ET_REL`, resolves its
same-object `CALL26` with entry offset 80, and executes both as 42,
emits a separate 168-byte three-definition arithmetic unit in a parsed 784-byte
`ET_REL` and executes signed division `6`/`-6`, remainder `2`/`-2`, and unary
negation `-42`/`42` while rejecting direct literal-zero divisors,
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
A target and decoy upstream-musl pthread block on distinct handles in surface
syscall 140. QMP Ctrl-A selects handle 7/TID 8 on AP2, wakes exactly one surface
watcher while three unrelated surface waiters remain blocked, and the group
leader receives a one-shot CPU0 handoff. The decoy wakes only when surface 8 is
destroyed, then its retry fails closed and joins. The final run reports
`cpu_mask=0xe`, `overlap_mask=0x6`, dispatches `9954,9924,11186`, and status 42
after the complete pthread/typed-IPC workload. Target syscall 148/feature bit
22 now owns real per-thread CPU masks; official-musl patch 65 translates
`sched_getaffinity`/`sched_setaffinity`. The fixture verifies the CPU0 leader,
forces and reads back three worker migrations through singleton AP masks, then
restores mask `0xe` before all joins. The stale deadline no longer
acts as a priority time slice, fixing observed fork-child starvation. Full
`make unit check` passes. Input delivery now uses genuine QEMU `virt` GICv2
SPIs: slots 29/30 map to INTIDs 77/78, are edge-rising and CPU0-targeted, and
lower-EL IRQs drain directly before EOI. EL1 IRQs acknowledge/defer to the
unchanged 100 Hz recovery poll. The latest focused run proves QMP Ctrl-A on
keyboard INTID 78, direct lower-EL dispatch, target TID 8 on AP1, one wake and
three skips, `cpu_mask=0xe`, dispatches `9557,10824,9509`, `overlap_mask=0xa`
with TIDs 5/6, and status 42. Focused cursor regression also passes seven
positions with zero changed scanout pixels. This is functional Pi/TCG evidence
only; strict Firefox timing still needs the unchanged idle macOS/HVF gate.

Virtio-net receive now also has a real device-interrupt path. QEMU `virt` slot
28 maps to CPU0-only GICv2 SPI INTID 76. A focused AP1 DNS waiter plus a
syscall-free CPU0 EL0 loop proves lower-EL direct RX (`frames=1`), AP wake by
SGI, exact status 63, and frame balance; EL1 entries acknowledge/defer and the
100 Hz pump is recovery-only. The combined network/input runtime, production
Firefox-role regression, cursor runtime, AArch64 release/artifact checks, and
full `make unit check` pass on Pi/TCG. The audit remains Partial.

One visible Pi/QEMU TCG login milestone is running at handoff: PID 668793, user
service `makos-visible-network-irq-final.service`, VNC
`127.0.0.1:5901`, session `build/makos-pi-visible-network-irq-final-D6pbvEPD`, private
boot/data/vars in that session, and QMP
`build/makos-pi-visible-network-irq-final-D6pbvEPD/qmp.sock`. Its
boot SHA-256 matches `build/makos-aarch64.img` at
`a4f5d6f697730482d3182bc79abbf049b384cb79ac24fb93c7c9f39245c1d67d`;
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
