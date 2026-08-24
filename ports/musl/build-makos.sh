#!/bin/sh
# SPDX-License-Identifier: MIT
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
source_dir=${MUSL_SOURCE_DIR:-"$repo_dir/build/ports/musl/source"}
build_dir=${MUSL_BUILD_DIR:-"$repo_dir/build/ports/musl/makos-static"}
stage_dir=${MUSL_STAGE_DIR:-"$repo_dir/build/ports/musl/sysroot-static"}
cc=${MAKOS_CC:-"$repo_dir/ports/firefox/toolchain/makos-clang"}
ar_tool=${MAKOS_AR:-/opt/homebrew/opt/llvm/bin/llvm-ar}
ranlib_tool=${MAKOS_RANLIB:-/opt/homebrew/opt/llvm/bin/llvm-ranlib}
jobs=${MUSL_JOBS:-$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 4)}

test -d "$source_dir/.git" || "$port_dir/clone.sh" >/dev/null
MUSL_REAPPLY_PATCHES=1 "$port_dir/apply-patches.sh"
test -x "$cc" || { echo "MakOS clang driver absent: $cc" >&2; exit 1; }
test -x "$ar_tool" || { echo "LLVM ar absent: $ar_tool" >&2; exit 1; }
test -x "$ranlib_tool" || { echo "LLVM ranlib absent: $ranlib_tool" >&2; exit 1; }

mkdir -p "$build_dir" "$stage_dir"
if test -f "$build_dir/config.mak"; then
	make -C "$build_dir" clean
fi
(
	cd "$build_dir"
	"$source_dir/configure" \
		--target=aarch64-unknown-makos \
		--prefix=/usr \
		--syslibdir=/lib \
		--disable-shared \
		--disable-wrapper \
		CC="$cc" AR="$ar_tool" RANLIB="$ranlib_tool" \
		CFLAGS="-fPIC"
	make -s -j"$jobs"
	make -s DESTDIR="$stage_dir" install-headers install-libs
)

# lld cannot lazily extract musl's protected stdio-internal symbols when a
# Gecko executable overrides another libc public symbol. Stage exact upstream
# objects for explicit links; keep libc.a unchanged.
mkdir -p "$stage_dir/usr/lib/makos"
"$ar_tool" p "$stage_dir/usr/lib/libc.a" __overflow.lo \
	>"$stage_dir/usr/lib/makos/__overflow.o"
"$ar_tool" p "$stage_dir/usr/lib/libc.a" __uflow.lo \
	>"$stage_dir/usr/lib/makos/__uflow.o"

"$cc" --sysroot="$stage_dir" -ffreestanding -fno-stack-protector \
	-c "$port_dir/sysroot-probe.c" -o "$build_dir/sysroot-probe.o"
"$cc" -nostdlib -static -Wl,-T,"$port_dir/makos/linker.ld" -Wl,-e,_start \
	-o "$build_dir/makos-musl-probe.elf" \
	"$build_dir/sysroot-probe.o" "$stage_dir/usr/lib/libc.a"

"$cc" --sysroot="$stage_dir" -ffreestanding -fstack-protector-strong \
	-c "$port_dir/crt-probe.c" -o "$build_dir/crt-probe.o"
"$cc" -nostdlib -static -Wl,-T,"$port_dir/makos/linker.ld" -Wl,-e,_start \
	-o "$build_dir/makos-musl-crt-probe.elf" \
	"$stage_dir/usr/lib/crt1.o" "$stage_dir/usr/lib/crti.o" \
	"$build_dir/crt-probe.o" "$stage_dir/usr/lib/libc.a" \
	"$stage_dir/usr/lib/crtn.o"

"$cc" --sysroot="$stage_dir" -ffreestanding -fno-stack-protector \
	-c "$port_dir/pthread-probe.c" -o "$build_dir/pthread-probe.o"
"$cc" -nostdlib -static -Wl,-T,"$port_dir/makos/linker.ld" -Wl,-e,_start \
	-o "$build_dir/makos-musl-pthread-probe.elf" \
	"$stage_dir/usr/lib/crt1.o" "$stage_dir/usr/lib/crti.o" \
	"$build_dir/pthread-probe.o" "$stage_dir/usr/lib/libc.a" \
	"$stage_dir/usr/lib/crtn.o"

MUSL_SOURCE_DIR="$source_dir" MUSL_BUILD_DIR="$build_dir" \
	MUSL_STAGE_DIR="$stage_dir" "$port_dir/audit-static.sh"
echo "stage=$stage_dir"
echo "runtime_probes=custom-entry,upstream-crt1,pthread"
