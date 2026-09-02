# Official Firefox source port foundation

This directory pins Mozilla Firefox ESR 140.13.0 source. ESR is chosen for a
stable security-maintained base while MakOS develops a new platform port.
Mozilla's official product metadata listed 140.13.0esr as current ESR when
pinned on 2026-08-17.

## Provenance

- Release archive: <https://archive.mozilla.org/pub/firefox/releases/140.13.0esr/source/>
- Official repository: <https://github.com/mozilla-firefox/firefox>
- Tag: `FIREFOX_140_13_0esr_RELEASE`
- Commit: `90ad18aabeaa9cbd63a1f749a57f266e758e50da`
- Archive SHA-512: recorded in `source.lock`, matching Mozilla `SHA512SUMS`.
- Mozilla build docs: <https://firefox-source-docs.mozilla.org/setup/linux_build.html>
- Supported targets: <https://firefox-source-docs.mozilla.org/contributing/build/supported.html>

Source stays outside the repository under `build/ports/firefox/`. Choose one:

```sh
ports/firefox/fetch.sh       # 615 MiB release archive
ports/firefox/clone.sh       # shallow checkout of exact release tag
```

Cheap provenance checks download no source tree:

```sh
ports/firefox/fetch.sh --check
ports/firefox/clone.sh --check
ports/firefox/test.sh
```

`build-makos.sh` never builds a host Firefox and labels it MakOS, runs a remote
browser, or installs a fake browser UI. It now cross-builds official Gecko when
the isolated MakOS sysroot is present, then audits the resulting ELF files.
See `ABI.md` and `required-abi.txt` for target runtime gates.

The latest release build reached the final `libxul.so` link before exposing a
Rust `errno` 0.3.8 target-selection defect: its unknown-Unix fallback requested
`errno_location`, but MakOS's upstream-musl libc correctly exports the
thread-local `__errno_location`. The staged checksum-safe errno crate now
selects the real musl accessor for MakOS. Focused source/Cargo staging and exact
AArch64 object/runtime-libc symbol checks pass; a fresh complete link, package,
and guest runtime are still pending. Patch `0059` carries the Cargo routing so
the independent print-settings patch remains `0058` in the combined series.

Ordered patches recognize MakOS without Linux masquerading; add
`cairo-makos`/`MOZ_WIDGET_MAKOS`; provide a retained-surface `nsIWidget`,
software Cairo/Skia/FreeType path, event bridge, POSIX platform services,
MakOS locale/MIME/process slices, AArch64 XPTCall, Rust target plumbing, and a
distinct AArch64/LP64 MakOS NSPR configuration. NSPR selects generic
Unix/pthreads code, never Linux platform identity. `test-widget.sh` and
`test-nspr.sh` compile focused slices independently. Source patches now bridge
retained-window pixels, pointer/button/wheel/resize/close events, ASCII/navigation keys,
accelerator shortcuts, and per-user MakOS plain-text clipboard data. These
latest widget changes await constrained compile/guest verification. IME,
accessibility, GPU acceleration, audio, and production multiprocess compositor
integration remain target runtime work.

Patch `0058` supplies the platform print-settings factory required by Gecko's
generic printer code when `NS_PRINTING` is enabled. MakOS uses Gecko's complete
platform-neutral settings with PDF output and a real default PDF filename; it
does not invent a native printer, driver, or printer-service ABI. The focused
AArch64/MakOS object compile defines the exact hidden factory symbol required
by `Unified_cpp_widget3.o`. A fresh full `libxul.so` link, package, and guest
print-preview/PDF runtime proof remain pending.

`test-print-settings.py` reports structural-only coverage explicitly by
default. To reproduce the object/symbol evidence against an existing generated
Firefox configuration without writing into that source or object tree, run:

```sh
MAKOS_FIREFOX_PRINT_COMPILE_EVIDENCE=1 \
MAKOS_FIREFOX_SOURCE_DIR=/path/to/firefox/source \
MAKOS_FIREFOX_OBJ_DIR=/path/to/obj-aarch64-makos-developer \
python3 ports/firefox/test-print-settings.py
```

Requested compile-evidence mode fails closed if the generated headers, MakOS
configuration, compiler, AArch64 symbol tools, or exact `GLOBAL HIDDEN`
factory definition are absent.

`patches/0001-makos-target-recognition.patch` adds a distinct `MakOS` OS/kernel
identity to Gecko's triplet parser and GNU `config.sub`. It does not alias
MakOS to Linux. `apply-patches.sh` applies port patches idempotently after
verifying they match the pinned source; `test.sh` checks every patch against a
clean official checkout before any build begins.

`mozconfig.makos` uses Mozilla's actual toolkit choice name, `cairo-makos`, and
does not export Linux-only `NM` configuration. On macOS, even upstream clang
delegates unknown-OS links to host GCC. `toolchain/makos-clang.py` is a strict
bootstrap driver: clang performs MakOS preprocessing/codegen; ELF `ld.lld`
links target objects directly. It rejects non-MakOS targets and rejects default
links until real MakOS crt/libc archives exist. No Linux triple or host library
fallback exists. Override underlying tools with `MAKOS_REAL_CLANG`,
`MAKOS_REAL_CLANGXX`, and `MAKOS_LLD`; override drivers with `MAKOS_CC` and
`MAKOS_CXX`. `toolchain-audit.sh` compiles and links a freestanding target ELF;
`build-makos.sh` refuses toolchains that cannot pass this executable check.
The compiler resource directory must also contain the matching AArch64
intrinsic headers. `test-toolchain.sh` compiles `arm_neon.h` operations, and the
full build rejects a partial LLVM installation before expensive Gecko work.

`MAKOS_FIREFOX_DEVELOPER_BUILD=1` is an explicit, unoptimized full-source
qualification mode for memory-constrained development hosts. It still runs the
final ELF binary audit, but uses the isolated `obj-aarch64-makos-developer`
directory, invalidates both object-root and staged runtime provenance there
before the build, and never writes a new release stamp. The supported/default
packaging flow therefore rejects
developer outputs, and they cannot qualify runtime performance. The
unset/default mode remains the release build contract.

Official musl and LLVM ports supply isolated C/C++ build and shared-runtime
sysroots. Build and audit them without promoting them into the SDK:

```sh
ports/musl/build-makos.sh
ports/libcxx/build-makos.sh
MAKOS_SYSROOT="$PWD/build/ports/libcxx/sysroot" ports/firefox/audit.sh
```

The build sysroot contains upstream musl and LLVM libc++, libc++abi, libunwind,
and compiler-rt. `prepare-runtime-sysroot.sh` adds musl `Scrt1.o`, shared
`libc.so`, and `/lib/ld-musl-aarch64.so.1`. With
`MAKOS_DYNAMIC_RUNTIME=1`, the strict clang driver emits shared-musl PIEs and
DSOs while retaining static C++ runtimes; no host libraries enter target ELF.
`test-dynamic-runtime.sh` validates the exact interpreter and dependencies.

## Current cross-build result

Official ESR source completes a full `mach build` for
`aarch64-unknown-makos`. `audit-binary.sh` verifies ELF64 AArch64 outputs and
requires `firefox`, `plugin-container`, and `xpcshell` to use
`/lib/ld-musl-aarch64.so.1` plus shared `libc.so`:

- `firefox`: native PIE entry executable.
- `plugin-container` and `xpcshell`: native PIE executables linked to Gecko.
- `libxul.so`: genuine Gecko shared library linked with NSS/SSL, SpiderMonkey,
  DOM, layout, WebRender, Necko, HTTP/2, HTTP/3/QUIC, WebSocket, and WebGPU code.

This is a compile/link milestone, not a runnable browser claim. MakOS now
executes PIEs through upstream musl `PT_INTERP`, resolves libc, and loads a
separate `DT_NEEDED` DSO from MakOS VFS with RELA/PLT/GOT and RELRO. It still
executes basic runtime `dlopen`/`dlsym`/call/`dlclose`. It still lacks full
recursive/TLS/versioned relocation coverage, general process launch, scalable
file-backed/shared VM, and remaining pthread,
signal, socket, profile-storage, compositor, audio, and sandbox contracts.
Mozilla's `stage-package` emits a real 28-file runtime tree. `package-makos.sh`
replaces the 2.1 GiB debug `libxul.so` with the audited 191 MiB stripped ELF,
then writes and fully CRC-verifies a 344 MiB sector-backed package image.
The full build writes a canonical provenance stamp only after the binary audit.
Packaging rechecks its pinned source HEAD, exact applied-patch-series marker,
and exact patched tracked tree. The tree is reconstructed from the pinned
commit plus ordered local patches in a temporary Git index; tracked byte/mode
or symlink-target differences and unexpected non-ignored source paths fail
closed. Actual bytes are hashed directly without clean filters, line-ending
normalization, or trust in `core.fileMode`, and the check does not change the
real source index or object store. The stamp binds that source
tree identity plus SHA-256 of `firefox`, `plugin-container`, `xpcshell`,
`libxul.so`, and
`libnspr4.so`; after stripping it emits a runtime record with hashes of those
exact five packaged payloads. Integrated-image and strict-runtime preflights
reject a missing/stale record or any package hash mismatch before QEMU.
MakOS VFS now mounts its checksummed package manifest and streams arbitrary
offset reads directly from virtio-blk/ATA, avoiding fixed in-kernel file
buffers. Guest Firefox now launches genuine packaged binaries through
package-backed demand paging and reaches Gecko/XPCOM plus SpiderMonkey JIT
initialization. Latest source changes add sparse JIT reservation, retained-window
output, native event pumping, keyboard, system clipboard bridges, musl
scatter/gather DNS sockets, DHCP DNS-config fallback, and frame-releasing
`madvise(DONTNEED/FREE)`, plus thread-consistent parent identity. These
changes, true non-truncating `O_RDWR` profile-file descriptors, and POSIX
byte-range profile locks still need constrained rebuild/runtime proof.
MakFS4 now has source-integrated collision-safe 1 GiB volume format/mount,
extent/COW metadata, and `/home/user` VFS descriptors. Firefox profile files now
route to those extents in source. Persistence remains unproven until atomic
replace-rename, unlink-while-open, performance work, and fault/remount/SQLite
tests land. HTTPS, production compositor, audio, sandbox, and multiprocess
contracts remain.

## License and branding

Mozilla-authored Firefox code is primarily MPL-2.0; vendored third-party files
carry their own licenses. Preserve the upstream source tree's `LICENSE`, file
headers, and third-party notices when distributing source or binaries:
<https://www.mozilla.org/MPL/2.0/>.

Firefox names/logos are trademarks, separate from source licenses. A modified
MakOS build must use independent product branding unless Mozilla grants written
permission. Policy:
<https://www.mozilla.org/en-US/foundation/trademarks/distribution-policy/>.

Current result: reproducible official-source AArch64 MakOS Firefox binaries.
No claim that modern websites or protocols run inside MakOS yet.
