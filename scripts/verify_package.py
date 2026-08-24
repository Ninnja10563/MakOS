#!/usr/bin/env python3
"""Verify every manifest and payload byte in a MakOS package image."""

import argparse
import binascii
import pathlib
import struct
import sys

import mkpackage


def verify(image: pathlib.Path) -> tuple[int, int]:
    with image.open("rb") as source:
        source.seek(mkpackage.HEADER_LBA * mkpackage.SECTOR)
        header = source.read(mkpackage.SECTOR)
        if len(header) != mkpackage.SECTOR or header[:8] != mkpackage.HEADER_MAGIC:
            raise ValueError("package header absent")
        if struct.unpack_from("<I", header, 508)[0] != mkpackage.crc32(header[:508]):
            raise ValueError("package header CRC mismatch")
        version, count = struct.unpack_from("<II", header, 8)
        entry_lba, data_lba, end_lba = struct.unpack_from("<QQQ", header, 16)
        if (
            version != 1
            or count > mkpackage.MAX_ENTRIES
            or entry_lba != mkpackage.ENTRY_LBA
            or data_lba != mkpackage.DATA_LBA
            or end_lba > mkpackage.PACKAGE_LIMIT_LBA
            or image.stat().st_size < mkpackage.IMAGE_BYTES
            or end_lba * mkpackage.SECTOR > image.stat().st_size
        ):
            raise ValueError("package header bounds invalid")
        paths: set[bytes] = set()
        total = 0
        for index in range(count):
            source.seek((entry_lba + index) * mkpackage.SECTOR)
            entry = source.read(mkpackage.SECTOR)
            if entry[:8] != mkpackage.ENTRY_MAGIC:
                raise ValueError(f"entry {index} magic mismatch")
            if struct.unpack_from("<I", entry, 508)[0] != mkpackage.crc32(entry[:508]):
                raise ValueError(f"entry {index} CRC mismatch")
            path_length, flags, stored_index = struct.unpack_from("<HHI", entry, 8)
            size, first_lba, sectors, wanted_crc = struct.unpack_from("<QQQI", entry, 16)
            path = entry[64 : 64 + path_length]
            if (
                flags != 1
                or stored_index != index
                or not path.startswith(b"/")
                or path == b"/packages"
                or path.startswith(b"/packages/")
                or path in paths
                or sectors != (size + mkpackage.SECTOR - 1) // mkpackage.SECTOR
                or first_lba < data_lba
                or first_lba + sectors > end_lba
            ):
                raise ValueError(f"entry {index} bounds invalid")
            paths.add(path)
            source.seek(first_lba * mkpackage.SECTOR)
            remaining = size
            actual_crc = 0
            while remaining:
                block = source.read(min(1024 * 1024, remaining))
                if not block:
                    raise ValueError(f"entry {index} short payload")
                actual_crc = binascii.crc32(block, actual_crc)
                remaining -= len(block)
            if actual_crc & 0xFFFFFFFF != wanted_crc:
                raise ValueError(f"entry {index} payload CRC mismatch")
            total += size
    return count, total


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("image", type=pathlib.Path)
    args = parser.parse_args()
    try:
        count, total = verify(args.image)
    except (OSError, ValueError) as error:
        print(f"verify_package: {error}", file=sys.stderr)
        return 1
    print(f"MAKOS_PACKAGE_VERIFY_OK files={count} payload_bytes={total} metadata_crc=1 payload_crc=1")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
