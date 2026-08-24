#!/bin/sh
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
. "$port_dir/source.lock"

binary=$($port_dir/build-makos.sh | tail -1)
test -x "$binary"
file "$binary" | grep -E 'ELF 64-bit.*pie executable.*ARM aarch64' >/dev/null
readelf=${MAKOS_NANO_READELF:-/opt/homebrew/opt/llvm/bin/llvm-readelf}
audit=$($readelf -h -l -d "$binary")
printf '%s\n' "$audit" | grep -F 'Type:                              DYN' >/dev/null
printf '%s\n' "$audit" | grep -F '/lib/ld-musl-aarch64.so.1' >/dev/null
printf '%s\n' "$audit" | grep -F 'Shared library: [libc.so]' >/dev/null
strings "$binary" | grep -F "GNU nano $NANO_VERSION" >/dev/null

source_dir="$repo_dir/build/ports/nano/makos-source-$NANO_VERSION"
grep -F "AC_INIT([GNU nano], [$NANO_VERSION]" "$source_dir/configure.ac" >/dev/null
grep -F 'GNU GENERAL PUBLIC LICENSE' "$source_dir/COPYING" >/dev/null
test -s "$repo_dir/build/ports/ncurses/stage/usr/lib/libncurses.a"
file "$repo_dir/build/ports/ncurses/stage/usr/share/terminfo/m/makos" |
    grep -F 'Compiled terminfo entry' >/dev/null

tmp_dir=$(mktemp -d "${TMPDIR:-/tmp}/makos-nano-test.XXXXXX")
trap 'rm -rf "$tmp_dir"' EXIT HUP INT TERM
$port_dir/package-makos.sh "$tmp_dir/nano.img" >/dev/null
python3 "$repo_dir/scripts/verify_package.py" "$tmp_dir/nano.img" >/dev/null

printf '%s\n' \
    "GNU_NANO_PORT_TEST_OK source=official version=$NANO_VERSION target=aarch64-unknown-makos pie=1 ncurses=6.5 package=verified fake=0 runtime_test=scripts/boot_test_aarch64_nano.py"
