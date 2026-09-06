#!/usr/bin/env python3
"""Apply the pinned target patch, including recovery from old partial runs."""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


def apply(source: Path) -> None:
    patch_path = Path(__file__).with_name("patches") / "0001-makos-target.patch"
    sections = patch_path.read_bytes().split(b"diff --git ")[1:]
    expected = ("config.sub", "configure.ac", "configure")
    if len(sections) != len(expected):
        raise RuntimeError("unexpected CPython target patch layout")
    # Work only on copies until every file has passed a complete forward or
    # reverse check. Existing local edits and old .rej/.orig files are preserved.
    with tempfile.TemporaryDirectory(prefix=".makos-patch-", dir=source) as tmp:
        staging = Path(tmp)
        originals = {}
        for name, section in zip(expected, sections):
            if not section.startswith(f"a/{name} b/{name}\n".encode()):
                raise RuntimeError("unexpected CPython target patch file")
            path = source / name
            if path.is_symlink() or not path.is_file():
                raise RuntimeError(f"CPython source is not a regular file: {path}")
            originals[name] = path.read_bytes()
            shutil.copy2(path, staging / name)
            patch = b"diff --git " + section

            def run(*flags: str) -> subprocess.CompletedProcess:
                return subprocess.run(
                    # --force fixes direction: --batch may silently ignore -R
                    # when it recognizes an unpatched file.
                    ["patch", "--force", "--fuzz=0", "-p1", *flags],
                    cwd=staging, input=patch, stdout=subprocess.PIPE,
                    stderr=subprocess.STDOUT,
                )

            if run("--dry-run", "--reverse").returncode == 0:
                continue
            result = run("--dry-run", "--forward")
            if result.returncode:
                raise RuntimeError(
                    f"CPython target patch does not match {name}; source unchanged\n"
                    + result.stdout.decode(errors="replace")
                )
            result = run("--forward")
            if result.returncode or run("--dry-run", "--reverse").returncode:
                raise RuntimeError(f"CPython target patch verification failed: {name}")
        result = subprocess.run(
            ["sh", str(staging / "config.sub"), "aarch64-unknown-makos"],
            check=True, text=True, stdout=subprocess.PIPE,
        )
        if result.stdout.strip() != "aarch64-unknown-makos":
            raise RuntimeError("CPython config.sub did not preserve MakOS identity")
        for name in expected:
            if (source / name).read_bytes() != originals[name]:
                raise RuntimeError("CPython source changed during patching; retry")
        for name in expected:
            if (staging / name).read_bytes() != originals[name]:
                os.replace(staging / name, source / name)
    # Publication is atomic per file, not across the three files. An interrupted
    # publication is recoverable by this same forward/reverse-verified path.
    print("MAKOS_CPYTHON_PATCHES_OK target=aarch64-unknown-makos")


if __name__ == "__main__":
    try:
        apply(Path(sys.argv[1]).resolve(strict=True))
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        sys.exit(str(error))
