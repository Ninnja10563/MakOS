#!/bin/sh
# SPDX-License-Identifier: Python-2.0
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
image=${1:-"$repo_dir/build/makos-cpython-data.img"}
stage="$repo_dir/build/ports/cpython/package"

"$port_dir/stage-makos.sh" "$stage"

python3 "$repo_dir/scripts/mkpackage.py" "$image" "$stage" --prefix ""
python3 "$repo_dir/scripts/verify_package.py" "$image"
echo "MAKOS_CPYTHON_PACKAGE_OK version=3.14.7 image=$image exec=/usr/bin/python3 stdlib=/usr/lib/python314.zip fake=0 host_delegation=0"
