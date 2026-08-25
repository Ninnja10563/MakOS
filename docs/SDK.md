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
kernel-owned thread-affinity get/set masks,
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
normal VFS. Its C subset accepts up to three `int` functions in one translation
unit, each with up to three typed `int`/`int *` parameters. Integer parameters
may be used and assigned in the body.
The body may declare up to four initialized register locals, use integer
locals, unsigned 16-bit constants, parentheses, multiplication, addition and
subtraction, assign expressions to declared integer locals, contain
signed `==`, `!=`, `<`, `<=`, `>`, or `>=` conditions in an `if` whose block returns an expression,
run a bounded assignment-only `while` body, and call one external function with
one through three arguments. A pointer local may be initialized from `&local` or from
`pointer-or-array + constant-or-scalar`, and either form may cross the external-call
boundary. `*pointer` performs a 32-bit load and `*pointer = expression` a 32-bit
store through a pointer local or pointer parameter; parenthesized
`*(pointer + constant-or-scalar)` performs the corresponding scaled access.
Constant pointer addition emits a 64-bit address `ADD` and scales an accepted
0..3 element offset by four. An `int` parameter or non-address-taken scalar
local may offset an unknown-bound pointer through signed `SXTW #2`; known bounds
reject one-past-end constants and unproved variable offsets.
`int *left - int *right` produces the signed element distance by subtracting
the 64-bit addresses and arithmetic-shifting by two. The result is the subset's
32-bit `int`; callers must satisfy C's same-array provenance requirement, and
pointer-minus-scalar is rejected.
A final unconditional return is required.
Fixed local `int` arrays may contain one to four exactly initialized elements
within the same four-slot frame budget. The compiler supports constant indexed
loads/stores and rejects indices outside a known local array; passing a bare
array to the bounded external call decays it to its 64-bit stack address, and
`array + constant` passes the scaled derived address.
It emits AAPCS64 32-bit `int` code, passing up to three arguments in `x0`
through `x2` (`w0` through `w2` for integers), with
validated forward conditional and signed backward branch fixups
and a 96-byte non-leaf FP/LR/x19-x24 frame containing four bounded local slots.
Three-parameter functions additionally preserve `x25` in an aligned unused
frame slot, while smaller functions retain their prior byte layout. It wraps
the result in a real
ELF64 `ET_REL` with `.text`, `.rela.text`, `.symtab`, `.strtab`, and
`.shstrtab`. Multiple definitions carry distinct `.text` offsets and sizes;
relocations may reference a same-object definition or an external undefined
symbol. The companion assembler supplies `_start`; the bounded linker discovers
symbols across two through six objects, resolves three
`R_AARCH64_CALL26` relocations, and produces a validated static `ET_EXEC`.
The primary fixture graph persists one assembly and three C sources as four objects:
`answer` and `adjust` share the program object, while `combine` is defined in a
separate library object. An independent `helper(int value)` translation unit
emits 56 code bytes in a 608-byte object, is linked into the final image, and is
also executed directly to prove `helper(40)=42`. Linking without that required
library fails closed. A separate source emits `sum3(int,int,int)` and its
three-argument caller `invoke3(int)` as 140 code bytes in a 752-byte object;
the linker resolves the internal `CALL26`, selects entry offset 80, and both
functions execute as 42 from RX memory.
The guest reads `/home/user/generated.build` in the versioned `MAKBUILD1`
format. Its input records select language, absolute MakFS source path,
and distinct absolute object path; the final record selects the absolute ELF
output and entry symbol. Parsed fields drive every source read, object
write/reopen, link entry, and final write. The current driver deliberately
accepts two through six inputs, with one `asm` input first and one through five
`c` inputs after it; malformed version, relative/colliding paths, a seventh
input, wrong language order, or missing final link record fail closed.
Authenticated terminal users can run `makbuild generated.build` (or the
absolute `/home/user/generated.build`). Selector 16 validates the home path,
copies it into the toolchain's child-owned SysV startup vector, and launches the
EL0 toolchain with `argv[1]` naming the manifest. `MODE=build` consumes existing
MakFS files and does not seed or overwrite source/manifest inputs;
`selfhost-aarch64` alone requests the deterministic `MODE=fixture` seeding path.
Unsupported tokens, duplicate parameter names, more than three parameters or
call arguments, malformed relocation types, unresolved symbols, duplicate
definitions, and malformed object metadata fail closed.

Build mode derives `<manifest>.state` and therefore accepts manifest paths up
to 90 bytes. The exact 120-byte `MAKSTATE2` record contains its nine-byte
version magic, actual input count, six reserved zero bytes, one manifest
fingerprint, six source-fingerprint slots, and six object-fingerprint slots;
unused slots must be zero. Fingerprints use 64-bit FNV-1a solely for build
cache change detection; they are non-cryptographic and confer no trust. Cached
objects are reused only when the state and source/object fingerprints match and
the object passes the normal ELF parser and symbol validator. A source or
object change selectively recompiles that input. Missing, malformed, corrupt,
or manifest-stale state rebuilds all inputs. Linking and final-ELF output happen
on every invocation, and the state record is written last so an interrupted
build cannot bless partial output. Focused Pi/QEMU TCG runtime proves four-input
hit/miss sequences `0/4`, `4/0`, `3/1`, `4/0`, `3/1`, `4/0`, and `0/4` for
cold, warm, object corruption, rewarm, source edit, rewarm, and state
corruption, followed by a separate three-input graph's cold `0/3` and warm
`3/0` results. All eight CLI builds execute and reap with status 42.

Pointer expressions also accept a scalar `int` parameter or non-address-taken
scalar local as the element offset when the pointer's bound is unknown. AArch64
codegen uses `ADD ... SXTW #2`, so positive and negative 32-bit offsets retain C
signedness. Variable offsets from known-bounded local arrays fail closed because
this seed cannot yet prove their range. Conditions accept signed `==`, `!=`,
`<`, `<=`, `>`, and `>=`; focused guest execution covers every relation and a
`pointer + -1` load.

This seed has no general pointer arithmetic beyond constant/scalar-variable
element addition and typed pointer difference, no pointer-provenance analysis
or broader pointer/lvalue expressions, variable-length/global/multidimensional
arrays, structs,
nested/general blocks, more than three functions per translation unit,
more than six objects, aggregate linked code beyond 512 bytes, general
relocations, preprocessing, optimization,
archives, dynamic linking, transitive dependency/header discovery, variable
input graphs beyond the documented bound, parallel builds,
general CLI options, or debug information. It must not be presented as a general C compiler or a
self-hosted MakOS build.

Current libc is intentionally narrow: no stdio allocator, dynamic linker,
shared libraries, TLS, process-wide environment mutation, bind/listen/accept, nonblocking sockets,
signals, or full POSIX conformance. `user/worker.c` is boot-tested evidence for
static C execution, libc calls, isolation, VM, threads, synchronization, and
graphics.
