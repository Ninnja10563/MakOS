#!/bin/sh
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. https://mozilla.org/MPL/2.0/
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
. "$port_dir/source.lock"

case "$FIREFOX_URL $FIREFOX_REPOSITORY" in
    https://archive.mozilla.org/*https://github.com/mozilla-firefox/firefox.git) ;;
    *) echo "firefox audit: non-official source endpoint" >&2; exit 1 ;;
esac
test ${#FIREFOX_SHA512} -eq 128
test ${#FIREFOX_COMMIT} -eq 40
test -s "$port_dir/required-abi.txt"
test -s "$port_dir/mozconfig.makos"
test -s "$port_dir/patches/0001-makos-target-recognition.patch"
test -s "$port_dir/patches/0002-makos-widget-surface-abi.patch"
test -s "$port_dir/patches/0003-nspr-makos-platform.patch"
test -s "$port_dir/patches/0058-makos-pdf-print-settings.patch"
test -s "$port_dir/patches/0059-rust-errno-makos-accessor.patch"
test -s "$port_dir/toolchain-probe.c"
toolchain_status=$("$port_dir/toolchain-audit.sh")

sysroot=${MAKOS_SYSROOT:-"$repo_dir/sdk/sysroot-aarch64"}
missing=0
for path in \
    usr/include/stdio.h usr/include/stdlib.h usr/include/unistd.h \
    usr/include/pthread.h usr/include/signal.h usr/include/sys/mman.h \
    usr/include/sys/socket.h usr/include/poll.h usr/lib/crt1.o \
    usr/lib/libc.a usr/lib/libm.a usr/lib/libpthread.a usr/lib/libc++.a \
    usr/lib/libc++abi.a
do
    if test ! -e "$sysroot/$path"; then
        echo "missing-sysroot=$path"
        missing=$((missing + 1))
    fi
done

echo "MAKOS_FIREFOX_FOUNDATION_OK version=$FIREFOX_VERSION commit=$FIREFOX_COMMIT"
echo "MAKOS_FIREFOX_WIDGET_FOUNDATION_OK toolkit=makos surface_abi=declared nsIWidget=blocked"
echo "MAKOS_FIREFOX_NSPR_FOUNDATION_OK os=MakOS arch=aarch64 pthreads=selected runtime=blocked"
echo "$toolchain_status"
echo "MAKOS_FIREFOX_EXECUTABLE_BLOCKED official_source=1 fake=0 missing_sysroot=$missing"
echo "sysroot=$sysroot"
echo "runtime-gates=$(grep -v '^#' "$port_dir/required-abi.txt" | paste -sd, -)"
