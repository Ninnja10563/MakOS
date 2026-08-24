# Official musl MakOS port

Official musl 1.2.6 is pinned and patched into a genuine static AArch64
archive. This is not Linux relabeling: musl's Linux AArch64 request numbers
enter one translation boundary, which invokes MakOS syscall numbers and
normalizes each syscall's actual return convention.

## Provenance

- Project: <https://musl.libc.org/>
- Release: <https://musl.libc.org/releases/musl-1.2.6.tar.gz>
- Official git: <https://git.musl-libc.org/cgit/musl/>
- Tag/commit: `v1.2.6`, `9fa28ece75d8a2191de7c5bb53bed224c5947417`
- SHA-256: `d585fd3b613c66151fc3249e8ed44f77020cb5e6c1e635a616d3f9f82460512a`
- Signing key: `8364 8929 0BB6 B70F 99FF DA05 56BC DB59 3020 450F`
- Primary license: MIT; upstream `COPYRIGHT` remains authoritative.

```sh
ports/musl/fetch.sh
ports/musl/clone.sh
ports/musl/apply-patches.sh
ports/musl/build-makos.sh
ports/musl/build-shared-makos.sh
ports/musl/test.sh
```

Output is isolated at `build/ports/musl/sysroot-static`. It is deliberately
not copied into `sdk/sysroot-aarch64` or the boot image.

`build-shared-makos.sh` separately builds official musl's AArch64 dynamic
loader/libc at `build/ports/musl/sysroot-shared`, plus a PIE carrying
`PT_INTERP=/lib/ld-musl-aarch64.so.1`. `audit-shared.sh` verifies ELF class,
machine, entry, interpreter, `DT_NEEDED=libc.so`, `_dlstart`, `dlopen`, and the
MakOS syscall boundary. `scripts/boot_test_aarch64.py` also embeds the upstream
loader and a minimal PIE, enters `_dlstart` through `PT_INTERP`, applies
`R_AARCH64_RELATIVE` and RELRO, reaches the relocated PIE entry, then verifies
exit/wait/reap under HVF. A second PIE resolves `DT_NEEDED=libc.so`, executes
upstream libc calls, and returns through `__libc_start_main`. A third PIE makes
the upstream loader search `/usr/lib`, open/fstat/eager-private-map a separate
VFS `libmakosdemo.so`, apply RELA/PLT/GOT and RELRO, call its exported symbol,
then exit/wait/reap. A fourth PIE starts without that dependency, calls upstream
`dlopen(RTLD_NOW)`, resolves `makos_shared_add` with `dlsym`, calls it, and
first opens/maps the 920 KiB system `libc.so`, proving system-file descriptors
and positional reads are no longer capped at MakFS's 2 KiB mutable-file size.
It then `dlclose`s successfully. Large dependency counts, recursive/versioned/TLS DSO
suites, writable/shared mappings, demand paging, and unload stress remain.

## Executable evidence

`build-makos.sh` performs a fresh upstream configure for
`aarch64-unknown-makos`, builds all 1,350 objects in `libc.a`, installs static
headers/libraries, compiles through that sysroot, then links three ELF64 AArch64
static executables. `makos-musl-probe.elf` uses a bounded custom entry to test
translated I/O. `makos-musl-crt-probe.elf` uses upstream `crt1.o`,
`__libc_start_main`, TLS initialization, `main`, normal return/exit, and real
per-process `dup`/`dup3`/`fcntl`, shared-offset `lseek`, descriptor flags, and
independent close semantics.
`makos-musl-pthread-probe.elf` uses upstream `pthread_create`/`pthread_join`,
distinct TLS, shared VM, futex wait/wake, and clear-child-tid teardown. Patch 63
also translates robust-list register/query; a raw worker exit requires bounded
kernel owner-death cleanup, preserved waiter state, and wake-one.
Process `kill`, `pthread_kill` through `tkill`, and explicit `tgkill` also route
through credential-checked process/task queues; probe handlers verify exact TID.
It executes real blocking and nonblocking `pipe2`/`poll`: 512-byte POSIX
`PIPE_BUF`, atomic bounded writes, CLOEXEC, `EAGAIN`, byte integrity,
read/write readiness, timed timeout, scheduler block/wake, EOF, and HUP.
Process-owned `epoll_create1`/`epoll_ctl`/`epoll_wait` support ADD/DEL/MOD,
level triggering, edge triggering, one-shot rearm, finite/infinite waits, pipe
wakeups, and asynchronous UDP/TCP readiness. Guest runtime sends real DNS and
HTTP requests through virtio-net; timer-bottom-half RX buffering wakes epoll,
then userspace drains UDP and validates an HTTP/1.x TCP response.
MakOS resolver patch uses DHCP-provided DNS from network-config syscall 61 when
`/etc/resolv.conf` is unavailable. The guest probe runs real
`getaddrinfo(AF_UNSPEC, "example.com", AI_ADDRCONFIG)`; IPv4-only MakOS
truthfully constrains lookup to A records while exercising libc resolver and
bounded per-socket UDP FIFO before HTTP check.
Path `stat`/`lstat` plus descriptor `fstat` execute through upstream musl
conversion for regular files, links, pipes, and inherited TTY descriptors.
MakFS4 returns Unix-epoch atime/mtime/ctime. `symlink`/`readlink`, relative link
following, `AT_SYMLINK_NOFOLLOW`, `DT_LNK`, and unlink-with-target-retention run
inside guest probe.
Directory FDs translate `getdents64`; upstream `opendir`/`readdir`/`rewinddir`
discover dot entries and mounted user files in guest runtime tests.
Current MakFS4 source indexes 512 inode records through 1,024 collision-chained
lookup buckets and resumes directory scans by raw inode cursor. Current two-boot
probe source creates 64 siblings plus a 255-byte name, verifies every entry and
random lookup after remount, then cleans up; fresh guest execution is pending.
MakFS v3 node records add persistent directory kind while retaining v2 file
read compatibility. musl `mkdir`/`rmdir`, nested cwd/open/stat, nested directory
streams, `ENOTEMPTY`, and busy-current-directory rejection execute in guest.
Kernel-owned per-process cwd state drives musl `getcwd`/`chdir` and canonical
relative open/stat/create/rename/unlink lookup.
Patched `openat` passes real POSIX read/write access modes and explicit
truncate/preserve intent. Same regular FD supports `O_RDWR` read, write,
`pread`, `pwrite`, shared offset, readiness, and reopen-without-truncation.
`fcntl` translates process-owned byte-range `F_GETLK`, `F_SETLK`, and
`F_SETLKW`; kernel resolves `SEEK_SET/CUR/END`, detects shared/exclusive
conflicts, splits partial unlocks, retries blocking waits through scheduler,
and releases locks on POSIX close semantics.
Source probe and boot assertion are ready; execution remains deferred in
low-resource mode.
Current runtime probe writes anonymous pages through upstream musl, calls both
`madvise(MADV_DONTNEED)` and MakOS's immediate-decommit `MADV_FREE`, verifies
frame-releasing zero refault after each operation, then unmaps cleanly.
Writable regular FDs support POSIX `ftruncate`: grow zero-fills, shrink removes
the tail, and current offset stays unchanged. `fsync` issues a real virtio-blk
FLUSH barrier (ATA CACHE FLUSH on x86) after persistent MakFS writes.
`pread`/`pwrite` use dedicated positional kernel paths rather than seek/restore;
shared open-description offsets stay unchanged, sparse gaps zero-fill, and
positional pipe I/O returns `ESPIPE`.
Linux `getrandom(2)` maps to syscall 83 backed only by `virtio-rng`; no
timer-derived or deterministic entropy fallback exists.
`CLOCK_REALTIME` maps to QEMU `virt`'s PL031 RTC through syscall 84, enabling
certificate validity checks independently from monotonic scheduler ticks.
`clock_getres` reports truthful 10 ms monotonic and 1 s realtime granularity.
`prctl(PR_SET_NAME)` stores each task's bounded POSIX name. `sched_setscheduler`
accepts same-process `SCHED_OTHER` priority 0, matching MakOS's native
round-robin scheduler; unsupported realtime policies remain rejected.
POSIX `shm_open` uses kernel-owned `/dev/shm` RAM objects with `O_EXCL`,
unlink-after-open lifetime, `ftruncate`, read-only reopen, and coherent
`MAP_SHARED` physical pages. Handles and VM mappings independently retain each
object; final unlink/close/unmap frees resident frames once.
`nanosleep` plus supported `clock_nanosleep` modes use per-task monotonic
deadlines: scheduler blocks caller, generic-timer IRQ wakes it, and userspace
does not busy-spin. Absolute realtime and full signal-interruption/remainder
semantics remain explicit gates.
`rt_sigprocmask` uses task-owned masks inherited by pthread clone. Upstream
`pselect`, `ppoll`, and `epoll_pwait` atomically install temporary masks across
kernel block/retry, deliver a real `SIGWINCH` handler with `EINTR`, and restore
the original mask. HVF runs set/get/block/unblock, clone inheritance, all three
masked waits, fd-set readiness/timeout/`EBADF`, kernel-owned handler context,
and `sigreturn`.
`audit-static.sh` proves:

- `__makos_syscall_dispatch` is defined by `libc.a`.
- AArch64 raw `svc` instructions exist only in audited MakOS dispatcher,
  clone, sigreturn, and unmap/exit paths.
- `clone` uses native thread syscall 80; `vfork` remains `-ENOSYS`.
- musl's `libpthread.a` is its normal empty compatibility archive; pthread
  symbols reside in `libc.a` and execute in native HVF tests.

See `supported-syscalls.txt` for exact translated calls,
`missing-syscalls.txt` for remaining Firefox/musl gates, and
`STARTUP_ABI.md` for exact argc/argv/envp/auxv and TLS-entry requirements.

## Runtime boundary

This is a boot-tested static libc runtime, not full POSIX. MakOS supplies the
standard process-entry stack (`argc`, `argv`, `envp`, auxv), mapped ELF program
headers, virtio-rng `AT_RANDOM`, preserved `TPIDR_EL0`, real main-thread TID,
and clear-child-tid zeroing. HVF tests execute upstream `crt1.o` through
`__libc_start_main`, `main`, thread create/join, then exit/wait/reap. A dynamic
musl caller also executes syscall 221 through MakOS 112, replacing its image
with a system-package PT_INTERP target while retaining PID, copying bounded
argv/env, applying FD_CLOEXEC, and reclaiming its old root. Firefox additionally
requires broader process launch/wait, signal-masked waits, fuller
files/sockets, scalable dependency loading and broad `dlopen`/TLS coverage,
graphics, audio, and C++ runtime support.

The adapter never treats the kernel's mixed returns as one global errno rule:
TTY calls preserve negative errno, sentinel-return calls map `u64::MAX`, and
boolean calls map `1/0` separately.

## Security

musl reports CVE-2026-6042 affecting releases through 1.2.6 (pathological
GB18030 `iconv` performance). CVE-2026-40200 affects 32-bit targets; this port
is AArch64. Before distribution, move to a fixed upstream revision and repeat
the provenance/build audits.
