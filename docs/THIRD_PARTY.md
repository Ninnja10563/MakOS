# Third-party components

## Build/runtime boundary

- Rust compiler and `rust-lld`: host build tools only.
- QEMU and OVMF: test/emulated hardware only; not part of MakOS image logic.
- `uefi` 0.39.x: loader-side Rust bindings, allocator, and safe protocol
  wrappers. It runs before `ExitBootServices`; no code from it supplies kernel
  scheduling, memory, filesystems, drivers, networking, or userspace.
- PE32+ parsing/loading is implemented locally in `crates/pe64`; no Wine or
  foreign compatibility runtime is embedded.
- 95.css visual design informed MakOS native framebuffer theme. CSS cannot run
  in MakOS and is not copied or embedded; widget rendering is local Rust code.
- MicroPython 1.28.0 (MIT) is a real upstream language runtime embedded as an
  isolated AArch64 EL0 application. MakOS supplies its freestanding port, GC
  register capture, VFS source loader, process sandbox, and syscalls; no host
  Python process executes guest scripts.
- CPython 3.14.7 (PSF License Version 2) is pinned to official python.org source
  plus SHA-256. It cross-builds as an AArch64 MakOS musl PIE and runs in EL0;
  its PEG parser, bytecode compiler, ceval VM, generational GC, VFS source/file
  reads, and upstream stored-ZIP stdlib import execute under HVF. Host Python
  3.14 performs build-time freezing only; it never interprets guest source.
- musl 1.2.6 commit `9fa28ece75d8a2191de7c5bb53bed224c5947417`
  (MIT) builds a static AArch64 `libc.a` containing 1,350 upstream objects plus
  the explicit MakOS syscall adapter. Static-link audit and native HVF probes
  pass bounded custom entry, upstream `crt1.o` → `__libc_start_main` → `main`,
  and upstream `pthread_create`/`pthread_join`, including SysV argv/envp/auxv,
  distinct TLS/TIDs, shared VM, futex join, VFS, TTY, and exit/reap. Robust-list
  register/query and bounded owner-death cleanup compile in patch 63; fresh
  guest proof remains. Patch 64 adds process and exact-task `kill`/`tkill`/
  `tgkill` probe coverage. Remaining pthread gaps: timed waits, broader signal
  delivery semantics and cancellation edge cases. Relative futex timer expiry
  now compiles with a bounded `pthread_mutex_timedlock` probe. `vfork` remains
  `-ENOSYS`.
- GNU nano 9.1 and Firefox ESR 140.13.0 source ports are pinned under `ports/`.
  Nano cross-builds as an AArch64 MakOS PIE against official ncurses 6.5 and
  musl 1.2.6; its package includes the truthful `makos` terminfo and GPL text.
  Nano's two-boot edit/save/reopen persistence probe passes. Firefox browser
  runtime gates remain.

Kernel runtime depends only on `core`, `alloc`, and local MakOS crates. No host
OS library or foreign kernel is linked into MakOS.
