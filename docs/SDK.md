# MakOS native SDK

Current C17 SDK consists of `sdk/include/makos.h` plus static freestanding
wrapper library `sdk/libc/makos.c`. `kernel/build.rs` cross-compiles both with
Clang for `x86_64-unknown-none-elf`, links through host Rust `rust-lld`, embeds
resulting ELF64 application, then MakOS loads it into isolated ring 3.

AArch64 musl applications built through
`ports/firefox/toolchain/makos-clang` default to
`-fstack-protector-strong`. Musl bootstrap explicitly opts out until its
`__stack_chk_guard`/`__stack_chk_fail` runtime is linked; normal process startup
seeds that guard from virtio-RNG-backed `AT_RANDOM`.

Minimal application:

```c
#include <makos.h>

void _start(void) {
    static const char message[] = "hello from MakOS\n";
    makos_write(message, sizeof(message) - 1);
    makos_exit(0);
}
```

Compile/link recipe mirrors `kernel/build.rs`:

```sh
clang -target x86_64-unknown-none-elf -std=c17 -ffreestanding \
  -fno-stack-protector -fno-pic -mno-red-zone -mcmodel=large -Os \
  -Isdk/include -c app.c -o app.o
clang -target x86_64-unknown-none-elf -std=c17 -ffreestanding \
  -fno-stack-protector -fno-pic -mno-red-zone -mcmodel=large -Os \
  -Isdk/include -c sdk/libc/makos.c -o makos-libc.o
rust-lld -flavor gnu -T user/linker.ld -o app.elf app.o makos-libc.o
```

Query `makos_abi_info(0)` against `MAKOS_ABI_VERSION`; query feature mask with
selector 2. Available wrappers cover console output, VFS open/read/close,
legacy channels, typed 64-byte IPC messages, capability-gated service routing,
generation-safe attenuated handle transfer, stat/directory iteration, page mapping, threads with one argument,
kernel events, process exit, process-owned surfaces, connected AF_INET UDP/TCP
sockets, authenticated package install/query/rollback/removal, and 48 kHz
stereo PCM. `makos_package_install` accepts legacy `libc` or signed `MAKDEP1`
dependency bytes described in `docs/SYSCALLS.md`.
`makos_process_spawn_path` launches a kernel-validated static ELF64 from a VFS
path when process capability is present; it synthesizes `argv[0]` from path.
`makos_spawn_arguments_init` packs up to eight arguments, eight environment
strings, and 256 string bytes into versioned descriptor;
`makos_process_spawn_path_args` copies it into child startup stack. Current
implementation uses two bounded concurrent PID7/PID8 slots. Child entry gets
`argc`, `argv`, `envp` in both SysV stack layout and `rdi`/`rsi`/`rdx`.
`makos_mprotect` exposes read/write/execute masks while kernel rejects W+X.
`makos_mmap_range`, `makos_munmap_range`, and `makos_mprotect_range` expose
page-rounded multi-region anonymous memory; legacy one-page wrappers remain.
See `docs/SYSCALLS.md`.

Current libc is intentionally narrow: no stdio allocator, dynamic linker,
shared libraries, TLS, process-wide environment mutation, bind/listen/accept, nonblocking sockets,
signals, or full POSIX conformance. `user/worker.c` is boot-tested evidence for
static C execution, libc calls, isolation, VM, threads, synchronization, and
graphics.
