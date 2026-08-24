#!/bin/sh
set -eu

PORT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
ROOT_DIR=$(CDPATH= cd -- "$PORT_DIR/../.." && pwd)
DEST=${1:-"$ROOT_DIR/build/ports/micropython"}
ARCHIVE="$DEST/micropython-1.28.0.tar.xz"
SOURCE="$DEST/micropython-1.28.0"
URL=https://github.com/micropython/micropython/releases/download/v1.28.0/micropython-1.28.0.tar.xz
SHA256=4e43c59657b8da33b4bc503509a827cc3ea6cb66c446475c57776cf4467ba215

mkdir -p "$DEST"
if [ ! -f "$ARCHIVE" ]; then
    curl --fail --location --output "$ARCHIVE.part" "$URL"
    mv "$ARCHIVE.part" "$ARCHIVE"
fi
ACTUAL=$(shasum -a 256 "$ARCHIVE" | awk '{print $1}')
if [ "$ACTUAL" != "$SHA256" ]; then
    echo "MicroPython archive checksum mismatch: $ACTUAL" >&2
    exit 1
fi
if [ ! -d "$SOURCE" ]; then
    tar -xJf "$ARCHIVE" -C "$DEST"
fi
printf '%s\n' "$SOURCE"
