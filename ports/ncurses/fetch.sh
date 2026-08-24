#!/bin/sh
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
. "$port_dir/source.lock"
dist_dir="$repo_dir/build/ports/ncurses/distfiles"
archive="$dist_dir/$NCURSES_ARCHIVE"
mkdir -p "$dist_dir"

verify() {
    actual=$(shasum -a 256 "$1" | awk '{print $1}')
    test "$actual" = "$NCURSES_SHA256" || {
        echo "ncurses fetch: SHA-256 mismatch: $1" >&2
        return 1
    }
}

if test ! -f "$archive" || ! verify "$archive"; then
    temporary=$(mktemp "$dist_dir/$NCURSES_ARCHIVE.download.XXXXXX")
    trap 'rm -f "$temporary"' EXIT HUP INT TERM
    curl -fL --proto '=https' --tlsv1.2 -o "$temporary" "$NCURSES_URL"
    verify "$temporary"
    mv "$temporary" "$archive"
    chmod 0644 "$archive"
    trap - EXIT HUP INT TERM
fi
verify "$archive"
printf '%s\n' "$archive"
