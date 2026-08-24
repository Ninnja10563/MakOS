#!/usr/bin/env python3
"""Reject malformed MakOS build artifacts before boot testing."""

import pathlib
import struct
import sys


def check_image(path: pathlib.Path) -> None:
    data = path.read_bytes()
    assert len(data) == 64 * 1024 * 1024, "unexpected disk-image size"
    assert data[510:512] == b"\x55\xaa", "missing FAT boot signature"
    assert data[82:90] == b"FAT32   ", "not FAT32"
    assert b"EFI     " in data and b"KERNEL  ELF" in data and b"MAKOS   CFG" in data, "required root entries absent"
    assert b"root=ata1 log=serial makfs.recover=auto\n" in data, "boot config payload absent"


def check_kernel(path: pathlib.Path) -> None:
    data = path.read_bytes()
    assert data[:4] == b"\x7fELF", "kernel not ELF"
    assert data[4:7] == bytes((2, 1, 1)), "kernel not ELF64 little-endian v1"
    machine = struct.unpack_from("<H", data, 18)[0]
    assert machine in (62, 183), "kernel machine is neither x86_64 nor AArch64"
    entry = struct.unpack_from("<Q", data, 24)[0]
    phoff = struct.unpack_from("<Q", data, 32)[0]
    phentsize, phnum = struct.unpack_from("<HH", data, 54)
    assert phentsize == 56 and phnum > 0, "invalid program-header table"
    spans = []
    for index in range(phnum):
        offset = phoff + index * phentsize
        p_type = struct.unpack_from("<I", data, offset)[0]
        if p_type == 1:
            paddr, memsz = struct.unpack_from("<QQ", data, offset + 24)[0], struct.unpack_from("<Q", data, offset + 40)[0]
            spans.append((paddr, paddr + memsz))
    assert spans, "kernel has no PT_LOAD"
    assert any(start <= entry < end for start, end in spans), "entry outside PT_LOAD"
    minimum = 0x04000000 if machine == 62 else 0x40000000
    assert min(start for start, _ in spans) >= minimum, "kernel load address too low"


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit(f"usage: {sys.argv[0]} IMAGE KERNEL")
    check_image(pathlib.Path(sys.argv[1]))
    check_kernel(pathlib.Path(sys.argv[2]))
    print("artifact checks passed")
