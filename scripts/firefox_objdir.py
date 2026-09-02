#!/usr/bin/env python3
"""Fail-closed checks for a preserved Firefox object directory."""

from __future__ import annotations

import argparse
import ast
import json
import os
import pathlib
import re
import stat
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


def fsync_directory(path: pathlib.Path) -> None:
    flags = os.O_RDONLY
    if hasattr(os, "O_DIRECTORY"):
        flags |= os.O_DIRECTORY
    try:
        descriptor = os.open(path, flags)
        try:
            os.fsync(descriptor)
        finally:
            os.close(descriptor)
    except OSError as error:
        raise OSError(
            error.errno,
            f"required directory fsync failed for {path}: {error.strerror}",
        ) from error


def validate_exact_journal_file(
    path: pathlib.Path, payload: bytes, links: tuple[int, ...]
) -> os.stat_result:
    if not hasattr(os, "O_NOFOLLOW"):
        raise OSError("migration journal validation requires O_NOFOLLOW")
    flags = os.O_RDONLY | os.O_NOFOLLOW
    descriptor = os.open(path, flags)
    try:
        metadata = os.fstat(descriptor)
        if (
            not stat.S_ISREG(metadata.st_mode)
            or stat.S_IMODE(metadata.st_mode) != 0o600
            or metadata.st_uid != os.geteuid()
            or metadata.st_nlink not in links
        ):
            raise OSError(f"migration journal file is not exact: {path}")
        with os.fdopen(descriptor, "rb", closefd=False) as input_file:
            if input_file.read() != payload:
                raise OSError(f"migration journal file is not exact: {path}")
        os.fsync(descriptor)
        return metadata
    finally:
        os.close(descriptor)


def install_exact_journal_temp(
    temporary: pathlib.Path,
    journal: pathlib.Path,
    quarantine: pathlib.Path,
    payload: bytes,
) -> None:
    if temporary.is_symlink():
        raise OSError(f"migration journal temporary is a symlink: {temporary}")
    validate_exact_journal_file(temporary, payload, (1,))
    os.link(temporary, journal, follow_symlinks=False)
    os.unlink(temporary)
    fsync_directory(quarantine)
    validate_exact_journal_file(journal, payload, (1,))


def recover_linked_journal_temp(
    temporary: pathlib.Path,
    journal: pathlib.Path,
    quarantine: pathlib.Path,
    payload: bytes,
) -> None:
    if temporary.is_symlink() or journal.is_symlink():
        raise OSError("migration journal recovery path is a symlink")
    temporary_metadata = validate_exact_journal_file(temporary, payload, (2,))
    journal_metadata = validate_exact_journal_file(journal, payload, (2,))
    if (temporary_metadata.st_dev, temporary_metadata.st_ino) != (
        journal_metadata.st_dev,
        journal_metadata.st_ino,
    ):
        raise OSError("migration journal recovery files are not the same inode")
    os.unlink(temporary)
    fsync_directory(quarantine)
    validate_exact_journal_file(journal, payload, (1,))


def quarantine_moved_cargo(obj: pathlib.Path, source: pathlib.Path) -> int:
    """Recoverably isolate Cargo trees whose dep-info embeds the old objdir."""
    try:
        roots = recorded_roots(obj, source)
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"Firefox object-directory check blocked: {error}", file=sys.stderr)
        return 2
    if not roots:
        print("Firefox Cargo cache quarantine blocked: identity metadata absent", file=sys.stderr)
        return 2
    stale = {pathlib.Path(root) for root in roots if root != str(obj)}
    if not stale:
        print("MAKOS_FIREFOX_CARGO_QUARANTINE_OK moved=0 reason=current-object-directory")
        return 0
    expected_old = (obj.parent / "obj-aarch64-makos").resolve()
    if obj.name != "obj-aarch64-makos-developer" or stale != {expected_old}:
        print(
            f"Firefox Cargo cache quarantine blocked: selected={obj} stale={sorted(map(str, stale))}",
            file=sys.stderr,
        )
        return 2

    quarantine = obj / "makos-moved-cargo-quarantine"
    if quarantine.is_symlink() or (quarantine.exists() and not quarantine.is_dir()):
        print("Firefox Cargo cache quarantine blocked: quarantine path is unsafe", file=sys.stderr)
        return 2
    candidates = (
        (obj / "release", quarantine / "host-release"),
        (obj / "aarch64-unknown-makos" / "release", quarantine / "target-release"),
    )
    journal = quarantine / "migration.json"
    expected_journal = {
        "version": 1,
        "selected_objdir": str(obj),
        "old_objdir": str(expected_old),
        "mappings": [
            {"source": str(source_path), "destination": str(destination)}
            for source_path, destination in candidates
        ],
    }
    payload = (json.dumps(expected_journal, sort_keys=True, separators=(",", ":")) + "\n").encode()
    temporary = quarantine / ".migration.json.tmp"
    journal_valid = False
    journal_present = journal.exists() or journal.is_symlink()
    temporary_present = temporary.exists() or temporary.is_symlink()
    if journal_present and temporary_present:
        try:
            recover_linked_journal_temp(temporary, journal, quarantine, payload)
            journal_valid = True
        except OSError as error:
            print(f"Firefox Cargo cache quarantine blocked: {error}", file=sys.stderr)
            return 2
    elif journal_present:
        try:
            if journal.is_symlink():
                raise OSError(f"migration journal is a symlink: {journal}")
            validate_exact_journal_file(journal, payload, (1,))
            journal_valid = True
        except OSError as error:
            print(f"Firefox Cargo cache quarantine blocked: {error}", file=sys.stderr)
            return 2
    elif temporary_present:
        try:
            install_exact_journal_temp(temporary, journal, quarantine, payload)
            journal_valid = True
        except OSError as error:
            print(f"Firefox Cargo cache quarantine blocked: {error}", file=sys.stderr)
            return 2
    for source_path, destination in candidates:
        if source_path.parent.is_symlink() or source_path.is_symlink():
            print(
                f"Firefox Cargo cache quarantine blocked: cache path is a symlink: {source_path}",
                file=sys.stderr,
            )
            return 2
        if source_path.exists() and not source_path.is_dir():
            print(
                f"Firefox Cargo cache quarantine blocked: cache path is not a directory: {source_path}",
                file=sys.stderr,
            )
            return 2
        if source_path.exists() and destination.exists():
            print(
                f"Firefox Cargo cache quarantine blocked: source and destination both exist: {source_path}",
                file=sys.stderr,
            )
            return 2
        if destination.is_symlink():
            print(
                f"Firefox Cargo cache quarantine blocked: destination is a symlink: {destination}",
                file=sys.stderr,
            )
            return 2
        if destination.exists() and not destination.is_dir():
            print(
                f"Firefox Cargo cache quarantine blocked: destination is not a directory: {destination}",
                file=sys.stderr,
            )
            return 2
        if destination.exists() and not journal_valid:
            print(
                f"Firefox Cargo cache quarantine blocked: destination lacks valid journal: {destination}",
                file=sys.stderr,
            )
            return 2

    moved = 0
    try:
        quarantine.mkdir(mode=0o700, exist_ok=True)
        if not journal_valid:
            flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL
            if hasattr(os, "O_NOFOLLOW"):
                flags |= os.O_NOFOLLOW
            descriptor = os.open(temporary, flags, 0o600)
            try:
                with os.fdopen(descriptor, "wb", closefd=False) as output:
                    output.write(payload)
                    output.flush()
                    os.fsync(output.fileno())
            finally:
                os.close(descriptor)
            install_exact_journal_temp(temporary, journal, quarantine, payload)
            journal_valid = True
        # Persist the quarantine directory entry in the selected objdir before
        # any Cargo tree can move underneath it. Fsyncing only the quarantine
        # itself does not make its name durable in the parent directory.
        fsync_directory(obj)
        for source_path, destination in candidates:
            if source_path.is_dir():
                source_path.rename(destination)
                moved += 1
            if source_path.parent.is_dir():
                fsync_directory(source_path.parent)
            fsync_directory(destination.parent)
    except OSError as error:
        print(f"Firefox Cargo cache quarantine blocked: {error}", file=sys.stderr)
        return 2
    print(
        "MAKOS_FIREFOX_CARGO_QUARANTINE_OK "
        f"moved={moved} old={expected_old} selected={obj} journal={journal} recoverable={quarantine}"
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "mode", choices=("needs-configure", "quarantine-moved-cargo", "verify")
    )
    parser.add_argument("obj", type=pathlib.Path)
    parser.add_argument("--source-dir", type=pathlib.Path)
    args = parser.parse_args()
    if args.obj.is_symlink():
        print("Firefox object-directory check blocked: selected objdir is a symlink", file=sys.stderr)
        return 2
    obj = args.obj.resolve()
    source = (args.source_dir or (obj.parent / "source")).resolve()
    if args.mode == "quarantine-moved-cargo":
        return quarantine_moved_cargo(obj, source)
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
