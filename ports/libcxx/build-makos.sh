#!/bin/sh
# SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
. "$port_dir/source.lock"

source_dir=${LLVM_SOURCE_DIR:-"$repo_dir/build/ports/libcxx/source"}
musl_sysroot=${MUSL_STAGE_DIR:-"$repo_dir/build/ports/musl/sysroot-static"}
runtime_build=${LIBCXX_BUILD_DIR:-"$repo_dir/build/ports/libcxx/makos-static"}
builtins_build=${BUILTINS_BUILD_DIR:-"$repo_dir/build/ports/libcxx/compiler-rt-builtins"}
stage_dir=${LIBCXX_STAGE_DIR:-"$repo_dir/build/ports/libcxx/sysroot"}
cc=${MAKOS_CC:-"$repo_dir/ports/firefox/toolchain/makos-clang"}
cxx=${MAKOS_CXX:-"$repo_dir/ports/firefox/toolchain/makos-clang++"}
ar_tool=${MAKOS_AR:-/opt/homebrew/opt/llvm/bin/llvm-ar}
ranlib_tool=${MAKOS_RANLIB:-/opt/homebrew/opt/llvm/bin/llvm-ranlib}
jobs=${LIBCXX_JOBS:-$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 4)}
toolchain="$port_dir/cmake/makos-toolchain.cmake"

test -d "$source_dir/.git" || "$port_dir/clone.sh" >/dev/null
test "$(git -C "$source_dir" rev-parse HEAD)" = "$LLVM_COMMIT" || {
    echo "LLVM source is not pinned commit $LLVM_COMMIT" >&2
    exit 1
}
for patch in "$port_dir"/patches/*.patch; do
    test -e "$patch" || continue
    if git -C "$source_dir" apply --reverse --check "$patch" >/dev/null 2>&1; then
        continue
    fi
    git -C "$source_dir" apply --check "$patch"
    git -C "$source_dir" apply "$patch"
done
test -f "$musl_sysroot/usr/lib/crt1.o" || {
    echo "upstream musl crt1 missing; run ports/musl/build-makos.sh" >&2
    exit 1
}
for executable in cmake ninja rsync "$cc" "$cxx" "$ar_tool" "$ranlib_tool"; do
    command -v "$executable" >/dev/null 2>&1 || {
        echo "required tool missing: $executable" >&2
        exit 1
    }
done

mkdir -p "$runtime_build" "$builtins_build" "$stage_dir"

MAKOS_CC="$cc" MAKOS_CXX="$cxx" MAKOS_AR="$ar_tool" \
MAKOS_RANLIB="$ranlib_tool" MAKOS_SYSROOT="$musl_sysroot" \
cmake -G Ninja -S "$source_dir/runtimes" -B "$runtime_build" \
    -DCMAKE_TOOLCHAIN_FILE="$toolchain" \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_POSITION_INDEPENDENT_CODE=ON \
    -DCMAKE_INSTALL_PREFIX=/usr \
    -DCMAKE_INSTALL_LIBDIR=lib \
    '-DLLVM_ENABLE_RUNTIMES=libcxx;libcxxabi;libunwind' \
    -DLLVM_INCLUDE_TESTS=OFF \
    -DLLVM_INCLUDE_BENCHMARKS=OFF \
    -DLIBCXX_INCLUDE_TESTS=OFF \
    -DLIBCXXABI_INCLUDE_TESTS=OFF \
    -DLIBUNWIND_INCLUDE_TESTS=OFF \
    -DLIBCXX_ENABLE_SHARED=OFF \
    -DLIBCXX_ENABLE_STATIC=ON \
    -DLIBCXXABI_ENABLE_SHARED=OFF \
    -DLIBCXXABI_ENABLE_STATIC=ON \
    -DLIBUNWIND_ENABLE_SHARED=OFF \
    -DLIBUNWIND_ENABLE_STATIC=ON \
    -DLIBCXX_HAS_MUSL_LIBC=ON \
    -DLIBCXX_HAS_PTHREAD_API=ON \
    -DLIBCXXABI_USE_LLVM_UNWINDER=ON \
    -DLIBCXXABI_HAS_CXA_THREAD_ATEXIT_IMPL=OFF \
    -DLIBCXX_ENABLE_ABI_LINKER_SCRIPT=OFF \
    -DLIBCXX_ENABLE_TIME_ZONE_DATABASE=OFF \
    -DLLVM_DEFAULT_TARGET_TRIPLE=aarch64-unknown-makos \
    '-DCMAKE_C_FLAGS=-D__makos__ -D_GNU_SOURCE' \
    '-DCMAKE_CXX_FLAGS=-D__makos__ -D_GNU_SOURCE -D_LIBUNWIND_USE_DLADDR=0' \
    '-DCMAKE_ASM_FLAGS=-D__makos__' \
    -DLIBUNWIND_HAS_DL_LIB=NO
ninja -C "$runtime_build" -j"$jobs" cxx cxxabi unwind

MAKOS_CC="$cc" MAKOS_CXX="$cxx" MAKOS_AR="$ar_tool" \
MAKOS_RANLIB="$ranlib_tool" MAKOS_SYSROOT="$musl_sysroot" \
cmake -G Ninja -S "$source_dir/compiler-rt/lib/builtins" \
    -B "$builtins_build" \
    -DCMAKE_TOOLCHAIN_FILE="$toolchain" \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_INSTALL_PREFIX=/usr \
    -DCOMPILER_RT_DEFAULT_TARGET_ONLY=ON \
    -DCOMPILER_RT_BUILTINS_ENABLE_PIC=ON \
    -DCOMPILER_RT_BUILTINS_HIDE_SYMBOLS=OFF \
    -DCOMPILER_RT_BAREMETAL_BUILD=OFF \
    '-DCMAKE_C_FLAGS=-D__makos__ -D_GNU_SOURCE' \
    '-DCMAKE_CXX_FLAGS=-D__makos__ -D_GNU_SOURCE' \
    '-DCMAKE_ASM_FLAGS=-D__makos__'
ninja -C "$builtins_build" -j"$jobs"

rsync -a "$musl_sysroot/" "$stage_dir/"
DESTDIR="$stage_dir" cmake --install "$runtime_build" --strip \
    >"$runtime_build/install.log"
DESTDIR="$stage_dir" cmake --install "$builtins_build" --strip \
    >"$builtins_build/install.log"

LIBCXX_STAGE_DIR="$stage_dir" "$port_dir/build-probe.sh"
LIBCXX_STAGE_DIR="$stage_dir" "$port_dir/audit.sh"
echo "stage=$stage_dir"
echo "runtime_probe=$repo_dir/build/ports/libcxx/probe/runtime-probe.elf"
