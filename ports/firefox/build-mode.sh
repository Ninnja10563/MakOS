#!/bin/sh
# Select an isolated Firefox object directory and fail closed on provenance.
set -eu

test "$#" -eq 1 || {
    echo "usage: $0 REPOSITORY_ROOT" >&2
    exit 2
}
repo_dir=$1
case "$repo_dir" in
    /*) ;;
    *)
        echo "Firefox MakOS build blocked: repository root must be absolute." >&2
        exit 2
        ;;
esac
case "${MAKOS_FIREFOX_DEVELOPER_BUILD:-0}" in
    0)
        obj="$repo_dir/build/ports/firefox/obj-aarch64-makos"
        ;;
    1)
        obj="$repo_dir/build/ports/firefox/obj-aarch64-makos-developer"
        # A developer artifact can never authorize release packaging, even if
        # this separate directory was restored from an older build cache.
        rm -f "$obj/makos-build-provenance.json"
        ;;
    *)
        echo "Firefox MakOS build blocked: MAKOS_FIREFOX_DEVELOPER_BUILD must be 0 or 1." >&2
        exit 1
        ;;
esac
printf '%s\n' "$obj"
