#!/usr/bin/env python3
"""Fail-closed checks for a preserved Firefox object directory."""

from __future__ import annotations

import argparse
import ast
import json
import pathlib
import re
import sys


def exact_assignment(text: str, name: str) -> str:
    values = re.findall(rf"^{name} = (.+)$", text, re.M)
    if len(values) != 1:
        raise ValueError(f"config.status must have one exact {name} assignment")
    value = ast.literal_eval(values[0])
    if not isinstance(value, str) or not value:
        raise ValueError(f"config.status {name} is not a string")
    return value


def recorded_roots(obj: pathlib.Path, source: pathlib.Path) -> list[str]:
    roots: list[str] = []
    status = obj / "config.status"
    if status.is_file():
        status_text = status.read_text()
        roots.append(str(pathlib.Path(exact_assignment(status_text, "topobjdir")).resolve()))
        if pathlib.Path(exact_assignment(status_text, "topsrcdir")).resolve() != source:
            raise ValueError("config.status must have the selected exact topsrcdir")
    mozconfig = obj / ".mozconfig.json"
    if mozconfig.is_file():
        data = json.loads(mozconfig.read_text())
        nested = data.get("mozconfig")
        values = (
            data.get("topobjdir"),
            nested.get("topobjdir") if isinstance(nested, dict) else None,
        )
        if any(not isinstance(value, str) or not value for value in values):
            raise ValueError(".mozconfig.json lacks exact topobjdir fields")
        json_source = data.get("topsrcdir")
        if not isinstance(json_source, str) or pathlib.Path(json_source).resolve() != source:
            raise ValueError(".mozconfig.json lacks the selected exact topsrcdir")
        roots.extend(str(pathlib.Path(value).resolve()) for value in values)
    return roots


def regenerated_metadata_valid(obj: pathlib.Path, source: pathlib.Path) -> bool:
    required = (
        obj / "config" / "autoconf.mk",
        obj / "widget" / "makos" / "backend.mk",
        obj / "backend.RecursiveMakeBackend.in",
        obj / "backend.RecursiveMakeBackend",
    )
    if any(not path.is_file() for path in required):
        return False
    autoconf = required[0].read_text()
    dist_values = re.findall(r"^DIST = (.+)$", autoconf, re.M)
    if len(dist_values) != 1 or pathlib.Path(dist_values[0]).resolve() != obj / "dist":
        return False
    if len(
        re.findall(
            r"^CPPSRCS \+= \$\(srcdir\)/nsPrintSettingsMakOS\.cpp$",
            required[1].read_text(),
            re.M,
        )
    ) != 1:
        return False
    source_entry = str((source / "widget/makos/moz.build").resolve())
    inputs = required[2].read_text().splitlines()
    outputs = required[3].read_text().splitlines()
    return (
        inputs.count(source_entry) == 1
        and outputs.count("widget/makos/Makefile") == 1
        and outputs.count("widget/makos/backend.mk") == 1
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=("needs-configure", "verify"))
    parser.add_argument("obj", type=pathlib.Path)
    parser.add_argument("--source-dir", type=pathlib.Path)
    args = parser.parse_args()
    if args.obj.is_symlink():
        print("Firefox object-directory check blocked: selected objdir is a symlink", file=sys.stderr)
        return 2
    obj = args.obj.resolve()
    source = (args.source_dir or (obj.parent / "source")).resolve()
    status_present = (obj / "config.status").is_file()
    json_present = (obj / ".mozconfig.json").is_file()
    has_state = obj.is_dir() and any(obj.iterdir())
    if not (status_present and json_present):
        if args.mode == "needs-configure":
            return 10 if has_state else 0
        print("Firefox object-directory check blocked: identity metadata incomplete", file=sys.stderr)
        return 1
    try:
        roots = recorded_roots(obj, source)
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"Firefox object-directory check blocked: {error}", file=sys.stderr)
        return 2
    stale = [root for root in roots if root != str(obj)]
    if args.mode == "needs-configure":
        return 10 if stale or (roots and not regenerated_metadata_valid(obj, source)) else 0
    if not roots or stale:
        print(
            f"Firefox object-directory check blocked: selected={obj} recorded={roots}",
            file=sys.stderr,
        )
        return 1
    if not regenerated_metadata_valid(obj, source):
        print("Firefox object-directory check blocked: regenerated build graph invalid", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
