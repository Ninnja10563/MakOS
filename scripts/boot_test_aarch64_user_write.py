#!/usr/bin/env python3
"""Require cold immutable-mmap syscall input and preserved invalid-range denial.

The unchanged VM/EL0 harness provides one private guest, its original deadlines
and assertions, and retained raw serial/disks. No fragmented records are joined.
This fixture is not Firefox execution or macOS/HVF performance qualification.
"""

from __future__ import annotations

import json
import os
import pathlib
import re
import tempfile

import boot_test_aarch64_vm_fault as vm


ROOT = pathlib.Path(__file__).resolve().parent.parent
BEGIN_MARKER = "MAKOS_MUSL_USER_WRITE_BEGIN"
RESULT_MARKER = "MAKOS_MUSL_USER_WRITE_OK"
TTY_LINE = "#include <stdint.h>"


def validate_output(decoded: str) -> tuple[tuple[int, int, int], int]:
    tids, base = vm.validate_output(decoded)
    if "MAKOS_MUSL_USER_WRITE_FAIL" in decoded or "UNSAFE!!" in decoded:
        raise AssertionError("user-write probe failed or leaked an invalid buffer prefix")
    begins = list(re.finditer(
        r"(?:^|\n)MAKOS_MUSL_USER_WRITE_BEGIN source=immutable-mmap tty_records=2\r*\n",
        decoded,
    ))
    results = list(re.finditer(
        r"(?:^|\n)MAKOS_MUSL_USER_WRITE_OK source=immutable-mmap "
        r"first_touch=kernel-copy tty=musl-17,native-63 tty_records=2 "
        r"file_offset=4093 file_bytes=67 readback=exact negatives=10 "
        r"errors=17:-1,63:-22 cleanup=unmapped,unlinked\r*\n",
        decoded,
    ))
    if (len(begins) != 1 or decoded.count(BEGIN_MARKER) != 1 or
            len(results) != 1 or decoded.count(RESULT_MARKER) != 1):
        raise AssertionError("expected one complete bounded user-write begin/result pair")
    # Lookahead leaves the line separator available for an immediately
    # adjacent record; it does not concatenate or repair bytes.
    lines = list(re.finditer(r"(?m)^#include <stdint\.h>\r*(?=\n)", decoded))
    if len(lines) != 2:
        raise AssertionError("expected exactly two intact immutable-file TTY lines")
    if not begins[0].start() < lines[0].start() < lines[1].start() < results[0].start():
        raise AssertionError("cold TTY lines are not between their begin/result records")
    return tids, base


def main() -> int:
    output_root = pathlib.Path(os.environ.get("MAKOS_AARCH64_TEMP_ROOT", ROOT / "build"))
    session_root = pathlib.Path(tempfile.mkdtemp(prefix="makos-user-write-", dir=output_root))
    print(f"MAKOS_AARCH64_USER_WRITE_SESSION path={session_root}", flush=True)
    old_root = os.environ.get("MAKOS_AARCH64_TEMP_ROOT")
    os.environ["MAKOS_AARCH64_TEMP_ROOT"] = str(session_root)
    try:
        status = vm.main()
    finally:
        if old_root is None:
            os.environ.pop("MAKOS_AARCH64_TEMP_ROOT", None)
        else:
            os.environ["MAKOS_AARCH64_TEMP_ROOT"] = old_root
    if status != 0:
        raise AssertionError(f"unchanged VM/EL0 gate returned {status}")
    sessions = list(session_root.glob("makos-vm-fault-*/makos-el0-entry-*/session.json"))
    if len(sessions) != 1:
        raise AssertionError(f"expected one retained VM/EL0 session, found {len(sessions)}")
    session = sessions[0].parent
    record = json.loads(sessions[0].read_text())
    validate_output((session / "serial.log").read_bytes().decode(errors="replace"))
    print("MAKOS_AARCH64_USER_WRITE_RUNTIME_OK "
          f"accel={record['accelerator']} fixture=dynamic-musl "
          "source=immutable-mmap first_touch=kernel-copy "
          "tty=musl-17,native-63 tty_records=2 file_offset=4093 file_bytes=67 "
          "readback=exact negatives=10 errors=17:-1,63:-22 "
          "cleanup=unmapped,unlinked boot=unchanged "
          f"session={session} firefox=not-tested")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
