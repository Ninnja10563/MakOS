#!/bin/sh
# SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
"$port_dir/build-probe.sh" >/dev/null
"$port_dir/audit.sh"
