#!/bin/sh
# SPDX-License-Identifier: Python-2.0
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
source_dir="$repo_dir/build/ports/cpython/source"
build_dir="$repo_dir/build/ports/cpython/makos"
stage=${1:-"$repo_dir/build/ports/cpython/package"}
strip_tool=${MAKOS_LLVM_STRIP:-/opt/homebrew/opt/llvm/bin/llvm-strip}

"$port_dir/build-makos.sh"
test -x "$strip_tool"
mkdir -p "$stage/usr/bin" "$stage/usr/lib" "$stage/usr/share/licenses/cpython"
cp "$build_dir/python.exe" "$stage/usr/bin/python3"
"$strip_tool" --strip-sections "$stage/usr/bin/python3"
"$port_dir/make-stdlib-zip.py" "$source_dir/Lib" "$stage/usr/lib/python314.zip"
cp "$source_dir/LICENSE" "$stage/usr/share/licenses/cpython/LICENSE"

echo "MAKOS_CPYTHON_STAGE_OK root=$stage exec=usr/bin/python3 stdlib=usr/lib/python314.zip license=usr/share/licenses/cpython/LICENSE"
