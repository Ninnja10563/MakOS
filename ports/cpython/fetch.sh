#!/bin/sh
# SPDX-License-Identifier: Python-2.0
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
. "$port_dir/source.lock"
dist_dir="$repo_dir/build/ports/cpython/distfiles"
source_dir="$repo_dir/build/ports/cpython/source"
archive="$dist_dir/$CPYTHON_ARCHIVE"

mkdir -p "$dist_dir"
if test ! -f "$archive"; then
	curl --fail --location --proto '=https' --tlsv1.2 "$CPYTHON_URL" -o "$archive"
fi
actual=$(shasum -a 256 "$archive" | awk '{print $1}')
test "$actual" = "$CPYTHON_SHA256" || {
	echo "CPython archive SHA-256 mismatch" >&2
	exit 1
}
if test "${1-}" = --check; then
	echo "MAKOS_CPYTHON_SOURCE_OK version=$CPYTHON_VERSION sha256=$actual"
	exit 0
fi
if test ! -f "$source_dir/configure"; then
	mkdir -p "$source_dir"
	tar -xJf "$archive" --strip-components=1 -C "$source_dir"
fi
echo "MAKOS_CPYTHON_SOURCE_OK version=$CPYTHON_VERSION sha256=$actual source=$source_dir"
