#!/bin/sh
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. https://mozilla.org/MPL/2.0/
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
source_dir="$repo_dir/build/ports/firefox/source"
out_dir="$repo_dir/build/ports/firefox/nspr-probe"

test -d "$source_dir/.git" || {
    echo "Firefox source missing; run ports/firefox/clone.sh" >&2
    exit 1
}
"$port_dir/apply-patches.sh" >/dev/null

grep -Fq '*-makos*)' "$source_dir/nsprpub/configure.in"
grep -Fq 'MDCPUCFG_H=_makos.cfg' "$source_dir/nsprpub/configure.in"
grep -Fq 'PR_MD_CSRCS=makos.c' "$source_dir/nsprpub/configure.in"
grep -Fq '#include "md/_makos.h"' \
    "$source_dir/nsprpub/pr/include/md/prosdep.h"
grep -Fq 'defined(RISCOS) || defined(MAKOS)' \
    "$source_dir/nsprpub/pr/include/md/_pth.h"

mkdir -p "$out_dir"
clang --target=aarch64-unknown-makos -D__makos__ -D__AARCH64EL__ \
    -std=c11 -ffreestanding -fno-builtin -fno-stack-protector \
    -I"$source_dir/nsprpub/pr/include/md" \
    -c "$port_dir/nspr-abi-probe.c" -o "$out_dir/nspr-abi-probe.o"
file "$out_dir/nspr-abi-probe.o" | grep -q 'ELF 64-bit.*ARM aarch64'
git -C "$source_dir" diff --check

echo "MAKOS_FIREFOX_NSPR_ABI_OK os=MakOS arch=aarch64 model=lp64 threads=pthreads linux_masquerade=0 runtime=blocked"
