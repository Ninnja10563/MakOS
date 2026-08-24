#!/bin/sh
# SPDX-License-Identifier: MIT
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
source_dir=${MUSL_SOURCE_DIR:-"$repo_dir/build/ports/musl/source"}
build_dir=${MUSL_SHARED_BUILD_DIR:-"$repo_dir/build/ports/musl/makos-shared"}
stage_dir=${MUSL_SHARED_STAGE_DIR:-"$repo_dir/build/ports/musl/sysroot-shared"}
cc=${MAKOS_CC:-"$repo_dir/ports/firefox/toolchain/makos-clang"}
ar_tool=${MAKOS_AR:-/opt/homebrew/opt/llvm/bin/llvm-ar}
ranlib_tool=${MAKOS_RANLIB:-/opt/homebrew/opt/llvm/bin/llvm-ranlib}
builtins=${MAKOS_BUILTINS:-"$repo_dir/build/ports/libcxx/sysroot/usr/lib/makos/libclang_rt.builtins-aarch64.a"}
jobs=${MUSL_JOBS:-$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 4)}

test -d "$source_dir/.git" || "$port_dir/clone.sh" >/dev/null
"$port_dir/apply-patches.sh"
test -x "$cc" || { echo "MakOS clang driver absent: $cc" >&2; exit 1; }
test -x "$ar_tool" || { echo "LLVM ar absent: $ar_tool" >&2; exit 1; }
test -x "$ranlib_tool" || { echo "LLVM ranlib absent: $ranlib_tool" >&2; exit 1; }
test -s "$builtins" || {
	echo "MakOS compiler-rt builtins absent: $builtins" >&2
	echo "Run ports/libcxx/build-makos.sh first." >&2
	exit 1
}

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
		--enable-shared \
		--disable-wrapper \
		CC="$cc" AR="$ar_tool" RANLIB="$ranlib_tool" \
		CFLAGS="-fPIC"
	make -s -j"$jobs" LIBCC="$builtins"
	make -s DESTDIR="$stage_dir" LIBCC="$builtins" install-headers install-libs
)

"$cc" --sysroot="$stage_dir" -fno-stack-protector \
	-c "$port_dir/dynamic-probe.c" -o "$build_dir/dynamic-probe.o"
"$cc" --sysroot="$stage_dir" -nostdlib -pie \
	-Wl,--dynamic-linker=/lib/ld-musl-aarch64.so.1 \
	-o "$build_dir/makos-musl-dynamic-probe.elf" \
	"$stage_dir/usr/lib/Scrt1.o" "$stage_dir/usr/lib/crti.o" \
	"$build_dir/dynamic-probe.o" -L"$stage_dir/usr/lib" -lc \
	"$stage_dir/usr/lib/crtn.o"

"$cc" -ffreestanding -fPIC -fno-builtin -fno-stack-protector \
	-c "$port_dir/interp-probe.c" -o "$build_dir/interp-probe.o"
"$cc" -nostdlib -pie -Wl,-z,max-page-size=4096 -Wl,--entry=_start \
	-Wl,--dynamic-linker=/lib/ld-musl-aarch64.so.1 \
	-o "$build_dir/makos-musl-interp-probe.elf" "$build_dir/interp-probe.o"

"$cc" -ffreestanding -fPIC -fvisibility=hidden \
	-c "$port_dir/shared-demo.c" -o "$build_dir/shared-demo.o"
"$cc" -nostdlib -shared -Wl,-soname,libmakosdemo.so \
	-Wl,-z,max-page-size=4096 -Wl,--strip-all \
	-o "$build_dir/libmakosdemo.so" "$build_dir/shared-demo.o"
"$cc" --sysroot="$stage_dir" -fno-stack-protector \
	-c "$port_dir/shared-demo-app.c" -o "$build_dir/shared-demo-app.o"
"$cc" --sysroot="$stage_dir" -nostdlib -pie \
	-Wl,-z,max-page-size=4096 \
	-Wl,--dynamic-linker=/lib/ld-musl-aarch64.so.1 \
	-o "$build_dir/makos-musl-dso-probe.elf" \
	"$stage_dir/usr/lib/Scrt1.o" "$stage_dir/usr/lib/crti.o" \
	"$build_dir/shared-demo-app.o" -L"$build_dir" -lmakosdemo \
	-L"$stage_dir/usr/lib" -lc "$stage_dir/usr/lib/crtn.o"

"$cc" --sysroot="$stage_dir" -fno-stack-protector \
	-c "$port_dir/dlopen-demo-app.c" -o "$build_dir/dlopen-demo-app.o"
"$cc" --sysroot="$stage_dir" -nostdlib -pie \
	-Wl,-z,max-page-size=4096 \
	-Wl,--dynamic-linker=/lib/ld-musl-aarch64.so.1 \
	-o "$build_dir/makos-musl-dlopen-probe.elf" \
	"$stage_dir/usr/lib/Scrt1.o" "$stage_dir/usr/lib/crti.o" \
	"$build_dir/dlopen-demo-app.o" -L"$stage_dir/usr/lib" -ldl -lc \
	"$stage_dir/usr/lib/crtn.o"

for program in exec-caller exec-target; do
	"$cc" --sysroot="$stage_dir" -fPIC -fno-stack-protector \
		-c "$port_dir/$program.c" -o "$build_dir/$program.o"
	"$cc" --sysroot="$stage_dir" -nostdlib -pie \
		-Wl,-z,max-page-size=4096 \
		-Wl,--dynamic-linker=/lib/ld-musl-aarch64.so.1 \
		-o "$build_dir/makos-musl-$program.elf" \
		"$stage_dir/usr/lib/Scrt1.o" "$stage_dir/usr/lib/crti.o" \
		"$build_dir/$program.o" -L"$stage_dir/usr/lib" -lc \
		"$stage_dir/usr/lib/crtn.o"
done

MUSL_SOURCE_DIR="$source_dir" MUSL_SHARED_BUILD_DIR="$build_dir" \
	MUSL_SHARED_STAGE_DIR="$stage_dir" "$port_dir/audit-shared.sh"
echo "stage=$stage_dir"
echo "runtime_probe=linked-not-executed"
