#!/bin/sh
# SPDX-License-Identifier: MIT
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
. "$port_dir/source.lock"

remote=$(git ls-remote "$MUSL_REPOSITORY" "refs/tags/$MUSL_TAG" |
    awk 'NR == 1 { print $1 }')
test "$remote" = "$MUSL_COMMIT" || {
    echo "musl clone: pinned tag changed or unavailable" >&2
    exit 1
}
if test "${1-}" = "--check"; then
    echo "MAKOS_MUSL_REPOSITORY_OK tag=$MUSL_TAG commit=$MUSL_COMMIT"
    exit 0
fi

source_dir="$repo_dir/build/ports/musl/source"
if test ! -d "$source_dir/.git"; then
    mkdir -p "$source_dir"
    git -C "$source_dir" init
    git -C "$source_dir" remote add origin "$MUSL_REPOSITORY"
fi
origin=$(git -C "$source_dir" remote get-url origin)
test "$origin" = "$MUSL_REPOSITORY" || {
    echo "musl clone: unexpected origin $origin" >&2
    exit 1
}

if ! git -C "$source_dir" cat-file -e "$MUSL_COMMIT^{commit}" 2>/dev/null; then
    git -C "$source_dir" fetch --depth=1 origin "refs/tags/$MUSL_TAG"
fi
actual=$(git -C "$source_dir" rev-parse "$MUSL_COMMIT^{commit}")
test "$actual" = "$MUSL_COMMIT" || {
    echo "musl clone: fetched commit mismatch" >&2
    exit 1
}
git -C "$source_dir" update-ref "refs/tags/$MUSL_TAG" "$MUSL_COMMIT"
git -C "$source_dir" checkout --detach "$MUSL_COMMIT"
printf '%s\n' "$source_dir"
