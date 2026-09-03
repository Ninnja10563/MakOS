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

    host_test = output / "include-parser-test.c"
    host_binary = output / "include-parser-test"
    host_test.write_text(
        f'''#define MAKOS_AARCH64_TOOLCHAIN_HOST_TEST 1
#define _start makos_aarch64_toolchain_guest_start
#include "{ROOT / "user/aarch64_toolchain.c"}"
#undef _start

static const char inline_path[] = "/home/user/generated-inline.h";
static const char leaf_path[] = "/home/user/generated-leaf.h";
static const char stdint_path[] = "/usr/include/stdint.h";
static const uint8_t inline_bytes[] =
    "#ifndef GEN_INLINE_H\\n"
    "#define GEN_INLINE_H\\n"
    "#include \\"/home/user/generated-leaf.h\\"\\n"
    "#endif\\n";
static const uint8_t leaf_bytes[] =
    "#define INCLUDED_VALUE 2\\n"
    "int included_answer = INCLUDED_VALUE;\\n";
static const uint8_t stdint_bytes[] = "typedef unsigned long uint64_t;\\n";

uint64_t makos_aarch64_toolchain_host_syscall(
    uint64_t number, uint64_t first, uint64_t second, uint64_t third,
    uint64_t fourth) {{
    (void)fourth;
    if (number == SYS_OPEN) {{
        const char *path = (const char *)(uintptr_t)first;
        size_t length = (size_t)second;
        if (same_name(path, length, inline_path, sizeof(inline_path) - 1)) return 3;
        if (same_name(path, length, leaf_path, sizeof(leaf_path) - 1)) return 4;
        if (same_name(path, length, stdint_path, sizeof(stdint_path) - 1)) return 5;
        return UINT64_MAX;
    }}
    if (number == SYS_READ) {{
        const uint8_t *source = 0;
        size_t length = 0;
        if (first == 3) {{ source = inline_bytes; length = sizeof(inline_bytes) - 1; }}
        if (first == 4) {{ source = leaf_bytes; length = sizeof(leaf_bytes) - 1; }}
        if (first == 5) {{ source = stdint_bytes; length = sizeof(stdint_bytes) - 1; }}
        if (!source || length > (size_t)third) return UINT64_MAX;
        copy_bytes((uint8_t *)(uintptr_t)second, source, length);
        return length;
    }}
    if (number == SYS_CLOSE) return first >= 3 && first <= 5;
    if (number == SYS_WRITE) return second;
    return UINT64_MAX;
}}

static int bytes_equal(const uint8_t *first, const uint8_t *second,
                       size_t length) {{
    for (size_t index = 0; index < length; ++index)
        if (first[index] != second[index]) return 0;
    return 1;
}}

int main(void) {{
    static const uint8_t quoted_source[] =
        "int root;\\n"
        "#include \\"/home/user/generated-inline.h\\"\\n"
        "#include \\"/home/user/generated-inline.h\\"\\n";
    static const uint8_t quoted_expected[] =
        "int root;\\nint included_answer = 2;\\n";
    static const uint8_t angle_source[] = "#include <stdint.h>\\n";
    struct build_manifest build = {{0}};
    build.input_count = 2;
    build.inputs[0].language = BUILD_LANGUAGE_ASM;
    build.inputs[0].source_path = "/home/user/root.s";
    build.inputs[0].source_path_length = sizeof("/home/user/root.s") - 1;
    build.inputs[0].object_path = "/home/user/root.o";
    build.inputs[0].object_path_length = sizeof("/home/user/root.o") - 1;
    build.inputs[1].language = BUILD_LANGUAGE_C;
    build.inputs[1].source_path = "/home/user/root.c";
    build.inputs[1].source_path_length = sizeof("/home/user/root.c") - 1;
    build.inputs[1].object_path = "/home/user/root-c.o";
    build.inputs[1].object_path_length = sizeof("/home/user/root-c.o") - 1;
    build.output_path = "/home/user/root.elf";
    build.output_path_length = sizeof("/home/user/root.elf") - 1;

    uint8_t output[BUILD_EXPANDED_SOURCE_CAPACITY] = {{0}};
    struct build_dependencies dependencies = {{0}};
    size_t length = expand_build_source(
        &build, 1, quoted_source, sizeof(quoted_source) - 1, output,
        sizeof(output), &dependencies);
    if (length != sizeof(quoted_expected) - 1 || dependencies.count != 2 ||
        dependencies.max_depth != 2 ||
        !bytes_equal(output, quoted_expected, length)) return 1;

    memset(output, 0, sizeof(output));
    memset(&dependencies, 0, sizeof(dependencies));
    length = expand_build_source(
        &build, 1, angle_source, sizeof(angle_source) - 1, output,
        sizeof(output), &dependencies);
    if (length != sizeof(stdint_bytes) - 1 || dependencies.count != 1 ||
        dependencies.max_depth != 1 ||
        !bytes_equal(output, stdint_bytes, length)) return 2;
    return 0;
}}
'''
    )
    subprocess.run(
        [clang, "-std=c17", "-O0", "-I", str(output), str(host_test), "-o", str(host_binary)],
        check=True,
    )
    subprocess.run([str(host_binary)], check=True)

print("MAKOS_AARCH64_TOOLCHAIN_FREESTANDING_OK memcpy=defined-exact nonrecursive=1 undefined=0 link=aarch64-et_exec include=self-generated-exact include_parser=quoted-absolute-recursive-guard,angle-stdint")
