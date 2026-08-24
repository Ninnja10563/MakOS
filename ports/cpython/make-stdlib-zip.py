#!/usr/bin/env python3
"""Build deterministic, uncompressed CPython stdlib archive for MakOS.

ZIP_STORED is intentional: initial MakOS CPython has no packaged zlib extension,
while upstream zipimport can read stored modules using only core runtime code.
"""

from __future__ import annotations

import pathlib
import sys
import zipfile


EXCLUDED_TOP_LEVEL = {
    "ensurepip",
    "idlelib",
    "test",
    "tkinter",
    "turtledemo",
    "venv",
}


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: make-stdlib-zip.py CPYTHON_LIB OUTPUT", file=sys.stderr)
        return 2
    source = pathlib.Path(sys.argv[1])
    output = pathlib.Path(sys.argv[2])
    if not source.is_dir():
        print(f"stdlib source absent: {source}", file=sys.stderr)
        return 2
    paths = [
        path
        for path in source.rglob("*.py")
        if path.relative_to(source).parts[0] not in EXCLUDED_TOP_LEVEL
        and "__pycache__" not in path.parts
    ]
    output.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_STORED) as archive:
        for path in sorted(paths):
            name = path.relative_to(source).as_posix()
            info = zipfile.ZipInfo(name, (2026, 8, 17, 0, 0, 0))
            info.compress_type = zipfile.ZIP_STORED
            info.external_attr = 0o100644 << 16
            archive.writestr(info, path.read_bytes())
    print(
        f"MAKOS_CPYTHON_STDLIB_OK files={len(paths)} bytes={output.stat().st_size} "
        "compression=stored zipimport=upstream"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
