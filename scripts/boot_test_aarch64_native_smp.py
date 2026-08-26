#!/usr/bin/env python3
"""Prove ordinary native application threads use production AArch64 APs."""

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
    b"policy=interactive-leaders-cpu0,application-workers-shared-ap,"
    b"toolchain-leaders-least-loaded-ap roles=firefox,native,toolchain "
    b"device_mmio_owner=cpu0 wake=sgi block=ap-idle"
)
PROCESS_MARKER = b"MAKOS_AARCH64_NATIVE_SMP_PROCESS_OK"
OVERLAP_MARKER = b"MAKOS_AARCH64_NATIVE_SMP_OVERLAP_OK"
PTHREAD_MARKER = (
    b"MAKOS_NATIVE_SMP_PTHREAD_OVERLAP_OK workers=3 rendezvous=ready "
    b"release=bounded affinity=default:0xe,explicit singleton=0x2,0x4,0x8 "
    b"restored=0xe get=kernel-owned placement=least-reserved-ap "
    b"migrations=automatic:load,forced:3 caller_selected_automatic=0"
)
AUTOMATIC_MIGRATION_MARKER = b"MAKOS_AARCH64_APPLICATION_MIGRATION_OK role=native"
RESULT_MARKER = b"MAKOS_AARCH64_NATIVE_SMP_OK"
REAP_MARKER = (
    b"MAKOS_AARCH64_NATIVE_SMP_REAP_OK fixture=upstream-musl-pthread "
    b"role=native status=42"
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
    variables_template = common.first_file(
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
    serial_log = ROOT / "build/makos-native-smp-focused-serial.log"
    cpu_mask = 0
    dispatches = (0, 0, 0)
    overlap_mask = 0
    overlap_tids = (0, 0, 0)
    automatic_placements = (0, 0, 0)
    automatic_migrations = 0

    with tempfile.TemporaryDirectory(
        prefix="makos-native-smp-focused-", dir=output_root
    ) as name:
        temporary = pathlib.Path(name)
        boot = temporary / "boot.img"
        data = temporary / "data.img"
        variables = temporary / "vars.fd"
        shutil.copyfile(IMAGE, boot)
        shutil.copyfile(variables_template, variables)
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
                common.send_command(stream, "native-smp")
                common.wait_for_output(selector, process, output, PROCESS_MARKER, 30)
                common.wait_for_output(selector, process, output, OVERLAP_MARKER, 60)
                common.wait_for_output(selector, process, output, PTHREAD_MARKER, 30)
                common.wait_for_output(
                    selector, process, output, AUTOMATIC_MIGRATION_MARKER, 30
                )
                common.wait_for_output(selector, process, output, RESULT_MARKER, 90)
                common.wait_for_output(selector, process, output, REAP_MARKER, 20)

                decoded = output.decode(errors="replace")
                process_matches = re.findall(
                    r"MAKOS_AARCH64_NATIVE_SMP_PROCESS_OK pid=(\d+)", decoded
                )
                overlap_matches = re.findall(
                    r"MAKOS_AARCH64_NATIVE_SMP_OVERLAP_OK group_pid=(\d+) "
                    r"cpu_mask=(0x[0-9a-f]+) tids=(\d+),(\d+),(\d+)",
                    decoded,
                )
                result_matches = re.findall(
                    r"MAKOS_AARCH64_NATIVE_SMP_OK cpu_mask=(0x[0-9a-f]+) "
                    r"dispatches=(\d+),(\d+),(\d+) overlap_mask=(0x[0-9a-f]+) "
                    r"overlap_tids=(\d+),(\d+),(\d+) worker_role=native "
                    r"leader_cpu=0 run_queue=shared-ready ownership=exclusive "
                    r"block=ap-idle automatic_placements=(\d+),(\d+),(\d+) "
                    r"automatic_placement_mask=(0x[0-9a-f]+) automatic_migrations=(\d+) "
                    r"automatic_source_mask=(0x[0-9a-f]+) "
                    r"automatic_target_mask=(0x[0-9a-f]+) "
                    r"automatic_policy=least-reserved-ap,timer-safe-dispatch-imbalance "
                    r"automatic_delta=(\d+) migration_evidence_drops=(\d+) "
                    r"caller_selected=0 explicit_affinity=authoritative status=42",
                    decoded,
                )
                if not process_matches or not overlap_matches or not result_matches:
                    raise AssertionError("native production-SMP fields were malformed")
                group, marker_mask, mt1, mt2, mt3 = overlap_matches[-1]
                (
                    mask_text, d1, d2, d3, final_mask, t1, t2, t3,
                    p1, p2, p3, placement_text, migration_text,
                    source_text, target_text, delta_text, drops_text,
                ) = result_matches[-1]
                if group != process_matches[-1]:
                    raise AssertionError("native overlap group did not match launch")
                cpu_mask = int(mask_text, 16) & 0xE
                dispatches = (int(d1), int(d2), int(d3))
                overlap_mask = int(final_mask, 16) & 0xE
                overlap_tids = (int(t1), int(t2), int(t3))
                automatic_placements = (int(p1), int(p2), int(p3))
                automatic_migrations = int(migration_text)
                placement_mask = int(placement_text, 16)
                source_mask = int(source_text, 16)
                target_mask = int(target_text, 16)
                marker_overlap_mask = int(marker_mask, 16) & 0xE
                marker_overlap_tids = (int(mt1), int(mt2), int(mt3))
                if cpu_mask != 0xE or any(value == 0 for value in dispatches):
                    raise AssertionError(
                        f"native workers did not use every AP: mask={cpu_mask:#x} "
                        f"dispatches={dispatches}"
                    )
                if (
                    placement_mask != 0xE
                    or any(value == 0 for value in automatic_placements)
                    or automatic_migrations < 1
                    or source_mask & 0xE == 0
                    or target_mask & 0xE == 0
                    or int(delta_text) != 64
                    or int(drops_text) != 0
                ):
                    raise AssertionError(
                        "native automatic-balancing evidence was incomplete: "
                        f"placements={automatic_placements} mask={placement_mask:#x} "
                        f"migrations={automatic_migrations} source={source_mask:#x} "
                        f"target={target_mask:#x} delta={delta_text} drops={drops_text}"
                    )
                if (
                    marker_overlap_mask != overlap_mask
                    or marker_overlap_tids != overlap_tids
                ):
                    raise AssertionError("native live/final overlap evidence disagreed")
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
                        "native workers did not overlap as distinct TIDs: "
                        f"mask={overlap_mask:#x} tids={overlap_tids}"
                    )
                affinity_gets = {
                    (int(mask, 16), int(cpu))
                    for _tid, mask, cpu in re.findall(
                        r"MAKOS_AARCH64_THREAD_AFFINITY_OK tid=(\d+) "
                        r"operation=get mask=(0x[0-9a-f]+) cpu=(\d+)",
                        decoded,
                    )
                }
                expected_gets = {(0x1, 0), (0x2, 1), (0x4, 2), (0x8, 3)}
                if not expected_gets.issubset(affinity_gets):
                    raise AssertionError(
                        "native affinity did not reach every CPU: "
                        f"observed={sorted(affinity_gets)}"
                    )
                common.qmp_command(stream, "quit")
            process.wait(timeout=10)
        finally:
            serial_log.write_bytes(output)
            selector.close()
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=10)

    print(
        "MAKOS_AARCH64_NATIVE_SMP_RUNTIME_OK "
        f"accel={accel} cpu_mask={cpu_mask:#x} "
        f"dispatches={dispatches[0]},{dispatches[1]},{dispatches[2]} "
        f"overlap_mask={overlap_mask:#x} "
        f"overlap_tids={overlap_tids[0]},{overlap_tids[1]},{overlap_tids[2]} "
        f"automatic_placements={automatic_placements[0]},{automatic_placements[1]},{automatic_placements[2]} "
        f"automatic_migrations={automatic_migrations} "
        "fixture=upstream-musl-pthread role=native leader_cpu=0 "
        "workers=shared-ap affinity=kernel-owned ownership=exclusive "
        "automatic_policy=least-reserved-ap,timer-safe-dispatch-imbalance "
        "caller_selected_automatic=0 explicit_affinity=authoritative "
        "device_mmio_owner=cpu0 status=42"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
