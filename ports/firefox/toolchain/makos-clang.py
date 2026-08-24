#!/usr/bin/env python3
"""Bootstrap clang driver for a distinct AArch64 MakOS ELF target.

LLVM clang emits correct MakOS objects but Darwin-host builds delegate unknown
OS links to /usr/bin/gcc.  This driver keeps clang for preprocessing/codegen and
invokes ELF ld.lld directly.  Default runtime links remain blocked until a real
MakOS sysroot exists; -nostdlib links are usable for ABI probes now.
"""

from __future__ import annotations

import os
from pathlib import Path
import subprocess
import sys
import tempfile


TARGET = "aarch64-unknown-makos"
COMPILE_ONLY = {"-c", "-E", "-S", "-fsyntax-only", "-M", "-MM"}
DRIVER_QUERIES = {
    "-v",
    "-dumpmachine",
    "-print-libgcc-file-name",
    "-print-search-dirs",
    "-print-resource-dir",
    "--print-resource-dir",
    "-print-target-triple",
}
SOURCE_SUFFIXES = {".c", ".cc", ".cpp", ".cxx", ".C", ".m", ".mm", ".s", ".S"}
C_FAMILY_SUFFIXES = {".c", ".cc", ".cpp", ".cxx", ".C", ".m", ".mm"}
LINK_INPUT_SUFFIXES = {".o", ".lo", ".a", ".rlib", ".so"}


def fail(message: str) -> "None":
    print(f"makos-clang: {message}", file=sys.stderr)
    raise SystemExit(1)


def tool(name: str, fallback: str) -> str:
    value = os.environ.get(name, fallback)
    if not Path(value).is_file():
        fail(f"{name} tool missing: {value}")
    return value


def target_args(args: list[str]) -> list[str]:
    result: list[str] = []
    found = False
    index = 0
    # cc-rs derives a `-musl` suffix from Rust target metadata. LLVM's real
    # distinct target remains aarch64-unknown-makos; normalize only this alias.
    accepted = {TARGET, f"{TARGET}-musl"}
    while index < len(args):
        arg = args[index]
        if arg in ("-target", "--target"):
            if index + 1 == len(args):
                fail(f"{arg} requires a value")
            value = args[index + 1]
            if value not in accepted:
                fail(f"only {TARGET} is supported")
            result.extend((arg, TARGET))
            found = True
            index += 2
            continue
        if arg.startswith("--target="):
            value = arg.split("=", 1)[1]
            if value not in accepted:
                fail(f"only {TARGET} is supported")
            result.append(f"--target={TARGET}")
            found = True
            index += 1
            continue
        result.append(arg)
        index += 1
    return result if found else [f"--target={TARGET}", *result]


def is_compile_only(args: list[str]) -> bool:
    return any(arg in COMPILE_ONLY or arg.startswith("-M") for arg in args)


def add_security_defaults(args: list[str]) -> list[str]:
    if not any(Path(arg).suffix in C_FAMILY_SUFFIXES for arg in args):
        return args
    if any(
        arg == "-fno-stack-protector" or arg.startswith("-fstack-protector")
        for arg in args
    ):
        return args
    return [*args, "-fstack-protector-strong"]


def compile_source(clang: str, args: list[str], source: str, output: str) -> None:
    compile_flags: list[str] = []
    skip = False
    for index, arg in enumerate(args):
        if skip:
            skip = False
            continue
        if arg in ("-o", "-Xlinker"):
            skip = True
            continue
        if arg.startswith("-Wl,") or arg in (
            "-shared",
            "-static",
            "-pie",
            "-nostdlib",
            "-nodefaultlibs",
            "-rdynamic",
        ):
            continue
        if Path(arg).suffix in SOURCE_SUFFIXES or Path(arg).suffix in LINK_INPUT_SUFFIXES:
            continue
        if arg.startswith("-l") or arg.startswith("-L") or arg.startswith("-fuse-ld="):
            continue
        compile_flags.append(arg)
    command = [clang, *target_args(compile_flags), "-c", source, "-o", output]
    raise_if_failed(command)


def raise_if_failed(command: list[str]) -> None:
    result = subprocess.run(command)
    if result.returncode:
        raise SystemExit(result.returncode)


def linker_args(
    args: list[str], generated_objects: dict[int, str]
) -> tuple[list[str], str | None, bool, bool, bool]:
    result: list[str] = []
    sysroot: str | None = None
    nostdlib = False
    nodefaultlibs = False
    shared = False
    index = 0
    while index < len(args):
        arg = args[index]
        if index in generated_objects:
            result.append(generated_objects[index])
            index += 1
            continue
        if arg in ("--target", "-target"):
            index += 2
            continue
        if arg.startswith("--target=") or arg.startswith("-fuse-ld="):
            index += 1
            continue
        if arg == "-o":
            if index + 1 == len(args):
                fail("-o requires a value")
            result.extend(("-o", args[index + 1]))
            index += 2
            continue
        if arg == "-Xlinker":
            if index + 1 == len(args):
                fail("-Xlinker requires a value")
            result.append(args[index + 1])
            index += 2
            continue
        if arg.startswith("-Wl,"):
            result.extend(part for part in arg[4:].split(",") if part)
            index += 1
            continue
        if arg == "--sysroot":
            if index + 1 == len(args):
                fail("--sysroot requires a value")
            sysroot = args[index + 1]
            result.append(f"--sysroot={sysroot}")
            index += 2
            continue
        if arg.startswith("--sysroot="):
            sysroot = arg.split("=", 1)[1]
            result.append(arg)
            index += 1
            continue
        if arg == "-nostdlib":
            nostdlib = True
            index += 1
            continue
        if arg == "-nodefaultlibs":
            nodefaultlibs = True
            index += 1
            continue
        if arg == "-shared":
            shared = True
            result.append(arg)
            index += 1
            continue
        if arg in ("-static", "-pie") or arg.startswith(("-L", "-l")):
            result.append(arg)
            index += 1
            continue
        if arg == "-pthread":
            result.append("-lpthread")
            index += 1
            continue
        if arg == "-rdynamic":
            result.append("--export-dynamic")
            index += 1
            continue
        if arg.startswith("@") or Path(arg).suffix in LINK_INPUT_SUFFIXES:
            result.append(arg)
            index += 1
            continue
        index += 1
    return result, sysroot, nostdlib, nodefaultlibs, shared


def add_runtime(args: list[str], sysroot: str | None, shared: bool, cxx: bool) -> list[str]:
    if not sysroot:
        fail("default link requires --sysroot=<MakOS sysroot>")
    lib = Path(sysroot) / "usr/lib"
    builtins = lib / "makos/libclang_rt.builtins-aarch64.a"
    stdio_internals = (lib / "makos/__overflow.o", lib / "makos/__uflow.o")
    dynamic = os.environ.get("MAKOS_DYNAMIC_RUNTIME") == "1"
    required = [lib / ("libc.so" if dynamic else "libc.a"), builtins]
    if not dynamic:
        required.extend(stdio_internals)
    if not shared:
        required.extend(
            (lib / ("Scrt1.o" if dynamic else "crt1.o"), lib / "crti.o", lib / "crtn.o")
        )
    if cxx:
        required.extend((lib / "libc++.a", lib / "libc++abi.a", lib / "libunwind.a"))
    missing = [str(path) for path in required if not path.is_file()]
    if missing:
        fail("default target runtime incomplete: " + ", ".join(missing))
    # Executables may consume Gecko/NSPR/NSS DSOs.  With only archive inputs,
    # lld still emits a dependency-free static ELF; do not force -static and
    # reject intentional shared inputs.
    start = (
        []
        if shared
        else [str(lib / ("Scrt1.o" if dynamic else "crt1.o")), str(lib / "crti.o")]
    )
    end = [] if shared else [str(lib / "crtn.o")]
    libraries = ["--start-group", f"-L{lib}"]
    if cxx:
        libraries.extend(("-lc++", "-lc++abi", "-lunwind"))
    # musl's stdio scanner calls protected archive-internal entries. Link exact
    # upstream objects without importing duplicate public overrides such as
    # Gecko's abort().
    if not dynamic:
        libraries.extend(str(path) for path in stdio_internals)
    libraries.append("-lc")
    libraries.extend(("-lm", "-lpthread", "-ldl", str(builtins), "--end-group"))
    dynamic_options = (
        ["--dynamic-linker=/lib/ld-musl-aarch64.so.1"]
        if dynamic and not shared
        else []
    )
    if dynamic and "-static" in args:
        fail("dynamic MakOS runtime cannot satisfy explicit -static link")
    return [*start, *dynamic_options, *args, *libraries, *end]


def add_start_files(args: list[str], sysroot: str | None, shared: bool) -> list[str]:
    if shared:
        return args
    if not sysroot:
        fail("startup files require --sysroot=<MakOS sysroot>")
    lib = Path(sysroot) / "usr/lib"
    required = (lib / "crt1.o", lib / "crti.o", lib / "crtn.o")
    missing = [str(path) for path in required if not path.is_file()]
    if missing:
        fail("target startup runtime incomplete: " + ", ".join(missing))
    return ["-static", str(required[0]), str(required[1]), *args, str(required[2])]


def add_cxx_sysroot_headers(args: list[str], cxx: bool) -> list[str]:
    if not cxx or "-nostdinc++" in args:
        return args
    sysroot: str | None = None
    for index, arg in enumerate(args):
        if arg == "--sysroot" and index + 1 < len(args):
            sysroot = args[index + 1]
        elif arg.startswith("--sysroot="):
            sysroot = arg.split("=", 1)[1]
    if not sysroot:
        return args
    headers = Path(sysroot) / "usr/include/c++/v1"
    if not headers.is_dir():
        return args
    return [*args, "-isystem", str(headers)]


def main() -> None:
    args = sys.argv[1:]
    clang = tool("MAKOS_REAL_CLANG", "/opt/homebrew/opt/llvm/bin/clang")
    lld = tool("MAKOS_LLD", "/opt/homebrew/opt/lld/bin/ld.lld")
    cxx = os.environ.get("MAKOS_DRIVER_CXX") == "1" or "++" in Path(sys.argv[0]).name
    if cxx:
        clang = tool("MAKOS_REAL_CLANGXX", "/opt/homebrew/opt/llvm/bin/clang++")

    args = target_args(args)
    args = add_cxx_sysroot_headers(args, cxx)
    args = add_security_defaults(args)
    if any(arg in DRIVER_QUERIES or arg.startswith("-print-file-name=") for arg in args):
        os.execv(clang, [clang, *args])
    if is_compile_only(args):
        os.execv(clang, [clang, *args])

    if any(arg == "--version" for arg in args) and not any(
        arg.startswith("-Wl,") for arg in args
    ):
        os.execv(clang, [clang, *args])

    with tempfile.TemporaryDirectory(prefix="makos-clang-") as temporary:
        generated_objects: dict[int, str] = {}
        for number, arg in enumerate(args):
            if Path(arg).suffix in SOURCE_SUFFIXES:
                output = str(Path(temporary) / f"input-{number}.o")
                compile_source(clang, args, arg, output)
                generated_objects[number] = output
        link, sysroot, nostdlib, nodefaultlibs, shared = linker_args(args, generated_objects)
        if "--version" in link:
            os.execv(lld, [lld, "--version"])
        if not nostdlib and not nodefaultlibs:
            link = add_runtime(link, sysroot, shared, cxx)
        elif not nostdlib and nodefaultlibs:
            link = add_start_files(link, sysroot, shared)
        if (nostdlib or nodefaultlibs) and "-static" in link:
            # rustc passes mutually-dependent rlibs with -nodefaultlibs.  Keep
            # their one-pass ELF archive resolution correct without injecting
            # any runtime libraries that rustc did not request.
            link = ["--start-group", *link, "--end-group"]
        if "-o" not in link:
            link.extend(("-o", "a.out"))
        raise_if_failed([lld, "-m", "aarch64elf", *link])


if __name__ == "__main__":
    main()
