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
    b"MAKOS_AARCH64_LINKER_OK sources=4 languages=aarch64-asm,c-subset-v1 "
    b"compiler=guest-native assembler=guest-native objects=4 "
    b"format=elf64-et-rel linker=guest-native relocations=R_AARCH64_CALL26:3 "
    b"symbols=_start,answer,adjust,combine,helper output=/home/user/generated-aarch64.elf "
    b"build_manifest=argv1 build_driver=makbuild-v1 build_inputs=4 "
    b"cache=makstate-v2 cache_hits=0 cache_misses=4 state_committed=1 "
    b"c_sources=/home/user/generated-program.c,/home/user/generated-library.c,/home/user/generated-helper.c translation_unit_functions=2,1,1 "
    b"c_abi=aapcs64-int32-pointer64 "
    b"c_features=multi-function,multi-parameter,parameter,pointer-parameter,local,array,array-decay,index,assignment,pointer,pointer-add,pointer-variable-add,pointer-difference,address-of,address-expression,dereference,if,equality,inequality,relational,while,call,return "
    b"max_parameters=2 max_call_arguments=2 nonleaf_frame=96 c_operators=mul,sub,add c_relations=eq,ne,lt,le,gt,ge branch_results=42,86 "
    b"loop_results=42,2 memory_results=42,2 pointer_call=answer-to-adjust "
    b"pointee_results=42,44,2 delta_results=1:42,2:44,1:2 array_results=41:42:0,42:0:44,1:2:0 pointer_offset_call=1 pointer_variable_offset=delta dynamic_pointer_adds=2 signed_pointer_offset=-1:42 signed_pointer_difference=3:-3 relational_results=gt:42:0,le:42:0,ge:42:86,lt:42:44 "
    b"code_bytes=76,140,168,60,56 object_bytes=688,976,616,608 intra_object_calls=1 cross_object_calls=2 "
    b"linked_bytes=500 output_bytes=815 helper_result=42 persisted_reopened=1 manifest_input_bounds=2..6 malformed_build_denied=6 malformed_c_denied=17 "
    b"malformed_relocation_denied=1 unresolved_symbol_denied=1 "
    b"duplicate_definition_denied=1"
)
FIXTURE_BUILD_MARKER = (
    b"MAKOS_AARCH64_MAKBUILD_OK mode=fixture "
    b"manifest=/home/user/generated.build startup=sysv argc=2 envc=1 "
    b"seeded=1 cache=makstate-v2 build_inputs=4 cache_hits=0 cache_misses=4 "
    b"state_committed=1 status=42"
)
WARM_BUILD_MARKER = (
    b"MAKOS_AARCH64_MAKBUILD_OK mode=build "
    b"manifest=/home/user/generated.build startup=sysv argc=2 envc=1 "
    b"seeded=0 cache=makstate-v2 build_inputs=4 cache_hits=4 cache_misses=0 "
    b"state_committed=1 status=42"
)
SELECTIVE_BUILD_MARKER = (
    b"MAKOS_AARCH64_MAKBUILD_OK mode=build "
    b"manifest=/home/user/generated.build startup=sysv argc=2 envc=1 "
    b"seeded=0 cache=makstate-v2 build_inputs=4 cache_hits=3 cache_misses=1 "
    b"state_committed=1 status=42"
)
INVALIDATED_BUILD_MARKER = (
    b"MAKOS_AARCH64_MAKBUILD_OK mode=build "
    b"manifest=/home/user/generated.build startup=sysv argc=2 envc=1 "
    b"seeded=0 cache=makstate-v2 build_inputs=4 cache_hits=0 cache_misses=4 "
    b"state_committed=1 status=42"
)
THREE_INPUT_COLD_MARKER = (
    b"MAKOS_AARCH64_MAKBUILD_OK mode=build "
    b"manifest=/home/user/generated-three.build startup=sysv argc=2 envc=1 "
    b"seeded=0 cache=makstate-v2 build_inputs=3 cache_hits=0 cache_misses=3 "
    b"state_committed=1 status=42"
)
THREE_INPUT_WARM_MARKER = (
    b"MAKOS_AARCH64_MAKBUILD_OK mode=build "
    b"manifest=/home/user/generated-three.build startup=sysv argc=2 envc=1 "
    b"seeded=0 cache=makstate-v2 build_inputs=3 cache_hits=3 cache_misses=0 "
    b"state_committed=1 status=42"
)
CLI_REAP_MARKER = (
    b"MAKOS_AARCH64_MAKBUILD_CLI_OK manifest=/home/user/generated.build "
    b"source=existing-makfs seeded=0 startup=sysv status=42"
)
THREE_CLI_REAP_MARKER = (
    b"MAKOS_AARCH64_MAKBUILD_CLI_OK manifest=/home/user/generated-three.build "
    b"source=existing-makfs seeded=0 startup=sysv status=42"
)
EXECUTION_MARKER = (
    b"MAKOS_AARCH64_SELFHOST_LINK_OK source=guest-makfs sources=4 "
    b"languages=aarch64-asm,c-subset-v1 compiler=guest-native "
    b"assembler=guest-native linker=guest-native objects=4 "
    b"object_format=elf64-et-rel relocations=R_AARCH64_CALL26:3 "
    b"symbols=_start,answer,adjust,combine,helper build_manifest=/home/user/generated.build "
    b"build_driver=makbuild-v1 build_inputs=4 cache=makstate-v2 "
    b"cache_hits=0 cache_misses=4 state_committed=1 "
    b"toolchain_startup=sysv manifest_arg=1 "
    b"c_sources=/home/user/generated-program.c,/home/user/generated-library.c,/home/user/generated-helper.c "
    b"translation_unit_functions=2,1,1 c_abi=aapcs64-int32-pointer64 "
    b"c_features=multi-function,multi-parameter,parameter,pointer-parameter,local,array,array-decay,index,assignment,pointer,pointer-add,pointer-variable-add,pointer-difference,address-of,address-expression,dereference,if,equality,inequality,relational,while,call,return "
    b"max_parameters=2 max_call_arguments=2 nonleaf_frame=96 c_operators=mul,sub,add c_relations=eq,ne,lt,le,gt,ge branch_results=42,86 "
    b"loop_results=42,2 memory_results=42,2 pointer_call=answer-to-adjust "
    b"pointee_results=42,44,2 delta_results=1:42,2:44,1:2 array_results=41:42:0,42:0:44,1:2:0 pointer_offset_call=1 pointer_variable_offset=delta dynamic_pointer_adds=2 signed_pointer_offset=-1:42 signed_pointer_difference=3:-3 relational_results=gt:42:0,le:42:0,ge:42:86,lt:42:44 "
    b"code_bytes=76,140,168,60,56 object_bytes=688,976,616,608 intra_object_calls=1 cross_object_calls=2 "
    b"linked_bytes=500 output_bytes=815 helper_result=42 persisted_reopened=1 manifest_input_bounds=2..6 malformed_build_denied=6 malformed_c_denied=17 "
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
                    selector, process, output, FIXTURE_BUILD_MARKER, 60
                )
                common.wait_for_output(
                    selector, process, output, LINKER_MARKER, 60
                )
                common.wait_for_output(
                    selector, process, output, EXECUTION_MARKER, 60
                )
                common.send_command(
                    stream, "makbuild /home/user/generated.build"
                )
                common.wait_for_output(
                    selector, process, output, WARM_BUILD_MARKER, 60
                )
                common.wait_for_output(
                    selector, process, output, CLI_REAP_MARKER, 60
                )
                common.send_command(
                    stream, "write generated-library.o corrupt"
                )
                common.wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_AARCH64_SHELL_CMD write bytes=7 persisted=1",
                    30,
                )
                common.send_command(
                    stream, "makbuild /home/user/generated.build"
                )
                common.wait_for_output(
                    selector, process, output, SELECTIVE_BUILD_MARKER, 60
                )
                common.wait_for_output_count(
                    selector, process, output, CLI_REAP_MARKER, 2, 60
                )
                common.send_command(
                    stream, "makbuild /home/user/generated.build"
                )
                common.wait_for_output_count(
                    selector, process, output, WARM_BUILD_MARKER, 2, 60
                )
                common.wait_for_output_count(
                    selector, process, output, CLI_REAP_MARKER, 3, 60
                )
                common.send_command(
                    stream,
                    "write generated-library.c "
                    "int combine(int value, int delta) { return value + delta; }",
                )
                common.wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_AARCH64_SHELL_CMD write bytes=59 persisted=1",
                    30,
                )
                common.send_command(
                    stream, "makbuild /home/user/generated.build"
                )
                common.wait_for_output_count(
                    selector, process, output, SELECTIVE_BUILD_MARKER, 2, 60
                )
                common.wait_for_output_count(
                    selector, process, output, CLI_REAP_MARKER, 4, 60
                )
                common.send_command(
                    stream, "makbuild /home/user/generated.build"
                )
                common.wait_for_output_count(
                    selector, process, output, WARM_BUILD_MARKER, 3, 60
                )
                common.wait_for_output_count(
                    selector, process, output, CLI_REAP_MARKER, 5, 60
                )
                common.send_command(
                    stream, "write generated.build.state corrupt"
                )
                common.wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_AARCH64_SHELL_CMD write bytes=7 persisted=1",
                    30,
                )
                common.send_command(
                    stream, "makbuild /home/user/generated.build"
                )
                common.wait_for_output(
                    selector, process, output, INVALIDATED_BUILD_MARKER, 60
                )
                common.wait_for_output_count(
                    selector, process, output, CLI_REAP_MARKER, 6, 60
                )
                common.send_command(
                    stream, "makbuild /home/user/generated-three.build"
                )
                common.wait_for_output(
                    selector, process, output, THREE_INPUT_COLD_MARKER, 60
                )
                common.wait_for_output(
                    selector, process, output, THREE_CLI_REAP_MARKER, 60
                )
                common.send_command(
                    stream, "makbuild /home/user/generated-three.build"
                )
                common.wait_for_output(
                    selector, process, output, THREE_INPUT_WARM_MARKER, 60
                )
                common.wait_for_output_count(
                    selector, process, output, THREE_CLI_REAP_MARKER, 2, 60
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
        f"accel={accel} sources=4 languages=aarch64-asm,c-subset-v1 "
        "compiler=guest-native assembler=guest-native objects=4 "
        "format=elf64-et-rel linker=guest-native relocations=R_AARCH64_CALL26:3 "
        "symbols=_start,answer,adjust,combine,helper build_driver=makbuild-v1 build_inputs=4 "
        "toolchain_startup=sysv manifest_arg=1 cli_builds=8 seeded_modes=fixture,existing "
        "cache=makstate-v2 input_bounds=2..6 runtime_graphs=4,3 invalidations=object,source,state "
        "cache_results=cold:0/4,warm:4/0,object:3/1,rewarm:4/0,source:3/1,rewarm:4/0,state:0/4,three-cold:0/3,three-warm:3/0 "
        "translation_unit_functions=2,1,1 "
        "c_abi=aapcs64-int32-pointer64 "
        "c_features=multi-function,multi-parameter,parameter,pointer-parameter,local,array,array-decay,index,assignment,pointer,pointer-add,pointer-variable-add,pointer-difference,address-of,address-expression,dereference,if,equality,inequality,relational,while,call,return "
        "max_parameters=2 max_call_arguments=2 nonleaf_frame=96 c_operators=mul,sub,add c_relations=eq,ne,lt,le,gt,ge branch_results=42,86 "
        "loop_results=42,2 memory_results=42,2 pointer_call=answer-to-adjust "
        "pointee_results=42,44,2 delta_results=1:42,2:44,1:2 array_results=41:42:0,42:0:44,1:2:0 pointer_offset_call=1 pointer_variable_offset=delta dynamic_pointer_adds=2 signed_pointer_offset=-1:42 signed_pointer_difference=3:-3 relational_results=gt:42:0,le:42:0,ge:42:86,lt:42:44 "
        "code_bytes=76,140,168,60,56 object_bytes=688,976,616,608 intra_object_calls=1 cross_object_calls=2 "
        "linked_bytes=500 output_bytes=815 helper_result=42 persisted_reopened=1 manifest_input_bounds=2..6 malformed_build_denied=6 malformed_c_denied=17 "
        "malformed_relocation_denied=1 unresolved_symbol_denied=1 "
        "duplicate_definition_denied=1 executed=2 status=42"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
