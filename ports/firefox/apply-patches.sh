#!/bin/sh
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. https://mozilla.org/MPL/2.0/
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
source_dir="$repo_dir/build/ports/firefox/source"

test -d "$source_dir/.git" || {
    echo "Firefox source missing; run ports/firefox/clone.sh" >&2
    exit 1
}

git_dir=$(git -C "$source_dir" rev-parse --git-dir)
case "$git_dir" in
    /*) ;;
    *) git_dir="$source_dir/$git_dir" ;;
esac
series_hash=$(
    for patch in "$port_dir"/patches/*.patch; do
        shasum -a 256 "$patch" | awk '{print $1}'
    done | shasum -a 256 | awk '{print $1}'
)
marker="$git_dir/makos-patches.sha256"
if test -f "$marker" && test "$(sed -n '1p' "$marker")" = "$series_hash"; then
    python3 "$repo_dir/scripts/firefox_provenance.py" verify-source \
        --source-dir "$source_dir" >/dev/null
    echo "MAKOS_FIREFOX_PATCHES_OK target=MakOS toolkit=makos nspr=MakOS rust_target=MakOS linux_masquerade=0"
    exit 0
fi

recorded_hash=
if test -f "$marker"; then
    recorded_hash=$(sed -n '1p' "$marker")
fi
prefix_file=$(mktemp "${TMPDIR:-/tmp}/makos-firefox-patches.XXXXXX")
trap 'rm -f "$prefix_file"' EXIT HUP INT TERM
past_recorded=false

for patch in "$port_dir"/patches/*.patch; do
    patch_hash=$(shasum -a 256 "$patch" | awk '{print $1}')
    printf '%s\n' "$patch_hash" >>"$prefix_file"
    prefix_hash=$(shasum -a 256 "$prefix_file" | awk '{print $1}')
    if test -n "$recorded_hash" && test "$past_recorded" = false; then
        if test "$prefix_hash" = "$recorded_hash"; then
            past_recorded=true
        fi
        continue
    fi
    if git -C "$source_dir" apply --reverse --check "$patch" >/dev/null 2>&1; then
        continue
    fi
    git -C "$source_dir" apply --check "$patch"
    git -C "$source_dir" apply "$patch"
done

if test -n "$recorded_hash" && test "$past_recorded" = false; then
    echo "Firefox patch series changed before recorded suffix; use a clean source checkout" >&2
    exit 1
fi

printf '%s\n' "$series_hash" >"$marker"
python3 "$repo_dir/scripts/firefox_provenance.py" verify-source \
    --source-dir "$source_dir" >/dev/null

echo "MAKOS_FIREFOX_PATCHES_OK target=MakOS toolkit=makos nspr=MakOS rust_target=MakOS linux_masquerade=0"
