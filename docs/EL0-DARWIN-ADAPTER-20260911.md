# EL0 host adapter: Darwin fortified snprintf — 2026-09-11

## Authoritative user-reported Mac result

Qualification of `2413aded422057bf345faf1a800c58d51c956656` stopped at
`make unit check`: exit 2, real 31.31 s, user 5.09 s, sys 3.40 s. The unit
target failed; `check` was not reached. Apple `/usr/bin/cc` (Apple clang
21.0.0) rejected the generated emission fixture with
`error: 'snprintf' macro redefined [-Werror,-Wmacro-redefined]`.

Darwin SDK `secure/_stdio.h` already defines a function-like macro:

```c
#define snprintf(str, len, ...) __snprintf_chk_func (str, len, 0, __VA_ARGS__)
```

The test adapter then defined `snprintf` as `checked_snprintf`. This is a
host-test compilation collision, not a guest or Firefox runtime failure.
The Mac did not substitute a compiler or modify source, tests, fortification,
assertions, thresholds, timeouts or provenance.

Host: Apple M3, 16 GB, macOS 26.6.2 (25G83), QEMU 11.0.3/HVF available but
never launched. HEAD/main/origin/main/remote main matched, worktree clean,
memory pressure normal, no concurrent QEMU/heavy build. Image rebuild, all
runtimes, Firefox preflight and visible login were **not run**. Existing
images, accounts, profiles, provenance and evidence were preserved. The
integrated image still hashes to
`c23395ff4644b183991f2508bdd475ad2120110019f134ebd2b5af0c550a12dc`.
The Mac report is supplied evidence; its absolute paths are not Pi paths.

## Host-only repair and regression

`scripts/test_aarch64_el0_emission.py` no longer defines or undefines the SDK's
`snprintf`. It redirects exactly one call identifier in the extracted emitter
to the existing `checked_snprintf` adapter and asserts that replacement count.
The production emitter file and its control flow are unchanged. The adapter
continues to call the host's real `vsnprintf`; host SDK macros and default
fortification remain intact. No warning suppression or compiler substitution
is added. The original `-std=c11 -Wall -Wextra -Werror -O2` flags remain.

Both the normal host-header variant and an added `_FORTIFY_SOURCE=2` variant
run the same 11 emission cases and generated split-write rejection. The latter
inserts the reported function-like macro after the real headers only if no
actual SDK macro exists, then verifies the macro remains defined after the
fixture. This is a header-collision model, not an implementation or execution
of Darwin's checked libc. The real SDK macro takes precedence when present.
A structural check forbids adapter define/undef operations on `snprintf`.

An additional compile-negative fixture restores the old global macro
redirection and requires compilation to fail with a `snprintf` redefinition
diagnostic under the same `-Werror`. Existing short/error/overflow assertions,
one-write/no-retry checks, exact reported Mac fragment rejection and every
existing runtime parser/assertion/deadline remain unchanged.

## Local Linux evidence and qualification boundary

Raspberry Pi/Debian AArch64 focused checks pass with both default GCC 14.2.0
and repository-staged LLVM clang 19.1.7. Each compiler passes all 11 cases in
both header variants, split-write rejection, exact Mac-fragment rejection
and the macro-collision compile-negative. The unchanged 17-case EL0 parser
suite and six serial atomicity cases/negative controls also pass. Log:
`build/logs/el0-emission-darwin-adapter-20260911-focused.log`.

Full `make unit check` passes on Pi/Debian: exit 0, real 118.491 s,
user 95.249 s, sys 22.324 s, including both architecture cross-checks and
all existing suites. Log:
`build/logs/full-unit-check-el0-darwin-adapter-20260911.log`.

Log SHA-256 identities (retained locally, not committed build artifacts):

- Focused: `7d9aa4a2bd337ad9eaafc21dc42ad615d29f1653702a919b1f892eaa4e123ac7`.
- Full unit/check: `1a0bb7eb4ef29c3176405797de2acbffff891e60bebe424231d68b971605882c`.

The pre/post local boot image SHA-256 remains
`d487a9e8a8effcf6c11661aebd4cdbc95334a4b8ece785c0c30581b083cfc524`;
kernel ELF remains
`598f28d115f2ef370769b8675d5c7db9365acff2df3de27a42f25333c526f4b6`.
No QEMU was launched, and no MakOS build/test process remains. Existing
images, private guest sessions and previous logs were preserved.

No QEMU or guest runtime is needed to exercise this host-only change, and no
new Pi runtime or Mac/HVF result is claimed. The preceding producer/serial
repair's Pi/TCG results remain historical evidence at `2413ade`, documented in
[the atomicity report](EL0-EVIDENCE-ATOMICITY-20260911.md). Production source,
Make commands, runtime harnesses, Firefox patches/provenance and gate limits
are unchanged by this adapter repair. Audit Partial/Missing rows remain.

Rerun the same sequential [Mac qualification protocol](MACOS-HVF-TEST-AGENT-PROMPT.md)
from `make unit check` at the exact pushed repair commit supplied in chat.
Keep `/usr/bin/cc`/Apple clang and fortification; do not set `HOST_CC` to avoid
the SDK collision. Reuse the preserved integrated image only after its exact
hash and unchanged provenance preflight pass. Do not rebuild/restamp Firefox
solely for this test change. Strict Firefox retains its 600-second probe,
first-character <500 ms, Ctrl-A <10,000 ms and every other assertion. Stop on
the first failure, preserve complete evidence, and launch the sole visible
login from private clones only after all qualification passes.
