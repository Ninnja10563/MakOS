#!/usr/bin/env python3
"""Guest-triggered AArch64 install, destructive-safety, detached boot proof."""

from __future__ import annotations

import json
import hashlib
import os
import pathlib
import platform
import selectors
import shutil
import socket
import subprocess
import tempfile

import mkgpt
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
ESP_IMAGE = pathlib.Path(
    os.environ.get("MAKOS_AARCH64_IMAGE", ROOT / "build/makos-aarch64.img")
)
TIMEOUT = 90
INSTALL_TIMEOUT = 600


def command(
    qemu: str,
    accel: str,
    code: pathlib.Path,
    variables: pathlib.Path,
    source: pathlib.Path,
    qmp: pathlib.Path,
    target: pathlib.Path | None = None,
) -> list[str]:
    args = [
        qemu,
        "-machine", f"virt,accel={accel},highmem=off,gic-version=2",
        "-cpu", "host" if accel == "hvf" else "max",
        "-global", "virtio-mmio.force-legacy=false",
        "-smp", "4",
        "-m", "1G",
        "-drive", f"if=pflash,format=raw,readonly=on,file={code}",
        "-drive", f"if=pflash,format=raw,file={variables}",
        "-drive", f"id=source,if=none,format=raw,file={source}",
        "-device", "virtio-blk-device,drive=source,bootindex=0",
    ]
    if target is not None:
        args += [
            "-drive", f"id=target,if=none,format=raw,file={target}",
            "-device", "virtio-blk-device,drive=target",
        ]
    args += [
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
    return args


def login(stream, selector, process, output: bytearray) -> None:
    wait_for_output(selector, process, output, b"MAKOS_AARCH64_SHELL_PROCESS_OK", TIMEOUT)
    for key in ("m", "a", "r", "c", "u", "s", "ret", "m", "a", "k", "o", "s", "ret"):
        send_key(stream, key)
    wait_for_output(selector, process, output, b"MAKOS_AARCH64_DESKTOP_OK", TIMEOUT)
    wait_for_output(selector, process, output, b"MAKOS_AARCH64_FILES_OK", 20)
    # Terminal is last window created and already focused. Clicking its taskbar
    # button here would minimize it, causing subsequent keyboard input to be
    # intentionally ignored. Click terminal client area instead so this stays
    # correct if another startup app later takes focus.
    # Bottom-right strip belongs only to terminal at 800x600; browser, Files,
    # and Monitor all end above/left of it even when startup ordering differs.
    click_pointer(stream, 770, 520)
    wait_for_output(selector, process, output, b"focused_surface=2", 10)


def run_once(arguments: list[str], qmp_path: pathlib.Path, action) -> bytes:
    process = subprocess.Popen(arguments, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    assert process.stdout is not None
    selector = selectors.DefaultSelector()
    selector.register(process.stdout, selectors.EVENT_READ)
    output = bytearray()
    try:
        wait_for_output(selector, process, output, b"MakOS loader v0.1", TIMEOUT)
        wait_for_output(selector, process, output, b"MAKOS_GPT_DATA_OK", TIMEOUT)
        wait_for_socket(qmp_path, process)
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
            client.connect(str(qmp_path))
            stream = client.makefile("rwb", buffering=0)
            json.loads(stream.readline())
            if "error" in qmp_command(stream, "qmp_capabilities"):
                raise AssertionError("QMP capability negotiation failed")
            login(stream, selector, process, output)
            action(stream, selector, process, output)
            if process.poll() is None:
                qmp_command(stream, "quit")
        if process.poll() is None:
            process.wait(timeout=10)
        return bytes(output)
    finally:
        selector.close()
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=10)


def fresh_vars(template: pathlib.Path, root: pathlib.Path, name: str) -> pathlib.Path:
    output = root / f"vars-{name}.fd"
    shutil.copyfile(template, output)
    return output


def blank_disk(path: pathlib.Path, size: int) -> None:
    with path.open("wb") as output:
        output.truncate(size)


def digest(path: pathlib.Path) -> str:
    checksum = hashlib.sha256()
    with path.open("rb") as source:
        while chunk := source.read(1024 * 1024):
            checksum.update(chunk)
    return checksum.hexdigest()


def seed_resume_probe(path: pathlib.Path) -> None:
    marker = b"MAKOS_INSTALL_RESUME_PROBE\0"
    block = (marker * (4096 // len(marker) + 1))[:4096]
    with path.open("r+b") as output:
        output.seek(34 * 512)
        output.write(block)


def verify_interrupted_copy(source: pathlib.Path, target: pathlib.Path) -> int:
    copied_blocks = 0
    with source.open("rb") as source_file, target.open("rb") as target_file:
        if target_file.read(512) != bytes(512):
            raise AssertionError("interrupted install committed LBA0")
        source_file.seek(0)
        target_file.seek(0)
        while target_block := target_file.read(4096):
            source_block = source_file.read(len(target_block))
            if any(target_block):
                if target_block != source_block:
                    raise AssertionError("interrupted target contains non-source data")
                copied_blocks += 1
    if copied_blocks == 0:
        raise AssertionError("interrupted install left no copied payload")
    return copied_blocks


def main() -> int:
    if not ESP_IMAGE.is_file():
        raise FileNotFoundError(ESP_IMAGE)
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
    # macOS sockaddr_un paths cap at 104 bytes. Workspace can already be long,
    # so keep randomized test directory component deliberately short.
    with tempfile.TemporaryDirectory(prefix="i-", dir=ROOT / "build") as temporary:
        root = pathlib.Path(temporary)
        data = root / "live-data.img"
        blank_disk(data, 16 * 1024 * 1024)
        source_template = root / "live-template.img"
        mkgpt.build(source_template, ESP_IMAGE, data)
        source = root / "live.img"
        copy_sparse(source_template, source)
        disk_size = source.stat().st_size

        # Destructive guards: wrong confirmation and any nonzero target byte
        # must both refuse before writes.
        occupied = root / "occupied.img"
        blank_disk(occupied, disk_size)
        with occupied.open("r+b") as output:
            output.write(b"occupied")
        occupied_before = digest(occupied)
        refusal_qmp = root / "refusal.qmp"

        def refuse(stream, selector, process, output):
            send_command(stream, "install disk1 confirm")
            wait_for_output(
                selector, process, output,
                b"MAKOS_INSTALL_CONFIRMATION_DENIED target=disk1 expected=erase-disk1 destructive_io=0",
                30,
            )
            send_command(stream, "install disk1 erase-disk1")
            wait_for_output(
                selector, process, output,
                b"MAKOS_INSTALL_ERROR error=TargetNotBlank",
                30,
            )
            send_command(stream, "install disk1 resume-disk1")
            wait_for_output(
                selector, process, output,
                b"MAKOS_INSTALL_ERROR error=ResumeCommitted",
                30,
            )

        refusal = run_once(
            command(qemu, accel, code, fresh_vars(vars_template, root, "refusal"), source, refusal_qmp, occupied),
            refusal_qmp,
            refuse,
        )
        if digest(occupied) != occupied_before:
            raise AssertionError("occupied target changed despite confirmation/blankness refusal")

        # A blank disk is still unsafe if geometry differs: GPT LBAs and backup
        # header would no longer describe target. Require exact sector count.
        wrong_size = root / "wrong-size.img"
        blank_disk(wrong_size, disk_size - 512)
        wrong_size_before = digest(wrong_size)
        geometry_qmp = root / "geometry.qmp"

        def refuse_geometry(stream, selector, process, output):
            send_command(stream, "install disk1 erase-disk1")
            wait_for_output(
                selector, process, output,
                b"MAKOS_INSTALL_ERROR error=Geometry",
                30,
            )

        run_once(
            command(
                qemu, accel, code,
                fresh_vars(vars_template, root, "geometry"),
                source, geometry_qmp, wrong_size,
            ),
            geometry_qmp,
            refuse_geometry,
        )
        if digest(wrong_size) != wrong_size_before:
            raise AssertionError("wrong-size target changed despite geometry refusal")

        conflict = root / "conflict.img"
        blank_disk(conflict, disk_size)
        with conflict.open("r+b") as output:
            output.seek(34 * 512)
            output.write(b"not-source")
        conflict_before = digest(conflict)
        conflict_qmp = root / "conflict.qmp"

        def refuse_conflict(stream, selector, process, output):
            send_command(stream, "install disk1 resume-disk1")
            wait_for_output(
                selector,
                process,
                output,
                b"MAKOS_INSTALL_ERROR error=ResumeConflict",
                60,
            )

        run_once(
            command(
                qemu,
                accel,
                code,
                fresh_vars(vars_template, root, "conflict"),
                source,
                conflict_qmp,
                conflict,
            ),
            conflict_qmp,
            refuse_conflict,
        )
        if digest(conflict) != conflict_before:
            raise AssertionError("conflicting resume target changed despite refusal")

        seed_resume_probe(source)
        target = root / "installed.img"
        blank_disk(target, disk_size)
        install_qmp = root / "install.qmp"

        def interrupt_install(stream, selector, process, output):
            send_command(stream, "install disk1 erase-disk1")
            wait_for_output(
                selector,
                process,
                output,
                b"MAKOS_INSTALL_BEGIN source=disk0 target=disk1 mode=fresh",
                180,
            )
            wait_for_output(
                selector, process, output, b"MAKOS_INSTALL_PROGRESS", INSTALL_TIMEOUT
            )
            process.kill()
            process.wait(timeout=10)

        interrupted = run_once(
            command(qemu, accel, code, fresh_vars(vars_template, root, "install"), source, install_qmp, target),
            install_qmp,
            interrupt_install,
        )
        if b"MAKOS_AARCH64_BLOCK_ENUM_OK devices=2" not in interrupted:
            raise AssertionError("installer did not enumerate exactly two guest disks")
        copied_blocks = verify_interrupted_copy(source, target)

        resume_qmp = root / "resume.qmp"

        def resume_install(stream, selector, process, output):
            send_command(stream, "install disk1 resume-disk1")
            wait_for_output(
                selector,
                process,
                output,
                b"MAKOS_INSTALL_BEGIN source=disk0 target=disk1 mode=resume",
                180,
            )
            wait_for_output(selector, process, output, b"MAKOS_INSTALL_OK", INSTALL_TIMEOUT)

        run_once(
            command(
                qemu,
                accel,
                code,
                fresh_vars(vars_template, root, "resume"),
                source,
                resume_qmp,
                target,
            ),
            resume_qmp,
            resume_install,
        )
        if digest(target) != digest(source):
            raise AssertionError("resumed target differs from live source")

        # Boot only installed disk; live source is intentionally absent.
        first_qmp = root / "installed-first.qmp"

        def create(stream, selector, process, output):
            # With source detached, exact destructive command must still refuse
            # because no distinct target exists; boot disk remains device 0.
            send_command(stream, "install disk1 erase-disk1")
            wait_for_output(
                selector, process, output,
                b"MAKOS_INSTALL_ERROR error=DeviceCount",
                20,
            )
            send_command(stream, "write installed.txt guest-install")
            wait_for_output(
                selector, process, output,
                b"MAKOS_AARCH64_SHELL_CMD write bytes=13 persisted=1",
                20,
            )

        first = run_once(
            command(qemu, accel, code, fresh_vars(vars_template, root, "installed-first"), target, first_qmp),
            first_qmp,
            create,
        )
        if b"MAKOS_AARCH64_BLOCK_ENUM_OK devices=1" not in first:
            raise AssertionError("installed-only boot still saw live source")

        second_qmp = root / "installed-second.qmp"

        def verify(stream, selector, process, output):
            send_command(stream, "cat installed.txt")
            wait_for_output(
                selector, process, output,
                b"MAKOS_AARCH64_SHELL_CMD cat bytes=13",
                20,
            )

        second = run_once(
            command(qemu, accel, code, fresh_vars(vars_template, root, "installed-second"), target, second_qmp),
            second_qmp,
            verify,
        )
        # `cat` writes file bytes to guest framebuffer; serial evidence reports
        # exact byte count. Source image began without this path, so 13 bytes on
        # second installed-only boot proves creation survived target reboot.
        if b"MAKOS_AARCH64_SHELL_CMD cat bytes=13" not in second:
            raise AssertionError("installed system persistence payload missing")

    print(
        "MAKOS_AARCH64_INSTALL_BOOT_OK guest_initiated=1 target=disk1 explicit_confirmation=1 "
        "nonblank_refusal=1 committed_resume_refusal=1 conflict_resume_refusal=1 size_refusal=1 "
        "missing_target_refusal=1 gpt=1 esp=1 data=1 power_interrupt=pre-mbr "
        f"mbr_blank_after_interrupt=1 partial_blocks={copied_blocks} resume=1 "
        "source_digest_match=1 source_detached=1 persistence=two-boot"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
