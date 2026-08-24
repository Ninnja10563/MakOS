#!/bin/sh
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
out_dir="$repo_dir/build/ports/nano/makos-abi"
mkdir -p "$out_dir"

clang -target aarch64-unknown-none-elf -std=c17 -ffreestanding -fno-builtin \
    -fno-stack-protector -fno-pic -fno-unwind-tables \
    -fno-asynchronous-unwind-tables -mgeneral-regs-only -Os \
    -I"$port_dir/makos" -c "$port_dir/makos/makos_abi.c" \
    -o "$out_dir/makos_abi.o"
clang -target aarch64-unknown-none-elf -std=c17 -ffreestanding -fno-builtin \
    -fno-stack-protector -fno-pic -fno-unwind-tables \
    -fno-asynchronous-unwind-tables -mgeneral-regs-only -Os \
    -I"$port_dir/makos" -c "$port_dir/makos/abi_probe.c" \
    -o "$out_dir/abi_probe.o"

rust_sysroot=$(rustc --print sysroot)
rust_host=$(rustc -vV | awk '/^host: / { print $2 }')
lld="$rust_sysroot/lib/rustlib/$rust_host/bin/rust-lld"
"$lld" -flavor gnu --build-id=none -z max-page-size=4096 \
    -T "$port_dir/makos/linker.ld" -o "$out_dir/abi-probe.elf" \
    "$out_dir/abi_probe.o" "$out_dir/makos_abi.o"

file "$out_dir/abi-probe.elf"
printf '%s\n' \
    'MAKOS_NANO_ABI_LAYER_OK arch=aarch64 svc=write,read_key,clock,exit' \
    'MAKOS_NANO_ABI_FOUNDATION_OK official_source=1 fake=0' \
    'native_build=ports/nano/build-makos.sh runtime_test=scripts/boot_test_aarch64_nano.py'
