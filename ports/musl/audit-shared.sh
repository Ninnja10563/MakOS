#!/bin/sh
# SPDX-License-Identifier: MIT
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
build_dir=${MUSL_SHARED_BUILD_DIR:-"$repo_dir/build/ports/musl/makos-shared"}
stage_dir=${MUSL_SHARED_STAGE_DIR:-"$repo_dir/build/ports/musl/sysroot-shared"}
readelf=${MAKOS_READELF:-/opt/homebrew/opt/llvm/bin/llvm-readelf}
nm_tool=${MAKOS_NM:-/opt/homebrew/opt/llvm/bin/llvm-nm}
loader="$stage_dir/usr/lib/libc.so"
probe="$build_dir/makos-musl-dynamic-probe.elf"
interp_probe="$build_dir/makos-musl-interp-probe.elf"
dso="$build_dir/libmakosdemo.so"
dso_probe="$build_dir/makos-musl-dso-probe.elf"
dlopen_probe="$build_dir/makos-musl-dlopen-probe.elf"
exec_caller="$build_dir/makos-musl-exec-caller.elf"
exec_target="$build_dir/makos-musl-exec-target.elf"

test -x "$readelf" || { echo "llvm-readelf absent: $readelf" >&2; exit 1; }
test -x "$nm_tool" || { echo "llvm-nm absent: $nm_tool" >&2; exit 1; }
test -x "$loader"
test -x "$probe"
test -x "$interp_probe"
test -x "$dso"
test -x "$dso_probe"
test -x "$dlopen_probe"
test -x "$exec_caller"
test -x "$exec_target"
test -L "$stage_dir/lib/ld-musl-aarch64.so.1"
test "$(readlink "$stage_dir/lib/ld-musl-aarch64.so.1")" = /usr/lib/libc.so

for path in "$loader" "$probe" "$interp_probe" "$dso" "$dso_probe" "$dlopen_probe" "$exec_caller" "$exec_target"; do
	"$readelf" -h "$path" | grep -Fq 'Class:                             ELF64'
	"$readelf" -h "$path" | grep -Fq 'Machine:                           AArch64'
done
"$readelf" -h "$loader" | grep -Eq 'Entry point address: +0x[1-9a-fA-F][0-9a-fA-F]*'
"$readelf" -l "$probe" | grep -Fq '/lib/ld-musl-aarch64.so.1'
"$readelf" -d "$probe" | grep -Fq 'Shared library: [libc.so]'
"$readelf" -l "$interp_probe" | grep -Fq '/lib/ld-musl-aarch64.so.1'
"$readelf" -d "$interp_probe" | grep -Fq '(RELA)'
"$readelf" -d "$dso" | grep -Fq 'Library soname: [libmakosdemo.so]'
"$readelf" -d "$dso_probe" | grep -Fq 'Shared library: [libmakosdemo.so]'
"$readelf" -d "$dso_probe" | grep -Fq 'Shared library: [libc.so]'
"$readelf" -d "$dlopen_probe" | grep -Fq 'Shared library: [libc.so]'
for path in "$exec_caller" "$exec_target"; do
	"$readelf" -l "$path" | grep -Fq '/lib/ld-musl-aarch64.so.1'
	"$readelf" -d "$path" | grep -Fq 'Shared library: [libc.so]'
done
if "$readelf" -d "$dlopen_probe" | grep -Fq 'Shared library: [libmakosdemo.so]'; then
	echo "dlopen probe unexpectedly has a static DT_NEEDED demo dependency" >&2
	exit 1
fi
test "$(wc -c < "$dso" | tr -d ' ')" -le 2048
if "$readelf" -d "$interp_probe" | grep -Fq '(NEEDED)'; then
	echo "interpreter probe unexpectedly needs a shared library" >&2
	exit 1
fi
"$nm_tool" "$loader" | grep -Eq ' T _dlstart$'
"$nm_tool" "$loader" | grep -Eq ' T dlopen$'
"$nm_tool" "$loader" | grep -Eq ' [Tt] __makos_syscall_dispatch$'

echo "MAKOS_MUSL_SHARED_OK version=1.2.6 arch=aarch64 loader=ld-musl-aarch64.so.1 pie=linked interp_probe=relative-rela dso=libmakosdemo.so execve=caller,target relocations=not-executed"
