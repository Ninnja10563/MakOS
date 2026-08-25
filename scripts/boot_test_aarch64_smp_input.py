#!/usr/bin/env python3
"""Prove real virtio-net and keyboard IRQs wake blocked AP waiters."""

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

from boot_test_aarch64 import first_file, qmp_command, send_key, wait_for_output


ROOT = pathlib.Path(__file__).resolve().parent.parent
IMAGE = pathlib.Path(
    os.environ.get(
        "MAKOS_AARCH64_IMAGE", ROOT / "build/makos-aarch64-smp-input.img"
    )
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
    serial_log = output_dir / "makos-smp-input-focused-serial.log"
    with tempfile.TemporaryDirectory(prefix="makos-smp-input-", dir=output_dir) as name:
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
        output = bytearray()
        try:
            with qmp_parent:
                stream = qmp_parent.makefile("rwb", buffering=0)
                json.loads(stream.readline())
                if "error" in qmp_command(stream, "qmp_capabilities"):
                    raise AssertionError("QMP capability negotiation failed")

                ready = (
                    b"MAKOS_AARCH64_SMP_INPUT_DEVICE_READY "
                    b"waiter_cpu=1 poller_cpu=0 device=virtio-keyboard "
                    b"chord=ctrl-k input_idle_mask=0x2"
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    ready,
                    90,
                )
                for marker in (
                    b"MAKOS_CONFIG_OK source=fat",
                    b"smp_input_probe=1",
                    b"MAKOS_AARCH64_INPUT_OK transport=virtio-mmio devices=2",
                    b"MAKOS_AARCH64_NETWORK_IRQ_ROUTE_OK intid=76 target_cpu=0 trigger=edge-rising transport=virtio-mmio",
                    b"MAKOS_AARCH64_NETWORK_IRQ_OK intid=76 cpu=0 entry=lower-el dispatch=direct source=virtio-mmio frames=",
                    b"MAKOS_M7_OK graphics_abi=1 surface=96x64 compositor=1 present=1 scanout=0 windows=1 z_order=1 clipping=1 deferred=1",
                    b"MAKOS_AARCH64_SMP_GPU_OK presenter_cpu=1 service_cpu=0",
                    b"device=virtio-gpu request=surface-create,fill,present ring_activity=real",
                    b"mmio_owner=cpu0 contention=ap-deferred service_point=cpu0-timer-bottom-half",
                    b"owner_composes=",
                    b"ap_deferrals=",
                    b"owner_submissions=",
                    b"transfer_completions=",
                    b"flush_completions=",
                    b"status=67 surface_lifecycle=create,fill,present,reap free_balance=1",
                    b"MAKOS_AARCH64_SMP_BLOCK_OK requester_cpu=1 service_cpu=0",
                    b"device=virtio-blk requests=read4k,write4k,fsync ring_activity=real",
                    b"mmio_owner=cpu0 transport=bounded-copy-queue service_point=cpu0-timer-bottom-half",
                    b"owner_completions=",
                    b"ap_requests=",
                    b"read_completions=",
                    b"write_completions=",
                    b"flush_completions=",
                    b"timer_completions=",
                    b"wait=bounded-el1-wfe status=65",
                    b"file=/home/user/.smp-block-io lifecycle=create,write,fsync,reopen,read,verify,remove",
                    b"free_balance=1 scheduler_scope=opt-in-boot-probe desktop_gate=closed",
                    b"MAKOS_AARCH64_SMP_NETWORK_RX_OK waiter_cpu=1 poller_cpu=0",
                    b"device=virtio-net response=dns ring_activity=real",
                    b"rx_mmio_owner=cpu0 contention=ap-deferred owner_frames=",
                    b"irq_frames=",
                    b"delivery=gicv2-spi intid=76 entry=lower-el dispatch=direct timer_fallback=100hz",
                    b"tx_mmio_owner=cpu0 tx_transport=bounded-copy-queue owner_transmits=",
                    b"ap_tx_requests=",
                    b"block=ap-idle wake=cpu0-rx-irq,sgi",
                    b"io_idle_mask=0x2 io_resume_mask=0x2 status=63",
                    b"tcp_ap_tx=cpu0-service-ready runtime=separate-tcp4-probe",
                    ready,
                ):
                    if marker not in output:
                        raise AssertionError(f"missing pre-input marker {marker!r}")

                send_key(stream, "ctrl-k")
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"input=virtio desktop=login\r\n",
                    30,
                )
                required = (
                    b"MAKOS_AARCH64_SMP_INPUT_DEVICE_OK waiter_cpu=1 poller_cpu=0",
                    b"device=virtio-keyboard event=ctrl-k ring_activity=real",
                    b"mmio_owner=cpu0 contention=ap-deferred owner_activity=",
                    b"ap_deferrals=",
                    b"block=ap-idle wake=cpu0-device-poll,sgi",
                    b"input_idle_mask=0x2 input_resume_mask=0x2 status=61 free_balance=1",
                    b"MAKOS_LOGIN_UI_OK framebuffer=800x600 prompt=visible",
                    b"MAKOS_AARCH64_BOOT_OK uefi=1 hvf_ready=1 native_isa=1 ",
                    b"input=virtio desktop=login\r\n",
                )
                for marker in required:
                    if marker not in output:
                        raise AssertionError(f"missing runtime marker {marker!r}")
                if b"MAKOS_FATAL:" in output or b"MAKOS_PANIC:" in output:
                    raise AssertionError("guest reported fatal/panic")
                qmp_command(stream, "quit")
            process.wait(timeout=5)
        finally:
            serial_log.write_bytes(output)
            selector.close()
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=5)

    print(
        "MAKOS_AARCH64_SMP_INPUT_RUNTIME_OK "
        f"accel={accel} waiter_cpu=1 poller_cpu=0 device=virtio-keyboard "
        "event=ctrl-k mmio_owner=cpu0 contention=ap-deferred "
        "gpu=cpu0-owned-transfer-flush,timer-serviced,ap-deferred "
        "block=cpu0-owned-read4k-write4k-fsync,timer-serviced,ap-idle-input "
        "wake=device-ring,sgi "
        "network=cpu0-owned-udp-tx,dns-rx-irq-wake,intid76,direct-lower-el "
        "free_balance=1"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
