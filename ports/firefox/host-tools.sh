#!/bin/sh
# Source-only host-tool selection shared by the Firefox build and its tests.

makos_firefox_select_host_tools() {
    test "$#" -eq 2 || {
        echo "Firefox MakOS host-tool preflight blocked: expected repository and Python paths." >&2
        return 1
    }
    makos_host_repo=$1
    makos_host_python=$2

    case "${MAKOS_HOST_CC-}:${MAKOS_HOST_CXX-}" in
        :) ;;
        :*|*:)
            echo "Firefox MakOS host-tool preflight blocked: set both MAKOS_HOST_CC and MAKOS_HOST_CXX." >&2
            return 1
            ;;
        *) ;;
    esac

    if test -z "${MAKOS_HOST_CC-}"; then
        makos_host_pair=
        case "$(uname -s):$(uname -m)" in
            Linux:aarch64)
                makos_host_c="$makos_host_repo/build/host-tools/llvm19/usr/bin/clang-19"
                makos_host_cxx="$makos_host_repo/build/host-tools/llvm19/usr/bin/clang++-19"
                if test -x "$makos_host_c" && test -x "$makos_host_cxx"; then
                    makos_host_pair=staged-llvm19
                fi
                ;;
            Darwin:arm64)
                makos_host_c=/opt/homebrew/opt/llvm/bin/clang
                makos_host_cxx=/opt/homebrew/opt/llvm/bin/clang++
                if test -x "$makos_host_c" && test -x "$makos_host_cxx"; then
                    makos_host_pair=homebrew-llvm
                fi
                ;;
        esac
        if test -z "$makos_host_pair"; then
            for makos_host_names in 'clang-19 clang++-19' 'clang clang++' 'gcc g++'; do
                set -- $makos_host_names
                makos_host_c=$(command -v "$1" 2>/dev/null || true)
                makos_host_cxx=$(command -v "$2" 2>/dev/null || true)
                if test -n "$makos_host_c" && test -n "$makos_host_cxx"; then
                    makos_host_pair=$1
                    break
                fi
            done
        fi
        test -n "$makos_host_pair" || {
            echo "Firefox MakOS host-tool preflight blocked: no complete host C/C++ compiler pair." >&2
            return 1
        }
        MAKOS_HOST_CC=$makos_host_c
        MAKOS_HOST_CXX=$makos_host_cxx
    else
        makos_host_pair=explicit
    fi
    test -x "$MAKOS_HOST_CC" && test -x "$MAKOS_HOST_CXX" || {
        echo "Firefox MakOS host-tool preflight blocked: host compiler pair is not executable." >&2
        return 1
    }

    if test -z "${CBINDGEN-}"; then
        makos_host_cbindgen="$makos_host_repo/build/host-tools/cbindgen/bin/cbindgen"
        if test -x "$makos_host_cbindgen"; then
            CBINDGEN=$makos_host_cbindgen
        else
            CBINDGEN=$(command -v cbindgen 2>/dev/null || true)
        fi
    fi
    test -n "${CBINDGEN-}" && test -x "$CBINDGEN" || {
        echo "Firefox MakOS host-tool preflight blocked: cbindgen is missing." >&2
        return 1
    }
    makos_host_cbindgen_version=$("$CBINDGEN" --version 2>/dev/null || true)
    "$makos_host_python" - "$makos_host_cbindgen_version" <<'PY' || return 1
import re
import sys

match = re.fullmatch(r"cbindgen ([0-9]+)\.([0-9]+)\.([0-9]+)", sys.argv[1])
if not match or tuple(map(int, match.groups())) < (0, 27, 0):
    print("Firefox MakOS host-tool preflight blocked: cbindgen 0.27.0 or newer required.", file=sys.stderr)
    raise SystemExit(1)
PY

    makos_host_probe=$(mktemp -d "${TMPDIR:-/tmp}/makos-firefox-host.XXXXXX") || return 1
    if ! (
        trap 'rm -rf -- "$makos_host_probe"' EXIT
        trap 'exit 129' HUP
        trap 'exit 130' INT
        trap 'exit 143' TERM
        printf '%s\n' 'int main(void) { return 0; }' |
            "$MAKOS_HOST_CC" -x c - -o "$makos_host_probe/c-probe" &&
            "$makos_host_probe/c-probe" &&
            printf '%s\n' 'int main() { return 0; }' |
            "$MAKOS_HOST_CXX" -x c++ - -o "$makos_host_probe/cxx-probe" &&
            "$makos_host_probe/cxx-probe"
    )
    then
        echo "Firefox MakOS host-tool preflight blocked: host compiler probe failed." >&2
        return 1
    fi
    export MAKOS_HOST_CC MAKOS_HOST_CXX CBINDGEN
    printf '%s\n' "MAKOS_FIREFOX_HOST_TOOLS_OK pair=$makos_host_pair cbindgen=$makos_host_cbindgen_version"
}
