#!/usr/bin/env python3
"""Install a read-only MakOS package tree into a data-disk image."""

from __future__ import annotations

import argparse
import binascii
import os
import pathlib
import struct
import sys

import package_layout


SECTOR = package_layout.SECTOR
HEADER_LBA = package_layout.LEGACY_HEADER_LBA
ENTRY_LBA = package_layout.LEGACY_ENTRY_LBA
DATA_LBA = package_layout.LEGACY_DATA_LBA
PACKAGE_LIMIT_LBA = package_layout.TRANSACTION_BASE_LBA
PROFILE_DATA_LBA = package_layout.PROFILE_DATA_LBA
IMAGE_BYTES = package_layout.IMAGE_BYTES
# VFS reserves eight of 384 package descriptors for durable activation.
MAX_ENTRIES = 376
MAX_PATH = 255
HEADER_MAGIC = b"MAKPKG01"
ENTRY_MAGIC = b"MAKFILE4"


def crc32(data: bytes | bytearray) -> int:
    return binascii.crc32(data) & 0xFFFFFFFF


def files_under(root: pathlib.Path) -> list[pathlib.Path]:
    return sorted(path for path in root.rglob("*") if path.is_file())


def guest_path(root: pathlib.Path, path: pathlib.Path, prefix: str) -> bytes:
    relative = path.relative_to(root).as_posix()
    value = f"/{prefix.strip('/')}/{relative}".replace("//", "/").encode()
    if not value.startswith(b"/") or len(value) > MAX_PATH:
        raise ValueError(f"invalid/long guest path: {value!r}")
    reject_reserved_path(value)
    return value


def reject_reserved_path(value: bytes) -> None:
    if value == b"/packages" or value.startswith(b"/packages/"):
        raise ValueError(f"reserved durable-package path: {value!r}")


def install(
    image: pathlib.Path,
    root: pathlib.Path,
    prefix: str,
    replacements: dict[str, pathlib.Path] | None = None,
    additions: list[tuple[bytes, pathlib.Path]] | None = None,
) -> tuple[int, int]:
    paths = files_under(root)
    additions = additions or []
    total_count = len(paths) + len(additions)
    if not paths or total_count > MAX_ENTRIES:
        raise ValueError(f"package file count must be 1..{MAX_ENTRIES}; got {total_count}")

    entries: list[tuple[bytes, pathlib.Path, int, int, int]] = []
    next_lba = DATA_LBA
    replacements = replacements or {}
    for path in paths:
        relative = path.relative_to(root).as_posix()
        source = replacements.get(relative, path)
        if not source.is_file():
            raise ValueError(f"replacement absent: {relative}={source}")
        size = source.stat().st_size
        sectors = (size + SECTOR - 1) // SECTOR
        name = guest_path(root, path, prefix)
        entries.append((name, source, size, next_lba, sectors))
        next_lba += sectors
    existing_names = {entry[0] for entry in entries}
    for name, source in additions:
        reject_reserved_path(name)
        if name in existing_names:
            raise ValueError(f"duplicate guest path: {name!r}")
        if not source.is_file():
            raise ValueError(f"addition absent: {name.decode(errors='replace')}={source}")
        size = source.stat().st_size
        sectors = (size + SECTOR - 1) // SECTOR
        entries.append((name, source, size, next_lba, sectors))
        existing_names.add(name)
        next_lba += sectors

    required_size = next_lba * SECTOR
    if next_lba > PACKAGE_LIMIT_LBA:
        raise ValueError(
            "package payload overlaps durable transaction region: "
            f"{next_lba}>{PACKAGE_LIMIT_LBA}"
        )
    image.parent.mkdir(parents=True, exist_ok=True)
    mode = "r+b" if image.exists() else "w+b"
    with image.open(mode) as output:
        output.truncate(max(required_size, IMAGE_BYTES))
        for index, (name, path, size, first_lba, sectors) in enumerate(entries):
            data_crc = 0
            output.seek(first_lba * SECTOR)
            with path.open("rb") as source:
                remaining = size
                while remaining:
                    block = source.read(min(1024 * 1024, remaining))
                    if not block:
                        raise OSError(f"short source read: {path}")
                    output.write(block)
                    data_crc = binascii.crc32(block, data_crc)
                    remaining -= len(block)
            padding = sectors * SECTOR - size
            if padding:
                output.write(b"\0" * padding)

            record = bytearray(SECTOR)
            record[:8] = ENTRY_MAGIC
            struct.pack_into("<HHIQQQI", record, 8, len(name), 1, index, size, first_lba, sectors, data_crc & 0xFFFFFFFF)
            record[64 : 64 + len(name)] = name
            struct.pack_into("<I", record, 508, crc32(record[:508]))
            output.seek((ENTRY_LBA + index) * SECTOR)
            output.write(record)

        header = bytearray(SECTOR)
        header[:8] = HEADER_MAGIC
        struct.pack_into("<IIQQQ", header, 8, 1, len(entries), ENTRY_LBA, DATA_LBA, next_lba)
        struct.pack_into("<I", header, 508, crc32(header[:508]))
        output.seek(HEADER_LBA * SECTOR)
        output.write(header)
        output.flush()
        os.fsync(output.fileno())
    return len(entries), required_size


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("image", type=pathlib.Path)
    parser.add_argument("root", type=pathlib.Path)
    parser.add_argument("--prefix", default="usr/lib/firefox")
    parser.add_argument(
        "--replace",
        action="append",
        default=[],
        metavar="RELATIVE=FILE",
        help="replace one packaged payload while retaining its guest path",
    )
    parser.add_argument(
        "--add",
        action="append",
        default=[],
        metavar="/GUEST/PATH=FILE",
        help="add a payload at an explicit absolute guest path",
    )
    args = parser.parse_args()
    if not args.root.is_dir():
        parser.error(f"package root absent: {args.root}")
    replacements: dict[str, pathlib.Path] = {}
    for replacement in args.replace:
        relative, separator, source = replacement.partition("=")
        if not separator or not relative or relative.startswith("/") or ".." in pathlib.PurePosixPath(relative).parts:
            parser.error(f"invalid replacement: {replacement}")
        replacements[relative] = pathlib.Path(source)
    additions: list[tuple[bytes, pathlib.Path]] = []
    for addition in args.add:
        guest, separator, source = addition.partition("=")
        guest_path_value = pathlib.PurePosixPath(guest)
        encoded = guest.encode()
        if (
            not separator
            or not guest.startswith("/")
            or guest == "/"
            or ".." in guest_path_value.parts
            or len(encoded) > MAX_PATH
        ):
            parser.error(f"invalid addition: {addition}")
        additions.append((encoded, pathlib.Path(source)))
    try:
        count, size = install(args.image, args.root, args.prefix, replacements, additions)
    except (OSError, ValueError) as error:
        print(f"mkpackage: {error}", file=sys.stderr)
        return 1
    print(f"MAKOS_PACKAGE_IMAGE_OK files={count} bytes={size} prefix=/{args.prefix.strip('/')}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
