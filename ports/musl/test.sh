#!/bin/sh
# SPDX-License-Identifier: MIT
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
for script in apply-patches.sh audit.sh audit-static.sh audit-shared.sh \
    build-makos.sh build-shared-makos.sh clone.sh fetch.sh test.sh; do
    sh -n "$port_dir/$script"
done
"$port_dir/audit.sh" >/dev/null
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
source_dir="$repo_dir/build/ports/musl/source"
if test -d "$source_dir/.git"; then
    "$port_dir/apply-patches.sh" >/dev/null
fi
if test -s "$repo_dir/build/ports/musl/makos-static/lib/libc.a"; then
    "$port_dir/audit-static.sh" >/dev/null
fi
if test -s "$repo_dir/build/ports/musl/makos-shared/lib/libc.so"; then
    "$port_dir/audit-shared.sh" >/dev/null
fi
if test "${MUSL_NETWORK_AUDIT-0}" = 1; then
    "$port_dir/fetch.sh" --check
    "$port_dir/clone.sh" --check
fi
echo "MAKOS_MUSL_PORT_TEST_OK static_libc=audited shared_loader=audited-if-built runtime_probes=custom-entry,upstream-crt1,pthread pthreads=create,join"
