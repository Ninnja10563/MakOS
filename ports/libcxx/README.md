# LLVM C++ runtimes for MakOS

Real static target runtimes from official LLVM 22.1.8 source. This port builds
libc++, libc++abi, libunwind, and compiler-rt builtins for the distinct
`aarch64-unknown-makos` ELF target. It does not use host libraries, a Linux
target triple, a remote runtime, or a browser substitute.

## Provenance

- Repository: <https://github.com/llvm/llvm-project.git>
- Tag: `llvmorg-22.1.8`
- Commit: `ca7933e47d3a3451d81e72ac174dcb5aa28b59d1`
- License: Apache-2.0 WITH LLVM-exception; upstream third-party components keep
  their own notices.

`source.lock` records exact inputs. `clone.sh --check` verifies the peeled
official tag. Source is stored under `build/ports/libcxx/source`.

## Build and evidence

First build the official musl port, then build LLVM runtimes:

```sh
ports/musl/build-makos.sh
ports/libcxx/clone.sh
ports/libcxx/build-makos.sh
ports/libcxx/test.sh
```

The combined static sysroot is `build/ports/libcxx/sysroot`. The test links
`runtime-probe.cpp` with upstream musl `crt1.o`, libc++, exceptions/unwinding,
pthreads, and compiler-rt. Audit requires a static AArch64 ELF, no dynamic
dependencies, and no global/weak undefined symbols.

`_GNU_SOURCE` only exposes APIs already supplied by musl (`syscall`,
`nanosleep`); it does not select Linux code. `_LIBUNWIND_USE_DLADDR=0` selects
upstream libunwind's static ELF program-header path because MakOS has no dynamic
loader. musl resolves those headers through the process auxv.

Current executable evidence is build/link/audit only. The probe has not yet
executed inside MakOS. Runtime proof still requires loading it in MakOS and
observing `MAKOS_LIBCXX_RUNTIME_OK`; do not claim runtime success before that.
