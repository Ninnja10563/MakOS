#!/bin/sh
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
. "$port_dir/source.lock"

dist_dir="$repo_dir/build/ports/nano/distfiles"
archive="$dist_dir/$NANO_ARCHIVE"
signature="$archive.sig"
mkdir -p "$dist_dir"
temporary=
signature_temporary=

cleanup() {
    test -z "$temporary" || rm -f "$temporary"
    test -z "$signature_temporary" || rm -f "$signature_temporary"
}
trap cleanup EXIT HUP INT TERM

verify_sha256() {
    expected=$1
    file=$2
    if command -v shasum >/dev/null 2>&1; then
        actual=$(shasum -a 256 "$file" | awk '{print $1}')
    elif command -v sha256sum >/dev/null 2>&1; then
        actual=$(sha256sum "$file" | awk '{print $1}')
    else
        echo "nano fetch: need shasum or sha256sum" >&2
        return 1
    fi
    test "$actual" = "$expected" || {
        echo "nano fetch: SHA-256 mismatch for $file" >&2
        echo "expected $expected" >&2
        echo "actual   $actual" >&2
        return 1
    }
}

if test ! -f "$archive" || ! verify_sha256 "$NANO_SHA256" "$archive"; then
    temporary=$(mktemp "$dist_dir/$NANO_ARCHIVE.download.XXXXXX")
    curl -fL --proto '=https' --tlsv1.2 -o "$temporary" "$NANO_URL"
    verify_sha256 "$NANO_SHA256" "$temporary"
    mv "$temporary" "$archive"
    chmod 0644 "$archive"
fi

if test ! -f "$signature"; then
    signature_temporary=$(mktemp "$dist_dir/$NANO_ARCHIVE.sig.download.XXXXXX")
    curl -fL --proto '=https' --tlsv1.2 -o "$signature_temporary" \
        "$NANO_SIGNATURE_URL"
    mv "$signature_temporary" "$signature"
    chmod 0644 "$signature"
fi

verify_sha256 "$NANO_SHA256" "$archive"
printf '%s\n' "$archive"
