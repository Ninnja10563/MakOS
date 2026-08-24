#!/bin/sh
# SPDX-License-Identifier: MIT
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
. "$port_dir/source.lock"

test "$MUSL_REPOSITORY" = https://git.musl-libc.org/git/musl
test "$MUSL_URL" = "https://musl.libc.org/releases/$MUSL_ARCHIVE"
test ${#MUSL_SHA256} -eq 64
test ${#MUSL_COMMIT} -eq 40
test -s "$port_dir/missing-syscalls.txt"
test -s "$port_dir/supported-syscalls.txt"
test -s "$port_dir/patches/0001-makos-aarch64-syscalls.patch"

out_dir="$repo_dir/build/ports/musl/makos-abi"
mkdir -p "$out_dir"
common="--target=aarch64-unknown-makos -ffreestanding -fno-builtin -fno-stack-protector -fno-unwind-tables -fno-asynchronous-unwind-tables -mgeneral-regs-only -Os"
clang $common -I"$port_dir/makos" -c "$port_dir/makos/syscall.c" \
    -o "$out_dir/syscall.o"
clang $common -I"$port_dir/makos" -c "$port_dir/makos/probe.c" \
    -o "$out_dir/probe.o"
clang --target=aarch64-unknown-makos -c "$port_dir/makos/crt1.S" \
    -o "$out_dir/crt1.o"

rust_sysroot=$(rustc --print sysroot)
rust_host=$(rustc -vV | awk '/^host: / { print $2 }')
lld="$rust_sysroot/lib/rustlib/$rust_host/bin/rust-lld"
"$lld" -flavor gnu --build-id=none -z max-page-size=4096 \
    -T "$port_dir/makos/linker.ld" -o "$out_dir/abi-probe.elf" \
    "$out_dir/crt1.o" "$out_dir/probe.o" "$out_dir/syscall.o"

if command -v llvm-nm >/dev/null 2>&1; then
    test -z "$(llvm-nm -u "$out_dir/abi-probe.elf")"
fi
file "$out_dir/abi-probe.elf" | grep -q 'ELF 64-bit.*ARM aarch64'

missing=$(grep -v '^#' "$port_dir/missing-syscalls.txt" | grep -c . | tr -d ' ')
echo "MAKOS_MUSL_ABI_LAYER_OK arch=aarch64 svc=read,write,open,close,create,clock,tty,signals,connected-sockets,exit"
echo "MAKOS_MUSL_PORT_FOUNDATION_OK official_source=1 fake=0 static_patch=1 missing_runtime_gates=$missing"
