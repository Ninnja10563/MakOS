#!/bin/sh
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT HUP INT TERM
real_cc=$(command -v cc)
real_cxx=$(command -v c++)
python=$(command -v python3)

make_tool() {
    path=$1
    target=$2
    mkdir -p "$(dirname -- "$path")"
    printf '#!/bin/sh\nexec "%s" "$@"\n' "$target" >"$path"
    chmod +x "$path"
}
make_cbindgen() {
    path=$1
    version=$2
    mkdir -p "$(dirname -- "$path")"
    printf '#!/bin/sh\nprintf "cbindgen %s\\n"\n' "$version" >"$path"
    chmod +x "$path"
}
run_select() {
    repo=$1
    shift
    env -i PATH="$PATH" TMPDIR="$tmp" "$@" sh -c \
        '. "$1"; makos_firefox_select_host_tools "$2" "$3" || exit; printf "CC=%s\nCXX=%s\nCBINDGEN=%s\n" "$MAKOS_HOST_CC" "$MAKOS_HOST_CXX" "$CBINDGEN"' \
        test-host-tools "$port_dir/host-tools.sh" "$repo" "$python"
}

repo="$tmp/repo ;[] with spaces"
staged="$repo/build/host-tools/llvm19/usr/bin"
make_tool "$staged/clang-19" "$real_cc"
make_tool "$staged/clang++-19" "$real_cxx"
make_cbindgen "$repo/build/host-tools/cbindgen/bin/cbindgen" 0.27.0
output=$(run_select "$repo")
printf '%s\n' "$output" | grep -Fq 'cbindgen=cbindgen 0.27.0'
if test "$(uname -s):$(uname -m)" = Linux:aarch64; then
    printf '%s\n' "$output" | grep -Fq 'MAKOS_FIREFOX_HOST_TOOLS_OK pair=staged-llvm19'
    printf '%s\n' "$output" | grep -Fxq "CC=$staged/clang-19"
    printf '%s\n' "$output" | grep -Fxq "CXX=$staged/clang++-19"
fi

explicit="$tmp/explicit \$(touch SENTINEL) tools"
make_tool "$explicit/host cc" "$real_cc"
make_tool "$explicit/host cxx" "$real_cxx"
make_cbindgen "$explicit/cbindgen" 0.28.1
output=$(run_select "$repo" MAKOS_HOST_CC="$explicit/host cc" \
    MAKOS_HOST_CXX="$explicit/host cxx" CBINDGEN="$explicit/cbindgen")
printf '%s\n' "$output" | grep -Fq 'pair=explicit cbindgen=cbindgen 0.28.1'

if run_select "$repo" MAKOS_HOST_CC="$explicit/host cc" >"$tmp/one.out" 2>"$tmp/one.err"; then
    echo "Firefox host-tool test accepted one-sided compiler override" >&2
    exit 1
fi
grep -Fq 'set both MAKOS_HOST_CC and MAKOS_HOST_CXX' "$tmp/one.err"

make_cbindgen "$explicit/old-cbindgen" 0.26.0
if run_select "$repo" MAKOS_HOST_CC="$explicit/host cc" \
    MAKOS_HOST_CXX="$explicit/host cxx" CBINDGEN="$explicit/old-cbindgen" \
    >"$tmp/old.out" 2>"$tmp/old.err"; then
    echo "Firefox host-tool test accepted old cbindgen" >&2
    exit 1
fi
grep -Fq 'cbindgen 0.27.0 or newer required' "$tmp/old.err"

make_cbindgen "$explicit/bad-cbindgen" malformed
if run_select "$repo" MAKOS_HOST_CC="$explicit/host cc" \
    MAKOS_HOST_CXX="$explicit/host cxx" CBINDGEN="$explicit/bad-cbindgen" \
    >"$tmp/bad.out" 2>"$tmp/bad.err"; then
    echo "Firefox host-tool test accepted malformed cbindgen version" >&2
    exit 1
fi
grep -Fq 'cbindgen 0.27.0 or newer required' "$tmp/bad.err"

empty_repo="$tmp/no staged tools"
mkdir -p "$empty_repo"
if run_select "$empty_repo" MAKOS_HOST_CC="$explicit/host cc" \
    MAKOS_HOST_CXX="$explicit/host cxx" CBINDGEN="$tmp/does-not-exist" \
    >"$tmp/missing.out" 2>"$tmp/missing.err"; then
    echo "Firefox host-tool test accepted missing cbindgen" >&2
    exit 1
fi
grep -Fq 'cbindgen is missing' "$tmp/missing.err"

failing="$explicit/failing cxx"
printf '%s\n' '#!/bin/sh' 'exit 9' >"$failing"
chmod +x "$failing"
if run_select "$repo" MAKOS_HOST_CC="$explicit/host cc" MAKOS_HOST_CXX="$failing" \
    CBINDGEN="$explicit/cbindgen" >"$tmp/fail.out" 2>"$tmp/fail.err"; then
    echo "Firefox host-tool test accepted failing compiler" >&2
    exit 1
fi
grep -Fq 'host compiler probe failed' "$tmp/fail.err"

terminating="$explicit/terminating cc"
printf '%s\n' '#!/bin/sh' 'kill -TERM "$PPID"' 'sleep 1' 'exit 9' >"$terminating"
chmod +x "$terminating"
if run_select "$repo" MAKOS_HOST_CC="$terminating" \
    MAKOS_HOST_CXX="$explicit/host cxx" CBINDGEN="$explicit/cbindgen" \
    >"$tmp/term.out" 2>"$tmp/term.err"; then
    echo "Firefox host-tool test swallowed probe termination" >&2
    exit 1
fi
grep -Fq 'host compiler probe failed' "$tmp/term.err"
for residue in "$tmp"/makos-firefox-host.*; do
    test ! -e "$residue" || {
        echo "Firefox host-tool test found a leaked probe directory" >&2
        exit 1
    }
done
test ! -e "$tmp/SENTINEL"

echo 'MAKOS_FIREFOX_HOST_TOOLS_TEST_OK staged=preferred explicit=preserved pair=complete cbindgen=min-version probes=executed paths=literal'
