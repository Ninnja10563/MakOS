#!/bin/sh
# SPDX-License-Identifier: Python-2.0
set -eu
port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
for script in fetch.sh apply-patches.sh build-makos.sh host-tools.sh \
    stage-makos.sh package-makos.sh test-host-tools.sh test.sh
do
	sh -n "$port_dir/$script"
done
sh "$port_dir/test-host-tools.sh"
"$port_dir/fetch.sh" --check
"$port_dir/apply-patches.sh"
if test -x "$port_dir/../../build/ports/cpython/makos/python.exe"; then
	file "$port_dir/../../build/ports/cpython/makos/python.exe" |
		grep -q 'ELF 64-bit LSB pie executable, ARM aarch64'
	echo "MAKOS_CPYTHON_PORT_TEST_OK version=3.14.7 executable=1"
else
	echo "MAKOS_CPYTHON_PORT_TEST_OK version=3.14.7 executable=not-built"
fi
