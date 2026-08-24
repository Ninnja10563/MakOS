#!/usr/bin/env python3
"""Guest-triggered x86_64 install, power-cut resume, detached boot proof."""

from __future__ import annotations

import hashlib
import os
import pathlib
import selectors
import shutil
import subprocess
import tempfile
import time

import boot_test

ROOT = pathlib.Path(__file__).resolve().parent.parent
IMAGE = pathlib.Path(
    os.environ.get("MAKOS_X86_64_GPT_IMAGE", ROOT / "build/makos-x86_64-gpt.img")
)
BOOT_TIMEOUT = 180
INSTALL_TIMEOUT = 1200


def copy_sparse(source: pathlib.Path, destination: pathlib.Path) -> None:
    with source.open("rb") as input_file, destination.open("wb") as output_file:
        while block := input_file.read(1024 * 1024):
            if block.count(0) == len(block):
                output_file.seek(len(block), 1)
            else:
                output_file.write(block)
        output_file.truncate(source.stat().st_size)


def blank_disk(path: pathlib.Path, size: int) -> None:
    with path.open("wb") as output:
        output.truncate(size)


def seed_resume_probe(path: pathlib.Path) -> None:
    marker = b"MAKOS_INSTALL_RESUME_PROBE\0"
    block = (marker * (4096 // len(marker) + 1))[:4096]
    with path.open("r+b") as output:
        output.seek(34 * 512)
        output.write(block)


def digest(path: pathlib.Path) -> str:
    checksum = hashlib.sha256()
    with path.open("rb") as source:
        while block := source.read(1024 * 1024):
            checksum.update(block)
    return checksum.hexdigest()


def create_overlay(qemu_img: str, backing: pathlib.Path, overlay: pathlib.Path) -> None:
    subprocess.run(
        [qemu_img, "create", "-q", "-f", "qcow2", "-F", "raw", "-b", str(backing), str(overlay)],
        check=True,
    )


def export_overlay(qemu_img: str, overlay: pathlib.Path, output: pathlib.Path) -> None:
    subprocess.run(
        [qemu_img, "convert", "-q", "-f", "qcow2", "-O", "raw", str(overlay), str(output)],
        check=True,
    )


def command(
    qemu: str,
    source: pathlib.Path,
    monitor: pathlib.Path,
    target: pathlib.Path | None = None,
    *,
    source_format: str = "raw",
) -> list[str]:
    result = [
        qemu,
        "-machine", "pc,accel=tcg",
        "-cpu", "qemu64",
        "-smp", "4",
        "-m", "256M",
        "-drive", f"if=pflash,format=raw,readonly=on,file={boot_test.firmware()}",
        "-drive", f"id=source,if=none,format={source_format},cache=writethrough,file={source}",
        "-device", "ide-hd,drive=source,bus=ide.1,unit=0,bootindex=0",
    ]
    if target is not None:
        result += [
            "-drive", f"id=target,if=none,format=raw,cache=writethrough,file={target}",
            "-device", "ide-hd,drive=target,bus=ide.1,unit=1,bootindex=1",
        ]
    result += [
        "-display", "none",
        "-serial", "stdio",
        "-monitor", f"unix:{monitor},server=on,wait=off",
        "-netdev", "user,id=net0",
        "-device", "rtl8139,netdev=net0",
        "-audiodev", "driver=none,id=audio0",
        "-device", "AC97,audiodev=audio0",
        "-device", "piix3-usb-uhci,id=usb",
        "-device", "usb-kbd,bus=usb.0",
        "-no-reboot",
        "-no-shutdown",
    ]
    return result


def wait_for(
    selector: selectors.BaseSelector,
    process: subprocess.Popen[bytes],
    output: bytearray,
    marker: bytes,
    timeout: float,
) -> None:
    deadline = time.monotonic() + timeout
    while marker not in output and time.monotonic() < deadline:
        for key, _ in selector.select(timeout=0.5):
            chunk = os.read(key.fileobj.fileno(), 4096)
            if chunk:
                output.extend(chunk)
        if process.poll() is not None:
            break
    if marker not in output:
        raise AssertionError(
            f"missing {marker!r}; tail={bytes(output[-4096:]).decode(errors='replace')}"
        )


def run_once(arguments: list[str], monitor: pathlib.Path, action) -> bytes:
    monitor.unlink(missing_ok=True)
    process = subprocess.Popen(arguments, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    assert process.stdout is not None
    selector = selectors.DefaultSelector()
    selector.register(process.stdout, selectors.EVENT_READ)
    output = bytearray()
    try:
        wait_for(selector, process, output, b"MAKOS_LOGIN_READY", BOOT_TIMEOUT)
        boot_test.click_login(monitor)
        boot_test.send_keyboard_lines(monitor, ("marcus", "makos"))
        wait_for(selector, process, output, b"MAKOS_SHELL_READY", BOOT_TIMEOUT)
        action(selector, process, output)
        return bytes(output)
    finally:
        selector.close()
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)


def send_and_wait(
    monitor: pathlib.Path,
    selector: selectors.BaseSelector,
    process: subprocess.Popen[bytes],
    output: bytearray,
    command_text: str,
    marker: bytes,
    timeout: float = 60,
) -> None:
    boot_test.send_keyboard_lines(monitor, (command_text,))
    wait_for(selector, process, output, marker, timeout)


def verify_interrupted_copy(source: pathlib.Path, target: pathlib.Path) -> int:
    copied_blocks = 0
    with source.open("rb") as source_file, target.open("rb") as target_file:
        if target_file.read(512) != bytes(512):
            raise AssertionError("interrupted install committed LBA0")
        source_file.seek(34 * 512)
        target_file.seek(34 * 512)
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
    if not IMAGE.is_file():
        raise FileNotFoundError(IMAGE)
    qemu = os.environ.get("QEMU_SYSTEM_X86_64") or shutil.which("qemu-system-x86_64")
    if not qemu:
        raise RuntimeError("qemu-system-x86_64 not found")
    qemu_img = shutil.which("qemu-img")
    if not qemu_img:
        raise RuntimeError("qemu-img not found")

    with tempfile.TemporaryDirectory(prefix="xi-", dir=ROOT / "build") as temporary:
        root = pathlib.Path(temporary)
        source_template = root / "live-template.img"
        copy_sparse(IMAGE, source_template)
        disk_size = source_template.stat().st_size

        occupied = root / "occupied.img"
        blank_disk(occupied, disk_size)
        with occupied.open("r+b") as output:
            output.write(b"occupied")
        occupied_before = digest(occupied)
        source = root / "live-refusal.img"
        copy_sparse(source_template, source)
        refusal_monitor = root / "r.sock"

        def refuse(selector, process, output):
            send_and_wait(
                refusal_monitor, selector, process, output, "install disk1 confirm",
                b"MAKOS_X86_INSTALL_CONFIRMATION_DENIED target=disk1 expected=erase-disk1 destructive_io=0",
            )
            send_and_wait(
                refusal_monitor, selector, process, output, "install disk1 erase-disk1",
                b"MAKOS_X86_INSTALL_ERROR error=TargetNotBlank",
            )

        run_once(command(qemu, source, refusal_monitor, occupied), refusal_monitor, refuse)
        if digest(occupied) != occupied_before:
            raise AssertionError("occupied target changed despite refusal")

        wrong_size = root / "wrong-size.img"
        blank_disk(wrong_size, disk_size - 512)
        wrong_before = digest(wrong_size)
        source = root / "live-geometry.img"
        copy_sparse(source_template, source)
        geometry_monitor = root / "g.sock"

        def refuse_geometry(selector, process, output):
            send_and_wait(
                geometry_monitor, selector, process, output, "install disk1 erase-disk1",
                b"MAKOS_X86_INSTALL_ERROR error=Geometry",
            )

        run_once(command(qemu, source, geometry_monitor, wrong_size), geometry_monitor, refuse_geometry)
        if digest(wrong_size) != wrong_before:
            raise AssertionError("wrong-sized target changed despite refusal")

        source = root / "live-install.img"
        copy_sparse(source_template, source)
        prepare_monitor = root / "p.sock"
        run_once(command(qemu, source, prepare_monitor), prepare_monitor, lambda *_: None)
        seed_resume_probe(source)

        install_source = root / "live-install-overlay.qcow2"
        create_overlay(qemu_img, source, install_source)

        target = root / "installed.img"
        blank_disk(target, disk_size)
        install_monitor = root / "i.sock"

        def interrupt_install(selector, process, output):
            send_and_wait(
                install_monitor, selector, process, output, "install disk1 erase-disk1",
                b"MAKOS_X86_INSTALL_BEGIN source=disk0 target=disk1 mode=fresh", 180,
            )
            wait_for(selector, process, output, b"MAKOS_X86_INSTALL_PROGRESS", INSTALL_TIMEOUT)
            process.kill()
            process.wait(timeout=10)

        run_once(
            command(qemu, install_source, install_monitor, target, source_format="qcow2"),
            install_monitor,
            interrupt_install,
        )
        interrupted_source = root / "interrupted-source.img"
        export_overlay(qemu_img, install_source, interrupted_source)
        copied_blocks = verify_interrupted_copy(interrupted_source, target)

        resume_monitor = root / "u.sock"
        resume_source = root / "live-resume-overlay.qcow2"
        create_overlay(qemu_img, source, resume_source)

        def resume_install(selector, process, output):
            send_and_wait(
                resume_monitor, selector, process, output, "install disk1 resume-disk1",
                b"MAKOS_X86_INSTALL_BEGIN source=disk0 target=disk1 mode=resume", 180,
            )
            wait_for(selector, process, output, b"MAKOS_X86_INSTALL_OK", INSTALL_TIMEOUT)

        run_once(
            command(qemu, resume_source, resume_monitor, target, source_format="qcow2"),
            resume_monitor,
            resume_install,
        )
        resumed_source = root / "resumed-source.img"
        export_overlay(qemu_img, resume_source, resumed_source)
        if digest(target) != digest(resumed_source):
            raise AssertionError("resumed target differs from immutable source")

        first_monitor = root / "f.sock"

        def installed_first(selector, process, output):
            send_and_wait(
                first_monitor, selector, process, output, "install disk1 erase-disk1",
                b"MAKOS_X86_INSTALL_ERROR error=DeviceCount",
            )
            send_and_wait(
                first_monitor, selector, process, output, "write installed.txt guest-install",
                b"MAKOS_SHELL_CMD write bytes=13 persisted=1",
            )

        run_once(command(qemu, target, first_monitor), first_monitor, installed_first)

        second_monitor = root / "s.sock"

        def installed_second(selector, process, output):
            send_and_wait(
                second_monitor, selector, process, output, "cat installed.txt",
                b"MAKOS_SHELL_CMD cat bytes=13",
            )

        run_once(command(qemu, target, second_monitor), second_monitor, installed_second)

    print(
        "MAKOS_X86_INSTALL_BOOT_OK guest_initiated=1 target=disk1 explicit_confirmation=1 "
        "nonblank_refusal=1 size_refusal=1 missing_target_refusal=1 gpt=1 esp=1 data=1 "
        f"power_interrupt=pre-mbr mbr_blank_after_interrupt=1 partial_blocks={copied_blocks} "
        "resume=1 source_digest_match=1 source_detached=1 persistence=two-boot"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
