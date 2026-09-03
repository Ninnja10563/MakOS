#!/bin/sh
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. https://mozilla.org/MPL/2.0/
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
for script in apply-patches.sh audit.sh audit-binary.sh build-makos.sh build-mode.sh clone.sh fetch.sh host-tools.sh \
    prepare-rust-libc.sh prepare-rust-errno.sh prepare-rust-getrandom.sh prepare-rust-rustix.sh \
    prepare-rust-mtu.sh prepare-rust-nss-gk-api.sh prepare-rust-socket2.sh \
    prepare-rust-libloading.sh \
    test-build-mode.sh test-host-tools.sh test-nspr.sh test-package-coherence.sh test-toolchain.sh test-widget.sh test.sh \
    toolchain-audit.sh
do
    sh -n "$port_dir/$script"
done
"$port_dir/test-host-tools.sh" >/dev/null
python3 "$port_dir/test-print-settings.py"
"$port_dir/audit.sh" >/dev/null
"$port_dir/toolchain-audit.sh" | \
    grep -Eq '^MAKOS_FIREFOX_TOOLCHAIN_(OK|BLOCKED) '
"$port_dir/test-toolchain.sh" >/dev/null
"$port_dir/test-build-mode.sh" >/dev/null
"$port_dir/test-package-coherence.sh" >/dev/null
grep -Fq 'ac_add_options --enable-default-toolkit=cairo-makos' \
    "$port_dir/mozconfig.makos"
grep -Fq 'if test "${MAKOS_FIREFOX_DEVELOPER_BUILD:-0}" = 1' \
    "$port_dir/mozconfig.makos"
grep -Fq 'MAKOS_FIREFOX_DEVELOPER_BUILD_OK binary_audit=passed release_provenance=withheld' \
    "$port_dir/build-makos.sh"
grep -Fq 'clear_developer_provenance()' "$port_dir/build-makos.sh"
test "$(grep -Fc '    clear_developer_provenance' "$port_dir/build-makos.sh")" -eq 3
grep -Fq '"$obj/dist/firefox/makos-build-provenance.json"' \
    "$port_dir/build-makos.sh"
test "$(grep -Fc '"$port_dir/prepare-rust-errno.sh"' "$port_dir/build-makos.sh")" -eq 1
grep -Fq 'target_os = "makos"' \
    "$port_dir/rust-patches/errno-0.3.8-makos.patch"
grep -Fq 'link_name = "__errno_location"' \
    "$port_dir/rust-patches/errno-0.3.8-makos.patch"
grep -Fq 'errno = { path = "../errno-makos" }' \
    "$port_dir/patches/0059-rust-errno-makos-accessor.patch"
if grep -Eq '^export (NM|RANLIB|STRIP)=' "$port_dir/mozconfig.makos"; then
    echo "Firefox MakOS mozconfig exports unavailable configure variables" >&2
    exit 1
fi

source_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)/build/ports/firefox/source
if test -d "$source_dir/.git"; then
    "$port_dir/apply-patches.sh" >/dev/null
    "$port_dir/test-widget.sh" >/dev/null
    "$port_dir/test-nspr.sh" >/dev/null
fi

repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
obj=$("$port_dir/build-mode.sh" "$repo_dir")
bin_dir="$obj/dist/bin"
if test -f "$bin_dir/libxul.so"; then
    "$port_dir/audit-binary.sh" >/dev/null
fi

if test "${FIREFOX_NETWORK_AUDIT-0}" = 1; then
    "$port_dir/fetch.sh" --check
    "$port_dir/clone.sh" --check
fi

echo "MAKOS_FIREFOX_PORT_TEST_OK target_recognition=patched toolkit=cairo-makos toolchain=elf-linked widget_abi=compiled nspr_abi=compiled gecko=linked-if-built runtime=blocked"
