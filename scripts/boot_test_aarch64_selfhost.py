#!/usr/bin/env python3
"""Focused runtime proof for MakOS guest-native ET_REL static linking."""

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


def fnv1a(data: bytes) -> int:
    value = 14695981039346656037
    for byte in data:
        value = ((value ^ byte) * 1099511628211) & ((1 << 64) - 1)
    return value


REPOSITORY_C_SOURCE = (ROOT / "user/aarch64_selfhost_probe.c").read_bytes()
REPOSITORY_ASM_SOURCE = (ROOT / "user/aarch64_selfhost_probe.S").read_bytes()
REPOSITORY_SOURCE_MARKER = (
    "MAKOS_AARCH64_REPOSITORY_SOURCE_OK "
    "c=user/aarch64_selfhost_probe.c asm=user/aarch64_selfhost_probe.S "
    f"c_bytes={len(REPOSITORY_C_SOURCE)} "
    f"asm_bytes={len(REPOSITORY_ASM_SOURCE)} "
    f"c_fnv1a={fnv1a(REPOSITORY_C_SOURCE):016x} "
    f"asm_fnv1a={fnv1a(REPOSITORY_ASM_SOURCE):016x} "
    "identity=build-generated-exact host_reference=compiled"
).encode()
LINKER_MARKER = (
    b"MAKOS_AARCH64_LINKER_OK sources=4 languages=aarch64-asm,c-subset-v1 "
    b"compiler=guest-native assembler=guest-native objects=4 "
    b"format=elf64-et-rel linker=guest-native relocations=R_AARCH64_CALL26:3 "
    b"symbols=_start,answer,adjust,combine,helper output=/home/user/generated-aarch64.elf "
    b"build_manifest=argv1 build_driver=makbuild-v1 build_inputs=4 "
    b"cache=makstate-v2 cache_hits=0 cache_misses=4 state_committed=1 "
    b"c_sources=/home/user/generated-program.c,/home/user/generated-library.c,/home/user/generated-helper.c translation_unit_functions=2,1,1 "
    b"c_abi=aapcs64-int32-pointer64 "
    b"c_features=multi-function,multi-parameter,six-argument,signed-arithmetic,parameter,pointer-parameter,local,array,array-decay,index,assignment,pointer,pointer-add,pointer-variable-add,pointer-difference,address-of,address-expression,dereference,if,if-assignment,if-else,nested-control,equality,inequality,relational,while,call,return "
    b"max_parameters=6 max_call_arguments=6 nonleaf_frame=96,112 three_argument_result=42 three_argument_link=et-rel,same-object six_argument_result=42 six_argument_link=et-rel,same-object c_operators=mul,sdiv,srem,neg,sub,add signed_division_results=20:6,-20:-6 signed_remainder_results=20:2,-20:-2 unary_negation_results=42:-42,-42:42 arithmetic_object=elf64-et-rel:784 c_relations=eq,ne,lt,le,gt,ge branch_results=42,86 "
    b"loop_results=42,2 memory_results=42,2 pointer_call=answer-to-adjust "
    b"pointee_results=42,44,2 delta_results=1:42,2:44,1:2 array_results=41:42:0,42:0:44,1:2:0 pointer_offset_call=1 pointer_variable_offset=delta dynamic_pointer_adds=2 signed_pointer_offset=-1:42 signed_pointer_difference=3:-3 relational_results=gt:42:0,le:42:0,ge:42:86,lt:42:44 "
    b"code_bytes=76,140,168,60,56 object_bytes=688,976,616,608 intra_object_calls=1 cross_object_calls=2 "
    b"linked_bytes=500 output_bytes=1583 helper_result=42 persisted_reopened=1 manifest_input_bounds=2..6 malformed_build_denied=6 malformed_c_denied=21 "
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
HEADER_COLD_MARKER = (
    b"MAKOS_AARCH64_MAKBUILD_OK mode=build "
    b"manifest=/home/user/generated-header.build startup=sysv argc=2 envc=1 "
    b"seeded=0 cache=makstate-v2 build_inputs=2 cache_hits=0 cache_misses=2 "
    b"state_committed=1 status=42"
)
HEADER_WARM_MARKER = (
    b"MAKOS_AARCH64_MAKBUILD_OK mode=build "
    b"manifest=/home/user/generated-header.build startup=sysv argc=2 envc=1 "
    b"seeded=0 cache=makstate-v2 build_inputs=2 cache_hits=2 cache_misses=0 "
    b"state_committed=1 status=42"
)
HEADER_SELECTIVE_MARKER = (
    b"MAKOS_AARCH64_MAKBUILD_OK mode=build "
    b"manifest=/home/user/generated-header.build startup=sysv argc=2 envc=1 "
    b"seeded=0 cache=makstate-v2 build_inputs=2 cache_hits=1 cache_misses=1 "
    b"state_committed=1 status=42"
)
HEADER_DEP_MARKER = (
    b"MAKOS_AARCH64_C_HEADER_DEP_OK source=/home/user/generated-header.c "
    b"root=/home/user/generated-inline.h leaf=/home/user/generated-leaf.h "
    b"headers=2 max_depth=2 resolver=quoted-absolute-recursive depth_limit=4 "
    b"preprocessor=bounded-macro-if-expressions macros=6 conditional_depth=2 "
    b"macro_expansion=text,function-like parameters=4 expansion_depth=8 "
    b"if_expression=defined,numeric,arithmetic,shift,comparison,bitwise,not,"
    b"and,or,short-circuit,conditional elif=selected "
    b"include_guard=deduplicated fingerprint=expanded-source"
)
HEADER_GUARD_MARKER = (
    b"MAKOS_AARCH64_C_PREPROCESSOR_GUARD_OK headers=2 max_depth=2 macros=6 "
    b"conditional_depth=2 include_guard=deduplicated missing=denied "
    b"relative=denied cycle=denied overdepth=denied "
    b"macro_expansion=text,function-like parameters=4 expansion_depth=8 "
    b"if_expression=defined,numeric,arithmetic,shift,comparison,bitwise,not,"
    b"and,or,short-circuit,conditional elif=selected "
    b"malformed=define,endif,unterminated,duplicate-else,expression,"
    b"elif-after-else,zero-divisor,shift-range,overflow,conditional-syntax,"
    b"conditional-selected-trap,macro-parameters,macro-arity,"
    b"macro-recursion,macro-token-op-denied depth_limit=4"
)
HEADER_CLI_REAP_MARKER = (
    b"MAKOS_AARCH64_MAKBUILD_CLI_OK manifest=/home/user/generated-header.build "
    b"source=existing-makfs seeded=0 startup=sysv status=42"
)
HEADER_RUN_MARKER = (
    b"MAKOS_AARCH64_RUN_OK path=/home/user/generated-header.elf status=42 "
    b"lifecycle=spawn,run,exit,wait,reap"
)
REPOSITORY_COLD_MARKER = (
    b"MAKOS_AARCH64_MAKBUILD_OK mode=build "
    b"manifest=/home/user/makos-repo-probe.build startup=sysv argc=2 envc=1 "
    b"seeded=0 cache=makstate-v2 build_inputs=2 cache_hits=0 cache_misses=2 "
    b"state_committed=1 status=42"
)
REPOSITORY_WARM_MARKER = (
    b"MAKOS_AARCH64_MAKBUILD_OK mode=build "
    b"manifest=/home/user/makos-repo-probe.build startup=sysv argc=2 envc=1 "
    b"seeded=0 cache=makstate-v2 build_inputs=2 cache_hits=2 cache_misses=0 "
    b"state_committed=1 status=42"
)
REPOSITORY_CLI_REAP_MARKER = (
    b"MAKOS_AARCH64_MAKBUILD_CLI_OK "
    b"manifest=/home/user/makos-repo-probe.build source=existing-makfs "
    b"seeded=0 startup=sysv status=42"
)
REPOSITORY_RUN_MARKER = (
    b"MAKOS_AARCH64_RUN_OK path=/home/user/makos-repo-probe.elf status=42 "
    b"lifecycle=spawn,run,exit,wait,reap"
)
NESTED_COLD_MARKER = (
    b"MAKOS_AARCH64_MAKBUILD_OK mode=build "
    b"manifest=/home/user/generated-nested.build startup=sysv argc=2 envc=1 "
    b"seeded=0 cache=makstate-v2 build_inputs=3 cache_hits=0 cache_misses=3 "
    b"state_committed=1 status=42"
)
NESTED_WARM_MARKER = (
    b"MAKOS_AARCH64_MAKBUILD_OK mode=build "
    b"manifest=/home/user/generated-nested.build startup=sysv argc=2 envc=1 "
    b"seeded=0 cache=makstate-v2 build_inputs=3 cache_hits=3 cache_misses=0 "
    b"state_committed=1 status=42"
)
NESTED_OUTPUT_PREFIX = (
    b"MAKOS_AARCH64_MAKBUILD_OUTPUT_OK "
    b"manifest=/home/user/generated-nested.build linked_bytes="
)
NESTED_CLI_REAP_MARKER = (
    b"MAKOS_AARCH64_MAKBUILD_CLI_OK "
    b"manifest=/home/user/generated-nested.build source=existing-makfs "
    b"seeded=0 startup=sysv status=42"
)
NESTED_RUN_MARKER = (
    b"MAKOS_AARCH64_RUN_OK path=/home/user/generated-nested.elf status=42 "
    b"lifecycle=spawn,run,exit,wait,reap"
)
TOOLCHAIN_SMP_MARKER = b"MAKOS_AARCH64_TOOLCHAIN_SMP_OK "
TOOLCHAIN_PROCESS_COUNT = 17
SIX_FUNCTION_MARKER = (
    b"MAKOS_AARCH64_C_SIX_FUNCTION_OK functions=6 calls=5 "
    b"relocations=R_AARCH64_CALL26:5 object=elf64-et-rel linked=1 "
    b"result=42 max_functions=6 overflow=7-denied"
)
SIX_ARGUMENT_MARKER = (
    b"MAKOS_AARCH64_C_SIX_ARGUMENT_OK parameters=6 call_arguments=6 "
    b"registers=x0-x5 callee_saved=x23-x28 frame=112 "
    b"object=elf64-et-rel:808 relocation=R_AARCH64_CALL26 "
    b"direct_result=42 same_object_call_result=42 overflow=7-denied"
)
BRANCH_BLOCK_MARKER = (
    b"MAKOS_AARCH64_C_BRANCH_BLOCK_OK forms=if,if-else,nested-if,nested-loop "
    b"body=bounded-control-assignment continuation=return max_depth=4 "
    b"object=elf64-et-rel symbols=choose,bump,nested,accumulate linked=1 wx=denied "
    b"results=42,2,5,8,42,2,1,6 "
    b"malformed=empty-else,branch-declaration-denied,depth-5-denied"
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
    b"preprocessor=bounded-macro-if-expressions "
    b"macro_expansion=text,function-like parameters=4 expansion_depth=8 "
    b"if_expression=defined,numeric,arithmetic,shift,comparison,bitwise,not,"
    b"and,or,short-circuit,conditional elif=selected "
    b"c_sources=/home/user/generated-program.c,/home/user/generated-library.c,/home/user/generated-helper.c "
    b"translation_unit_functions=2,1,1 c_abi=aapcs64-int32-pointer64 "
    b"c_features=multi-function,multi-parameter,six-argument,signed-arithmetic,parameter,pointer-parameter,local,array,array-decay,index,assignment,pointer,pointer-add,pointer-variable-add,pointer-difference,address-of,address-expression,dereference,if,if-assignment,if-else,nested-control,equality,inequality,relational,while,call,return "
    b"max_parameters=6 max_call_arguments=6 nonleaf_frame=96,112 three_argument_result=42 three_argument_link=et-rel,same-object six_argument_result=42 six_argument_link=et-rel,same-object c_operators=mul,sdiv,srem,neg,sub,add signed_division_results=20:6,-20:-6 signed_remainder_results=20:2,-20:-2 unary_negation_results=42:-42,-42:42 arithmetic_object=elf64-et-rel:784 c_relations=eq,ne,lt,le,gt,ge branch_results=42,86 "
    b"loop_results=42,2 memory_results=42,2 pointer_call=answer-to-adjust "
    b"pointee_results=42,44,2 delta_results=1:42,2:44,1:2 array_results=41:42:0,42:0:44,1:2:0 pointer_offset_call=1 pointer_variable_offset=delta dynamic_pointer_adds=2 signed_pointer_offset=-1:42 signed_pointer_difference=3:-3 relational_results=gt:42:0,le:42:0,ge:42:86,lt:42:44 "
    b"code_bytes=76,140,168,60,56 object_bytes=688,976,616,608 intra_object_calls=1 cross_object_calls=2 "
    b"linked_bytes=500 output_bytes=1583 helper_result=42 persisted_reopened=1 manifest_input_bounds=2..6 malformed_build_denied=6 malformed_c_denied=21 "
    b"malformed_relocation_denied=1 unresolved_symbol_denied=1 "
    b"duplicate_definition_denied=1 "
    b"output=elf64-aarch64 kernel_loader=validated abi56=1 abi57=1 "
    b"argv=3 env=1 malformed_startup_denied=3 executed=2 status=42"
)


def validate_toolchain_smp(
    output: bytes,
) -> tuple[list[int], list[int], int, int, int, int, int]:
    placements = re.findall(
        rb"MAKOS_AARCH64_TOOLCHAIN_PLACEMENT_OK pid=(\d+) cpu=([1-3]) "
        rb"affinity=0x([0-9a-f]+) loads=(\d+),(\d+),(\d+) "
        rb"idle_mask=0x([0-9a-f]+) policy=least-dispatched-idle-ap "
        rb"caller_selected=0 device_mmio_owner=cpu0",
        output,
    )
    if len(placements) != TOOLCHAIN_PROCESS_COUNT:
        raise AssertionError(
            f"expected {TOOLCHAIN_PROCESS_COUNT} toolchain placements, "
            f"observed {len(placements)}"
        )
    for _, cpu_bytes, affinity_bytes, *load_and_idle in placements:
        cpu = int(cpu_bytes)
        affinity = int(affinity_bytes, 16)
        loads = [int(value) for value in load_and_idle[:3]]
        idle_mask = int(load_and_idle[3], 16)
        candidates = [
            candidate
            for candidate in range(1, 4)
            if idle_mask == 0 or idle_mask & (1 << candidate)
        ]
        if affinity != 1 << cpu or cpu not in candidates:
            raise AssertionError(
                f"invalid toolchain placement cpu={cpu} affinity={affinity:#x} "
                f"idle_mask={idle_mask:#x}"
            )
        if loads[cpu - 1] != min(loads[candidate - 1] for candidate in candidates):
            raise AssertionError(
                f"toolchain placement was not least-loaded: cpu={cpu} "
                f"loads={loads} idle_mask={idle_mask:#x}"
            )

    migrations = re.findall(
        rb"MAKOS_AARCH64_TOOLCHAIN_MIGRATION_OK pid=(\d+) source_cpu=([1-3]) "
        rb"target_cpu=([1-3]) old_affinity=0x([0-9a-f]+) "
        rb"new_affinity=0x([0-9a-f]+) loads=(\d+),(\d+),(\d+) "
        rb"idle_mask=0x([0-9a-f]+) policy=timer-safe-dispatch-imbalance "
        rb"delta=(\d+) context=gpr,sp,tls,simd ownership=ready-unowned "
        rb"evidence_emitter=cpu0 device_mmio_owner=cpu0 caller_selected=0",
        output,
    )
    if not migrations:
        raise AssertionError("no automatic Toolchain migration was observed")
    for migration in migrations:
        source_cpu = int(migration[1])
        target_cpu = int(migration[2])
        old_affinity = int(migration[3], 16)
        new_affinity = int(migration[4], 16)
        loads = [int(value) for value in migration[5:8]]
        idle_mask = int(migration[8], 16)
        delta = int(migration[9])
        if (
            source_cpu == target_cpu
            or old_affinity != 1 << source_cpu
            or new_affinity != 1 << target_cpu
            or idle_mask & (1 << target_cpu) == 0
            or loads[source_cpu - 1] < loads[target_cpu - 1] + delta
            or delta != 8
        ):
            raise AssertionError(
                "invalid automatic Toolchain migration: "
                f"source={source_cpu} target={target_cpu} loads={loads} "
                f"idle_mask={idle_mask:#x} delta={delta}"
            )

    summaries = re.findall(
        rb"MAKOS_AARCH64_TOOLCHAIN_SMP_OK cpu_mask=0x([0-9a-f]+) "
        rb"placements=(\d+),(\d+),(\d+) dispatches=(\d+),(\d+),(\d+) "
        rb"migrations=(\d+) migration_source_mask=0x([0-9a-f]+) "
        rb"migration_target_mask=0x([0-9a-f]+) "
        rb"migration_policy=timer-safe-dispatch-imbalance migration_delta=(\d+) "
        rb"migration_evidence_drops=(\d+) "
        rb"leader=ap kernel_placement=least-dispatched-idle caller_selected=0 "
        rb"ownership=exclusive device_mmio_owner=cpu0 "
        rb"console_gpu_handoff=ap-defer,cpu0-compose owner_composes=(\d+) "
        rb"ap_deferrals=(\d+) pending=0 status=42",
        output,
    )
    if len(summaries) != TOOLCHAIN_PROCESS_COUNT:
        raise AssertionError(
            f"expected {TOOLCHAIN_PROCESS_COUNT} toolchain SMP summaries, "
            f"observed {len(summaries)}"
        )
    final = summaries[-1]
    cpu_mask = int(final[0], 16)
    final_placements = [int(value) for value in final[1:4]]
    final_dispatches = [int(value) for value in final[4:7]]
    migration_count = int(final[7])
    migration_source_mask = int(final[8], 16)
    migration_target_mask = int(final[9], 16)
    migration_delta = int(final[10])
    migration_evidence_drops = int(final[11])
    owner_composes = int(final[12])
    ap_deferrals = int(final[13])
    if (
        cpu_mask != 0xE
        or sum(final_placements) != TOOLCHAIN_PROCESS_COUNT
        or min(final_placements) == 0
        or min(final_dispatches) == 0
        or migration_count != len(migrations)
        or migration_count == 0
        or migration_source_mask == 0
        or migration_source_mask & ~0xE != 0
        or migration_target_mask == 0
        or migration_target_mask & ~0xE != 0
        or migration_delta != 8
        or migration_evidence_drops != 0
        or owner_composes == 0
        or ap_deferrals == 0
    ):
        raise AssertionError(
            f"incomplete toolchain SMP coverage: mask={cpu_mask:#x} "
            f"placements={final_placements} dispatches={final_dispatches}"
        )
    dispatched_cpus = {
        int(cpu)
        for cpu in re.findall(
            rb"MAKOS_AARCH64_TOOLCHAIN_DISPATCH_OK pid=\d+ cpu=([1-3]) ", output
        )
    }
    if dispatched_cpus != {1, 2, 3}:
        raise AssertionError(f"missing toolchain AP dispatch markers: {dispatched_cpus}")
    return (
        final_placements,
        final_dispatches,
        migration_count,
        migration_source_mask,
        migration_target_mask,
        owner_composes,
        ap_deferrals,
    )


def validate_nested_build_output(output: bytes) -> int:
    records = re.findall(
        rb"MAKOS_AARCH64_MAKBUILD_OUTPUT_OK "
        rb"manifest=/home/user/generated-nested\.build linked_bytes=(\d+) "
        rb"output_bytes=(\d+) linked_capacity=(\d+) image_capacity=(\d+) "
        rb"data_offset=(\d+)",
        output,
    )
    if len(records) != 2:
        raise AssertionError(
            f"expected two nested-build output records, observed {len(records)}"
        )
    values = [tuple(int(field) for field in record) for record in records]
    if values[0] != values[1]:
        raise AssertionError(f"cold/warm nested-build output changed: {values}")
    linked_bytes, output_bytes, linked_capacity, image_capacity, data_offset = (
        values[0]
    )
    if (
        linked_bytes <= 512
        or linked_bytes > linked_capacity
        or linked_capacity != 1024
        or output_bytes != 1583
        or image_capacity != 2048
        or data_offset != 1536
    ):
        raise AssertionError(f"invalid nested-build output bounds: {values[0]}")
    return linked_bytes


def validate_gpu_recovery(output: bytes) -> int:
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
            f"unpaired delayed GPU completions: delayed={delayed} recovered={recovered}"
        )
    return len(recovered)


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
    toolchain_placements: list[int] = []
    toolchain_dispatches: list[int] = []
    toolchain_owner_composes = 0
    toolchain_ap_deferrals = 0
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
                    selector, process, output, REPOSITORY_SOURCE_MARKER, 60
                )
                common.wait_for_output(
                    selector, process, output, HEADER_GUARD_MARKER, 60
                )
                common.wait_for_output(
                    selector, process, output, SIX_FUNCTION_MARKER, 60
                )
                common.wait_for_output(
                    selector, process, output, SIX_ARGUMENT_MARKER, 60
                )
                common.wait_for_output(
                    selector, process, output, BRANCH_BLOCK_MARKER, 60
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
                common.send_command(
                    stream, "makbuild /home/user/generated-header.build"
                )
                common.wait_for_output(
                    selector, process, output, HEADER_COLD_MARKER, 60
                )
                common.wait_for_output(
                    selector, process, output, HEADER_DEP_MARKER, 60
                )
                common.wait_for_output(
                    selector, process, output, HEADER_CLI_REAP_MARKER, 60
                )
                common.send_command(
                    stream, "makbuild /home/user/generated-header.build"
                )
                common.wait_for_output(
                    selector, process, output, HEADER_WARM_MARKER, 60
                )
                common.wait_for_output_count(
                    selector, process, output, HEADER_CLI_REAP_MARKER, 2, 60
                )
                common.send_command(
                    stream,
                    "write generated-leaf.h "
                    "int included_answer(int value) { return value +  2; }",
                )
                common.wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_AARCH64_SHELL_CMD write bytes=53 persisted=1",
                    30,
                )
                common.send_command(
                    stream, "makbuild /home/user/generated-header.build"
                )
                common.wait_for_output(
                    selector, process, output, HEADER_SELECTIVE_MARKER, 60
                )
                common.wait_for_output_count(
                    selector, process, output, HEADER_CLI_REAP_MARKER, 3, 60
                )
                common.send_command(stream, "run generated-header.elf")
                common.wait_for_output(
                    selector, process, output, HEADER_RUN_MARKER, 60
                )
                common.send_command(
                    stream, "makbuild /home/user/generated-header.build"
                )
                common.wait_for_output_count(
                    selector, process, output, HEADER_WARM_MARKER, 2, 60
                )
                common.wait_for_output_count(
                    selector, process, output, HEADER_CLI_REAP_MARKER, 4, 60
                )
                common.send_command(
                    stream, "makbuild /home/user/makos-repo-probe.build"
                )
                common.wait_for_output(
                    selector, process, output, REPOSITORY_COLD_MARKER, 60
                )
                common.wait_for_output(
                    selector, process, output, REPOSITORY_CLI_REAP_MARKER, 60
                )
                common.send_command(
                    stream, "makbuild /home/user/makos-repo-probe.build"
                )
                common.wait_for_output(
                    selector, process, output, REPOSITORY_WARM_MARKER, 60
                )
                common.wait_for_output_count(
                    selector, process, output, REPOSITORY_CLI_REAP_MARKER, 2, 60
                )
                common.send_command(stream, "run makos-repo-probe.elf")
                common.wait_for_output(
                    selector, process, output, REPOSITORY_RUN_MARKER, 60
                )
                common.send_command(
                    stream, "makbuild /home/user/generated-nested.build"
                )
                common.wait_for_output(
                    selector, process, output, NESTED_COLD_MARKER, 60
                )
                common.wait_for_output(
                    selector, process, output, NESTED_OUTPUT_PREFIX, 60
                )
                common.wait_for_output(
                    selector, process, output, NESTED_CLI_REAP_MARKER, 60
                )
                common.send_command(
                    stream, "makbuild /home/user/generated-nested.build"
                )
                common.wait_for_output(
                    selector, process, output, NESTED_WARM_MARKER, 60
                )
                common.wait_for_output_count(
                    selector, process, output, NESTED_OUTPUT_PREFIX, 2, 60
                )
                common.wait_for_output_count(
                    selector, process, output, NESTED_CLI_REAP_MARKER, 2, 60
                )
                common.send_command(stream, "run generated-nested.elf")
                common.wait_for_output(
                    selector, process, output, NESTED_RUN_MARKER, 60
                )
                common.wait_for_output_count(
                    selector,
                    process,
                    output,
                    TOOLCHAIN_SMP_MARKER,
                    TOOLCHAIN_PROCESS_COUNT,
                    60,
                )
                (
                    toolchain_placements,
                    toolchain_dispatches,
                    toolchain_migrations,
                    toolchain_migration_source_mask,
                    toolchain_migration_target_mask,
                    toolchain_owner_composes,
                    toolchain_ap_deferrals,
                ) = validate_toolchain_smp(bytes(output))
                nested_linked_bytes = validate_nested_build_output(bytes(output))
                gpu_delayed_recoveries = validate_gpu_recovery(bytes(output))
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
        "toolchain_startup=sysv manifest_arg=1 cli_builds=16 seeded_modes=fixture,existing "
        f"toolchain_smp=kernel-least-loaded-ap cpu_mask=0xe placements={toolchain_placements[0]},{toolchain_placements[1]},{toolchain_placements[2]} dispatches={toolchain_dispatches[0]},{toolchain_dispatches[1]},{toolchain_dispatches[2]} processes={TOOLCHAIN_PROCESS_COUNT} migrations={toolchain_migrations} migration_source_mask={toolchain_migration_source_mask:#x} migration_target_mask={toolchain_migration_target_mask:#x} migration_policy=timer-safe-dispatch-imbalance migration_delta=8 migration_evidence_drops=0 caller_selected=0 ownership=exclusive device_mmio_owner=cpu0 console_gpu_handoff=ap-defer,cpu0-compose owner_composes={toolchain_owner_composes} ap_deferrals={toolchain_ap_deferrals} pending=0 "
        "cache=makstate-v2 input_bounds=2..6 runtime_graphs=4,3,2,2,3 invalidations=object,source,state,header "
        "cache_results=cold:0/4,warm:4/0,object:3/1,rewarm:4/0,source:3/1,rewarm:4/0,state:0/4,three-cold:0/3,three-warm:3/0,header-cold:0/2,header-warm:2/0,header-edit:1/1,header-rewarm:2/0,repository-cold:0/2,repository-warm:2/0,nested-cold:0/3,nested-warm:3/0 "
        f"nested_build=authenticated-makfs linked_bytes={nested_linked_bytes} linked_capacity=1024 output_bytes=1583 image_capacity=2048 data_offset=1536 control=while-to-if-else execution=42 "
        f"gpu_completion=fast-plus-bounded-recovery delayed_recoveries={gpu_delayed_recoveries} timeouts=0 errors=0 "
        f"repository_source=user/aarch64_selfhost_probe.c,user/aarch64_selfhost_probe.S c_bytes={len(REPOSITORY_C_SOURCE)} asm_bytes={len(REPOSITORY_ASM_SOURCE)} c_fnv1a={fnv1a(REPOSITORY_C_SOURCE):016x} asm_fnv1a={fnv1a(REPOSITORY_ASM_SOURCE):016x} identity=build-generated-exact host_reference=compiled guest_execution=42 "
        "header_dependency=quoted-absolute-recursive headers=2 max_depth=2 depth_limit=4 preprocessor=bounded-macro-if-expressions macros=6 conditional_depth=2 macro_expansion=text,function-like parameters=4 expansion_depth=8 if_expression=defined,numeric,arithmetic,shift,comparison,bitwise,not,and,or,short-circuit,conditional elif=selected include_guard=deduplicated fingerprint=expanded-source malformed_headers=missing,relative,cycle,overdepth-denied malformed_preprocessor=define,endif,unterminated,duplicate-else,expression,elif-after-else,zero-divisor,shift-range,overflow,conditional-syntax,conditional-selected-trap,macro-parameters,macro-arity,macro-recursion,macro-token-op-denied transitive_header_execution=42 "
        "translation_unit_functions=2,1,1 "
        "max_functions_per_unit=6 six_function_calls=5 six_function_result=42 "
        "c_abi=aapcs64-int32-pointer64 "
        "branch_blocks=if,if-else,nested-if,nested-loop branch_block_body=bounded-control-assignment branch_block_max_depth=4 "
        "branch_block_results=42,2,5,8,42,2,1,6 branch_block_object=elf64-et-rel "
        "malformed_branch_blocks=empty-else,branch-declaration-denied,depth-5-denied "
        "c_features=multi-function,multi-parameter,six-argument,signed-arithmetic,parameter,pointer-parameter,local,array,array-decay,index,assignment,pointer,pointer-add,pointer-variable-add,pointer-difference,address-of,address-expression,dereference,if,if-assignment,if-else,nested-control,equality,inequality,relational,while,call,return "
        "max_parameters=6 max_call_arguments=6 nonleaf_frame=96,112 three_argument_result=42 three_argument_link=et-rel,same-object six_argument_result=42 six_argument_link=et-rel,same-object six_argument_object=elf64-et-rel:808 c_operators=mul,sdiv,srem,neg,sub,add signed_division_results=20:6,-20:-6 signed_remainder_results=20:2,-20:-2 unary_negation_results=42:-42,-42:42 arithmetic_object=elf64-et-rel:784 c_relations=eq,ne,lt,le,gt,ge branch_results=42,86 "
        "loop_results=42,2 memory_results=42,2 pointer_call=answer-to-adjust "
        "pointee_results=42,44,2 delta_results=1:42,2:44,1:2 array_results=41:42:0,42:0:44,1:2:0 pointer_offset_call=1 pointer_variable_offset=delta dynamic_pointer_adds=2 signed_pointer_offset=-1:42 signed_pointer_difference=3:-3 relational_results=gt:42:0,le:42:0,ge:42:86,lt:42:44 "
        "code_bytes=76,140,168,60,56 object_bytes=688,976,616,608 intra_object_calls=1 cross_object_calls=2 "
        "linked_bytes=500 output_bytes=1583 helper_result=42 persisted_reopened=1 manifest_input_bounds=2..6 malformed_build_denied=6 malformed_c_denied=21 "
        "malformed_relocation_denied=1 unresolved_symbol_denied=1 "
        "duplicate_definition_denied=1 executed=2 status=42"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
