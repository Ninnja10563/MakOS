#!/usr/bin/env python3
"""Require coherent concurrent first touches by real dynamic-musl AP threads.

The existing EL0 harness supplies its unchanged launch, deadlines, assertions,
private disks, and retained raw serial. This additional gate validates only
complete VM-fault records; it does not reconstruct fragments or count races
that the kernel has not independently observed. This is not Firefox execution.
"""

from __future__ import annotations

import json
import os
import pathlib
import re
import tempfile

import boot_test_aarch64_el0_entry as el0


ROOT = pathlib.Path(__file__).resolve().parent.parent
RESULT_MARKER = "MAKOS_MUSL_VM_FAULT_OK"
ROUNDS = 16
TABLE_BYTES = 2 * 1024 * 1024


def validate_output(decoded: str) -> tuple[tuple[int, int, int], int]:
    tids, _, _ = el0.validate_output(decoded)
    if "MAKOS_PANIC" in decoded:
        raise AssertionError("VM-fault probe reached a kernel panic")
    matches = re.findall(
        r"(?:^|\n)MAKOS_MUSL_VM_FAULT_OK loader=musl threads=3 "
        r"singleton=0x2,0x4,0x8 tids=(\d+),(\d+),(\d+) rounds=16 "
        r"same_page_rounds=16 distinct_pages=48 table_stride=2097152 "
        r"base=(0x[0-9a-f]+) coherent_checks=288 "
        r"first_touch=barrier-released statuses=42,42,42 cleanup=unmapped\r*\n",
        decoded,
    )
    if len(matches) != 1 or decoded.count(RESULT_MARKER) != 1:
        raise AssertionError("expected one complete dynamic-musl VM-fault result")
    first, second, third, base_text = matches[0]
    if (int(first), int(second), int(third)) != tids:
        raise AssertionError("VM-fault workers differ from validated EL0 AP workers")
    base = int(base_text, 16)
    if (not 0x80000000 <= base < 0x3C0000000
            or base % TABLE_BYTES
            or base + ROUNDS * 2 * TABLE_BYTES > 0x3C0000000):
        raise AssertionError(f"VM-fault reservation is not a valid fresh-table span: {base:#x}")
    return tids, base


def main() -> int:
    output_root = pathlib.Path(os.environ.get("MAKOS_AARCH64_TEMP_ROOT", ROOT / "build"))
    session_root = pathlib.Path(tempfile.mkdtemp(prefix="makos-vm-fault-", dir=output_root))
    print(f"MAKOS_AARCH64_VM_FAULT_SESSION path={session_root}", flush=True)
    old_root = os.environ.get("MAKOS_AARCH64_TEMP_ROOT")
    os.environ["MAKOS_AARCH64_TEMP_ROOT"] = str(session_root)
    try:
        # The existing harness checks for any running QEMU, uses one private
        # guest, retains failure evidence, and terminates it before returning.
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
    tids, base = validate_output((session / "serial.log").read_bytes().decode(errors="replace"))
    print("MAKOS_AARCH64_VM_FAULT_RUNTIME_OK "
          f"accel={record['accelerator']} fixture=dynamic-musl-pthread "
          f"threads=3 tids={tids[0]},{tids[1]},{tids[2]} singleton=0x2,0x4,0x8 "
          f"rounds=16 same_page_rounds=16 distinct_pages=48 table_stride={TABLE_BYTES} "
          f"base={base:#x} coherent_checks=288 first_touch=barrier-released "
          "statuses=42,42,42 cleanup=unmapped boot=unchanged "
          f"session={session} kernel_loser_interleaving=not-counted firefox=not-tested")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
