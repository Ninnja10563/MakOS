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
runtimes when using an extracted, non-system QEMU installation.

After login, `selfhost-aarch64` runs the guest-native compiler/assembler/static-
linker gate. It writes an A64 startup to `/home/user/generated.s` and valid C to
`/home/user/generated-answer.c` and `/home/user/generated-adjust.c`, then
rereads all three from MakFS. Each bounded C translation unit accepts one `int`
function with one `int` or `int *` parameter, 0..65535 constants,
parentheses, precedence-correct `*`, `+`, and `-`, up to four register locals,
mutable parameter/local assignments, equality and inequality comparisons, one
equality `if` block containing a return, a bounded `while` body containing one
or more assignments, a one-argument function call, and the bounded pointer form
`int *pointer = &local`. Address expressions may be passed to the bounded
external call. Dereference expressions load a 32-bit `int` through a pointer
local or pointer parameter; `*pointer = expression` stores it back.
A final unconditional return is required so every accepted path returns.
Non-leaf functions preserve FP/LR and x19-x23 in a 96-byte AAPCS64 frame with
four bounded 32-bit local slots. The
current `answer` computes `normalized = (value * 3) - 20`, passes
`&normalized` to `adjust` when it equals 40, and otherwise returns 86. `adjust`
accepts that pointer in AAPCS64 `x0`, preserves it in `x23`, increments the
caller's stack-backed value once through dereference, then uses
`while (count != 1)` to increment it again through dereference and advance the
counter. Its return reloads the pointee from memory. The compiler
emits 120/132 code bytes in 728/688-byte `generated-answer.o` and
`generated-adjust.o`; the assembler emits 76 code bytes in the 688-byte
`generated-main.o`. These genuine ELF64 `ET_REL` files persist/reopen. The
bounded linker discovers definitions and undefined symbols across all three,
applies two `R_AARCH64_CALL26` relocations, and emits 328 code bytes in the
815-byte `/home/user/generated-aarch64.elf`. Fully linked RX calls require
`answer(20)=42`, `answer(0)=86`, `adjust(&forty)=42`, and
`adjust(&zero)=2`, with the direct-call pointees also required to become 42
and 2. Invalid
relocation type/addend/site, unresolved `adjust`, and duplicate `answer` inputs
are denied, as are unsupported division, a conditional-only function, a loop
without a terminal return, assignment to an undefined variable, address-of an
undefined local, pointer reassignment outside the typed initializer, and
returning a pointer as an `int`.
The shell launches the final ELF through syscall 56 with default `argc=1`, then
syscall 57 with three arguments and one environment string; `_start` validates
both forms, passes 20 to compiled `answer`, and exits with its result 42. Three
malformed startup-vector forms must be denied.

Run the focused gate with:

```sh
make test-aarch64-selfhost-runtime
```

The gate is a real but bounded A64 C-compiler/assembler/static-linker seed. It
has no pointer arithmetic, arrays/structs, nested/general
blocks, multiple functions per translation unit, general object
count/relocation repertoire, or build driver. It is not a
full C/Rust compiler, general linker, build system, debugger, or end-to-end
in-guest OS build.

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
The gate requires an AP CPU mask, nonzero dispatch counters, a simultaneous
multi-AP interval with distinct TIDs, exclusive ownership, AP block/idle/wake
behavior, and status-42 reap. The final Raspberry Pi/QEMU 10.0.11 TCG pass
records all APs (`cpu_mask=0xe`) and live/final matching overlap on AP1/AP3
(`overlap_mask=0xa`, TIDs 6/5). AArch64 serial output is protected by an
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
