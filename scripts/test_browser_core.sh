#!/bin/sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
TEMP=$(mktemp -d "${TMPDIR:-/tmp}/makos-browser.XXXXXX")
trap 'rm -rf "$TEMP"' EXIT HUP INT TERM

rustc --edition=2024 --test "$ROOT/kernel/src/browser.rs" \
    -o "$TEMP/browser-core-tests"
"$TEMP/browser-core-tests"

rustc --edition=2024 --test "$ROOT/kernel/src/aarch64_net_wire.rs" \
    -o "$TEMP/aarch64-net-wire-tests"
"$TEMP/aarch64-net-wire-tests"

grep -q 'pub const AF_INET6: u64 = 10' "$ROOT/kernel/src/aarch64_socket.rs"
grep -q 'parse_udp_v6' "$ROOT/kernel/src/aarch64_socket.rs"
grep -q 'tcp6_ingest' "$ROOT/kernel/src/aarch64_socket.rs"
grep -q 'M_socket_name = 134' \
    "$ROOT/ports/musl/patches/0060-makos-ipv6-sockets.patch"
grep -q '__makos_address_family_configured' \
    "$ROOT/ports/musl/patches/0060-makos-ipv6-sockets.patch"

clang -target aarch64-unknown-none-elf -std=c17 -ffreestanding \
    -fno-builtin -fno-stack-protector -fno-pic -fno-unwind-tables \
    -fno-asynchronous-unwind-tables -mgeneral-regs-only -Os \
    -Wall -Wextra -Werror -c "$ROOT/user/aarch64_browser.c" \
    -o "$TEMP/aarch64-browser.o"

if nm -u "$TEMP/aarch64-browser.o" | grep -q .; then
    nm -u "$TEMP/aarch64-browser.o" >&2
    exit 1
fi

RUST_SYSROOT=$(rustc --print sysroot)
RUST_HOST=$(rustc -vV | sed -n 's/^host: //p')
LLD="$RUST_SYSROOT/lib/rustlib/$RUST_HOST/bin/rust-lld"
"$LLD" -flavor gnu --build-id=none -z max-page-size=4096 \
    -T "$ROOT/user/linker-aarch64.ld" -o "$TEMP/aarch64-browser.elf" \
    "$TEMP/aarch64-browser.o"
file "$TEMP/aarch64-browser.elf" | grep -q 'ARM aarch64'

echo "MakOS browser: parser/wire tests + freestanding AArch64 ELF passed"
