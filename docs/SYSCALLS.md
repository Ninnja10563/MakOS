# MakOS native syscall ABI v1.0

Normative implementation: `kernel/src/syscall.rs`. ABI discovery returns
`0x0001_0000` for major 1, minor 0. Current x86_64 entry is `int 0x80` with:

```text
rax = syscall number
rdi, rsi, rdx, r10 = arguments 1..4
rax = nonnegative result or UINT64_MAX on error
```

Kernel walks caller's live user PTEs for every pointer span before dereference;
copyout additionally requires writable permission on every covered page. RX
memory remains valid copyin, while unmapped gaps and RX copyout fail without a
kernel fault. Handles and file descriptors are process-owned. Capability checks
gate console, input, graphics, IPC, network, process creation, writes, and sync.
`rcx`, `r11`, flags, and memory are caller-clobbered. AArch64 uses `svc #0`,
number in `x8`, arguments in `x0`-`x5`, and result in `x0`. Current ARM EL0
shell uses write, input, shell-command, surface create/fill/present,
authentication, clock, ABI discovery, and exit entries. Remaining syscall
meanings and object model stay architecture-neutral while parity expands.

## Discovery

`abi_info(selector)` is always available:

| Selector | Result |
|---:|---|
| 0 | ABI version (`0x0001_0000`) |
| 1 | highest normative cross-architecture syscall number (`57`) |
| 2 | feature bitset |
| 3 | highest target extension (`147` both targets) |

Feature bits: IPC 0, process 1, VM 2, VFS 3, network 4, graphics 5, auth 6,
structured log 7, synchronization 8, Linux personality 9, audio 10, IPv6 11,
Windows personality 12, service supervision 13, self-hosting seed 14, socket
objects 15, package transactions 16, VM regions 17, exec-by-path 18, process
startup vectors 19, controlling TTY/signals 20, typed IPC/service routing 21.
Programs must query features
before using optional groups.

Structured records use a bounded 32-entry ring. Once MakFS4 mounts, kernel
loads `/.makos-system-log`, merges records emitted before mount, and rewrites a
fixed `MAKLOG01` whole-image-CRC snapshot through filesystem COW. Invalid
journals fail closed and remain on disk for diagnosis.
`log_read` requires `CAP_CONSOLE`. Kernel-generated severity-4 audit records
cover authentication, account, session, and package transaction decisions;
record metadata attributes each event to current PID.

AArch64 currently reports bits 0, 1, 2, 3, 4, 5, 6, 7, 8, 11, 14, 15, 16, 17,
18, 19, 20, and 21. Calls 56 and 57 now share the immutable static-ELF snapshot
and loader; bit 19 records the versioned explicit startup-vector path.
Restricted same-process system-package `execve` remains a target extension.

## Calls

| No. | Name | Arguments | Result |
|---:|---|---|---|
| 0 | `write` | bytes, length | bytes written |
| 1 | `yield` | — | 0 |
| 2 | `channel_create` | output pair pointer | 0 |
| 3 | `channel_send` | handle, value | 0 |
| 4 | `channel_receive` | handle | value |
| 5 | `process_exit` | status | no return |
| 6 | `read_key` | — | key byte or 0 |
| 7 | `shell_command` | bytes, length | status |
| 8 | `surface_create` | width, height | surface handle |
| 9 | `surface_fill` | handle, ARGB, packed rectangle | 1/0 |
| 10 | `surface_present` | handle | 1/0 |
| 11 | `open` | path, length, access 0/1/2, truncate mode 0/1/2 | FD |
| 12 | `read` | FD, output, length | bytes read |
| 13 | `close` | FD | 1/0 |
| 14 | `process_spawn` | — | PID |
| 15 | `process_wait` | PID | exit status |
| 16 | `udp_dns` | request, request length, response, response capacity | response length |
| 17 | `file_write` | FD, bytes, length | bytes written |
| 18 | `package_install` | name/length, contiguous manifest fields, packed lengths/algorithm | 1/0 |
| 19 | `package_query` | name, length, output, capacity | version length |
| 20 | `package_rollback` | — | 1/0 |
| 21 | `vm_map` | — | writable NX page address |
| 22 | `vm_unmap` | address | 1/0 |
| 23 | `thread_create` | entry, argument | TID |
| 24 | `thread_join` | TID | exit status |
| 25 | `thread_exit` | status | no return |
| 26 | `tcp_http` | request, length, response, capacity | response length |
| 27 | `clock_monotonic` | — | PIT ticks |
| 28 | `log_append` | severity, message, length | sequence |
| 29 | `log_read` | sequence, output, capacity, metadata | bytes copied |
| 30 | `auth_login` | username, length, password, length | 1/0 |
| 31 | `abi_info` | selector | discovery value |
| 32 | `event_create` | initially signaled | event handle |
| 33 | `event_signal` | event handle | 1/0 |
| 34 | `event_wait` | event handle | 1/0 after wake |
| 35 | `handle_close` | typed handle | 1/0 |
| 36 | `stat` | path, length, metadata output | 1/error |
| 37 | `read_dir` | path, length, index, entry output | 1/0/error |
| 38 | `process_spawn_linux` | — | compatibility PID |
| 39 | `audio_write` | s16le samples, frames, rate, channels | 1/0 |
| 40 | `ipv6_echo` | — | 1/0 |
| 41 | `process_spawn_windows` | — | compatibility PID |
| 42 | `service_start` | built-in unit | service PID |
| 43 | `create` | path, length | 1/0 |
| 44 | `unlink` | path, length | 1/0 |
| 45 | `vm_protect` | mapped page, protection mask | 1/0 |
| 46 | `process_spawn_toolchain` | — | toolchain PID |
| 47 | `socket_create` | domain, type, protocol | socket handle |
| 48 | `socket_connect` | socket, address, address length | 1/0/error |
| 49 | `socket_send` | socket, bytes, length, flags | bytes sent |
| 50 | `socket_receive` | socket, output, capacity, flags | bytes copied |
| 51 | `socket_close` | socket | 1/0 |
| 52 | `package_remove` | name, length | 1/0/error |
| 53 | `vm_map_range` | length, protection, flags=0 | region address/error |
| 54 | `vm_unmap_range` | region address, length | 1/0 |
| 55 | `vm_protect_range` | address, length, protection | 1/0 |
| 56 | `process_spawn_path` | path, length | PID/error |
| 57 | `process_spawn_path_args` | path, length, startup descriptor, descriptor length | PID/error |

AArch64 keeps normative calls 0-57 and adds compositor/browser extensions:

AArch64 implements calls 56 and 57 for a process-capable shell: the kernel
snapshots a readable VFS file, validates static `ET_EXEC`/`EM_AARCH64` layout,
segment file bounds, address bounds, nonoverlap, executable entry and W^X before
allocating an address space. Call 56 synthesizes `argc=1`, `argv[0]=path`, and
an empty environment. Call 57 first copies and validates the exact version-1
descriptor below, then builds child-owned vectors and advertises feature bit 19.

| No. | Name | Arguments | Result |
|---:|---|---|---|
| 58 | `surface_close` | surface handle | 1/0 |
| 59 | `surface_text` | handle, packed x/y, bytes, length | 1/0 |
| 60 | `surface_read_event` | handle, 28-byte event output, length | 28/0/error |
| 140 | `surface_wait_event` | handle, 28-byte event output, length | 28/error; blocks and retries until ordered event or destroyed handle |
| 141 | `robust_list` | operation, task/head, length/head-output, length-output | 0/-errno; register/query per-task Linux robust-futex list |
| 142 | `signal` | operation, process/task IDs, signal | 0/-errno; process-directed kill or exact task-directed tkill/tgkill |
| 143 | `typed_service_publish` | service name, length | generation-tagged listener handle/error; requires service-publish capability |
| 144 | `typed_service_connect` | service name, length | generation-tagged channel handle/error; same UID/session only |
| 145 | `typed_service_accept` | listener handle | oldest pending server endpoint/error |
| 146 | `typed_channel_send` | endpoint, 64-byte message, optional handle, attenuated rights | 0/error; bounded FIFO and atomic transfer |
| 147 | `typed_channel_receive` | endpoint, 64-byte output, transferred-handle output | 0/error; sender PID/UID stamped by kernel |
| 61 | `net_config` | 12-byte IPv4/gateway/DNS output, length | 12/error |
| 62 | `tty_read` | FD, output, length | bytes or negative errno |
| 63 | `tty_write` | FD, bytes, length | bytes or negative errno |
| 64 | `isatty` | FD | 1/0 |
| 65 | `tcgetattr` | FD, 56-byte termios output, length | 0/negative errno |
| 66 | `tcsetattr` | FD, action, 56-byte termios input, length | 0/negative errno |
| 67 | `tcflush` | FD, queue selector | 0/negative errno |
| 68 | `ioctl` | FD, request, 8-byte winsize pointer, length | 0/negative errno |
| 69 | `sigaction` | signal, new action/null, old action/null, 32 | 0/negative errno |
| 70 | `raise` | signal | 0/negative errno |
| 71 | `getpgrp` | — | process group/negative errno |
| 72 | `setpgid` | PID or 0, process group or 0 | 0/negative errno |
| 73 | `tcgetpgrp` | FD | foreground process group/negative errno |
| 74 | `tcsetpgrp` | FD, process group | 0/negative errno |
| 75 | `sigreturn` | — | restores interrupted context |
| 76 | `brk` | requested break, or 0 to query | resulting break/error |
| 77 | `rename` | old path/length, new path/length | 1/0 |
| 78 | `set_tid_address` | writable u32 pointer or 0 | current TID/error |
| 79 | `gettid` | — | current TID |
| 80 | `thread_clone` | musl flags, child SP, parent-TID pointer, TLS, child-TID pointer | child TID/error |
| 81 | `thread_exit` | status | no return |
| 82 | `futex` | aligned u32, WAIT/WAKE op, value/count, timeout/null | 0/woken count/negative errno |
| 83 | `getrandom` | writable bytes, length, flags | bytes/error |
| 84 | `clock_realtime` | — | Unix seconds |
| 85 | `process_identity` | PID/UID/GID/parent-PID selector | value/error |
| 86 | `fd_dup` | existing FD | new FD/error |
| 87 | `fd_seek` | FD, signed offset, SEEK_SET/CUR/END | resulting offset/error |
| 88 | `fd_dup3` | existing FD, exact new FD, CLOEXEC boolean | new FD/negative errno |
| 89 | `fd_control` | FD/socket, operation, argument or 32-byte `flock` | flags, lock result, or negative errno |
| 90 | `pipe2` | writable two-i32 output, `O_NONBLOCK\|O_CLOEXEC` flags | 0/negative errno |
| 91 | `poll` | writable `pollfd[32]`, count, timeout ms, optional task mask/value-present | ready count/negative errno |
| 92 | `fstat_metadata` | FD, writable 40-byte native metadata | 0/negative errno |
| 93 | `read_dir_fd` | directory FD, writable 48-byte native entry | next offset/0/negative errno |
| 94 | `chdir` | path, length | 0/negative errno |
| 95 | `getcwd` | writable buffer, capacity | bytes including NUL/negative errno |
| 96 | `ftruncate` | writable regular FD, new length | 0/negative errno |
| 97 | `fsync` | regular/directory FD | 0/negative errno |
| 98 | `mkdir` | path, length | 0/negative errno |
| 99 | `rmdir` | path, length | 0/negative errno |
| 100 | `pread` | readable regular FD, output, length, offset | bytes/negative errno |
| 101 | `pwrite` | writable regular FD, input, length, offset | bytes/negative errno |
| 102 | `clock_resolution` | realtime=0 or monotonic=1 | nanoseconds/negative errno |
| 103 | `sleep_until` | absolute monotonic tick deadline | 0/negative errno |
| 104 | `epoll_create` | `O_CLOEXEC` flags | epoll handle/negative errno |
| 105 | `epoll_ctl` | handle, ADD/DEL/MOD, target, 16-byte event | 0/negative errno |
| 106 | `epoll_wait` | handle, writable events, max events, timeout ms, optional task mask/value-present | count/negative errno |
| 107 | `epoll_close` | epoll handle | 1/0 |
| 108 | `sigprocmask` | how, mask value, value-present, writable old-mask/null | 0/negative errno |
| 109 | `pselect` | nfds ≤256, read/write/except fd sets, 24-byte timeout/mask options | ready descriptor count/negative errno |
| 110 | `clipboard_write` | readable bytes, length ≤64 KiB | bytes/negative errno |
| 111 | `clipboard_read` | writable buffer, capacity ≤64 KiB | bytes/negative errno |
| 112 | `process_exec` | path/length, argv, envp | success replaces current image; negative errno |
| 113 | `surface_blit` | handle, ARGB bytes, width, height, stride, packed destination x/y | 1/0/error |
| 114 | `madvise` | page-aligned owned range, Linux advice | 0/negative errno |
| 124 | `socketpair` | AF_UNIX, stream type/flags, protocol 0, writable two-i32 output | 0/negative errno |
| 127 | `sendmsg_rights` | socketpair FD, bytes, flags, readable FD array | bytes/negative errno |
| 128 | `recvmsg_rights` | socketpair FD, writable bytes, flags, writable FD array | high32 FD count + low32 bytes/negative errno |
| 129 | `session_status` | — | PID 1 active-session flag (1/0) |
| 134 | `socket_name` | socket, peer flag, writable sockaddr, writable socklen | 0/negative errno; AF_INET or AF_INET6 |
| 135 | `socket_bind` | socket, readable sockaddr, socklen | 0/negative errno; client bind only |

Surface-event kinds are key=1, pointer=2, resize=3, close=4, scroll=5. Scroll
keeps pointer-local coordinates in `x/y`; signed horizontal/vertical wheel
deltas occupy `modifiers/key`. Record size remains 28 bytes.

Calls 62-135 use Linux/POSIX errno numbers encoded as negative signed 64-bit
results (`-ENOENT`, `-EIO`, `-EBADF`, `-EAGAIN`, `-EACCES`, `-EBUSY`,
`-EEXIST`, `-ENOTDIR`, `-EINVAL`, `-EFBIG`, `-ENOTEMPTY`, `-ENOTSUP`).
Legacy calls retain their documented error convention. Calls 12, 13, and 17
also recognize inherited controlling-terminal descriptors 0, 0-2, and 1-2
respectively, allowing libc to use ordinary `read`, `close`, and `write` paths.

TTY C layouts are fixed little-endian ABI records. `termios` is 56 bytes:
four `uint32_t` flag words, `cc[32]`, then input/output speeds. `winsize` is
four `uint16_t` values. `sigaction` is four `uint64_t` values: handler, flags,
restorer, mask. Supported signals are `SIGINT=2`, `SIGQUIT=3`, `SIGCONT=18`,
`SIGTSTP=20`, and `SIGWINCH=28`. Handler return enters syscall 75 through the
registered restorer; kernel, not writable userspace memory, owns saved context.
Signal dispositions/pending bits are process-owned. Masks and handler contexts
are task-owned; clone inherits caller mask. `SIGKILL`/`SIGSTOP` cannot be
blocked. Masked waits preserve their temporary mask across scheduler retry,
then atomically restore it on readiness, timeout, error, or signal delivery.

AArch64 anonymous VM matches normative calls 21, 22, 45, and 53-55. Each
process owns mappings in `0x20000000..0x38000000`; lengths round upward to 4 KiB
pages, holes are reused, partial protection/unmap is supported, and write+execute
is rejected. Syscall 76 grows/shrinks the zero-filled RW/NX heap within
`0x14000000..0x18000000`. ELF image, heap, mmap, and stack arenas are disjoint;
exit/reap drops VM metadata before page-table destruction frees all remaining
leaf and table frames.

For AArch64 `process_spawn`, selector 0 launches the bounded scheduler fixture;
selector 1 launches the embedded Browser after authentication. Browser receives
an explicit graphics/network/input-only role, never PID-derived ambient rights.
Selector 5 launches the native ABI validator from the authenticated shell.
Selector 15 accepts a bounded `/home/user/` path and launches packaged upstream
GNU nano as a dynamic AArch64 PIE. Kernel assigns a new foreground process
group, bounded SysV argv/env, session credentials, file-write/console-only
capabilities, then the shell waits/reaps and restores its foreground group.
Selector 16 accepts a nonempty `/home/user/` manifest path in arguments two and
three plus a boolean fixture flag in argument four. The kernel validates the
entire readable path, rejects non-home/NUL/oversize paths and flags above one,
then copies `/system/aarch64-toolchain` plus that path into the EL0 toolchain's
child-owned SysV `argv`. Flag zero supplies `MODE=build` and never seeds inputs;
flag one supplies `MODE=fixture`, which is reserved for the deterministic
`selfhost-aarch64` gate.

AArch64 native entry uses a canonical 16-byte-aligned SysV stack: `argc`,
`argv[]`, NULL, `envp[]`, NULL, then auxiliary-vector pairs ending in
`AT_NULL`. Bounded limits are eight arguments and eight environment entries.
Kernel copies all strings into child-owned stack pages and supplies matching
`x0=argc`, `x1=argv`, and `x2=envp`. Auxv includes `AT_PHDR`, `AT_PHENT=56`,
`AT_PHNUM`, `AT_PAGESZ=4096`, `AT_ENTRY`, real session UID/EUID/GID/EGID,
`AT_CLKTCK=100`, `AT_SECURE`, `AT_PLATFORM`, truthful zero HWCAP/HWCAP2,
`AT_EXECFN`, and `AT_RANDOM` backed by virtio-rng `/dev/urandom` entropy.
EL0 `abi-startup` validation checks registers, stack, strings, auxv, exit,
wait, and reap through the real scheduler. `TPIDR_EL0` is part of every saved
context. Threads have real scheduler TIDs and distinct SP/TPIDR_EL0 while
sharing process VM/files/credentials. Private futex WAIT/WAKE uses keyed FIFO
queues; per-thread exit zeroes clear-child-tid, wakes joiners, and reaps only
task state. Upstream musl `pthread_create`/`pthread_join` passes HVF. Per-task
signal masks inherit through clone. Robust futex lists register per task;
thread/group exit performs a bounded owner-death scan, preserves `WAITERS`,
sets `OWNER_DIED`, and wakes one waiter. Credential-checked process-directed
`kill` and exact-task `tkill`/`tgkill` queue into existing handler/mask delivery.
Relative timed futex wait/expiry is implemented; absolute realtime/bitset
variants and broader signal semantics remain pending.

Syscall 102 reports truthful current clock granularity: monotonic 10 ms and
realtime 1 s. Syscall 103 saves current EL0 context, blocks its scheduler task,
and arms a monotonic deadline. Generic-timer IRQ wakes due tasks before normal
round-robin selection; no userspace busy-spin is used. Upstream musl maps
`nanosleep`, relative realtime/monotonic `clock_nanosleep`, and absolute
monotonic `clock_nanosleep` onto this path. Absolute realtime deadlines, full
signal interruption/remainder semantics, and sleep when no successor is
runnable remain pending.

Pipes use shared open-file descriptions, 512-byte POSIX `PIPE_BUF`, atomic
writes through that bound, EOF/HUP/broken-pipe transitions, and blocking or
`O_NONBLOCK` operation. Blocking read/write and timed/infinite poll save EL0
context, rewind to retry original `svc`, and wake on pipe-state or timer events;
they never spin or expose transient `EAGAIN`. Poll includes asynchronous socket
readiness. Atomic temporary signal masks work for `ppoll`; an idle fallback
when no other task is runnable remains pending.

AF_UNIX stream `socketpair` uses two bounded kernel byte rings for duplex I/O.
`SCM_RIGHTS` queues one bounded descriptor record against its exact first data
byte; queued references keep open-file descriptions alive across sender close.
Receiving creates process-owned FD numbers sharing those descriptions, while
truncated/discarded controls release references. Strict musl guest runtime
passes sender-close lifetime and stream-byte association.

Epoll instances are process-owned generation handles. Four instances with 16
watches each support ADD/DEL/MOD, user-data preservation, level triggering,
edge triggering, one-shot rearm, zero/finite/infinite waits, and automatic
reap cleanup. Targets include files, pipes, inherited TTY output, and connected
sockets. Pipe transitions wake blocked epoll waiters. A bounded 100 Hz network
bottom half drains completed virtio RX descriptors into pooled per-socket 32 KiB
TCP buffers, acknowledges segments, and wakes poll/epoll; UDP responses and socket
writability use same readiness backend. `epoll_pwait` atomically swaps and
restores caller's task mask across block/retry and handler delivery.

Both x86_64 path-spawn calls require process capability and read access to path. Current
x86_64 loader accepts one to four page-aligned, page-disjoint static ELF64
`PT_LOAD` segments inside `0x100000000..0x100020000`. Each segment must be
readable; unknown permissions and write+execute are rejected. Entry must lie in
an executable segment. Loader maps declared write/execute/NX flags plus a zeroed
16 KiB NX stack and supports two concurrent bounded children, PID7 and PID8.
Invalid ELF/path/layout returns error without creating an address space. Path
children use console-only credentials; ordinary wait reaps handles and all
page-table/leaf frames.

Syscall 57 startup descriptor is fixed-size/versioned:

```c
struct makos_spawn_arguments {       /* 336 bytes, little-endian */
    uint32_t version;                /* 1 */
    uint32_t argc;                   /* 1..8 */
    uint32_t envc;                   /* 0..8 */
    uint32_t data_length;            /* 1..256 */
    uint32_t argv_offsets[8];        /* offsets into data */
    uint32_t env_offsets[8];
    char data[256];                  /* NUL-terminated strings */
};
```

Unused offsets must be zero. Arguments must be nonempty. Environment strings
must contain a nonempty name followed by `=`. Kernel copies descriptor and
strings into child-owned stack pages before scheduling child; caller memory is
never shared. SysV-style initial stack is 16-byte aligned and contains `argc`,
`argv[]`, NULL, `envp[]`, NULL, then auxv entries for `AT_PAGESZ`, `AT_ENTRY`,
UID/EUID/GID/EGID, `AT_CLKTCK`, `AT_SECURE`, `AT_EXECFN`, and `AT_NULL`.
MakOS also passes `argc`, `argv`, `envp` in `rdi`, `rsi`, `rdx` on x86_64 and
`x0`, `x1`, `x2` on AArch64 for direct C entry stubs. Syscall 56 remains
compatible and synthesizes `argc=1`, `argv[0]=path`, empty environment.

Socket objects support `AF_INET` (2), connected UDP (`SOCK_DGRAM` 2,
`IPPROTO_UDP` 17) and connected TCP (`SOCK_STREAM` 1, `IPPROTO_TCP` 6).
`socket_connect` accepts eight bytes: little-endian family, big-endian port,
then four IPv4 octets. Flags must be zero. Handles are generation-tagged and
process-owned; stale or cross-process use fails. Process reaping closes owned
sockets. Current send performs one bounded synchronous NIC exchange and queues
its response for receive. No bind/listen/accept, nonblocking mode, readiness,
stream continuation, or datagram source-address API yet.

Events are auto-reset, single-waiter kernel objects. `event_wait` removes caller
thread from runnable set; `event_signal` wakes it. Threads share process address
space and accept one validated pointer argument, while owning separate user and
kernel stacks.

`fd_dup` creates another process-owned descriptor referencing the same open-file
description. Duplicates share seek/read/write offset and survive independent
close operations through reference counting. `fd_seek` permits offsets from
zero through current bounded MakFS maximum (2 KiB); invalid descriptors,
origins, negative results, and larger offsets return the generic error sentinel.
Each process owns an independent descriptor-number namespace. `fd_dup3` replaces
an exact descriptor from 3 through 255. `fd_control` supports minimum-FD
duplication, descriptor `CLOEXEC` get/set, access/status get, and regular-file
`NONBLOCK` status set. Regular files also support process-owned POSIX byte-range
`F_GETLK`, `F_SETLK`, and scheduler-blocked/retried `F_SETLKW`; read locks share,
write locks exclude, partial unlock splits ranges, and any process close of that
inode releases its locks. Close-on-exec state is tracked and swept by AArch64
`execve`.

`pipe2` creates a process-owned 512-byte byte ring with distinct read/write
open-file descriptions. Duplicated endpoints share endpoint state; last-reader
and last-writer close drive `EPIPE`, EOF, and `POLLHUP`. Blocking operations
suspend and retry; `O_NONBLOCK` empty/full operations return `EAGAIN`. `poll`
supports zero, finite, and infinite waits over files, pipes, and connected
sockets, including `POLLIN`, `POLLOUT`, `POLLERR`, `POLLHUP`, and `POLLNVAL`.
Epoll adds bounded level/edge/one-shot watch sets. `pselect`, `poll`, and epoll
waits accept optional temporary task signal masks and return `EINTR` after
handler delivery. `pselect` supports read/write/exception fd sets, zero/finite/
infinite waits, pipe/socket/TTY targets, `EBADF`, and scheduler wake.

`stat_extended` and `fstat_extended` report inode, mode/type, UID/GID, size,
and separate atime/mtime/ctime fields. Official musl converts these records to
AArch64 `struct stat`; persistent MakFS4 records carry Unix-epoch timestamps.
`symlinkat` and `readlinkat` create and inspect persistent absolute or relative
links, path lookup follows intermediate/final links, `AT_SYMLINK_NOFOLLOW`
returns link metadata, and directory streams report `DT_LNK`. Arbitrary dirfd
traversal and Unix-epoch timestamps for legacy/device nodes remain pending.

Directories can be opened read-only as process-owned FDs. `read_dir_fd`
advances shared open-file-description offset and emits `.`/`..` plus real VFS
entries. Musl converts entries into aligned AArch64 `dirent64` records;
`opendir`, `readdir`, `rewinddir`, `dirfd`, and `closedir` execute normally.
Directory seeking uses entry indices. Mounted MakFS4 distinguishes files,
directories, and symlinks across 512 inode records with 255-byte components.
Its in-memory 1,024-bucket collision-chained child index is rebuilt from
authoritative metadata after mount/commit; `getdents64` resumes from raw inode
cursor. `mkdir` verifies parent, `rmdir` requires an empty, unopened directory
outside every process cwd, and nested lookup/list/stat/chdir use canonical
relative paths. Legacy small-volume fallback remains sixteen-slot/32-byte;
on-disk tree indexing and unbounded directory geometry remain pending.

Each process owns a bounded canonical working directory, defaulting to its user
home. `chdir` resolves absolute/relative paths, removes repeated separators and
`.` components, processes `..` without escaping root, verifies directory access,
then commits atomically. Open/stat/create/unlink/rename/path-directory calls use
the same resolver. Process reap deletes cwd state. Musl `getcwd`/`chdir` and
relative file mutations execute in guest tests. `openat` with arbitrary dirfd
and mutable nested directories remain pending.

Open-file descriptions track independent readable/writable rights. Access 2
supports true shared-offset `O_RDWR`; reads, writes, positional I/O, polling,
duplication, and `F_GETFL` preserve both rights. Syscall 11 truncate mode 1
truncates and mode 2 preserves data; mode 0 retains legacy write-open
truncation for existing native apps. Musl always uses explicit mode 1/2.

`ftruncate` accepts writable regular FDs, preserves their shared current
offset, zero-fills growth, and removes truncated bytes. Current MakFS limit is
2 KiB (64 bytes for the legacy user record). `fsync` validates a regular or
directory FD, then issues virtio-blk `VIRTIO_BLK_T_FLUSH`; x86 uses ATA CACHE
FLUSH. Guest tests reopen grown/shrunk files and verify persisted data/size.
Dedicated `pread`/`pwrite` paths never modify the shared open-file-description
offset. Positional writes beyond EOF zero-fill the bounded gap; positional
pipe/directory I/O returns `ESPIPE`. Guest tests combine ordinary and
positional I/O, verify sparse bytes, and check offsets before/after each call.

VM protections are read (1), write (2), and execute (4). Read is mandatory;
write+execute is rejected. Legacy calls 21/22/45 retain one-page semantics.
Range calls round nonzero lengths upward to 4 KiB, support up to sixteen
regions per process and sixteen pages (64 KiB) per region inside a 1 MiB
first-fit arena. New pages are zero-filled. Protect may cover page-aligned
subranges; unmap requires original base and page-rounded region length. Holes
are reused. Region records are process-owned; process teardown forgets records
while page-table destruction reclaims every mapped frame.

`package_install` argument 3 points to contiguous `version || content ||
dependency || signature`. Argument 4 packs one-byte version/content/dependency
lengths at bits 0/8/16 and authentication algorithm at bit 24. Algorithm 1 is a
fixed 256-byte RSA-2048 PKCS#1 v1.5 SHA-256 signature over canonical
`MAKPKG1` manifest fields. Verification precedes store mutation.
Dependency bytes accept legacy built-in `libc` or versioned `MAKDEP1\0` graph:
count byte followed by up to three name/kind/version records; kind 0 is exact,
kind 1 is minimum. Graph bytes remain covered by manifest signature. On current
1 GiB layout, authenticated
payload/version bytes commit to CRC-protected A/B disk snapshots; disks too
small for reserved region retain legacy RAM slots. Static-package overlap or
invalid durable metadata fails closed without formatting.
Install, rollback, and remove require file-mutation capability. Removal commits
an inactive snapshot without target; query then fails, while rollback
atomically re-commits prior authenticated payload generation.
Active payloads appear read-only at `/packages/<name>/payload`; Settings reports
backing mode, durable generation, and active package count.

Current x86 generic-ABI limitations: one generic error value, four-argument
trap path, no async cancellation/timeouts, descriptor transfer, readiness port,
signal delivery, or full binary compatibility. AArch64 target extensions above
provide negative errno, readiness/signals, and AF_UNIX `SCM_RIGHTS`; these are
not yet frozen as cross-architecture normative ABI.

Linux personality fixture selects its dispatcher by process metadata and uses
Linux x86_64 syscall numbers for `write(1)`, `getpid(39)`, `exit(60)`,
`uname(63)`, and `clock_gettime(228)`. Current entry is an `int 0x80` adapter,
not Linux `syscall` instruction compatibility; generic Linux binaries are not
claimed.

Windows fixture is PE32+ x86_64 and calls a userspace Win32 thunk layer using
Microsoft x64 ABI. Kernel validates headers/sections/ranges, enforces W^X/NX,
maps image-base-relative sections, and enters AddressOfEntryPoint. Eight brokered
operations cover `WriteFile`, process ID/time, auto-reset events, handle close,
and process exit. DLL imports, relocations, NT syscall compatibility, registry,
GUI APIs, and generic Windows binaries are not claimed.
