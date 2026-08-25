#!/bin/sh
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. https://mozilla.org/MPL/2.0/
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
out_dir="$repo_dir/build/ports/firefox/toolchain-test"
driver="$port_dir/toolchain/makos-clang"
nm=${MAKOS_NM:-/opt/homebrew/opt/llvm/bin/llvm-nm}

test -x "$driver"
test -x "$port_dir/toolchain/makos-clang++"
test -x "$nm"
python3 -m py_compile "$port_dir/toolchain/makos-clang.py"
mkdir -p "$out_dir/empty-sysroot"

"$driver" --target=aarch64-unknown-makos -ffreestanding -c \
    "$port_dir/toolchain-probe.c" -o "$out_dir/probe.o"
file "$out_dir/probe.o" | grep -q 'ELF 64-bit.*ARM aarch64'
"$driver" --target=aarch64-unknown-makos -ffreestanding -c \
    "$port_dir/stack-protector-probe.c" -o "$out_dir/stack-protected.o"
"$nm" -u "$out_dir/stack-protected.o" | grep -q '__stack_chk_fail'
"$driver" --target=aarch64-unknown-makos -ffreestanding \
    -fno-stack-protector -c "$port_dir/stack-protector-probe.c" \
    -o "$out_dir/stack-unprotected.o"
if "$nm" -u "$out_dir/stack-unprotected.o" | grep -q '__stack_chk_fail'; then
    echo "MakOS clang ignored explicit stack-protector opt-out" >&2
    exit 1
fi
cp "$out_dir/probe.o" "$out_dir/probe.lo"
"$driver" --target=aarch64-unknown-makos -nostdlib \
    -Wl,--build-id=none -Wl,-e,_start "$out_dir/probe.lo" \
    -o "$out_dir/probe-lo.elf"
file "$out_dir/probe-lo.elf" | grep -q 'ELF 64-bit.*ARM aarch64'
"$driver" --target=aarch64-unknown-makos -ffreestanding -nostdlib \
    -Wl,--build-id=none -Wl,-e,_start "$port_dir/toolchain-probe.c" \
    -o "$out_dir/probe.elf"
file "$out_dir/probe.elf" | grep -q 'ELF 64-bit.*ARM aarch64'
python3 - "$port_dir/toolchain/makos-clang.py" <<'PY'
import importlib.util
import pathlib
import sys

spec = importlib.util.spec_from_file_location("makos_clang", pathlib.Path(sys.argv[1]))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
link, *_ = module.linker_args(["-rdynamic"], {})
assert link == ["--export-dynamic"], link
assert module.add_security_defaults(["probe.c"])[-1] == "-fstack-protector-strong"
assert module.add_security_defaults(["probe.S"]) == ["probe.S"]
assert module.add_security_defaults(["-fno-stack-protector", "probe.c"]) == [
    "-fno-stack-protector", "probe.c"
]
PY

if "$driver" --target=aarch64-unknown-makos \
    --sysroot="$out_dir/empty-sysroot" "$port_dir/toolchain-probe.c" \
    -o "$out_dir/invalid.elf" >"$out_dir/runtime.stdout" \
    2>"$out_dir/runtime.stderr"; then
    echo "MakOS clang driver accepted missing target runtime" >&2
    exit 1
fi
grep -Fq 'default target runtime incomplete' "$out_dir/runtime.stderr"
MAKOS_CC="$driver" "$port_dir/toolchain-audit.sh" --require >/dev/null

echo "MAKOS_FIREFOX_TOOLCHAIN_DRIVER_OK target=aarch64-unknown-makos compile=elf link=elf stack_protector=strong-default,explicit-bootstrap-optout host_gcc=unused default_runtime=blocked"
