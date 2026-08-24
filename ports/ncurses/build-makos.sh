#!/bin/sh
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
. "$port_dir/source.lock"

base_sysroot=${MAKOS_NCURSES_BASE_SYSROOT:-$repo_dir/build/ports/libcxx/sysroot}
cc=${MAKOS_NCURSES_CC:-"$repo_dir/ports/firefox/toolchain/makos-clang --sysroot=$base_sysroot"}
ar=${MAKOS_NCURSES_AR:-/opt/homebrew/opt/llvm/bin/llvm-ar}
ranlib=${MAKOS_NCURSES_RANLIB:-/opt/homebrew/opt/llvm/bin/llvm-ranlib}
tic=${MAKOS_HOST_TIC:-/opt/homebrew/opt/ncurses/bin/tic}
work_root="$repo_dir/build/ports/ncurses"
source_dir="$work_root/source-$NCURSES_VERSION"
build_dir="$work_root/makos"
stage_dir="$work_root/stage"

for required in "$base_sysroot/usr/lib/libc.a" \
    "$base_sysroot/usr/lib/makos/libclang_rt.builtins-aarch64.a" \
    "$ar" "$ranlib" "$tic"; do
    test -e "$required" || {
        echo "ncurses MakOS build missing dependency: $required" >&2
        exit 2
    }
done

archive=$($port_dir/fetch.sh)
rm -rf "$source_dir" "$build_dir" "$stage_dir"
mkdir -p "$source_dir" "$build_dir" "$stage_dir"
tar -xzf "$archive" -C "$source_dir" --strip-components=1
patch -s -d "$source_dir" -p1 < \
    "$port_dir/patches/0001-config-sub-recognize-makos.patch"

build_triplet=$($source_dir/config.guess)
(
    cd "$build_dir"
    CC="$cc" AR="$ar" RANLIB="$ranlib" \
    CFLAGS="-O2 -fPIC" \
    cf_cv_func_nanosleep=yes ac_cv_func_sigaction=yes \
    ac_cv_header_sys_select_h=yes \
    "$source_dir/configure" \
        --build="$build_triplet" \
        --host=aarch64-unknown-makos \
        --prefix=/usr \
        --with-terminfo-dirs=/usr/share/terminfo \
        --with-default-terminfo-dir=/usr/share/terminfo \
        --enable-overwrite \
        --without-shared --with-normal --without-debug \
        --without-ada --without-cxx --without-cxx-binding \
        --without-tests --without-manpages --without-progs \
        --disable-widec --disable-stripping \
        --disable-home-terminfo --disable-root-environ \
        >configure.log
    make -j"${NCURSES_JOBS:-4}" >build.log
    make DESTDIR="$stage_dir" install.includes install.libs >install.log
)

terminfo_build="$work_root/terminfo-host"
rm -rf "$terminfo_build"
mkdir -p "$terminfo_build" "$stage_dir/usr/share/terminfo/m"
"$tic" -x -o "$terminfo_build" "$port_dir/makos.terminfo"
if test -f "$terminfo_build/m/makos"; then
    compiled="$terminfo_build/m/makos"
elif test -f "$terminfo_build/6d/makos"; then
    compiled="$terminfo_build/6d/makos"
else
    echo "ncurses MakOS terminfo compilation failed" >&2
    exit 1
fi
cp "$compiled" "$stage_dir/usr/share/terminfo/m/makos"
cp "$source_dir/COPYING" "$stage_dir/COPYING.ncurses"

file "$stage_dir/usr/lib/libncurses.a" | grep -F 'current ar archive' >/dev/null
file "$stage_dir/usr/share/terminfo/m/makos" | grep -F 'Compiled terminfo entry' >/dev/null
echo "MAKOS_NCURSES_BUILD_OK version=$NCURSES_VERSION target=aarch64-unknown-makos static_pic=1 terminfo=makos"
echo "stage=$stage_dir"
