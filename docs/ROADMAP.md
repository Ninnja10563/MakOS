# Roadmap

Each exit criterion requires code, automated evidence, docs, and accurate
`STATUS.md`. Numbers show order, not calendar promises.

## M0 — architecture — complete

- Architecture, build, boot, ABI, testing, security, compatibility decisions.
- Clean repository boundaries and reproducible toolchain strategy.

## M1 — bootable kernel — implemented

- x86_64 UEFI loader and separate validated ELF64 kernel.
- Final UEFI memory map, GOP framebuffer, ACPI RSDP handoff.
- Serial and visible framebuffer kernel output.
- FAT32 image builder, artifact checks, QEMU serial boot test.

## M2 — memory and CPU — substantial verified subset

- Reserve firmware/kernel/MMIO ranges; physical page allocator.
- Owned four-level page tables; higher-half map; guard pages; NX/W^X.
- GDT/TSS/IDT, exceptions, APIC, timer, page faults.
- Kernel heap, per-CPU state, AP startup, preemptive thread scheduler.

## M3 — processes — substantial verified subset

- User page maps and ring-3 entry; versioned syscall ABI.
- ELF process loader, handles, channels/events/shared memory.
- Threads, waits, termination, COW process clone, fault delivery.
- Static init process proving user/kernel isolation.
- Two concurrent ring-3 processes with separate address spaces and ring-0
  stacks; spawn/exit/wait lifecycle verified.
- Current implementation adds user threads, per-process mmap/unmap, blocking
  event synchronization, handle close, exited-process resource/address-space
  reaping, user-fault containment, and ABI v1 feature discovery. Anonymous VM
  now supports sixteen variable-length regions/process, partial W^X protection,
  hole reuse, zero-fill, explicit unmap, and leaked-page teardown.

## M4 — storage — verified persistent/VFS subset

- PCI and virtio-block; GPT; async block layer.
- VFS, FDs, directories, metadata, mounts, page cache.
- MakFS transactions/checksums/recovery plus checker and persistence tests.
- Current implementation provides ATA PIO, redundant MakFS commits, one
  persistent user file, VFS open/read/write/close/stat/readdir, static
  directories, mode enforcement, plus corruption/recovery two-boot proof.
- Sixteen CRC-protected dynamic inodes allocate from an 80-block bitmap; files
  reach 2 KiB through noncontiguous sectors and copy-on-write replacement.
  Legacy-record migration, ten-file/multi-block persistence, bitmap rebuild,
  unlink, and absence are proven across boots. Unbounded allocation, dynamic
  directories, and full crash-atomic root transactions remain.

## M5 — networking — packet/app-API subset verified

- Virtio-net; Ethernet, ARP, IPv4/IPv6, ICMP, UDP, TCP.
- Socket API, DHCP, DNS service; deterministic packet/integration tests.
- Current implementation proves DHCP, ARP, IPv4 ICMP, UDP/DNS, complete TCP
  HTTP, IPv6 NDP/ICMPv6 echo, plus process-owned connected UDP/TCP socket
  objects. Bind/listen/accept, nonblocking readiness, continued streams, SLAAC,
  routing, firewall, and IRQ mode remain.

## M6 — userspace — substantial native subset verified

- Init/service manager, libc, shell, core/file/process/network tools.
- Dynamic linker/shared libraries; users/groups/login/config/log service.
- Signed package manager, immutable generations, rollback/recovery.
- Current implementation adds static C SDK/libc, interactive shell, salted
  PBKDF2-HMAC-SHA256 password login/session, RSA-2048/SHA-256 package generations
  with tamper rejection, disk-backed A/B payload snapshots on compatible 1 GiB
  volumes, transactional removal, rollback, mutation capability
  enforcement, structured logs, and a boot-tested isolated service restart
  policy. One built-in `libc` dependency is checked. General graph/version
  solver, repositories/key rotation, declarative unit files, readiness, and
  long-running supervision remain.
- AArch64 minimal images embed upstream MicroPython 1.28.0. CPython package
  images now run official upstream CPython 3.14.7 as isolated musl PIE. Focused
  HVF proof covers version, PEG parsing, bytecode/ceval, arithmetic, strings,
  VFS source/file reads, stored-ZIP `json` import, exit/wait, and 1,838-frame
  reclaim. Dynamic extension/package ecosystem and broader POSIX remain.

## M7 — graphics — initial window system verified

- Input event stack, virtio-input, display API, virtio-gpu/framebuffer.
- Surface protocol, compositor, window manager, GUI toolkit.
- Login, desktop, terminal, launcher, panel, files, settings, monitor.
- Current implementation has process-owned surfaces, clipped
  software compositor, z-order, visible trail-free cursor, click focus/hit
  testing, fast-outline title-bar drag, close, taskbar minimize/restore, and a
  retained 58x22 terminal rendering real shell command input/output. AArch64
  adds six surface slots, bounded resizable windows, real virtio-gpu 2D
  scanout with three live modes, Settings, VFS-backed native Text Edit, and an
  isolated native HTTP Browser, isolated Text Edit, VFS-backed Files, live
  System Monitor, persistent user creation in Settings, and signout/login loop.
- Login/session is native userspace. Full desktop suite remains.

## M8 — hardware — initial USB/audio subset verified

- ACPI/PCI routing, IOMMU, userspace driver hosts.
- NVMe/AHCI/xHCI/HID, selected Ethernet, HDA/virtio-sound.
- Real-hardware qualification matrix; AArch64 UEFI/QEMU bring-up.
- Current implementation has AC97 PCM DMA plus UHCI control/interrupt transfers
  and a live USB HID keyboard on QEMU. Ring-3 fixed-format PCM write reaches
  verified hardware DMA.
- PS/2 input now uses IRQ1/IRQ12; keyboard events queue, button edges cannot be
  lost under coalescing, pointer acceleration is adaptive, and motion coalesces
  to latest position. AArch64 UEFI image boots under Apple HVF with native ARM
  code, PL011, PMM/MMU, exceptions, GIC/timer, isolated EL0/SVC, virtio
  input/block/net, persistent MakFS/VFS, login, compositor, and EL0 apps.
  Process table performs timer-driven full-register/TTBR0 context switches,
  parent wait/reap, and FD/surface/socket/address-space cleanup. Virtio-net
  supplies DHCP/ARP/IPv4/UDP/DNS/TCP and Browser HTTP. Audio/USB, SMP scheduling,
  compatibility, dynamic-linker, and package/service parity remain. Native
  virtio-gpu 2D scanout, dirty transfers, and live
  800x600/1024x768/1280x800 modes are implemented.

## M9 — compatibility — initial Linux/Win32 API slices verified

- POSIX conformance subset with published results.
- Linux syscall personality expanded by tested programs.
- Windows NT/Win32 userspace personality expanded by tested programs.
- Current isolated Linux x86_64 fixture translates five Linux syscall numbers
  (`write`, `getpid`, `uname`, `clock_gettime`, `exit`) through personality
  dispatch. Entry uses int80 adapter; Linux ELF conventions/syscall instruction,
  signals, futexes, procfs, dynamic loader, and general binaries remain.
- Current isolated Win32 x86_64 fixture exercises Microsoft x64 ABI thunks for
  `WriteFile`, PID/time, events, wait, close, and exit through a brokered adapter.
  PE32+ section loading with W^X/NX is verified. DLL imports/relocations, NT
  syscall ABI, registry/GUI, and general Windows binaries remain.

## M10 — self-hosting — bounded C compiler/relocatable-linker seed verified

- Native compiler/assembler/linker/build/debug package set.
- Rebuild reproducible substantial MakOS subset inside MakOS.
- Current isolated ring-3 expression compiler parses source, emits native x86_64
  machine code, changes mapping from writable/NX to RX under kernel W^X policy,
  and executes generated result. It additionally emits a minimal static ELF64
  file into MakFS; VFS exec-by-path validates/maps it in a fresh address space,
  builds a versioned argc/argv/envp/auxv startup stack, executes result 42
  concurrently in PID7/PID8, and reaps both. On AArch64, the guest reads an A64
  startup and two C sources from MakFS. A bounded source-driven C compiler emits
  AAPCS64-int32 expressions, register locals, mutable parameter/local
  assignments, equality/inequality control flow, a real backward-branch
  `while`, stack-backed address-taken locals, bounded address-of/dereference
  loads and stores, non-leaf frames, cross-object calls, and genuine ELF64
  `ET_REL` objects; both linked branch outcomes and direct loop/memory outcomes
  execute in EL0.
  The assembler emits `_start`. The guest static
  linker resolves `_start`→`answer`→`adjust` across three objects, applies two
  `R_AARCH64_CALL26` relocations, rejects malformed C, invalid relocation type,
  unresolved symbols and duplicate definitions, emits `ET_EXEC`, and executes
  the result twice under Pi/QEMU TCG. Full C/Rust
  compiler semantics, general assembler/linker, build system, debugger, package
  delivery, and an in-OS MakOS rebuild remain.
