#!/bin/sh
# SPDX-License-Identifier: Apache-2.0 OR MIT
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
stage_dir="$repo_dir/build/ports/rust/toolchain-makos"
source_dir=$(rustc +nightly --print sysroot)
std_dir="$stage_dir/lib/rustlib/src/rust/library/std/src"
std_build="$stage_dir/lib/rustlib/src/rust/library/std/build.rs"

test -d "$source_dir/lib/rustlib/src/rust/library" || {
    echo "Rust nightly rust-src missing; run: rustup component add rust-src --toolchain nightly" >&2
    exit 1
}

if test ! -d "$stage_dir"; then
    mkdir -p "$(dirname "$stage_dir")"
    cp -cR "$source_dir" "$stage_dir" 2>/dev/null || cp -R "$source_dir" "$stage_dir"
fi

# rust-objcopy looks for libLLVM one directory beside its host rustlib tools.
# Keep lookup local: a global DYLD_LIBRARY_PATH/LD_LIBRARY_PATH poisons
# unrelated LLVM tools. Official macOS nightlies use libLLVM.dylib; Linux
# nightlies encode their versioned libLLVM.so basename in DT_NEEDED.
host_triple=$("$stage_dir/bin/rustc" -vV | sed -n 's/^host: //p')
host_rustlib="$stage_dir/lib/rustlib/$host_triple"
test -n "$host_triple"
mkdir -p "$host_rustlib/lib"
if test -f "$stage_dir/lib/libLLVM.dylib"; then
    llvm_runtime=libLLVM.dylib
else
    llvm_runtime=$(
        find "$stage_dir/lib" -maxdepth 1 -type f \
            -name 'libLLVM.so.*-rust-*' -print | sed -n '1p'
    )
    test -n "$llvm_runtime" || {
        echo "MakOS Rust toolchain stage lacks its host libLLVM runtime" >&2
        exit 1
    }
    llvm_runtime=$(basename "$llvm_runtime")
fi
ln -sfn "../../../$llvm_runtime" "$host_rustlib/lib/$llvm_runtime"

test -f "$std_dir/os/mod.rs" || {
    echo "MakOS Rust toolchain stage is incomplete: $stage_dir" >&2
    exit 1
}

if ! grep -q 'any(target_os = "linux", target_os = "makos")' "$std_dir/os/mod.rs"; then
    find "$std_dir" -type f -name '*.rs' -print0 | xargs -0 perl -0pi \
        -e 's/target_os = "linux"/any(target_os = "linux", target_os = "makos")/g'
fi

# MakOS std backend is explicitly ported above. Mark it supported so downstream
# no_std crates do not receive std-only `restricted_std` feature injection.
if ! grep -q 'target_os == "makos"' "$std_build"; then
    perl -0pi -e 's/if target_os == "linux"/if target_os == "makos"\n        || target_os == "linux"/' "$std_build"
fi

test "$("$stage_dir/bin/rustc" --print sysroot)" = "$stage_dir"
echo "MAKOS_RUST_TOOLCHAIN_OK target_os=makos unix_backend=linux-musl distinct_target=1 host=$host_triple llvm_runtime=$llvm_runtime"
