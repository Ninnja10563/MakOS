# Build and test

## Supported host path

Apple Silicon macOS uses native ARM QEMU to emulate full-featured x86_64 MakOS
through TCG. HVF cannot accelerate a guest ISA different from host ISA. Native
AArch64 path uses HVF and avoids cross-ISA translation. Interactive runs attach
`build/makos-data-aarch64.img` through virtio-blk and enable Cocoa
`zoom-to-fit` for live host-window scaling.
Data images have 1 GiB virtual size but remain sparse; unused MakFS4 capacity
does not consume 1 GiB host storage. Run scripts extend older images in place.
For a final image retaining an existing account and Firefox profile while
refreshing Firefox, GNU nano, ncurses, and CPython, see
`docs/INTEGRATED-DATA-IMAGE.md`.

```sh
brew install qemu
rustup target add x86_64-unknown-none x86_64-unknown-uefi \
  aarch64-unknown-none aarch64-unknown-uefi
make
make test
make run
make image-x86_64-gpt
make test-x86_64-gpt
make run-x86_64-gpt
make test-x86_64-install
make test-aarch64-cursor-runtime
make test-aarch64-production-smp-runtime
make test-aarch64-native-smp-runtime
make test-aarch64-firefox-runtime
make test-aarch64-ipv6-runtime
make test-aarch64
make run-aarch64
make image-aarch64-gpt
make test-aarch64-gpt
make run-aarch64-gpt
make test-aarch64-install
```

Both architecture GPT targets build one sparse disk with protective MBR, redundant GPT
headers/entry arrays, a 64 MiB EFI System Partition, and an aligned 1 GiB MakOS
data partition. Kernel validates GPT CRCs and the MakOS partition type before
offsetting all filesystem I/O. A disk without a protective MBR retains legacy
raw-data behavior; a protective MBR with invalid primary and backup GPT is
rejected rather than treated as raw storage.

x86_64 GPT uses secondary ATA master; guest installer selects secondary slave
only after administrator login and exact `install disk1 erase-disk1`. Target
must be equal-sized and wholly blank. `install disk1 resume-disk1` accepts only
blank MBR plus zero/source-identical sectors. Shared host tests prove refusal,
resume conflicts, MBR-last commit, and interrupted-copy unbootability. QEMU
install harness SIGKILLs QEMU after the first verified payload block, checks
blank LBA0 plus source-identical partial sectors, resumes, checks full-image
SHA-256 equality, then proves source-detached two-boot persistence.

`make test-aarch64-install` exercises guest-side installation, not a host image
copy shortcut. It creates temporary live/blank virtio-blk disks, enters exact
Terminal confirmation `install disk1 erase-disk1`, SIGKILLs QEMU after first
verified payload progress, and proves LBA0 remains blank while every nonzero
partial block matches source. It reboots with exact `install disk1
resume-disk1`, verifies final source/target SHA-256 equality and MBR-last
commit, then removes live source and boots installed target twice. Installer
flushes disk0 and freezes all source writes across copy/commit, thawing on
error and retaining freeze until shutdown on success. Wrong token,
nonblank/committed/conflicting media, unequal geometry, and absent second disk
are refusal gates. Test never attaches or modifies
`build/makos-data-aarch64.img` or a currently running interactive disk.

`make test-makfs4-guest-fsck` runs the full two-boot AArch64 workload, exports
its data image only after QEMU exits, checks that quiescent real guest volume
with `makos-makfs4-fsck`, then removes the temporary volume.

`make release` runs full two-boot suite, regenerates blank distributable data
disk, then writes deterministic images/source archive/framebuffer PNG and
`SHA256SUMS` under `outputs/`.

Boot tests copy both disk images into an invocation-private temporary directory.
This prevents stale/parallel QEMU processes from locking or mutating build
artifacts. Fixture data seeds one legacy dynamic record; boot 1 migrates it,
while boot 2 injects primary-superblock and bitmap corruption before recovery.
Focused `test-aarch64-cursor-runtime` uses a fresh private sparse data disk,
moves the guest cursor through seven positions, and requires zero changed
virtio-GPU scanout pixels. QEMU still renders the separate guest cursor plane.
Both virtio-GPU queues retain a 10,000,000-spin fast completion path and use a
bounded 200,000,000-spin recovery ceiling. Runtime gates reject timeout/error
markers and require every delayed record to have a matching recovered record
for the same queue and command.
Focused `test-aarch64-firefox-runtime` requires the integrated Firefox package,
and first runs a no-QEMU fail-closed preflight. The package must carry a
canonical record for the pinned Firefox source commit and current ordered
patch-series SHA-256. Before creating or accepting that record, the verifier
reconstructs the expected patched Git tree in a temporary index, hashes the
actual tracked source's raw regular-file bytes, executable mode, and symlink
target bytes directly against expected blob IDs without applying Git clean
filters or line-ending conversion. It rejects type/mode/content differences or
unexpected non-ignored files without modifying the source index or object
store. The resulting tree identity is part of the record, which also binds the
five successfully audited build artifacts and the
five exact stripped runtime artifacts. Package CRCs, AArch64 PIE/shared-object
shape, and runtime hashes are checked before QEMU creation; a historical,
unprovenanced, stale-patch, or mismatched-payload image is rejected. The
successful preflight marker is `MAKOS_FIREFOX_RUNTIME_IMAGE_OK`.
For a clean Firefox source build, run `ports/firefox/clone.sh`,
`ports/rust/build-std.sh`, then `ports/firefox/build-makos.sh`. The scripts
support Apple Silicon/macOS and AArch64 Debian build hosts: set
`FIREFOX_BUILD_PYTHON` to Python 3.11/3.12 and point the documented `MAKOS_*`
LLVM/libclang variables at locally staged tools when they are not installed
system-wide. `ports/firefox/build-makos.sh widget/makos -j1` is a supported
compile-only prerequisite gate and deliberately defers the final binary audit;
it does not produce a package or qualify Firefox runtime behavior.
The supported wrapper selects a complete host C/C++ compiler pair and cbindgen
before expensive source staging. On the Debian Pi it prefers the repository's
staged LLVM 19 pair and staged cbindgen 0.27.0. Explicit
`MAKOS_HOST_CC`/`MAKOS_HOST_CXX` overrides must be supplied together;
`CBINDGEN` may select another version 0.27.0 or newer. Tiny host C and C++
executables are compiled and run so missing, unusable, or wrong-architecture
tools fail before Gecko configure.
Use this supported `mach build` path, never a bare make/relink against a moved
object cache. Before building, it parses `config.status` and `.mozconfig.json`;
if their recorded object directory differs, it runs `mach configure` once and
then requires regenerated autoconf/backend metadata selecting
`nsPrintSettingsMakOS.cpp`. Missing, malformed, still-stale, or incomplete
metadata fails closed without textual substitution. The preserved Pi cache is
known to need this regeneration, so a cheap incremental relink is not assumed.
Before reconfiguration of a release cache preserved under the developer objdir,
the wrapper recoverably moves only the host `release` and target
`aarch64-unknown-makos/release` Cargo trees into
`makos-moved-cargo-quarantine`. Cargo dep-info embeds absolute paths, so those
two trees cannot be reused after the move. C/C++ objects remain in place;
unexpected roots, symlinks, files, or destination collisions fail closed, and
the quarantine is retained for inspection or recovery rather than deleted. A
canonical `migration.json` binds the exact selected/old identities and both
source-to-destination mappings. It is atomically installed and fsynced, then
the selected objdir itself is fsynced to make the quarantine name durable,
before the first rename; destination-only retries are accepted only with that
exact journal. Every retry fsyncs each existing source parent and the quarantine
destination parent even when no rename remains; a source parent for a Cargo
tree that never existed is not fabricated. The journal is accepted only from one no-follow
file descriptor whose metadata and raw bytes match the canonical encoding;
semantically equivalent whitespace is rejected. A crash after
the exact temporary journal is fsynced but before it is linked is recoverable
only when that temp remains a regular owner-matching mode-0600 single-link file
with exact canonical bytes. A crash after linking but before unlinking the temp
is recoverable only when temp and journal are exact two-link names for the same
inode; recovery unlinks the temp, fsyncs the directory, and revalidates the
journal as single-link. Required directory fsync is tested on supported hosts
and fails closed with the path and OS error where unavailable.
The Rust `errno` crate is staged separately because Cargo verifies vendored
checksums. `prepare-rust-errno.sh` accepts only version 0.3.8, reconstructs the
stage from exact source bytes on every invocation, and applies the MakOS cfg
that selects upstream musl's thread-local `__errno_location`. The focused gate
is:

```sh
python3 scripts/test_firefox_errno.py
```

It always runs behavioral temporary-source/stage and Cargo-patch fixtures and
prints whether optional real libc/object symbol checks were performed or
skipped. Set `MAKOS_FIREFOX_ERRNO_SOURCE_DIR`, `MAKOS_FIREFOX_ERRNO_LIBC`, and
`MAKOS_FIREFOX_ERRNO_OBJECT`, plus
`MAKOS_FIREFOX_ERRNO_REQUIRE_FIXTURES=1`, to make all three repository evidence
checks mandatory. This corrects the observed final-link input ABI; it does not
claim a successful fresh `libxul.so` link or Firefox runtime.
The selected Clang installation must include its matching AArch64 intrinsic
resource headers (`arm_neon.h`; Debian LLVM 19 packages this in
`libclang-common-19-dev`). `ports/firefox/test-toolchain.sh` compiles a real
NEON intrinsic probe so a compiler-only LLVM extraction cannot pass and then
fail much later in SWGL or media code.

On a memory-constrained source-qualification host, an unoptimized complete
developer build may be requested explicitly:

```sh
MAKOS_FIREFOX_DEVELOPER_BUILD=1 ports/firefox/build-makos.sh -j1
```

This opt-in mode keeps the ordinary release configuration unchanged, still
requires the complete Firefox binary audit, and never emits release provenance.
It uses `obj-aarch64-makos-developer`, separate from the release object
directory, and removes both the object-root build stamp and any staged runtime
record there before `mach` can modify it, so
interruption also fails closed. Its artifacts prove only that
the full patched source graph compiles and links. The supported/default
packaging flow rejects them because this mode withholds release provenance;
they also cannot be used as Firefox runtime, latency, or macOS/HVF
qualification evidence. Run the default build in `obj-aarch64-makos` to
produce release artifacts and a provenance stamp.
Release binary audit additionally emits `MAKOS_FIREFOX_BUILD_ELF_OK` only after
bounded parsing of all five exact artifacts: `firefox`, `plugin-container`,
`xpcshell`, `libxul.so`, and `libnspr4.so`. The three executables require a
nonzero entry and exact MakOS musl `PT_INTERP` before `PT_LOAD`; both DSOs reject
an interpreter. Load/dynamic file and virtual ranges, artifact-specific
dependencies, and exact libxul/libnspr SONAMEs are fail-closed. The same parser
runs directly over package-image ranges before QEMU, after runtime hashes are
bound by provenance. Self-hashed corrupt, wrong-machine/type/range/entry,
interpreter, dependency, and SONAME fixtures are rejected.

The strict interaction harness proves JIT/blit/client pixels at first paint.
Patch 0057's post-enqueue syscall-149 proof is input-dependent, so its widget
and kernel markers are required immediately after the timed Ctrl-A returns raw
132, not before the first Firefox key. The elapsed Ctrl-A interval is captured
before marker verification and the existing strictly-less-than-10,000-ms gate
is unchanged.
The runtime then requires
strict paint/input/TLS/exact-URI/page-pixel proof, then copies the selected URL
through the MakOS system clipboard, clears the URL bar, pastes, and requires a
second exact-URI page completion. It then left-clicks the real `example.com`
document link and requires exact `https://www.iana.org/help/example-domains`
completion, built-in-root TLS, Firefox pointer routing, and changed page pixels.
The Make target also requires a kernel-published live interval in which at
least two distinct, nonzero TIDs from the launched Firefox process group own
different guest APs simultaneously. Missing, single-CPU, stale-group, or
duplicate-TID evidence fails the run. This adds SMP proof without changing the
existing paint, Ctrl-A, first-input, navigation, interaction, resource, or
survival thresholds.
Focused `test-aarch64-production-smp-runtime` uses the release image and an
upstream-musl Firefox-role pthread fixture, not the packaged Firefox browser.
Its QMP Ctrl-A phase requires a genuine CPU0-targeted GICv2 virtio-input SPI,
direct lower-EL dispatch, exact-handle selection, one target wake with unrelated
waiters retained, AP watcher execution, CPU0 leader handoff, live multi-AP
worker overlap, and status-42 reap. The 100 Hz input poll remains a recovery
path and cannot satisfy the required `MAKOS_AARCH64_INPUT_IRQ_OK` marker. Pi/TCG
results are functional evidence only; strict Firefox timings still require the
integrated package on idle macOS/HVF.
Focused `test-aarch64-ipv6-runtime` runs the full two-boot workload and adds
post-login proof for validated RA/SLAAC, AF_INET6 sockaddr ABI, NDP resolution,
and checksum-valid UDPv6 transmit. QEMU user networking does not return UDPv6
DNS in this host configuration, so TCPv6 and UDPv6 receive remain separate
open gates.

`OVMF_CODE=/path/to/OVMF_CODE.fd` overrides firmware discovery.
`QEMU_SYSTEM_X86_64=/path/to/qemu-system-x86_64` overrides QEMU discovery.
`AAVMF_CODE`, `AAVMF_VARS`, and `QEMU_SYSTEM_AARCH64` override AArch64 tools.
`MAKOS_QEMU_DATA_DIR` supplies QEMU's `-L` data directory to focused AArch64
runtimes when using an extracted, non-system QEMU installation. Such a modular
Debian package may also require `LD_LIBRARY_PATH` to its extracted architecture
library directory and `QEMU_MODULE_DIR` to the adjacent `qemu` module directory;
otherwise the binary can start but fail to resolve libraries or
`virtio-gpu-device`.
On a host using the repository's extracted LLVM, prepend its `bin` directory
to `PATH` and set `MAKOS_NM`, `MAKOS_REAL_CLANG`, `MAKOS_REAL_CLANGXX`, and
`MAKOS_LLD` to the matching pinned tools. The Firefox driver test scopes its
own `MAKOS_CC` wrapper to the Firefox audit, so it cannot replace the bare-metal
compiler used by MicroPython during the same `make unit check` run.

After login, `selfhost-aarch64` runs the deterministic guest-native
compiler/assembler/static-linker gate. Its fixture mode writes an A64 startup to
`/home/user/generated.s` and valid C to
`/home/user/generated-program.c`, `/home/user/generated-library.c`, and
`/home/user/generated-helper.c`, then rereads all four from MakFS. It also
writes and rereads the build description
`/home/user/generated.build`:

```text
MAKBUILD1
asm /home/user/generated.s /home/user/generated-main.o
c /home/user/generated-program.c /home/user/generated-program.o
c /home/user/generated-library.c /home/user/generated-library.o
c /home/user/generated-helper.c /home/user/generated-helper.o
link /home/user/generated-aarch64.elf _start
```

This bounded guest-native build driver accepts two through six inputs: exactly
one assembly input first followed by one through five C inputs. Every input has
a distinct absolute source/object path of at most 96 bytes,
a distinct absolute output path, and one valid entry symbol. The parsed paths
drive all source reads, object writes/reopens, entry-symbol emission/selection,
and the final executable write. Bad version, relative path, colliding paths,
and missing-link manifests fail closed before compilation. A bounded C
translation unit accepts up to six `int` functions, each with up to six typed
`int`/`int *` parameters, 0..65535 constants,
parentheses, unary `+`/`-`, and precedence-correct `*`, signed `/`/`%`, `+`,
and `-`, up to four register locals,
mutable parameter/local assignments, signed `==`, `!=`, `<`, `<=`, `>`, and
`>=` comparisons, one conditional `if` block containing a return, a bounded `while` body containing one
or more assignments, a one- through six-argument function call, and bounded pointer
initializers from `&local` or `pointer-or-array + constant-or-scalar`. Address and pointer
expressions may be passed to the bounded external call. Dereference expressions
load a 32-bit `int` through a pointer local or pointer parameter;
`*pointer = expression` stores it back. Parenthesized
`*(pointer + constant-or-scalar)` supports the same scaled load/store form.
Constant addition uses a 64-bit AArch64 `ADD` with a 0..3 element offset scaled
by `sizeof(int)`. A scalar `int` parameter or non-address-taken scalar local may
also supply the offset for an unknown-bound pointer; codegen uses signed
`SXTW #2`, and guest execution proves both positive offsets and `-1`. Known
local-array/derived-pointer bounds reject one-past-end constants and all
variable offsets whose range this compiler cannot prove.
Signed 32-bit division emits AArch64 `SDIV`; signed remainder emits `SDIV`
plus `MSUB`, retaining C's truncation-toward-zero and dividend-sign remainder
semantics. A direct literal-zero divisor is rejected before code emission.
Subtracting one typed `int *` expression from another emits a 64-bit `SUB`
followed by arithmetic shift-right two, producing a signed element count as
the subset's 32-bit `int`. The caller remains responsible for C's same-array
provenance rule; the compiler rejects pointer-minus-scalar syntax.
Fixed local `int` arrays accept one to four exactly supplied initializer
expressions, subject to the shared four-slot frame limit. Constant indices are
bounded to 0..3 and known local-array indices are checked against the declared
length. Indexed expressions and assignments emit scaled 32-bit loads/stores;
a bare array call argument decays to its 64-bit stack address; an accepted
`array + constant` call argument passes the scaled derived address. Arguments
occupy AAPCS64 `x0` through `x5`, using the `w` view for `int` values.
A final unconditional return is required so every accepted path returns.
Non-leaf functions preserve FP/LR and x19-x24 in a 96-byte AAPCS64 frame with
four bounded 32-bit local slots. A three-parameter function additionally saves
and restores `x25` in the frame's aligned unused slot. Four- through
six-parameter functions use a 112-byte frame and save/restore `x25` through
`x28`; one- and two-parameter code remains byte-for-byte unchanged. The
current `answer` initializes `values[3]` with `(value * 3) - 20`, 40, and zero,
then calls `adjust(values + 1, 1)` when element zero is at least 40; otherwise it
returns 86. `adjust(int *pointer, int delta)` accepts its pointer in AAPCS64
`x0` and delta in `w1`, preserves them in `x23`/`w24`, derives
`next = pointer + delta`, adds delta to element zero,
computes `distance = next - pointer`, then uses `while (count < distance)` and
`*(pointer + delta)` to store through the
dynamically derived address and advance the counter. Its return reloads through
`next`. `adjust` calls `combine(int value, int delta)` from the separate library
translation unit. The compiler emits 140-byte `answer` and 168-byte `adjust`
definitions in a 308-byte `.text` and 976-byte `generated-program.o`; it emits
the 60-byte `combine` in a 616-byte `generated-library.o`. The assembler emits
76 code bytes in the 688-byte `generated-main.o`. An independent fourth C
translation unit emits the 56-byte `helper(int value)` definition in the
608-byte `generated-helper.o`; direct RX execution proves `helper(40)=42`.
All four genuine ELF64 `ET_REL` files persist/reopen. The program object's symbol table defines
`answer` at offset zero and `adjust` at offset 140, while `combine` is undefined
there and defined at offset zero in the library object. The bounded linker
discovers definitions and undefined symbols across all four, applies external
`_start`→`answer`, same-object `answer`→`adjust`, and external
`adjust`→`combine` `R_AARCH64_CALL26` relocations, includes the independent
`helper` definition, and emits 500 code bytes in the
1,583-byte `/home/user/generated-aarch64.elf`. Fully linked RX calls require
`answer(20)=42`, `answer(0)=86`, `adjust(forty,1)=42`,
`adjust(scaled,2)=44`, and `adjust(zero,1)=2`, with the three-element direct-call arrays also
required to become `41:42:0`, `42:0:44`, and `1:2:0`. Separate RX probes cover
all four signed ordering relations, load 42 through `pointer + -1`, and return
signed pointer differences of `3` and `-3`. A separate two-function source
compiles `sum3(int,int,int)` plus `invoke3(int)`, preserves the third parameter
from `x2` in `x25`, emits 140 code bytes in a 752-byte ELF64 `ET_REL`, resolves
the same-object `invoke3`→`sum3` `R_AARCH64_CALL26` relocation, links with entry
offset 80, and executes both `sum3(40,1,1)` and `invoke3(40)` as 42 from RX
memory. A separate `sum6`/`invoke6` unit exercises all six integer argument
registers `x0` through `x5`. The six-parameter callee preserves its inputs in
`x23` through `x28`, uses a 112-byte frame with explicit save/restore pairs,
emits 196 code bytes in a parsed 808-byte ELF64 `ET_REL`, resolves its
same-object `R_AARCH64_CALL26`, and executes direct `sum6(10,5,6,7,8,6)` plus
`invoke6(37)` as 42 from RX memory. Seven-parameter and seven-argument sources
fail closed. A separate three-definition arithmetic unit emits 168 code bytes in a
784-byte parsed ELF64 `ET_REL`, links it with entry offset zero, and executes
positive and negative division (`6`/`-6`), remainder (`2`/`-2`), and negation
(`-42`/`42`) from RX memory. A separate six-definition unit forms a call chain
from `stage6` through `stage1`. It emits five genuine same-object
`R_AARCH64_CALL26` relocations into a parsed ELF64 `ET_REL`, links with
`stage6` as the selected entry, transitions through writable/NX to RX, and
executes `stage6(36)=42`. The compiler and object validator now bound one unit
at six definitions and eight call relocations; a seven-definition source fails
closed. Invalid
relocation type/addend/site, unresolved `adjust`, a missing library object, and
duplicate `answer` inputs are denied, as are unsupported bitwise syntax,
literal-zero division/remainder, a conditional-only function, a loop
without a terminal return, assignment to an undefined variable, address-of an
undefined local, pointer reassignment outside the typed initializer, and
returning a pointer or address expression as an `int`.
Known local-array out-of-bounds indexing, known one-past-end pointer derivation,
unproved variable offset from a known-bounded local array, pointer-minus-scalar,
duplicate functions/parameters, a seventh function, and more than six parameters or call arguments
are also denied.
The shell launches the final ELF through syscall 56 with default `argc=1`, then
syscall 57 with three arguments and one environment string; `_start` validates
both forms, passes 20 to compiled `answer`, and exits with its result 42. Three
malformed startup-vector forms must be denied.

Run the focused gate with:

```sh
make test-aarch64-selfhost-runtime
```

The unchanged gate now stages one bounded parallel-build phase. After login,
the authenticated fixed command `makbuild-parallel` launches the existing
disjoint cold manifests `/home/user/generated-three.build`,
`/home/user/generated-header.build`, and
`/home/user/generated-nested.build`. All three spawn syscalls occur before the
first wait. The shell then waits for and reaps every successfully launched
child, requires status 42 from each, writes each complete per-build success
record atomically, and finally emits:

```text
MAKOS_AARCH64_MAKBUILD_PARALLEL_OK spawn_before_wait=3 statuses=42,42,42
```

Syscall 126 returns a consumed exit status plus one, zero for Pending, and -10
for NoChild, so the shell cannot confuse a still-running process with a missing
child. A partial spawn failure still drains every launched sibling and cannot
emit the parallel-success marker. Toolchain spawning remains restricted to the
authenticated Shell role and manifests under `/home/user/`; failed session
registration terminates, reaps, and discards the unregistered child.

The scheduler selects idle AP1-3 for the distinct singleton Toolchain leaders.
While holding its scheduler lock it captures a one-shot overlap snapshot only
when all three APs own running leaders with exact affinity, unique PID and
TTBR0, matching process-group leadership, and singleton ownership. CPU0 later
emits the snapshot outside the lock as
`MAKOS_AARCH64_TOOLCHAIN_PARALLEL_OK`; timer rebalancing may migrate a leader
only to an actually idle AP. The focused harness therefore validates each
ordered migration chain and requires the immutable snapshot CPU to occur in
that child's visited CPU history, rather than incorrectly requiring it to be
the final endpoint after a legal post-snapshot migration. It also accepts
nondeterministic child-build marker order while requiring all three placement
and process records before the first reap/SMP summary, exact positional
PID/group/root correlation, three status-42 reaps, and the preserved cold,
warm, output, and execution checks.

This implementation is staged through
`5db7e3588227a97e878c989fab3e1361ddff8ed5`. Focused process-table,
scheduler/self-host structural, synthetic parallel-evidence, Python syntax,
and strict AArch64 shell compilation checks pass; the combined focused log is
`build/logs/parallel-combined-focused-20260902.log`, with SHA-256
`9bd3ad49858c4335b2db997c4974c310e5601a2e461554435b5ced1be72be583`.
No QEMU runtime was run for this increment. The next unchanged runtime gate
expects eight graphs, 20 authenticated CLI builds, and 21 Toolchain processes,
including these three simultaneous cold graphs; this is an expectation, not a
pass. The authoritative prior Pi/QEMU TCG result remains five graphs, 16 CLI
builds, and 17 Toolchain processes. Scheduling, SDK, and self-hosting remain
Partial.

The focused gate also proves a repository-source path that is distinct from
the synthetic language fixtures. `kernel/build.rs` reads the exact bytes of
`user/aarch64_selfhost_probe.c` and `user/aarch64_selfhost_probe.S`, compiles
the C file once with the configured host AArch64 clang as
`aarch64-selfhost-reference.o`, and generates the byte include consumed by the
guest toolchain. Fixture mode validates FNV-1a identities before writing those
same bytes to `/home/user/makos-repo-probe.c` and
`/home/user/makos-repo-probe.s`, alongside this manifest:

```text
MAKBUILD1
asm /home/user/makos-repo-probe.s /home/user/makos-repo-probe-main.o
c /home/user/makos-repo-probe.c /home/user/makos-repo-probe-c.o
link /home/user/makos-repo-probe.elf _start
```

The runtime requires the exact source marker (440 C bytes, 53 assembly bytes,
FNV-1a `5d0b854c29106f84` and `7ad8871bd0e68af4`), a cold guest build with two
misses, a warm build with two hits, and `run makos-repo-probe.elf` exiting 42.
Changing either tracked source changes the generated identity and the runtime
expectation together; a hard-coded substitute cannot satisfy the gate.

The same gate is now extended to qualify bounded file-scope data against the unchanged
production source `ports/musl/shared-demo.c`, not a normalized copy. MakOS
exposes it read-only as `/usr/src/makos/ports/musl/shared-demo.c` and exposes
the bounded SDK header `sdk/selfhost/include/stdint.h` read-only as
`/usr/include/stdint.h`. The manifest remains writable under `/home/user`:

The guest compares both files byte-for-byte and by generated length/FNV identity.
It separately denies a writable preserving open (`open` mode 2) and a writable
truncating open (`open` mode 1), denies replacement, then rereads the original
bytes before reporting the two read-only claims.

```text
MAKBUILD1
asm /home/user/makos-shared-demo.s /home/user/makos-shared-demo-main.o
c /usr/src/makos/ports/musl/shared-demo.c /home/user/makos-shared-demo-production.o
c /home/user/makos-shared-mutable.c /home/user/makos-shared-demo-mutable.o
link /home/user/makos-shared-demo.elf _start
```

The compiler accepts the source's exact `uint64_t`, default-visibility
attribute, constant string object, and 64-bit addition. Its object contains
typed function/object symbols plus bounded `.text`, `.rodata`, and `.data`;
global addresses use validated paired `R_AARCH64_ADR_PREL_PG_HI21` and
`R_AARCH64_ADD_ABS_LO12_NC` relocations. The final static ELF separates R-X,
R--, and RW/NX load regions. Focused runtime requires cold `0/3`, warm `3/0`,
`makos_shared_add(20,22) == 42`, a relocated read from the production string
object, and writable data mutation. The build process performs real mutated-object
negative checks for malformed pairs and unresolved, duplicate, or out-of-range
data symbols before emitting its success marker. These runtime
requirements remain pending until the focused gate is actually run; this does
not claim a self-hosted shared library.

The linker uses a 4 KiB in-memory ET_REL work buffer so section tables and
relocations can be validated without truncation. Persisted MakFS objects and
executables still have the filesystem's 2 KiB per-file ceiling; writes fail
closed when serialized output exceeds it. The packed final ELF uses file
offsets 0/1024/1536 and distinct virtual pages for R-X/R--/RW-NX mappings.
Empty read-only or writable regions are omitted rather than serialized as
zero-sized `PT_LOAD` entries.

Each authenticated compiler/assembler/linker invocation is a real sandboxed
EL0 process with the distinct `Toolchain` role. Its leader is not pinned to
CPU0: the kernel snapshots AP1-3 dispatch counts, prefers idle APs, chooses the
least-dispatched candidate, rotates ties, and installs a singleton affinity.
That affinity is not permanent. At a timer-safe exception boundary, an
eight-dispatch imbalance causes the kernel to capture the full context, move
the singleton affinity to an idle lower-load AP under the scheduler lock,
publish the task Ready/unowned, and wake the destination by SGI. The source
cannot retain ownership and the destination restores GPR/SP/TLS/SIMD state.
The focused gate validates every placement decision and requires all three APs
to receive work across 21 toolchain processes. It also validates every
migration's measured loads and affinity transition, requires nonzero source
and destination masks contained in `0xe`, and rejects dropped evidence.
Console bytes written by an AP
still update retained terminal state, but graphics composition is coalesced
and deferred to CPU0; the final marker requires nonzero AP deferrals and CPU0
owner compositions with no pending handoff or off-owner virtio-GPU MMIO. This
is bounded automatic placement and dynamic migration of single-threaded
toolchain leaders, not general load-driven migration of arbitrary desktop
processes.

The gate also proves bounded nested assignment/control bodies with continuation.
It compiles `if`, `if`/`else`, nested `if`, and a `while` containing
`if`/`else` into a genuine ELF64 `ET_REL`, links separate `choose`, `bump`,
`nested`, and `accumulate` entries, applies W^X, and requires results
`42,2,5,8,42,2,1,6`. Control nesting is capped at four; empty `else`, a fifth
level, and declaration-bearing nested bodies remain denied. This is not
general block or nested lexical-scope support.

The gate is a real but bounded A64 C-compiler/assembler/static-linker seed. It
has no general pointer arithmetic beyond constant/scalar-variable element
addition and typed pointer difference, no pointer-provenance analysis or
broader pointer/lvalue expressions, variable-length or multidimensional
arrays, aggregates, arbitrary global initializers, internal-linkage `static`
objects, tentative/common objects, TLS, structs, nested/general blocks beyond that bounded depth-four control
form, more than six functions or parameters per translation unit, or a
general object/relocation repertoire. Angle includes are limited to the exact
bounded `/usr/include/stdint.h`; this is not a general system-header,
preprocessor, or transitive dependency engine, and input graphs remain bounded.
The
authenticated shell command `makbuild <manifest>` accepts either a name under
`/home/user/` or an absolute `/home/user/` path. The kernel validates and copies
that path into the sandboxed toolchain's child-owned SysV `argv[1]`; build mode
reads the existing MakFS manifest and sources without seeding or overwriting
them. The deterministic self-host fixture uses a separate `MODE=fixture` startup
and is the only path that seeds the documented files. The fixture also seeds a
separate three-input manifest, a two-input quoted-header graph, the exact
two-input repository-source graph, the three-input read-only production-source
graph, separate two-input const-only and mutable-only data graphs, and a
three-input nested-control graph; focused runtime builds all eight. The current
CLI remains bounded to one leading assembly
input plus one through five C inputs. It is not a
full C/Rust compiler, general linker/build system, debugger, or end-to-end
in-guest OS build. The repository probe is the first exact tracked component,
not a claim that MakOS can yet build substantial parts of itself.

`makbuild` now persists a bounded incremental cache beside the manifest as
`<manifest>.state`; consequently the manifest path itself is limited to 90
bytes so the `.state` suffix remains in the validated path bound. `MAKSTATE2`
is exactly 120 bytes: a nine-byte magic, one-byte actual input count, six
reserved zero bytes, one manifest fingerprint, six source-fingerprint slots,
and six object-fingerprint slots. Unused fingerprint slots must be zero. These
are 64-bit FNV-1a build fingerprints, not cryptographic
integrity or a security boundary. A cache hit additionally requires the object
bytes to match their saved fingerprint and pass the existing ELF object parser
and symbol validator. Every build still links and writes the final ELF; it
commits state only after every rebuilt object and the final ELF have been
written. Missing, malformed, stale-manifest, or corrupt state safely rebuilds
all actual inputs. Changed source or corrupt/missing object selectively rebuilds
only the affected input.

For a C input, `makbuild` recognizes exact `#include "<absolute-path>"`
directive lines, with optional leading spaces or tabs, anywhere in the source
or a resolved header. It recursively reads each bounded header through the
guest VFS, substitutes its bytes at the directive, and compiles and
fingerprints the fully expanded byte stream. Paths must be absolute, use the
same bounded path alphabet, and not collide with a manifest
source/object/output. Discovery is capped at four nested headers and eight
unique dependencies, with an active-path stack for cycle detection. Empty,
missing, oversized, relative, malformed, cyclic, or over-depth includes fail
closed. The same pass accepts up to eight 16-byte macro names. Object and
function-like replacements may contain up to 64 printable bytes; a
function-like definition must place `(` immediately after its name and may
declare up to four distinct parameters. Active C lines use bounded identifier
token rescanning with nested-parenthesis argument parsing, pre-expanded
arguments, a 64-byte bound per argument, a 256-byte substitution scratch
buffer, and an eight-level expansion limit. Direct or indirect recursion,
wrong arity, duplicate or excess parameters, `#`/`##`, backslash continuations,
and limit overflow fail closed. Variadic macros are not accepted. The pass
also processes `#ifdef`, `#ifndef`, `#if`, `#elif`, `#else`, and `#endif`,
bounded to four conditional levels within each source/header. `#if`/`#elif`
use a bounded recursive evaluator with checked signed 32-bit semantics. It
supports `defined(NAME)` or `defined NAME`, numeric literals and numeric
signed-decimal object macros, unknown identifiers as zero, unary `+`, `-`, `!`, and `~`,
multiplicative and additive arithmetic, shifts, equality/relations, bitwise
`&`, `^`, and `|`, short-circuit `&&` and `||`, and right-associative
conditional `?:`. The precedence and associativity follow C for this subset.
Only the selected conditional arm is evaluated, while both remain
syntax-checked. Active division or remainder by zero,
invalid shift counts, negative or overflowing left shifts, and signed overflow
fail closed; unevaluated logical operands are still parsed but do not trigger
those semantic errors. Literal and macro magnitude remains capped at 65,535
and every evaluated intermediate must fit `int32_t`. Exactly one branch may be
selected. Function-like invocations are expanded on active C lines, not inside
`#if`/`#elif`; expression macros therefore remain signed-decimal object macros.
Redefinition, malformed/unbalanced expressions or conditionals, and `#elif`
after `#else` fail closed. Each expanded header is capped at 1,280 bytes. This
is not a general preprocessor: it has no variadics, token
pasting/stringification, multiline definitions, system include search, or
unbounded include graphs.

The fixture seeds `/home/user/generated-header.build`, a small assembly
startup, `/home/user/generated-header.c`,
`/home/user/generated-inline.h`, and `/home/user/generated-leaf.h`. The C unit
defines a function before including the guarded root twice. The root's
defined/undefined branches include a guarded leaf. That leaf sets
`INCLUDED_DELTA=1`; a false `#if` is followed by a selected `#elif`, which
defines `ACTIVE_DELTA=2` for the emitted function. `RETURN_TYPE` supplies a
text replacement and two-parameter `APPLY_DELTA(value, delta)` expands nested
object-macro arguments into the compiled function. The leaf is 1,215 bytes
under the 1,280-byte cap. The leaf also proves every
new arithmetic, shift, and bitwise tier, C precedence with `1 == 2 < 3`, and
short-circuit suppression of a zero divisor and invalid shift. Conditional
expressions prove both arm directions, logical-before-conditional precedence,
right associativity, and selected-arm-only evaluation. Separate active
zero-divisor, shift-range, overflow, malformed-conditional, and
selected-conditional-trap fixtures fail closed. Separate definitions and
invocations deny duplicate/five-parameter forms, wrong arity, recursive
expansion, and `#` stringification. The repeated root is
deduplicated and inactive missing-header branches are skipped. The focused gate
builds this two-object graph cold (`0/2`) and warm
(`2/0`), edits only the leaf header from the authenticated
guest shell, proves selective dependent-object rebuild (`1/1`) and rewarm
(`2/0`), then uses `run generated-header.elf` to launch the normal validated
ELF-by-path loader and reap status 42.

The focused runtime proves four-input cold `0/4`, warm `4/0`, corrupt-object
`3/1`, warm `4/0`, changed-source `3/1`, warm `4/0`, and corrupt-state `0/4`
hit/miss sequences, then separate three-input cold `0/3` and warm `3/0`, plus
header-graph cold `0/2`, warm `2/0`, edited-header `1/1`, and rewarm `2/0`
results. The nested-control graph then proves cold `0/3` and warm `3/0`, emits
564 linked code bytes under the 1,024-byte aggregate bound, writes a 1,583-byte
two-segment ELF whose read-only/NX segment starts at offset 1,536, and executes
that persisted output through the ordinary loader with status 42. All sixteen
authenticated CLI builds link and reap with status 42; the state-invalidated
build re-establishes a valid cache. This is bounded
incremental reuse with bounded recursive quoted-header discovery, object/text
and four-parameter function-like macros, include guards, and the documented
expression subset—not a general preprocessor,
parallel builds, an
arbitrary graph beyond six inputs, a general dependency engine, or a trust
mechanism. The linker has a 1,024-byte aggregate code bound and fails closed
when a user-supplied accepted graph exceeds it; individual build translation
units retain their separate 512-byte code workspace.

Linux uses equivalent Rust targets plus distro QEMU/OVMF packages. Image
creation requires only Python 3 and does not mount filesystems.

Every AArch64 boot also runs a bounded EL0 SMP scheduler proof before mounting
storage. Three AP contexts rendezvous immediately before EL0 transition, CPU0
joins and releases all four PEs, and four independent processes must overlap,
block on an absolute monotonic sleep with no local successor, return to AP idle,
resume after CPU0's timer wake, return statuses 40-43, reap cleanly, and restore
the free-frame count. They also perform a 20 ms timed futex wait with CPU0 in
WFI and APs back in their idle dispatchers. The marker requires
`idle_mask=0xe`, `resume_mask=0xe`, `futex_idle_mask=0xe`, and
`futex_resume_mask=0xe`. A zero-descriptor 200 ms `poll` must also return APs to
idle, retry the original SVC after timer wake, and report
`io_idle_mask=0xe`/`io_resume_mask=0xe`. A second EL0 fixture creates an
auto-reset event, clones a shared-VM thread, blocks its leader on AP1, signals
from the child on CPU0, and requires AP1 idle/resume masks `0x2`, child
thread-return status 0, parent status 44, and balanced frames. The scheduler
closes AP dispatch between boot fixtures. After driver/login-UI initialization,
the desktop opens a bounded production policy: leaders, shell, UI, and service
roles remain on CPU0, while non-leader Firefox and ordinary native-application
threads are eligible on AP1-3. Device MMIO remains CPU0-owned; this is not
unrestricted multicore userspace.

Run the post-desktop production-role gate with:

```sh
make test-aarch64-production-smp-runtime
```

The shell's `firefox-smp` command executes the upstream musl pthread workload
under the exact Firefox scheduler role. A production-only three-pthread
rendezvous holds distinct workers Ready while the shared queue dispatches them.
The kernel's target syscall 148 owns each task's CPU mask. Official-musl patch
65 maps Linux AArch64 `sched_getaffinity` and `sched_setaffinity` onto that ABI.
Before any affinity syscall, the kernel assigns each default worker a
least-reserved AP preference while leaving its reported mask at `0xe`. The
fixture creates a real load imbalance by repeatedly yielding one default
worker while two peers sleep; at a 64-dispatch difference, timer preemption
moves it through Ready/unowned without caller selection. The leader verifies
its fixed CPU0 mask; each worker then selects and reads back two different
singleton AP masks, restores the production AP-pool mask `0xe`, and joins. An
explicit affinity request disables automatic preference and stays authoritative.
It then creates two real process-owned overflow surfaces and blocks target and
decoy pthreads in `surface_wait_event`. Input waits retain their exact handle;
QMP Ctrl-A must select handle 7, wake exactly that watcher on AP1-3, leave the
decoy and every unrelated empty-surface waiter blocked. On key dequeue the
kernel records the exact Firefox watcher/group and arms an older-widget
compatibility boost. The widget/fixture invokes AArch64 target syscall 149 only
after the key is in its bounded queue and a Gecko-main drain runnable is
already present or was successfully posted. The kernel accepts only that exact
watcher, refreshes bounded CPU0 leader priority, sends a scheduler SGI, and
then requires the leader to dispatch on CPU0. Feature bit 23 advertises this
post-enqueue contract. Destroying surface 8 must wake its decoy so the retried
syscall fails closed and the join completes. Priority is one-shot after a
successful dispatch; the deadline only expires stale hints. The gate also requires an AP CPU mask, nonzero dispatch counters, a
simultaneous multi-AP interval with distinct TIDs, exclusive ownership, the
complete upstream-musl pthread/IPC workload, and status-42 reap. The final
Raspberry Pi/QEMU 10.0.11 TCG pass records all APs (`cpu_mask=0xe`), automatic
placements `4,2,14`, three natural load migrations with zero evidence drops,
live/final overlap on AP1/AP2 (`overlap_mask=0x6`, TIDs 5/6), watcher TID 8 on
AP2, dispatch counts `10650,13916,10330`, one targeted wake, accepted syscall
149, and three skipped
unrelated surface waiters. The runtime also requires at least three explicit
affinity changes that exclude the source PE and confirms all three workers
restore mask `0xe`. AArch64 serial output is protected by an
IRQ-masked cross-PE lock so these records cannot interleave by byte. This is a
scheduler-role fixture, not real Firefox or
macOS/HVF performance evidence. Real Firefox must still pass the unchanged
strict Gate 3 on the intended idle host. The historical integrated image
`makos-integrated-a9c604254f094de2.img` predates patch `0057`; rebuild and stage
a new Firefox package/integrated image as
`build/makos-integrated-firefox-handoff149.img` (or override
`AARCH64_FIREFOX_PACKAGE_IMAGE`) before running that gate.

Run the ordinary native-application role gate with:

```sh
make test-aarch64-native-smp-runtime
```

The gate first sends the shell's `native-smp` command to execute the freshly
built upstream-musl pthread workload under `ProcessRole::Native`. Its leader
must retain mask
`0x1` on CPU0; three non-leader workers are automatically placed across the
shared AP pool, create and prove a natural load migration, then force and read
back singleton migrations across AP1-3, restore mask `0xe`, and run
the remaining pthread/IPC workload to status 42. The host gate requires every
AP to have a nonzero dispatch count, a live/final matching overlap interval
with at least two distinct TIDs, exclusive ownership, CPU0-only device MMIO,
and shell wait/reap. It then sends `python-smp`, runs a separately tagged copy
of that real pthread workload under `ProcessRole::Python`, and requires the
same AP coverage, locked Running-owner overlap, automatic migration, join,
status-42 exit, wait, and reap. The current Pi/TCG pass records Native
placements `3,13,2`, two automatic migrations and dispatches
`10096,9841,13848`; Python-role placements are `1,1,1`, with one automatic
migration and dispatches `12997,9286,9224`. The Python phase proves scheduler
policy for the built-in role, not execution of MicroPython or CPython. This is
production-policy evidence, not Firefox or macOS/HVF performance evidence.

The ordinary image also contains a bounded forced-migration proof, with a
focused early-exit harness:

```sh
make test-aarch64-smp-migration-runtime
```

One immutable TID begins on AP1, enters an armed yield, and resumes on AP2 only
after the scheduler has captured its context and published it Ready/unowned
under the process lock. The 90-second gate requires the same TID on both PEs,
source/target masks `0x2`/`0x4`, exactly one migration, exclusive ownership,
GPR/SP/TLS/SIMD preservation, status 71, and frame balance. This is Pi/TCG
functional evidence for forced migration, not automatic load balancing or
general desktop SMP.

The same ordinary image contains a six-task shared-Ready-queue contention
proof. Run its focused gate with:

```sh
make test-aarch64-smp-load-runtime
```

Each task performs 48 real yield syscalls and exits with its distinct status
80-85. The gate requires all three APs (`cpu_mask=0xe`), at least 288 yield
contention points, exclusive single-CPU ownership at every recorded selection,
bounded dispatch skew, exact reap/frame balance, and the prior migration proof.
The 2026-08-25 Raspberry Pi/QEMU 10.0.11 TCG pass records 99 dispatches on each
AP (297 total). This is functional AP load-sharing evidence; production scope
now includes Firefox, native, and Python-role workers, while additional
built-in/service roles and macOS/HVF Firefox qualification are still required.

The boot also runs a remote `exit_group` fixture. A CPU0 leader clones a worker
fixed to AP1; after the worker proves active EL0 execution, the leader invokes
syscall 119 with status 55. The kernel must publish the dying group, prevent
redispatch, send an SGI, detach the AP1 task on AP1, switch that PE to the
kernel root, and report matching `stopped_cpu_mask=0x2`/`ack_mask=0x2` before
reaping. The marker also requires one shared-root reap and balanced frames.

A second teardown fixture places AP1 inside the real yield SVC before its CPU0
leader invokes syscall 119 with status 56. The bounded boot-probe rendezvous
must report `entered_el1_mask=0x2`; the safe EL0-return boundary must detach the
worker, switch to the kernel root, and report
`deferred_ack_mask=0x2` together with matching target/ack masks. This also
guards the race where an exception entered scheduling just before publication.
Normal yield behavior is unchanged outside the armed boot fixture.

A fifth teardown fixture starts independent status-57 and status-58 processes
on CPU0/AP1 and rendezvous-blocks both inside syscall 119 before coordinator
acquisition. It requires `cpu_mask=0x3`, `rendezvous_mask=0x3`, and
`serialized_acquire_mask=0x3`, proving bounded max-one serialization without
holding the scheduler lock. All AP kernel stacks are 1 MiB; the boot marker
must report `stack_bytes=1048576`. The former 64 KiB allocation was insufficient
for two concurrent full cleanup paths and could overwrite adjacent kernel
state.

The same-group teardown fixture clones a shared-root worker and enters syscall
119 concurrently from CPU0/AP1 with requested statuses 59/60. It requires two
arrivals, exactly one owner, the complementary joined caller, first-owner-wins
status propagation, one root reap, and exact frame balance. The joining CPU
leaves the shared TTBR0 and acknowledges under the process lock; it never runs
a second group cleanup.

The focused device-wake gate uses a separate boot configuration so a normal
boot never waits for harness input:

```sh
make test-aarch64-smp-input-runtime
```

`boot/MAKOS-SMP-INPUT.CFG` arms an AP1 EL0 `read_key` waiter only after
virtio-input initialization. The harness waits for the guest readiness marker,
sends QEMU Ctrl-K, and requires exclusive CPU0 used-ring polling plus an SGI to
resume AP1 from its idle dispatcher. The runtime marker must report nonzero
`owner_activity` and `ap_deferrals`; the structural guard permits the one
low-level poll call only inside the CPU0 owner wrapper and makes the driver
fail closed on a non-owner CPU. Exact idle/resume masks, key-derived status 61,
frame balance, and normal boot completion are mandatory. The harness waits for
the complete readiness line so a partial serial read cannot trigger input
early. `boot/MAKOS.CFG` does not contain the test option.

Before the keyboard phase, the same focused image runs a real UDP/DNS TX/RX
phase. AP1 copies transaction `0x4d4c` into a bounded eight-slot service queue;
CPU0 alone performs the virtio-net transmit, then AP1 blocks in receive. CPU0
enters an ordinary syscall-free EL0 counter loop while the response arrives.
QEMU `virt` network slot 28 maps to GICv2 SPI INTID 76; its lower-EL handler
drains the CPU0-owned RX ring before EOI and wakes AP1 by SGI. An EL1 entry only
acknowledges transport status, and the 100 Hz owner pump remains a recovery
path. The gate requires nonzero `owner_transmits`/`ap_tx_requests`,
`owner_frames`/`ap_deferrals`, and `irq_frames`, exact
`entry=lower-el dispatch=direct`, matching I/O idle/resume masks, validated DNS
response status 63, and balanced frames. The marker reports
`delivery=gicv2-spi intid=76`, `tx_transport=bounded-copy-queue`, and
`tcp_ap_tx=cpu0-service-ready runtime=separate-tcp4-probe`: copied UDPv4/v6 is
qualified here. The receive phase separately proves device-IRQ RX plus AP
scheduler idle/wake.

Stateful AP TCPv4 has its own focused image and host fixture:

```sh
make test-aarch64-smp-tcp-runtime
```

`boot/MAKOS-SMP-TCP.CFG` arms only this network proof. AP1 creates a TCPv4
socket, connects through QEMU slirp to the harness listener at
`10.0.2.2:18080`, sends exact `MAKOS_AP_TCP_TX\n`, blocks in receive, verifies
exact `MAKOS_CPU0_TCP_RX\n`, and closes with FIN. CPU0 exclusively owns route
resolution and virtio-net TX/RX; AP connect and segment state cross the bounded
copied queue. The listener delays its response by 0.5 seconds, forcing the AP
through the scheduler idle/wake path. The 90-second gate requires exact bytes,
statuses 69/70, four owner completions/four AP requests (connect, data, ACK,
FIN), one owner RX frame/AP deferral, I/O masks `0x2`, and frame balance. This
Pi/TCG gate is functional evidence only; it does not replace Firefox or other
performance qualification on macOS/HVF. TCPv6 still needs a guest runtime.

The same image now also runs an AP1 native-graphics phase. A kernel-only
pre-login binding grants its immutable probe only `CAP_GRAPHICS`; the probe
uses the ordinary surface create/fill/present syscalls. Off-owner composition
publishes retained scene state into one coalesced deferred action. CPU0's
production 100 Hz timer consumes that action and must complete real
`TRANSFER_TO_HOST_2D` and `RESOURCE_FLUSH` commands. The runtime requires
nonzero AP deferrals, CPU0 deferred compositions, control-queue submissions,
transfer completions and flush completions, status 67, surface reap, and exact
frame balance. Every low-level virtio-GPU MMIO submission fails closed off
CPU0; this qualifies graphics service ownership, not accelerated rendering or
general desktop AP scheduling.

The focused image also runs an AP1 block-service phase before networking. The
kernel creates a private uid/gid-1000 fixture inode; an immutable minimally
authorized probe uses normal VFS/MakFS4 calls to write and `fsync` 4 KiB, close,
reopen, read and verify 4 KiB, and finally remove the inode. Every AP block
operation crosses an eight-slot copied-request queue. CPU0's 100 Hz timer bottom
half exclusively submits the real virtio-blk requests and defers a tick instead
of recursively taking an owner lock interrupted during direct CPU0 I/O. Current
Latest Pi/TCG evidence is 22 requests/completions: 13 reads, 6 writes, 3
flushes, and 22 timer-service completions, with status 65 and balanced frames. Low-level ring
submission fails closed off CPU0. The AP request wait is bounded EL1 `WFE`, not
a scheduler idle/wake result. Post-desktop AP scope remains limited to
non-leader Firefox and ordinary Native application workers.

The UEFI loader allocates the direct kernel handoff span as `LOADER_CODE`.
Current AAVMF releases may enforce execute-never on `LOADER_DATA`; using that
data memory type for an ELF entry would fault immediately after
`ExitBootServices`. `scripts/test_uefi_kernel_handoff.py` guards this contract.

## MakFS4 offline check

Stop every QEMU process using the raw MakFS4 data image before checking it.
Checking a live writable image can observe an inconsistent commit boundary.
The tool is read-only. It accepts raw 1 GiB data volumes or whole-disk images
whose primary/backup GPT identifies a MakOS data partition:

```sh
cargo run --release -p makos-makfs4-fsck -- path/to/data.img
```

Success prints `MAKOS_MAKFS4_FSCK_OK` with active generation/root slot and
inode/block counts. Failure exits nonzero without modifying the image. Repair
mode is not implemented.

## Debugging

Kernel has symbols. Start QEMU manually with `-s -S`, then connect a cross-GDB
to port 1234 and load `target/x86_64-unknown-none/release/makos-kernel`.
Early kernel diagnostics use COM1 (`-serial stdio`). Success marker is
`MAKOS_BOOT_OK`; fatal boot-ABI errors use `MAKOS_FATAL`; Rust panics use
`MAKOS_PANIC`.
