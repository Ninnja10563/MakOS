#!/usr/bin/env python3
"""Require actual AP UDP transmission through IRQ-masked and EL1 owner paths.

Only a retained private boot clone is armed with the opt-in fixture flag. The
host receives actual virtio/slirp datagrams; it does not supply guest results.
This is a native workload, not Firefox or Firefox performance qualification.
"""

from __future__ import annotations

import json
import os
import pathlib
import platform
import re
import selectors
import socket
import struct
import subprocess
import tempfile
import threading
import time

import boot_test_aarch64 as common
from boot_test_aarch64_el0_entry import sha256


ROOT = pathlib.Path(__file__).resolve().parents[1]
IMAGE = pathlib.Path(os.environ.get("MAKOS_AARCH64_IMAGE", ROOT / "build/makos-aarch64.img"))
PREFIX = "MAKOS_AARCH64_NET_OWNER_"
PAYLOADS = [f"MAKOS_NET_OWNER_{phase}\n".encode() for phase in (1, 2, 3)]
BASE_CONFIG = b"root=ata1 log=serial makfs.recover=auto\n"
PROBE_CONFIG = BASE_CONFIG + b"test.net-owner=required\n"
RESULT_END = (b"statuses=42,42,42 cleanup=reaped free_balance=1 transport=virtio-net-udp4 "
              b"scope=immutable-boot-probe\r\n")


def validate_output(decoded: str) -> tuple[tuple[int, int, int], int]:
    if any(marker in decoded for marker in ("MAKOS_FATAL", "MAKOS_PANIC", "MAKOS_FAILURE_DETAIL")):
        raise AssertionError("network owner probe reached a kernel fatal/panic")
    armed = list(re.finditer(r"(?m)^" + PREFIX
        + r"ARMED config=test.net-owner=required\r*\n", decoded))
    phases = list(re.finditer(r"(?m)^" + PREFIX
        + r"PHASE_OK phase=([123]) tid=(\d+) root=(0x[0-9a-f]+) "
          r"requester_cpu=1 service_cpu=0 timer_completions=(\d+) "
          r"lock_wait_completions=(\d+) busy_deferrals=(\d+) payload_bytes=18 "
          r"irq=(masked|enabled) proof=([a-z0-9-]+) transport=virtio-net-udp4\r*\n", decoded))
    results = list(re.finditer(r"(?m)^" + PREFIX
        + r"OK tids=(\d+),(\d+),(\d+) phases=1,2,3 requester_cpu=1 service_cpu=0 "
          r"owner_completions=3 ap_requests=3 timer_completions=2 lock_wait_completions=1 "
          r"busy_deferrals=([1-9]\d*) statuses=42,42,42 cleanup=reaped free_balance=1 "
          r"transport=virtio-net-udp4 scope=immutable-boot-probe\r*\n", decoded))
    for name, matches, count in (("ARMED", armed, 1), ("PHASE_OK", phases, 3), ("OK", results, 1)):
        if len(matches) != count or decoded.count(PREFIX + name) != count:
            raise AssertionError(f"expected {count} complete {PREFIX + name} records")
    if [int(record[1]) for record in phases] != [1, 2, 3] \
        or not armed[0].start() < phases[0].start() < phases[1].start() \
        < phases[2].start() < results[0].start():
        raise AssertionError("network owner phase records are out of order")
    tids = tuple(int(record[2]) for record in phases)
    if min(tids) <= 0 or len(set(tids)) != 3 or tuple(map(int, results[0].groups()[:3])) != tids:
        raise AssertionError("network owner workload identities disagree")
    proofs = ((0, 1, "masked", "socket-lock-contention"),
              (1, 0, "enabled", "el1-timer"),
              (1, 0, "enabled", "ready-locked-defer-unlock"))
    for index, (record, expected) in enumerate(zip(phases, proofs)):
        root = int(record[3], 16)
        if root == 0 or root % 4096:
            raise AssertionError("network owner workload root is invalid")
        observed = (int(record[4]), int(record[5]), record[7], record[8])
        if observed != expected or (index < 2 and int(record[6]) != 0) \
            or (index == 2 and int(record[6]) <= 0):
            raise AssertionError("network owner service source or recursive-deferral proof invalid")
        end = phases[index + 1].start() if index < 2 else results[0].start()
        reaps = list(re.finditer(
            rf"(?m)^process-reap arch=aarch64 pid={tids[index]} status=42 "
            r"closed_fds=\d+ closed_tty_fds=\d+ closed_surfaces=\d+ closed_sockets=\d+ "
            r"closed_epolls=\d+ closed_ipc_handles=\d+ closed_futex_waiters=\d+ "
            r"vm_regions=\d+ vm_pages=\d+ reclaimed_frames=([1-9]\d*)\r*\n",
            decoded[record.end():end + 1],
        ))
        if len(reaps) != 1:
            raise AssertionError("network owner task lacks one actual ordered status-42 reap")
    busy = int(results[0][4])
    if busy != int(phases[2][6]):
        raise AssertionError("network owner aggregate deferrals disagree with locked phase")
    return tids, busy


def config_offsets(image: pathlib.Path) -> tuple[int, int]:
    """Accept the supported mkfat one-cluster config, not arbitrary FAT writes."""
    with image.open("rb") as source:
        boot = source.read(512)
        if len(boot) != 512 or boot[510:512] != b"\x55\xaa" or boot[82:90] != b"FAT32   ":
            raise AssertionError("private probe boot is not the supported FAT32 image")
        sector = struct.unpack_from("<H", boot, 11)[0]
        reserved = struct.unpack_from("<H", boot, 14)[0]
        copies = boot[16]
        fat_sectors = struct.unpack_from("<I", boot, 36)[0]
        root_cluster = struct.unpack_from("<I", boot, 44)[0]
        if sector != 512 or boot[13] != 1 or reserved == 0 or copies != 2 or fat_sectors == 0 \
            or root_cluster < 2:
            raise AssertionError("unsupported FAT geometry for private probe config")
        data_offset = (reserved + copies * fat_sectors) * sector
        root_offset = data_offset + (root_cluster - 2) * sector
        source.seek(root_offset)
        directory = source.read(sector)
        entries = [(offset, directory[offset:offset + 32]) for offset in range(0, len(directory), 32)
                   if directory[offset:offset + 11] == b"MAKOS   CFG"]
        if len(entries) != 1 or len(entries[0][1]) != 32 or entries[0][1][11] != 0x20:
            raise AssertionError("private probe boot lacks one regular MAKOS.CFG")
        entry_offset, entry = entries[0]
        cluster = (struct.unpack_from("<H", entry, 20)[0] << 16) | struct.unpack_from("<H", entry, 26)[0]
        size = struct.unpack_from("<I", entry, 28)[0]
        if cluster < 2 or size != len(BASE_CONFIG) or len(PROBE_CONFIG) > sector:
            raise AssertionError("unsupported config allocation or original config size")
        for copy in range(copies):
            source.seek((reserved + copy * fat_sectors) * sector + cluster * 4)
            entry_bytes = source.read(4)
            if len(entry_bytes) != 4 or struct.unpack("<I", entry_bytes)[0] & 0x0fffffff < 0x0ffffff8:
                raise AssertionError("private probe config must occupy one complete cluster")
        config_offset = data_offset + (cluster - 2) * sector
        source.seek(config_offset)
        if source.read(size) != BASE_CONFIG:
            raise AssertionError("original boot config differs from supported production config")
        return root_offset + entry_offset + 28, config_offset


def assert_config_only_clone(source: pathlib.Path, private: pathlib.Path,
                             size_offset: int, config_offset: int) -> None:
    allowed = ((size_offset, 4), (config_offset, len(PROBE_CONFIG)))
    with source.open("rb") as original, private.open("rb") as modified:
        position = 0
        while True:
            before = original.read(1024 * 1024)
            after = bytearray(modified.read(1024 * 1024))
            if len(before) != len(after):
                raise AssertionError("private boot size changed")
            if not before:
                break
            for start, length in allowed:
                left, right = max(start, position), min(start + length, position + len(before))
                if left < right:
                    after[left - position:right - position] = before[left - position:right - position]
            if before != after:
                raise AssertionError("private boot differs outside the opt-in config")
            position += len(before)
    with private.open("rb") as modified:
        modified.seek(size_offset)
        if modified.read(4) != struct.pack("<I", len(PROBE_CONFIG)):
            raise AssertionError("private config directory size invalid")
        modified.seek(config_offset)
        if modified.read(len(PROBE_CONFIG)) != PROBE_CONFIG:
            raise AssertionError("private opt-in config invalid")


def arm_private_boot(source: pathlib.Path, private: pathlib.Path) -> None:
    if source.resolve() == private.resolve() or private.exists():
        raise AssertionError("probe boot destination must be new and private")
    size_offset, config_offset = config_offsets(source)
    common.copy_sparse(source, private)
    with private.open("r+b") as modified:
        modified.seek(size_offset)
        modified.write(struct.pack("<I", len(PROBE_CONFIG)))
        modified.seek(config_offset)
        modified.write(PROBE_CONFIG)
    assert_config_only_clone(source, private, size_offset, config_offset)


def receive_packets(listener: socket.socket, stop: threading.Event, result: dict) -> None:
    packets = []
    result["packets"] = packets
    listener.settimeout(0.25)
    deadline = time.monotonic() + 90
    try:
        while len(packets) < 3 and not stop.is_set():
            if time.monotonic() >= deadline:
                raise AssertionError("host UDP fixture missed required datagrams")
            try:
                payload, peer = listener.recvfrom(2048)
            except socket.timeout:
                continue
            packets.append({"payload_hex": payload.hex(), "peer": peer})
            if payload != PAYLOADS[len(packets) - 1]:
                raise AssertionError(f"unexpected UDP payload {payload!r}")
        result["complete"] = len(packets) == 3
    except BaseException as error:
        result["error"] = repr(error)


def main() -> int:
    if not IMAGE.is_file():
        raise FileNotFoundError(f"AArch64 boot image not found: {IMAGE}")
    processes = subprocess.check_output(["ps", "-axo", "pid=,comm="], text=True)
    running = [line for line in processes.splitlines() if len(line.split(None, 1)) == 2
               and pathlib.Path(line.split(None, 1)[1]).name.startswith("qemu-system-")]
    if running:
        raise RuntimeError("refusing concurrent QEMU: " + "; ".join(running))
    qemu = os.environ.get("QEMU_SYSTEM_AARCH64", "qemu-system-aarch64")
    code = common.first_file("AAVMF_CODE", ("/opt/homebrew/share/qemu/edk2-aarch64-code.fd",
        "/usr/local/share/qemu/edk2-aarch64-code.fd", "/usr/share/AAVMF/AAVMF_CODE.fd"))
    variables_template = common.first_file("AAVMF_VARS", ("/opt/homebrew/share/qemu/edk2-arm-vars.fd",
        "/usr/local/share/qemu/edk2-arm-vars.fd", "/usr/share/AAVMF/AAVMF_VARS.fd"))
    accel = os.environ.get("MAKOS_AARCH64_ACCEL", "hvf"
        if platform.system() == "Darwin" and platform.machine() == "arm64" else "tcg")
    output_root = pathlib.Path(os.environ.get("MAKOS_AARCH64_TEMP_ROOT", ROOT / "build"))
    session = pathlib.Path(tempfile.mkdtemp(prefix="makos-net-owner-", dir=output_root))
    boot, data, variables = session / "boot.img", session / "data.img", session / "vars.fd"
    boot_hash = sha256(IMAGE)
    arm_private_boot(IMAGE, boot)
    private_hash = sha256(boot)
    common.copy_sparse(variables_template, variables)
    with data.open("wb") as output_file:
        output_file.truncate(1024 * 1024 * 1024)
    qmp_parent, qmp_child = socket.socketpair()
    qmp_parent.settimeout(10)
    command = [qemu]
    if qemu_data := os.environ.get("MAKOS_QEMU_DATA_DIR"):
        command.extend(("-L", qemu_data))
    command.extend((
        "-machine", f"virt,accel={accel},highmem=off,gic-version=2",
        "-cpu", "host" if accel == "hvf" else "max",
        "-global", "virtio-mmio.force-legacy=false", "-smp", "4", "-m", "1G",
        "-drive", f"if=pflash,format=raw,readonly=on,file={code}",
        "-drive", f"if=pflash,format=raw,file={variables}",
        "-drive", f"id=boot,if=none,format=raw,readonly=on,file={boot}",
        "-device", "virtio-blk-pci,drive=boot",
        "-drive", f"id=data,if=none,format=raw,file={data}",
        "-device", "virtio-blk-device,drive=data",
        "-device", "virtio-keyboard-device", "-device", "virtio-tablet-device",
        "-netdev", "user,id=makosnet",
        "-device", "virtio-net-device,netdev=makosnet,mac=52:54:00:12:34:56",
        "-device", "virtio-gpu-device,xres=800,yres=600",
        "-object", "rng-random,id=makosrng,filename=/dev/urandom",
        "-device", "virtio-rng-device,rng=makosrng",
        "-display", "none", "-serial", "stdio", "-monitor", "none",
        "-chardev", f"socket,id=makosqmp,fd={qmp_child.fileno()}", "-qmp", "chardev:makosqmp",
        "-no-reboot", "-no-shutdown",
    ))
    record = {"host": platform.platform(), "accelerator": accel,
              "master_boot": str(IMAGE), "boot_sha256_before": boot_hash,
              "private_boot": str(boot), "private_boot_sha256_before": private_hash,
              "private_data": str(data), "private_vars": str(variables),
              "private_boot_change": "MAKOS.CFG only: test.net-owner=required",
              "harness_pid": os.getpid(), "qmp": f"inherited socketpair fd {qmp_child.fileno()}",
              "command": command, "fixture": "UDP 127.0.0.1:18081 receives exact guest packets"}
    (session / "session.json").write_text(json.dumps(record, indent=2) + "\n")
    print(f"MAKOS_AARCH64_NET_OWNER_SESSION path={session} accel={accel}", flush=True)
    listener = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    stop = threading.Event()
    fixture_result = {}
    fixture = None
    process = None
    selector = selectors.DefaultSelector()
    output = bytearray()
    try:
        listener.bind(("127.0.0.1", 18081))
        fixture = threading.Thread(target=receive_packets, args=(listener, stop, fixture_result),
                                   name="makos-net-owner-udp-fixture", daemon=True)
        fixture.start()
        process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                   pass_fds=(qmp_child.fileno(),))
        qmp_child.close()
        record["pid"] = process.pid
        (session / "session.json").write_text(json.dumps(record, indent=2) + "\n")
        assert process.stdout is not None
        selector.register(process.stdout, selectors.EVENT_READ)
        with qmp_parent:
            stream = qmp_parent.makefile("rwb", buffering=0)
            json.loads(stream.readline())
            if "error" in common.qmp_command(stream, "qmp_capabilities"):
                raise AssertionError("QMP capability negotiation failed")
            common.wait_for_output(selector, process, output, RESULT_END, 90)
            validate_output(output.decode(errors="replace"))
            fixture.join(timeout=3)
            if fixture.is_alive() or "error" in fixture_result or not fixture_result.get("complete"):
                raise AssertionError(f"host did not receive exact guest datagrams: {fixture_result}")
            common.qmp_command(stream, "quit")
        process.wait(timeout=10)
        if process.returncode != 0:
            raise AssertionError(f"QEMU exited with status {process.returncode}")
    finally:
        stop.set()
        if fixture is not None:
            fixture.join(timeout=1)
        listener.close()
        if process is not None:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=10)
            if process.stdout is not None:
                output.extend(process.stdout.read())
            record["qemu_exit_status"] = process.returncode
        (session / "serial.log").write_bytes(output)
        selector.close()
        qmp_parent.close()
        qmp_child.close()
        record["boot_sha256_after"] = sha256(IMAGE)
        record["private_boot_sha256_after"] = sha256(boot)
        record["udp_fixture"] = fixture_result
        (session / "session.json").write_text(json.dumps(record, indent=2) + "\n")
    if record["boot_sha256_after"] != boot_hash or record["private_boot_sha256_after"] != private_hash:
        raise AssertionError("master or private boot image changed during network owner regression")
    tids, busy = validate_output(output.decode(errors="replace"))
    print("MAKOS_AARCH64_NET_OWNER_RUNTIME_OK "
          f"accel={accel} tids={','.join(map(str, tids))} requester_cpu=1 service_cpu=0 "
          f"timer_completions=2 lock_wait_completions=1 busy_deferrals={busy} "
          "udp_host_received=3 payloads=exact phases=irq-masked-socket,el1-timer,locked-defer-unlock "
          f"statuses=42,42,42 cleanup=reaped free_balance=1 boot=unchanged session={session} firefox=not-tested")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
