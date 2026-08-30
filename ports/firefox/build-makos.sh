#!/bin/sh
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. https://mozilla.org/MPL/2.0/
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
source_dir="$repo_dir/build/ports/firefox/source"
obj=$("$port_dir/build-mode.sh" "$repo_dir")
sysroot=${MAKOS_SYSROOT:-"$repo_dir/build/ports/firefox/sysroot-runtime"}
build_python=${FIREFOX_BUILD_PYTHON:-}
rust_toolchain="$repo_dir/build/ports/rust/toolchain-makos"
bindgen_libdir=${MAKOS_BINDGEN_LIBDIR:-}
profiler_bindgen_libdir=${MAKOS_PROFILER_BINDGEN_LIBDIR:-}

clear_developer_provenance() {
    for provenance in \
        "$obj/makos-build-provenance.json" \
        "$obj/dist/firefox/makos-build-provenance.json"
    do
        rm -f "$provenance" || {
            echo "Firefox MakOS developer build blocked: cannot remove stale provenance: $provenance" >&2
            return 1
        }
    done
}

developer_cleanup_on_exit() {
    status=$?
    trap - EXIT HUP INT TERM
    if ! clear_developer_provenance; then
        status=1
    fi
    exit "$status"
}

if test "${MAKOS_FIREFOX_DEVELOPER_BUILD:-0}" = 1; then
    # Failures and interruptions must not leave provenance restored after the
    # pre-build invalidation. Signal traps route through the same EXIT cleanup.
    trap developer_cleanup_on_exit EXIT
    trap 'exit 129' HUP
    trap 'exit 130' INT
    trap 'exit 143' TERM
fi

if test -z "$build_python"; then
    for candidate in python3.12 python3.11 python3; do
        if command -v "$candidate" >/dev/null 2>&1; then
            build_python=$(command -v "$candidate")
            break
        fi
    done
fi
if test -z "$bindgen_libdir"; then
    for candidate in \
        "$repo_dir/build/host-tools/llvm19/usr/lib/llvm-19/lib" \
        /opt/homebrew/opt/llvm@18/lib /usr/lib/llvm-19/lib
    do
        if find "$candidate" -maxdepth 1 \( -name 'libclang.dylib' -o \
            -name 'libclang.so' -o -name 'libclang-*.so.*' \) \
            -print -quit 2>/dev/null | grep -q .; then
            bindgen_libdir=$candidate
            break
        fi
    done
fi
if test -z "$profiler_bindgen_libdir"; then
    profiler_bindgen_libdir=$bindgen_libdir
fi

test -x "$build_python" || {
    echo "Firefox MakOS build blocked: host Python 3.11/3.12 required by ESR mach." >&2
    exit 1
}
build_python_version=$(
    "$build_python" -c 'import sys; print(f"{sys.version_info.major}.{sys.version_info.minor}")'
)
case "$build_python_version" in
    3.11|3.12) ;;
    *)
        echo "Firefox MakOS build blocked: Python 3.11/3.12 required (found $build_python_version)." >&2
        exit 1
        ;;
esac
find "$bindgen_libdir" -maxdepth 1 \( -name 'libclang.dylib' -o \
    -name 'libclang.so' -o -name 'libclang-*.so.*' \) \
    -print -quit 2>/dev/null | grep -q . || {
    echo "Firefox MakOS build blocked: bindgen 0.69 requires compatible libclang 18." >&2
    echo "Set MAKOS_BINDGEN_LIBDIR for another host." >&2
    exit 1
}
export MAKOS_BINDGEN_LIBDIR="$bindgen_libdir"
find "$profiler_bindgen_libdir" -maxdepth 1 \( -name 'libclang.dylib' -o \
    -name 'libclang.so' -o -name 'libclang-*.so.*' \) \
    -print -quit 2>/dev/null | grep -q . || {
    echo "Firefox MakOS build blocked: profiler/libc++ 22 needs matching libclang." >&2
    echo "Set MAKOS_PROFILER_BINDGEN_LIBDIR for another host." >&2
    exit 1
}
export MAKOS_PROFILER_BINDGEN_LIBDIR="$profiler_bindgen_libdir"
export MAKOS_MODERN_BINDGEN_LIBDIR=${MAKOS_MODERN_BINDGEN_LIBDIR:-"$profiler_bindgen_libdir"}
if test -z "${MAKOS_BUILD_HOST:-}"; then
    case "$(uname -s):$(uname -m)" in
        Linux:aarch64) MAKOS_BUILD_HOST=aarch64-unknown-linux-gnu ;;
        Darwin:arm64) MAKOS_BUILD_HOST=aarch64-apple-darwin ;;
        *)
            echo "Firefox MakOS build blocked: set MAKOS_BUILD_HOST for this host." >&2
            exit 1
            ;;
    esac
fi
export MAKOS_BUILD_HOST

if test -z "${MAKOS_CC-}" && test -x "$port_dir/toolchain/makos-clang"; then
    MAKOS_CC="$port_dir/toolchain/makos-clang"
fi
if test -z "${MAKOS_CXX-}" && test -x "$port_dir/toolchain/makos-clang++"; then
    MAKOS_CXX="$port_dir/toolchain/makos-clang++"
fi
export MAKOS_CC MAKOS_CXX
export PATH="$port_dir/toolchain:$PATH"
clang_resource_dir=$("$MAKOS_CC" --print-resource-dir)
test -f "$clang_resource_dir/include/arm_neon.h" || {
    echo "Firefox MakOS build blocked: Clang AArch64 intrinsic headers are missing." >&2
    echo "Expected: $clang_resource_dir/include/arm_neon.h" >&2
    echo "Install/stage the matching Clang resource-header package (Debian: libclang-common-19-dev)." >&2
    exit 1
}
intrinsic_probe_dir="$repo_dir/build/ports/firefox/toolchain-preflight"
mkdir -p "$intrinsic_probe_dir"
"$MAKOS_CC" --target=aarch64-unknown-makos -ffreestanding -c \
    "$port_dir/toolchain-neon-probe.c" -o "$intrinsic_probe_dir/neon-probe.o" || {
    echo "Firefox MakOS build blocked: Clang AArch64 intrinsic headers are unusable." >&2
    echo "Compiler resource directory: $clang_resource_dir" >&2
    exit 1
}
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
if test "${MAKOS_FIREFOX_DEVELOPER_BUILD:-0}" = 1; then
    # Invalidate again immediately before mach in case a prerequisite command
    # restored this developer-only object directory from a build cache.
    clear_developer_provenance
fi
# Stable default keeps incremental Gecko/Rust artifacts valid across retries.
# Override for release packaging when a different reproducible build ID is
# required.
export MOZ_BUILD_DATE=${MOZ_BUILD_DATE:-20260818193048}
"$build_python" "$source_dir/mach" build "$@"
if test "${MAKOS_FIREFOX_DEVELOPER_BUILD:-0}" = 1; then
    # A partial or complete successful mach invocation must not leave a staged
    # record restored by an object-cache or packaging prerequisite.
    clear_developer_provenance
fi
full_build=true
for argument in "$@"; do
    case "$argument" in
        -*) ;;
        *) full_build=false ;;
    esac
done
if test "$full_build" = false; then
    echo "MAKOS_FIREFOX_PARTIAL_BUILD_OK targets=$* developer=${MAKOS_FIREFOX_DEVELOPER_BUILD:-0} binary_audit=deferred"
    exit 0
fi
FIREFOX_BIN_DIR="$obj/dist/bin" "$port_dir/audit-binary.sh"
if test "${MAKOS_FIREFOX_DEVELOPER_BUILD:-0}" = 1; then
    clear_developer_provenance
    echo "MAKOS_FIREFOX_DEVELOPER_BUILD_OK binary_audit=passed release_provenance=withheld"
    exit 0
fi
python3 "$repo_dir/scripts/firefox_provenance.py" create-build-stamp \
    --source-dir "$source_dir" \
    --bin-dir "$obj/dist/bin" \
    --output "$obj/makos-build-provenance.json"
echo "MAKOS_FIREFOX_BUILD_OK developer=0"
