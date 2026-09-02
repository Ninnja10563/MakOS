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
import verify_firefox_build_elf as build_elf


def elf(
    interpreter: bool,
    dependencies: tuple[str, ...] = (),
    soname: str | None = None,
    *,
    machine: int = 183,
    elf_type: int = 3,
    entry_point: int | None = None,
) -> bytes:
    phnum = 3 if interpreter else 2
    result = bytearray(1024)
    ident = b"\x7fELF\x02\x01\x01" + bytes(9)
    base_address = 0x400000
    struct.pack_into(
        "<16sHHIQQQIHHHHHH",
        result,
        0,
        ident,
        elf_type,
        machine,
        1,
        (
            entry_point
            if entry_point is not None
            else base_address + 0x100 if interpreter else 0
        ),
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
    strings = bytearray(b"\0")

    def add_string(value: str) -> int:
        offset = len(strings)
        strings.extend(value.encode("ascii") + b"\0")
        return offset

    needed_offsets = [add_string(dependency) for dependency in dependencies]
    soname_offset = add_string(soname) if soname is not None else None
    string_offset = 768
    result[string_offset : string_offset + len(strings)] = strings
    dynamic = [
        (5, base_address + string_offset),
        (10, len(strings)),
        *((1, offset) for offset in needed_offsets),
    ]
    if soname_offset is not None:
        dynamic.append((14, soname_offset))
    dynamic.append((0, 0))
    dynamic_offset = 512
    for index, item in enumerate(dynamic):
        struct.pack_into("<QQ", result, dynamic_offset + index * 16, *item)
    programs = []
    if interpreter:
        offset = 256
        programs.append(
            (
                3,
                4,
                offset,
                base_address + offset,
                base_address + offset,
                len(integrated.INTERPRETER),
                len(integrated.INTERPRETER),
                1,
            )
        )
        result[offset : offset + len(integrated.INTERPRETER)] = integrated.INTERPRETER
    programs.extend(
        (
            (
                1,
                5,
                0,
                base_address,
                base_address,
                len(result),
                len(result),
                4096,
            ),
            (
                2,
                6,
                dynamic_offset,
                base_address + dynamic_offset,
                base_address + dynamic_offset,
                len(dynamic) * 16,
                len(dynamic) * 16,
                8,
            ),
        )
    )
    for index, program in enumerate(programs):
        struct.pack_into("<IIQQQQQQ", result, 64 + index * 56, *program)
    return bytes(result)


def firefox_payloads() -> dict[str, bytes]:
    return {
        name: elf(
            contract.interpreter,
            contract.dependencies,
            contract.soname,
        )
        for name, contract in integrated.FIREFOX_ELF_CONTRACTS.items()
    }


def mutated_elf(payload: bytes, kind: str) -> bytes:
    result = bytearray(payload)

    def program_offset(program_kind: int) -> int:
        phoff = struct.unpack_from("<Q", result, 32)[0]
        phentsize, phnum = struct.unpack_from("<HH", result, 54)
        matches = [
            phoff + index * phentsize
            for index in range(phnum)
            if struct.unpack_from("<I", result, phoff + index * phentsize)[0]
            == program_kind
        ]
        if len(matches) != 1:
            raise AssertionError(f"expected one program kind {program_kind}: {matches}")
        return matches[0]

    if kind == "corrupt":
        result[0] ^= 0xFF
    elif kind == "wrong-machine":
        struct.pack_into("<H", result, 18, 62)
    elif kind == "wrong-type":
        struct.pack_into("<H", result, 16, 2)
    elif kind == "zero-entry":
        struct.pack_into("<Q", result, 24, 0)
    elif kind == "load-max-size":
        load = program_offset(1)
        struct.pack_into(
            "<QQ",
            result,
            load + 32,
            integrated.UINT64_MAX,
            integrated.UINT64_MAX,
        )
    elif kind == "load-filesz-over-memsz":
        load = program_offset(1)
        file_size = struct.unpack_from("<Q", result, load + 32)[0]
        struct.pack_into("<Q", result, load + 40, file_size - 1)
    elif kind == "load-vaddr-overflow":
        load = program_offset(1)
        struct.pack_into("<Q", result, load + 16, integrated.UINT64_MAX)
    elif kind == "dynamic-file-range":
        dynamic = program_offset(2)
        struct.pack_into("<Q", result, dynamic + 8, len(result) - 8)
    elif kind == "dynamic-unmapped-vaddr":
        dynamic = program_offset(2)
        address = struct.unpack_from("<Q", result, dynamic + 16)[0]
        struct.pack_into("<Q", result, dynamic + 16, address + 1)
    elif kind == "dynamic-outside-load":
        load = program_offset(1)
        dynamic = program_offset(2)
        load_offset = struct.unpack_from("<Q", result, load + 8)[0]
        dynamic_offset = struct.unpack_from("<Q", result, dynamic + 8)[0]
        dynamic_size = struct.unpack_from("<Q", result, dynamic + 32)[0]
        struct.pack_into(
            "<Q",
            result,
            load + 32,
            dynamic_offset - load_offset + dynamic_size - 1,
        )
    elif kind == "dynamic-filesz-over-memsz":
        dynamic = program_offset(2)
        file_size = struct.unpack_from("<Q", result, dynamic + 32)[0]
        struct.pack_into("<Q", result, dynamic + 40, file_size - 1)
    elif kind == "dynamic-vaddr-overflow":
        dynamic = program_offset(2)
        struct.pack_into("<Q", result, dynamic + 16, integrated.UINT64_MAX)
    elif kind == "interp-huge-size":
        interpreter = program_offset(3)
        struct.pack_into("<Q", result, interpreter + 32, integrated.UINT64_MAX)
    elif kind == "interp-after-load":
        interpreter = program_offset(3)
        load = program_offset(1)
        first = bytes(result[interpreter : interpreter + 56])
        second = bytes(result[load : load + 56])
        result[interpreter : interpreter + 56] = second
        result[load : load + 56] = first
    else:
        raise AssertionError(kind)
    return bytes(result)


COMMON_ELF_FAILURES = (
    ("corrupt", "not ELF64 little-endian"),
    ("wrong-machine", "not AArch64 ET_DYN"),
    ("wrong-type", "not AArch64 ET_DYN"),
    ("load-max-size", "PT_LOAD file range invalid"),
    ("load-filesz-over-memsz", "PT_LOAD filesz exceeds memsz"),
    ("load-vaddr-overflow", "PT_LOAD virtual range overflows"),
    ("dynamic-file-range", "PT_DYNAMIC file range invalid"),
    ("dynamic-unmapped-vaddr", "PT_DYNAMIC is not consistently mapped"),
    ("dynamic-outside-load", "PT_DYNAMIC is not consistently mapped"),
    ("dynamic-filesz-over-memsz", "PT_DYNAMIC filesz exceeds memsz"),
    ("dynamic-vaddr-overflow", "PT_DYNAMIC virtual range overflows"),
)

EXECUTABLE_ELF_FAILURES = (
    ("zero-entry", "executable entry point is zero"),
    ("interp-huge-size", "PT_INTERP size invalid"),
    ("interp-after-load", "PT_INTERP must precede PT_LOAD"),
)


def expect_elf_failure(action, fragment: str) -> None:
    try:
        action()
    except ValueError as error:
        assert fragment in str(error), error
    else:
        raise AssertionError(f"expected ELF verification failure: {fragment}")


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
    runtime_payloads = firefox_payloads()
    provenance_record = {
        **firefox_provenance.expected_identity(),
        "source_tree": "1" * 40,
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
    mismatched_plugin = bytearray(runtime_payloads["plugin-container"])
    mismatched_plugin[-1] = 1
    mismatched_sources["/usr/lib/firefox/plugin-container"] = write(
        package_root / "mismatched-plugin-container", bytes(mismatched_plugin)
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


def test_build_elf_verification(base: pathlib.Path) -> None:
    bin_dir = base / "firefox-bin"
    payloads = firefox_payloads()
    for name, payload in payloads.items():
        write(bin_dir / name, payload)
    build_elf.verify(bin_dir)

    for name, original in payloads.items():
        path = bin_dir / name
        contract = integrated.FIREFOX_ELF_CONTRACTS[name]
        failures = COMMON_ELF_FAILURES
        if contract.interpreter:
            failures += EXECUTABLE_ELF_FAILURES
        for kind, fragment in failures:
            path.write_bytes(mutated_elf(original, kind))
            expect_elf_failure(lambda: build_elf.verify(bin_dir), fragment)
        path.write_bytes(
            elf(
                not contract.interpreter,
                contract.dependencies,
                contract.soname,
                entry_point=0x400100 if contract.interpreter else None,
            )
        )
        expect_elf_failure(
            lambda: build_elf.verify(bin_dir),
            "wrong/missing MakOS musl PT_INTERP"
            if contract.interpreter
            else "shared library unexpectedly has PT_INTERP",
        )
        path.write_bytes(elf(contract.interpreter, (), contract.soname))
        expect_elf_failure(
            lambda: build_elf.verify(bin_dir), "required dependencies absent"
        )
        path.write_bytes(original)
    for name in ("libxul.so", "libnspr4.so"):
        contract = integrated.FIREFOX_ELF_CONTRACTS[name]
        assert contract.soname == name
        path = bin_dir / name
        original = payloads[name]
        path.write_bytes(elf(False, contract.dependencies, "wrong.so"))
        expect_elf_failure(lambda: build_elf.verify(bin_dir), "SONAME mismatch")
        path.write_bytes(original)
    build_elf.verify(bin_dir)


def preflight_image(
    base: pathlib.Path, label: str, payloads: dict[str, bytes]
) -> pathlib.Path:
    package_root = base / f"preflight-{label}"
    provenance_record = {
        **firefox_provenance.expected_identity(),
        "source_tree": "2" * 40,
        "build_artifacts": {
            name: hashlib.sha256(("build-" + name).encode()).hexdigest()
            for name in firefox_provenance.BUILD_ARTIFACTS
        },
        "runtime_artifacts": {
            name: hashlib.sha256(payload).hexdigest()
            for name, payload in payloads.items()
        },
    }
    additions = [
        (f"/usr/lib/firefox/{name}".encode(), write(package_root / name, payload))
        for name, payload in payloads.items()
    ]
    additions.append(
        (
            firefox_provenance.GUEST_PATH.encode(),
            write(
                package_root / "makos-build-provenance.json",
                firefox_provenance.canonical_bytes(provenance_record),
            ),
        )
    )
    image = base / f"preflight-{label}.img"
    mkpackage.install(image, package_root, "fixture", additions=additions)
    return image


def test_preflight_rejects_self_hashed_invalid_elf(base: pathlib.Path) -> None:
    valid = firefox_payloads()
    runtime_image.verify(preflight_image(base, "valid", valid))
    for name, original in valid.items():
        safe_name = name.replace(".", "-")
        contract = integrated.FIREFOX_ELF_CONTRACTS[name]
        failures = COMMON_ELF_FAILURES
        if contract.interpreter:
            failures += EXECUTABLE_ELF_FAILURES
        for kind, fragment in failures:
            payloads = dict(valid)
            payloads[name] = mutated_elf(original, kind)
            image = preflight_image(base, f"{safe_name}-{kind}", payloads)
            expect_elf_failure(lambda: runtime_image.verify(image), fragment)
        payloads = dict(valid)
        payloads[name] = elf(
            not contract.interpreter,
            contract.dependencies,
            contract.soname,
            entry_point=0x400100 if contract.interpreter else None,
        )
        image = preflight_image(base, f"{safe_name}-wrong-interp", payloads)
        expect_elf_failure(
            lambda: runtime_image.verify(image),
            "wrong/missing MakOS musl PT_INTERP"
            if contract.interpreter
            else "shared library unexpectedly has PT_INTERP",
        )
        payloads = dict(valid)
        payloads[name] = elf(contract.interpreter, (), contract.soname)
        image = preflight_image(base, f"{safe_name}-missing-deps", payloads)
        expect_elf_failure(
            lambda: runtime_image.verify(image), "required dependencies absent"
        )
    for name in ("libxul.so", "libnspr4.so"):
        contract = integrated.FIREFOX_ELF_CONTRACTS[name]
        assert contract.soname == name
        payloads = dict(valid)
        payloads[name] = elf(False, contract.dependencies, "wrong.so")
        safe_name = name.replace(".", "-")
        image = preflight_image(base, f"{safe_name}-wrong-soname", payloads)
        expect_elf_failure(lambda: runtime_image.verify(image), "SONAME mismatch")


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="makos-integrated-test-") as name:
        base = pathlib.Path(name)
        test_preserved_clone(base)
        test_package_verification(base)
        test_build_elf_verification(base)
        test_preflight_rejects_self_hashed_invalid_elf(base)
    print(
        "MAKOS_INTEGRATED_DATA_TEST_OK deterministic_package_zero=1 "
        "preserved_hashes=filesystem-metadata,account-profile "
        "crc_rejection=1 elf=all-five-aarch64-et-dyn,interp-by-kind,deps-by-artifact "
        "firefox_provenance=pinned-source,ordered-patched-tree,build-and-runtime-sha256 "
        "stale_image=denied mismatched_runtime=denied "
        "self_hashed_invalid_elf=corrupt,wrong-machine,wrong-type,ranges,entry,"
        "interp-order,interp-size,deps,soname "
        "pre_qemu=1"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
