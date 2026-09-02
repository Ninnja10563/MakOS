#!/usr/bin/env python3
"""Behavioral fixtures for moved Firefox object-directory detection."""

import json
import pathlib
import subprocess
import sys
import tempfile

SCRIPT = pathlib.Path(__file__).with_name("firefox_objdir.py")


def run(mode: str, obj: pathlib.Path) -> int:
    return subprocess.run([sys.executable, str(SCRIPT), mode, str(obj)], check=False).returncode


with tempfile.TemporaryDirectory() as directory:
    root = pathlib.Path(directory) / "repo ;$[] with spaces"
    root.mkdir()
    obj = root / "obj-aarch64-makos-developer"
    obj.mkdir()
    stale = root / "obj-aarch64-makos"
    source = root / "source"
    (obj / "config.status").write_text(
        f"topobjdir = {str(stale)!r}\ntopsrcdir = {str(source)!r}\n"
    )
    (obj / ".mozconfig.json").write_text(
        json.dumps(
            {
                "topobjdir": str(stale),
                "topsrcdir": str(source),
                "mozconfig": {"topobjdir": str(stale)},
            }
        )
    )
    assert run("needs-configure", obj) == 10
    assert run("verify", obj) == 1
    (obj / "config.status").write_text(
        f"topobjdir = {str(obj)!r}\ntopsrcdir = {str(source)!r}\n"
    )
    (obj / ".mozconfig.json").write_text(
        json.dumps(
            {
                "topobjdir": str(obj),
                "topsrcdir": str(source),
                "mozconfig": {"topobjdir": str(obj)},
            }
        )
    )
    assert run("needs-configure", obj) == 10
    (obj / "config").mkdir()
    (obj / "config" / "autoconf.mk").write_text(f"DIST = {obj}/dist\n")
    (obj / "widget" / "makos").mkdir(parents=True)
    (obj / "widget" / "makos" / "backend.mk").write_text(
        "CPPSRCS += $(srcdir)/nsPrintSettingsMakOS.cpp\n"
    )
    (obj / "backend.RecursiveMakeBackend.in").write_text(
        str((root / "source/widget/makos/moz.build").resolve()) + "\n"
    )
    (obj / "backend.RecursiveMakeBackend").write_text(
        "widget/makos/Makefile\nwidget/makos/backend.mk\n"
    )
    assert run("needs-configure", obj) == 0
    assert run("verify", obj) == 0
    (obj / "config.status").unlink()
    assert run("needs-configure", obj) == 10
    assert run("verify", obj) == 1
    (obj / "config.status").write_text(
        f"topobjdir = {str(obj)!r}\ntopsrcdir = {str(source)!r}\n"
    )
    (obj / ".mozconfig.json").unlink()
    assert run("needs-configure", obj) == 10
    assert run("verify", obj) == 1
    (obj / ".mozconfig.json").write_text(
        json.dumps(
            {
                "topobjdir": str(obj),
                "topsrcdir": str(source),
                "mozconfig": {"topobjdir": str(obj)},
            }
        )
    )
    (obj / "config.status").write_text(
        f"topobjdir = {str(obj)!r}\ntopobjdir = {str(obj)!r}\n"
        f"topsrcdir = {str(source)!r}\n"
    )
    assert run("needs-configure", obj) == 2
    (obj / "config.status").write_text(
        f"topobjdir = {str(obj)!r}\ntopsrcdir = {str(source)!r}\n"
    )
    (obj / ".mozconfig.json").write_text("{malformed")
    assert run("needs-configure", obj) == 2
    link = root / "linked"
    link.symlink_to(obj, target_is_directory=True)
    assert run("needs-configure", link) == 2
    assert not (root / "SENTINEL").exists()
    empty = root / "empty-obj"
    empty.mkdir()
    assert run("needs-configure", empty) == 0
    assert run("verify", empty) == 1
print("MAKOS_FIREFOX_OBJDIR_TEST_OK moved=configure-required current=accepted")
