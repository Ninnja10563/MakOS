# Build and test

## Supported host path

Apple Silicon macOS uses native ARM QEMU to emulate full-featured x86_64 MakOS
through TCG. HVF cannot accelerate a guest ISA different from host ISA. Native
AArch64 path uses HVF and avoids cross-ISA translation. Interactive runs attach
`build/makos-data-aarch64.img` through virtio-blk and enable Cocoa
`zoom-to-fit` for live host-window scaling.
Data images have 1 GiB virtual size but remain sparse; unused MakFS4 capacity
does not consume 1 GiB host storage. Run scripts extend older images in place.
For a final image retaining an existing account and Firefox profile while
refreshing Firefox, GNU nano, ncurses, and CPython, see
`docs/INTEGRATED-DATA-IMAGE.md`.

```sh
brew install qemu
rustup target add x86_64-unknown-none x86_64-unknown-uefi \
  aarch64-unknown-none aarch64-unknown-uefi
make
make test
make run
make image-x86_64-gpt
make test-x86_64-gpt
make run-x86_64-gpt
make test-x86_64-install
make test-aarch64-cursor-runtime
make test-aarch64-firefox-runtime
make test-aarch64-ipv6-runtime
make test-aarch64
make run-aarch64
make image-aarch64-gpt
make test-aarch64-gpt
make run-aarch64-gpt
make test-aarch64-install
```

Both architecture GPT targets build one sparse disk with protective MBR, redundant GPT
headers/entry arrays, a 64 MiB EFI System Partition, and an aligned 1 GiB MakOS
data partition. Kernel validates GPT CRCs and the MakOS partition type before
offsetting all filesystem I/O. A disk without a protective MBR retains legacy
raw-data behavior; a protective MBR with invalid primary and backup GPT is
rejected rather than treated as raw storage.

x86_64 GPT uses secondary ATA master; guest installer selects secondary slave
only after administrator login and exact `install disk1 erase-disk1`. Target
must be equal-sized and wholly blank. `install disk1 resume-disk1` accepts only
blank MBR plus zero/source-identical sectors. Shared host tests prove refusal,
resume conflicts, MBR-last commit, and interrupted-copy unbootability. QEMU
install harness SIGKILLs QEMU after the first verified payload block, checks
blank LBA0 plus source-identical partial sectors, resumes, checks full-image
SHA-256 equality, then proves source-detached two-boot persistence.

`make test-aarch64-install` exercises guest-side installation, not a host image
copy shortcut. It creates temporary live/blank virtio-blk disks, enters exact
Terminal confirmation `install disk1 erase-disk1`, SIGKILLs QEMU after first
verified payload progress, and proves LBA0 remains blank while every nonzero
partial block matches source. It reboots with exact `install disk1
resume-disk1`, verifies final source/target SHA-256 equality and MBR-last
commit, then removes live source and boots installed target twice. Installer
flushes disk0 and freezes all source writes across copy/commit, thawing on
error and retaining freeze until shutdown on success. Wrong token,
nonblank/committed/conflicting media, unequal geometry, and absent second disk
are refusal gates. Test never attaches or modifies
`build/makos-data-aarch64.img` or a currently running interactive disk.

`make test-makfs4-guest-fsck` runs the full two-boot AArch64 workload, exports
its data image only after QEMU exits, checks that quiescent real guest volume
with `makos-makfs4-fsck`, then removes the temporary volume.

`make release` runs full two-boot suite, regenerates blank distributable data
disk, then writes deterministic images/source archive/framebuffer PNG and
`SHA256SUMS` under `outputs/`.

Boot tests copy both disk images into an invocation-private temporary directory.
This prevents stale/parallel QEMU processes from locking or mutating build
artifacts. Fixture data seeds one legacy dynamic record; boot 1 migrates it,
while boot 2 injects primary-superblock and bitmap corruption before recovery.
Focused `test-aarch64-cursor-runtime` uses a fresh private sparse data disk,
moves the guest cursor through seven positions, and requires zero changed
virtio-GPU scanout pixels. QEMU still renders the separate guest cursor plane.
Focused `test-aarch64-firefox-runtime` requires the integrated Firefox package,
strict paint/input/TLS/exact-URI/page-pixel proof, then copies the selected URL
through the MakOS system clipboard, clears the URL bar, pastes, and requires a
second exact-URI page completion. It then left-clicks the real `example.com`
document link and requires exact `https://www.iana.org/help/example-domains`
completion, built-in-root TLS, Firefox pointer routing, and changed page pixels.
The Make target also requires a kernel-published live interval in which at
least two distinct, nonzero TIDs from the launched Firefox process group own
different guest APs simultaneously. Missing, single-CPU, stale-group, or
duplicate-TID evidence fails the run. This adds SMP proof without changing the
existing paint, Ctrl-A, first-input, navigation, interaction, resource, or
survival thresholds.
Focused `test-aarch64-ipv6-runtime` runs the full two-boot workload and adds
post-login proof for validated RA/SLAAC, AF_INET6 sockaddr ABI, NDP resolution,
and checksum-valid UDPv6 transmit. QEMU user networking does not return UDPv6
DNS in this host configuration, so TCPv6 and UDPv6 receive remain separate
open gates.

`OVMF_CODE=/path/to/OVMF_CODE.fd` overrides firmware discovery.
`QEMU_SYSTEM_X86_64=/path/to/qemu-system-x86_64` overrides QEMU discovery.
`AAVMF_CODE`, `AAVMF_VARS`, and `QEMU_SYSTEM_AARCH64` override AArch64 tools.
`MAKOS_QEMU_DATA_DIR` supplies QEMU's `-L` data directory to focused AArch64
runtimes when using an extracted, non-system QEMU installation. Such a modular
Debian package may also require `LD_LIBRARY_PATH` to its extracted architecture
library directory and `QEMU_MODULE_DIR` to the adjacent `qemu` module directory;
otherwise the binary can start but fail to resolve libraries or
`virtio-gpu-device`.
On a host using the repository's extracted LLVM, prepend its `bin` directory
to `PATH` and set `MAKOS_NM`, `MAKOS_REAL_CLANG`, `MAKOS_REAL_CLANGXX`, and
`MAKOS_LLD` to the matching pinned tools. The Firefox driver test scopes its
own `MAKOS_CC` wrapper to the Firefox audit, so it cannot replace the bare-metal
compiler used by MicroPython during the same `make unit check` run.

After login, `selfhost-aarch64` runs the deterministic guest-native
compiler/assembler/static-linker gate. Its fixture mode writes an A64 startup to
`/home/user/generated.s` and valid C to
`/home/user/generated-program.c`, `/home/user/generated-library.c`, and
`/home/user/generated-helper.c`, then rereads all four from MakFS. It also
writes and rereads the build description
`/home/user/generated.build`:

```text
MAKBUILD1
asm /home/user/generated.s /home/user/generated-main.o
c /home/user/generated-program.c /home/user/generated-program.o
c /home/user/generated-library.c /home/user/generated-library.o
c /home/user/generated-helper.c /home/user/generated-helper.o
link /home/user/generated-aarch64.elf _start
```

This bounded guest-native build driver accepts two through six inputs: exactly
one assembly input first followed by one through five C inputs. Every input has
a distinct absolute source/object path of at most 96 bytes,
a distinct absolute output path, and one valid entry symbol. The parsed paths
drive all source reads, object writes/reopens, entry-symbol emission/selection,
and the final executable write. Bad version, relative path, colliding paths,
and missing-link manifests fail closed before compilation. A bounded C
translation unit accepts up to three `int` functions, each with up to three typed
`int`/`int *` parameters, 0..65535 constants,
parentheses, precedence-correct `*`, `+`, and `-`, up to four register locals,
mutable parameter/local assignments, signed `==`, `!=`, `<`, `<=`, `>`, and
`>=` comparisons, one conditional `if` block containing a return, a bounded `while` body containing one
or more assignments, a one- through three-argument function call, and bounded pointer
initializers from `&local` or `pointer-or-array + constant-or-scalar`. Address and pointer
expressions may be passed to the bounded external call. Dereference expressions
load a 32-bit `int` through a pointer local or pointer parameter;
`*pointer = expression` stores it back. Parenthesized
`*(pointer + constant-or-scalar)` supports the same scaled load/store form.
Constant addition uses a 64-bit AArch64 `ADD` with a 0..3 element offset scaled
by `sizeof(int)`. A scalar `int` parameter or non-address-taken scalar local may
also supply the offset for an unknown-bound pointer; codegen uses signed
`SXTW #2`, and guest execution proves both positive offsets and `-1`. Known
local-array/derived-pointer bounds reject one-past-end constants and all
variable offsets whose range this compiler cannot prove.
Subtracting one typed `int *` expression from another emits a 64-bit `SUB`
followed by arithmetic shift-right two, producing a signed element count as
the subset's 32-bit `int`. The caller remains responsible for C's same-array
provenance rule; the compiler rejects pointer-minus-scalar syntax.
Fixed local `int` arrays accept one to four exactly supplied initializer
expressions, subject to the shared four-slot frame limit. Constant indices are
bounded to 0..3 and known local-array indices are checked against the declared
length. Indexed expressions and assignments emit scaled 32-bit loads/stores;
a bare array call argument decays to its 64-bit stack address; an accepted
`array + constant` call argument passes the scaled derived address. Arguments
occupy AAPCS64 `x0` through `x2`, using the `w` view for `int` values.
A final unconditional return is required so every accepted path returns.
Non-leaf functions preserve FP/LR and x19-x24 in a 96-byte AAPCS64 frame with
four bounded 32-bit local slots. A three-parameter function additionally saves
and restores `x25` in the frame's aligned unused slot; one- and two-parameter
code remains byte-for-byte unchanged. The
current `answer` initializes `values[3]` with `(value * 3) - 20`, 40, and zero,
then calls `adjust(values + 1, 1)` when element zero is at least 40; otherwise it
returns 86. `adjust(int *pointer, int delta)` accepts its pointer in AAPCS64
`x0` and delta in `w1`, preserves them in `x23`/`w24`, derives
`next = pointer + delta`, adds delta to element zero,
computes `distance = next - pointer`, then uses `while (count < distance)` and
`*(pointer + delta)` to store through the
dynamically derived address and advance the counter. Its return reloads through
`next`. `adjust` calls `combine(int value, int delta)` from the separate library
translation unit. The compiler emits 140-byte `answer` and 168-byte `adjust`
definitions in a 308-byte `.text` and 976-byte `generated-program.o`; it emits
the 60-byte `combine` in a 616-byte `generated-library.o`. The assembler emits
76 code bytes in the 688-byte `generated-main.o`. An independent fourth C
translation unit emits the 56-byte `helper(int value)` definition in the
608-byte `generated-helper.o`; direct RX execution proves `helper(40)=42`.
All four genuine ELF64 `ET_REL` files persist/reopen. The program object's symbol table defines
`answer` at offset zero and `adjust` at offset 140, while `combine` is undefined
there and defined at offset zero in the library object. The bounded linker
discovers definitions and undefined symbols across all four, applies external
`_start`→`answer`, same-object `answer`→`adjust`, and external
`adjust`→`combine` `R_AARCH64_CALL26` relocations, includes the independent
`helper` definition, and emits 500 code bytes in the
815-byte `/home/user/generated-aarch64.elf`. Fully linked RX calls require
`answer(20)=42`, `answer(0)=86`, `adjust(forty,1)=42`,
`adjust(scaled,2)=44`, and `adjust(zero,1)=2`, with the three-element direct-call arrays also
required to become `41:42:0`, `42:0:44`, and `1:2:0`. Separate RX probes cover
all four signed ordering relations, load 42 through `pointer + -1`, and return
signed pointer differences of `3` and `-3`. A separate two-function source
compiles `sum3(int,int,int)` plus `invoke3(int)`, preserves the third parameter
from `x2` in `x25`, emits 140 code bytes in a 752-byte ELF64 `ET_REL`, resolves
the same-object `invoke3`→`sum3` `R_AARCH64_CALL26` relocation, links with entry
offset 80, and executes both `sum3(40,1,1)` and `invoke3(40)` as 42 from RX
memory. Invalid
relocation type/addend/site, unresolved `adjust`, a missing library object, and
duplicate `answer` inputs are denied, as are unsupported division, a conditional-only function, a loop
without a terminal return, assignment to an undefined variable, address-of an
undefined local, pointer reassignment outside the typed initializer, and
returning a pointer or address expression as an `int`.
Known local-array out-of-bounds indexing, known one-past-end pointer derivation,
unproved variable offset from a known-bounded local array, pointer-minus-scalar,
duplicate functions/parameters, a fourth function, and more than three parameters or call arguments
are also denied.
The shell launches the final ELF through syscall 56 with default `argc=1`, then
syscall 57 with three arguments and one environment string; `_start` validates
both forms, passes 20 to compiled `answer`, and exits with its result 42. Three
malformed startup-vector forms must be denied.

Run the focused gate with:

```sh
make test-aarch64-selfhost-runtime
```

The gate is a real but bounded A64 C-compiler/assembler/static-linker seed. It
has no general pointer arithmetic beyond constant/scalar-variable element
addition and typed pointer difference, no pointer-provenance analysis or
broader pointer/lvalue expressions, variable-length/global/multidimensional arrays,
structs, nested/general
blocks, more than three functions per translation unit, general object
count/relocation repertoire, transitive dependency/header discovery, or
unbounded input graphs. The
authenticated shell command `makbuild <manifest>` accepts either a name under
`/home/user/` or an absolute `/home/user/` path. The kernel validates and copies
that path into the sandboxed toolchain's child-owned SysV `argv[1]`; build mode
reads the existing MakFS manifest and sources without seeding or overwriting
them. The deterministic self-host fixture uses a separate `MODE=fixture` startup
and is the only path that seeds the documented files. The fixture also seeds a
separate three-input manifest, and focused runtime builds both four- and
three-input graphs. The current CLI remains bounded to one leading assembly
input plus one through five C inputs. It is not a
full C/Rust compiler, general linker/build system, debugger, or end-to-end
in-guest OS build.

`makbuild` now persists a bounded incremental cache beside the manifest as
`<manifest>.state`; consequently the manifest path itself is limited to 90
bytes so the `.state` suffix remains in the validated path bound. `MAKSTATE2`
is exactly 120 bytes: a nine-byte magic, one-byte actual input count, six
reserved zero bytes, one manifest fingerprint, six source-fingerprint slots,
and six object-fingerprint slots. Unused fingerprint slots must be zero. These
are 64-bit FNV-1a build fingerprints, not cryptographic
integrity or a security boundary. A cache hit additionally requires the object
bytes to match their saved fingerprint and pass the existing ELF object parser
and symbol validator. Every build still links and writes the final ELF; it
commits state only after every rebuilt object and the final ELF have been
written. Missing, malformed, stale-manifest, or corrupt state safely rebuilds
all actual inputs. Changed source or corrupt/missing object selectively rebuilds
only the affected input.

The focused runtime proves four-input cold `0/4`, warm `4/0`, corrupt-object
`3/1`, warm `4/0`, changed-source `3/1`, warm `4/0`, and corrupt-state `0/4`
hit/miss sequences, then separate three-input cold `0/3` and warm `3/0`
results. All eight authenticated CLI builds link, execute, and reap with status
42; the state-invalidated build re-establishes a valid cache. This is bounded
incremental reuse, not transitive header discovery, parallel builds, an
arbitrary graph beyond six inputs, a general dependency engine, or a trust
mechanism. The linker also retains its 512-byte aggregate code bound and fails
closed when a user-supplied accepted graph exceeds it.

Linux uses equivalent Rust targets plus distro QEMU/OVMF packages. Image
creation requires only Python 3 and does not mount filesystems.

Every AArch64 boot also runs a bounded EL0 SMP scheduler proof before mounting
storage. Three AP contexts rendezvous immediately before EL0 transition, CPU0
joins and releases all four PEs, and four independent processes must overlap,
block on an absolute monotonic sleep with no local successor, return to AP idle,
resume after CPU0's timer wake, return statuses 40-43, reap cleanly, and restore
the free-frame count. They also perform a 20 ms timed futex wait with CPU0 in
WFI and APs back in their idle dispatchers. The marker requires
`idle_mask=0xe`, `resume_mask=0xe`, `futex_idle_mask=0xe`, and
`futex_resume_mask=0xe`. A zero-descriptor 200 ms `poll` must also return APs to
idle, retry the original SVC after timer wake, and report
`io_idle_mask=0xe`/`io_resume_mask=0xe`. A second EL0 fixture creates an
auto-reset event, clones a shared-VM thread, blocks its leader on AP1, signals
from the child on CPU0, and requires AP1 idle/resume masks `0x2`, child
thread-return status 0, parent status 44, and balanced frames. The scheduler
closes AP dispatch between boot fixtures. After driver/login-UI initialization,
the desktop opens a bounded production policy: leaders and all non-Firefox
roles remain on CPU0, while non-leader Firefox-role threads are eligible on
AP1-3. Device MMIO remains CPU0-owned; this is not general multicore userspace.

Run the post-desktop production-role gate with:

```sh
make test-aarch64-production-smp-runtime
```

The shell's `firefox-smp` command executes the upstream musl pthread workload
under the exact Firefox scheduler role. A production-only three-pthread
rendezvous holds distinct workers Ready while the shared queue dispatches them.
The kernel's target syscall 148 owns each task's CPU mask. Official-musl patch
65 maps Linux AArch64 `sched_getaffinity` and `sched_setaffinity` onto that ABI.
The leader verifies its fixed CPU0 mask; each worker selects and reads back two
different singleton AP masks, forcing at least one migration, then restores and
verifies the production AP-pool mask `0xe` before the rendezvous and joins.
It then creates a real process-owned overflow surface, blocks a non-leader
pthread in `surface_wait_event`, injects QMP Ctrl-A, and requires that exact
watcher to dispatch on AP1-3 followed by one leader dispatch on CPU0. Priority
is one-shot after a successful dispatch; the deadline only expires stale
hints. The gate also requires an AP CPU mask, nonzero dispatch counters, a
simultaneous multi-AP interval with distinct TIDs, exclusive ownership, the
complete upstream-musl pthread/IPC workload, and status-42 reap. The final
Raspberry Pi/QEMU 10.0.11 TCG pass records all APs (`cpu_mask=0xe`), live/final
matching overlap on AP1/AP2 (`overlap_mask=0x6`, TIDs 5/6), watcher TID 8 on
AP2, and dispatch counts `9867,11100,9833`. The runtime also requires at least
three kernel-recorded affinity changes that exclude the source PE and confirms
all three workers restore mask `0xe`. AArch64 serial output is protected by an
IRQ-masked cross-PE lock so these records cannot interleave by byte. This is a
scheduler-role fixture, not real Firefox or
macOS/HVF performance evidence. Real Firefox must still pass the unchanged
strict Gate 3 on the intended idle host.

The ordinary image also contains a bounded forced-migration proof, with a
focused early-exit harness:

```sh
make test-aarch64-smp-migration-runtime
```

One immutable TID begins on AP1, enters an armed yield, and resumes on AP2 only
after the scheduler has captured its context and published it Ready/unowned
under the process lock. The 90-second gate requires the same TID on both PEs,
source/target masks `0x2`/`0x4`, exactly one migration, exclusive ownership,
GPR/SP/TLS/SIMD preservation, status 71, and frame balance. This is Pi/TCG
functional evidence for forced migration, not automatic load balancing or
general desktop SMP.

The same ordinary image contains a six-task shared-Ready-queue contention
proof. Run its focused gate with:

```sh
make test-aarch64-smp-load-runtime
```

Each task performs 48 real yield syscalls and exits with its distinct status
80-85. The gate requires all three APs (`cpu_mask=0xe`), at least 288 yield
contention points, exclusive single-CPU ownership at every recorded selection,
bounded dispatch skew, exact reap/frame balance, and the prior migration proof.
The 2026-08-25 Raspberry Pi/QEMU 10.0.11 TCG pass records 99 dispatches on each
AP (297 total). This is functional AP load-sharing evidence; production scope
remains restricted to Firefox workers and macOS/HVF Firefox qualification is
still required.

The boot also runs a remote `exit_group` fixture. A CPU0 leader clones a worker
fixed to AP1; after the worker proves active EL0 execution, the leader invokes
syscall 119 with status 55. The kernel must publish the dying group, prevent
redispatch, send an SGI, detach the AP1 task on AP1, switch that PE to the
kernel root, and report matching `stopped_cpu_mask=0x2`/`ack_mask=0x2` before
reaping. The marker also requires one shared-root reap and balanced frames.

A second teardown fixture places AP1 inside the real yield SVC before its CPU0
leader invokes syscall 119 with status 56. The bounded boot-probe rendezvous
must report `entered_el1_mask=0x2`; the safe EL0-return boundary must detach the
worker, switch to the kernel root, and report
`deferred_ack_mask=0x2` together with matching target/ack masks. This also
guards the race where an exception entered scheduling just before publication.
Normal yield behavior is unchanged outside the armed boot fixture.

A fifth teardown fixture starts independent status-57 and status-58 processes
on CPU0/AP1 and rendezvous-blocks both inside syscall 119 before coordinator
acquisition. It requires `cpu_mask=0x3`, `rendezvous_mask=0x3`, and
`serialized_acquire_mask=0x3`, proving bounded max-one serialization without
holding the scheduler lock. All AP kernel stacks are 1 MiB; the boot marker
must report `stack_bytes=1048576`. The former 64 KiB allocation was insufficient
for two concurrent full cleanup paths and could overwrite adjacent kernel
state.

The same-group teardown fixture clones a shared-root worker and enters syscall
119 concurrently from CPU0/AP1 with requested statuses 59/60. It requires two
arrivals, exactly one owner, the complementary joined caller, first-owner-wins
status propagation, one root reap, and exact frame balance. The joining CPU
leaves the shared TTBR0 and acknowledges under the process lock; it never runs
a second group cleanup.

The focused device-wake gate uses a separate boot configuration so a normal
boot never waits for harness input:

```sh
make test-aarch64-smp-input-runtime
```

`boot/MAKOS-SMP-INPUT.CFG` arms an AP1 EL0 `read_key` waiter only after
virtio-input initialization. The harness waits for the guest readiness marker,
sends QEMU Ctrl-K, and requires exclusive CPU0 used-ring polling plus an SGI to
resume AP1 from its idle dispatcher. The runtime marker must report nonzero
`owner_activity` and `ap_deferrals`; the structural guard permits the one
low-level poll call only inside the CPU0 owner wrapper and makes the driver
fail closed on a non-owner CPU. Exact idle/resume masks, key-derived status 61,
frame balance, and normal boot completion are mandatory. The harness waits for
the complete readiness line so a partial serial read cannot trigger input
early. `boot/MAKOS.CFG` does not contain the test option.

Before the keyboard phase, the same focused image runs a real UDP/DNS TX/RX
phase. AP1 copies transaction `0x4d4c` into a bounded eight-slot service queue;
CPU0 alone performs the virtio-net transmit, then AP1 blocks in receive. CPU0
alone drains the RX ring and wakes AP1 by SGI. The gate requires nonzero
`owner_transmits`/`ap_tx_requests`, `owner_frames`/`ap_deferrals`, matching I/O
idle/resume masks, validated DNS response status 63, and balanced frames. The
marker reports `tx_transport=bounded-copy-queue` and
`tcp_ap_tx=cpu0-service-ready runtime=separate-tcp4-probe`: copied UDPv4/v6 is
qualified here. The UDP completion wait is bounded in EL1; the receive phase
separately proves AP scheduler idle/wake.

Stateful AP TCPv4 has its own focused image and host fixture:

```sh
make test-aarch64-smp-tcp-runtime
```

`boot/MAKOS-SMP-TCP.CFG` arms only this network proof. AP1 creates a TCPv4
socket, connects through QEMU slirp to the harness listener at
`10.0.2.2:18080`, sends exact `MAKOS_AP_TCP_TX\n`, blocks in receive, verifies
exact `MAKOS_CPU0_TCP_RX\n`, and closes with FIN. CPU0 exclusively owns route
resolution and virtio-net TX/RX; AP connect and segment state cross the bounded
copied queue. The listener delays its response by 0.5 seconds, forcing the AP
through the scheduler idle/wake path. The 90-second gate requires exact bytes,
statuses 69/70, four owner completions/four AP requests (connect, data, ACK,
FIN), one owner RX frame/AP deferral, I/O masks `0x2`, and frame balance. This
Pi/TCG gate is functional evidence only; it does not replace Firefox or other
performance qualification on macOS/HVF. TCPv6 still needs a guest runtime.

The same image now also runs an AP1 native-graphics phase. A kernel-only
pre-login binding grants its immutable probe only `CAP_GRAPHICS`; the probe
uses the ordinary surface create/fill/present syscalls. Off-owner composition
publishes retained scene state into one coalesced deferred action. CPU0's
production 100 Hz timer consumes that action and must complete real
`TRANSFER_TO_HOST_2D` and `RESOURCE_FLUSH` commands. The runtime requires
nonzero AP deferrals, CPU0 deferred compositions, control-queue submissions,
transfer completions and flush completions, status 67, surface reap, and exact
frame balance. Every low-level virtio-GPU MMIO submission fails closed off
CPU0; this qualifies graphics service ownership, not accelerated rendering or
general desktop AP scheduling.

The focused image also runs an AP1 block-service phase before networking. The
kernel creates a private uid/gid-1000 fixture inode; an immutable minimally
authorized probe uses normal VFS/MakFS4 calls to write and `fsync` 4 KiB, close,
reopen, read and verify 4 KiB, and finally remove the inode. Every AP block
operation crosses an eight-slot copied-request queue. CPU0's 100 Hz timer bottom
half exclusively submits the real virtio-blk requests and defers a tick instead
of recursively taking an owner lock interrupted during direct CPU0 I/O. Current
Latest Pi/TCG evidence is 22 requests/completions: 13 reads, 6 writes, 3
flushes, and 22 timer-service completions, with status 65 and balanced frames. Low-level ring
submission fails closed off CPU0. The AP request wait is bounded EL1 `WFE`, not
a scheduler idle/wake result. Post-desktop AP scope remains limited to
Firefox-role workers.

The UEFI loader allocates the direct kernel handoff span as `LOADER_CODE`.
Current AAVMF releases may enforce execute-never on `LOADER_DATA`; using that
data memory type for an ELF entry would fault immediately after
`ExitBootServices`. `scripts/test_uefi_kernel_handoff.py` guards this contract.

## MakFS4 offline check

Stop every QEMU process using the raw MakFS4 data image before checking it.
Checking a live writable image can observe an inconsistent commit boundary.
The tool is read-only. It accepts raw 1 GiB data volumes or whole-disk images
whose primary/backup GPT identifies a MakOS data partition:

```sh
cargo run --release -p makos-makfs4-fsck -- path/to/data.img
```

Success prints `MAKOS_MAKFS4_FSCK_OK` with active generation/root slot and
inode/block counts. Failure exits nonzero without modifying the image. Repair
mode is not implemented.

## Debugging

Kernel has symbols. Start QEMU manually with `-s -S`, then connect a cross-GDB
to port 1234 and load `target/x86_64-unknown-none/release/makos-kernel`.
Early kernel diagnostics use COM1 (`-serial stdio`). Success marker is
`MAKOS_BOOT_OK`; fatal boot-ABI errors use `MAKOS_FATAL`; Rust panics use
`MAKOS_PANIC`.
