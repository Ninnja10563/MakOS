#!/bin/sh
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. https://mozilla.org/MPL/2.0/
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
source_dir="$repo_dir/build/ports/firefox/source"
obj="$repo_dir/build/ports/firefox/obj-aarch64-makos"
sysroot=${MAKOS_SYSROOT:-"$repo_dir/build/ports/firefox/sysroot-runtime"}
build_python=${FIREFOX_BUILD_PYTHON:-/opt/homebrew/opt/python@3.12/bin/python3.12}
rust_toolchain="$repo_dir/build/ports/rust/toolchain-makos"
bindgen_libdir=${MAKOS_BINDGEN_LIBDIR:-/opt/homebrew/opt/llvm@18/lib}
profiler_bindgen_libdir=${MAKOS_PROFILER_BINDGEN_LIBDIR:-/opt/homebrew/opt/llvm/lib}

test -x "$build_python" || {
    echo "Firefox MakOS build blocked: host Python 3.11/3.12 required by ESR mach." >&2
    exit 1
}
test -f "$bindgen_libdir/libclang.dylib" || {
    echo "Firefox MakOS build blocked: bindgen 0.69 requires compatible libclang 18." >&2
    echo "Set MAKOS_BINDGEN_LIBDIR for another host." >&2
    exit 1
}
export MAKOS_BINDGEN_LIBDIR="$bindgen_libdir"
test -f "$profiler_bindgen_libdir/libclang.dylib" || {
    echo "Firefox MakOS build blocked: profiler/libc++ 22 needs matching libclang." >&2
    echo "Set MAKOS_PROFILER_BINDGEN_LIBDIR for another host." >&2
    exit 1
}
export MAKOS_PROFILER_BINDGEN_LIBDIR="$profiler_bindgen_libdir"
export MAKOS_MODERN_BINDGEN_LIBDIR=${MAKOS_MODERN_BINDGEN_LIBDIR:-"$profiler_bindgen_libdir"}

if test -z "${MAKOS_CC-}" && test -x "$port_dir/toolchain/makos-clang"; then
    MAKOS_CC="$port_dir/toolchain/makos-clang"
fi
if test -z "${MAKOS_CXX-}" && test -x "$port_dir/toolchain/makos-clang++"; then
    MAKOS_CXX="$port_dir/toolchain/makos-clang++"
fi
export MAKOS_CC MAKOS_CXX
if test -z "${MAKOS_SYSROOT-}"; then
    FIREFOX_RUNTIME_SYSROOT="$sysroot" "$port_dir/prepare-runtime-sysroot.sh"
fi
export MAKOS_DYNAMIC_RUNTIME=1
# Mozilla's rust.mk sees custom target JSON path, so it cannot create cc-rs's
# target-specific env names. Export real Cargo target names before mach/make.
target_cflags="--target=aarch64-unknown-makos --sysroot=$sysroot -D__makos__ -D__unix__=1 -D_GNU_SOURCE"
export CC_aarch64_unknown_makos="$MAKOS_CC"
export CXX_aarch64_unknown_makos="$MAKOS_CXX"
export AR_aarch64_unknown_makos=${MAKOS_AR:-/opt/homebrew/opt/llvm/bin/llvm-ar}
export CFLAGS_aarch64_unknown_makos="$target_cflags"
export CXXFLAGS_aarch64_unknown_makos="$target_cflags -D_LIBCPP_DISABLE_VISIBILITY_ANNOTATIONS"

test -x "$rust_toolchain/bin/rustc" || {
    echo "Firefox MakOS build blocked: run ports/rust/build-std.sh first." >&2
    exit 1
}
export MAKOS_REAL_RUSTC=${MAKOS_RUSTC:-"$rust_toolchain/bin/rustc"}
export RUSTC="$repo_dir/ports/rust/makos-rustc.py"
export CARGO=${MAKOS_CARGO:-"$rust_toolchain/bin/cargo"}
export MAKOS_RUST_TARGET=${MAKOS_RUST_TARGET:-"$repo_dir/ports/rust/aarch64-unknown-makos.json"}
export RUSTC_BOOTSTRAP=1
export CARGOFLAGS=${MAKOS_CARGOFLAGS:--Zjson-target-spec}

"$port_dir/toolchain-audit.sh" --require || {
    echo "Firefox MakOS build blocked: upstream LLVM clang plus ELF ld.lld required." >&2
    echo "Apple clang can emit target objects but cannot drive MakOS ELF links." >&2
    exit 1
}

missing=
for path in usr/include/stdio.h usr/include/pthread.h usr/include/sys/mman.h \
    usr/lib/Scrt1.o usr/lib/libc.so usr/lib/libpthread.a usr/lib/libc++.a \
    usr/lib/libc++abi.a
do
    test -e "$sysroot/$path" || missing="$missing $path"
done
if test -n "$missing"; then
    echo "Firefox MakOS build blocked: target sysroot incomplete." >&2
    echo "missing:$missing" >&2
    echo "Run ports/firefox/audit.sh for full runtime gates." >&2
    exit 1
fi

test -x "$source_dir/mach" || {
    echo "Firefox source missing; run ports/firefox/clone.sh" >&2
    exit 1
}
if test "${MAKOS_PATCHES_APPLIED:-0}" != 1; then
    "$port_dir/apply-patches.sh"
fi
"$port_dir/prepare-rust-libc.sh"
"$port_dir/prepare-rust-getrandom.sh"
"$port_dir/prepare-rust-rustix.sh"
"$port_dir/prepare-rust-mtu.sh"
"$port_dir/prepare-rust-nss-gk-api.sh"
"$port_dir/prepare-rust-socket2.sh"
"$port_dir/prepare-rust-libloading.sh"
test -e "$source_dir/widget/makos/MakOSSurface.cpp" || {
    echo "Firefox MakOS build blocked: Gecko widget/makos ABI slice missing." >&2
    exit 1
}

export MAKOS_SYSROOT="$sysroot"
export MOZCONFIG="$port_dir/mozconfig.makos"
# Stable default keeps incremental Gecko/Rust artifacts valid across retries.
# Override for release packaging when a different reproducible build ID is
# required.
export MOZ_BUILD_DATE=${MOZ_BUILD_DATE:-20260818193048}
"$build_python" "$source_dir/mach" build "$@"
FIREFOX_BIN_DIR="$obj/dist/bin" "$port_dir/audit-binary.sh"
python3 "$repo_dir/scripts/firefox_provenance.py" create-build-stamp \
    --source-dir "$source_dir" \
    --bin-dir "$obj/dist/bin" \
    --output "$obj/makos-build-provenance.json"
