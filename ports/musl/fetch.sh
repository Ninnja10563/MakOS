#!/bin/sh
# SPDX-License-Identifier: MIT
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
. "$port_dir/source.lock"

dist_dir="$repo_dir/build/ports/musl/distfiles"
archive="$dist_dir/$MUSL_ARCHIVE"
signature="$archive.asc"
public_key="$dist_dir/musl.pub"
mkdir -p "$dist_dir"

sha256() {
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{print $1}'
    elif command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    else
        echo "musl fetch: need shasum or sha256sum" >&2
        return 1
    fi
}

verify_archive() {
    actual=$(sha256 "$1")
    test "$actual" = "$MUSL_SHA256" || {
        echo "musl fetch: SHA-256 mismatch" >&2
        echo "expected $MUSL_SHA256" >&2
        echo "actual   $actual" >&2
        return 1
    }
}

fetch_file() {
    temporary=$(mktemp "$dist_dir/download.XXXXXX")
    trap 'rm -f "$temporary"' EXIT HUP INT TERM
    curl -fL --proto '=https' --tlsv1.2 -o "$temporary" "$1"
    mv "$temporary" "$2"
    chmod 0644 "$2"
    trap - EXIT HUP INT TERM
}

if test "${1-}" = "--check"; then
    curl -fsSI --proto '=https' --tlsv1.2 "$MUSL_URL" >/dev/null
    curl -fsSI --proto '=https' --tlsv1.2 "$MUSL_SIGNATURE_URL" >/dev/null
    curl -fsSI --proto '=https' --tlsv1.2 "$MUSL_PUBLIC_KEY_URL" >/dev/null
    echo "MAKOS_MUSL_SOURCE_REMOTE_OK version=$MUSL_VERSION"
    exit 0
fi

if test ! -f "$archive" || ! verify_archive "$archive"; then
    fetch_file "$MUSL_URL" "$archive"
fi
verify_archive "$archive"
test -f "$signature" || fetch_file "$MUSL_SIGNATURE_URL" "$signature"
test -f "$public_key" || fetch_file "$MUSL_PUBLIC_KEY_URL" "$public_key"

if command -v gpg >/dev/null 2>&1; then
    gpg_home=$(mktemp -d "$dist_dir/gpg.XXXXXX")
    trap 'rm -rf "$gpg_home"' EXIT HUP INT TERM
    chmod 0700 "$gpg_home"
    gpg --batch --homedir "$gpg_home" --import "$public_key" >/dev/null 2>&1
    actual_fingerprint=$(gpg --batch --homedir "$gpg_home" \
        --with-colons --fingerprint | awk -F: '$1 == "fpr" { print $10; exit }')
    test "$actual_fingerprint" = "$MUSL_SIGNING_FINGERPRINT" || {
        echo "musl fetch: signing-key fingerprint mismatch" >&2
        exit 1
    }
    gpg --batch --homedir "$gpg_home" --verify "$signature" "$archive"
else
    echo "musl fetch: SHA-256 verified; gpg unavailable, signature retained" >&2
fi

printf '%s\n' "$archive"
