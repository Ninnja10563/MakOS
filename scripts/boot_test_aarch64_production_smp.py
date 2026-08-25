#!/usr/bin/env python3
"""Focused runtime proof for bounded production AArch64 userspace SMP."""

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

import boot_test_aarch64 as common


ROOT = pathlib.Path(__file__).resolve().parent.parent
IMAGE = pathlib.Path(
    os.environ.get("MAKOS_AARCH64_IMAGE", ROOT / "build/makos-aarch64.img")
)
READY_MARKER = (
    b"MAKOS_AARCH64_PRODUCTION_SMP_READY userspace_scheduler_cpus=4 "
    b"policy=leader-cpu0,firefox-workers-shared-ap device_mmio_owner=cpu0 "
    b"wake=sgi block=ap-idle"
)
PROCESS_MARKER = b"MAKOS_AARCH64_FIREFOX_SMP_PROCESS_OK"
RESULT_MARKER = b"MAKOS_AARCH64_PRODUCTION_SMP_OK"
OVERLAP_MARKER = b"MAKOS_AARCH64_FIREFOX_SMP_OVERLAP_OK"
REAP_MARKER = (
    b"MAKOS_AARCH64_FIREFOX_SMP_REAP_OK fixture=upstream-musl-pthread "
    b"role=firefox status=42"
)


def main() -> int:
    if not IMAGE.is_file():
        raise FileNotFoundError(f"AArch64 boot image not found: {IMAGE}")
    qemu = os.environ.get("QEMU_SYSTEM_AARCH64", "qemu-system-aarch64")
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
    accel = os.environ.get(
        "MAKOS_AARCH64_ACCEL",
        "hvf"
        if platform.system() == "Darwin" and platform.machine() == "arm64"
        else "tcg",
    )
    output_root = pathlib.Path(
        os.environ.get("MAKOS_AARCH64_TEMP_ROOT", ROOT / "build")
    )
    serial_log = ROOT / "build/makos-production-smp-focused-serial.log"
    cpu_mask = 0
    dispatches = (0, 0, 0)
    overlap_mask = 0
    overlap_tids = (0, 0, 0)

    with tempfile.TemporaryDirectory(
        prefix="makos-production-smp-focused-", dir=output_root
    ) as name:
        temporary = pathlib.Path(name)
        boot = temporary / "boot.img"
        data = temporary / "data.img"
        variables = temporary / "vars.fd"
        shutil.copyfile(IMAGE, boot)
        shutil.copyfile(vars_template, variables)
        with data.open("wb") as output_file:
            output_file.truncate(1024 * 1024 * 1024)

        qmp_parent, qmp_child = socket.socketpair()
        command = [qemu]
        qemu_data = os.environ.get("MAKOS_QEMU_DATA_DIR")
        if qemu_data:
            command.extend(("-L", qemu_data))
        command.extend(
            (
                "-machine", f"virt,accel={accel},highmem=off,gic-version=2",
                "-cpu", "host" if accel == "hvf" else "max",
                "-global", "virtio-mmio.force-legacy=false",
                "-smp", "4", "-m", "1G",
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
                "-display", "none", "-serial", "stdio", "-monitor", "none",
                "-chardev", f"socket,id=makosqmp,fd={qmp_child.fileno()}",
                "-qmp", "chardev:makosqmp", "-no-reboot", "-no-shutdown",
            )
        )
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
            common.wait_for_output(
                selector, process, output, b"MAKOS_AARCH64_BOOT_OK", 90
            )
            with qmp_parent:
                stream = qmp_parent.makefile("rwb", buffering=0)
                json.loads(stream.readline())
                if "error" in common.qmp_command(stream, "qmp_capabilities"):
                    raise AssertionError("QMP capability negotiation failed")
                common.click_pointer(stream, 390, 220)
                common.wait_for_output(
                    selector, process, output, b"MAKOS_LOGIN_CLICK_OK", 20
                )
                for key in (
                    "m", "a", "r", "c", "u", "s", "tab",
                    "m", "a", "k", "o", "s", "ret",
                ):
                    common.send_key(stream, key)
                common.wait_for_output(
                    selector, process, output, b"MAKOS_AARCH64_DESKTOP_OK", 90
                )
                common.wait_for_output(selector, process, output, READY_MARKER, 20)
                common.wait_for_output(
                    selector, process, output, b"MAKOS_AARCH64_FILES_OK", 30
                )
                common.click_pointer(stream, 250, 580)
                common.wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_TASKBAR_APP_OK surface=2 activate=1 minimize_toggle=1",
                    20,
                )
                common.send_command(stream, "firefox-smp")
                common.wait_for_output(selector, process, output, PROCESS_MARKER, 30)
                common.wait_for_output(selector, process, output, OVERLAP_MARKER, 60)
                common.wait_for_output(selector, process, output, RESULT_MARKER, 60)
                common.wait_for_output(selector, process, output, REAP_MARKER, 20)

                decoded = output.decode(errors="replace")
                process_matches = re.findall(
                    r"MAKOS_AARCH64_FIREFOX_SMP_PROCESS_OK pid=(\d+)", decoded
                )
                overlap_matches = re.findall(
                    r"MAKOS_AARCH64_FIREFOX_SMP_OVERLAP_OK group_pid=(\d+) "
                    r"cpu_mask=(0x[0-9a-f]+) tids=(\d+),(\d+),(\d+)",
                    decoded,
                )
                if not process_matches or not overlap_matches:
                    raise AssertionError("production SMP overlap fields were malformed")
                overlap_group, marker_mask, mt1, mt2, mt3 = overlap_matches[-1]
                if overlap_group != process_matches[-1]:
                    raise AssertionError("production SMP overlap group did not match launch")
                matches = re.findall(
                    r"MAKOS_AARCH64_PRODUCTION_SMP_OK cpu_mask=(0x[0-9a-f]+) "
                    r"dispatches=(\d+),(\d+),(\d+) overlap_mask=(0x[0-9a-f]+) "
                    r"overlap_tids=(\d+),(\d+),(\d+) worker_role=firefox "
                    r"leader_cpu=0 run_queue=shared-ready ownership=exclusive "
                    r"block=ap-idle status=42",
                    decoded,
                )
                if not matches:
                    raise AssertionError("production SMP result fields were malformed")
                mask_text, d1, d2, d3, overlap_text, t1, t2, t3 = matches[-1]
                cpu_mask = int(mask_text, 16)
                dispatches = (int(d1), int(d2), int(d3))
                overlap_mask = int(overlap_text, 16) & 0xE
                overlap_tids = (int(t1), int(t2), int(t3))
                marker_overlap_mask = int(marker_mask, 16) & 0xE
                marker_overlap_tids = (int(mt1), int(mt2), int(mt3))
                if marker_overlap_mask != overlap_mask or marker_overlap_tids != overlap_tids:
                    raise AssertionError("production SMP live/final overlap evidence disagreed")
                if cpu_mask & 0xE == 0:
                    raise AssertionError(
                        f"Firefox-role worker never ran on a secondary CPU: {mask_text}"
                    )
                if sum(dispatches) == 0:
                    raise AssertionError("production AP dispatch counter stayed at zero")
                active_tids = [
                    overlap_tids[cpu - 1]
                    for cpu in range(1, 4)
                    if overlap_mask & (1 << cpu)
                ]
                if (
                    overlap_mask.bit_count() < 2
                    or any(tid == 0 for tid in active_tids)
                    or len(active_tids) != len(set(active_tids))
                ):
                    raise AssertionError(
                        "production Firefox-role workers did not overlap as distinct TIDs: "
                        f"mask={overlap_mask:#x} tids={overlap_tids}"
                    )
                common.qmp_command(stream, "quit")
            process.wait(timeout=10)
        finally:
            serial_log.write_bytes(output)
            selector.close()
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=10)

    worker_cpus = (cpu_mask & 0xE).bit_count()
    print(
        "MAKOS_AARCH64_PRODUCTION_SMP_RUNTIME_OK "
        f"accel={accel} cpu_mask={cpu_mask:#x} worker_cpus={worker_cpus} "
        f"dispatches={dispatches[0]},{dispatches[1]},{dispatches[2]} "
        f"overlap_mask={overlap_mask:#x} overlap_tids={overlap_tids[0]},{overlap_tids[1]},{overlap_tids[2]} "
        "fixture=upstream-musl-pthread role=firefox leader_cpu=0 "
        "device_mmio_owner=cpu0 ownership=exclusive concurrent=1 block=ap-idle status=42"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
