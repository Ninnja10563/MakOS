#!/usr/bin/env python3
"""Add unstable custom-target flags only to MakOS target rustc commands."""

import os
import sys


def makos_target(arguments: list[str]) -> bool:
    for index, argument in enumerate(arguments):
        if argument.startswith("--target="):
            return "aarch64-unknown-makos" in argument
        if argument == "--target" and index + 1 < len(arguments):
            return "aarch64-unknown-makos" in arguments[index + 1]
    return False


real_rustc = os.environ["MAKOS_REAL_RUSTC"]
arguments = sys.argv[1:]
if makos_target(arguments):
    sysroot = os.environ["MAKOS_SYSROOT"]
    arguments.extend(
        [
            "-Zunstable-options",
            "-C",
            f"link-arg=--sysroot={sysroot}",
            "-C",
            f"link-arg=-L{sysroot}/usr/lib",
        ]
    )
os.execv(real_rustc, [real_rustc, *arguments])
