#!/bin/sh
set -eu

PORT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
ROOT_DIR=$(CDPATH= cd -- "$PORT_DIR/../.." && pwd)
DEST=${1:-"$ROOT_DIR/build/ports/micropython"}
mkdir -p "$DEST"
DEST=$(CDPATH= cd -- "$DEST" && pwd)
SOURCE=${MAKOS_MICROPYTHON_SOURCE:-}
if [ -z "$SOURCE" ]; then
    SOURCE=$($PORT_DIR/fetch.sh "$DEST")
fi
if [ ! -f "$SOURCE/py/py.mk" ]; then
    echo "invalid MicroPython source: $SOURCE" >&2
    exit 1
fi

TARGET_PORT="$SOURCE/ports/makos"
rm -rf -- "$TARGET_PORT"
cp -R "$PORT_DIR/makos" "$TARGET_PORT"

RUST_SYSROOT=$(rustc --print sysroot)
HOST=$(rustc -vV | awk '/^host:/ {print $2}')
MAKOS_LD=${MAKOS_LD:-"$RUST_SYSROOT/lib/rustlib/$HOST/bin/rust-lld"}
if [ ! -x "$MAKOS_LD" ]; then
    echo "rust-lld unavailable: $MAKOS_LD" >&2
    exit 1
fi
BUILD="$DEST/build-makos"
make -C "$TARGET_PORT" \
    BUILD="$BUILD" \
    MAKOS_CC="${MAKOS_CC:-clang}" \
    MAKOS_LD="$MAKOS_LD" \
    V=${V:-0}
cp "$BUILD/micropython-makos.elf" "$DEST/micropython-makos.elf"
printf '%s\n' "$DEST/micropython-makos.elf"
