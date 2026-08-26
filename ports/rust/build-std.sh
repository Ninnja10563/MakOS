#!/bin/sh
# SPDX-License-Identifier: Apache-2.0 OR MIT
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
toolchain="$repo_dir/build/ports/rust/toolchain-makos"
sysroot=${MAKOS_SYSROOT:-"$repo_dir/build/ports/libcxx/sysroot"}
target_dir="$repo_dir/build/ports/rust/target-std"
probe="$target_dir/aarch64-unknown-makos/debug/makos-rust-std-probe"
rustlib_dir="$toolchain/lib/rustlib/aarch64-unknown-makos/lib"
readelf=${MAKOS_READELF:-}
if test -z "$readelf"; then
    for candidate in llvm-readelf llvm-readelf-19 readelf \
        "$repo_dir/build/host-tools/llvm19/usr/bin/llvm-readelf-19" \
        /opt/homebrew/opt/llvm/bin/llvm-readelf
    do
        if command -v "$candidate" >/dev/null 2>&1; then
            readelf=$(command -v "$candidate")
            break
        fi
    done
fi
test -n "$readelf" && test -x "$readelf" || {
    echo "ELF inspection tool missing (set MAKOS_READELF)" >&2
    exit 1
}

"$port_dir/prepare-toolchain.sh"
"$toolchain/bin/cargo" fetch \
    --manifest-path "$port_dir/libc-fetch/Cargo.toml" --locked
"$port_dir/prepare-libc.sh"
test -f "$sysroot/usr/lib/libc.a" || {
    echo "MakOS C/C++ sysroot missing: run ports/libcxx/build-makos.sh" >&2
    exit 1
}

export MAKOS_SYSROOT="$sysroot"
export CARGO_TARGET_DIR="$target_dir"
export RUSTC="$toolchain/bin/rustc"
export PATH="$repo_dir/ports/firefox/toolchain:$PATH"
if test -z "${MAKOS_REAL_CLANG:-}"; then
    for candidate in clang clang-19 "$repo_dir/build/host-tools/llvm19/usr/bin/clang-19"; do
        if command -v "$candidate" >/dev/null 2>&1; then
            MAKOS_REAL_CLANG=$(command -v "$candidate")
            export MAKOS_REAL_CLANG
            break
        fi
    done
fi
if test -z "${MAKOS_LLD:-}"; then
    for candidate in ld.lld ld.lld-19 "$repo_dir/build/host-tools/llvm19/usr/bin/ld.lld-19"; do
        if command -v "$candidate" >/dev/null 2>&1; then
            MAKOS_LLD=$(command -v "$candidate")
            export MAKOS_LLD
            break
        fi
    done
fi
# Building std's default debug information can exceed the Raspberry Pi host's
# physical RAM plus zram even with one Cargo job. The runtime probe validates
# symbols/ELF execution contracts, not debugger metadata; keep the staged
# libraries non-incremental and without debug sections on every host.
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export RUSTFLAGS="-C embed-bitcode=yes -C link-arg=--sysroot=$sysroot -C link-arg=-L$sysroot/usr/lib"

# build-std hashes change when panic-runtime profiles change.  Never stage stale
# duplicate libstd metadata from an earlier dedicated target build.
"$toolchain/bin/cargo" clean \
    --manifest-path "$port_dir/probe/Cargo.toml" \
    --target-dir "$target_dir"

"$toolchain/bin/cargo" \
    --config "patch.crates-io.libc.path='$repo_dir/build/ports/rust/libc-makos'" \
    build --manifest-path "$port_dir/probe/Cargo.toml" \
    -Z build-std=std,panic_unwind -Zjson-target-spec \
    --target "$port_dir/aarch64-unknown-makos.json"

test -x "$probe"
mkdir -p "$rustlib_dir"
find "$rustlib_dir" -maxdepth 1 -type f \
    \( -name '*.rlib' -o -name '*.rmeta' \) -delete
find "$target_dir/aarch64-unknown-makos/debug/build" -type f \
    \( -name '*.rlib' -o -name '*.rmeta' \) -exec cp -f '{}' "$rustlib_dir/" \;
entry=$($readelf -h "$probe" | awk '/Entry point address:/ { print $4 }')
test -n "$entry"
test "$entry" != 0x0
test -z "$($readelf -Ws "$probe" | awk '$7 == "UND" && $5 == "GLOBAL" { print $8 }')"
echo "MAKOS_RUST_STD_OK target=aarch64-unknown-makos std=1 threads=linked elf=aarch64-static entry=$entry rustlib=staged runtime_executed=0"
