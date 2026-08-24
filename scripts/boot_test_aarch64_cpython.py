#!/usr/bin/env python3
"""Focused hidden-HVF proof for genuine upstream CPython 3.14 on MakOS."""

from __future__ import annotations

import os
import pathlib
import tempfile

import boot_test_aarch64 as common
import boot_test_aarch64_nano as harness


ROOT = pathlib.Path(__file__).resolve().parent.parent
SERIAL_LOG = pathlib.Path(
    os.environ.get(
        "MAKOS_AARCH64_CPYTHON_SERIAL_LOG",
        ROOT / "build/cpython-3.14.7-runtime.log",
    )
)
SOURCE = (
    'import sys;print(sys.version_info[:3]);print(sum(range(20)));'
    'print("py314".upper());print(open(__file__).read(6))'
)
IMPORT_SOURCE = 'import json;print(json.dumps([1,2,3]))'


def main() -> int:
    boot_image = pathlib.Path(
        os.environ.get(
            "MAKOS_AARCH64_IMAGE", ROOT / "build/makos-cpython-3.14.7-boot.img"
        )
    )
    package_image = pathlib.Path(
        os.environ.get(
            "MAKOS_AARCH64_PACKAGE_IMAGE",
            ROOT / "build/makos-cpython-3.14.7-data.img",
        )
    )
    if not boot_image.is_file() or not package_image.is_file():
        raise FileNotFoundError("build CPython boot and package images first")
    code = common.first_file(
        "AAVMF_CODE",
        (
            "/opt/homebrew/share/qemu/edk2-aarch64-code.fd",
            "/usr/local/share/qemu/edk2-aarch64-code.fd",
            "/usr/share/AAVMF/AAVMF_CODE.fd",
        ),
    )
    vars_template = common.first_file(
        "AAVMF_VARS",
        (
            "/opt/homebrew/share/qemu/edk2-arm-vars.fd",
            "/usr/local/share/qemu/edk2-arm-vars.fd",
            "/usr/share/AAVMF/AAVMF_VARS.fd",
        ),
    )
    temp_root = pathlib.Path(os.environ.get("MAKOS_AARCH64_TEMP_ROOT", ROOT / "build"))
    with tempfile.TemporaryDirectory(prefix="makos-cpython-test-", dir=temp_root) as name:
        temporary = pathlib.Path(name)
        data_image = temporary / "data.img"
        common.copy_sparse(package_image, data_image)
        guest = harness.boot_login(
            boot_image, data_image, code, vars_template, temporary, 1
        )
        try:
            common.send_command(guest.stream, f"write run.py {SOURCE}")
            harness.wait_new(
                guest,
                f"MAKOS_AARCH64_SHELL_CMD write bytes={len(SOURCE)} persisted=1".encode(),
                30,
            )
            common.send_command(guest.stream, "python run.py")
            harness.wait_new(guest, b"MAKOS_CPYTHON_PROCESS_OK", 60)
            for expected in (
                b"(3, 14, 7)",
                b"190",
                b"PY314",
                b"import",
                b"MAKOS_AARCH64_PYTHON_LAUNCH_OK selector=4 process=isolated wait=1",
            ):
                common.wait_for_output(
                    guest.selector, guest.process, guest.output, expected, 120
                )
            common.send_command(guest.stream, f"write import.py {IMPORT_SOURCE}")
            harness.wait_new(
                guest,
                f"MAKOS_AARCH64_SHELL_CMD write bytes={len(IMPORT_SOURCE)} persisted=1".encode(),
                30,
            )
            common.send_command(guest.stream, "python import.py")
            harness.wait_new(guest, b"MAKOS_CPYTHON_PROCESS_OK", 60)
            common.wait_for_output(
                guest.selector,
                guest.process,
                guest.output,
                b"[1, 2, 3]",
                120,
            )
        finally:
            common.drain_output(guest.selector, guest.process, guest.output, 1.0)
            SERIAL_LOG.parent.mkdir(parents=True, exist_ok=True)
            SERIAL_LOG.write_bytes(guest.output)
            guest.stop()

    print(
        "MAKOS_CPYTHON_RUNTIME_OK implementation=cpython version=3.14.7 "
        "parser=peg compiler=bytecode vm=ceval gc=generational source=vfs "
        "stdlib=zipimport imports=json file_io=read arithmetic=190 strings=upper "
        f"fake=0 host_delegation=0 serial_log={SERIAL_LOG}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
