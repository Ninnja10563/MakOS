#!/bin/sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
RELEASE_OBJ=$ROOT/build/ports/firefox/obj-aarch64-makos
RELEASE_DIST=$RELEASE_OBJ/dist/firefox
RELEASE_BIN=$RELEASE_OBJ/dist/bin
RELEASE_BUILD_PROVENANCE=$RELEASE_OBJ/makos-build-provenance.json
OBJ=${MAKOS_FIREFOX_OBJ:-$RELEASE_OBJ}
DIST=${MAKOS_FIREFOX_DIST:-$RELEASE_DIST}
BIN=${MAKOS_FIREFOX_BIN_DIR:-$RELEASE_BIN}
BUILD_PROVENANCE=${MAKOS_FIREFOX_BUILD_PROVENANCE:-$RELEASE_BUILD_PROVENANCE}
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
STAMPED_ARTIFACTS='firefox plugin-container xpcshell libxul.so libnspr4.so'

# Release packaging is deliberately narrower than inspection/integration code:
# developer object directories and independently redirected build inputs can
# never qualify a runtime image. The libxul output itself may be redirected,
# but it is always replaced atomically with bytes derived below.
if [ "${MAKOS_FIREFOX_DEVELOPER_BUILD:-0}" != 0 ] || \
   [ "$OBJ" != "$RELEASE_OBJ" ] || [ "$DIST" != "$RELEASE_DIST" ] || \
   [ "$BIN" != "$RELEASE_BIN" ] || \
   [ "$BUILD_PROVENANCE" != "$RELEASE_BUILD_PROVENANCE" ]; then
    echo "Firefox release packaging rejects developer or redirected build inputs" >&2
    exit 2
fi

require_literal_canonical_path() {
    required=$1
    resolved=$(python3 -c \
        'import pathlib, sys; print(pathlib.Path(sys.argv[1]).resolve(strict=False))' \
        "$required") || {
        echo "Firefox release packaging cannot resolve required path: $required" >&2
        exit 2
    }
    if [ "$resolved" != "$required" ]; then
        echo "Firefox release packaging rejects noncanonical path: $required -> $resolved" >&2
        exit 2
    fi
}

verify_release_paths() {
    require_literal_canonical_path "$RELEASE_OBJ"
    require_literal_canonical_path "$RELEASE_DIST"
    require_literal_canonical_path "$RELEASE_BIN"
    require_literal_canonical_path "$RELEASE_BUILD_PROVENANCE"
    if [ ! -d "$RELEASE_OBJ" ] || [ ! -d "$RELEASE_DIST" ] || \
       [ ! -d "$RELEASE_BIN" ] || [ ! -f "$RELEASE_BUILD_PROVENANCE" ]; then
        echo "Firefox release packaging required path has wrong type" >&2
        exit 2
    fi
    for artifact in $STAMPED_ARTIFACTS; do
        if [ ! -f "$RELEASE_BIN/$artifact" ] || \
           [ -L "$RELEASE_BIN/$artifact" ]; then
            echo "Firefox release BIN artifact is absent or symlinked: $artifact" >&2
            exit 2
        fi
    done
}

verify_release_paths

canonical_output_path() {
    python3 -c \
        'import pathlib, sys; print(pathlib.Path(sys.argv[1]).resolve(strict=False))' \
        "$1"
}

paths_alias() {
    python3 -c '
import os
import pathlib
import sys
left, right = map(pathlib.Path, sys.argv[1:])
same = left.resolve(strict=False) == right.resolve(strict=False)
if not same and (left.exists() or left.is_symlink()) and (right.exists() or right.is_symlink()):
    try:
        same = os.path.samefile(left, right)
    except OSError:
        pass
raise SystemExit(0 if same else 1)
' "$1" "$2"
}

reject_output_alias() {
    label=$1
    output=$2
    shift 2
    case "$output" in
        "$RELEASE_OBJ"|"$RELEASE_OBJ"/*|\
        "$ROOT/build/ports/firefox/source"|"$ROOT/build/ports/firefox/source"/*)
            echo "Firefox package $label destination aliases protected Firefox tree: $output" >&2
            exit 2
            ;;
    esac
    for protected in "$@"; do
        if paths_alias "$output" "$protected"; then
            echo "Firefox package $label destination aliases protected input: $protected" >&2
            exit 2
        fi
    done
}

validate_output_paths() {
    if [ -L "$IMAGE" ] || { [ -e "$IMAGE" ] && [ ! -f "$IMAGE" ]; }; then
        echo "Firefox package image destination is symlinked or non-regular: $IMAGE" >&2
        exit 2
    fi
    if [ -L "$STRIPPED" ] || { [ -e "$STRIPPED" ] && [ ! -f "$STRIPPED" ]; }; then
        echo "Firefox stripped libxul destination is symlinked or non-regular: $STRIPPED" >&2
        exit 2
    fi
    IMAGE=$(canonical_output_path "$IMAGE") || {
        echo "Firefox package cannot resolve image destination" >&2
        exit 2
    }
    STRIPPED=$(canonical_output_path "$STRIPPED") || {
        echo "Firefox package cannot resolve stripped libxul destination" >&2
        exit 2
    }
    reject_output_alias IMAGE "$IMAGE" "$STRIPPED" "$BUILD_PROVENANCE" \
        "$FONT_SOURCE" "$FONT_LICENSE" "$FIREFOX_LICENSE" \
        "$FIREFOX_LICENSE_HTML" "$MAKOS_PREFS" "$NANO_BINARY" \
        "$NANO_TERMINFO" "$NANO_LICENSE" "$NCURSES_LICENSE" \
        "$CPYTHON_BINARY" "$CPYTHON_STDLIB" "$CPYTHON_LICENSE"
    reject_output_alias STRIPPED "$STRIPPED" "$IMAGE" \
        "$BUILD_PROVENANCE" "$FONT_SOURCE" "$FONT_LICENSE" \
        "$FIREFOX_LICENSE" "$FIREFOX_LICENSE_HTML" "$MAKOS_PREFS" \
        "$NANO_BINARY" "$NANO_TERMINFO" "$NANO_LICENSE" \
        "$NCURSES_LICENSE" "$CPYTHON_BINARY" "$CPYTHON_STDLIB" \
        "$CPYTHON_LICENSE"
    for artifact in $STAMPED_ARTIFACTS; do
        reject_output_alias IMAGE "$IMAGE" "$BIN/$artifact"
        reject_output_alias STRIPPED "$STRIPPED" "$BIN/$artifact"
    done
}

validate_output_paths

# Keep the final system image reproducible from a clean tree: stage the real
# CPython runtime before deciding whether its payload is available.
"$ROOT/ports/cpython/stage-makos.sh"

# Refuse stale incremental outputs. The build stamp is written only after a
# successful full mach build and binary audit, binds five unstripped artifacts
# to the pinned source commit and ordered patch series, and is reverified here
# before stage-package modifies distribution output.
python3 "$ROOT/scripts/firefox_provenance.py" verify-build-stamp \
    --source-dir "$ROOT/build/ports/firefox/source" \
    --bin-dir "$BIN" \
    --stamp "$BUILD_PROVENANCE"

# Keep an invocation-private copy of the exact five bytes just authorized by
# the stamp. The candidate package tree and all optional additions remain under
# this mode-0700 directory, so mkpackage never consumes mutable release DIST.
snapshot_dir=$(mktemp -d "${TMPDIR:-/tmp}/makos-firefox-package.XXXXXX")
artifact_snapshot=$snapshot_dir/stamped-bin
package_root=$snapshot_dir/package-root
additions=$snapshot_dir/additions
image_tmp=
stripped_tmp=
firefox_publish=
plugin_publish=
xpcshell_publish=
nspr_publish=
runtime_publish=
cleanup_package_temps() {
    rm -rf "$snapshot_dir"
    for temporary in "$image_tmp" "$stripped_tmp" "$firefox_publish" \
        "$plugin_publish" "$xpcshell_publish" "$nspr_publish" \
        "$runtime_publish"
    do
        if [ -n "$temporary" ]; then
            rm -f "$temporary"
        fi
    done
}
trap cleanup_package_temps EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
mkdir -p "$artifact_snapshot" "$package_root" "$additions"
for artifact in $STAMPED_ARTIFACTS; do
    cp "$BIN/$artifact" "$artifact_snapshot/$artifact"
done
python3 "$ROOT/scripts/firefox_provenance.py" verify-build-stamp \
    --source-dir "$ROOT/build/ports/firefox/source" \
    --bin-dir "$artifact_snapshot" \
    --stamp "$BUILD_PROVENANCE"

if [ ! -x "$LLVM_STRIP" ]; then
    echo "LLVM strip absent: $LLVM_STRIP" >&2
    exit 2
fi
for artifact in $STAMPED_ARTIFACTS; do
    "$LLVM_STRIP" --strip-sections "$artifact_snapshot/$artifact"
done

# Always refresh Mozilla's packaged runtime after incremental builds. Gecko's
# build output in dist/bin can otherwise be newer than dist/firefox.
make -C "$OBJ/browser/installer" stage-package
verify_release_paths

if [ ! -x "$DIST/firefox" ] || [ ! -f "$DIST/omni.ja" ]; then
    echo "Mozilla stage-package output absent after refresh: $DIST" >&2
    exit 2
fi
for artifact in $STAMPED_ARTIFACTS; do
    if [ ! -f "$DIST/$artifact" ]; then
        echo "Mozilla stage-package artifact absent: $DIST/$artifact" >&2
        exit 2
    fi
done
if [ -n "$(find "$DIST" -type l -print -quit)" ]; then
    echo "Mozilla stage-package output contains a symlink: $DIST" >&2
    exit 2
fi
if [ ! -f "$FONT_SOURCE" ] || [ ! -f "$FONT_LICENSE" ] || \
   [ ! -f "$FIREFOX_LICENSE" ] || [ ! -f "$FIREFOX_LICENSE_HTML" ] || \
   [ ! -f "$MAKOS_PREFS" ]; then
    echo "Bundled open-source system font absent" >&2
    exit 2
fi

# Preserve the existing integration-source contract while assembling the
# actual image candidate privately. These files are immutable project inputs;
# all files read by mkpackage below are private copies.
mkdir -p "$DIST/fonts" "$DIST/defaults/pref" "$DIST/licenses"
cp "$FONT_SOURCE" "$DIST/fonts/MakOSSystem-Regular.ttf"
cp "$FONT_LICENSE" "$DIST/fonts/LICENSE-MPLUS.txt"
cp "$FIREFOX_LICENSE" "$DIST/licenses/LICENSE"
cp "$FIREFOX_LICENSE_HTML" "$DIST/licenses/license.html"
cp "$MAKOS_PREFS" "$DIST/defaults/pref/makos.js"
cp -R "$DIST/." "$package_root/"
if [ -n "$(find "$package_root" -type l -print -quit)" ]; then
    echo "Private Firefox package candidate contains a symlink" >&2
    exit 2
fi

# Strip staged copies only inside the private tree and compare them directly
# with the independently stripped, stamp-authorized snapshots. Then overwrite
# all five candidate payloads from the snapshots, making their derivation—not
# mutable DIST—the actual source consumed by mkpackage.
for artifact in $STAMPED_ARTIFACTS; do
    "$LLVM_STRIP" --strip-sections "$package_root/$artifact"
    if ! cmp -s "$artifact_snapshot/$artifact" "$package_root/$artifact"; then
        echo "Firefox packaged artifact is not derived from stamped BIN: $artifact" >&2
        exit 2
    fi
    cp "$artifact_snapshot/$artifact" "$package_root/$artifact"
done

# Mozilla's stage-package invokes macOS /usr/bin/strip, which cannot process
# AArch64 ELF. Apply the same section-removal policy to every other ELF in the
# private candidate without modifying or later reading mutable DIST.
find "$package_root" -type f | while IFS= read -r binary; do
    case "$binary" in
        "$package_root/firefox"|"$package_root/plugin-container"|\
        "$package_root/xpcshell"|"$package_root/libxul.so"|\
        "$package_root/libnspr4.so") continue ;;
    esac
    if file "$binary" | grep -q 'ELF 64-bit LSB'; then
        "$LLVM_STRIP" --strip-sections "$binary"
    fi
done

# Bind runtime hashes to the private candidate. create-runtime-record also
# revalidates release BIN; repeat direct comparisons afterwards so neither
# metadata generation nor later DIST mutation can redirect the five payloads.
python3 "$ROOT/scripts/firefox_provenance.py" create-runtime-record \
    --source-dir "$ROOT/build/ports/firefox/source" \
    --bin-dir "$BIN" \
    --stamp "$BUILD_PROVENANCE" \
    --package-dir "$package_root" \
    --stripped-libxul "$package_root/libxul.so" \
    --output "$package_root/makos-build-provenance.json"
for artifact in $STAMPED_ARTIFACTS; do
    if ! cmp -s "$artifact_snapshot/$artifact" "$package_root/$artifact"; then
        echo "Private Firefox package candidate changed: $artifact" >&2
        exit 2
    fi
done

# Snapshot every additional package input before invoking mkpackage.
cp "$package_root/fonts/MakOSSystem-Regular.ttf" "$additions/MakOSSystem-Regular.ttf"
cp "$package_root/fonts/LICENSE-MPLUS.txt" "$additions/LICENSE-MPLUS.txt"
set -- "$ROOT/scripts/mkpackage.py" \
    PLACEHOLDER_IMAGE "$package_root" \
    --prefix usr/lib/firefox \
    --add "/fonts/MakOSSystem-Regular.ttf=$additions/MakOSSystem-Regular.ttf" \
    --add "/fonts/LICENSE-MPLUS.txt=$additions/LICENSE-MPLUS.txt"
NANO_STATUS=absent
if [ -f "$NANO_BINARY" ] && [ -f "$NANO_TERMINFO" ] && \
   [ -f "$NANO_LICENSE" ] && [ -f "$NCURSES_LICENSE" ]; then
    mkdir -p "$additions/nano"
    cp "$NANO_BINARY" "$additions/nano/nano"
    cp "$NANO_TERMINFO" "$additions/nano/makos"
    cp "$NANO_LICENSE" "$additions/nano/COPYING.nano"
    cp "$NCURSES_LICENSE" "$additions/nano/COPYING.ncurses"
    set -- "$@" \
        --add "/usr/bin/nano=$additions/nano/nano" \
        --add "/usr/share/terminfo/m/makos=$additions/nano/makos" \
        --add "/usr/share/licenses/nano/COPYING=$additions/nano/COPYING.nano" \
        --add "/usr/share/licenses/ncurses/COPYING=$additions/nano/COPYING.ncurses"
    NANO_STATUS=included
fi
CPYTHON_STATUS=absent
if [ -f "$CPYTHON_BINARY" ] && [ -f "$CPYTHON_STDLIB" ] && \
   [ -f "$CPYTHON_LICENSE" ]; then
    mkdir -p "$additions/cpython"
    cp "$CPYTHON_BINARY" "$additions/cpython/python3"
    cp "$CPYTHON_STDLIB" "$additions/cpython/python314.zip"
    cp "$CPYTHON_LICENSE" "$additions/cpython/LICENSE"
    set -- "$@" \
        --add "/usr/bin/python3=$additions/cpython/python3" \
        --add "/usr/lib/python314.zip=$additions/cpython/python314.zip" \
        --add "/usr/share/licenses/cpython/LICENSE=$additions/cpython/LICENSE"
    CPYTHON_STATUS=included
fi

# Construct and fully preflight a same-directory temporary image. If IMAGE
# already exists, clone it first so non-package regions remain unchanged.
image_parent=$(dirname "$IMAGE")
mkdir -p "$image_parent"
image_parent=$(CDPATH= cd -- "$image_parent" && pwd)
IMAGE=$image_parent/$(basename "$IMAGE")
if [ -L "$IMAGE" ] || { [ -e "$IMAGE" ] && [ ! -f "$IMAGE" ]; }; then
    echo "Firefox package image destination is not a regular file: $IMAGE" >&2
    exit 2
fi
image_tmp=$(mktemp "$image_parent/.makos-firefox-image.XXXXXX")
if [ -f "$IMAGE" ]; then
    cp "$IMAGE" "$image_tmp"
fi
package_script=$1
shift 2
set -- "$package_script" "$image_tmp" "$@"
python3 "$@"
python3 "$ROOT/scripts/verify_package.py" "$image_tmp"
python3 "$ROOT/scripts/verify_firefox_runtime_image.py" "$image_tmp"

# Catch build-tree mutation overlapping either staging or image construction.
# Recheck canonical release paths too, before publishing beside them.
verify_release_paths
python3 "$ROOT/scripts/firefox_provenance.py" verify-build-stamp \
    --source-dir "$ROOT/build/ports/firefox/source" \
    --bin-dir "$BIN" \
    --stamp "$BUILD_PROVENANCE"

# Prepare every expected_sources output beside its destination, then publish
# each complete file atomically. IMAGE is moved last, so every earlier failure
# preserves the previously published image.
mkdir -p "$(dirname "$STRIPPED")"
stripped_tmp=$(mktemp "$STRIPPED.XXXXXX")
firefox_publish=$(mktemp "$DIST/.firefox.XXXXXX")
plugin_publish=$(mktemp "$DIST/.plugin-container.XXXXXX")
xpcshell_publish=$(mktemp "$DIST/.xpcshell.XXXXXX")
nspr_publish=$(mktemp "$DIST/.libnspr4.so.XXXXXX")
runtime_publish=$(mktemp "$DIST/.makos-build-provenance.json.XXXXXX")
cp -p "$package_root/libxul.so" "$stripped_tmp"
cp -p "$package_root/firefox" "$firefox_publish"
cp -p "$package_root/plugin-container" "$plugin_publish"
cp -p "$package_root/xpcshell" "$xpcshell_publish"
cp -p "$package_root/libnspr4.so" "$nspr_publish"
cp -p "$package_root/makos-build-provenance.json" "$runtime_publish"
mv "$firefox_publish" "$DIST/firefox"
firefox_publish=
mv "$plugin_publish" "$DIST/plugin-container"
plugin_publish=
mv "$xpcshell_publish" "$DIST/xpcshell"
xpcshell_publish=
mv "$nspr_publish" "$DIST/libnspr4.so"
nspr_publish=
mv "$runtime_publish" "$DIST/makos-build-provenance.json"
runtime_publish=
mv "$stripped_tmp" "$STRIPPED"
stripped_tmp=
for artifact in firefox plugin-container xpcshell libnspr4.so; do
    if ! cmp -s "$package_root/$artifact" "$DIST/$artifact"; then
        echo "Published Firefox integration source differs: $artifact" >&2
        exit 2
    fi
done
if ! cmp -s "$package_root/libxul.so" "$STRIPPED" || \
   ! cmp -s "$package_root/makos-build-provenance.json" \
        "$DIST/makos-build-provenance.json"; then
    echo "Published Firefox integration source differs" >&2
    exit 2
fi
verify_release_paths
python3 "$ROOT/scripts/firefox_provenance.py" verify-build-stamp \
    --source-dir "$ROOT/build/ports/firefox/source" \
    --bin-dir "$BIN" \
    --stamp "$BUILD_PROVENANCE"
mv "$image_tmp" "$IMAGE"
image_tmp=

echo "MAKOS_FIREFOX_PACKAGE_OK image=$IMAGE exec=/usr/lib/firefox/firefox libxul=stripped font=mplus provenance=source,patch-series,artifact-sha256 nano=$NANO_STATUS cpython=$CPYTHON_STATUS"
