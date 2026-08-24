#!/bin/sh
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. https://mozilla.org/MPL/2.0/
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
out_dir="$repo_dir/build/ports/firefox/toolchain-probe"
if test -n "${MAKOS_CC-}"; then
    cc=$MAKOS_CC
elif test -x "$port_dir/toolchain/makos-clang" && \
    test -x /opt/homebrew/opt/llvm/bin/clang && \
    test -x /opt/homebrew/opt/lld/bin/ld.lld; then
    cc="$port_dir/toolchain/makos-clang"
else
    cc=clang
fi
require=0
test "${1-}" != --require || require=1

mkdir -p "$out_dir"
if ! command -v "$cc" >/dev/null 2>&1; then
    echo "MAKOS_FIREFOX_TOOLCHAIN_BLOCKED compiler=$cc reason=missing"
    test "$require" -eq 0
    exit
fi

"$cc" --target=aarch64-unknown-makos -D__makos__ -ffreestanding \
    -fno-builtin -fno-stack-protector -c "$port_dir/toolchain-probe.c" \
    -o "$out_dir/probe.o"
file "$out_dir/probe.o" | grep -q 'ELF 64-bit.*ARM aarch64'

if "$cc" --target=aarch64-unknown-makos -D__makos__ -ffreestanding \
    -fno-builtin -fno-stack-protector -nostdlib -fuse-ld=lld \
    -Wl,--build-id=none -Wl,-e,_start "$port_dir/toolchain-probe.c" \
    -o "$out_dir/probe.elf" >"$out_dir/link.stdout" \
    2>"$out_dir/link.stderr" && \
    file "$out_dir/probe.elf" | grep -q 'ELF 64-bit.*ARM aarch64'; then
    echo "MAKOS_FIREFOX_TOOLCHAIN_OK compiler=$cc object=elf64-aarch64 linker=lld"
    exit 0
fi

echo "MAKOS_FIREFOX_TOOLCHAIN_BLOCKED compiler=$cc object=elf64-aarch64 linker=lld-missing-or-unusable details=$out_dir/link.stderr"
test "$require" -eq 0
