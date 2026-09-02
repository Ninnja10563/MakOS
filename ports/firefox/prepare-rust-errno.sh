#!/bin/sh
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. https://mozilla.org/MPL/2.0/
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
source_dir=${MAKOS_FIREFOX_ERRNO_SOURCE_DIR:-"$repo_dir/build/ports/firefox/source/third_party/rust/errno"}
stage_dir=${MAKOS_FIREFOX_ERRNO_STAGE_DIR:-"$repo_dir/build/ports/firefox/errno-makos"}
patch_file=${MAKOS_FIREFOX_ERRNO_PATCH_FILE:-"$port_dir/rust-patches/errno-0.3.8-makos.patch"}

test -f "$source_dir/Cargo.toml"
crate_version=$(sed -n 's/^version = "\([^"]*\)"$/\1/p' "$source_dir/Cargo.toml" | sed -n '1p')
test "$crate_version" = 0.3.8 || {
    echo "Firefox MakOS errno stage blocked: expected errno 0.3.8." >&2
    exit 1
}

mkdir -p "$stage_dir"
rsync -a --delete --checksum "$source_dir/" "$stage_dir/"
patch -s -d "$stage_dir" -p1 < "$patch_file"
find "$stage_dir" -type f -name '*.orig' -delete
printf '%s\n' 'MAKOS_FIREFOX_ERRNO_OK version=0.3.8 accessor=__errno_location tls=musl checksum-safe=1'
