#!/usr/bin/env python3
"""Audit every stamped Firefox build artifact through the package ELF parser."""

from __future__ import annotations

import argparse
import pathlib
import sys

import integrate_data_image


def verify(bin_dir: pathlib.Path) -> None:
    for name, contract in integrate_data_image.FIREFOX_ELF_CONTRACTS.items():
        path = bin_dir / name
        if not path.is_file():
            raise FileNotFoundError(f"Firefox build artifact absent: {path}")
        entry = integrate_data_image.Entry(
            path=name,
            size=path.stat().st_size,
            offset=0,
            sha256="",
        )
        integrate_data_image.verify_aarch64_elf(
            path,
            entry,
            contract.interpreter,
            contract.dependencies,
            contract.soname,
        )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("bin_dir", type=pathlib.Path)
    args = parser.parse_args()
    try:
        verify(args.bin_dir)
    except (OSError, ValueError) as error:
        print(f"verify_firefox_build_elf: {error}", file=sys.stderr)
        return 1
    print(
        "MAKOS_FIREFOX_BUILD_ELF_OK "
        "artifacts=firefox,plugin-container,xpcshell,libxul.so,libnspr4.so "
        "identity=elf64,aarch64,et-dyn "
        "interp=executables-only dependencies=artifact-specific"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
