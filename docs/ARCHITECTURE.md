# MakOS architecture

Status labels in this document are normative: **implemented**, **partial**, or
**planned**. A planned design is not a claim of working code.

## Goals and invariants

MakOS is a general-purpose OS with its own kernel and userspace. Initial target
is x86_64 UEFI; AArch64 is a first-class future port. Core invariants:

1. Kernel and userspace execute without host-OS services.
2. User address spaces cannot directly access kernel memory.
3. Drivers receive only resources granted by the device manager/IOMMU policy.
4. Stable user ABI is versioned; internal kernel interfaces may evolve.
5. Failures are observable through serial, structured logs, crash records, and
   deterministic QEMU tests.
6. Compatibility personalities translate onto native objects. They never load
   a foreign kernel.

## Repository and build

Current source is split by boundary, not aspiration:

```text
boot/uefi/          UEFI loader
crates/boot-api/    versioned loader/kernel handoff ABI
crates/elf64/       dependency-free, tested ELF64 parser
kernel/             freestanding kernel
scripts/            image construction, QEMU, artifact/boot tests
docs/               architecture, ABI, status, roadmap
```

Directories are added when implementation begins; empty subsystem facades are
avoided. Cargo uses prebuilt freestanding Rust targets. `rust-lld` links the
kernel with a checked-in linker script. `mkfat.py` creates FAT32 directly, so
image creation does not depend on privileged mounting or host filesystem APIs.

## Boot architecture — implemented at Milestone 1

```text
UEFI firmware
  -> removable-media path EFI/BOOT/BOOTX64.EFI
  -> MakOS loader reads /KERNEL.ELF and /MAKOS.CFG
  -> validate ELF64/x86_64 and PT_LOAD bounds
  -> allocate one exact physical span, zero it, copy segments
  -> discover GOP framebuffer and ACPI RSDP
  -> ExitBootServices with final UEFI memory map
  -> SysV x86_64 call to kernel ELF entry
```

Loader and kernel are distinct binaries. Handoff uses `BootInfo` with magic,
ABI version, physical framebuffer address/geometry/format, raw final UEFI
memory map, ACPI RSDP physical pointer, and inline validated boot configuration.
Loader transfers ownership of its
loader-data allocations to the kernel. No firmware boot service is called
after `ExitBootServices`.

Current kernel is linked at physical address 64 MiB and entered under UEFI's
identity mapping. This is deliberately temporary. Milestone 2 creates owned
page tables, maps a higher-half kernel, switches CR3, then reclaims boot memory.

## Kernel architecture — planned, with current foundation

Modular monolithic kernel chosen for early correctness and tractable debugging.
Architecture-specific code stays under `kernel/src/arch/<arch>`; portable
objects expose narrow traits. Performance-critical core drivers may be in
kernel. Risky or optional drivers migrate to isolated userspace driver hosts.

Kernel layers:

- `arch`: privilege transitions, page tables, interrupt entry, SMP, atomics.
- `mm`: physical frames, VM objects, mappings, faults, heap, accounting.
- `sched`: CPUs, threads, run queues, timers, wait queues.
- `object`: ref-counted typed kernel objects and handle tables.
- `ipc`: channels, shared-memory VM objects, event ports.
- `device`: device graph, resource ownership, driver matching.
- `vfs`: vnode/mount/file-description layer and page cache.
- `net`: packet buffers, interfaces, routes, sockets, protocol modules.
- `security`: credentials, capabilities, MAC policy, audit.

Kernel code uses Rust where practical. Small reviewed assembly owns entry and
context-switch boundaries. Unsafe code must document aliasing, lifetime,
privilege, and concurrency invariants.

## CPU abstraction

Portable scheduler/MM code consumes an `Arch` boundary providing address-space
activation, user-context construction, interrupt masking, CPU-local storage,
TLB shootdown, and idle/reboot hooks. x86_64 uses GDT/TSS, IDT, local/x2APIC,
IOAPIC, HPET/TSC-deadline, XSAVE, and eventual `syscall/sysret`. Current entry
uses a DPL3 `int 0x80` gate. AArch64 maps these onto
EL1 exception vectors, GIC, generic timer, TTBR, ASIDs, and `svc`.

## Physical and virtual memory — partial

Current bitmap allocator ingests UEFI conventional ranges, reserves occupied
ranges, accounts 64 GiB, and backs zeroed per-process mappings. Kernel owns
four-level tables and separate CR3 roots; code is RX, stacks/heaps writable NX,
guard gaps remain unmapped.

Anonymous VM uses a process-owned region table: sixteen records/process, a
1 MiB virtual arena, and at most sixteen pages per record. First-fit placement
reuses holes. Mapping eagerly allocates zeroed frames; page-aligned protection
subranges update page tables while enforcing W^X. Exact page-rounded region
unmap reclaims frames immediately. Exited-process region metadata is discarded
before address-space destruction reclaims leaked pages. File-backed mapping,
COW, lazy faults, swap, ASLR, and general pager objects remain planned.

AArch64 uses up to 96 region records across eight isolated processes and a
384 MiB first-fit arena. Its 64 MiB image, 64 MiB `brk` heap, mmap arena, and
32 MiB stack reserve occupy disjoint VA ranges below 1 GiB. Per-process L3
tables are created on demand across multiple L2 slots. Eager zero-fill,
partial range split/protect/unmap, W^X, cache synchronization before executable
permission, rollback on OOM, and reap-time frame accounting are enforced.

Target allocator adds buddy zones, refcounts, demand paging, scalable VM region
trees, COW, shared/file objects, ASLR, higher-half kernel, and swap.

Each process owns a page-map plus VM region tree. VM regions reference VM
objects (anonymous, file, physical, or shared). Faults resolve lazy anonymous
pages, file-backed cache pages, copy-on-write, guard violations, or fatal user
exceptions. Kernel has a higher-half global map plus per-CPU stacks with guard
pages. W^X, NX, supervisor/write-protect, SMAP/SMEP, ASLR, and zero-before-map
are policy. PCID/ASID and huge pages are later optimizations, not assumptions.

Kernel heap starts as size-class slabs backed by page allocator. User heaps are
libc policy built on `vm_map`; kernel never implements a per-process malloc.
Swap is a later VM-object pager, keeping page-fault core independent of storage.

## Processes, threads, scheduling — partial

Process = credential set, handle table, address space, signal/event state,
parent/session metadata. Thread = schedulable context, kernel stack, user
registers, TLS base, affinity, priority, wait state. Creation is explicit:
`process_create` + address-space population + `thread_start`; POSIX `fork` is a
compatibility operation implemented with copy-on-write.

Scheduler starts preemptive per-CPU round-robin, then becomes priority-aware
fair scheduling with per-CPU run queues and work stealing. Timers use a heap or
timing wheel; idle CPUs receive reschedule IPIs. Locks: spinlocks only in short
non-sleepable regions; mutexes, rwlocks, semaphores, events, futex-like waits,
RCU where measurement proves value. Lock order and interrupt context are
machine-checkable debug metadata.

Current eight-entry BSP scheduler preempts kernel/user tasks at 100 Hz and has
five reusable dynamic slots. It supports spawn/wait/exit, two concurrent
path-loaded ELF children, shared-address-space user threads,
thread arguments/join/exit, plus event-driven task block/wake. APs complete
INIT/SIPI but do not run independent queues yet.

AArch64 uses the same allocation-free `makos-process-table` lifecycle core:
monotonic PIDs, ready/running/blocked/zombie states, parent-checked wait, reap,
and round-robin selection. Full `x0..x30`, `ELR_EL1`, `SPSR_EL1`, `SP_EL0`, and
`TTBR0_EL1` context switches run from the generic-timer interrupt. Exit closes
process-owned FDs, TTY state, sockets, surfaces, VM metadata, user frames, page
tables, and address-space root.

The four-PE QEMU `virt` path has CPU-indexed ownership and a bounded production
policy after desktop startup. CPU0 retains process leaders and device service;
Firefox and native worker threads can execute on AP1-3. Each context owns an
8-bit affinity mask. Native syscall 148 validates same-thread-group access and
nonempty online masks; exception-time replacement forces Ready/unowned
publication and a scheduler SGI when migration is required. The current policy
is deliberately narrower than general work-stealing desktop SMP.

Parent wait reaps exited scheduler slot, process-owned FDs/handles/surfaces,
user leaf mappings, page tables, and address-space root. Shared kernel identity
mappings remain untouched; subsequent processes reuse reclaimed frames.
Ring-3 #GP/#PF becomes contained process exit (`128 + vector`); essential PID1
or kernel-mode faults remain fatal. Service test deliberately faults at null,
then proves restart and continued network/desktop/shell execution.

## Interrupts, exceptions, timers — partial

Architecture stubs save a complete trap frame, normalize error-code layout,
switch to known kernel stacks, then call portable dispatch. Exceptions from
user mode become process faults/events; kernel faults produce crash records and
panic unless guarded copyin/copyout recovery applies. IRQ handlers acknowledge
hardware quickly and schedule deferred work. Vector allocation and IRQ routing
belong to interrupt controller, not individual drivers.

## Native syscall ABI — implemented subset

ABI v1.0 currently uses x86_64 `int 0x80`: number in `rax`; args in `rdi`,
`rsi`, `rdx`, `r10`; result or `UINT64_MAX` in `rax`. Pointer arguments include
lengths. Validation walks active page tables, requires USER+PRESENT across the
whole span, and requires WRITABLE for copyout. Calls act on process-owned handles/FDs.
AArch64 uses `svc` with number in `x8`, arguments in `x0..x5`, and result in
`x0` for its documented implemented subset. Full syscall-table parity remains.

Initial groups: task/thread, VM, handle/channel/event, VFS, socket, clock,
security, package, graphics, and debug log are implemented subsets. Versioned
feature discovery prevents guessing. `docs/SYSCALLS.md` is normative.

## IPC and object model — partial

Typed handles reference kernel objects with explicit rights: read, write,
execute, map, duplicate, transfer, administer. Channels transfer bounded
messages and handles; shared VM objects carry bulk data; event ports multiplex
async readiness. Receiver obtains no right sender did not possess. Namespaces
are userspace services reached through bootstrapped handles.

Current bounded channels carry scalar messages. Typed handles enforce owner PID
and rights; auto-reset events block/wake scheduler threads; handle close
reclaims objects. Handle transfer, shared VM objects, event ports, and general
rights duplication remain target design.

## Executables, runtime, dynamic linking — partial

Native executable container is standard static ELF64. Kernel validates and loads
Rust init plus Clang-built C applications. Bounded exec-by-path snapshots a
MakFS file, validates up to four page-disjoint `PT_LOAD` segments with entry in
an executable segment, maps each segment's declared W/X/NX flags into fresh
CR3 roots, launches isolated PID7/PID8 concurrently, and integrates with
wait/reap. Invalid files,
overlapping segments, unknown permissions, and writable/executable segments are
rejected. Static SDK/libc wrappers use SysV C ABI. Userspace `ld-makos.so`
later handles PIE, ELF relocations, shared objects, symbol versions, TLS, RELRO,
and library search policy. Versioned startup descriptor carries up to eight
arguments, eight environment strings, and 256 string bytes. Kernel builds
child-owned, 16-byte-aligned SysV `argc/argv/envp/auxv` stack and also supplies
direct-entry registers `rdi/rsi/rdx`; ring-3 generated ELF validates both.
Native libc provides C/POSIX-like calls atop native syscalls. C ABI follows
platform psABI; MakOS syscall ABI remains separate.

## Device and driver model — planned Milestones 4 and 8

ACPI/PCI discovery creates a device graph. Bus managers publish typed resources:
MMIO ranges, port ranges, IRQs, DMA domains. Drivers bind by IDs and interface
contracts. Virtio block/net/input/GPU are first deterministic QEMU targets,
followed by NVMe, AHCI, xHCI/HID, Intel/Realtek Ethernet, HDA. Unsupported
hardware remains explicit.

Userspace driver hosts receive restricted MMIO/IRQ/DMA capabilities. IOMMU
domains constrain DMA where VT-d/AMD-Vi exists. Boot-critical storage can begin
in kernel, then move after stable IPC and pager paths exist.

## Storage and filesystems — planned Milestone 4

Block layer provides async requests, partitions, flush/discard, barriers, and
device error semantics. VFS separates paths/dentries, vnodes, mounts, open file
descriptions, FDs, and page cache. First persistent native filesystem, MakFS,
uses checksummed copy-on-write metadata, extents, atomic root commit, redundant
superblocks, and offline checker. This gives crash consistency without a large
journal replay engine. FAT32 is later supported for EFI interchange, not root
security semantics.

Credentials and ACL evaluation occur in VFS before driver requests. Metadata
includes owner/group/mode, timestamps, type, size, links, xattrs, and optional
ACLs. Namespace supports symlinks, mount points, and per-process roots.

Current MakFS/VFS subset includes persistent fixed files plus sixteen
CRC-protected dynamic inodes and an 80-sector allocation bitmap. Files use up
to four noncontiguous sectors (2 KiB). Writes allocate replacement blocks,
commit inode metadata, then release old blocks; mount reconstructs allocation
from CRC-valid inodes, repairing bitmap corruption/leaks. Validated arbitrary
leaf names under `/home/user` support `create`, write/read, remount persistence,
directory enumeration, and `unlink`. Integration tests migrate a legacy v1
record, persist ten names including a two-block file, corrupt both primary
superblock and bitmap, recover, validate, unlink, and prove absence. Unbounded
allocation, dynamic directories, and full crash-atomic root transactions remain.

MakFS4 implementation is underway in `crates/makfs4`: 4 KiB extent maps,
caller-sized allocation bitmap, CRC-validated inode/superblock codecs,
redundant-generation root selection, and enforced data→inode→bitmap→catalog
→flush→superblock→flush commits. See `docs/MAKFS4.md`. Kernel block/VFS
integration and v3 migration remain incomplete; runtime still uses v3.

## Networking — implemented packet/API subset

Current RTL8139 path performs Ethernet, DHCPv4, ARP, IPv4 ICMP, UDP/DNS,
checksum-validated TCP HTTP, IPv6 NDP, and ICMPv6 echo. Locked transmission
cycles the four C-mode descriptors in hardware order with distinct DMA buffers
and checked completion before reuse. Ring-3 calls exercise
process-owned generation-tagged AF_INET socket objects for connected UDP/TCP,
plus IPv6 over real emulated NIC traffic. Capability checks gate creation;
reaping closes leaked sockets. Current socket send/receive is a bounded,
synchronous one-exchange queue. Bind/listen/accept, nonblocking readiness,
continued streams, SLAAC, routing, firewall, TLS, IRQ mode, and network
namespaces are future work.

## Graphics, input, audio, desktop — planned Milestone 7+

Boot GOP framebuffer becomes early console only. Display service later owns
scanout through framebuffer/virtio-gpu DRM-like driver. Applications allocate
shared surfaces; compositor validates damage, composes, handles focus/clipboard,
and presents. Window protocol is async and versioned. GUI toolkit, fonts, DPI,
multi-display, terminal, launcher, panel, settings, files, monitor, and login
are ordinary sandboxed native applications/services.

Input drivers emit normalized timestamped events to seat service. Audio uses a
kernel PCM device contract and userspace mixer/session service; virtio-sound or
HDA is first backend. No app maps arbitrary device MMIO.

## Userspace and service management — partial

Kernel starts static `init`; current init performs password login, launches and
waits on native C worker, applies on-failure restart policy to an isolated demo
service, tests package/log/network services, renders desktop, then runs
interactive shell. Unauthenticated PID1 has only console/input; successful login
grants session capabilities. Declarative units/dependencies, readiness, service
discovery, general PID allocation, unbounded startup vectors, and full utilities remain planned.

System config is schema-versioned files under `/system/config` with user config
under home directories. Structured logs carry monotonic/wall timestamps,
facility, severity, process/thread IDs, and fields. Ring buffers survive service
failure; storage daemon persists them.

## Security model

Current salted PBKDF2-HMAC-SHA256 login (100,000 iterations) yields uid/gid
session state; wrong-password path is tested. Per-PID credentials, capabilities,
mode checks, pointer isolation, NX, handle/FD ownership, and worker IPC denial
are enforced. Package installs
verify a canonical manifest using a pinned RSA-2048 public key and PKCS#1 v1.5
SHA-256; a modified payload is denied before store mutation. Target model adds
memory-hard password KDF, group lists/roles, MAC, ASLR, quotas, signed
executables, repository key rotation, and Secure Boot policy.

Authentication yields credential objects (UID, groups, roles, session). VFS
uses ownership/mode/ACL; all other authority uses unforgeable handles with
least rights. Sandboxes combine new namespaces, resource quotas, syscall/object
policy, and brokered devices/services. Admin is a scoped capability, not a
permanent all-powerful execution mode.

Executable/package signatures establish provenance, not blanket trust. Secure
Boot integration signs loader/kernel and measures boot chain where TPM exists.
Kernel enforces W^X, NX, stack guards, user/kernel isolation, copyin/out,
entropy-seeded ASLR, and audited privilege transitions. Update verification
uses offline root keys plus rotating online metadata keys.

Current copyin/out enforcement is live-PTE based and boot-tested with readable
RX input, denied RX output, denied unmapped input, and compatibility/native
success paths. Guarded fault recovery, SMAP/SMEP, ASLR, and audited copy helpers
remain target work.

## Packages, updates, recovery — prototype plus planned Milestone 6+

Implemented path canonicalizes name, version, payload, and dependency,
checks a pinned RSA-2048/SHA-256 signature, hashes payload/dependency with
SHA-256, then commits payload/version into CRC-validated A/B disk snapshots on
layout-compatible 1 GiB volumes. Commit marker follows payload/catalog flush;
mount selects highest fully valid generation and falls back after interruption
or corruption. Removal and rollback are atomic. Mutation is
capability-gated. Tamper rejection, upgrade, query, removal, and both rollback
paths run from ring 3 during boot on small-disk RAM compatibility path; host
fault injection plus kernel structural checks cover durable routing/boundaries.
Guest durable-path proof remains missing. Resolver currently recognizes one
built-in `libc` dependency; versioned syscall graphs remain target work.

Target package is content-addressed archive plus full dependency graph,
capability declaration, hashes, and Ed25519 signature. Solver produces an
immutable system generation. Install writes a new generation, verifies it, then
atomically changes next-boot selection. Previous generation remains bootable.
User apps use same store with per-user activation.

Bootloader tracks attempt/success counters in a small recovery record. Repeated
failure selects last-known-good generation or recovery image. Recovery can
verify MakFS, inspect logs/crashes, roll back, and reinstall signed images.

## Crash, logging, debugging

Early serial is always available on supported platforms. Kernel logs to a
lock-aware ring with serial mirror in debug builds. Panic freezes peer CPUs,
records registers/backtraces/memory-map/build ID, writes preallocated crash
area when safe, then reboots or enters debugger. QEMU builds support GDB stub,
symbolized traces, debug-exit, fault injection, and deterministic virtual time
where possible.

## Compatibility architecture — initial slice

ABI dispatcher selects native or registered personality from ELF notes. POSIX
belongs mostly in libc/userspace services plus native process/VFS primitives.
Linux personality translates implemented syscall numbers, flags, signals,
`procfs` views, futexes, and ELF conventions onto native objects. Windows
personality combines a userspace NT/Win32 service/runtime with narrowly defined
kernel primitives for process, section, wait, APC-like, registry, graphics, and
filesystem semantics. Neither personality gets raw kernel pointers or bypasses
rights checks. Compatibility is claimed per tested API/application only.

Current initial slice launches isolated PID3 with Linux personality dispatch and
translates Linux x86_64 numbers for write/getpid/uname/clock_gettime/exit. C
fixture validates all five. Current int80 trap adapter is not Linux binary ABI;
`syscall` entry, ELF notes/interpreter, Linux startup stack, futexes, signals,
procfs, and dynamic linking remain planned.

Windows slice parses PE32+ headers/sections, validates fixed image base and
bounds, maps `.text` RX and `.rdata` R/NX, enters AddressOfEntryPoint in isolated
PID4, then exercises Microsoft x64 ABI thunks backed by brokered
console/time/event/process operations. DLL imports/relocations, NT syscall ABI,
registry, GUI, and general Windows compatibility remain planned.

## Testing strategy

Every milestone retains bootability. Host unit tests cover pure parsers and
algorithms. Freestanding kernel tests run under QEMU and report over serial and
debug-exit. Integration images test syscalls, VM isolation, scheduler races,
filesystems with power-cut injection, packet traces, drivers with malformed
descriptors, userspace/service restart, packages/rollback, and compatibility
fixtures. Hardware support needs named machine/firmware evidence before status
changes from untested.
