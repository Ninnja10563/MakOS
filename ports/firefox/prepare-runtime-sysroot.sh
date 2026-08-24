#!/bin/sh
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. https://mozilla.org/MPL/2.0/
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
libcxx=${LIBCXX_STAGE_DIR:-"$repo_dir/build/ports/libcxx/sysroot"}
musl=${MUSL_SHARED_STAGE_DIR:-"$repo_dir/build/ports/musl/sysroot-shared"}
destination=${FIREFOX_RUNTIME_SYSROOT:-"$repo_dir/build/ports/firefox/sysroot-runtime"}

for path in \
    "$libcxx/usr/lib/libc++.a" \
    "$libcxx/usr/lib/libc++abi.a" \
    "$libcxx/usr/lib/libunwind.a" \
    "$musl/usr/lib/libc.so" \
    "$musl/usr/lib/Scrt1.o"; do
    test -e "$path" || { echo "Firefox runtime sysroot prerequisite missing: $path" >&2; exit 1; }
done
test -L "$musl/lib/ld-musl-aarch64.so.1" || {
    echo "Firefox runtime sysroot interpreter link missing" >&2
    exit 1
}

mkdir -p "$destination"
rsync -a --delete "$libcxx/" "$destination/"
rsync -a "$musl/usr/include/" "$destination/usr/include/"
rsync -a "$musl/usr/lib/" "$destination/usr/lib/"
mkdir -p "$destination/lib"
rsync -a "$musl/lib/" "$destination/lib/"

test -f "$destination/usr/lib/libc.so"
test -f "$destination/usr/lib/Scrt1.o"
test -f "$destination/usr/lib/libc++.a"
test -L "$destination/lib/ld-musl-aarch64.so.1"
echo "MAKOS_FIREFOX_RUNTIME_SYSROOT_OK path=$destination libc=shared libcxx=static interp=/lib/ld-musl-aarch64.so.1"
