#!/usr/bin/env python3
"""Focused AArch64 runtime proof that cursor motion never mutates scanout."""

from __future__ import annotations

import json
import os
import pathlib
import platform
import re
import selectors
import shutil
import socket
import subprocess
import tempfile
import time

from boot_test_aarch64 import (
    copy_sparse,
    first_file,
    qmp_command,
    send_pointer,
    verify_cursor_plane_scene_stable,
    wait_for_output,
)


ROOT = pathlib.Path(__file__).resolve().parent.parent
IMAGE = pathlib.Path(
    os.environ.get("MAKOS_AARCH64_IMAGE", ROOT / "build/makos-aarch64.img")
)
POSITIONS = (
    (40, 80),
    (200, 110),
    (400, 200),
    (700, 400),
    (130, 575),
    (500, 575),
    (760, 540),
)


def main() -> int:
    qemu = os.environ.get("QEMU_SYSTEM_AARCH64", "qemu-system-aarch64")
    code = first_file(
        "AAVMF_CODE",
        (
            "/opt/homebrew/share/qemu/edk2-aarch64-code.fd",
            "/usr/local/share/qemu/edk2-aarch64-code.fd",
            "/usr/share/AAVMF/AAVMF_CODE.fd",
        ),
    )
    vars_template = first_file(
        "AAVMF_VARS",
        (
            "/opt/homebrew/share/qemu/edk2-arm-vars.fd",
            "/usr/local/share/qemu/edk2-arm-vars.fd",
            "/usr/share/AAVMF/AAVMF_VARS.fd",
        ),
    )
    accel = os.environ.get(
        "MAKOS_AARCH64_ACCEL",
        "hvf" if platform.system() == "Darwin" and platform.machine() == "arm64" else "tcg",
    )
    output_dir = ROOT / "build"
    output_dir.mkdir(parents=True, exist_ok=True)
    baseline = output_dir / "makos-cursor-focused-baseline.ppm"
    snapshots = tuple(
        output_dir / f"makos-cursor-focused-move-{index}.ppm"
        for index in range(len(POSITIONS))
    )
    serial_log = output_dir / "makos-cursor-focused-serial.log"

    with tempfile.TemporaryDirectory(prefix="makos-cursor-focused-", dir=output_dir) as name:
        temp = pathlib.Path(name)
        boot = temp / "boot.img"
        data = temp / "data.img"
        variables = temp / "vars.fd"
        shutil.copyfile(IMAGE, boot)
        shutil.copyfile(vars_template, variables)
        package = os.environ.get("MAKOS_AARCH64_PACKAGE_IMAGE")
        if package:
            copy_sparse(pathlib.Path(package), data)
        else:
            with data.open("wb") as output:
                output.truncate(1024 * 1024 * 1024)

        qmp_parent, qmp_child = socket.socketpair()
        command = [
            qemu,
            "-machine", f"virt,accel={accel},highmem=off,gic-version=2",
            "-cpu", "host" if accel == "hvf" else "max",
            "-global", "virtio-mmio.force-legacy=false",
            "-smp", "4",
            "-m", "1G",
            "-drive", f"if=pflash,format=raw,readonly=on,file={code}",
            "-drive", f"if=pflash,format=raw,file={variables}",
            "-drive", f"id=boot,if=none,format=raw,readonly=on,file={boot}",
            "-device", "virtio-blk-pci,drive=boot",
            "-drive", f"id=data,if=none,format=raw,file={data}",
            "-device", "virtio-blk-device,drive=data",
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
            "-chardev", f"socket,id=makosqmp,fd={qmp_child.fileno()}",
            "-qmp", "chardev:makosqmp",
            "-no-reboot",
            "-no-shutdown",
        ]
        process = subprocess.Popen(
            command,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            pass_fds=(qmp_child.fileno(),),
        )
        qmp_child.close()
        assert process.stdout is not None
        selector = selectors.DefaultSelector()
        selector.register(process.stdout, selectors.EVENT_READ)
        output = bytearray()
        try:
            wait_for_output(
                selector,
                process,
                output,
                b"MAKOS_AARCH64_BOOT_OK uefi=1 hvf_ready=1 native_isa=1",
                60,
            )
            for marker in (
                b"cursor=virtio-gpu-plane move=cursorq scanout_damage=none host-cursor=hidden",
                b"MAKOS_LOGIN_UI_OK framebuffer=800x600 prompt=visible console=live cursor=virtio-gpu-plane",
            ):
                if marker not in output:
                    raise AssertionError(
                        f"missing cursor runtime marker {marker!r}\n"
                        f"{output.decode(errors='replace')}"
                    )
            with qmp_parent:
                stream = qmp_parent.makefile("rwb", buffering=0)
                json.loads(stream.readline())
                if "error" in qmp_command(stream, "qmp_capabilities"):
                    raise AssertionError("QMP capability negotiation failed")

                send_pointer(stream, *POSITIONS[-1])
                time.sleep(0.15)
                response = qmp_command(
                    stream, "screendump", {"filename": str(baseline)}
                )
                if "error" in response:
                    raise AssertionError(f"baseline screendump failed: {response}")
                for path, position in zip(snapshots, POSITIONS):
                    send_pointer(stream, *position)
                    response = qmp_command(
                        stream, "screendump", {"filename": str(path)}
                    )
                    if "error" in response:
                        raise AssertionError(
                            f"cursor screendump failed at {position}: {response}"
                        )
                verify_cursor_plane_scene_stable(baseline, snapshots)
                if b"MAKOS_AARCH64_GPU_TIMEOUT" in output:
                    raise AssertionError("virtio-GPU completion timed out")
                if b"MAKOS_AARCH64_GPU_ERROR" in output:
                    raise AssertionError("virtio-GPU returned an invalid descriptor")
                delayed = re.findall(
                    rb"MAKOS_AARCH64_GPU_DELAYED queue=(control|cursor) command=0x([0-9a-f]+)",
                    output,
                )
                recovered = re.findall(
                    rb"MAKOS_AARCH64_GPU_RECOVERED queue=(control|cursor) command=0x([0-9a-f]+)",
                    output,
                )
                if sorted(delayed) != sorted(recovered):
                    raise AssertionError(
                        "unpaired delayed GPU completions: "
                        f"delayed={delayed} recovered={recovered}"
                    )
                qmp_command(stream, "quit")
            process.wait(timeout=5)
        finally:
            serial_log.write_bytes(output)
            selector.close()
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=5)

    print(
        "MAKOS_AARCH64_CURSOR_RUNTIME_OK "
        f"accel={accel} positions={len(POSITIONS)} changed_scanout_pixels=0 "
        "backend=virtio-gpu-plane host_cursor=hidden "
        f"completion=fast-plus-bounded-recovery delayed_recoveries={len(recovered)} timeouts=0 errors=0"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
