#!/usr/bin/env python3
"""Extend existing MakOS data image sparsely without changing stored sectors."""

from __future__ import annotations

import pathlib
import sys

from mkdata import IMAGE_BYTES


def main() -> int:
    if len(sys.argv) != 2:
        print(f"usage: {sys.argv[0]} IMAGE", file=sys.stderr)
        return 2
    image = pathlib.Path(sys.argv[1])
    if not image.is_file():
        print(f"data image absent: {image}", file=sys.stderr)
        return 1
    previous = image.stat().st_size
    if previous < IMAGE_BYTES:
        with image.open("r+b") as output:
            output.truncate(IMAGE_BYTES)
        print(f"extended {image}: {previous} -> {IMAGE_BYTES} bytes (sparse)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
