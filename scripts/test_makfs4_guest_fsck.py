#!/usr/bin/env python3
"""Run full AArch64 gate, then fsck its closed real guest volume."""

from __future__ import annotations

import os
import pathlib
import subprocess
import tempfile


ROOT = pathlib.Path(__file__).resolve().parent.parent


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="makos-guest-fsck-", dir=ROOT / "build") as temporary:
        volume = pathlib.Path(temporary) / "quiescent-data.img"
        environment = os.environ.copy()
        environment.setdefault("MAKOS_AARCH64_SKIP_BROWSER_FETCH", "1")
        environment["MAKOS_AARCH64_PRESERVE_DATA_IMAGE"] = str(volume)
        subprocess.run(
            [os.environ.get("PYTHON", "python3"), str(ROOT / "scripts/boot_test_aarch64.py")],
            cwd=ROOT,
            env=environment,
            check=True,
        )
        subprocess.run(
            [
                "cargo",
                "run",
                "--release",
                "-q",
                "-p",
                "makos-makfs4-fsck",
                "--",
                str(volume),
            ],
            cwd=ROOT,
            check=True,
        )
    print("MAKOS_MAKFS4_GUEST_FSCK_OK state=quiescent qemu=closed runtime=two-boot")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
