#!/bin/sh
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. https://mozilla.org/MPL/2.0/
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
source_dir="$repo_dir/build/ports/firefox/source/third_party/rust/getrandom"
stage_dir="$repo_dir/build/ports/firefox/getrandom-makos"
patch_file="$port_dir/rust-patches/getrandom-0.3.3-makos.patch"

test -f "$source_dir/Cargo.toml"
grep -Eq '^version = "0\.3\.3"$' "$source_dir/Cargo.toml" || {
    echo "Firefox MakOS getrandom stage blocked: expected getrandom 0.3.3." >&2
    exit 1
}

if test -f "$stage_dir/Cargo.toml" && \
    patch -s -R --dry-run -d "$stage_dir" -p1 < "$patch_file"; then
    printf '%s\n' 'MAKOS_FIREFOX_GETRANDOM_OK version=0.3.3 backend=musl-getrandom entropy=virtio-rng'
    exit 0
fi

mkdir -p "$stage_dir"
rsync -a --delete "$source_dir/" "$stage_dir/"
patch -s -d "$stage_dir" -p1 < "$patch_file"
printf '%s\n' 'MAKOS_FIREFOX_GETRANDOM_OK version=0.3.3 backend=musl-getrandom entropy=virtio-rng'
