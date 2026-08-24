#!/bin/sh
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. https://mozilla.org/MPL/2.0/
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
bin_dir=${FIREFOX_BIN_DIR:-"$repo_dir/build/ports/firefox/obj-aarch64-makos/dist/bin"}
readelf=${MAKOS_READELF:-/opt/homebrew/opt/llvm/bin/llvm-readelf}

test -x "$readelf" || { echo "llvm-readelf absent: $readelf" >&2; exit 1; }

for name in firefox plugin-container xpcshell libxul.so
do
    path="$bin_dir/$name"
    test -f "$path" || { echo "Firefox MakOS binary absent: $path" >&2; exit 1; }
    "$readelf" -h "$path" | grep -Fq 'Machine:                           AArch64'
    "$readelf" -h "$path" | grep -Fq 'Class:                             ELF64'
done

for name in firefox plugin-container xpcshell
do
    path="$bin_dir/$name"
    "$readelf" -l "$path" | \
        grep -Fq '[Requesting program interpreter: /lib/ld-musl-aarch64.so.1]'
    "$readelf" -d "$path" | grep -Fq 'Shared library: [libc.so]'
done

"$readelf" -h "$bin_dir/firefox" | \
    grep -Eq 'Entry point address: +0x[1-9a-fA-F][0-9a-fA-F]*'
"$readelf" -d "$bin_dir/plugin-container" | grep -Fq 'Shared library: [libxul.so]'
"$readelf" -d "$bin_dir/xpcshell" | grep -Fq 'Shared library: [libxul.so]'
"$readelf" -d "$bin_dir/libxul.so" | grep -Fq 'Library soname: [libxul.so]'
"$readelf" -d "$bin_dir/libxul.so" | grep -Fq 'Shared library: [libnss3.so]'
"$readelf" -d "$bin_dir/libxul.so" | grep -Fq 'Shared library: [libssl3.so]'
"$readelf" -d "$bin_dir/libnspr4.so" | grep -Fq 'Shared library: [libc.so]'

echo "MAKOS_FIREFOX_BINARY_OK target=aarch64-unknown-makos elf=firefox,plugin-container,xpcshell,libxul gecko=linked nss=linked runtime=shared-musl interp=/lib/ld-musl-aarch64.so.1"
