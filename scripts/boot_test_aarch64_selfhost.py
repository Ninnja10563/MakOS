#!/usr/bin/env python3
"""Focused runtime proof for MakOS guest-native ET_REL static linking."""

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

import boot_test_aarch64 as common


ROOT = pathlib.Path(__file__).resolve().parent.parent
IMAGE = pathlib.Path(
    os.environ.get("MAKOS_AARCH64_IMAGE", ROOT / "build/makos-aarch64.img")
)
LINKER_MARKER = (
    b"MAKOS_AARCH64_LINKER_OK sources=3 languages=aarch64-asm,c-subset-v1 "
    b"compiler=guest-native assembler=guest-native objects=3 "
    b"format=elf64-et-rel linker=guest-native relocations=R_AARCH64_CALL26:2 "
    b"symbols=_start,answer,adjust output=/home/user/generated-aarch64.elf "
    b"c_source=/home/user/generated-answer.c c_abi=aapcs64-int32 "
    b"c_features=parameter,local,assignment,pointer,address-of,dereference,if,equality,inequality,while,call,return "
    b"nonleaf_frame=96 c_operators=mul,sub,add branch_results=42,86 "
    b"loop_results=42,2 memory_results=42,2 code_bytes=76,120,152 "
    b"object_bytes=688,728,704 linked_bytes=348 output_bytes=815 "
    b"persisted_reopened=1 malformed_c_denied=6 "
    b"malformed_relocation_denied=1 unresolved_symbol_denied=1 "
    b"duplicate_definition_denied=1"
)
EXECUTION_MARKER = (
    b"MAKOS_AARCH64_SELFHOST_LINK_OK source=guest-makfs sources=3 "
    b"languages=aarch64-asm,c-subset-v1 compiler=guest-native "
    b"assembler=guest-native linker=guest-native objects=3 "
    b"object_format=elf64-et-rel relocations=R_AARCH64_CALL26:2 "
    b"symbols=_start,answer,adjust c_source=/home/user/generated-answer.c "
    b"c_abi=aapcs64-int32 "
    b"c_features=parameter,local,assignment,pointer,address-of,dereference,if,equality,inequality,while,call,return "
    b"nonleaf_frame=96 c_operators=mul,sub,add branch_results=42,86 "
    b"loop_results=42,2 memory_results=42,2 code_bytes=76,120,152 "
    b"object_bytes=688,728,704 linked_bytes=348 output_bytes=815 "
    b"persisted_reopened=1 malformed_c_denied=6 "
    b"malformed_relocation_denied=1 unresolved_symbol_denied=1 "
    b"duplicate_definition_denied=1 "
    b"output=elf64-aarch64 kernel_loader=validated abi56=1 abi57=1 "
    b"argv=3 env=1 malformed_startup_denied=3 executed=2 status=42"
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
    serial_log = ROOT / "build/makos-selfhost-focused-serial.log"
    with tempfile.TemporaryDirectory(
        prefix="makos-selfhost-focused-", dir=output_root
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
                common.send_command(stream, "selfhost-aarch64")
                common.wait_for_output(
                    selector, process, output, LINKER_MARKER, 60
                )
                common.wait_for_output(
                    selector, process, output, EXECUTION_MARKER, 60
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
        "MAKOS_AARCH64_SELFHOST_RUNTIME_OK "
        f"accel={accel} sources=3 languages=aarch64-asm,c-subset-v1 "
        "compiler=guest-native assembler=guest-native objects=3 "
        "format=elf64-et-rel linker=guest-native relocations=R_AARCH64_CALL26:2 "
        "symbols=_start,answer,adjust c_abi=aapcs64-int32 "
        "c_features=parameter,local,assignment,pointer,address-of,dereference,if,equality,inequality,while,call,return "
        "nonleaf_frame=96 c_operators=mul,sub,add branch_results=42,86 "
        "loop_results=42,2 memory_results=42,2 code_bytes=76,120,152 "
        "object_bytes=688,728,704 linked_bytes=348 output_bytes=815 "
        "persisted_reopened=1 malformed_c_denied=6 "
        "malformed_relocation_denied=1 unresolved_symbol_denied=1 "
        "duplicate_definition_denied=1 executed=2 status=42"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
