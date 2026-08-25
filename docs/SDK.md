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
Those SDK wrappers currently describe x86_64 register details. AArch64 now
implements the same syscall-56 default and fixed 336-byte version-1 syscall-57
descriptor contract over a validated immutable ELF snapshot. Its child receives
the canonical SysV stack plus `argc`, `argv`, and `envp` in `x0`, `x1`, and
`x2`; malformed versions, offsets, strings, lengths, and environment entries
fail before process allocation.
`makos_mprotect` exposes read/write/execute masks while kernel rejects W+X.
`makos_mmap_range`, `makos_munmap_range`, and `makos_mprotect_range` expose
page-rounded multi-region anonymous memory; legacy one-page wrappers remain.
See `docs/SYSCALLS.md`.

## Guest-native AArch64 seed

The `selfhost-aarch64` shell command launches a sandboxed EL0 tool that reads
source from MakFS and writes ELF64 objects and an executable back through the
normal VFS. Its C subset accepts up to two `int` functions in one translation
unit, each with one `int` or `int *` parameter. With an `int` parameter, the
body may use and assign that parameter.
The body may declare up to four initialized register locals, use integer
locals, unsigned 16-bit constants, parentheses, multiplication, addition and
subtraction, assign expressions to declared integer locals, contain
an equality/inequality condition in an `if` whose block returns an expression,
run a bounded assignment-only `while` body, and call one external function with
one argument. A pointer local may be initialized as `int *pointer = &local`,
and an address expression may be passed to the external function. `*pointer`
performs a 32-bit load and `*pointer = expression` a 32-bit store through a
pointer local or pointer parameter. A final unconditional return is required.
Fixed local `int` arrays may contain one to four exactly initialized elements
within the same four-slot frame budget. The compiler supports constant indexed
loads/stores and rejects indices outside a known local array; passing a bare
array to the bounded external call decays it to its 64-bit stack address.
It emits AAPCS64 32-bit `int` code, passing pointer arguments in `x0`, with
validated forward conditional and signed backward branch fixups
and a 96-byte non-leaf FP/LR/x19-x23 frame containing four bounded local slots,
then a real
ELF64 `ET_REL` with `.text`, `.rela.text`, `.symtab`, `.strtab`, and
`.shstrtab`. Multiple definitions carry distinct `.text` offsets and sizes;
relocations may reference a same-object definition or an external undefined
symbol. The companion assembler supplies `_start`; the bounded linker discovers
symbols across up to three objects, resolves two
`R_AARCH64_CALL26` relocations, and produces a validated static `ET_EXEC`.
Unsupported tokens, malformed relocation types, unresolved symbols, duplicate
definitions, and malformed object metadata fail closed.

This seed has no pointer arithmetic, variable-length/global/multidimensional
arrays, structs,
nested/general blocks, more than two functions per translation unit,
more than three objects, general relocations, preprocessing, optimization,
archives, dynamic linking, CLI build driver, or
debug information. It must not be presented as a general C compiler or a
self-hosted MakOS build.

Current libc is intentionally narrow: no stdio allocator, dynamic linker,
shared libraries, TLS, process-wide environment mutation, bind/listen/accept, nonblocking sockets,
signals, or full POSIX conformance. `user/worker.c` is boot-tested evidence for
static C execution, libc calls, isolation, VM, threads, synchronization, and
graphics.
