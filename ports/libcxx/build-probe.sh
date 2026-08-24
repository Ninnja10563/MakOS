#!/bin/sh
# SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
sysroot=${LIBCXX_STAGE_DIR:-"$repo_dir/build/ports/libcxx/sysroot"}
out_dir=${LIBCXX_PROBE_DIR:-"$repo_dir/build/ports/libcxx/probe"}
cxx=${MAKOS_CXX:-"$repo_dir/ports/firefox/toolchain/makos-clang++"}
builtins="$sysroot/usr/lib/makos/libclang_rt.builtins-aarch64.a"

for path in \
    "$sysroot/usr/lib/crt1.o" \
    "$sysroot/usr/lib/crti.o" \
    "$sysroot/usr/lib/crtn.o" \
    "$sysroot/usr/lib/libc.a" \
    "$sysroot/usr/lib/libc++.a" \
    "$sysroot/usr/lib/libc++abi.a" \
    "$sysroot/usr/lib/libunwind.a" \
    "$builtins"; do
    test -f "$path" || { echo "C++ probe prerequisite missing: $path" >&2; exit 1; }
done

mkdir -p "$out_dir"
"$cxx" --sysroot="$sysroot" \
    -D_GNU_SOURCE -std=c++17 -O2 -fexceptions -frtti -pthread \
    -Wl,--build-id=none -Wl,-z,max-page-size=4096 \
    -Wl,--image-base=0x10000000 -Wl,--eh-frame-hdr \
    "$port_dir/runtime-probe.cpp" -o "$out_dir/runtime-probe.elf"

echo "$out_dir/runtime-probe.elf"
