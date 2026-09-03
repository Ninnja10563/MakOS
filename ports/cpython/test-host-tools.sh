#!/bin/sh
# SPDX-License-Identifier: Python-2.0
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$port_dir/host-tools.sh"

temporary=$(mktemp -d "${TMPDIR:-/tmp}/makos-cpython-host-tools.XXXXXX")
trap 'rm -rf -- "$temporary"' EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

make_readelf() {
    destination=$1
    version=$2
    mkdir -p "$(dirname "$destination")"
    {
        printf '%s\n' '#!/bin/sh'
        printf '%s\n' "printf '%s\\n' '$version'"
    } >"$destination"
    chmod +x "$destination"
}

staged_repo=$temporary/staged-repository
staged_tool=$staged_repo/build/host-tools/llvm19/usr/bin/llvm-readelf-19
make_readelf "$staged_tool" 'Debian LLVM version 19.1.7'
unset MAKOS_READELF
makos_cpython_select_readelf "$staged_repo" >"$temporary/staged.out"
test "$MAKOS_READELF" = "$staged_tool"
grep -Fxq 'MAKOS_CPYTHON_HOST_TOOLS_OK readelf=staged-llvm19' \
    "$temporary/staged.out"

path_dir=$temporary/path
path_tool=$path_dir/llvm-readelf-19
make_readelf "$path_tool" 'LLVM version 19.1.0'
empty_repo=$temporary/empty-repository
mkdir -p "$empty_repo"
# A present but incompatible staged binary must not suppress a usable LLVM
# tool from PATH.
make_readelf "$staged_tool" 'GNU readelf (GNU Binutils) 2.44'
unset MAKOS_READELF
saved_path=$PATH
PATH=$path_dir:/usr/bin:/bin
makos_cpython_select_readelf "$staged_repo" >"$temporary/path.out"
PATH=$saved_path
test "$MAKOS_READELF" = "$path_tool"
grep -Fxq 'MAKOS_CPYTHON_HOST_TOOLS_OK readelf=path' "$temporary/path.out"

explicit_tool=$temporary/explicit-readelf
make_readelf "$explicit_tool" 'LLVM version 22.1.3'
MAKOS_READELF=$explicit_tool
makos_cpython_select_readelf "$empty_repo" >"$temporary/explicit.out"
test "$MAKOS_READELF" = "$explicit_tool"
grep -Fxq 'MAKOS_CPYTHON_HOST_TOOLS_OK readelf=explicit' \
    "$temporary/explicit.out"

not_llvm=$temporary/not-llvm
make_readelf "$not_llvm" 'GNU readelf (GNU Binutils) 2.44'
MAKOS_READELF=$not_llvm
if makos_cpython_select_readelf "$staged_repo" >"$temporary/reject.out" \
    2>"$temporary/reject.err"; then
    echo "CPython host-tool test accepted a non-LLVM explicit readelf" >&2
    exit 1
fi
grep -Fq 'MAKOS_READELF is not executable LLVM readelf' "$temporary/reject.err"

MAKOS_READELF=$temporary/absent
if makos_cpython_select_readelf "$staged_repo" >"$temporary/missing.out" \
    2>"$temporary/missing.err"; then
    echo "CPython host-tool test accepted an absent explicit readelf" >&2
    exit 1
fi
grep -Fq 'MAKOS_READELF is not executable LLVM readelf' "$temporary/missing.err"

grep -Fq '. "$port_dir/host-tools.sh"' "$port_dir/build-makos.sh"
grep -Fq 'makos_cpython_select_readelf "$repo_dir"' "$port_dir/build-makos.sh"
grep -Fq '"$MAKOS_READELF" -h -l -d python.exe' "$port_dir/build-makos.sh"
if grep -Fq '/opt/homebrew/opt/llvm/bin/llvm-readelf -h' \
    "$port_dir/build-makos.sh"; then
    echo "CPython build still invokes hardcoded Homebrew llvm-readelf" >&2
    exit 1
fi

echo "MAKOS_CPYTHON_HOST_TOOLS_TEST_OK staged=1 path=1 explicit=1 fail_closed=1"
