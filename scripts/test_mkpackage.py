#!/usr/bin/env python3
"""Host format test for MakOS disk-backed package images."""

import pathlib
import struct
import tempfile

import mkpackage


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="makos-package-test-") as temporary:
        base = pathlib.Path(temporary)
        root = base / "tree"
        (root / "defaults").mkdir(parents=True)
        (root / "firefox").write_bytes(b"ELF" + bytes(range(251)) * 9)
        (root / "defaults" / "prefs.js").write_text("pref('makos', true);\n")
        replacement = base / "stripped-firefox"
        replacement.write_bytes(b"STRIPPED")
        image = base / "data.img"
        count, _ = mkpackage.install(
            image, root, "usr/lib/firefox", {"firefox": replacement}
        )
        assert count == 2
        with image.open("rb") as source:
            source.seek(mkpackage.HEADER_LBA * mkpackage.SECTOR)
            header = source.read(mkpackage.SECTOR)
            assert header[:8] == mkpackage.HEADER_MAGIC
            assert mkpackage.PACKAGE_LIMIT_LBA < mkpackage.PROFILE_DATA_LBA
            assert struct.unpack_from("<I", header, 12)[0] == 2
            assert struct.unpack_from("<I", header, 508)[0] == mkpackage.crc32(header[:508])
            paths = []
            payloads = []
            for index in range(count):
                source.seek((mkpackage.ENTRY_LBA + index) * mkpackage.SECTOR)
                entry = source.read(mkpackage.SECTOR)
                assert entry[:8] == mkpackage.ENTRY_MAGIC
                name_length = struct.unpack_from("<H", entry, 8)[0]
                size, first_lba = struct.unpack_from("<QQ", entry, 16)
                paths.append(entry[64 : 64 + name_length])
                source.seek(first_lba * mkpackage.SECTOR)
                payloads.append(source.read(size))
            assert paths == [
                b"/usr/lib/firefox/defaults/prefs.js",
                b"/usr/lib/firefox/firefox",
            ]
            assert payloads[0] == b"pref('makos', true);\n"
            assert payloads[1] == b"STRIPPED"
        marker = b"PERSISTENT-PACKAGE-STATE"
        with image.open("r+b") as output:
            output.seek(mkpackage.PACKAGE_LIMIT_LBA * mkpackage.SECTOR)
            output.write(marker)
        mkpackage.install(image, root, "usr/lib/firefox", {"firefox": replacement})
        with image.open("rb") as source:
            source.seek(mkpackage.PACKAGE_LIMIT_LBA * mkpackage.SECTOR)
            assert source.read(len(marker)) == marker
        oversized = base / "oversized"
        with oversized.open("wb") as output:
            output.truncate(
                (mkpackage.PACKAGE_LIMIT_LBA - mkpackage.DATA_LBA + 1)
                * mkpackage.SECTOR
            )
        try:
            mkpackage.install(image, root, "usr/lib/firefox", {"firefox": oversized})
        except ValueError as error:
            assert "durable transaction region" in str(error)
        else:
            raise AssertionError("legacy payload overlap accepted")
        try:
            mkpackage.install(
                image,
                root,
                "usr/lib/firefox",
                additions=[(b"/packages/evil/payload", replacement)],
            )
        except ValueError as error:
            assert "reserved durable-package path" in str(error)
        else:
            raise AssertionError("reserved durable namespace accepted")
    print("MAKOS_PACKAGE_FORMAT_TEST_OK entries=2 crc=1 sector_stream=1")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
