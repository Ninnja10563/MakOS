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
    b"policy=interactive-leaders-cpu0,application-workers-shared-ap,"
    b"toolchain-leaders-least-loaded-ap roles=firefox,native,toolchain "
    b"device_mmio_owner=cpu0 "
    b"wake=sgi block=ap-idle"
)
PROCESS_MARKER = b"MAKOS_AARCH64_FIREFOX_SMP_PROCESS_OK"
RESULT_MARKER = b"MAKOS_AARCH64_PRODUCTION_SMP_OK"
OVERLAP_MARKER = b"MAKOS_AARCH64_FIREFOX_SMP_OVERLAP_OK"
WATCHER_BLOCKED_MARKER = b"MAKOS_AARCH64_FIREFOX_INPUT_WATCHER_BLOCKED_OK"
HANDLE_WAITERS_MARKER = (
    b"MAKOS_AARCH64_INPUT_HANDLE_WAITERS_OK firefox_waiters_at_least=2 "
    b"routing=exact-handle"
)
TARGET_SELECTION_MARKER = b"MAKOS_AARCH64_FIREFOX_INPUT_TARGET_OK"
TARGET_WAKE_MARKER = (
    b"MAKOS_AARCH64_INPUT_TARGET_WAKE_OK surface_woken=1 surface_skipped="
)
WATCHER_AP_MARKER = b"MAKOS_AARCH64_SURFACE_PRIORITY_AP_OK"
LEADER_DISPATCH_MARKER = b"MAKOS_AARCH64_SURFACE_MAIN_DISPATCH_OK"
INPUT_PRIORITY_MARKER = b"MAKOS_FIREFOX_SMP_INPUT_PRIORITY_OK"
INPUT_IRQ_MARKER = b"MAKOS_AARCH64_INPUT_IRQ_OK"
AFFINITY_MARKER = (
    b"affinity=default:0xe,explicit singleton=0x2,0x4,0x8 restored=0xe "
    b"get=kernel-owned placement=least-reserved-ap "
    b"migrations=automatic:load,forced:3 caller_selected_automatic=0"
)
AUTOMATIC_MIGRATION_MARKER = b"MAKOS_AARCH64_APPLICATION_MIGRATION_OK role=firefox"
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
    automatic_placements = (0, 0, 0)
    automatic_migrations = 0
    watcher_tid = 0
    watcher_cpu = 0
    surface_woken = 0
    surface_skipped = 0
    input_irq_intid = 0

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
                common.wait_for_output(
                    selector, process, output, WATCHER_BLOCKED_MARKER, 30
                )
                common.wait_for_output(
                    selector, process, output, HANDLE_WAITERS_MARKER, 30
                )
                common.send_key(stream, "ctrl-a")
                common.wait_for_output(selector, process, output, INPUT_IRQ_MARKER, 30)
                common.wait_for_output(
                    selector, process, output, TARGET_SELECTION_MARKER, 30
                )
                common.wait_for_output(
                    selector, process, output, TARGET_WAKE_MARKER, 30
                )
                common.wait_for_output(selector, process, output, WATCHER_AP_MARKER, 30)
                common.wait_for_output(
                    selector, process, output, LEADER_DISPATCH_MARKER, 30
                )
                common.wait_for_output(
                    selector, process, output, INPUT_PRIORITY_MARKER, 30
                )
                common.wait_for_output(selector, process, output, AFFINITY_MARKER, 20)
                common.wait_for_output(
                    selector, process, output, AUTOMATIC_MIGRATION_MARKER, 30
                )
                common.wait_for_output(selector, process, output, RESULT_MARKER, 60)
                common.wait_for_output(selector, process, output, REAP_MARKER, 20)

                decoded = output.decode(errors="replace")
                affinity_gets = {
                    (int(mask, 16), int(cpu))
                    for _tid, mask, cpu in re.findall(
                        r"MAKOS_AARCH64_THREAD_AFFINITY_OK tid=(\d+) "
                        r"operation=get mask=(0x[0-9a-f]+) cpu=(\d+)",
                        decoded,
                    )
                }
                expected_affinity_gets = {(0x1, 0), (0x2, 1), (0x4, 2), (0x8, 3)}
                if not expected_affinity_gets.issubset(affinity_gets):
                    raise AssertionError(
                        "explicit affinity did not migrate every Firefox worker: "
                        f"observed={sorted(affinity_gets)}"
                    )
                forced_migrations = re.findall(
                    r"MAKOS_AARCH64_THREAD_AFFINITY_OK tid=(\d+) operation=set "
                    r"old_mask=(0x[0-9a-f]+) mask=(0x[0-9a-f]+) "
                    r"source_cpu=(\d+) migrate=1",
                    decoded,
                )
                if len(forced_migrations) < 3:
                    raise AssertionError(
                        "three explicit Firefox worker migrations were not observed: "
                        f"{forced_migrations}"
                    )
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
                watcher_matches = re.findall(
                    r"MAKOS_AARCH64_FIREFOX_INPUT_WATCHER_BLOCKED_OK tid=(\d+) "
                    r"group_pid=(\d+) cpu=(\d+) wait=surface-event retry=svc",
                    decoded,
                )
                priority_matches = re.findall(
                    r"MAKOS_AARCH64_SURFACE_PRIORITY_AP_OK tid=(\d+) cpu=(\d+) "
                    r"target=firefox-input-watcher affinity=nonleader-ap "
                    r"ownership=exclusive",
                    decoded,
                )
                leader_matches = re.findall(
                    r"MAKOS_AARCH64_SURFACE_MAIN_DISPATCH_OK tid=(\d+) cpu=0 "
                    r"source=bounded-handoff affinity=leader-cpu0",
                    decoded,
                )
                target_matches = re.findall(
                    r"MAKOS_AARCH64_FIREFOX_INPUT_TARGET_OK tid=(\d+) "
                    r"handle=(\d+) selection=queued-surface",
                    decoded,
                )
                target_wake_matches = re.findall(
                    r"MAKOS_AARCH64_INPUT_TARGET_WAKE_OK surface_woken=(\d+) "
                    r"surface_skipped=(\d+) selection=queued-handle",
                    decoded,
                )
                input_irq_matches = re.findall(
                    r"MAKOS_AARCH64_INPUT_IRQ_OK intid=(\d+) cpu=0 "
                    r"entry=lower-el dispatch=direct source=virtio-mmio "
                    r"timer_fallback=100hz",
                    decoded,
                )
                if (
                    not watcher_matches
                    or not priority_matches
                    or not leader_matches
                    or not target_matches
                    or not target_wake_matches
                    or not input_irq_matches
                ):
                    raise AssertionError("production input-priority fields were malformed")
                overlap_group, marker_mask, mt1, mt2, mt3 = overlap_matches[-1]
                if overlap_group != process_matches[-1]:
                    raise AssertionError("production SMP overlap group did not match launch")
                blocked_tid, blocked_group, _blocked_cpu = watcher_matches[-1]
                priority_tid, priority_cpu = priority_matches[-1]
                leader_tid = leader_matches[-1]
                target_tid, target_handle = target_matches[-1]
                surface_woken, surface_skipped = map(int, target_wake_matches[-1])
                input_irq_intid = int(input_irq_matches[-1])
                watcher_tid = int(priority_tid)
                watcher_cpu = int(priority_cpu)
                if (
                    blocked_group != process_matches[-1]
                    or target_tid != priority_tid
                    or int(target_handle) != 7
                    or surface_woken != 1
                    or surface_skipped < 1
                    or input_irq_intid not in (77, 78)
                    or leader_tid != process_matches[-1]
                    or watcher_tid == int(leader_tid)
                    or watcher_cpu not in (1, 2, 3)
                ):
                    raise AssertionError(
                        "Firefox watcher/leader affinity handoff was inconsistent: "
                        f"blocked={watcher_matches[-1]} priority={priority_matches[-1]} "
                        f"target={target_matches[-1]} leader={leader_matches[-1]}"
                    )
                matches = re.findall(
                    r"MAKOS_AARCH64_PRODUCTION_SMP_OK cpu_mask=(0x[0-9a-f]+) "
                    r"dispatches=(\d+),(\d+),(\d+) overlap_mask=(0x[0-9a-f]+) "
                    r"overlap_tids=(\d+),(\d+),(\d+) worker_role=firefox "
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
                if not matches:
                    raise AssertionError("production SMP result fields were malformed")
                (
                    mask_text, d1, d2, d3, overlap_text, t1, t2, t3,
                    p1, p2, p3, placement_text, migration_text,
                    source_text, target_text, delta_text, drops_text,
                ) = matches[-1]
                cpu_mask = int(mask_text, 16)
                dispatches = (int(d1), int(d2), int(d3))
                overlap_mask = int(overlap_text, 16) & 0xE
                overlap_tids = (int(t1), int(t2), int(t3))
                automatic_placements = (int(p1), int(p2), int(p3))
                automatic_migrations = int(migration_text)
                placement_mask = int(placement_text, 16)
                source_mask = int(source_text, 16)
                target_mask = int(target_text, 16)
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
                        "kernel-owned Firefox worker balancing evidence was incomplete: "
                        f"placements={automatic_placements} mask={placement_mask:#x} "
                        f"migrations={automatic_migrations} source={source_mask:#x} "
                        f"target={target_mask:#x} delta={delta_text} drops={drops_text}"
                    )
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
        f"automatic_placements={automatic_placements[0]},{automatic_placements[1]},{automatic_placements[2]} "
        f"automatic_migrations={automatic_migrations} "
        f"input_watcher_tid={watcher_tid} input_watcher_cpu={watcher_cpu} "
        "fixture=upstream-musl-pthread role=firefox leader_cpu=0 "
        "input_priority=watcher-ap,leader-cpu0 key=ctrl-a routing=exact-handle "
        f"input_irq_intid={input_irq_intid} delivery=gicv2-spi "
        f"surface_woken={surface_woken} surface_skipped={surface_skipped} "
        "decoy=blocked-until-destroy "
        "automatic_policy=least-reserved-ap,timer-safe-dispatch-imbalance "
        "caller_selected_automatic=0 explicit_affinity=authoritative "
        "device_mmio_owner=cpu0 ownership=exclusive concurrent=1 block=ap-idle status=42"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
