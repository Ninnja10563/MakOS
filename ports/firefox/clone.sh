#!/bin/sh
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. https://mozilla.org/MPL/2.0/
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
. "$port_dir/source.lock"

if test "${1-}" = "--check"; then
    remote=$(git ls-remote "$FIREFOX_REPOSITORY" "refs/tags/$FIREFOX_TAG" |
        awk 'NR == 1 { print $1 }')
    test "$remote" = "$FIREFOX_COMMIT" || {
        echo "firefox clone: pinned tag changed or unavailable" >&2
        echo "expected $FIREFOX_COMMIT" >&2
        echo "actual   ${remote:-missing}" >&2
        exit 1
    }
    echo "MAKOS_FIREFOX_REPOSITORY_OK tag=$FIREFOX_TAG commit=$FIREFOX_COMMIT"
    exit 0
fi

source_dir="$repo_dir/build/ports/firefox/source"
mkdir -p "$source_dir"
if test ! -d "$source_dir/.git"; then
    git -C "$source_dir" init
    git -C "$source_dir" remote add origin "$FIREFOX_REPOSITORY"
fi

origin=$(git -C "$source_dir" remote get-url origin)
test "$origin" = "$FIREFOX_REPOSITORY" || {
    echo "firefox clone: unexpected origin $origin" >&2
    exit 1
}

remote=$(git ls-remote "$FIREFOX_REPOSITORY" "refs/tags/$FIREFOX_TAG" |
    awk 'NR == 1 { print $1 }')
test "$remote" = "$FIREFOX_COMMIT" || {
    echo "firefox clone: pinned tag changed or unavailable" >&2
    exit 1
}

if git -C "$source_dir" cat-file -e "$FIREFOX_COMMIT^{commit}" 2>/dev/null; then
    actual=$FIREFOX_COMMIT
else
    git -C "$source_dir" fetch --depth=1 origin "refs/tags/$FIREFOX_TAG"
    actual=$(git -C "$source_dir" rev-parse FETCH_HEAD)
fi
test "$actual" = "$FIREFOX_COMMIT" || {
    echo "firefox clone: commit mismatch after fetch" >&2
    exit 1
}
git -C "$source_dir" update-ref "refs/tags/$FIREFOX_TAG" "$FIREFOX_COMMIT"
git -C "$source_dir" checkout --detach "$FIREFOX_COMMIT"
printf '%s\n' "$source_dir"
