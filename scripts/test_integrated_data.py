#!/usr/bin/env python3
"""Focused offline tests for deterministic integrated-data construction."""

from __future__ import annotations

import hashlib
import pathlib
import struct
import tempfile

import firefox_provenance
import integrate_data_image as integrated
import mkpackage
import verify_firefox_runtime_image as runtime_image


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
    runtime_payloads = {
        "firefox": elf(True),
        "plugin-container": b"runtime-plugin-container",
        "xpcshell": b"runtime-xpcshell",
        "libxul.so": elf(False),
        "libnspr4.so": b"runtime-libnspr4",
    }
    provenance_record = {
        **firefox_provenance.expected_identity(),
        "build_artifacts": {
            name: hashlib.sha256(name.encode()).hexdigest()
            for name in firefox_provenance.BUILD_ARTIFACTS
        },
        "runtime_artifacts": {
            name: hashlib.sha256(payload).hexdigest()
            for name, payload in runtime_payloads.items()
        },
    }
    sources = {
        **{
            f"/usr/lib/firefox/{name}": write(package_root / name, payload)
            for name, payload in runtime_payloads.items()
        },
        "/usr/lib/firefox/omni.ja": write(package_root / "omni.ja", b"omni"),
        "/usr/lib/firefox/application.ini": write(package_root / "application.ini", b"app"),
        firefox_provenance.GUEST_PATH: write(
            package_root / "makos-build-provenance.json",
            firefox_provenance.canonical_bytes(provenance_record),
        ),
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
    assert runtime_image.verify(image) == provenance_record

    stale_record = dict(provenance_record)
    stale_record["patch_series_sha256"] = "0" * 64
    stale_source = write(
        package_root / "stale-provenance.json",
        firefox_provenance.canonical_bytes(stale_record),
    )
    stale_sources = dict(sources)
    stale_sources[firefox_provenance.GUEST_PATH] = stale_source
    stale_image = base / "stale-package.img"
    stale_additions = [
        (guest.encode(), source) for guest, source in stale_sources.items()
    ]
    mkpackage.install(stale_image, package_root, "fixture", additions=stale_additions)
    try:
        integrated.verify_components(stale_image, stale_sources)
    except ValueError as error:
        assert "provenance identity mismatch" in str(error)
    else:
        raise AssertionError("stale Firefox patch provenance passed verification")

    mismatched_sources = dict(sources)
    mismatched_sources["/usr/lib/firefox/plugin-container"] = write(
        package_root / "mismatched-plugin-container", b"stale-runtime-plugin"
    )
    mismatched_image = base / "mismatched-runtime.img"
    mkpackage.install(
        mismatched_image,
        package_root,
        "fixture",
        additions=[
            (guest.encode(), source) for guest, source in mismatched_sources.items()
        ],
    )
    try:
        runtime_image.verify(mismatched_image)
    except ValueError as error:
        assert "differs from provenance" in str(error)
    else:
        raise AssertionError("mismatched Firefox runtime artifact passed preflight")

    legacy_image = base / "legacy-package.img"
    legacy_sources = {
        guest: source
        for guest, source in sources.items()
        if guest != firefox_provenance.GUEST_PATH
    }
    mkpackage.install(
        legacy_image,
        package_root,
        "fixture",
        additions=[(guest.encode(), source) for guest, source in legacy_sources.items()],
    )
    try:
        runtime_image.verify(legacy_image)
    except ValueError as error:
        assert "makos-build-provenance.json" in str(error)
    else:
        raise AssertionError("unprovenanced Firefox runtime image passed preflight")
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
        "crc_rejection=1 elf=aarch64-pie,interp licenses=6 "
        "firefox_provenance=pinned-source,ordered-patches,build-and-runtime-sha256 "
        "stale_image=denied mismatched_runtime=denied pre_qemu=1"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
