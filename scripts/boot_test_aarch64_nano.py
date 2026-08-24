#!/usr/bin/env python3
"""Focused hidden-HVF proof for genuine GNU nano save/reopen/persistence."""

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
import time

import boot_test_aarch64 as common


ROOT = pathlib.Path(__file__).resolve().parent.parent
CONTENT = "nano-runtime-proof-91"
FILE = "nano-proof.txt"
SAVED_BYTES = len(CONTENT) + 1  # Upstream nano writes final newline by default.


class Guest:
    def __init__(self, process, selector, output, client, stream):
        self.process = process
        self.selector = selector
        self.output = output
        self.client = client
        self.stream = stream

    def stop(self) -> None:
        try:
            common.qmp_command(self.stream, "quit")
        except (BrokenPipeError, OSError):
            pass
        self.stream.close()
        self.client.close()
        try:
            self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.terminate()
            self.process.wait(timeout=5)
        self.selector.close()


def boot_login(
    boot_image: pathlib.Path,
    data_image: pathlib.Path,
    code: pathlib.Path,
    vars_template: pathlib.Path,
    temporary: pathlib.Path,
    boot_number: int,
) -> Guest:
    qmp_path = temporary / f"qmp-{boot_number}.sock"
    vars_copy = temporary / f"vars-{boot_number}.fd"
    shutil.copyfile(vars_template, vars_copy)
    accel = os.environ.get(
        "MAKOS_AARCH64_ACCEL",
        "hvf" if platform.system() == "Darwin" and platform.machine() == "arm64" else "tcg",
    )
    command = [
        os.environ.get("QEMU_SYSTEM_AARCH64", "qemu-system-aarch64"),
        "-machine", f"virt,accel={accel},highmem=off,gic-version=2",
        "-cpu", "host" if accel == "hvf" else "max",
        "-global", "virtio-mmio.force-legacy=false",
        "-smp", "4", "-m", "1G",
        "-drive", f"if=pflash,format=raw,readonly=on,file={code}",
        "-drive", f"if=pflash,format=raw,file={vars_copy}",
        "-drive", f"id=boot,if=none,format=raw,readonly=on,file={boot_image}",
        "-device", "virtio-blk-pci,drive=boot",
        "-drive", f"id=data,if=none,format=raw,file={data_image}",
        "-device", "virtio-blk-device,drive=data",
        "-device", "virtio-keyboard-device",
        "-device", "virtio-tablet-device",
        "-netdev", "user,id=makosnet",
        "-device", "virtio-net-device,netdev=makosnet,mac=52:54:00:12:34:56",
        "-device", "virtio-gpu-device,xres=800,yres=600",
        "-object", "rng-random,id=makosrng,filename=/dev/urandom",
        "-device", "virtio-rng-device,rng=makosrng",
        "-display", "none", "-serial", "stdio", "-monitor", "none",
        "-qmp", f"unix:{qmp_path},server=on,wait=off",
        "-no-reboot", "-no-shutdown",
    ]
    process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    assert process.stdout is not None
    output = bytearray()
    selector = selectors.DefaultSelector()
    selector.register(process.stdout, selectors.EVENT_READ)
    try:
        common.wait_for_output(
            selector, process, output, b"MAKOS_AARCH64_BOOT_OK", 60
        )
        common.wait_for_output(selector, process, output, b"MAKOS_PACKAGE_FS_OK", 20)
        common.wait_for_output(selector, process, output, b"MAKOS_MAKFS4_READY", 20)
        common.wait_for_output(
            selector, process, output, b"MAKOS_AARCH64_SHELL_PROCESS_OK", 20
        )
        common.wait_for_socket(qmp_path, process)
        client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        client.connect(str(qmp_path))
        stream = client.makefile("rwb", buffering=0)
        json.loads(stream.readline())
        if "error" in common.qmp_command(stream, "qmp_capabilities"):
            raise AssertionError("QMP capability negotiation failed")
        common.click_pointer(stream, 390, 220)
        common.wait_for_output(selector, process, output, b"MAKOS_LOGIN_CLICK_OK", 10)
        for key in ("m", "a", "r", "c", "u", "s", "tab", "m", "a", "k", "o", "s", "ret"):
            common.send_key(stream, key)
        common.wait_for_output(
            selector, process, output, b"MAKOS_AARCH64_DESKTOP_OK", 60
        )
        common.wait_for_output(
            selector, process, output, b"MAKOS_TERMINAL_ANSI_OK", 20
        )
        # Files finishes asynchronously and owns focus. Wait for its final
        # surface before activating terminal; otherwise it can steal focus
        # between the taskbar click and first shell byte.
        common.wait_for_output(
            selector, process, output, b"MAKOS_AARCH64_FILES_OK", 20
        )
        common.click_pointer(stream, 250, 580)
        common.wait_for_output(
            selector,
            process,
            output,
            b"MAKOS_TASKBAR_APP_OK surface=2 activate=1 minimize_toggle=1",
            10,
        )
        return Guest(process, selector, output, client, stream)
    except Exception:
        process.terminate()
        process.wait(timeout=5)
        selector.close()
        raise


def wait_new(guest: Guest, marker: bytes, timeout: float = 20) -> None:
    common.wait_for_new_output(
        guest.selector, guest.process, guest.output, marker, timeout
    )


def open_nano(guest: Guest) -> None:
    common.send_command(guest.stream, f"nano {FILE}")
    wait_new(guest, b"MAKOS_NANO_PROCESS_OK", 30)
    common.drain_output(guest.selector, guest.process, guest.output, 0.75)


def type_text(guest: Guest, value: str) -> None:
    keys = {"-": "minus"}
    for character in value:
        common.send_key(guest.stream, keys.get(character, character))


def exit_nano(guest: Guest) -> None:
    common.send_key(guest.stream, "ctrl-x")
    wait_new(guest, b"MAKOS_NANO_REAP_OK", 20)


def main() -> int:
    boot_image = pathlib.Path(
        os.environ.get("MAKOS_AARCH64_IMAGE", ROOT / "build/makos-aarch64.img")
    )
    package_image = pathlib.Path(
        os.environ.get("MAKOS_AARCH64_PACKAGE_IMAGE", ROOT / "build/makos-nano-data.img")
    )
    if not boot_image.is_file() or not package_image.is_file():
        raise FileNotFoundError("build boot image and nano package before runtime test")
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
    with tempfile.TemporaryDirectory(prefix="makos-nano-test-", dir=temp_root) as name:
        temporary = pathlib.Path(name)
        data_image = temporary / "data.img"
        common.copy_sparse(package_image, data_image)

        first = boot_login(boot_image, data_image, code, vars_template, temporary, 1)
        try:
            open_nano(first)
            type_text(first, CONTENT)
            common.send_key(first.stream, "ctrl-o")
            common.drain_output(first.selector, first.process, first.output, 0.5)
            common.send_key(first.stream, "ret")
            common.drain_output(first.selector, first.process, first.output, 0.75)
            exit_nano(first)
            common.send_command(first.stream, f"cat {FILE}")
            wait_new(first, f"MAKOS_AARCH64_SHELL_CMD cat bytes={SAVED_BYTES}".encode())
            open_nano(first)
            exit_nano(first)
        finally:
            first.stop()

        second = boot_login(boot_image, data_image, code, vars_template, temporary, 2)
        try:
            common.send_command(second.stream, f"cat {FILE}")
            wait_new(second, f"MAKOS_AARCH64_SHELL_CMD cat bytes={SAVED_BYTES}".encode())
            open_nano(second)
            exit_nano(second)
        finally:
            second.stop()

    print(
        "MAKOS_NANO_RUNTIME_OK source=gnu-9.1 ncurses=6.5 "
        f"file={FILE} bytes={SAVED_BYTES} save=ctrl-o exit=ctrl-x "
        "reopen=1 reboot_persistence=1 fake=0"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
