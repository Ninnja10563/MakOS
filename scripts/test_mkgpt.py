#!/usr/bin/env python3
"""Deterministic structural test for the single-disk MakOS GPT builder."""

import pathlib
import struct
import tempfile

import boot_test
import mkgpt


def checked_header(source, lba: int) -> tuple[bytes, tuple[int, ...]]:
    source.seek(lba * mkgpt.SECTOR)
    header = source.read(mkgpt.SECTOR)
    assert header[:8] == b"EFI PART"
    size = struct.unpack_from("<I", header, 12)[0]
    wanted = struct.unpack_from("<I", header, 16)[0]
    cleared = bytearray(header[:size])
    struct.pack_into("<I", cleared, 16, 0)
    assert mkgpt.crc32(cleared) == wanted
    return header, struct.unpack_from("<QQQQ", header, 24)


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="makos-gpt-test-") as temporary:
        root = pathlib.Path(temporary)
        esp = root / "esp.img"
        data = root / "data.img"
        image = root / "system.img"
        esp.write_bytes(b"ESP!" + bytes(mkgpt.SECTOR * 8 - 4))
        data.write_bytes(b"DATA" + bytes(mkgpt.SECTOR * 16 - 4))
        mkgpt.build(image, esp, data)
        with image.open("rb") as source:
            protective = source.read(mkgpt.SECTOR)
            assert protective[450] == 0xEE and protective[510:512] == b"\x55\xaa"
            primary, primary_geometry = checked_header(source, 1)
            backup_lba = primary_geometry[1]
            backup, backup_geometry = checked_header(source, backup_lba)
            assert primary_geometry[0:2] == (1, backup_lba)
            assert backup_geometry[0:2] == (backup_lba, 1)
            entries_lba, count, size, wanted_crc = struct.unpack_from("<QIII", primary, 72)
            source.seek(entries_lba * mkgpt.SECTOR)
            entries = source.read(count * size)
            assert mkgpt.crc32(entries) == wanted_crc
            assert entries[:16] == mkgpt.ESP_TYPE.bytes_le
            assert entries[128:144] == mkgpt.MAKOS_DATA_TYPE.bytes_le
            esp_first, esp_last = struct.unpack_from("<QQ", entries, 32)
            data_first, data_last = struct.unpack_from("<QQ", entries, 128 + 32)
            assert esp_last - esp_first + 1 == 8
            assert data_last - data_first + 1 == 16
            source.seek(esp_first * mkgpt.SECTOR)
            assert source.read(4) == b"ESP!"
            source.seek(data_first * mkgpt.SECTOR)
            assert source.read(4) == b"DATA"
            assert image.stat().st_size == (backup_lba + 1) * mkgpt.SECTOR
        assert boot_test.gpt_data_offset(image) == data_first * mkgpt.SECTOR

        preserved = root / "preserved.img"
        preserved.write_bytes(b"do-not-overwrite")
        invalid_data = root / "invalid-data.img"
        invalid_data.write_bytes(b"unaligned")
        try:
            mkgpt.build(preserved, esp, invalid_data)
        except ValueError:
            pass
        else:
            raise AssertionError("invalid input unexpectedly built an image")
        assert preserved.read_bytes() == b"do-not-overwrite"

        try:
            mkgpt.build(esp, esp, data)
        except ValueError:
            pass
        else:
            raise AssertionError("output/input alias unexpectedly accepted")
    print("MAKOS_GPT_TEST_OK protective_mbr=1 primary=1 backup=1 esp=1 data=1")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
