#!/bin/sh
# SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
. "$port_dir/source.lock"

source_dir=${LLVM_SOURCE_DIR:-"$repo_dir/build/ports/libcxx/source"}
sysroot=${LIBCXX_STAGE_DIR:-"$repo_dir/build/ports/libcxx/sysroot"}
runtime_build=${LIBCXX_BUILD_DIR:-"$repo_dir/build/ports/libcxx/makos-static"}
builtins_build=${BUILTINS_BUILD_DIR:-"$repo_dir/build/ports/libcxx/compiler-rt-builtins"}
probe=${LIBCXX_PROBE_ELF:-"$repo_dir/build/ports/libcxx/probe/runtime-probe.elf"}
readelf=${LLVM_READELF:-/opt/homebrew/opt/llvm/bin/llvm-readelf}
nm_tool=${LLVM_NM:-/opt/homebrew/opt/llvm/bin/llvm-nm}

test "$LLVM_REPOSITORY" = https://github.com/llvm/llvm-project.git
test "$LLVM_TAG" = llvmorg-22.1.8
test "$LLVM_LICENSE" = 'Apache-2.0 WITH LLVM-exception'
test "$(git -C "$source_dir" rev-parse HEAD)" = "$LLVM_COMMIT"
test "$(git -C "$source_dir" remote get-url origin)" = "$LLVM_REPOSITORY"

for archive in \
    "$sysroot/usr/lib/libc++.a" \
    "$sysroot/usr/lib/libc++abi.a" \
    "$sysroot/usr/lib/libunwind.a" \
    "$sysroot/usr/lib/makos/libclang_rt.builtins-aarch64.a"; do
    test -s "$archive"
    machines=$($readelf -h "$archive" | awk '/Machine:/ { print $2 }' | sort -u)
    test "$machines" = AArch64 || {
        echo "non-AArch64 archive member in $archive: $machines" >&2
        exit 1
    }
done

test -f "$sysroot/usr/include/c++/v1/thread"
test -f "$sysroot/usr/include/c++/v1/string"
test -f "$sysroot/usr/lib/crt1.o"
test ! -e "$sysroot/usr/lib/libc++.so"
test ! -e "$sysroot/usr/lib/libc++.dylib"
runtime_system=$(find "$runtime_build/CMakeFiles" -name CMakeSystem.cmake -print | head -n 1)
builtins_system=$(find "$builtins_build/CMakeFiles" -name CMakeSystem.cmake -print | head -n 1)
grep -q 'set(CMAKE_SYSTEM_NAME "MakOS")' "$runtime_system"
grep -q 'set(CMAKE_SYSTEM_NAME "MakOS")' "$builtins_system"
grep -q -- '--target=aarch64-unknown-makos' "$runtime_build/CMakeFiles/rules.ninja"
grep -q -- '--target=aarch64-unknown-makos' "$builtins_build/CMakeFiles/rules.ninja"

builtins="$sysroot/usr/lib/makos/libclang_rt.builtins-aarch64.a"
for symbol in __extenddftf2 __floatsitf __multf3 __eqtf2 __netf2 \
    __subtf3 __addtf3 __fixunstfsi __floatunsitf __fixtfsi __divtf3 \
    __extendsftf2 __getf2 __trunctfsf2 __trunctfdf2 __letf2; do
    "$nm_tool" --defined-only "$builtins" | grep -q " $symbol$" || {
        echo "compiler-rt builtin missing: $symbol" >&2
        exit 1
    }
done

file "$probe" | grep -q 'ELF 64-bit.*ARM aarch64.*statically linked'
test -z "$($readelf -d "$probe" | grep '(NEEDED)' || true)"
test -z "$($readelf -Ws "$probe" | awk '$7 == "UND" && ($5 == "GLOBAL" || $5 == "WEAK") { print }')"

echo "MAKOS_LIBCXX_PORT_OK upstream=LLVM-$LLVM_VERSION target=aarch64-unknown-makos runtimes=libc++,libc++abi,libunwind,compiler-rt-builtins"
echo "MAKOS_LIBCXX_LINK_OK elf=static needed=0 global_undefined=0 runtime_executed=0"
