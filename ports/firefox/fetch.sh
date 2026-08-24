#!/bin/sh
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. https://mozilla.org/MPL/2.0/
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
. "$port_dir/source.lock"

dist_dir="$repo_dir/build/ports/firefox/distfiles"
archive="$dist_dir/$FIREFOX_ARCHIVE"
signature="$archive.asc"
checksums="$dist_dir/SHA512SUMS-$FIREFOX_VERSION"
checksums_signature="$checksums.asc"
mkdir -p "$dist_dir"

sha512() {
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 512 "$1" | awk '{print $1}'
    elif command -v sha512sum >/dev/null 2>&1; then
        sha512sum "$1" | awk '{print $1}'
    else
        echo "firefox fetch: need shasum or sha512sum" >&2
        return 1
    fi
}

verify_archive() {
    actual=$(sha512 "$1")
    test "$actual" = "$FIREFOX_SHA512" || {
        echo "firefox fetch: SHA-512 mismatch" >&2
        echo "expected $FIREFOX_SHA512" >&2
        echo "actual   $actual" >&2
        return 1
    }
}

verify_checksum_manifest() {
    expected_line="$FIREFOX_SHA512  source/$FIREFOX_ARCHIVE"
    grep -Fqx "$expected_line" "$1" || {
        echo "firefox fetch: release manifest lacks pinned archive digest" >&2
        return 1
    }
}

fetch_file() {
    url=$1
    destination=$2
    temporary=$(mktemp "$dist_dir/download.XXXXXX")
    trap 'rm -f "$temporary"' EXIT HUP INT TERM
    curl -fL --proto '=https' --tlsv1.2 -o "$temporary" "$url"
    mv "$temporary" "$destination"
    chmod 0644 "$destination"
    trap - EXIT HUP INT TERM
}

if test "${1-}" = "--check"; then
    temporary=$(mktemp "$dist_dir/checksums.XXXXXX")
    trap 'rm -f "$temporary"' EXIT HUP INT TERM
    curl -fsSL --proto '=https' --tlsv1.2 -o "$temporary" \
        "$FIREFOX_CHECKSUMS_URL"
    verify_checksum_manifest "$temporary"
    curl -fsSI --proto '=https' --tlsv1.2 "$FIREFOX_URL" >/dev/null
    curl -fsSI --proto '=https' --tlsv1.2 "$FIREFOX_SIGNATURE_URL" >/dev/null
    echo "MAKOS_FIREFOX_SOURCE_REMOTE_OK version=$FIREFOX_VERSION"
    exit 0
fi

test -f "$checksums" || fetch_file "$FIREFOX_CHECKSUMS_URL" "$checksums"
test -f "$checksums_signature" || \
    fetch_file "$FIREFOX_CHECKSUMS_SIGNATURE_URL" "$checksums_signature"
verify_checksum_manifest "$checksums"

if test ! -f "$archive" || ! verify_archive "$archive"; then
    fetch_file "$FIREFOX_URL" "$archive"
fi
verify_archive "$archive"
test -f "$signature" || fetch_file "$FIREFOX_SIGNATURE_URL" "$signature"

if test -n "${FIREFOX_GPG_KEYRING-}"; then
    command -v gpgv >/dev/null 2>&1 || {
        echo "firefox fetch: FIREFOX_GPG_KEYRING set but gpgv missing" >&2
        exit 1
    }
    gpgv --keyring "$FIREFOX_GPG_KEYRING" "$checksums_signature" "$checksums"
    gpgv --keyring "$FIREFOX_GPG_KEYRING" "$signature" "$archive"
else
    echo "firefox fetch: SHA-512 verified; OpenPGP files retained, not trusted" >&2
    echo "set FIREFOX_GPG_KEYRING to verify Mozilla release signatures" >&2
fi

printf '%s\n' "$archive"
