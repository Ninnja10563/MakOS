#!/usr/bin/env python3
"""Require real hardware BTYPE capture, outer EL0 entry and post-ERET IRQ.

The existing EL0 harness is invoked unchanged once: its deadlines, assertions,
private disks and serial retention still apply. This extra gate does not join
fragmented records or treat an unsupported CPU as successful qualification.
"""

from __future__ import annotations

import json
import os
import pathlib
import re
import tempfile

import boot_test_aarch64_el0_entry as el0


ROOT = pathlib.Path(__file__).resolve().parent.parent
PREFIX = "MAKOS_AARCH64_BTYPE_"
LOOP_PCS = {0x10000080, 0x10000084}


def unique_record(decoded: str, name: str, suffix: str) -> re.Match:
    marker = PREFIX + name
    matches = list(re.finditer(r"(?:^|\n)" + marker + suffix + r"\r*\n", decoded))
    if len(matches) != 1 or decoded.count(marker) != 1:
        raise AssertionError(f"expected one complete {marker} record")
    return matches[0]


def validate_output(decoded: str) -> tuple[int, int]:
    if PREFIX + "UNSUPPORTED" in decoded:
        raise AssertionError("FEAT_BTI unsupported: hardware BTYPE was not qualified")
    if "MAKOS_PANIC" in decoded:
        raise AssertionError("BTYPE probe reached a kernel panic")
    el0.validate_output(decoded)
    fields = (r" tid=(\d+) root=(0x[0-9a-f]+) pc=(0x[0-9a-f]+) "
              r"sp=(0x[0-9a-f]+) tls=(0x[0-9a-f]+) spsr=0x80000400 btype=1 ")
    source = unique_record(decoded, "SOURCE_OK", " cpu=1" + fields + "capture=hardware-timer")
    entry = unique_record(decoded, "ENTRY_OK", " cpu=2" + fields + "proof=validated-before-eret")
    target = unique_record(decoded, "TARGET_OK", " cpu=2" + fields + "proof=hardware-timer-after-eret")
    result = unique_record(
        decoded, "OK", r" tid=(\d+) source_cpu=1 target_cpu=2 root=(0x[0-9a-f]+) "
        r"spsr=0x80000400 migrations=1 capture=hardware-timer entry=validated-before-eret "
        r"resumed=hardware-timer status=42 termination=kernel-after-irq-proof "
        r"cleanup=reaped free_balance=1 scope=immutable-boot-probe",
    )
    if not source.start() < entry.start() < target.start() < result.start():
        raise AssertionError("BTYPE source/entry/target/result records are out of order")
    tid = int(source[1])
    root, pc, sp, tls = (int(value, 16) for value in source.groups()[1:])
    if tid <= 0 or root == 0 or root % 4096:
        raise AssertionError("BTYPE hardware capture has an invalid TID/root")
    if pc not in LOOP_PCS or sp != 0x3FFFFFFF0 or tls != 0x11223344:
        raise AssertionError("BTYPE hardware capture did not come from the immutable branch loop")
    if entry.groups() != source.groups():
        raise AssertionError("BTYPE selected entry differs from the captured source context")
    if (target[1], target[2], target[4], target[5]) != (source[1], source[2], source[4], source[5]):
        raise AssertionError("BTYPE post-ERET IRQ changed TID/root/SP/TLS")
    if int(target[3], 16) not in LOOP_PCS:
        raise AssertionError("BTYPE post-ERET IRQ did not resume the immutable branch loop")
    if result.groups() != (source[1], source[2]):
        raise AssertionError("BTYPE final result has a different TID/root")
    # Other immutable boot probes reuse PID 1 after resetting their process
    # tables. Require this task's real reap between its own target/result.
    reaps = list(re.finditer(
        rf"(?m)^process-reap arch=aarch64 pid={tid} status=42 "
        r"closed_fds=\d+ closed_tty_fds=\d+ closed_surfaces=\d+ closed_sockets=\d+ "
        r"closed_epolls=\d+ closed_ipc_handles=\d+ closed_futex_waiters=\d+ "
        r"vm_regions=\d+ vm_pages=\d+ reclaimed_frames=([1-9]\d*)\r*\n",
        decoded[target.end():result.start() + 1],
    ))
    if len(reaps) != 1:
        raise AssertionError("BTYPE task was not unambiguously reaped after its post-ERET IRQ")
    return tid, root


def main() -> int:
    output_root = pathlib.Path(os.environ.get("MAKOS_AARCH64_TEMP_ROOT", ROOT / "build"))
    session_root = pathlib.Path(tempfile.mkdtemp(prefix="makos-btype-", dir=output_root))
    print(f"MAKOS_AARCH64_BTYPE_SESSION path={session_root}", flush=True)
    old_root = os.environ.get("MAKOS_AARCH64_TEMP_ROOT")
    os.environ["MAKOS_AARCH64_TEMP_ROOT"] = str(session_root)
    try:
        status = el0.main()
    finally:
        if old_root is None:
            os.environ.pop("MAKOS_AARCH64_TEMP_ROOT", None)
        else:
            os.environ["MAKOS_AARCH64_TEMP_ROOT"] = old_root
    if status != 0:
        raise AssertionError(f"unchanged EL0-entry gate returned {status}")
    sessions = list(session_root.glob("makos-el0-entry-*/session.json"))
    if len(sessions) != 1:
        raise AssertionError(f"expected one retained EL0 session, found {len(sessions)}")
    session = sessions[0].parent
    record = json.loads(sessions[0].read_text())
    tid, root = validate_output((session / "serial.log").read_bytes().decode(errors="replace"))
    print("MAKOS_AARCH64_BTYPE_RUNTIME_OK "
          f"accel={record['accelerator']} tid={tid} root={root:#x} "
          "source_cpu=1 target_cpu=2 spsr=0x80000400 btype=1 "
          "capture=hardware-timer entry=validated-before-eret resumed=hardware-timer "
          "migrations=1 status=42 termination=kernel-after-irq-proof cleanup=reaped "
          f"free_balance=1 boot=unchanged session={session} firefox=not-tested")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
