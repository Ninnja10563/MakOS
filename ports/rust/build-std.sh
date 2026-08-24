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
readelf=${MAKOS_READELF:-/opt/homebrew/opt/llvm/bin/llvm-readelf}

"$port_dir/prepare-libc.sh"
"$port_dir/prepare-toolchain.sh"
test -f "$sysroot/usr/lib/libc.a" || {
    echo "MakOS C/C++ sysroot missing: run ports/libcxx/build-makos.sh" >&2
    exit 1
}

export MAKOS_SYSROOT="$sysroot"
export CARGO_TARGET_DIR="$target_dir"
export RUSTC="$toolchain/bin/rustc"
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
test "$entry" != 0x0
test -z "$($readelf -Ws "$probe" | awk '$7 == "UND" && $5 == "GLOBAL" { print $8 }')"
echo "MAKOS_RUST_STD_OK target=aarch64-unknown-makos std=1 threads=linked elf=aarch64-static entry=$entry rustlib=staged runtime_executed=0"
