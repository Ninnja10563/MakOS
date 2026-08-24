#!/bin/sh
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
. "$port_dir/source.lock"
archive=$($port_dir/fetch.sh)

work_root="$repo_dir/build/ports/nano"
source_dir="$work_root/source-$NANO_VERSION"
build_dir="$work_root/host"
lock_dir="$work_root/.host-build.lock"

mkdir -p "$work_root"
if ! mkdir "$lock_dir" 2>/dev/null; then
    echo "nano host build already running: $lock_dir" >&2
    exit 75
fi
trap 'rmdir "$lock_dir"' EXIT HUP INT TERM

# Targets are fixed descendants of build/ports/nano, never user-selected paths.
rm -rf "$source_dir" "$build_dir"
mkdir -p "$source_dir" "$build_dir"
tar -xJf "$archive" -C "$source_dir" --strip-components=1

case $(uname -s) in
    Darwin) jobs=$(sysctl -n hw.logicalcpu 2>/dev/null || printf '4') ;;
    *) jobs=$(getconf _NPROCESSORS_ONLN 2>/dev/null || printf '4') ;;
esac

(cd "$build_dir" && "$source_dir/configure" \
        --srcdir="$source_dir" \
        --disable-nls \
        --disable-libmagic \
        --prefix=/usr \
        >"$build_dir/configure.log")
make -C "$build_dir" -j"$jobs" >"$build_dir/build.log"

binary="$build_dir/src/nano"
test -x "$binary"
version=$($binary --version | sed -n '1p')
case "$version" in
    *"GNU nano, version $NANO_VERSION"*) ;;
    *) echo "nano host build: wrong binary identity: $version" >&2; exit 1 ;;
esac

cp "$source_dir/COPYING" "$build_dir/COPYING"
cp "$source_dir/COPYING.DOC" "$build_dir/COPYING.DOC"
printf '%s\n' "$binary"
