#!/bin/sh
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
. "$port_dir/source.lock"

sysroot=${MAKOS_NANO_SYSROOT:-$repo_dir/build/ports/nano/sysroot}
base_sysroot=${MAKOS_NANO_BASE_SYSROOT:-$repo_dir/build/ports/libcxx/sysroot}
shared_sysroot=${MAKOS_NANO_SHARED_SYSROOT:-$repo_dir/build/ports/musl/sysroot-shared}
ncurses_stage=${MAKOS_NANO_NCURSES_STAGE:-$repo_dir/build/ports/ncurses/stage}

test -s "$base_sysroot/usr/lib/libc.a" || {
    echo "GNU nano MakOS build blocked: base sysroot absent: $base_sysroot" >&2
    exit 2
}
test -s "$shared_sysroot/usr/lib/libc.so" || {
    echo "GNU nano MakOS build blocked: shared musl absent: $shared_sysroot" >&2
    exit 2
}
"$repo_dir/ports/ncurses/build-makos.sh"
rm -rf "$sysroot"
mkdir -p "$sysroot"
cp -R "$base_sysroot/." "$sysroot/"
cp -R "$shared_sysroot/." "$sysroot/"
cp -R "$ncurses_stage/." "$sysroot/"
missing=
for path in \
    usr/include/stdio.h usr/include/stdlib.h usr/include/string.h \
    usr/include/errno.h usr/include/fcntl.h usr/include/unistd.h \
    usr/include/dirent.h usr/include/regex.h usr/include/signal.h \
    usr/include/sys/stat.h usr/include/termios.h usr/lib/crt1.o \
    usr/lib/libc.a
do
    test -e "$sysroot/$path" || missing="$missing $path"
done
if test ! -e "$sysroot/usr/include/ncurses.h" && \
   test ! -e "$sysroot/usr/include/curses.h"; then
    missing="$missing usr/include/{ncurses.h,curses.h}"
fi
if test ! -e "$sysroot/usr/lib/libncursesw.a" && \
   test ! -e "$sysroot/usr/lib/libncurses.a"; then
    missing="$missing usr/lib/{libncursesw.a,libncurses.a}"
fi

if test -n "$missing"; then
    echo "GNU nano $NANO_VERSION MakOS cross-build blocked." >&2
    echo "Missing sysroot entries under $sysroot:" >&2
    for path in $missing; do echo "  $path" >&2; done
    echo "Kernel/runtime gates: required-abi.txt" >&2
    exit 2
fi

archive=$($port_dir/fetch.sh)
work_root="$repo_dir/build/ports/nano"
source_dir="$work_root/makos-source-$NANO_VERSION"
build_dir="$work_root/makos"
rm -rf "$source_dir" "$build_dir"
mkdir -p "$source_dir" "$build_dir"
tar -xJf "$archive" -C "$source_dir" --strip-components=1
patch -s -d "$source_dir" -p1 < \
    "$port_dir/patches/0001-config-sub-recognize-makos.patch"

build_triplet=$($source_dir/config.guess)
cc=${MAKOS_NANO_CC:-"$repo_dir/ports/firefox/toolchain/makos-clang --sysroot=$sysroot"}
ar=${MAKOS_NANO_AR:-/opt/homebrew/opt/llvm/bin/llvm-ar}
ranlib=${MAKOS_NANO_RANLIB:-/opt/homebrew/opt/llvm/bin/llvm-ranlib}

(cd "$build_dir" && \
    CC="$cc" AR="$ar" RANLIB="$ranlib" \
    PKG_CONFIG=false \
    ac_cv_func_fork=no ac_cv_func_vfork=no \
    CPPFLAGS="-I$sysroot/usr/include ${CPPFLAGS:-}" \
    CFLAGS="-O2 -fPIE ${CFLAGS:-}" \
    LDFLAGS="-pie -L$sysroot/usr/lib ${LDFLAGS:-}" \
    MAKOS_DYNAMIC_RUNTIME=1 \
    "$source_dir/configure" \
        --build="$build_triplet" \
        --host=aarch64-unknown-makos \
        --prefix=/usr \
        --disable-nls \
        --disable-libmagic \
        --disable-utf8 \
        >configure.log)

case $(uname -s) in
    Darwin) jobs=$(sysctl -n hw.logicalcpu 2>/dev/null || printf '4') ;;
    *) jobs=$(getconf _NPROCESSORS_ONLN 2>/dev/null || printf '4') ;;
esac
MAKOS_DYNAMIC_RUNTIME=1 make -C "$build_dir" -j"$jobs" >"$build_dir/build.log"

binary="$build_dir/src/nano"
file "$binary" | grep -E 'ELF 64-bit.*ARM aarch64' >/dev/null || {
    echo "nano cross-build produced non-MakOS artifact: $binary" >&2
    exit 1
}
"${MAKOS_NANO_READELF:-/opt/homebrew/opt/llvm/bin/llvm-readelf}" -h -l -d "$binary" \
    >"$build_dir/elf-audit.txt"
grep -F 'Type:                              DYN' "$build_dir/elf-audit.txt" >/dev/null
grep -F 'Requesting program interpreter: /lib/ld-musl-aarch64.so.1' \
    "$build_dir/elf-audit.txt" >/dev/null
grep -F 'Shared library: [libc.so]' "$build_dir/elf-audit.txt" >/dev/null
cp "$source_dir/COPYING" "$build_dir/COPYING"
cp "$source_dir/COPYING.DOC" "$build_dir/COPYING.DOC"
printf '%s\n' \
    "MAKOS_NANO_BUILD_OK version=$NANO_VERSION target=aarch64-unknown-makos source=official pie=1 pt_interp=musl ncurses=6.5 terminfo=makos fake=0"
printf '%s\n' "$binary"
