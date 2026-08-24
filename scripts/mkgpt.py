#!/usr/bin/env python3
"""Build one sparse, bootable MakOS GPT disk from ESP and data images."""

from __future__ import annotations

import argparse
import binascii
import os
import pathlib
import struct
import tempfile
import uuid

SECTOR = 512
ALIGN_LBA = 2048
ENTRY_COUNT = 128
ENTRY_BYTES = 128
ENTRY_SECTORS = ENTRY_COUNT * ENTRY_BYTES // SECTOR
ESP_TYPE = uuid.UUID("c12a7328-f81f-11d2-ba4b-00a0c93ec93b")
MAKOS_DATA_TYPE = uuid.UUID("8d6a8f74-3e33-4d44-a2e7-0f5a4b4f5301")
DISK_GUID = uuid.UUID("f53dbf1b-4a9d-4cb7-9810-4d616b4f5301")
ESP_GUID = uuid.UUID("ee586fc8-2f07-49de-a98c-4d616b455350")
DATA_GUID = uuid.UUID("1551bf02-b62c-4e59-8187-4d616b444154")


def align(value: int, alignment: int = ALIGN_LBA) -> int:
    return (value + alignment - 1) // alignment * alignment


def crc32(data: bytes | bytearray) -> int:
    return binascii.crc32(data) & 0xFFFFFFFF


def partition_entry(
    kind: uuid.UUID,
    unique: uuid.UUID,
    first: int,
    last: int,
    name: str,
) -> bytes:
    encoded_name = name.encode("utf-16-le")
    if len(encoded_name) > 72 or first > last:
        raise ValueError("invalid GPT partition")
    entry = bytearray(ENTRY_BYTES)
    entry[0:16] = kind.bytes_le
    entry[16:32] = unique.bytes_le
    struct.pack_into("<QQQ", entry, 32, first, last, 0)
    entry[56 : 56 + len(encoded_name)] = encoded_name
    return bytes(entry)


def gpt_header(
    current: int,
    backup: int,
    first_usable: int,
    last_usable: int,
    entries_lba: int,
    entries_crc: int,
) -> bytes:
    header = bytearray(SECTOR)
    header[:8] = b"EFI PART"
    struct.pack_into("<IIII", header, 8, 0x00010000, 92, 0, 0)
    struct.pack_into("<QQQQ", header, 24, current, backup, first_usable, last_usable)
    header[56:72] = DISK_GUID.bytes_le
    struct.pack_into("<QIII", header, 72, entries_lba, ENTRY_COUNT, ENTRY_BYTES, entries_crc)
    struct.pack_into("<I", header, 16, crc32(header[:92]))
    return bytes(header)


def copy_sparse(source: pathlib.Path, output, destination_offset: int) -> None:
    output.seek(destination_offset)
    with source.open("rb") as input_file:
        while block := input_file.read(1024 * 1024):
            if block.count(0) == len(block):
                output.seek(len(block), 1)
            else:
                output.write(block)


def build(output_path: pathlib.Path, esp: pathlib.Path, data: pathlib.Path) -> None:
    if not esp.is_file() or not data.is_file():
        raise ValueError("ESP/data input absent")
    if esp.stat().st_size == 0 or esp.stat().st_size % SECTOR:
        raise ValueError("ESP image must be nonempty and sector aligned")
    if data.stat().st_size == 0 or data.stat().st_size % SECTOR:
        raise ValueError("data image must be nonempty and sector aligned")
    if output_path.resolve() in (esp.resolve(), data.resolve()):
        raise ValueError("output must differ from input images")

    esp_sectors = esp.stat().st_size // SECTOR
    data_sectors = data.stat().st_size // SECTOR
    esp_first = ALIGN_LBA
    esp_last = esp_first + esp_sectors - 1
    data_first = align(esp_last + 1)
    data_last = data_first + data_sectors - 1
    backup_entries_lba = data_last + 1
    backup_header_lba = backup_entries_lba + ENTRY_SECTORS
    total_bytes = (backup_header_lba + 1) * SECTOR

    entries = bytearray(ENTRY_COUNT * ENTRY_BYTES)
    entries[0:ENTRY_BYTES] = partition_entry(
        ESP_TYPE, ESP_GUID, esp_first, esp_last, "MakOS ESP"
    )
    entries[ENTRY_BYTES : 2 * ENTRY_BYTES] = partition_entry(
        MAKOS_DATA_TYPE, DATA_GUID, data_first, data_last, "MakOS Data"
    )
    entries_crc = crc32(entries)
    primary = gpt_header(
        1, backup_header_lba, 2 + ENTRY_SECTORS, data_last, 2, entries_crc
    )
    backup = gpt_header(
        backup_header_lba,
        1,
        2 + ENTRY_SECTORS,
        data_last,
        backup_entries_lba,
        entries_crc,
    )
    protective = bytearray(SECTOR)
    protective[446 + 4] = 0xEE
    struct.pack_into("<II", protective, 446 + 8, 1, min(backup_header_lba, 0xFFFFFFFF))
    protective[510:512] = b"\x55\xaa"

    output_path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary_name = tempfile.mkstemp(
        prefix=f".{output_path.name}.", dir=output_path.parent
    )
    try:
        with os.fdopen(descriptor, "w+b") as output:
            output.truncate(total_bytes)
            output.seek(0)
            output.write(protective)
            output.write(primary)
            output.write(entries)
            copy_sparse(esp, output, esp_first * SECTOR)
            copy_sparse(data, output, data_first * SECTOR)
            output.seek(backup_entries_lba * SECTOR)
            output.write(entries)
            output.write(backup)
            output.flush()
            os.fsync(output.fileno())
        os.chmod(temporary_name, 0o644)
        os.replace(temporary_name, output_path)
    except BaseException:
        try:
            os.unlink(temporary_name)
        except FileNotFoundError:
            pass
        raise

    print(
        f"created {output_path} GPT esp_lba={esp_first} data_lba={data_first} "
        f"data_sectors={data_sectors} bytes={total_bytes}"
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=pathlib.Path)
    parser.add_argument("--esp", required=True, type=pathlib.Path)
    parser.add_argument("--data", required=True, type=pathlib.Path)
    args = parser.parse_args()
    try:
        build(args.output, args.esp, args.data)
    except (OSError, ValueError) as error:
        parser.error(str(error))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
