#!/usr/bin/env python3
"""Build a content-addressed MakOS data image without touching user state."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
import shutil
import struct
import subprocess
import sys
import tempfile
from dataclasses import dataclass

import firefox_provenance
import mkpackage
import verify_package


ROOT = pathlib.Path(__file__).resolve().parent.parent
CHUNK = 1024 * 1024
PACKAGE_START = mkpackage.HEADER_LBA * mkpackage.SECTOR
PROFILE_START = mkpackage.PROFILE_DATA_LBA * mkpackage.SECTOR
INTERPRETER = b"/lib/ld-musl-aarch64.so.1\0"
MAX_ELF_PROGRAM_HEADERS = 128
MAX_ELF_DYNAMIC_BYTES = 64 * 1024
MAX_ELF_DYNAMIC_STRING_BYTES = 64 * 1024 * 1024
UINT64_MAX = (1 << 64) - 1


@dataclass(frozen=True)
class Entry:
    path: str
    size: int
    offset: int
    sha256: str


@dataclass(frozen=True)
class FirefoxElfContract:
    interpreter: bool
    dependencies: tuple[str, ...]
    soname: str | None = None


FIREFOX_ELF_CONTRACTS = {
    "firefox": FirefoxElfContract(True, ("libc.so",)),
    "plugin-container": FirefoxElfContract(True, ("libc.so", "libxul.so")),
    "xpcshell": FirefoxElfContract(True, ("libc.so", "libxul.so")),
    "libxul.so": FirefoxElfContract(
        False, ("libnss3.so", "libssl3.so"), "libxul.so"
    ),
    "libnspr4.so": FirefoxElfContract(False, ("libc.so",), "libnspr4.so"),
}


def sha256_path(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while block := source.read(CHUNK):
            digest.update(block)
    return digest.hexdigest()


def hash_range(path: pathlib.Path, start: int, end: int) -> str:
    digest = hashlib.sha256()
    remaining = end - start
    with path.open("rb") as source:
        source.seek(start)
        while remaining:
            block = source.read(min(CHUNK, remaining))
            if not block:
                raise OSError(f"short read in preserved range: {path}")
            digest.update(block)
            remaining -= len(block)
    return digest.hexdigest()


def preservation_hashes(
    path: pathlib.Path,
    image_bytes: int = mkpackage.IMAGE_BYTES,
    package_start: int = PACKAGE_START,
    profile_start: int = PROFILE_START,
) -> dict[str, str]:
    return {
        "filesystem_metadata_sha256": hash_range(path, 0, package_start),
        "account_profile_sha256": hash_range(path, profile_start, image_bytes),
    }


def _copy_range_sparse(
    source, output, start: int, end: int, zero: bytes
) -> None:
    source.seek(start)
    output.seek(start)
    remaining = end - start
    while remaining:
        block = source.read(min(len(zero), remaining))
        if not block:
            raise OSError("short source image read")
        if block != zero[: len(block)]:
            output.write(block)
        else:
            output.seek(len(block), os.SEEK_CUR)
        remaining -= len(block)


def clone_preserved_regions(
    source_path: pathlib.Path,
    output_path: pathlib.Path,
    image_bytes: int = mkpackage.IMAGE_BYTES,
    package_start: int = PACKAGE_START,
    profile_start: int = PROFILE_START,
) -> None:
    """Clone only mutable regions; package-owned bytes begin as deterministic zeroes."""
    if not 0 < package_start < profile_start < image_bytes:
        raise ValueError("invalid data-image region layout")
    if source_path.stat().st_size != image_bytes:
        raise ValueError(
            f"source image must be exactly {image_bytes} bytes: {source_path}"
        )
    zero = bytes(CHUNK)
    with source_path.open("rb") as source, output_path.open("w+b") as output:
        output.truncate(image_bytes)
        _copy_range_sparse(source, output, 0, package_start, zero)
        _copy_range_sparse(source, output, profile_start, image_bytes, zero)
        output.flush()
        os.fsync(output.fileno())


def package_entries(image: pathlib.Path) -> dict[str, Entry]:
    verify_package.verify(image)
    result: dict[str, Entry] = {}
    occupied: list[tuple[int, int, str]] = []
    with image.open("rb") as source:
        source.seek(mkpackage.HEADER_LBA * mkpackage.SECTOR)
        header = source.read(mkpackage.SECTOR)
        count = struct.unpack_from("<I", header, 12)[0]
        entry_lba = struct.unpack_from("<Q", header, 16)[0]
        for index in range(count):
            source.seek((entry_lba + index) * mkpackage.SECTOR)
            raw = source.read(mkpackage.SECTOR)
            path_len = struct.unpack_from("<H", raw, 8)[0]
            size, first_lba = struct.unpack_from("<QQ", raw, 16)
            path = raw[64 : 64 + path_len].decode("utf-8", "strict")
            offset = first_lba * mkpackage.SECTOR
            digest = hashlib.sha256()
            source.seek(offset)
            remaining = size
            while remaining:
                block = source.read(min(CHUNK, remaining))
                if not block:
                    raise ValueError(f"short package payload: {path}")
                digest.update(block)
                remaining -= len(block)
            result[path] = Entry(path, size, offset, digest.hexdigest())
            occupied.append((offset, offset + size, path))
    for previous, current in zip(sorted(occupied), sorted(occupied)[1:]):
        if previous[1] > current[0]:
            raise ValueError(f"overlapping package payloads: {previous[2]}, {current[2]}")
    return result


def read_entry_slice(
    image: pathlib.Path, entry: Entry, offset: int, length: int
) -> bytes:
    if offset < 0 or length < 0 or offset + length > entry.size:
        raise ValueError(f"payload slice out of bounds: {entry.path}")
    with image.open("rb") as source:
        source.seek(entry.offset + offset)
        data = source.read(length)
    if len(data) != length:
        raise ValueError(f"short package payload: {entry.path}")
    return data


def verify_aarch64_elf(
    image: pathlib.Path,
    entry: Entry,
    require_interpreter: bool,
    required_dependencies: tuple[str, ...] = (),
    required_soname: str | None = None,
) -> None:
    header = read_entry_slice(image, entry, 0, 64)
    if header[:7] != b"\x7fELF\x02\x01\x01":
        raise ValueError(f"not ELF64 little-endian: {entry.path}")
    elf_type, machine = struct.unpack_from("<HH", header, 16)
    entry_point = struct.unpack_from("<Q", header, 24)[0]
    phoff = struct.unpack_from("<Q", header, 32)[0]
    phentsize, phnum = struct.unpack_from("<HH", header, 54)
    if (
        elf_type != 3
        or machine != 183
        or phentsize < 56
        or not 0 < phnum <= MAX_ELF_PROGRAM_HEADERS
    ):
        raise ValueError(f"not AArch64 ET_DYN with program headers: {entry.path}")
    if require_interpreter and entry_point == 0:
        raise ValueError(f"ELF executable entry point is zero: {entry.path}")
    load_segments: list[tuple[int, int, int, int]] = []
    dynamic_segments: list[tuple[int, int, int, int]] = []
    interpreter = None
    interpreter_seen = False
    for index in range(phnum):
        program = read_entry_slice(image, entry, phoff + index * phentsize, 56)
        kind = struct.unpack_from("<I", program, 0)[0]
        file_offset, virtual_address = struct.unpack_from("<QQ", program, 8)
        file_size, memory_size = struct.unpack_from("<QQ", program, 32)
        if kind == 1:
            if file_offset > entry.size or file_size > entry.size - file_offset:
                raise ValueError(f"ELF PT_LOAD file range invalid: {entry.path}")
            if file_size > memory_size:
                raise ValueError(f"ELF PT_LOAD filesz exceeds memsz: {entry.path}")
            if virtual_address > UINT64_MAX - memory_size:
                raise ValueError(f"ELF PT_LOAD virtual range overflows: {entry.path}")
            load_segments.append(
                (virtual_address, file_offset, file_size, memory_size)
            )
        elif kind == 2:
            if file_offset > entry.size or file_size > entry.size - file_offset:
                raise ValueError(f"ELF PT_DYNAMIC file range invalid: {entry.path}")
            if file_size > memory_size:
                raise ValueError(f"ELF PT_DYNAMIC filesz exceeds memsz: {entry.path}")
            if virtual_address > UINT64_MAX - memory_size:
                raise ValueError(f"ELF PT_DYNAMIC virtual range overflows: {entry.path}")
            dynamic_segments.append(
                (virtual_address, file_offset, file_size, memory_size)
            )
        elif kind == 3:
            if interpreter_seen:
                raise ValueError(f"ELF has multiple PT_INTERP entries: {entry.path}")
            interpreter_seen = True
            if not require_interpreter:
                raise ValueError(
                    f"shared library unexpectedly has PT_INTERP: {entry.path}"
                )
            if load_segments:
                raise ValueError(
                    f"ELF PT_INTERP must precede PT_LOAD: {entry.path}"
                )
            if file_size != len(INTERPRETER):
                raise ValueError(f"ELF PT_INTERP size invalid: {entry.path}")
            interpreter = read_entry_slice(image, entry, file_offset, file_size)
    if not load_segments:
        raise ValueError(f"ELF has no PT_LOAD: {entry.path}")
    if require_interpreter and interpreter != INTERPRETER:
        raise ValueError(f"wrong/missing MakOS musl PT_INTERP: {entry.path}")

    dependencies: set[str] = set()
    soname = None
    if required_dependencies or required_soname is not None:
        if len(dynamic_segments) != 1:
            raise ValueError(f"ELF must have one PT_DYNAMIC: {entry.path}")
        (
            dynamic_address,
            dynamic_offset,
            dynamic_size,
            dynamic_memory_size,
        ) = dynamic_segments[0]
        if (
            dynamic_size == 0
            or dynamic_size > MAX_ELF_DYNAMIC_BYTES
            or dynamic_size % 16
        ):
            raise ValueError(f"ELF PT_DYNAMIC size invalid: {entry.path}")
        dynamic_mapped = False
        for (
            load_address,
            load_offset,
            load_file_size,
            load_memory_size,
        ) in load_segments:
            if dynamic_offset < load_offset or dynamic_address < load_address:
                continue
            file_delta = dynamic_offset - load_offset
            virtual_delta = dynamic_address - load_address
            if (
                file_delta == virtual_delta
                and file_delta <= load_file_size
                and dynamic_size <= load_file_size - file_delta
                and virtual_delta <= load_memory_size
                and dynamic_memory_size <= load_memory_size - virtual_delta
            ):
                dynamic_mapped = True
                break
        if not dynamic_mapped:
            raise ValueError(
                f"ELF PT_DYNAMIC is not consistently mapped by PT_LOAD: {entry.path}"
            )
        needed_offsets: list[int] = []
        soname_offset = None
        string_address = None
        string_size = None
        terminated = False
        for offset in range(0, dynamic_size, 16):
            tag, value = struct.unpack(
                "<QQ", read_entry_slice(image, entry, dynamic_offset + offset, 16)
            )
            if tag == 0:
                terminated = True
                break
            if tag == 1:
                needed_offsets.append(value)
            elif tag == 5:
                if string_address is not None:
                    raise ValueError(f"ELF has duplicate DT_STRTAB: {entry.path}")
                string_address = value
            elif tag == 10:
                if string_size is not None:
                    raise ValueError(f"ELF has duplicate DT_STRSZ: {entry.path}")
                string_size = value
            elif tag == 14:
                if soname_offset is not None:
                    raise ValueError(f"ELF has duplicate DT_SONAME: {entry.path}")
                soname_offset = value
        if not terminated:
            raise ValueError(f"ELF PT_DYNAMIC lacks DT_NULL: {entry.path}")
        if (
            string_address is None
            or string_size is None
            or not 0 < string_size <= MAX_ELF_DYNAMIC_STRING_BYTES
        ):
            raise ValueError(f"ELF dynamic string table absent: {entry.path}")
        string_offset = None
        for virtual_address, file_offset, file_size, _ in load_segments:
            if (
                virtual_address <= string_address
                and string_address - virtual_address <= file_size
                and string_size <= file_size - (string_address - virtual_address)
            ):
                string_offset = file_offset + (string_address - virtual_address)
                break
        if string_offset is None:
            raise ValueError(f"ELF dynamic string table is not file-backed: {entry.path}")
        strings = read_entry_slice(image, entry, string_offset, string_size)

        def dynamic_string(offset: int, kind: str) -> str:
            if offset >= len(strings):
                raise ValueError(f"ELF {kind} offset out of bounds: {entry.path}")
            end = strings.find(b"\0", offset)
            if end < 0:
                raise ValueError(f"ELF {kind} is unterminated: {entry.path}")
            try:
                return strings[offset:end].decode("ascii")
            except UnicodeDecodeError as error:
                raise ValueError(f"ELF {kind} is not ASCII: {entry.path}") from error

        dependencies = {
            dynamic_string(offset, "DT_NEEDED") for offset in needed_offsets
        }
        if soname_offset is not None:
            soname = dynamic_string(soname_offset, "DT_SONAME")

    missing_dependencies = sorted(set(required_dependencies) - dependencies)
    if missing_dependencies:
        raise ValueError(
            f"ELF required dependencies absent: {entry.path}: "
            + ", ".join(missing_dependencies)
        )
    if required_soname is not None and soname != required_soname:
        raise ValueError(
            f"ELF SONAME mismatch: {entry.path}: "
            f"expected={required_soname} observed={soname}"
        )


def verify_firefox_elf_entries(
    image: pathlib.Path, entries: dict[str, Entry]
) -> None:
    for name, contract in FIREFOX_ELF_CONTRACTS.items():
        guest = f"/usr/lib/firefox/{name}"
        entry = entries.get(guest)
        if entry is None:
            raise ValueError(f"Firefox runtime ELF absent: {guest}")
        verify_aarch64_elf(
            image,
            entry,
            contract.interpreter,
            contract.dependencies,
            contract.soname,
        )


def expected_sources() -> dict[str, pathlib.Path]:
    obj = pathlib.Path(
        os.environ.get(
            "MAKOS_FIREFOX_OBJ", ROOT / "build/ports/firefox/obj-aarch64-makos"
        )
    )
    dist = pathlib.Path(os.environ.get("MAKOS_FIREFOX_DIST", obj / "dist/firefox"))
    stripped = pathlib.Path(
        os.environ.get(
            "MAKOS_FIREFOX_LIBXUL",
            ROOT / "build/ports/firefox/package-aarch64-makos/libxul.so",
        )
    )
    return {
        "/usr/lib/firefox/firefox": dist / "firefox",
        "/usr/lib/firefox/libxul.so": stripped,
        "/usr/lib/firefox/omni.ja": dist / "omni.ja",
        "/usr/lib/firefox/application.ini": dist / "application.ini",
        firefox_provenance.GUEST_PATH: dist / "makos-build-provenance.json",
        "/usr/lib/firefox/licenses/LICENSE": dist / "licenses/LICENSE",
        "/usr/lib/firefox/licenses/license.html": dist / "licenses/license.html",
        "/fonts/LICENSE-MPLUS.txt": ROOT / "build/ports/firefox/source/layout/reftests/fonts/mplus/mplus-license.txt",
        "/usr/bin/nano": ROOT / "build/ports/nano/makos/src/nano",
        "/usr/share/terminfo/m/makos": ROOT / "build/ports/ncurses/stage/usr/share/terminfo/m/makos",
        "/usr/share/licenses/nano/COPYING": ROOT / "build/ports/nano/makos/COPYING",
        "/usr/share/licenses/ncurses/COPYING": ROOT / "build/ports/ncurses/stage/COPYING.ncurses",
        "/usr/bin/python3": ROOT / "build/ports/cpython/package/usr/bin/python3",
        "/usr/lib/python314.zip": ROOT / "build/ports/cpython/package/usr/lib/python314.zip",
        "/usr/share/licenses/cpython/LICENSE": ROOT / "build/ports/cpython/package/usr/share/licenses/cpython/LICENSE",
    }


def stage_firefox_licenses() -> None:
    obj = pathlib.Path(
        os.environ.get(
            "MAKOS_FIREFOX_OBJ", ROOT / "build/ports/firefox/obj-aarch64-makos"
        )
    )
    dist = pathlib.Path(os.environ.get("MAKOS_FIREFOX_DIST", obj / "dist/firefox"))
    source = ROOT / "build/ports/firefox/source"
    license_dir = dist / "licenses"
    wanted = {
        license_dir / "LICENSE": source / "LICENSE",
        license_dir / "license.html": source / "toolkit/content/license.html",
    }
    for destination, origin in wanted.items():
        if not origin.is_file():
            raise FileNotFoundError(f"Firefox license source absent: {origin}")
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(origin, destination)


def ensure_nano_stage() -> None:
    required = (
        ROOT / "build/ports/nano/makos/src/nano",
        ROOT / "build/ports/nano/makos/COPYING",
        ROOT / "build/ports/ncurses/stage/usr/share/terminfo/m/makos",
        ROOT / "build/ports/ncurses/stage/COPYING.ncurses",
    )
    if not all(path.is_file() for path in required):
        subprocess.run([str(ROOT / "ports/nano/build-makos.sh")], cwd=ROOT, check=True)
    missing = [str(path) for path in required if not path.is_file()]
    if missing:
        raise FileNotFoundError(f"GNU nano/ncurses stage incomplete: {', '.join(missing)}")


def verify_components(
    image: pathlib.Path, expected: dict[str, pathlib.Path]
) -> tuple[dict[str, Entry], str]:
    entries = package_entries(image)
    missing = sorted(set(expected) - set(entries))
    if missing:
        raise ValueError(f"required integrated paths absent: {', '.join(missing)}")
    for guest, source in expected.items():
        if not source.is_file():
            raise FileNotFoundError(f"required source artifact absent: {source}")
        if entries[guest].sha256 != sha256_path(source):
            raise ValueError(f"packaged payload differs from source artifact: {guest}")
    verify_firefox_provenance(image, entries)
    verify_firefox_elf_entries(image, entries)
    for guest in ("/usr/bin/nano", "/usr/bin/python3"):
        verify_aarch64_elf(image, entries[guest], True)
    for guest in (
        "/usr/lib/firefox/licenses/LICENSE",
        "/usr/lib/firefox/licenses/license.html",
        "/fonts/LICENSE-MPLUS.txt",
        "/usr/share/licenses/nano/COPYING",
        "/usr/share/licenses/ncurses/COPYING",
        "/usr/share/licenses/cpython/LICENSE",
    ):
        if entries[guest].size < 100:
            raise ValueError(f"empty/truncated packaged license: {guest}")
    tree = hashlib.sha256()
    for guest, entry in sorted(entries.items()):
        tree.update(guest.encode() + b"\0")
        tree.update(struct.pack("<Q", entry.size))
        tree.update(bytes.fromhex(entry.sha256))
    return entries, tree.hexdigest()


def verify_firefox_provenance(
    image: pathlib.Path, entries: dict[str, Entry]
) -> dict:
    entry = entries.get(firefox_provenance.GUEST_PATH)
    if entry is None:
        raise ValueError("Firefox build provenance absent from package")
    if entry.size > firefox_provenance.MAX_RECORD_BYTES:
        raise ValueError("Firefox build provenance exceeds bounded size")
    record = firefox_provenance.parse_record(
        read_entry_slice(image, entry, 0, entry.size)
    )
    runtime_hashes = firefox_provenance.validate_runtime_record(record)
    for name, expected_hash in runtime_hashes.items():
        guest = f"/usr/lib/firefox/{name}"
        runtime_entry = entries.get(guest)
        if runtime_entry is None:
            raise ValueError(f"Firefox proven runtime artifact absent: {guest}")
        if runtime_entry.sha256 != expected_hash:
            raise ValueError(
                f"Firefox runtime artifact differs from provenance: {guest}"
            )
    return record


def lock_value(path: pathlib.Path, key: str) -> str:
    prefix = key + "="
    for line in path.read_text().splitlines():
        if line.startswith(prefix):
            return line[len(prefix) :]
    raise ValueError(f"{key} absent from {path}")


def component_versions() -> dict[str, str]:
    versions = {
        "firefox_esr": lock_value(ROOT / "ports/firefox/source.lock", "FIREFOX_VERSION"),
        "gnu_nano": lock_value(ROOT / "ports/nano/source.lock", "NANO_VERSION"),
        "ncurses": lock_value(ROOT / "ports/ncurses/source.lock", "NCURSES_VERSION"),
        "cpython": lock_value(ROOT / "ports/cpython/source.lock", "CPYTHON_VERSION"),
    }
    wanted = {
        "firefox_esr": "140.13.0esr",
        "gnu_nano": "9.1",
        "ncurses": "6.5",
        "cpython": "3.14.7",
    }
    if versions != wanted:
        raise ValueError(f"unexpected integrated component versions: {versions}")
    return versions


def write_manifest(path: pathlib.Path, manifest: dict) -> None:
    temporary = path.with_name(f".{path.name}.tmp")
    temporary.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    os.replace(temporary, path)


def build(source: pathlib.Path, output_dir: pathlib.Path, package_script: pathlib.Path) -> pathlib.Path:
    source = source.resolve()
    if not source.is_file():
        raise FileNotFoundError(f"source image absent: {source}")
    output_dir.mkdir(parents=True, exist_ok=True)
    versions = component_versions()
    ensure_nano_stage()
    stage_firefox_licenses()
    before = preservation_hashes(source)
    descriptor, temporary_name = tempfile.mkstemp(
        prefix=".makos-integrated-", suffix=".img.tmp", dir=output_dir
    )
    os.close(descriptor)
    temporary = pathlib.Path(temporary_name)
    try:
        clone_preserved_regions(source, temporary)
        if preservation_hashes(temporary) != before:
            raise ValueError("preserved regions changed during clone")
        subprocess.run([str(package_script), str(temporary)], cwd=ROOT, check=True)
        after_source = preservation_hashes(source)
        after_output = preservation_hashes(temporary)
        if before != after_source:
            raise RuntimeError("source mutable state changed during integration; retry snapshot")
        if before != after_output:
            raise ValueError("package staging modified account/profile state")
        entries, tree_hash = verify_components(temporary, expected_sources())
        firefox_build_provenance = verify_firefox_provenance(temporary, entries)
        identity_input = {
            "layout": {
                "image_bytes": mkpackage.IMAGE_BYTES,
                "package_start": PACKAGE_START,
                "profile_start": PROFILE_START,
            },
            "preservation": before,
            "package_tree_sha256": tree_hash,
            "firefox_build_provenance": firefox_build_provenance,
            "versions": versions,
        }
        semantic_identity = hashlib.sha256(
            json.dumps(identity_input, sort_keys=True, separators=(",", ":")).encode()
        ).hexdigest()
        image_hash = sha256_path(temporary)
        output = output_dir / f"makos-integrated-{image_hash[:16]}.img"
        if output.exists():
            existing_entries, existing_tree = verify_components(output, expected_sources())
            if (
                preservation_hashes(output) != before
                or existing_tree != tree_hash
                or sha256_path(output) != image_hash
            ):
                raise FileExistsError(f"content-address collision: {output}")
            temporary.unlink()
            entries = existing_entries
        else:
            os.replace(temporary, output)
        manifest = {
            "schema": 1,
            "artifact": output.name,
            "image_sha256": image_hash,
            "semantic_identity_sha256": semantic_identity,
            **identity_input,
            "package": {
                "files": len(entries),
                "payload_bytes": sum(entry.size for entry in entries.values()),
                "required_paths": sorted(expected_sources()),
                "metadata_crc32": True,
                "payload_crc32": True,
                "elf": "AArch64 ET_DYN; executables use /lib/ld-musl-aarch64.so.1",
                "firefox_provenance": "pinned-source,ordered-patches,audited-artifact-sha256",
            },
        }
        write_manifest(output.with_suffix(".manifest.json"), manifest)
        print(
            "MAKOS_INTEGRATED_DATA_OK "
            f"image={output} sha256={image_hash} files={len(entries)} "
            "preserved=filesystem-metadata,account-profile "
            f"firefox=140.13.0esr patches={firefox_build_provenance['patch_count']} "
            f"patch_series_sha256={firefox_build_provenance['patch_series_sha256']} "
            "nano=9.1 ncurses=6.5 cpython=3.14.7"
        )
        return output
    finally:
        temporary.unlink(missing_ok=True)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Clone mutable state and atomically stage the complete MakOS package set"
    )
    parser.add_argument("source", type=pathlib.Path, help="existing 1 GiB MakOS data image")
    parser.add_argument("--output-dir", type=pathlib.Path, default=ROOT / "build")
    parser.add_argument(
        "--package-script",
        type=pathlib.Path,
        default=ROOT / "ports/firefox/package-makos.sh",
        help=argparse.SUPPRESS,
    )
    args = parser.parse_args()
    try:
        build(args.source, args.output_dir, args.package_script.resolve())
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"integrate_data_image: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
