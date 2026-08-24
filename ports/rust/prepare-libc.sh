#!/bin/sh
# SPDX-License-Identifier: Apache-2.0 OR MIT
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
stage_dir="$repo_dir/build/ports/rust/libc-makos"
source_dir=$(find "${CARGO_HOME:-$HOME/.cargo}/registry/src" -type d -name libc-0.2.189 -print | head -n 1)

test -n "$source_dir" || {
    echo "Rust libc 0.2.189 source absent; run build-std once to fetch it." >&2
    exit 1
}
mkdir -p "$stage_dir"
rsync -a --delete "$source_dir/" "$stage_dir/"
patch -d "$stage_dir" -p1 <"$port_dir/patches/0001-libc-makos-musl.patch"

# MakOS uses musl's public ABI. Route cfg-gated declarations through libc's
# existing Linux-like/musl modules while preserving target_os="makos".
for relative in \
    src/new/common/posix/pthread.rs \
    src/new/common/linux_like/mod.rs \
    src/macros.rs \
    src/types.rs
do
    perl -0pi -e 's/target_os = "linux"/any(target_os = "linux", target_os = "makos")/g' \
        "$stage_dir/$relative"
done

echo "MAKOS_RUST_LIBC_PATCH_OK version=0.2.189 target_os=makos env=musl linux_target_alias=0 musl_abi_routes=1"
