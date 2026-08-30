#!/bin/sh
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT HUP INT TERM

release_obj="$tmp/build/ports/firefox/obj-aarch64-makos"
developer_obj="$tmp/build/ports/firefox/obj-aarch64-makos-developer"
mkdir -p "$release_obj" "$developer_obj"
printf 'release\n' > "$release_obj/makos-build-provenance.json"
mkdir -p "$release_obj/dist/firefox"
printf 'release-runtime\n' > "$release_obj/dist/firefox/makos-build-provenance.json"
printf 'stale\n' > "$developer_obj/makos-build-provenance.json"
mkdir -p "$developer_obj/dist/firefox"
printf 'stale-runtime\n' > "$developer_obj/dist/firefox/makos-build-provenance.json"

actual=$(MAKOS_FIREFOX_DEVELOPER_BUILD=0 "$port_dir/build-mode.sh" "$tmp")
test "$actual" = "$release_obj"
grep -Fxq release "$release_obj/makos-build-provenance.json"
grep -Fxq release-runtime \
    "$release_obj/dist/firefox/makos-build-provenance.json"

release_moz=$(MAKOS_FIREFOX_DEVELOPER_BUILD=0 sh -c '
    mk_add_options() { printf "mk:%s\n" "$*"; }
    ac_add_options() { printf "ac:%s\n" "$*"; }
    MAKOS_SYSROOT=/makos-test-sysroot
    . "$1"
' makos-test "$port_dir/mozconfig.makos")
printf '%s\n' "$release_moz" | \
    grep -Fxq 'mk:MOZ_OBJDIR=@TOPSRCDIR@/../obj-aarch64-makos'
if printf '%s\n' "$release_moz" | \
    grep -Eq '^ac:--disable-(release|debug-symbols|optimize)$'
then
    echo "Firefox release mozconfig selected developer options" >&2
    exit 1
fi

actual=$(MAKOS_FIREFOX_DEVELOPER_BUILD=1 "$port_dir/build-mode.sh" "$tmp")
test "$actual" = "$developer_obj"
test ! -e "$developer_obj/makos-build-provenance.json"
test ! -e "$developer_obj/dist/firefox/makos-build-provenance.json"
grep -Fxq release "$release_obj/makos-build-provenance.json"
grep -Fxq release-runtime \
    "$release_obj/dist/firefox/makos-build-provenance.json"
developer_moz=$(MAKOS_FIREFOX_DEVELOPER_BUILD=1 sh -c '
    mk_add_options() { printf "mk:%s\n" "$*"; }
    ac_add_options() { printf "ac:%s\n" "$*"; }
    MAKOS_SYSROOT=/makos-test-sysroot
    . "$1"
' makos-test "$port_dir/mozconfig.makos")
printf '%s\n' "$developer_moz" | \
    grep -Fxq 'mk:MOZ_OBJDIR=@TOPSRCDIR@/../obj-aarch64-makos-developer'
for option in --disable-release --disable-debug-symbols --disable-optimize; do
    printf '%s\n' "$developer_moz" | grep -Fxq "ac:$option"
done

# Refuse developer selection if either stale record cannot be removed. A
# directory at the runtime-record path models a malformed or hostile cache.
mkdir -p "$developer_obj/dist/firefox/makos-build-provenance.json"
if MAKOS_FIREFOX_DEVELOPER_BUILD=1 \
    "$port_dir/build-mode.sh" "$tmp" >"$tmp/refusal.stdout" \
    2>"$tmp/refusal.stderr"
then
    echo "Firefox developer build accepted unremovable staged provenance" >&2
    exit 1
fi
test ! -s "$tmp/refusal.stdout"
grep -Fq 'cannot remove stale provenance' "$tmp/refusal.stderr"
rmdir "$developer_obj/dist/firefox/makos-build-provenance.json"

mkdir "$developer_obj/makos-build-provenance.json"
if MAKOS_FIREFOX_DEVELOPER_BUILD=1 \
    "$port_dir/build-mode.sh" "$tmp" >"$tmp/root-refusal.stdout" \
    2>"$tmp/root-refusal.stderr"
then
    echo "Firefox developer build accepted unremovable root provenance" >&2
    exit 1
fi
test ! -s "$tmp/root-refusal.stdout"
grep -Fq 'cannot remove stale provenance' "$tmp/root-refusal.stderr"
rmdir "$developer_obj/makos-build-provenance.json"

if MAKOS_FIREFOX_DEVELOPER_BUILD=invalid \
    "$port_dir/build-mode.sh" "$tmp" >"$tmp/invalid.stdout" 2>"$tmp/invalid.stderr"
then
    echo "Firefox build mode accepted an invalid value" >&2
    exit 1
fi
test ! -s "$tmp/invalid.stdout"
grep -Fq 'MAKOS_FIREFOX_DEVELOPER_BUILD must be 0 or 1' "$tmp/invalid.stderr"

echo "MAKOS_FIREFOX_BUILD_MODE_TEST_OK release=isolated developer=isolated provenance=root-and-staged-withheld unremovable=refused invalid=rejected"
