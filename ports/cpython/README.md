# CPython for MakOS

Target: official CPython 3.14.7, current stable release pinned 2026-08-17.
Source: `python.org`, SHA-256 from official release metadata. License:
Python Software Foundation License Version 2 (`LICENSE` in source archive).

This port replaces no code with MicroPython and never runs host Python as guest
Python. When `/usr/bin/python3` exists in package FS, `python FILE` selects real
CPython. Minimal images without package retain real MicroPython fallback.

```sh
ports/cpython/fetch.sh
ports/cpython/build-makos.sh
ports/cpython/package-makos.sh build/makos-cpython-3.14.7-data.img
python3 scripts/boot_test_aarch64_cpython.py
```

Cross target is distinct `aarch64-unknown-makos`, using staged official musl
and MakOS syscall adapter. Host Python 3.14 is build-time generator only.
`build-makos.sh` emits PIE `/lib/ld-musl-aarch64.so.1` target executable,
depends only on target `libc.so`, disables host pkg-config, and refuses Linux or
host-library fallback. Package contains stripped interpreter, PSF license, and
556 upstream `.py` modules in deterministic `ZIP_STORED` archive. Stored form
requires no unbundled zlib extension during bootstrap.

Focused HVF proof executes version 3.14.7, arithmetic, string operations, VFS
source/file reads, and `json` through upstream zipimport, then exits status 0
and reclaims its address space. Current boundary: IPv4-only config, static core
module set, no packaged dynamic extensions/ensurepip, 2 KiB writable MakFS4
files, partial signals/process/readiness/POSIX breadth.
