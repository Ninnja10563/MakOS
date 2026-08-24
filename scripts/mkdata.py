#!/usr/bin/env python3
"""Create sparse 1 GiB MakOS data disk for legacy MakFS and MakFS4."""

import pathlib
import sys


IMAGE_BYTES = 1024 * 1024 * 1024


def main() -> int:
    if len(sys.argv) != 2:
        print(f"usage: {sys.argv[0]} IMAGE", file=sys.stderr)
        return 2
    path = pathlib.Path(sys.argv[1])
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("wb") as output:
        output.truncate(IMAGE_BYTES)
    print(f"created {path} (1 GiB sparse data disk)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
