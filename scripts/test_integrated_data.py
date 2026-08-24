#!/usr/bin/env python3
"""Focused offline tests for deterministic integrated-data construction."""

from __future__ import annotations

import pathlib
import struct
import tempfile

import integrate_data_image as integrated
import mkpackage


def elf(interpreter: bool) -> bytes:
    phnum = 2 if interpreter else 1
    result = bytearray(512)
    ident = b"\x7fELF\x02\x01\x01" + bytes(9)
    struct.pack_into(
        "<16sHHIQQQIHHHHHH",
        result,
        0,
        ident,
        3,
        183,
        1,
        0,
        64,
        0,
        0,
        64,
        56,
        phnum,
        0,
        0,
        0,
    )
    struct.pack_into("<IIQQQQQQ", result, 64, 1, 5, 0, 0, 0, 512, 512, 4096)
    if interpreter:
        offset = 256
        struct.pack_into(
            "<IIQQQQQQ",
            result,
            120,
            3,
            4,
            offset,
            0,
            0,
            len(integrated.INTERPRETER),
            len(integrated.INTERPRETER),
            1,
        )
        result[offset : offset + len(integrated.INTERPRETER)] = integrated.INTERPRETER
    return bytes(result)


def write(path: pathlib.Path, data: bytes) -> pathlib.Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    return path


def test_preserved_clone(base: pathlib.Path) -> None:
    source = base / "small-source.img"
    output = base / "small-output.img"
    image_bytes, package_start, profile_start = 1024 * 1024, 64 * 1024, 512 * 1024
    with source.open("wb") as stream:
        stream.truncate(image_bytes)
        stream.seek(17)
        stream.write(b"account-metadata")
        stream.seek(package_start + 99)
        stream.write(b"stale-package-must-disappear")
        stream.seek(profile_start + 777)
        stream.write(b"profile-state")
    before = integrated.preservation_hashes(
        source, image_bytes, package_start, profile_start
    )
    integrated.clone_preserved_regions(
        source, output, image_bytes, package_start, profile_start
    )
    assert integrated.preservation_hashes(
        output, image_bytes, package_start, profile_start
    ) == before
    with output.open("rb") as stream:
        stream.seek(package_start)
        assert stream.read(profile_start - package_start) == bytes(
            profile_start - package_start
        )


def test_package_verification(base: pathlib.Path) -> None:
    package_root = base / "package-root"
    sources = {
        "/usr/lib/firefox/firefox": write(package_root / "firefox", elf(True)),
        "/usr/lib/firefox/libxul.so": write(package_root / "libxul.so", elf(False)),
        "/usr/lib/firefox/omni.ja": write(package_root / "omni.ja", b"omni"),
        "/usr/lib/firefox/application.ini": write(package_root / "application.ini", b"app"),
        "/usr/lib/firefox/licenses/LICENSE": write(package_root / "LICENSE", b"MPL license\n" * 20),
        "/usr/lib/firefox/licenses/license.html": write(package_root / "license.html", b"Mozilla licenses\n" * 20),
        "/fonts/LICENSE-MPLUS.txt": write(package_root / "font-license", b"M+ font license\n" * 20),
        "/usr/bin/nano": write(package_root / "nano", elf(True)),
        "/usr/share/terminfo/m/makos": write(package_root / "terminfo", b"terminfo"),
        "/usr/share/licenses/nano/COPYING": write(package_root / "nano-license", b"GPL license\n" * 20),
        "/usr/share/licenses/ncurses/COPYING": write(package_root / "ncurses-license", b"ncurses license\n" * 20),
        "/usr/bin/python3": write(package_root / "python3", elf(True)),
        "/usr/lib/python314.zip": write(package_root / "python314.zip", b"zip"),
        "/usr/share/licenses/cpython/LICENSE": write(package_root / "python-license", b"Python license\n" * 20),
    }
    seed = write(package_root / "seed", b"seed")
    image = base / "package.img"
    additions = [(guest.encode(), source) for guest, source in sources.items()]
    mkpackage.install(image, package_root, "fixture", additions=additions)
    entries, tree = integrated.verify_components(image, sources)
    assert len(entries) == len(sources) + sum(
        path.is_file() for path in package_root.rglob("*")
    )
    assert len(tree) == 64
    with image.open("r+b") as stream:
        stream.seek(entries["/usr/bin/python3"].offset)
        byte = stream.read(1)
        stream.seek(entries["/usr/bin/python3"].offset)
        stream.write(bytes([byte[0] ^ 1]))
    try:
        integrated.verify_components(image, sources)
    except ValueError as error:
        assert "CRC mismatch" in str(error)
    else:
        raise AssertionError("corrupted package passed verification")
    assert seed.is_file()


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="makos-integrated-test-") as name:
        base = pathlib.Path(name)
        test_preserved_clone(base)
        test_package_verification(base)
    print(
        "MAKOS_INTEGRATED_DATA_TEST_OK deterministic_package_zero=1 "
        "preserved_hashes=filesystem-metadata,account-profile "
        "crc_rejection=1 elf=aarch64-pie,interp licenses=6"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
