#!/usr/bin/env python3
"""Create deterministic MakOS release artifacts from verified build outputs."""

from __future__ import annotations

import hashlib
import pathlib
import shutil
import struct
import zlib
import zipfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUTPUTS = ROOT / "outputs"
SOURCE_ROOTS = ("boot", "crates", "docs", "kernel", "scripts", "sdk", "user")
SOURCE_FILES = (
    ".cargo/config.toml",
    ".gitignore",
    "Cargo.lock",
    "Cargo.toml",
    "LICENSE-MIT",
    "Makefile",
    "README.md",
    "rust-toolchain.toml",
)
FIXED_TIME = (2026, 8, 14, 0, 0, 0)


def source_paths() -> list[pathlib.Path]:
    paths = [ROOT / name for name in SOURCE_FILES]
    for directory in SOURCE_ROOTS:
        paths.extend(
            path
            for path in (ROOT / directory).rglob("*")
            if path.is_file() and "__pycache__" not in path.parts and path.suffix != ".pyc"
        )
    return sorted(paths, key=lambda path: path.relative_to(ROOT).as_posix())


def write_source_zip(destination: pathlib.Path) -> None:
    with zipfile.ZipFile(destination, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for path in source_paths():
            relative = path.relative_to(ROOT).as_posix()
            info = zipfile.ZipInfo(relative, FIXED_TIME)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o100644 << 16
            archive.writestr(info, path.read_bytes())


def ppm_to_png(source: pathlib.Path, destination: pathlib.Path) -> None:
    data = source.read_bytes()
    header, dimensions, maximum, pixels = data.split(b"\n", 3)
    if header != b"P6" or maximum != b"255":
        raise ValueError("unsupported desktop PPM")
    columns, rows = (int(value) for value in dimensions.split())
    if len(pixels) != columns * rows * 3:
        raise ValueError("desktop PPM size mismatch")
    scanlines = b"".join(
        b"\0" + pixels[row * columns * 3 : (row + 1) * columns * 3]
        for row in range(rows)
    )

    def chunk(kind: bytes, payload: bytes) -> bytes:
        return (
            struct.pack(">I", len(payload))
            + kind
            + payload
            + struct.pack(">I", zlib.crc32(kind + payload) & 0xFFFFFFFF)
        )

    destination.write_bytes(
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", columns, rows, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(scanlines, level=9))
        + chunk(b"IEND", b"")
    )


def write_checksums(paths: list[pathlib.Path]) -> None:
    lines = []
    for path in paths:
        checksum = hashlib.sha256()
        with path.open("rb") as source:
            while block := source.read(1024 * 1024):
                checksum.update(block)
        digest = checksum.hexdigest()
        lines.append(f"{digest}  {path.name}\n")
    (OUTPUTS / "SHA256SUMS").write_text("".join(lines), encoding="ascii")


def copy_sparse(source: pathlib.Path, destination: pathlib.Path) -> None:
    """Copy image while retaining all-zero 1 MiB regions as filesystem holes."""
    with source.open("rb") as input_file, destination.open("wb") as output_file:
        while block := input_file.read(1024 * 1024):
            if block.count(0) == len(block):
                output_file.seek(len(block), 1)
            else:
                output_file.write(block)
        output_file.truncate(source.stat().st_size)


def main() -> int:
    boot_image = ROOT / "build/makos-x86_64.img"
    x86_64_gpt_image = ROOT / "build/makos-x86_64-gpt.img"
    aarch64_image = ROOT / "build/makos-aarch64.img"
    aarch64_gpt_image = ROOT / "build/makos-aarch64-gpt.img"
    data_image = ROOT / "build/makos-data.img"
    screenshot = ROOT / "build/makos-desktop.ppm"
    aarch64_screenshot = ROOT / "build/makos-aarch64.ppm"
    report = OUTPUTS / "BUILD-REPORT.md"
    for required in (
        boot_image,
        x86_64_gpt_image,
        aarch64_image,
        aarch64_gpt_image,
        data_image,
        screenshot,
        aarch64_screenshot,
        report,
    ):
        if not required.is_file():
            raise FileNotFoundError(required)
    OUTPUTS.mkdir(exist_ok=True)
    released_boot = OUTPUTS / boot_image.name
    released_x86_64_gpt = OUTPUTS / x86_64_gpt_image.name
    released_aarch64 = OUTPUTS / aarch64_image.name
    released_aarch64_gpt = OUTPUTS / aarch64_gpt_image.name
    released_data = OUTPUTS / data_image.name
    released_source = OUTPUTS / "makos-source.zip"
    released_desktop = OUTPUTS / "makos-desktop.png"
    released_aarch64_desktop = OUTPUTS / "makos-aarch64.png"
    shutil.copyfile(boot_image, released_boot)
    copy_sparse(x86_64_gpt_image, released_x86_64_gpt)
    shutil.copyfile(aarch64_image, released_aarch64)
    copy_sparse(aarch64_gpt_image, released_aarch64_gpt)
    copy_sparse(data_image, released_data)
    write_source_zip(released_source)
    ppm_to_png(screenshot, released_desktop)
    ppm_to_png(aarch64_screenshot, released_aarch64_desktop)
    artifacts = [
        released_boot,
        released_x86_64_gpt,
        released_aarch64,
        released_aarch64_gpt,
        released_data,
        released_source,
        released_desktop,
        released_aarch64_desktop,
        report,
    ]
    write_checksums(artifacts)
    print("release artifacts packaged")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
