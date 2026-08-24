#!/bin/sh
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. https://mozilla.org/MPL/2.0/
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
sysroot=${FIREFOX_RUNTIME_SYSROOT:-"$repo_dir/build/ports/firefox/sysroot-runtime"}
output="$repo_dir/build/ports/firefox/dynamic-runtime-probe.elf"
cxx=${MAKOS_CXX:-"$port_dir/toolchain/makos-clang++"}
readelf=${MAKOS_READELF:-/opt/homebrew/opt/llvm/bin/llvm-readelf}

FIREFOX_RUNTIME_SYSROOT="$sysroot" "$port_dir/prepare-runtime-sysroot.sh"
MAKOS_DYNAMIC_RUNTIME=1 "$cxx" --target=aarch64-unknown-makos \
    --sysroot="$sysroot" -D_GNU_SOURCE -std=c++17 -O2 -fPIC -pie \
    -fexceptions -frtti -pthread -Wl,--build-id=none \
    "$repo_dir/ports/libcxx/runtime-probe.cpp" -o "$output"

file "$output" | grep -q 'ELF 64-bit.*ARM aarch64'
"$readelf" -l "$output" | grep -Fq '/lib/ld-musl-aarch64.so.1'
"$readelf" -d "$output" | grep -Fq 'Shared library: [libc.so]'
echo "MAKOS_FIREFOX_DYNAMIC_RUNTIME_OK elf=aarch64 pie=1 interp=musl libc=shared libcxx=static dlopen=real"
