#!/bin/sh
# SPDX-License-Identifier: MIT
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
source_dir=${MUSL_SOURCE_DIR:-"$repo_dir/build/ports/musl/source"}
build_dir=${MUSL_BUILD_DIR:-"$repo_dir/build/ports/musl/makos-static"}
stage_dir=${MUSL_STAGE_DIR:-"$repo_dir/build/ports/musl/sysroot-static"}
ar_tool=${MAKOS_AR:-/opt/homebrew/opt/llvm/bin/llvm-ar}
nm_tool=${MAKOS_NM:-/opt/homebrew/opt/llvm/bin/llvm-nm}
objdump_tool=${MAKOS_OBJDUMP:-/opt/homebrew/opt/llvm/bin/llvm-objdump}
readelf_tool=${MAKOS_READELF:-/opt/homebrew/opt/llvm/bin/llvm-readelf}

test -s "$build_dir/lib/libc.a"
makos_syscall_obj="$build_dir/obj/src/internal/makos_syscall.lo"
clone_obj="$build_dir/obj/src/thread/aarch64/clone.lo"
vfork_obj="$build_dir/obj/src/process/aarch64/vfork.lo"
test -s "$makos_syscall_obj"
test -s "$stage_dir/usr/include/stdio.h"
test -s "$stage_dir/usr/include/pthread.h"
test -s "$stage_dir/usr/include/sys/mman.h"
test -s "$stage_dir/usr/lib/libc.a"
test -f "$stage_dir/usr/lib/libpthread.a"
test -x "$build_dir/makos-musl-probe.elf"
test -x "$build_dir/makos-musl-crt-probe.elf"
test -x "$build_dir/makos-musl-pthread-probe.elf"
file "$build_dir/makos-musl-probe.elf" | grep -q 'ELF 64-bit.*ARM aarch64.*statically linked'
$readelf_tool -l "$build_dir/makos-musl-probe.elf" | grep -q 'LOAD.*0x0000000010000000'
$readelf_tool -l "$build_dir/makos-musl-crt-probe.elf" | grep -q 'LOAD.*0x0000000010000000'
$readelf_tool -l "$build_dir/makos-musl-pthread-probe.elf" | grep -q 'LOAD.*0x0000000010000000'

members=$($ar_tool t "$build_dir/lib/libc.a" | wc -l | tr -d ' ')
test "$members" -gt 1300
$nm_tool "$build_dir/lib/libc.a" | grep -q ' T __makos_syscall_dispatch$'
$nm_tool "$build_dir/lib/libc.a" | grep -q ' T __clone$'
for symbol in _exit close open read write; do
	$nm_tool "$build_dir/makos-musl-probe.elf" | grep -q " T $symbol\$"
done
for symbol in _start __libc_start_main main; do
	$nm_tool "$build_dir/makos-musl-crt-probe.elf" | grep -q " T $symbol\$"
done
$nm_tool -u "$build_dir/crt-probe.o" | grep -q '__stack_chk_fail'
$nm_tool "$build_dir/makos-musl-crt-probe.elf" | grep -q ' T __stack_chk_fail$'
test -z "$($nm_tool -u "$build_dir/makos-musl-crt-probe.elf")"
for symbol in _start __libc_start_main main pthread_create pthread_join; do
	$nm_tool "$build_dir/makos-musl-pthread-probe.elf" | grep -Eq " [TW] $symbol\$"
done
test -z "$($nm_tool -u "$build_dir/makos-musl-pthread-probe.elf")"
test -z "$($ar_tool t "$stage_dir/usr/lib/libpthread.a")"

test -n "$($objdump_tool -d "$clone_obj" | grep 'svc' || true)"
test -z "$($objdump_tool -d "$vfork_obj" | grep 'svc' || true)"
svc_sources=$(rg -l '\bsvc\b' "$source_dir/src" "$source_dir/arch/aarch64" \
	| grep -E '(/aarch64/|/internal/makos_syscall\.c$)' | sort)
expected_sources=$(printf '%s\n' \
	"$source_dir/src/internal/makos_syscall.c" \
	"$source_dir/src/signal/aarch64/restore.s" \
	"$source_dir/src/thread/aarch64/__unmapself.s" \
	"$source_dir/src/thread/aarch64/clone.s" | sort)
test "$svc_sources" = "$expected_sources"

gates=$(grep -v '^#' "$port_dir/missing-syscalls.txt" | grep -c . | tr -d ' ')
echo "MAKOS_MUSL_STATIC_OK version=1.2.6 arch=aarch64 members=$members probes=custom-entry,upstream-crt1,pthread"
echo "MAKOS_MUSL_THREAD_ABI_OK clone=80 futex=82 exit=81 pthread_archive=compat-empty missing_gates=$gates"
