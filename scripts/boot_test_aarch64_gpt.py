#!/usr/bin/env python3
"""Prove UEFI boot and persistence from one AArch64 GPT disk."""

from __future__ import annotations

import json
import os
import pathlib
import platform
import selectors
import shutil
import socket
import subprocess
import tempfile

from boot_test_aarch64 import (
    click_pointer,
    copy_sparse,
    first_file,
    qmp_command,
    send_command,
    send_key,
    wait_for_output,
    wait_for_socket,
)

ROOT = pathlib.Path(__file__).resolve().parent.parent
IMAGE = pathlib.Path(
    os.environ.get("MAKOS_AARCH64_GPT_IMAGE", ROOT / "build/makos-aarch64-gpt.img")
)
TIMEOUT = 90


def command(qemu: str, accel: str, code: pathlib.Path, variables: pathlib.Path, image: pathlib.Path, qmp: pathlib.Path) -> list[str]:
    return [
        qemu,
        "-machine", f"virt,accel={accel},highmem=off,gic-version=2",
        "-cpu", "host" if accel == "hvf" else "max",
        "-global", "virtio-mmio.force-legacy=false",
        "-smp", "4",
        "-m", "1G",
        "-drive", f"if=pflash,format=raw,readonly=on,file={code}",
        "-drive", f"if=pflash,format=raw,file={variables}",
        "-drive", f"id=system,if=none,format=raw,file={image}",
        "-device", "virtio-blk-device,drive=system,bootindex=0",
        "-device", "virtio-keyboard-device",
        "-device", "virtio-tablet-device",
        "-netdev", "user,id=makosnet",
        "-device", "virtio-net-device,netdev=makosnet,mac=52:54:00:12:34:56",
        "-device", "virtio-gpu-device,xres=800,yres=600",
        "-object", "rng-random,id=makosrng,filename=/dev/urandom",
        "-device", "virtio-rng-device,rng=makosrng",
        "-display", "none",
        "-serial", "stdio",
        "-monitor", "none",
        "-qmp", f"unix:{qmp},server=on,wait=off",
        "-no-reboot",
        "-no-shutdown",
    ]


def login(stream, selector, process, output: bytearray) -> None:
    wait_for_output(selector, process, output, b"MAKOS_AARCH64_SHELL_PROCESS_OK", TIMEOUT)
    for key in ("m", "a", "r", "c", "u", "s", "ret", "m", "a", "k", "o", "s", "ret"):
        send_key(stream, key)
    wait_for_output(selector, process, output, b"MAKOS_AARCH64_DESKTOP_OK", TIMEOUT)
    wait_for_output(
        selector, process, output,
        b"MAKOS_AARCH64_BROWSER_BACKGROUND_OK startup_fetch=0",
        20,
    )
    wait_for_output(selector, process, output, b"MAKOS_AARCH64_FILES_OK", 20)
    click_pointer(stream, 250, 580)
    wait_for_output(selector, process, output, b"MAKOS_TASKBAR_APP_OK surface=2", 10)


def run_once(arguments: list[str], qmp_path: pathlib.Path, action) -> bytes:
    process = subprocess.Popen(arguments, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    assert process.stdout is not None
    selector = selectors.DefaultSelector()
    selector.register(process.stdout, selectors.EVENT_READ)
    output = bytearray()
    try:
        wait_for_output(selector, process, output, b"MakOS loader v0.1", TIMEOUT)
        wait_for_output(
            selector,
            process,
            output,
            b"MAKOS_GPT_DATA_OK start_lba=133120 sectors=2097152 legacy_raw=0",
            TIMEOUT,
        )
        wait_for_socket(qmp_path, process)
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
            client.connect(str(qmp_path))
            stream = client.makefile("rwb", buffering=0)
            json.loads(stream.readline())
            if "error" in qmp_command(stream, "qmp_capabilities"):
                raise AssertionError("QMP capability negotiation failed")
            login(stream, selector, process, output)
            action(stream, selector, process, output)
            qmp_command(stream, "quit")
        process.wait(timeout=5)
        return bytes(output)
    finally:
        selector.close()
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=5)


def main() -> int:
    if not IMAGE.is_file():
        raise FileNotFoundError(IMAGE)
    qemu = os.environ.get("QEMU_SYSTEM_AARCH64", "qemu-system-aarch64")
    code = first_file("AAVMF_CODE", (
        "/opt/homebrew/share/qemu/edk2-aarch64-code.fd",
        "/usr/local/share/qemu/edk2-aarch64-code.fd",
        "/usr/share/AAVMF/AAVMF_CODE.fd",
    ))
    vars_template = first_file("AAVMF_VARS", (
        "/opt/homebrew/share/qemu/edk2-arm-vars.fd",
        "/usr/local/share/qemu/edk2-arm-vars.fd",
        "/usr/share/AAVMF/AAVMF_VARS.fd",
    ))
    accel = os.environ.get(
        "MAKOS_AARCH64_ACCEL",
        "hvf" if platform.system() == "Darwin" and platform.machine() == "arm64" else "tcg",
    )
    with tempfile.TemporaryDirectory(prefix="makos-aarch64-gpt-test-", dir=ROOT / "build") as temporary:
        root = pathlib.Path(temporary)
        image = root / "system.img"
        copy_sparse(IMAGE, image)

        first_vars = root / "vars-first.fd"
        first_qmp = root / "first.qmp"
        shutil.copyfile(vars_template, first_vars)

        def create(stream, selector, process, output):
            send_command(stream, "write gpt-persist.txt single-disk")
            wait_for_output(
                selector, process, output,
                b"MAKOS_AARCH64_SHELL_CMD write bytes=11 persisted=1",
                20,
            )

        first_output = run_once(command(qemu, accel, code, first_vars, image, first_qmp), first_qmp, create)
        if b"MAKOS_MAKFS4_READY state=Formatted" not in first_output:
            raise AssertionError("first GPT boot did not format MakFS4")

        second_vars = root / "vars-second.fd"
        second_qmp = root / "second.qmp"
        shutil.copyfile(vars_template, second_vars)

        def verify(stream, selector, process, output):
            send_command(stream, "cat gpt-persist.txt")
            wait_for_output(
                selector, process, output,
                b"MAKOS_AARCH64_SHELL_CMD cat bytes=11",
                20,
            )

        second_output = run_once(command(qemu, accel, code, second_vars, image, second_qmp), second_qmp, verify)
        if b"makfs_generation=2 boot_count=2" not in second_output:
            raise AssertionError("second GPT boot did not remount persisted disk")
    print("MAKOS_AARCH64_GPT_BOOT_OK uefi=1 single_disk=1 esp=1 data_partition=1 persistence=two-boot")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
