#!/bin/sh
set -eu

PORT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
ROOT_DIR=$(CDPATH= cd -- "$PORT_DIR/../.." && pwd)
DEST="$ROOT_DIR/build/ports/micropython-test"
ELF="$DEST/micropython-makos.elf"
$PORT_DIR/build-makos.sh "$DEST" >/dev/null

python3 - "$ELF" <<'PY'
import pathlib
import struct
import sys

path = pathlib.Path(sys.argv[1])
data = path.read_bytes()
if data[:6] != b"\x7fELF\x02\x01":
    raise SystemExit("not little-endian ELF64")
etype, machine = struct.unpack_from("<HH", data, 16)
entry = struct.unpack_from("<Q", data, 24)[0]
phoff = struct.unpack_from("<Q", data, 32)[0]
phentsize, phnum = struct.unpack_from("<HH", data, 54)
if etype != 2 or machine != 183 or not (0x10000000 <= entry < 0x14000000):
    raise SystemExit("wrong MicroPython ELF target/layout")
loads = 0
for index in range(phnum):
    offset = phoff + index * phentsize
    p_type, flags = struct.unpack_from("<II", data, offset)
    if p_type != 1:
        continue
    loads += 1
    vaddr = struct.unpack_from("<Q", data, offset + 16)[0]
    filesz, memsz = struct.unpack_from("<QQ", data, offset + 32)
    if flags & 3 == 3 or filesz > memsz or vaddr < 0x10000000 or vaddr + memsz > 0x14000000:
        raise SystemExit("unsafe MicroPython PT_LOAD")
if loads != 3:
    raise SystemExit(f"expected 3 PT_LOAD segments, got {loads}")
print(f"MicroPython MakOS ELF verified: bytes={len(data)} entry={entry:#x} loads={loads}")
PY
