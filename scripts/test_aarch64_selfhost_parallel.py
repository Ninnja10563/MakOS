#!/usr/bin/env python3
"""Unit checks for parallel guest-makbuild serial evidence validation."""

from __future__ import annotations

import boot_test_aarch64_selfhost as runtime


PIDS = [201, 202, 203]
ROOTS = [0x41000, 0x52000, 0x63000]


def process_record(pid: int, root: int) -> bytes:
    return (
        f"MAKOS_AARCH64_TOOLCHAIN_PROCESS_OK pid={pid} parent=17 "
        f"elf=1 el=0 entry=0x400000 ttbr0={root:#x} "
        "source=guest-file startup=sysv argc=2 mode=build "
        "scheduler=kernel-least-loaded-ap device_mmio_owner=cpu0\n"
    ).encode()


def placement_record(pid: int, cpu: int) -> bytes:
    return (
        f"MAKOS_AARCH64_TOOLCHAIN_PLACEMENT_OK pid={pid} cpu={cpu} "
        f"affinity={1 << cpu:#x} loads=0,0,0 idle_mask=0xe "
        "policy=least-dispatched-idle-ap caller_selected=0 "
        "device_mmio_owner=cpu0\n"
    ).encode()


def migration_record(
    pid: int,
    source_cpu: int,
    target_cpu: int,
    loads: tuple[int, int, int],
    idle_mask: int,
) -> bytes:
    return (
        f"MAKOS_AARCH64_TOOLCHAIN_MIGRATION_OK pid={pid} "
        f"source_cpu={source_cpu} target_cpu={target_cpu} "
        f"old_affinity={1 << source_cpu:#x} new_affinity={1 << target_cpu:#x} "
        f"loads={loads[0]},{loads[1]},{loads[2]} idle_mask={idle_mask:#x} "
        "policy=timer-safe-dispatch-imbalance delta=8 "
        "context=gpr,sp,tls,simd ownership=ready-unowned "
        "evidence_emitter=cpu0 device_mmio_owner=cpu0 caller_selected=0\n"
    ).encode()


def transcript(
    child_marker_order: tuple[bytes, ...],
    placement_cpus: tuple[int, int, int] = (1, 2, 3),
    migrations: tuple[bytes, ...] = (),
) -> bytes:
    records = [runtime.CLI_REAP_MARKER + b"\n"]
    for pid, root, cpu in zip(PIDS, ROOTS, placement_cpus):
        records.append(placement_record(pid, cpu))
        records.append(process_record(pid, root))
    records.append(
        (
            "MAKOS_AARCH64_TOOLCHAIN_PARALLEL_OK "
            "pids=201,202,203 group_pids=201,202,203 cpus=1,2,3 "
            "cpu_mask=0xe roots=0x41000,0x52000,0x63000 "
            "groups=distinct address_spaces=distinct state=running "
            "ownership=singleton evidence=scheduler-lock-snapshot "
            "evidence_emitter=cpu0\n"
        ).encode()
    )
    records.extend(marker + b"\n" for marker in child_marker_order)
    records.extend(migrations)
    records.extend(
        (
            runtime.NESTED_OUTPUT_PREFIX
            + b"500 output_bytes=1583 linked_capacity=1024 "
            + b"image_capacity=2048 data_offset=1536\n",
            runtime.THREE_CLI_REAP_MARKER + b"\n",
            runtime.HEADER_CLI_REAP_MARKER + b"\n",
            runtime.NESTED_CLI_REAP_MARKER + b"\n",
        )
    )
    for pid in (203, 201, 202):
        records.append(runtime.TOOLCHAIN_SMP_MARKER + b"status=42\n")
        records.append(
            f"process-reap arch=aarch64 pid={pid} status=42 closed_fds=0\n".encode()
        )
    records.append(runtime.PARALLEL_CLI_MARKER + b"\n")
    return b"".join(records)


def expect_failure(serial: bytes, expected: str) -> None:
    try:
        runtime.validate_parallel_makbuild(serial)
    except AssertionError as error:
        if expected not in str(error):
            raise AssertionError(
                f"wrong validation failure: expected {expected!r}, got {error!r}"
            ) from error
    else:
        raise AssertionError(f"invalid parallel transcript passed: {expected}")


markers = (
    runtime.NESTED_COLD_MARKER,
    runtime.THREE_INPUT_COLD_MARKER,
    runtime.HEADER_DEP_MARKER,
    runtime.HEADER_COLD_MARKER,
)
serial = transcript(markers)
pids, roots = runtime.validate_parallel_makbuild(serial)
if pids != PIDS or roots != ROOTS:
    raise AssertionError(f"parallel identity changed: pids={pids} roots={roots}")

# Child output is explicitly unordered: a second distinct order must also pass.
runtime.validate_parallel_makbuild(transcript(tuple(reversed(markers))))

# Initial placement is historical. A lock-ordered idle-AP migration before
# overlap must be applied before correlating the PID's snapshot CPU.
early_migration = migration_record(201, 2, 1, (0, 8, 0), 0x2)
migrated_serial = transcript(markers, (2, 2, 3), (early_migration,))
runtime.validate_parallel_makbuild(migrated_serial)

# The immutable snapshot may precede a later migration: after one peer exits,
# its AP becomes idle while CPU0 is still waiting to drain migration records.
post_snapshot_migration = migration_record(201, 1, 3, (8, 0, 0), 0x8)
runtime.validate_parallel_makbuild(
    transcript(
        markers,
        (2, 2, 3),
        (early_migration, post_snapshot_migration),
    )
)

third_spawn = process_record(PIDS[2], ROOTS[2])
early_summary = runtime.TOOLCHAIN_SMP_MARKER + b"status=42\n"
expect_failure(
    serial.replace(early_summary, b"", 1).replace(
        third_spawn, early_summary + third_spawn, 1
    ),
    "before all three placement/process spawn records",
)
third_placement = placement_record(PIDS[2], 3)
expect_failure(
    serial.replace(early_summary, b"", 1).replace(
        third_placement + third_spawn,
        third_spawn + early_summary + third_placement,
        1,
    ),
    "before all three placement/process spawn records",
)
expect_failure(
    serial.replace(b"roots=0x41000,0x52000,0x63000", b"roots=0x41000,0x41000,0x63000"),
    "invalid parallel Toolchain overlap",
)
expect_failure(
    serial.replace(b"pid=202 cpu=2 affinity=0x4", b"pid=202 cpu=3 affinity=0x8"),
    "parallel snapshot CPUs were never present in migration history",
)
expect_failure(
    serial.replace(b"pid=202 parent=17 elf=1 el=0 entry=0x400000 ttbr0=0x52000", b"pid=202 parent=17 elf=1 el=0 entry=0x400000 ttbr0=0x72000"),
    "parallel TTBR0 mismatch for pid 202",
)
expect_failure(
    migrated_serial.replace(
        early_migration,
        early_migration + migration_record(201, 2, 3, (0, 8, 0), 0x8),
    ),
    "parallel migration chain source mismatch for pid 201",
)
expect_failure(
    migrated_serial.replace(b"idle_mask=0x2", b"idle_mask=0x4"),
    "invalid parallel migration hop for pid 201",
)
expect_failure(
    migrated_serial.replace(
        b"pids=201,202,203 group_pids=201,202,203 cpus=1,2,3 "
        b"cpu_mask=0xe roots=0x41000,0x52000,0x63000",
        b"pids=202,201,203 group_pids=202,201,203 cpus=1,2,3 "
        b"cpu_mask=0xe roots=0x52000,0x41000,0x63000",
    ),
    "parallel snapshot CPUs were never present in migration history",
)
expect_failure(
    serial.replace(b"group_pids=201,202,203", b"group_pids=203,201,202"),
    "invalid parallel Toolchain overlap",
)
expect_failure(
    serial.replace(
        b"process-reap arch=aarch64 pid=203 status=42 ",
        b"process-reap arch=aarch64 pid=203 status=41 ",
    ),
    "parallel reap records are incomplete",
)

print("AArch64 parallel guest self-hosting evidence test passed")
