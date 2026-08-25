#!/usr/bin/env python3
"""Prove AP1 stateful TCP TX/RX through CPU0-owned virtio-net service."""

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
import threading
import time

from boot_test_aarch64 import first_file, qmp_command, wait_for_output


ROOT = pathlib.Path(__file__).resolve().parent.parent
IMAGE = pathlib.Path(
    os.environ.get("MAKOS_AARCH64_IMAGE", ROOT / "build/makos-aarch64-smp-tcp.img")
)
TCP_REQUEST = b"MAKOS_AP_TCP_TX\n"
TCP_RESPONSE = b"MAKOS_CPU0_TCP_RX\n"


def serve_fixture(listener: socket.socket, result: dict[str, object]) -> None:
    try:
        listener.settimeout(120)
        connection, address = listener.accept()
        with connection:
            connection.settimeout(20)
            request = bytearray()
            while len(request) < len(TCP_REQUEST):
                chunk = connection.recv(len(TCP_REQUEST) - len(request))
                if not chunk:
                    raise AssertionError("guest closed before the TCP request completed")
                request.extend(chunk)
            if bytes(request) != TCP_REQUEST:
                raise AssertionError(f"unexpected guest request {bytes(request)!r}")
            # Give AP1 a deterministic opportunity to enter its blocking recv
            # syscall before the host makes RX ready. This tests the actual
            # Blocked -> CPU0 pump -> SGI wake path instead of a timing race.
            time.sleep(0.5)
            connection.sendall(TCP_RESPONSE)
            if connection.recv(1) != b"":
                raise AssertionError("guest sent bytes after the expected request")
        result.update(peer=address, request=bytes(request), closed=True)
    except BaseException as error:  # Relay fixture failures to the test thread.
        result["error"] = error
    finally:
        listener.close()


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
    variables_template = first_file(
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
    output_dir = ROOT / "build"
    output_dir.mkdir(parents=True, exist_ok=True)
    serial_log = output_dir / "makos-smp-tcp-focused-serial.log"

    listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    listener.bind(("127.0.0.1", 18080))
    listener.listen(1)
    fixture_result: dict[str, object] = {}
    fixture = threading.Thread(
        target=serve_fixture,
        args=(listener, fixture_result),
        name="makos-smp-tcp-fixture",
        daemon=True,
    )
    fixture.start()

    output = bytearray()
    process: subprocess.Popen[bytes] | None = None
    selector: selectors.BaseSelector | None = None
    try:
        with tempfile.TemporaryDirectory(prefix="makos-smp-tcp-", dir=output_dir) as name:
            temporary = pathlib.Path(name)
            boot = temporary / "boot.img"
            data = temporary / "data.img"
            variables = temporary / "vars.fd"
            shutil.copyfile(IMAGE, boot)
            shutil.copyfile(variables_template, variables)
            with data.open("wb") as output_file:
                output_file.truncate(1024 * 1024 * 1024)

            qmp_parent, qmp_child = socket.socketpair()
            command = [
                qemu,
                "-machine",
                f"virt,accel={accel},highmem=off,gic-version=2",
                "-cpu",
                "host" if accel == "hvf" else "max",
                "-global",
                "virtio-mmio.force-legacy=false",
                "-smp",
                "4",
                "-m",
                "1G",
                "-drive",
                f"if=pflash,format=raw,readonly=on,file={code}",
                "-drive",
                f"if=pflash,format=raw,file={variables}",
                "-drive",
                f"id=boot,if=none,format=raw,readonly=on,file={boot}",
                "-device",
                "virtio-blk-pci,drive=boot",
                "-drive",
                f"id=data,if=none,format=raw,file={data}",
                "-device",
                "virtio-blk-device,drive=data",
                "-device",
                "virtio-keyboard-device",
                "-device",
                "virtio-tablet-device",
                "-netdev",
                "user,id=makosnet",
                "-device",
                "virtio-net-device,netdev=makosnet,mac=52:54:00:12:34:56",
                "-device",
                "virtio-gpu-device,xres=800,yres=600",
                "-object",
                "rng-random,id=makosrng,filename=/dev/urandom",
                "-device",
                "virtio-rng-device,rng=makosrng",
                "-display",
                "none",
                "-serial",
                "stdio",
                "-monitor",
                "none",
                "-chardev",
                f"socket,id=makosqmp,fd={qmp_child.fileno()}",
                "-qmp",
                "chardev:makosqmp",
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
            with qmp_parent:
                stream = qmp_parent.makefile("rwb", buffering=0)
                json.loads(stream.readline())
                if "error" in qmp_command(stream, "qmp_capabilities"):
                    raise AssertionError("QMP capability negotiation failed")
                marker = (
                    b"status=69 owner_status=70 socket_state=locked-publication "
                    b"free_balance=1 scheduler_scope=opt-in-boot-probe desktop_gate=closed"
                )
                wait_for_output(selector, process, output, marker, 90)
                for required in (
                    b"MAKOS_CONFIG_OK source=fat",
                    b"smp_input_probe=0 smp_tcp_probe=1",
                    b"MAKOS_AARCH64_TCP_ASYNC_OK source=timer-rx buffer=32768",
                    b"MAKOS_AARCH64_SMP_TCP_TX_OK requester_cpu=1 service_cpu=0",
                    b"protocol=tcp4 endpoint=10.0.2.2:18080 handshake=syn,synack,ack",
                    b"payload=request,response close=fin ring_activity=real",
                    b"tx_mmio_owner=cpu0 rx_mmio_owner=cpu0 transport=bounded-copied-state",
                    b"MAKOS_AARCH64_SMP_TCP_TX_EVIDENCE owner_completions=4 ap_requests=4",
                    b"connect_completions=1 data_completions=1 ack_completions=1 fin_completions=1",
                    b"owner_frames=1 ap_rx_deferrals=",
                    b"MAKOS_AARCH64_SMP_TCP_WAKE_OK",
                    b"block=ap-idle wake=cpu0-rx-pump,sgi io_idle_mask=0x2 io_resume_mask=0x2",
                    b"status=69 owner_status=70 socket_state=locked-publication free_balance=1",
                    marker,
                ):
                    if required not in output:
                        raise AssertionError(f"missing SMP TCP marker {required!r}")
                if b"MAKOS_FATAL:" in output or b"MAKOS_PANIC:" in output:
                    raise AssertionError("guest reported fatal/panic")
                fixture.join(timeout=5)
                if fixture.is_alive():
                    raise AssertionError("host fixture did not observe the guest FIN")
                if "error" in fixture_result:
                    raise AssertionError(f"host fixture failed: {fixture_result['error']}")
                if fixture_result.get("request") != TCP_REQUEST or not fixture_result.get("closed"):
                    raise AssertionError("host fixture evidence incomplete")
                qmp_command(stream, "quit")
            process.wait(timeout=5)
    finally:
        serial_log.write_bytes(output)
        if selector is not None:
            selector.close()
        if process is not None and process.poll() is None:
            process.terminate()
            process.wait(timeout=5)
        if fixture.is_alive():
            listener.close()
            fixture.join(timeout=20)

    print(
        "MAKOS_AARCH64_SMP_TCP_RUNTIME_OK "
        f"accel={accel} requester_cpu=1 service_cpu=0 "
        "protocol=tcp4 request=exact response=exact close=fin "
        "tx_mmio_owner=cpu0 rx_mmio_owner=cpu0 socket_state=locked-publication"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
