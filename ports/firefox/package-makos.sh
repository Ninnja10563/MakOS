#!/bin/sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
OBJ=${MAKOS_FIREFOX_OBJ:-$ROOT/build/ports/firefox/obj-aarch64-makos}
DIST=${MAKOS_FIREFOX_DIST:-$OBJ/dist/firefox}
STRIPPED=${MAKOS_FIREFOX_LIBXUL:-$ROOT/build/ports/firefox/package-aarch64-makos/libxul.so}
IMAGE=${1:-$ROOT/build/makos-data-aarch64.img}
FONT_SOURCE=$ROOT/build/ports/firefox/source/layout/reftests/fonts/mplus/mplus-1p-regular.ttf
FONT_LICENSE=$ROOT/build/ports/firefox/source/layout/reftests/fonts/mplus/mplus-license.txt
FIREFOX_LICENSE=$ROOT/build/ports/firefox/source/LICENSE
FIREFOX_LICENSE_HTML=$ROOT/build/ports/firefox/source/toolkit/content/license.html
MAKOS_PREFS=$ROOT/ports/firefox/makos-prefs.js
LLVM_STRIP=${MAKOS_LLVM_STRIP:-/opt/homebrew/opt/llvm/bin/llvm-strip}
NANO_BINARY=$ROOT/build/ports/nano/makos/src/nano
NANO_TERMINFO=$ROOT/build/ports/ncurses/stage/usr/share/terminfo/m/makos
NANO_LICENSE=$ROOT/build/ports/nano/makos/COPYING
NCURSES_LICENSE=$ROOT/build/ports/ncurses/stage/COPYING.ncurses
CPYTHON_BINARY=$ROOT/build/ports/cpython/package/usr/bin/python3
CPYTHON_STDLIB=$ROOT/build/ports/cpython/package/usr/lib/python314.zip
CPYTHON_LICENSE=$ROOT/build/ports/cpython/package/usr/share/licenses/cpython/LICENSE

# Keep the final system image reproducible from a clean tree: stage the real
# CPython runtime before deciding whether its payload is available.
"$ROOT/ports/cpython/stage-makos.sh"

# Always refresh Mozilla's packaged runtime after incremental builds. Gecko's
# build output in dist/bin can otherwise be newer than dist/firefox.
make -C "$OBJ/browser/installer" stage-package

if [ ! -x "$DIST/firefox" ] || [ ! -f "$DIST/omni.ja" ]; then
    echo "Mozilla stage-package output absent after refresh: $DIST" >&2
    exit 2
fi
if [ ! -f "$DIST/libxul.so" ]; then
    echo "Built libxul absent: $DIST/libxul.so" >&2
    exit 2
fi
if [ ! -x "$LLVM_STRIP" ]; then
    echo "LLVM strip absent: $LLVM_STRIP" >&2
    exit 2
fi
if [ ! -f "$FONT_SOURCE" ] || [ ! -f "$FONT_LICENSE" ] || \
   [ ! -f "$FIREFOX_LICENSE" ] || [ ! -f "$FIREFOX_LICENSE_HTML" ] || \
   [ ! -f "$MAKOS_PREFS" ]; then
    echo "Bundled open-source system font absent" >&2
    exit 2
fi

mkdir -p "$DIST/fonts"
mkdir -p "$DIST/defaults/pref"
mkdir -p "$DIST/licenses"
cp "$FONT_SOURCE" "$DIST/fonts/MakOSSystem-Regular.ttf"
cp "$FONT_LICENSE" "$DIST/fonts/LICENSE-MPLUS.txt"
cp "$FIREFOX_LICENSE" "$DIST/licenses/LICENSE"
cp "$FIREFOX_LICENSE_HTML" "$DIST/licenses/license.html"
cp "$MAKOS_PREFS" "$DIST/defaults/pref/makos.js"

# Mozilla's stage-package invokes macOS /usr/bin/strip, which cannot process
# AArch64 ELF and leaves debug/comment sections directly after some PT_LOAD
# file ranges. MakOS demand paging must see EOF there so ELF BSS remains zero
# after DONTNEED/refault. Remove section headers and all non-segment data from
# every packaged target ELF; libxul is replaced by its audited artifact.
find "$DIST" -type f | while IFS= read -r binary; do
    [ "$(basename "$binary")" = libxul.so ] && continue
    if file "$binary" | grep -q 'ELF 64-bit LSB'; then
        "$LLVM_STRIP" --strip-sections "$binary"
    fi
done

# Never reuse an earlier stripped libxul: doing so silently packages stale
# Gecko code after an incremental build. Regenerate atomically from dist.
mkdir -p "$(dirname "$STRIPPED")"
stripped_tmp=$(mktemp "$STRIPPED.XXXXXX")
trap 'rm -f "$stripped_tmp"' EXIT HUP INT TERM
cp "$DIST/libxul.so" "$stripped_tmp"
"$LLVM_STRIP" --strip-sections "$stripped_tmp"
mv "$stripped_tmp" "$STRIPPED"
trap - EXIT HUP INT TERM

set -- "$ROOT/scripts/mkpackage.py" \
    "$IMAGE" "$DIST" \
    --prefix usr/lib/firefox \
    --replace "libxul.so=$STRIPPED" \
    --add "/fonts/MakOSSystem-Regular.ttf=$FONT_SOURCE" \
    --add "/fonts/LICENSE-MPLUS.txt=$FONT_LICENSE"
NANO_STATUS=absent
if [ -f "$NANO_BINARY" ] && [ -f "$NANO_TERMINFO" ] && \
   [ -f "$NANO_LICENSE" ] && [ -f "$NCURSES_LICENSE" ]; then
    set -- "$@" \
        --add "/usr/bin/nano=$NANO_BINARY" \
        --add "/usr/share/terminfo/m/makos=$NANO_TERMINFO" \
        --add "/usr/share/licenses/nano/COPYING=$NANO_LICENSE" \
        --add "/usr/share/licenses/ncurses/COPYING=$NCURSES_LICENSE"
    NANO_STATUS=included
fi
CPYTHON_STATUS=absent
if [ -f "$CPYTHON_BINARY" ] && [ -f "$CPYTHON_STDLIB" ] && \
   [ -f "$CPYTHON_LICENSE" ]; then
    set -- "$@" \
        --add "/usr/bin/python3=$CPYTHON_BINARY" \
        --add "/usr/lib/python314.zip=$CPYTHON_STDLIB" \
        --add "/usr/share/licenses/cpython/LICENSE=$CPYTHON_LICENSE"
    CPYTHON_STATUS=included
fi
python3 "$@"
python3 "$ROOT/scripts/verify_package.py" "$IMAGE"

echo "MAKOS_FIREFOX_PACKAGE_OK image=$IMAGE exec=/usr/lib/firefox/firefox libxul=stripped font=mplus nano=$NANO_STATUS cpython=$CPYTHON_STATUS"
