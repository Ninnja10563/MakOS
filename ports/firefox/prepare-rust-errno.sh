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

# Reconstruct and validate the complete patched candidate every time. Reverse
# patch applicability alone cannot detect unrelated drift in the staged crate.
# Do not expose an unpatched or partially patched crate on preparation failure.
private_dir=$(mktemp -d "${TMPDIR:-/tmp}/makos-firefox-errno-stage.XXXXXXXX")
trap 'rm -rf -- "$private_dir"' EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
# rsync copies the source root's permissions too; retain a separate 0700 parent.
candidate="$private_dir/candidate"
mkdir "$candidate"
rsync -a --delete --checksum "$source_dir/" "$candidate/"
patch -s -d "$candidate" -p1 < "$patch_file"
find "$candidate" -type f -name '*.orig' -delete

mkdir -p "$stage_dir"
# Archive semantics except timestamps: checksum comparison retains identical
# destination files (including their inode/mtime), even if upstream timestamps
# changed. Changed bytes are written with a fresh mtime, invalidating consumers.
# Publish the final patched tree, never its intermediate unpatched inputs.
# This is not a transactional directory swap: publication I/O failure is fatal
# and may leave a partially updated stage, as with the previous rsync path.
rsync -rlpgoD --delete --checksum "$candidate/" "$stage_dir/"
printf '%s\n' 'MAKOS_FIREFOX_ERRNO_OK version=0.3.8 accessor=__errno_location tls=musl checksum-safe=1'
