#!/bin/sh
# SPDX-License-Identifier: Python-2.0
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
source_dir="$repo_dir/build/ports/cpython/source"
build_dir="$repo_dir/build/ports/cpython/makos"
sysroot=${MAKOS_SYSROOT:-"$repo_dir/build/ports/cpython/sysroot"}
base_sysroot=${MAKOS_CPYTHON_BASE_SYSROOT:-"$repo_dir/build/ports/libcxx/sysroot"}
shared_sysroot=${MAKOS_CPYTHON_SHARED_SYSROOT:-"$repo_dir/build/ports/musl/sysroot-shared"}
cc=${MAKOS_CC:-"$repo_dir/ports/firefox/toolchain/makos-clang"}
build_python=${CPYTHON_BUILD_PYTHON:-/opt/homebrew/bin/python3.14}
ar_tool=${MAKOS_AR:-/opt/homebrew/opt/llvm/bin/llvm-ar}
ranlib_tool=${MAKOS_RANLIB:-/opt/homebrew/opt/llvm/bin/llvm-ranlib}
builtins=${MAKOS_BUILTINS:-"$repo_dir/build/ports/libcxx/sysroot/usr/lib/makos/libclang_rt.builtins-aarch64.a"}
jobs=${CPYTHON_JOBS:-$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 4)}

. "$port_dir/host-tools.sh"
makos_cpython_select_readelf "$repo_dir"

"$port_dir/fetch.sh" >/dev/null
"$port_dir/apply-patches.sh" >/dev/null
test -x "$build_python" || {
	echo "CPython MakOS build blocked: host Python 3.14 generator absent" >&2
	exit 2
}
test -s "$base_sysroot/usr/lib/makos/libclang_rt.builtins-aarch64.a" || {
	echo "CPython MakOS build blocked: base runtime sysroot absent" >&2
	exit 2
}
test -s "$shared_sysroot/usr/lib/libc.so" || {
	echo "CPython MakOS build blocked: shared musl sysroot absent" >&2
	exit 2
}
test -x "$cc" || {
	echo "CPython MakOS build blocked: MakOS clang driver absent" >&2
	exit 2
}
test -x "$ar_tool" && test -x "$ranlib_tool" || {
	echo "CPython MakOS build blocked: LLVM archive tools absent" >&2
	exit 2
}
test -s "$builtins" || {
	echo "CPython MakOS build blocked: AArch64 compiler-rt builtins absent" >&2
	exit 2
}
mkdir -p "$sysroot"
cp -R "$base_sysroot/." "$sysroot/"
cp -R "$shared_sysroot/." "$sysroot/"
test -s "$sysroot/usr/lib/libc.so"
test -s "$sysroot/usr/lib/makos/libclang_rt.builtins-aarch64.a"
mkdir -p "$build_dir"
cd "$build_dir"
if test ! -f Makefile; then
	CONFIG_SITE="$port_dir/config.site" \
	CC="$cc --sysroot=$sysroot" \
	CXX=false \
	AR="$ar_tool" \
	RANLIB="$ranlib_tool" \
	PKG_CONFIG=false \
	CFLAGS="-O2 -fPIE" \
	LDFLAGS="-pie -L$sysroot/usr/lib" \
	MAKOS_DYNAMIC_RUNTIME=1 \
	"$source_dir/configure" \
		--build="$("$source_dir/config.guess")" \
		--host=aarch64-unknown-makos \
		--prefix=/usr \
		--with-build-python="$build_python" \
		--disable-shared \
		--disable-test-modules \
		--disable-ipv6 \
		--with-pkg-config=no \
		--without-ensurepip
fi

MAKOS_DYNAMIC_RUNTIME=1 make -j"$jobs" python.exe
test -x python.exe
file python.exe | grep -q 'ELF 64-bit LSB pie executable, ARM aarch64'
"$MAKOS_READELF" -h -l -d python.exe >elf-audit.txt
grep -q 'Type:.*DYN' elf-audit.txt
grep -q 'Requesting program interpreter: /lib/ld-musl-aarch64.so.1' elf-audit.txt
grep -q 'Shared library: \[libc.so\]' elf-audit.txt

echo "MAKOS_CPYTHON_BUILD_OK version=3.14.7 target=aarch64-unknown-makos executable=$build_dir/python.exe pie=1 pt_interp=musl host_delegation=0"
