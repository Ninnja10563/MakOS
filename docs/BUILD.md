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
Focused `test-aarch64-ipv6-runtime` runs the full two-boot workload and adds
post-login proof for validated RA/SLAAC, AF_INET6 sockaddr ABI, NDP resolution,
and checksum-valid UDPv6 transmit. QEMU user networking does not return UDPv6
DNS in this host configuration, so TCPv6 and UDPv6 receive remain separate
open gates.

`OVMF_CODE=/path/to/OVMF_CODE.fd` overrides firmware discovery.
`QEMU_SYSTEM_X86_64=/path/to/qemu-system-x86_64` overrides QEMU discovery.
`AAVMF_CODE`, `AAVMF_VARS`, and `QEMU_SYSTEM_AARCH64` override AArch64 tools.

After login, `selfhost-aarch64` runs the first guest-native toolchain gate. It
writes A64 source to MakFS, assembles and persists
`/home/user/generated-aarch64.elf`, then launches that file through the kernel's
validated AArch64 syscall-56 path with default `argc=1`, then through syscall 57
with a version-1 descriptor containing three arguments and one environment
string. The generated program checks both startup forms itself and exits 42;
the shell also requires rejection of a bad version, a nonzero unused offset,
and a truncated descriptor. This is a bounded assembler seed, not yet a
complete compiler/linker or an end-to-end OS build.

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
closes AP dispatch again before the desktop; this is not yet general multicore
userspace.

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
