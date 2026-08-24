#!/usr/bin/env python3
"""AArch64 durable package mutation, recovery, and live-VFS runtime proof."""

from __future__ import annotations

import os
import pathlib
import struct
import tempfile

import boot_test_aarch64 as common
import boot_test_aarch64_nano as runtime


ROOT = pathlib.Path(__file__).resolve().parent.parent
PACKAGE_BASE_SECTOR = 786_432
PACKAGE_SLOT_SECTORS = 131_072
SECTOR_BYTES = 512
PACKAGE_PATH = "/packages/hello/payload"


def wait_new(guest: runtime.Guest, marker: bytes, timeout: float = 30) -> None:
    common.wait_for_new_output(
        guest.selector, guest.process, guest.output, marker, timeout
    )


def send_and_wait(
    guest: runtime.Guest, command: str, marker: bytes, timeout: float = 30
) -> None:
    common.send_command(guest.stream, command)
    wait_new(guest, marker, timeout)


def require_boot_marker(guest: runtime.Guest, marker: bytes) -> None:
    if marker not in guest.output:
        raise AssertionError(f"missing boot marker {marker!r}")


def corrupt_newest_header(data_image: pathlib.Path) -> tuple[int, int]:
    headers: list[tuple[int, int]] = []
    with data_image.open("r+b", buffering=0) as image:
        for slot in range(2):
            offset = (
                PACKAGE_BASE_SECTOR + slot * PACKAGE_SLOT_SECTORS
            ) * SECTOR_BYTES
            image.seek(offset)
            header = image.read(SECTOR_BYTES)
            if len(header) != SECTOR_BYTES or header[:8] != b"MAKPTS01":
                continue
            state = struct.unpack_from("<I", header, 12)[0]
            generation = struct.unpack_from("<Q", header, 16)[0]
            if state == 2:
                headers.append((generation, slot))
        if len(headers) != 2:
            raise AssertionError(f"expected two committed package slots, got {headers}")
        generation, slot = max(headers)
        offset = (
            PACKAGE_BASE_SECTOR + slot * PACKAGE_SLOT_SECTORS
        ) * SECTOR_BYTES
        image.seek(offset + 64)
        original = image.read(1)
        if len(original) != 1:
            raise AssertionError("could not read newest package header")
        image.seek(offset + 64)
        image.write(bytes((original[0] ^ 0x5A,)))
        image.flush()
        os.fsync(image.fileno())
    return generation, slot


def boot(
    boot_image: pathlib.Path,
    data_image: pathlib.Path,
    code: pathlib.Path,
    variables: pathlib.Path,
    temporary: pathlib.Path,
    number: int,
) -> runtime.Guest:
    return runtime.boot_login(
        boot_image, data_image, code, variables, temporary, number
    )


def prove_live_payload(guest: runtime.Guest) -> None:
    send_and_wait(
        guest,
        f"cat {PACKAGE_PATH}",
        b"MAKOS_AARCH64_SHELL_CMD cat bytes=8 path=/packages/hello/payload",
    )


def main() -> int:
    boot_image = pathlib.Path(
        os.environ.get("MAKOS_AARCH64_IMAGE", ROOT / "build/makos-aarch64.img")
    )
    package_image = pathlib.Path(
        os.environ.get(
            "MAKOS_AARCH64_PACKAGE_IMAGE",
            ROOT / "build/makos-integrated-a9c604254f094de2.img",
        )
    )
    if not boot_image.is_file() or not package_image.is_file():
        raise FileNotFoundError("build boot and integrated data images first")
    code = common.first_file(
        "AAVMF_CODE",
        (
            "/opt/homebrew/share/qemu/edk2-aarch64-code.fd",
            "/usr/local/share/qemu/edk2-aarch64-code.fd",
            "/usr/share/AAVMF/AAVMF_CODE.fd",
        ),
    )
    variables = common.first_file(
        "AAVMF_VARS",
        (
            "/opt/homebrew/share/qemu/edk2-arm-vars.fd",
            "/usr/local/share/qemu/edk2-arm-vars.fd",
            "/usr/share/AAVMF/AAVMF_VARS.fd",
        ),
    )
    temp_root = pathlib.Path(os.environ.get("MAKOS_AARCH64_TEMP_ROOT", ROOT / "build"))
    with tempfile.TemporaryDirectory(prefix="makos-package-test-", dir=temp_root) as name:
        temporary = pathlib.Path(name)

        durable = temporary / "durable.img"
        common.copy_sparse(package_image, durable)
        first = boot(boot_image, durable, code, variables, temporary, 1)
        try:
            send_and_wait(
                first,
                "pkg-probe-install",
                b"MAKOS_AARCH64_PACKAGE_TXN_OK install=1 replace=1 rollback=1 version=1.0 tamper_denied=1 open_fd_pin=replace pinned_reuse_denied=1",
            )
            require_boot_marker(
                first, b"MAKOS_PACKAGE_ACTIVATION_OK generation=3 packages=1"
            )
            prove_live_payload(first)
        finally:
            first.stop()

        second = boot(boot_image, durable, code, variables, temporary, 2)
        try:
            require_boot_marker(
                second, b"MAKOS_PACKAGE_ACTIVATION_OK generation=3 packages=1"
            )
            send_and_wait(
                second,
                "pkg-probe-query-v1",
                b"MAKOS_AARCH64_PACKAGE_QUERY_OK version=1.0",
            )
            prove_live_payload(second)
            send_and_wait(
                second,
                "pkg-probe-remove",
                b"MAKOS_AARCH64_PACKAGE_REMOVE_OK remove=1 query=absent vfs=refreshed open_fd_pin=remove pinned_rollback_denied=1",
            )
            require_boot_marker(
                second, b"MAKOS_PACKAGE_ACTIVATION_OK generation=4 packages=0"
            )
            send_and_wait(
                second,
                f"cat {PACKAGE_PATH}",
                b"MAKOS_AARCH64_SHELL_CAT_ERROR reason=not-found path=/packages/hello/payload",
            )
        finally:
            second.stop()

        third = boot(boot_image, durable, code, variables, temporary, 3)
        try:
            require_boot_marker(
                third, b"MAKOS_PACKAGE_ACTIVATION_OK generation=4 packages=0"
            )
            send_and_wait(
                third,
                "pkg-probe-rollback",
                b"MAKOS_AARCH64_PACKAGE_ROLLBACK_OK rollback=1 version=1.0 vfs=refreshed",
            )
            require_boot_marker(
                third, b"MAKOS_PACKAGE_ACTIVATION_OK generation=5 packages=1"
            )
            prove_live_payload(third)
        finally:
            third.stop()

        faulted = temporary / "faulted.img"
        common.copy_sparse(package_image, faulted)
        setup = boot(boot_image, faulted, code, variables, temporary, 4)
        try:
            send_and_wait(
                setup,
                "pkg-probe-install",
                b"MAKOS_AARCH64_PACKAGE_TXN_OK install=1 replace=1 rollback=1 version=1.0 tamper_denied=1 open_fd_pin=replace pinned_reuse_denied=1",
            )
        finally:
            setup.stop()
        corrupted_generation, corrupted_slot = corrupt_newest_header(faulted)
        if corrupted_generation != 3:
            raise AssertionError(
                f"unexpected newest generation before fault: {corrupted_generation}"
            )

        recovery = boot(boot_image, faulted, code, variables, temporary, 5)
        try:
            require_boot_marker(
                recovery, b"MAKOS_PACKAGE_ACTIVATION_OK generation=2 packages=1"
            )
            send_and_wait(
                recovery,
                "pkg-probe-query-v2",
                b"MAKOS_AARCH64_PACKAGE_QUERY_OK version=2.0",
            )
            prove_live_payload(recovery)
        finally:
            recovery.stop()

    print(
        "MAKOS_AARCH64_PACKAGE_RUNTIME_OK install=1 replace=1 remove=1 "
        "rollback=1 reboot_persistence=1 vfs=/packages/hello/payload "
        "open_fd_generation_pin=replace,remove "
        f"corrupt_newest_fallback=1 corrupted_slot={corrupted_slot} "
        "signature=rsa2048-sha256 tamper_denied=1"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
