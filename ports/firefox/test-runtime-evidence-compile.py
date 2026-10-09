#!/usr/bin/env python3
"""Compile the actual patched Gecko units without altering source/build caches.

Optional cross-build evidence, not a full release build or guest runtime test.
The generated MakOS configuration and backend flags come from an existing
configured object directory. Only patch0061's new exported header is overlaid.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import struct
import subprocess
import tempfile


PORT = Path(__file__).resolve().parent
PATCH = PORT / "patches/0061-makos-runtime-evidence.patch"
UPSTREAM = "90ad18aabeaa9cbd63a1f749a57f266e758e50da"
UNITS = (
    ("js/src/jit/ExecutableAllocator.cpp", "js/src/jit", b"MAKOS_JIT_POOL_OK"),
    ("gfx/layers/wr/WebRenderLayerManager.cpp", "gfx/layers", b"MAKOS_PRES_PAINT uri="),
)


def assignment(text: str, name: str, operator: str) -> list[str]:
    matches = re.findall(rf"^{re.escape(name)} {re.escape(operator)} (.+)$", text, re.M)
    if len(matches) != 1:
        raise ValueError(f"expected one generated {name} {operator} assignment")
    value = shlex.split(matches[0])
    if any("$(" in part or "${" in part for part in value):
        raise ValueError(f"unexpanded generated {name}")
    return value


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-dir", required=True, type=Path)
    parser.add_argument("--obj-dir", required=True, type=Path)
    parser.add_argument("--evidence-parent", required=True, type=Path)
    args = parser.parse_args()
    source = args.source_dir.resolve(strict=True)
    obj = args.obj_dir.resolve(strict=True)
    parent = args.evidence_parent.resolve(strict=True)
    if any(parent == protected or protected in parent.parents for protected in (source, obj)):
        raise ValueError("evidence parent must be outside source and object caches")
    config = (obj / "config/autoconf.mk").read_text()
    for expected in ("MOZ_WIDGET_TOOLKIT = makos", "OS_TARGET = MakOS", "TARGET_CPU = aarch64"):
        if expected not in config:
            raise ValueError(f"configuration lacks {expected}")
    driver = assignment(config, "CXX", "=")
    if not driver or not Path(driver[0]).is_file():
        raise ValueError("generated CXX driver is unavailable")
    if not any("aarch64-unknown-makos" in part for part in driver):
        raise ValueError("generated CXX is not the distinct MakOS target")
    if "#define XP_MAKOS 1" not in (obj / "mozilla-config.h").read_text():
        raise ValueError("generated headers do not enable XP_MAKOS")
    inputs = ["mfbt/moz.build", *(path for path, _, _ in UNITS)]
    for earlier in sorted(PORT.glob("patches/*.patch")):
        if earlier == PATCH:
            continue
        patch = earlier.read_text()
        if any(f"+++ b/{path}\n" in patch for path in inputs):
            raise ValueError(f"compile input also changed by {earlier.name}; extend exact replay")

    # Retain all generated evidence even on failure. Never write into the source
    # checkout or configured object directory, and never replace old evidence.
    evidence = Path(tempfile.mkdtemp(prefix="gecko-units-", dir=parent))
    print(f"Firefox evidence compile directory: {evidence}", flush=True)
    private_source = evidence / "source"
    private_source.mkdir()
    subprocess.run(["git", "init", "-q", str(private_source)], check=True)
    for path in inputs:
        content = subprocess.check_output(["git", "-C", str(source), "show", f"{UPSTREAM}:{path}"])
        target = private_source / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(content)
    subprocess.run(["git", "apply", "--check", str(PATCH)], cwd=private_source, check=True)
    subprocess.run(["git", "apply", str(PATCH)], cwd=private_source, check=True)
    include = evidence / "include/mozilla"
    include.mkdir(parents=True)
    shutil.copyfile(private_source / "mfbt/MakOSRuntimeEvidence.h", include / "MakOSRuntimeEvidence.h")
    records = []
    for path, backend_directory, marker in UNITS:
        backend = obj / backend_directory / "backend.mk"
        flags = assignment(backend.read_text(), "COMPUTED_CXXFLAGS", "+=")
        output = evidence / (Path(path).stem + ".o")
        # Preserve quoted-header lookup next to the original translation unit
        # after moving only its source bytes into this private build tree.
        command = [*driver, f"-I{include.parent}", f"-iquote{(source / path).parent}",
                   *flags, "-std=gnu++17", "-c",
                   str(private_source / path), "-o", str(output)]
        record = {"source": path, "backend": str(backend), "command": command,
                  "patch_sha256": hashlib.sha256(PATCH.read_bytes()).hexdigest()}
        (evidence / (output.stem + ".command.json")).write_text(json.dumps(record, indent=2) + "\n")
        print(shlex.join(command), flush=True)
        subprocess.run(command, cwd=obj / backend_directory, env=os.environ.copy(), check=True)
        content = output.read_bytes()
        if content[:6] != b"\x7fELF\x02\x01" or struct.unpack_from("<HH", content, 16) != (1, 183):
            raise ValueError(f"not an AArch64 ELF relocatable object: {output}")
        if marker not in content:
            raise ValueError(f"actual compiled object lacks runtime emitter: {output}")
        record.update(object=str(output), bytes=len(content), sha256=hashlib.sha256(content).hexdigest())
        records.append(record)
    (evidence / "results.json").write_text(json.dumps(records, indent=2) + "\n")
    print("MAKOS_FIREFOX_RUNTIME_EVIDENCE_COMPILE_OK "
          "target=aarch64-unknown-makos units=ExecutableAllocator,WebRenderLayerManager "
          "headers=generated backend=generated source=private-pinned+patch0061 "
          "elf=relocatable release=not-built runtime=not-tested")


if __name__ == "__main__":
    main()
