#!/bin/sh
# SPDX-License-Identifier: Python-2.0
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
source_dir=${CPYTHON_SOURCE_DIR:-"$repo_dir/build/ports/cpython/source"}

test -f "$source_dir/configure" || {
	echo "CPython source absent: run ports/cpython/fetch.sh" >&2
	exit 1
}

if ! grep -q '\*-\*-makos\*)' "$source_dir/configure.ac"; then
	patch -d "$source_dir" -p1 <"$port_dir/patches/0001-makos-target.patch"
fi

grep -q '\*-\*-makos\*)' "$source_dir/configure"
grep -q 'macos\* | makos\*' "$source_dir/config.sub"
echo "MAKOS_CPYTHON_PATCHES_OK target=aarch64-unknown-makos"
