#!/bin/sh
# SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
. "$port_dir/source.lock"

remote=$(git ls-remote "$LLVM_REPOSITORY" "refs/tags/$LLVM_TAG^{}" |
    awk 'NR == 1 { print $1 }')
test "$remote" = "$LLVM_COMMIT" || {
    echo "LLVM clone: pinned tag changed or unavailable" >&2
    exit 1
}
if test "${1-}" = "--check"; then
    echo "MAKOS_LLVM_REPOSITORY_OK tag=$LLVM_TAG commit=$LLVM_COMMIT"
    exit 0
fi

source_dir=${LLVM_SOURCE_DIR:-"$repo_dir/build/ports/libcxx/source"}
if test ! -d "$source_dir/.git"; then
    mkdir -p "$source_dir"
    git -C "$source_dir" init
    git -C "$source_dir" remote add origin "$LLVM_REPOSITORY"
fi
origin=$(git -C "$source_dir" remote get-url origin)
test "$origin" = "$LLVM_REPOSITORY" || {
    echo "LLVM clone: unexpected origin $origin" >&2
    exit 1
}
if ! git -C "$source_dir" cat-file -e "$LLVM_COMMIT^{commit}" 2>/dev/null; then
    git -C "$source_dir" fetch --depth=1 origin "refs/tags/$LLVM_TAG"
fi
actual=$(git -C "$source_dir" rev-parse "$LLVM_COMMIT^{commit}")
test "$actual" = "$LLVM_COMMIT" || {
    echo "LLVM clone: fetched commit mismatch" >&2
    exit 1
}
git -C "$source_dir" checkout --detach "$LLVM_COMMIT"
printf '%s\n' "$source_dir"
