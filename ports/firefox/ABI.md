# Firefox target contract

MakOS is an unsupported Mozilla target. Linux/AArch64 and macOS/AArch64 are
Tier 1; `aarch64-unknown-makos` is neither. A real port requires Gecko changes,
not a Linux binary, remote browser, webview, or UI imitation.

## Build target

- CPU/ABI: AArch64 LP64, little-endian, ELF, 4 KiB pages.
- Triple: `aarch64-unknown-makos`.
- Toolchain: LLVM clang/lld plus a MakOS sysroot.
- Product: Gecko desktop browser, native `makos` widget toolkit.
- Initial config: `mozconfig.makos`; updater/crash reporter disabled only until
  MakOS services exist. Tests remain host-driven during bring-up.

Current ordered patches recognize `MakOS`, define `XP_MAKOS` and
`MOZ_WIDGET_MAKOS`, select `cairo-makos`, provide a retained-surface
`nsIWidget`/event bridge, build software Skia/Cairo/FreeType paths, and give
bundled NSPR a distinct MakOS AArch64/LP64 identity using generic
Unix/pthreads code. Full Gecko, SpiderMonkey, Necko, NSS, WebRender, browser,
plugin-container, and xpcshell now compile and link as AArch64 ELF. Target
runtime contracts below remain incomplete.

Mozilla's build system must learn the OS in `build/moz.configure`, platform
defines, Rust target metadata, linker configuration, packaging, and
`moz.build` conditionals. NSPR and NSS need explicit MakOS platform ports.
Using `OS_TARGET=Linux` would hide missing ABI and is prohibited.

## P0 kernel/userspace ABI

Firefox requires general ELF process launch with argv/env/auxv; scalable VM;
dynamic linking, TLS, `dlopen`; C/C++ runtimes; pthreads; futex-like waits;
signals and alternate stacks; shared memory; pipes/socketpairs; descriptor
duplication; nonblocking I/O and readiness polling. JIT code requires safe
RW-to-RX transitions, AArch64 instruction-cache synchronization, fault
delivery, large reservations, and enforceable W^X.

Current loader executes PIEs through upstream musl `PT_INTERP`, resolves libc,
and loads one separate `DT_NEEDED` DSO from `/usr/lib` through real VFS
open/fstat/eager-private-map, RELA/PLT/GOT, symbol resolution, and RELRO.
Runtime `dlopen(RTLD_NOW)`/`dlsym`/call/`dlclose` of that DSO also executes.
The same probe maps a 920 KiB read-only system ELF through generic VFS package
descriptors. Genuine packaged Firefox reaches Gecko/XPCOM and SpiderMonkey JIT
initialization in guest. Sparse anonymous JIT reservation and widget
output/input bridges are source-implemented but not rebuilt or guest-verified.
Narrow relocation/TLS breadth, profile filesystem gaps, and incomplete
process/network services still prevent a usable browser.

## P0 web/runtime services

- NSPR: threads, processes, clocks, files, polling, sockets, DNS.
- NSS: entropy, certificates, files, time, dynamic libraries; TLS 1.2/1.3.
- Necko: IPv4/IPv6 TCP and UDP, nonblocking connect/read/write, DNS. Gecko/NSS
  then supply HTTP/1.1, HTTP/2, HTTP/3/QUIC, WebSocket, TLS, proxies, caching.
  MakOS source path currently supplies IPv4 TCP/UDP plus musl resolver fallback
  to DHCP-provided DNS; constrained Firefox guest proof remains pending.
- Profile storage: large files, nested directories, rename, locking, mmap,
  durable flush, SQLite-safe semantics, substantial capacity.
- CA trust store plus OS permissions for profile, downloads, camera/mic.

Kernel HTTP syscalls are not a replacement: Gecko must own protocol behavior.
HTTPS cannot be claimed until NSS, certificate validation, entropy, clock, and
socket semantics pass upstream tests.

## P1 platform backends

- `widget/makos`: windows, resize/DPI, pointer/keyboard, focus, IME, clipboard,
  drag/drop, accessibility, event-loop wakeups.
- Graphics: shared pixel/GPU surfaces, fonts, shaping, Skia/WebRender bridge,
  vsync. Software rendering may bootstrap; modern performance needs GPU paths.
- `cubeb_makos`: low-latency output/input, device enumeration, callbacks.
- Multiprocess: browser/content/GPU/socket processes, IPC, shared memory,
  process death notification, capability sandbox replacing Linux seccomp.

## Resource gate

Mozilla documents 4 GiB build RAM minimum, 8 GiB recommended, and at least
30 GiB free disk for source/build. Target runtime needs hundreds of MiB virtual
and physical memory, large per-process address spaces, demand paging, and many
threads/processes. MakOS now has package-backed demand paging and sparse
anonymous reservations; bounded RAM, process scale, and memory-pressure
behavior remain unproven.

Compile/link plus early guest-startup milestones are complete. Runtime completion means official-source MakOS ELF boots normally, renders Mozilla
web-platform tests, validates HTTPS, runs multiprocess isolation, plays audio,
persists a profile, and survives restart. Current guest execution has not
reached those completion gates.
