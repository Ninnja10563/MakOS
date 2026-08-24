#!/bin/sh
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. https://mozilla.org/MPL/2.0/
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
source_dir="$repo_dir/build/ports/firefox/source/third_party/rust/nss-gk-api"
stage_dir="$repo_dir/build/ports/firefox/nss-gk-api-makos"
patch_file="$port_dir/rust-patches/nss-gk-api-0.3.0-makos.patch"

test -f "$source_dir/Cargo.toml"
grep -Eq '^version = "0\.3\.0"$' "$source_dir/Cargo.toml" || {
    echo "Firefox MakOS nss-gk-api stage blocked: expected 0.3.0." >&2
    exit 1
}

if test -f "$stage_dir/Cargo.toml" && \
    patch -s -R --dry-run -d "$stage_dir" -p1 < "$patch_file"; then
    printf '%s\n' 'MAKOS_FIREFOX_NSS_GK_OK version=0.3.0 bindgen=libclang18 libcxx22_compat=1 checksum-safe=1'
    exit 0
fi

mkdir -p "$stage_dir"
rsync -a --delete "$source_dir/" "$stage_dir/"
patch -s -d "$stage_dir" -p1 < "$patch_file"
printf '%s\n' 'MAKOS_FIREFOX_NSS_GK_OK version=0.3.0 bindgen=libclang18 libcxx22_compat=1 checksum-safe=1'
