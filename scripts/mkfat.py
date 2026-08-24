#!/usr/bin/env python3
"""Create deterministic 64 MiB FAT32 MakOS UEFI boot image."""

from __future__ import annotations

import math
import pathlib
import struct
import sys

SECTOR = 512
IMAGE_BYTES = 64 * 1024 * 1024
SECTORS_PER_CLUSTER = 1
RESERVED_SECTORS = 32
FAT_COUNT = 2
EOC = 0x0FFFFFFF


def short_name(name: str) -> bytes:
    if name in (".", ".."):
        return name.encode("ascii").ljust(11, b" ")
    parts = name.upper().split(".")
    if len(parts) > 2 or not 1 <= len(parts[0]) <= 8:
        raise ValueError(f"not an 8.3 name: {name}")
    extension = parts[1] if len(parts) == 2 else ""
    if len(extension) > 3:
        raise ValueError(f"not an 8.3 name: {name}")
    return parts[0].encode("ascii").ljust(8, b" ") + extension.encode("ascii").ljust(3, b" ")


def dir_entry(name: str, attributes: int, cluster: int, size: int = 0) -> bytes:
    entry = bytearray(32)
    entry[0:11] = short_name(name)
    entry[11] = attributes
    struct.pack_into("<H", entry, 20, (cluster >> 16) & 0xFFFF)
    struct.pack_into("<H", entry, 26, cluster & 0xFFFF)
    struct.pack_into("<I", entry, 28, size)
    return bytes(entry)


def cluster_chain(start: int, count: int, fat: bytearray) -> None:
    for index in range(count):
        value = EOC if index + 1 == count else start + index + 1
        struct.pack_into("<I", fat, (start + index) * 4, value)


def main() -> int:
    if len(sys.argv) != 5:
        print(f"usage: {sys.argv[0]} IMAGE EFI/BOOT/BOOTX64.EFI=FILE|EFI/BOOT/BOOTAA64.EFI=FILE KERNEL.ELF=FILE MAKOS.CFG=FILE", file=sys.stderr)
        return 2
    output = pathlib.Path(sys.argv[1])
    mappings = dict(argument.split("=", 1) for argument in sys.argv[2:])
    loader_names = set(mappings) & {"EFI/BOOT/BOOTX64.EFI", "EFI/BOOT/BOOTAA64.EFI"}
    if len(loader_names) != 1 or set(mappings) - loader_names != {"KERNEL.ELF", "MAKOS.CFG"}:
        raise ValueError("required image paths: one architecture loader, KERNEL.ELF, MAKOS.CFG")
    loader_path = loader_names.pop()
    loader_name = loader_path.rsplit("/", 1)[1]
    loader = pathlib.Path(mappings[loader_path]).read_bytes()
    kernel = pathlib.Path(mappings["KERNEL.ELF"]).read_bytes()
    config = pathlib.Path(mappings["MAKOS.CFG"]).read_bytes()

    total_sectors = IMAGE_BYTES // SECTOR
    fat_sectors = 1
    while True:
        data_sectors = total_sectors - RESERVED_SECTORS - FAT_COUNT * fat_sectors
        cluster_count = data_sectors // SECTORS_PER_CLUSTER
        required = math.ceil((cluster_count + 2) * 4 / SECTOR)
        # FAT may be larger than minimum. Monotonic stop avoids integer-rounding
        # oscillation between adjacent valid sizes.
        if required <= fat_sectors:
            break
        fat_sectors = required
    if cluster_count < 65525:
        raise ValueError("image too small for FAT32")

    image = bytearray(IMAGE_BYTES)
    boot = memoryview(image)[:SECTOR]
    boot[0:3] = b"\xeb\x58\x90"
    boot[3:11] = b"MAKOS   "
    struct.pack_into("<H", boot, 11, SECTOR)
    boot[13] = SECTORS_PER_CLUSTER
    struct.pack_into("<H", boot, 14, RESERVED_SECTORS)
    boot[16] = FAT_COUNT
    struct.pack_into("<H", boot, 17, 0)
    struct.pack_into("<H", boot, 19, 0)
    boot[21] = 0xF8
    struct.pack_into("<H", boot, 22, 0)
    struct.pack_into("<H", boot, 24, 63)
    struct.pack_into("<H", boot, 26, 255)
    struct.pack_into("<I", boot, 28, 0)
    struct.pack_into("<I", boot, 32, total_sectors)
    struct.pack_into("<I", boot, 36, fat_sectors)
    struct.pack_into("<H", boot, 40, 0)
    struct.pack_into("<H", boot, 42, 0)
    struct.pack_into("<I", boot, 44, 2)
    struct.pack_into("<H", boot, 48, 1)
    struct.pack_into("<H", boot, 50, 6)
    boot[64] = 0x80
    boot[66] = 0x29
    struct.pack_into("<I", boot, 67, 0x534F4B4D)
    boot[71:82] = b"MAKOS      "
    boot[82:90] = b"FAT32   "
    boot[510:512] = b"\x55\xaa"

    fsinfo = memoryview(image)[SECTOR : 2 * SECTOR]
    struct.pack_into("<I", fsinfo, 0, 0x41615252)
    struct.pack_into("<I", fsinfo, 484, 0x61417272)
    struct.pack_into("<I", fsinfo, 488, 0xFFFFFFFF)
    struct.pack_into("<I", fsinfo, 492, 5)
    struct.pack_into("<I", fsinfo, 508, 0xAA550000)
    image[6 * SECTOR : 7 * SECTOR] = image[0:SECTOR]
    image[7 * SECTOR : 8 * SECTOR] = image[SECTOR : 2 * SECTOR]

    loader_clusters = max(1, math.ceil(len(loader) / SECTOR))
    kernel_clusters = max(1, math.ceil(len(kernel) / SECTOR))
    config_clusters = max(1, math.ceil(len(config) / SECTOR))
    loader_start = 5
    kernel_start = loader_start + loader_clusters
    config_start = kernel_start + kernel_clusters
    if config_start + config_clusters >= cluster_count + 2:
        raise ValueError("files do not fit image")

    fat = bytearray(fat_sectors * SECTOR)
    struct.pack_into("<I", fat, 0, 0x0FFFFFF8)
    struct.pack_into("<I", fat, 4, EOC)
    for directory_cluster in (2, 3, 4):
        struct.pack_into("<I", fat, directory_cluster * 4, EOC)
    cluster_chain(loader_start, loader_clusters, fat)
    cluster_chain(kernel_start, kernel_clusters, fat)
    cluster_chain(config_start, config_clusters, fat)
    fat_start = RESERVED_SECTORS * SECTOR
    for copy in range(FAT_COUNT):
        start = fat_start + copy * fat_sectors * SECTOR
        image[start : start + len(fat)] = fat

    data_start = (RESERVED_SECTORS + FAT_COUNT * fat_sectors) * SECTOR

    def write_cluster(cluster: int, data: bytes) -> None:
        offset = data_start + (cluster - 2) * SECTORS_PER_CLUSTER * SECTOR
        if len(data) > SECTORS_PER_CLUSTER * SECTOR:
            raise ValueError("directory exceeds one cluster")
        image[offset : offset + len(data)] = data

    volume = bytearray(32)
    volume[0:11] = b"MAKOS      "
    volume[11] = 0x08
    write_cluster(2, bytes(volume) + dir_entry("EFI", 0x10, 3) + dir_entry("KERNEL.ELF", 0x20, kernel_start, len(kernel)) + dir_entry("MAKOS.CFG", 0x20, config_start, len(config)))
    write_cluster(3, dir_entry(".", 0x10, 3) + dir_entry("..", 0x10, 2) + dir_entry("BOOT", 0x10, 4))
    write_cluster(4, dir_entry(".", 0x10, 4) + dir_entry("..", 0x10, 3) + dir_entry(loader_name, 0x20, loader_start, len(loader)))

    def write_file(start_cluster: int, content: bytes) -> None:
        for index in range(math.ceil(len(content) / SECTOR)):
            write_cluster(start_cluster + index, content[index * SECTOR : (index + 1) * SECTOR])

    write_file(loader_start, loader)
    write_file(kernel_start, kernel)
    write_file(config_start, config)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_bytes(image)
    print(f"created {output} ({IMAGE_BYTES // (1024 * 1024)} MiB FAT32)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
