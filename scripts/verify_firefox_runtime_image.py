#!/usr/bin/env python3
"""Fail before QEMU when a Firefox runtime image is stale or unprovenanced."""

from __future__ import annotations

import argparse
import pathlib
import sys

import firefox_provenance
import integrate_data_image


REQUIRED = (
    *(
        f"/usr/lib/firefox/{name}"
        for name in integrate_data_image.FIREFOX_ELF_CONTRACTS
    ),
    firefox_provenance.GUEST_PATH,
)


def verify(image: pathlib.Path) -> dict:
    entries = integrate_data_image.package_entries(image)
    missing = sorted(set(REQUIRED) - set(entries))
    if missing:
        raise ValueError(f"Firefox runtime paths absent: {', '.join(missing)}")
    integrate_data_image.verify_firefox_elf_entries(image, entries)
    return integrate_data_image.verify_firefox_provenance(image, entries)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("image", type=pathlib.Path)
    args = parser.parse_args()
    try:
        record = verify(args.image)
    except (OSError, ValueError) as error:
        print(f"verify_firefox_runtime_image: {error}", file=sys.stderr)
        return 1
    print(
        "MAKOS_FIREFOX_RUNTIME_IMAGE_OK "
        f"image={args.image} source={record['source_version']}@{record['source_commit']} "
        f"patches={record['patch_count']} "
        f"patch_series_sha256={record['patch_series_sha256']} "
        "artifacts=build-audited,runtime-sha256-matched "
        "elf=aarch64-pie,libxul all_five_elf=aarch64-et-dyn,interp-and-deps-by-kind"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
