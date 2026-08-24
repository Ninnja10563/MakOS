<p align="center">
  <img src="docs/assets/makos-icon.svg" width="96" height="96" alt="MakOS icon">
</p>

# MakOS

MakOS is a from-scratch operating-system project. Current implementation boots
through UEFI into its own x86_64 kernel; owns physical allocation, page tables,
CPU tables, faults, timer scheduling, and four-CPU startup; concurrently runs
isolated ring-3 ELF processes with separate CR3 roots and syscalls/IPC; persists
MakFS data through ATA and exposes it through VFS file descriptors; exchanges
real DHCP/ARP/IPv4/IPv6/ICMP/UDP/DNS/TCP packets through RTL8139 using
process-owned AF_INET UDP/TCP socket objects; composites two native process
windows using a native 95.css-inspired widget theme, live retained terminal,
Start-menu close/reopen launcher, application taskbar buttons, title-bar
drag/close, x86 software cursor plus AArch64 virtio-GPU cursor plane, and
click focus; drives AC97 PCM DMA;
enumerates a USB HID keyboard through its own UHCI
stack; and runs an interactive native shell.
MakOS also performs password-gated session creation, exposes ABI v1 discovery,
runs static C applications through its SDK/libc, and provides blocking kernel
events for cross-thread synchronization. Initial isolated Linux syscall and
PE32+/Microsoft-x64-ABI Win32 fixtures exercise compatibility translation.
Native init also proves on-failure restart of an isolated service process.
Native AArch64 desktop registers System Monitor, Terminal, Settings, Text Edit,
Browser, and Files windows. Settings manages persistent users and applies live
800x600, 1024x768, or 1280x800 modes
through native virtio-gpu. Its guest-defined pointer uses virtio-GPU's dedicated
cursor resource and cursor queue; pure motion never paints or restores scanout
pixels, while the macOS host pointer stays hidden. Browser runs as an isolated timer-preempted EL0
process and fetches HTTP through native virtio-net, DHCP, ARP, DNS, and TCP.
Files performs real VFS list/create/rename/delete/open operations. With CPython
package installed, `python FILE` launches official upstream CPython 3.14.7 as
a musl PIE in separate EL0 address space; parser/compiler/ceval, stored-ZIP
stdlib imports, VFS file reads, wait, and full resource reclaim pass under HVF.
Minimal images without `/usr/bin/python3` retain genuine upstream MicroPython
1.28.0 fallback; no host Python executes guest source.
Anonymous VM supports multiple variable-length regions, partial W^X protection,
hole reuse, and process-exit reclamation.
Syscall copyin/copyout walks live user page tables and rejects unmapped or
non-writable output spans before kernel dereference.
MakFS tests ten arbitrary-name files through allocation, remount readback, and
unlink across boots using sixteen CRC-protected inodes, an 80-block bitmap pool,
and a 1,024-byte multi-block file.
Package generations require RSA-2048/SHA-256 signatures; boot test mutates a
signed payload and proves rejection before upgrade, transactional removal, and
rollback restoration.
An isolated ring-3 toolchain seed parses `20+22`, emits six bytes of native
x86_64 code into writable/NX memory, switches page to RX, executes result, and
proves W+X denial. It also emits a 1,150-byte/three-block static ELF64 executable
into MakFS; PID1 opens it by path through the VFS, validates/maps separate RX
code and read-only/NX data segments into two new CR3 roots, builds bounded
argc/argv/envp/auxv startup stacks, executes it concurrently in isolated
PID7/PID8, observes exit-42 results, and reaps every frame. Generated ring-3
code validates startup fields; malformed and overlapping inputs are denied.

MakOS does **not** use Linux, macOS, or another kernel at runtime. Host tools
only compile the image and launch QEMU. Planned subsystems are documented as
planned; they are not represented as implemented.

## Build on Apple Silicon

Prerequisites:

```sh
brew install qemu
rustup target add x86_64-unknown-none x86_64-unknown-uefi \
  aarch64-unknown-none aarch64-unknown-uefi
```

Build one bootable FAT disk image:

```sh
make
```

Build/package/test genuine CPython on native AArch64 HVF:

```sh
make test-cpython-aarch64
```

Run x86_64 guest through QEMU TCG (works on M-series Macs; slower than a native
ARM guest):

```sh
make run
```

Cross-ISA x86_64 TCG cannot deliver near-native M-series CPU performance. Mouse
path now uses IRQs, latest-motion coalescing, preserved click edges, adaptive
acceleration, raw-pixel cursor redraw, and outline-only live drag.
Native AArch64 instead submits a guest-owned pointer through virtio-GPU
`CMD_UPDATE_CURSOR`/`CMD_MOVE_CURSOR`. Automated QMP captures exclude this
hardware plane and require every scanout byte to remain identical across seven
pointer moves. `make test-aarch64-cursor-runtime` runs this focused proof.
Retained
surface writes track real pixel changes, unchanged presents skip composition
and scanout transfer, and Files/Text Edit ignore hover motion that changes no
UI state. Last-runnable `nanosleep` waits in EL1 `WFI` until timer deadline
instead of returning `EAGAIN` and busy-spinning. Last-runnable blocking I/O
also waits for one interrupt, pumps timer-polled devices, preserves absolute
timeout, then retries original syscall. Boot regression measures idle accepted
presents, HVF host CPU time, pointer click latency, and hover redraws.

Build, test, or run native-ISA AArch64 UEFI bootstrap through Apple HVF:

```sh
make image-aarch64
make test-aarch64-cursor-runtime
make test-aarch64
make run-aarch64
```

Build/test/run x86_64 single-disk GPT form:

```sh
make image-x86_64-gpt
make test-x86_64-gpt
make run-x86_64-gpt
```

Test boots invocation-private image twice, checks GPT partition discovery,
MakFS recovery, and persistence. Interactive run preserves existing GPT image;
rerunning image target creates fresh image. Builder never modifies legacy
`build/makos-x86_64.img` or `build/makos-data.img`.

x86_64 guest installer uses secondary ATA master as live `disk0`, secondary
slave as explicit `disk1`. Create target exclusively, attach it, log in, then
type exact destructive confirmation:

```sh
python3 scripts/create_blank_install_target.py \
  build/makos-x86_64-gpt.img build/install-target.img
make run-x86_64-installer INSTALL_TARGET=build/install-target.img
# guest Terminal: install disk1 erase-disk1
```

Fresh install accepts only equal-sized completely blank target. Interrupted
copy leaves protective MBR blank. Retry uses
`install disk1 resume-disk1`; every existing nonzero sector must match live
source, otherwise resume refuses before new writes. Copy is read-after-write
verified, flushed, then commits MBR last. `make test-x86_64-install` provides
private refusal/install/detached-two-boot harness; runtime remains pending while
active AArch64 QEMU owns host capacity.

Build/test/run AArch64 single-disk GPT form:

```sh
make image-aarch64-gpt
make test-aarch64-gpt
make run-aarch64-gpt
make test-aarch64-install
```

`test-aarch64-gpt` boots the same temporary disk twice and proves a file written
on boot 1 is read from the MakFS4 partition on boot 2. `run-aarch64-gpt`
preserves the existing image; rerunning `image-aarch64-gpt` intentionally
creates a fresh install image.

`test-aarch64-install` boots a temporary live GPT disk with a separate blank
target, logs into guest Terminal, and runs exactly
`install disk1 erase-disk1`. Installer requires administrator session, exactly
two distinct virtio-blk disks, equal sector counts, valid source GPT, completely
blank target, and exact confirmation token. It verifies every written block,
flushes, commits protective MBR last, detaches live source, boots installed
target twice, and proves a file created after installation persists. Test also
proves wrong confirmation, nonblank target, wrong-sized target, and missing
target cause no destructive write. All test disks are invocation-private temp
images; interactive/user images remain untouched.

AArch64 path boots real MakOS ARM code through UEFI/HVF, validates BootInfo,
PMM, heap, MMU, exceptions, GIC timer, isolated EL0 executables, virtio input,
virtio-blk, virtio-net, persistent MakFS/VFS, timer-preemptive context switching,
native virtio-gpu 2D scanout, and native desktop apps. QEMU Cocoa uses
`zoom-to-fit=on`; Settings can switch guest scanout live among 800x600,
1024x768, and 1280x800. Tablet input tracks current guest dimensions.

Run parser tests plus deterministic serial boot test:

```sh
make test
```

Run full test, reset distributable data disk, package deterministic release
artifacts, and write SHA-256 checksums:

```sh
make release
```

Artifacts:

- `build/makos-x86_64.img` — bootable UEFI FAT32 disk
- `build/makos-x86_64-gpt.img` — single-disk x86_64 GPT image (ESP + MakFS data)
- `build/makos-aarch64.img` — native-ISA AArch64 UEFI/HVF boot image
- `build/makos-aarch64-gpt.img` — single-disk AArch64 GPT image (ESP + MakFS data)
- `build/makos-data-aarch64.img` — persistent AArch64 virtio-blk MakFS disk
- `build/makos-data.img` — persistent MakFS test/data disk
- `build/makos-integrated-<identity>.img` — Firefox + GNU nano + CPython data
  disk preserving selected account/profile state (`docs/INTEGRATED-DATA-IMAGE.md`)
- `build/esp/EFI/BOOT/BOOTX64.EFI` — MakOS UEFI loader
- `build/esp/KERNEL.ELF` — MakOS ELF64 kernel
- `boot/MAKOS.CFG` — boot root/log/recovery policy embedded in FAT image
- `outputs/` — tested images, source archive, framebuffer PNG, report, checksums

Login used by deterministic test: user `marcus`, password `makos`.

Terminal commands: `help`, `status`, `mem`, `ps`, `clear`, `pwd`, `ls`,
`cat FILE`, `stat FILE`, `touch FILE`, `write FILE TEXT`, `rm FILE`,
`cp SOURCE DEST`, `mv SOURCE DEST`, `wc FILE`, `edit [FILE]`, `echo TEXT`,
`python FILE`, `nano [FILE]`, `whoami`, `uname`, `uptime`, `adduser USER`, `signout`, `exit`. Terminal
editing supports lowercase, US punctuation, Backspace, eight-entry Up/Down
history, unique Tab completion, mouse highlighting, and per-user Ctrl/Command-C/V
clipboard transfer. Text Edit supports insertion, navigation,
Delete, Backspace, F2 save, dirty-close protection, close, and reopen.

`ports/nano/` pins and verifies real upstream GNU nano 9.1, cross-builds an
AArch64 MakOS PIE against genuine ncurses 6.5 and musl 1.2.6, and packages the
binary plus `makos` terminfo. No fake `nano` command exists. Static integration
and the focused two-boot HVF edit/Ctrl-O/save/Ctrl-X/reopen/persistence probe pass.

95.css itself targets semantic HTML/CSS. MakOS has no browser or CSS engine, so
its visual grammar is ported into native framebuffer primitives: classic system
colors, hard raised/sunken bevels, title bars, fields, buttons, focus rectangle,
and taskbar. No 95.css runtime code is embedded.

See `docs/ARCHITECTURE.md`, `docs/ROADMAP.md`, `docs/STATUS.md`,
`docs/SYSCALLS.md`, `docs/FILESYSTEM.md`, and `docs/SDK.md`.
