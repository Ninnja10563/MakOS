#!/usr/bin/env python3
"""Run genuine dynamic-musl AP threads and cross-page RX syscall resumption.

This fixture is not Firefox, and its timings do not qualify Firefox latency.
Private guest disks and serial/session evidence are retained even on failure.
"""

from __future__ import annotations

import hashlib
import json
import os
import pathlib
import platform
import re
import selectors
import socket
import subprocess
import tempfile

import boot_test_aarch64 as common


ROOT = pathlib.Path(__file__).resolve().parent.parent
IMAGE = pathlib.Path(os.environ.get("MAKOS_AARCH64_IMAGE", ROOT / "build/makos-aarch64.img"))
RESULT_MARKER = b"MAKOS_MUSL_EL0_ENTRY_OK"
REAP_MARKER = (
    b"MAKOS_MUSL_DYNAMIC_REAP_OK status=42 "
    b"lifecycle=spawn,interp,needed-libc,relocate,main,exit,wait,reap"
)


def sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while chunk := source.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def validate_output(decoded: str) -> tuple[tuple[int, int, int], int, int]:
    if "MAKOS_FATAL" in decoded or "EL0 entry rejected" in decoded:
        raise AssertionError("EL0-entry probe reached a kernel fatal/rejection")
    matches = re.findall(
        r"MAKOS_MUSL_EL0_ENTRY_OK loader=musl threads=3 "
        r"singleton=0x2,0x4,0x8 tids=(\d+),(\d+),(\d+) "
        r"pthread_create=(0x[0-9a-f]+) rx=(0x[0-9a-f]+) "
        r"resume=(0x[0-9a-f]+) calls=96 statuses=42,42,42 block=sleep-until",
        decoded,
    )
    if len(matches) != 1:
        raise AssertionError("expected one complete dynamic-musl EL0-entry result")
    first, second, third, loader_text, rx_text, resume_text = matches[0]
    tids = (int(first), int(second), int(third))
    loader = int(loader_text, 16)
    rx = int(rx_text, 16)
    resume = int(resume_text, 16)
    if min(tids) <= 0 or len(set(tids)) != 3:
        raise AssertionError(f"workers did not have distinct live TIDs: {tids}")
    if not 0x28000000 <= loader < 0x30000000:
        raise AssertionError(f"pthread_create did not resolve inside musl loader: {loader:#x}")
    if not 0x80000000 <= rx < 0x3C0000000 - 4096 or rx % 4096 or resume != rx + 4096:
        raise AssertionError(f"RX syscall did not resume on next high mmap page: {rx:#x}, {resume:#x}")
    affinity = {
        (int(tid), int(mask, 16), int(cpu))
        for tid, mask, cpu in re.findall(
            r"MAKOS_AARCH64_THREAD_AFFINITY_OK tid=(\d+) "
            r"operation=get mask=(0x[0-9a-f]+) cpu=(\d+)",
            decoded,
        )
    }
    expected = {(tid, 1 << cpu, cpu) for cpu, tid in enumerate(tids, 1)}
    if not expected.issubset(affinity):
        raise AssertionError(f"kernel did not observe each worker on its AP: {sorted(affinity)}")
    entries = {
        (int(cpu), int(tid), int(root, 16), int(pc, 16))
        for cpu, tid, root, pc in re.findall(
            r"MAKOS_AARCH64_HIGH_EL0_ENTRY_OK cpu=(\d+) tid=(\d+) "
            r"root=(0x[0-9a-f]+) pc=(0x[0-9a-f]+) proof=validated-before-eret",
            decoded,
        )
    }
    roots = set()
    for cpu, tid in enumerate(tids, 1):
        observed = {(root, pc) for entry_cpu, entry_tid, root, pc in entries
                    if entry_cpu == cpu and entry_tid == tid and pc == resume}
        if len(observed) != 1:
            raise AssertionError(f"missing unambiguous outer EL0 resume: cpu={cpu} tid={tid} pc={resume:#x}")
        root, _ = observed.pop()
        if root == 0 or root % 4096:
            raise AssertionError(f"outer EL0 resume had invalid address-space root: {root:#x}")
        roots.add(root)
    if len(roots) != 1:
        raise AssertionError(f"worker outer EL0 resumes used different address-space roots: {sorted(roots)}")
    if REAP_MARKER.decode() not in decoded:
        raise AssertionError("dynamic-musl process was not reaped with status 42")
    return tids, loader, rx


def main() -> int:
    if not IMAGE.is_file():
        raise FileNotFoundError(f"AArch64 boot image not found: {IMAGE}")
    processes = subprocess.check_output(["ps", "-axo", "pid=,comm="], text=True)
    running = [line for line in processes.splitlines()
               if len(line.split(None, 1)) == 2
               and pathlib.Path(line.split(None, 1)[1]).name.startswith("qemu-system-")]
    if running:
        raise RuntimeError("refusing concurrent QEMU: " + "; ".join(running))
    qemu = os.environ.get("QEMU_SYSTEM_AARCH64", "qemu-system-aarch64")
    code = common.first_file("AAVMF_CODE", (
        "/opt/homebrew/share/qemu/edk2-aarch64-code.fd",
        "/usr/local/share/qemu/edk2-aarch64-code.fd",
        "/usr/share/AAVMF/AAVMF_CODE.fd",
    ))
    variables_template = common.first_file("AAVMF_VARS", (
        "/opt/homebrew/share/qemu/edk2-arm-vars.fd",
        "/usr/local/share/qemu/edk2-arm-vars.fd",
        "/usr/share/AAVMF/AAVMF_VARS.fd",
    ))
    accel = os.environ.get("MAKOS_AARCH64_ACCEL", "hvf"
        if platform.system() == "Darwin" and platform.machine() == "arm64" else "tcg")
    output_root = pathlib.Path(os.environ.get("MAKOS_AARCH64_TEMP_ROOT", ROOT / "build"))
    session = pathlib.Path(tempfile.mkdtemp(prefix="makos-el0-entry-", dir=output_root))
    boot, data, variables = session / "boot.img", session / "data.img", session / "vars.fd"
    boot_hash = sha256(IMAGE)
    common.copy_sparse(IMAGE, boot)
    common.copy_sparse(variables_template, variables)
    with data.open("wb") as output_file:
        output_file.truncate(1024 * 1024 * 1024)
    qmp_parent, qmp_child = socket.socketpair()
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
        "-chardev", f"socket,id=makosqmp,fd={qmp_child.fileno()}",
        "-qmp", "chardev:makosqmp", "-no-reboot", "-no-shutdown",
    ))
    record = {"host": platform.platform(), "accelerator": accel,
              "master_boot": str(IMAGE), "boot_sha256_before": boot_hash,
              "private_boot": str(boot), "private_data": str(data),
              "qmp": f"inherited socketpair fd {qmp_child.fileno()}", "command": command}
    (session / "session.json").write_text(json.dumps(record, indent=2) + "\n")
    print(f"MAKOS_AARCH64_EL0_ENTRY_SESSION path={session} accel={accel}", flush=True)
    process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                               pass_fds=(qmp_child.fileno(),))
    qmp_child.close()
    record["pid"] = process.pid
    (session / "session.json").write_text(json.dumps(record, indent=2) + "\n")
    assert process.stdout is not None
    selector = selectors.DefaultSelector()
    selector.register(process.stdout, selectors.EVENT_READ)
    output = bytearray()
    try:
        common.wait_for_output(selector, process, output, b"MAKOS_AARCH64_BOOT_OK", 90)
        with qmp_parent:
            stream = qmp_parent.makefile("rwb", buffering=0)
            json.loads(stream.readline())
            if "error" in common.qmp_command(stream, "qmp_capabilities"):
                raise AssertionError("QMP capability negotiation failed")
            common.click_pointer(stream, 390, 220)
            common.wait_for_output(selector, process, output, b"MAKOS_LOGIN_CLICK_OK", 20)
            for key in ("m", "a", "r", "c", "u", "s", "tab", "m", "a", "k", "o", "s", "ret"):
                common.send_key(stream, key)
            common.wait_for_output(selector, process, output, b"MAKOS_AARCH64_DESKTOP_OK", 90)
            common.wait_for_output(selector, process, output,
                                   b"MAKOS_AARCH64_PRODUCTION_SMP_READY userspace_scheduler_cpus=4", 20)
            common.wait_for_output(selector, process, output, b"MAKOS_AARCH64_FILES_OK", 30)
            common.click_pointer(stream, 250, 580)
            common.wait_for_output(selector, process, output,
                b"MAKOS_TASKBAR_APP_OK surface=2 activate=1 minimize_toggle=1", 20)
            common.send_command(stream, "musl-shared")
            common.wait_for_output(selector, process, output, b"MAKOS_MUSL_DYNAMIC_PROCESS_OK", 30)
            common.wait_for_output(selector, process, output, RESULT_MARKER, 30)
            common.wait_for_output(selector, process, output,
                                   b"MAKOS_MUSL_DYNAMIC_OK loader=musl relocations=executed", 20)
            common.wait_for_output(selector, process, output, REAP_MARKER, 20)
            tids, loader, rx = validate_output(output.decode(errors="replace"))
            common.qmp_command(stream, "quit")
        process.wait(timeout=10)
        if process.returncode != 0:
            raise AssertionError(f"QEMU exited with status {process.returncode}")
    finally:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=10)
        output.extend(process.stdout.read())
        (session / "serial.log").write_bytes(output)
        selector.close()
        qmp_parent.close()
        record["boot_sha256_after"] = sha256(IMAGE)
        record["private_boot_sha256_after"] = sha256(boot)
        record["qemu_exit_status"] = process.returncode
        (session / "session.json").write_text(json.dumps(record, indent=2) + "\n")
    if record["boot_sha256_after"] != boot_hash or record["private_boot_sha256_after"] != boot_hash:
        raise AssertionError("boot image changed during EL0-entry regression")
    # Also reject any fatal emitted between the success marker and QEMU exit.
    tids, loader, rx = validate_output(output.decode(errors="replace"))
    print("MAKOS_AARCH64_EL0_ENTRY_RUNTIME_OK "
          f"accel={accel} fixture=dynamic-musl-pthread threads=3 "
          f"tids={tids[0]},{tids[1]},{tids[2]} singleton=0x2,0x4,0x8 "
          f"pthread_create={loader:#x} rx={rx:#x} "
          "resume=next-page calls=96 statuses=42,42,42 block=sleep-until "
          "outer_entry=validated-before-eret roots=shared boot=unchanged "
          f"session={session} firefox=not-tested")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
