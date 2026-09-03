#!/bin/sh
# Source-only host-tool selection shared by the CPython build and its tests.

makos_cpython_readelf_is_usable() {
    test "$#" -eq 1 || return 1
    makos_cpython_readelf_candidate=$1
    test -x "$makos_cpython_readelf_candidate" || return 1
    makos_cpython_readelf_version=$(
        "$makos_cpython_readelf_candidate" --version 2>/dev/null
    ) || return 1
    case "$makos_cpython_readelf_version" in
        *LLVM*|*llvm*) return 0 ;;
        *) return 1 ;;
    esac
}

makos_cpython_select_readelf() {
    test "$#" -eq 1 || {
        echo "CPython MakOS host-tool preflight blocked: expected repository path." >&2
        return 1
    }
    makos_cpython_host_repo=$1

    if test -n "${MAKOS_READELF-}"; then
        makos_cpython_readelf_source=explicit
        makos_cpython_readelf_selected=$MAKOS_READELF
        makos_cpython_readelf_is_usable "$makos_cpython_readelf_selected" || {
            echo "CPython MakOS host-tool preflight blocked: MAKOS_READELF is not executable LLVM readelf: $makos_cpython_readelf_selected" >&2
            return 1
        }
    else
        makos_cpython_readelf_source=
        for makos_cpython_readelf_candidate in \
            "$makos_cpython_host_repo/build/host-tools/llvm19/usr/bin/llvm-readelf-19" \
            llvm-readelf-19 llvm-readelf \
            /opt/homebrew/opt/llvm/bin/llvm-readelf
        do
            makos_cpython_readelf_path=$(command -v \
                "$makos_cpython_readelf_candidate" 2>/dev/null || true)
            if test -n "$makos_cpython_readelf_path" && \
               makos_cpython_readelf_is_usable "$makos_cpython_readelf_path"; then
                makos_cpython_readelf_selected=$makos_cpython_readelf_path
                case "$makos_cpython_readelf_candidate" in
                    "$makos_cpython_host_repo"/*)
                        makos_cpython_readelf_source=staged-llvm19
                        ;;
                    /opt/homebrew/*)
                        makos_cpython_readelf_source=homebrew-llvm
                        ;;
                    *)
                        makos_cpython_readelf_source=path
                        ;;
                esac
                break
            fi
        done
        test -n "$makos_cpython_readelf_source" || {
            echo "CPython MakOS host-tool preflight blocked: executable LLVM readelf missing (set MAKOS_READELF)." >&2
            return 1
        }
    fi

    MAKOS_READELF=$makos_cpython_readelf_selected
    export MAKOS_READELF
    printf '%s\n' "MAKOS_CPYTHON_HOST_TOOLS_OK readelf=$makos_cpython_readelf_source"
}
