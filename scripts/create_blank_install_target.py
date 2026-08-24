#!/usr/bin/env python3
"""Exclusively create a sparse blank disk matching one MakOS source image."""

from __future__ import annotations

import argparse
import os
import pathlib


def create(source: pathlib.Path, target: pathlib.Path) -> int:
    if not source.is_file():
        raise ValueError("source image absent")
    size = source.stat().st_size
    if size < 4096 * 512 or size % 512:
        raise ValueError("source image geometry invalid")
    if source.resolve() == target.resolve():
        raise ValueError("target must differ from source")
    target.parent.mkdir(parents=True, exist_ok=True)
    try:
        with target.open("xb") as output:
            output.truncate(size)
            output.flush()
            os.fsync(output.fileno())
    except FileExistsError as error:
        raise ValueError("target already exists; refusing overwrite") from error
    os.chmod(target, 0o644)
    return size


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=pathlib.Path)
    parser.add_argument("target", type=pathlib.Path)
    args = parser.parse_args()
    try:
        size = create(args.source, args.target)
    except (OSError, ValueError) as error:
        parser.error(str(error))
    print(f"created blank install target {args.target} bytes={size} exclusive=1 sparse=1")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
