#!/usr/bin/env python3
"""Check the MakOS print-settings factory, optionally compiling a real object."""

from __future__ import annotations

import os
import re
import shlex
import shutil
import subprocess
import tempfile
from pathlib import Path


PORT = Path(__file__).resolve().parent
REPO = PORT.parents[1]
PATCH = PORT / "patches" / "0058-makos-pdf-print-settings.patch"


def fail(message: str) -> "None":
    raise SystemExit(f"Firefox MakOS print-settings test blocked: {message}")


def added_file(patch: str, path: str) -> str:
    marker = f"+++ b/{path}\n"
    try:
        tail = patch.split(marker, 1)[1]
    except IndexError:
        fail(f"patch does not add {path}")
    body = tail.split("\ndiff --git ", 1)[0]
    lines = body.splitlines()
    while lines and not lines[0].startswith("@@"):
        lines.pop(0)
    if not lines:
        fail(f"patch has no content hunk for {path}")
    lines.pop(0)
    if any(line and line[0] not in "+ \\" for line in lines):
        fail(f"unsupported patch content for {path}")
    return "\n".join(line[1:] for line in lines if line.startswith("+")) + "\n"


patch = PATCH.read_text()
source_text = added_file(patch, "widget/makos/nsPrintSettingsMakOS.cpp")
required = (
    "CreatePlatformPrintSettings(",
    "settings->InitWithInitializer(aSettings);",
    "settings->SetOutputFormat(nsIPrintSettings::kOutputFormatPDF);",
    "settings->SetDefaultFileName();",
)
for text in required:
    if source_text.count(text) != 1:
        fail(f"factory must contain exactly one {text!r}")
if "kOutputFormatNative" in source_text or "PDF printer" in source_text:
    fail("factory falsely claims a native printer")

if os.environ.get("MAKOS_FIREFOX_PRINT_COMPILE_EVIDENCE", "0") != "1":
    print(
        "MAKOS_FIREFOX_PRINT_SETTINGS_STRUCTURAL_OK "
        "compile=skipped-request-MAKOS_FIREFOX_PRINT_COMPILE_EVIDENCE=1"
    )
    raise SystemExit(0)

source_dir = Path(
    os.environ.get(
        "MAKOS_FIREFOX_SOURCE_DIR",
        REPO / "build" / "ports" / "firefox" / "source",
    )
).resolve()
obj_dir = Path(
    os.environ.get(
        "MAKOS_FIREFOX_OBJ_DIR",
        REPO / "build" / "ports" / "firefox" / "obj-aarch64-makos-developer",
    )
).resolve()
autoconf = obj_dir / "config" / "autoconf.mk"
backend = obj_dir / "widget" / "makos" / "backend.mk"
required_paths = (
    source_dir / "widget" / "makos" / "nsPrintSettingsMakOS.cpp",
    source_dir / "widget" / "nsPrintSettingsImpl.h",
    source_dir / "config" / "gcc_hidden.h",
    obj_dir / "mozilla-config.h",
    obj_dir / "dist" / "include" / "nsIPrintSettings.h",
    autoconf,
    backend,
)
missing = [str(path) for path in required_paths if not path.is_file()]
if missing:
    fail("compile evidence requires generated source/object inputs: " + ", ".join(missing))
actual_source = source_dir / "widget" / "makos" / "nsPrintSettingsMakOS.cpp"
if actual_source.read_text() != source_text:
    fail("actual patched print source differs from exact patch payload")

config = autoconf.read_text()
for assignment in ("MOZ_WIDGET_TOOLKIT = makos", "OS_TARGET = MakOS", "TARGET_CPU = aarch64"):
    if assignment not in config:
        fail(f"generated config lacks {assignment!r}")
backend_text = backend.read_text()
backend_entry = r"^CPPSRCS \+= \$\(srcdir\)/nsPrintSettingsMakOS\.cpp$"
if len(re.findall(backend_entry, backend_text, re.MULTILINE)) != 1:
    fail(
        "generated widget backend does not select nsPrintSettingsMakOS.cpp; "
        "run the supported patched mach configure/build before compile evidence"
    )
match = re.search(r"^CXX = (.+)$", config, re.MULTILINE)
if not match:
    fail("generated config does not record CXX")
cxx = shlex.split(match.group(1))
if not cxx or not Path(cxx[0]).is_file():
    fail("generated CXX driver is unavailable")
compile_env = os.environ.copy()
if "MAKOS_REAL_CLANGXX" not in compile_env:
    host_cxx = next(
        (
            tool
            for candidate in (
                "clang++",
                "clang++-19",
                str(
                    Path(cxx[0]).parents[3]
                    / "build"
                    / "host-tools"
                    / "llvm19"
                    / "usr"
                    / "bin"
                    / "clang++-19"
                ),
            )
            if (tool := shutil.which(candidate))
        ),
        None,
    )
    if not host_cxx:
        fail("generated driver requires MAKOS_REAL_CLANGXX or an available clang++")
    compile_env["MAKOS_REAL_CLANGXX"] = host_cxx
    compile_env.setdefault("MAKOS_REAL_CLANG", host_cxx)
if "MAKOS_LLD" not in compile_env:
    generated_root = Path(cxx[0]).parents[3]
    lld = next(
        (
            tool
            for candidate in (
                "ld.lld",
                "ld.lld-19",
                str(generated_root / "build" / "host-tools" / "llvm19" / "usr" / "bin" / "ld.lld-19"),
            )
            if (tool := shutil.which(candidate))
        ),
        None,
    )
    if not lld:
        fail("generated driver requires MAKOS_LLD or an available ld.lld")
    compile_env["MAKOS_LLD"] = lld

nm = next(
    (tool for name in ("llvm-nm", "aarch64-linux-gnu-nm") if (tool := shutil.which(name))),
    None,
)
readelf = next(
    (
        tool
        for name in ("llvm-readelf", "aarch64-linux-gnu-readelf")
        if (tool := shutil.which(name))
    ),
    None,
)
if not nm or not readelf:
    fail("llvm-nm/readelf or AArch64 GNU equivalents are required")

with tempfile.TemporaryDirectory(prefix="makos-firefox-print-") as directory:
    directory_path = Path(directory)
    obj = directory_path / "nsPrintSettingsMakOS.o"
    command = [
        *cxx,
        "-std=gnu++17",
        "-fno-rtti",
        "-fno-exceptions",
        "-fPIC",
        "-DMOZILLA_INTERNAL_API",
        "-DIMPL_LIBXUL",
        "-DMOZ_WIDGET_MAKOS",
        "-include",
        str(source_dir / "config" / "gcc_hidden.h"),
        "-include",
        str(obj_dir / "mozilla-config.h"),
        f"-I{source_dir / 'widget'}",
        f"-I{source_dir / 'widget' / 'makos'}",
        f"-I{obj_dir / 'dist' / 'stl_wrappers'}",
        f"-I{obj_dir / 'dist' / 'system_wrappers'}",
        f"-I{obj_dir / 'dist' / 'include'}",
        f"-I{obj_dir / 'dist' / 'include' / 'nspr'}",
        f"-I{obj_dir / 'dist' / 'include' / 'nss'}",
        "-c",
        str(actual_source),
        "-o",
        str(obj),
    ]
    subprocess.run(command, check=True, env=compile_env)
    header = subprocess.run(
        [readelf, "--file-header", str(obj)],
        check=True,
        text=True,
        stdout=subprocess.PIPE,
    ).stdout
    if not re.search(r"Machine:\s+AArch64", header) or not re.search(
        r"Type:\s+REL ", header
    ):
        fail("compiled result is not an AArch64 ELF relocatable object")
    symbols = subprocess.run(
        [nm, "--demangle", "--defined-only", "--extern-only", str(obj)],
        check=True,
        text=True,
        stdout=subprocess.PIPE,
    ).stdout
    factory = "CreatePlatformPrintSettings(mozilla::PrintSettingsInitializer const&)"
    matches = [line for line in symbols.splitlines() if line.endswith(" " + factory)]
    if len(matches) != 1 or not re.search(r"\sT\s", matches[0]):
        fail("object does not define exactly one global text factory with the exact ABI")
    raw_symbols = subprocess.run(
        [readelf, "--wide", "--symbols", str(obj)],
        check=True,
        text=True,
        stdout=subprocess.PIPE,
    ).stdout
    hidden = [
        line
        for line in raw_symbols.splitlines()
        if "_Z27CreatePlatformPrintSettings" in line
    ]
    if len(hidden) != 1 or not re.search(r"\bGLOBAL\s+HIDDEN\b", hidden[0]):
        fail("factory is not one exact GLOBAL HIDDEN definition")

print(
    "MAKOS_FIREFOX_PRINT_SETTINGS_COMPILE_OK "
    "arch=aarch64 elf=relocatable factory=global-hidden-defined-exact "
    "source=actual-patched-source headers=generated-source+obj backend=generated-selected"
)
