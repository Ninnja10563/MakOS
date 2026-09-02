#!/usr/bin/env python3
"""Compile/link the exact freestanding guest toolchain and audit its memcpy."""

from __future__ import annotations

import os
import pathlib
import re
import shutil
import subprocess
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
STAGED_DIRS = [ROOT / "build/host-tools/llvm19/usr/bin"]
if ROOT.parent.name == "build":
    STAGED_DIRS.append(ROOT.parent.parent / "build/host-tools/llvm19/usr/bin")


def tool(environment: str, staged: str, fallback: str) -> str:
    value = os.environ.get(environment)
    if value:
        return value
    for directory in STAGED_DIRS:
        candidate = directory / staged
        if candidate.exists():
            return str(candidate)
    found = shutil.which(fallback)
    if found:
        return found
    raise SystemExit(f"AArch64 toolchain freestanding test blocked: {fallback} unavailable")


clang = tool("MAKOS_TEST_CLANG", "clang-19", "clang")
lld = tool("MAKOS_TEST_LLD", "ld.lld-19", "ld.lld")
nm = tool("MAKOS_TEST_NM", "llvm-nm-19", "llvm-nm")
readelf = tool("MAKOS_TEST_READELF", "llvm-readelf-19", "llvm-readelf")
objdump = tool("MAKOS_TEST_OBJDUMP", "llvm-objdump-19", "llvm-objdump")

def fnv1a(data: bytes) -> int:
    value = 14_695_981_039_346_656_037
    for byte in data:
        value ^= byte
        value = (value * 1_099_511_628_211) & 0xFFFF_FFFF_FFFF_FFFF
    return value


def append_array(output: list[str], name: str, data: bytes) -> None:
    output.append(f"static const uint8_t {name}[] = {{\n")
    for offset in range(0, len(data), 16):
        output.append("    ")
        output.extend(f"0x{byte:02x}, " for byte in data[offset : offset + 16])
        output.append("\n")
    output.append("};\n")
    output.append(f"static const size_t {name}_LENGTH = {len(data)};\n")
    output.append(
        f"static const uint64_t {name}_FNV1A = UINT64_C(0x{fnv1a(data):016x});\n"
    )


def generated_include() -> bytes:
    output = ["/* Generated from exact repository source bytes by kernel/build.rs. */\n"]
    for name, relative in (
        ("REPOSITORY_SELFHOST_C_SOURCE", "user/aarch64_selfhost_probe.c"),
        ("REPOSITORY_SELFHOST_ASM_SOURCE", "user/aarch64_selfhost_probe.S"),
        ("PRODUCTION_SHARED_DEMO_SOURCE", "ports/musl/shared-demo.c"),
        ("SELFHOST_STDINT_SOURCE", "sdk/selfhost/include/stdint.h"),
    ):
        data = (ROOT / relative).read_bytes()
        if not data:
            raise SystemExit(f"AArch64 toolchain freestanding test blocked: {relative} empty")
        append_array(output, name, data)
    return "".join(output).encode()


include_bytes = generated_include()
include_value = os.environ.get("MAKOS_AARCH64_SELFHOST_INCLUDE")
if include_value and pathlib.Path(include_value).read_bytes() != include_bytes:
    raise SystemExit("explicit generated self-host include differs from canonical bytes")

with tempfile.TemporaryDirectory(prefix="makos-aarch64-toolchain-") as directory:
    output = pathlib.Path(directory)
    (output / "aarch64-selfhost-sources.inc").write_bytes(include_bytes)
    obj = output / "aarch64-toolchain.o"
    elf = output / "aarch64-toolchain.elf"
    subprocess.run(
        [
            clang,
            "-target",
            "aarch64-unknown-none-elf",
            "-std=c17",
            "-ffreestanding",
            "-fno-builtin",
            "-fno-stack-protector",
            "-fno-pic",
            "-fno-unwind-tables",
            "-fno-asynchronous-unwind-tables",
            "-mgeneral-regs-only",
            "-Os",
            "-I",
            str(output),
            "-c",
            str(ROOT / "user/aarch64_toolchain.c"),
            "-o",
            str(obj),
        ],
        check=True,
    )
    undefined = subprocess.run(
        [nm, "--undefined-only", str(obj)], check=True, text=True, stdout=subprocess.PIPE
    ).stdout
    if undefined.strip():
        raise SystemExit(f"freestanding toolchain retains undefined symbols:\n{undefined}")
    defined = subprocess.run(
        [nm, "--defined-only", str(obj)], check=True, text=True, stdout=subprocess.PIPE
    ).stdout
    if len(re.findall(r"^[0-9a-fA-F]+ T memcpy$", defined, re.M)) != 1:
        raise SystemExit("freestanding toolchain does not define exactly one global memcpy")
    disassembly = subprocess.run(
        [objdump, "-dr", str(obj)], check=True, text=True, stdout=subprocess.PIPE
    ).stdout
    match = re.search(r"<memcpy>:\n(?P<body>.*?)(?=\n[0-9a-f]+ <|\Z)", disassembly, re.S)
    if not match or re.search(r"\b(bl|blr)\b|R_AARCH64_", match.group("body")):
        raise SystemExit("memcpy contains a call/relocation and may recurse")
    subprocess.run(
        [
            lld,
            "-flavor",
            "gnu",
            "--build-id=none",
            "--no-undefined",
            "-z",
            "max-page-size=4096",
            "-T",
            str(ROOT / "user/linker-aarch64.ld"),
            "-o",
            str(elf),
            str(obj),
        ],
        check=True,
    )
    header = subprocess.run(
        [readelf, "--file-header", str(elf)], check=True, text=True, stdout=subprocess.PIPE
    ).stdout
    if not re.search(r"Type:\s+EXEC", header) or not re.search(r"Machine:\s+AArch64", header):
        raise SystemExit("freestanding toolchain link is not AArch64 ET_EXEC")

print("MAKOS_AARCH64_TOOLCHAIN_FREESTANDING_OK memcpy=defined-exact nonrecursive=1 undefined=0 link=aarch64-et_exec include=self-generated-exact")
