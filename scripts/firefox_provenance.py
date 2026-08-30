#!/usr/bin/env python3
"""Bind staged Firefox artifacts to the pinned source and ordered patch set."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
import stat
import subprocess
import sys
import tempfile
from typing import Any


ROOT = pathlib.Path(__file__).resolve().parent.parent
SCHEMA = 2
TARGET = "aarch64-unknown-makos"
BUILD_ARTIFACTS = (
    "firefox",
    "plugin-container",
    "xpcshell",
    "libxul.so",
    "libnspr4.so",
)
RUNTIME_ARTIFACTS = BUILD_ARTIFACTS
GUEST_PATH = "/usr/lib/firefox/makos-build-provenance.json"
MAX_RECORD_BYTES = 4096


def sha256_path(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while block := source.read(1024 * 1024):
            digest.update(block)
    return digest.hexdigest()


def lock_values(root: pathlib.Path = ROOT) -> dict[str, str]:
    result: dict[str, str] = {}
    for line in (root / "ports/firefox/source.lock").read_text().splitlines():
        key, separator, value = line.partition("=")
        if separator:
            result[key] = value
    for key in ("FIREFOX_VERSION", "FIREFOX_COMMIT"):
        if not result.get(key):
            raise ValueError(f"{key} absent from Firefox source lock")
    return result


def patch_series_identity(root: pathlib.Path = ROOT) -> tuple[int, str]:
    patches = sorted((root / "ports/firefox/patches").glob("*.patch"))
    if not patches:
        raise ValueError("Firefox patch series is empty")
    series = hashlib.sha256()
    for patch in patches:
        series.update(sha256_path(patch).encode("ascii"))
        series.update(b"\n")
    return len(patches), series.hexdigest()


def expected_identity(root: pathlib.Path = ROOT) -> dict[str, Any]:
    locked = lock_values(root)
    source_commit = locked["FIREFOX_COMMIT"]
    if len(source_commit) != 40 or any(
        character not in "0123456789abcdef" for character in source_commit
    ):
        raise ValueError("Firefox source lock commit is not a full lowercase SHA-1")
    patch_count, patch_sha256 = patch_series_identity(root)
    return {
        "schema": SCHEMA,
        "target": TARGET,
        "source_version": locked["FIREFOX_VERSION"],
        "source_commit": source_commit,
        "patch_count": patch_count,
        "patch_series_sha256": patch_sha256,
    }


def artifact_hashes(bin_dir: pathlib.Path) -> dict[str, str]:
    result: dict[str, str] = {}
    for name in BUILD_ARTIFACTS:
        path = bin_dir / name
        if not path.is_file():
            raise FileNotFoundError(f"Firefox build artifact absent: {path}")
        result[name] = sha256_path(path)
    return result


def build_record(
    bin_dir: pathlib.Path, source_tree: str, root: pathlib.Path = ROOT
) -> dict[str, Any]:
    return {
        **expected_identity(root),
        "source_tree": source_tree,
        "build_artifacts": artifact_hashes(bin_dir),
    }


def runtime_artifact_hashes(
    package_dir: pathlib.Path, stripped_libxul: pathlib.Path
) -> dict[str, str]:
    result: dict[str, str] = {}
    for name in RUNTIME_ARTIFACTS:
        path = stripped_libxul if name == "libxul.so" else package_dir / name
        if not path.is_file():
            raise FileNotFoundError(f"Firefox runtime artifact absent: {path}")
        result[name] = sha256_path(path)
    return result


def runtime_record(
    build_stamp: pathlib.Path,
    bin_dir: pathlib.Path,
    package_dir: pathlib.Path,
    stripped_libxul: pathlib.Path,
    root: pathlib.Path = ROOT,
) -> dict[str, Any]:
    record = verify_build_stamp(build_stamp, bin_dir, root)
    return {
        **record,
        "runtime_artifacts": runtime_artifact_hashes(package_dir, stripped_libxul),
    }


def canonical_bytes(record: dict[str, Any]) -> bytes:
    encoded = json.dumps(record, sort_keys=True, separators=(",", ":")).encode()
    if len(encoded) + 1 > MAX_RECORD_BYTES:
        raise ValueError("Firefox provenance record exceeds bounded size")
    return encoded + b"\n"


def write_record(path: pathlib.Path, record: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.tmp")
    temporary.write_bytes(canonical_bytes(record))
    os.replace(temporary, path)


def parse_record(data: bytes) -> dict[str, Any]:
    if not data or len(data) > MAX_RECORD_BYTES or not data.endswith(b"\n"):
        raise ValueError("Firefox provenance record framing invalid")
    try:
        record = json.loads(data)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ValueError("Firefox provenance record is not valid JSON") from error
    if not isinstance(record, dict) or canonical_bytes(record) != data:
        raise ValueError("Firefox provenance record is not canonical")
    return record


def validate_build_record(
    record: dict[str, Any], root: pathlib.Path = ROOT
) -> dict[str, str]:
    expected = expected_identity(root)
    if set(record) != {*expected, "source_tree", "build_artifacts"}:
        raise ValueError("Firefox provenance record fields invalid")
    observed_identity = {key: record.get(key) for key in expected}
    if observed_identity != expected:
        raise ValueError(
            "Firefox provenance identity mismatch: "
            f"expected={expected} observed={observed_identity}"
        )
    source_tree = record.get("source_tree")
    if (
        not isinstance(source_tree, str)
        or len(source_tree) not in {40, 64}
        or any(character not in "0123456789abcdef" for character in source_tree)
    ):
        raise ValueError("Firefox provenance source tree invalid")
    artifacts = record.get("build_artifacts")
    if not isinstance(artifacts, dict) or set(artifacts) != set(BUILD_ARTIFACTS):
        raise ValueError("Firefox provenance artifact set invalid")
    for name, digest in artifacts.items():
        if (
            not isinstance(name, str)
            or not isinstance(digest, str)
            or len(digest) != 64
            or any(character not in "0123456789abcdef" for character in digest)
        ):
            raise ValueError("Firefox provenance artifact digest invalid")
    return artifacts


def validate_runtime_record(
    record: dict[str, Any], root: pathlib.Path = ROOT
) -> dict[str, str]:
    build_record_fields = set(expected_identity(root)) | {
        "source_tree",
        "build_artifacts",
    }
    if set(record) != build_record_fields | {"runtime_artifacts"}:
        raise ValueError("Firefox runtime provenance record fields invalid")
    build = {key: record[key] for key in build_record_fields}
    validate_build_record(build, root)
    artifacts = record.get("runtime_artifacts")
    if not isinstance(artifacts, dict) or set(artifacts) != set(RUNTIME_ARTIFACTS):
        raise ValueError("Firefox runtime provenance artifact set invalid")
    for digest in artifacts.values():
        if (
            not isinstance(digest, str)
            or len(digest) != 64
            or any(character not in "0123456789abcdef" for character in digest)
        ):
            raise ValueError("Firefox runtime provenance artifact digest invalid")
    return artifacts


def read_build_record(path: pathlib.Path, root: pathlib.Path = ROOT) -> dict[str, Any]:
    record = parse_record(path.read_bytes())
    validate_build_record(record, root)
    return record


def read_runtime_record(path: pathlib.Path, root: pathlib.Path = ROOT) -> dict[str, Any]:
    record = parse_record(path.read_bytes())
    validate_runtime_record(record, root)
    return record


def verify_build_stamp(
    stamp: pathlib.Path, bin_dir: pathlib.Path, root: pathlib.Path = ROOT
) -> dict[str, Any]:
    record = read_build_record(stamp, root)
    if record["build_artifacts"] != artifact_hashes(bin_dir):
        raise ValueError("Firefox build artifacts differ from audited build stamp")
    return record


def verify_record_source_tree(record: dict[str, Any], source_tree: str) -> None:
    if record.get("source_tree") != source_tree:
        raise ValueError(
            "Firefox provenance source tree differs from verified patched tree"
        )


def source_git_dir(source_dir: pathlib.Path) -> pathlib.Path:
    output = subprocess.check_output(
        ["git", "-C", str(source_dir), "rev-parse", "--absolute-git-dir"],
        text=True,
    ).strip()
    return pathlib.Path(output)


def git_output(
    source_dir: pathlib.Path,
    *arguments: str,
    environment: dict[str, str] | None = None,
) -> bytes:
    result = subprocess.run(
        ["git", "-C", str(source_dir), *arguments],
        env=environment,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        detail = result.stderr.decode(errors="replace").strip()
        raise ValueError(
            f"Firefox source tree verification command failed: git {' '.join(arguments)}"
            + (f": {detail}" if detail else "")
        )
    return result.stdout


def display_git_path(path: bytes) -> str:
    return os.fsdecode(path).encode("unicode_escape").decode("ascii")


def hash_git_blob(stream, size: int, algorithm: str) -> str:
    digest = hashlib.new(algorithm)
    digest.update(f"blob {size}\0".encode("ascii"))
    while block := stream.read(1024 * 1024):
        digest.update(block)
    return digest.hexdigest()


def hash_memory_git_blob(payload: bytes, algorithm: str) -> str:
    digest = hashlib.new(algorithm)
    digest.update(f"blob {len(payload)}\0".encode("ascii"))
    digest.update(payload)
    return digest.hexdigest()


def checked_relative_path(raw_path: bytes) -> pathlib.PurePosixPath:
    relative = pathlib.PurePosixPath(os.fsdecode(raw_path))
    if (
        relative.is_absolute()
        or not relative.parts
        or ".." in relative.parts
        or relative.parts[0] == ".git"
    ):
        raise ValueError("Firefox expected patched tree path is unsafe")
    return relative


def verify_parent_directories(
    source_dir: pathlib.Path,
    relative: pathlib.PurePosixPath,
    verified: set[pathlib.PurePosixPath],
) -> None:
    current = pathlib.PurePosixPath()
    for part in relative.parts[:-1]:
        current /= part
        if current in verified:
            continue
        try:
            metadata = (source_dir / pathlib.Path(*current.parts)).lstat()
        except FileNotFoundError as error:
            raise ValueError(
                "Firefox source tree differs from ordered patches: "
                f"missing directory {current}"
            ) from error
        if not stat.S_ISDIR(metadata.st_mode):
            raise ValueError(
                "Firefox source tree differs from ordered patches: "
                f"directory type {current}"
            )
        verified.add(current)


def verify_expected_entry(
    source_dir: pathlib.Path,
    raw_path: bytes,
    mode: bytes,
    expected_oid: bytes,
    algorithm: str,
    verified_directories: set[pathlib.PurePosixPath],
) -> None:
    relative = checked_relative_path(raw_path)
    verify_parent_directories(source_dir, relative, verified_directories)
    path = source_dir / pathlib.Path(*relative.parts)
    shown = display_git_path(raw_path)
    try:
        initial = path.lstat()
    except FileNotFoundError as error:
        raise ValueError(
            f"Firefox source tree differs from ordered patches: missing {shown}"
        ) from error

    if mode in {b"100644", b"100755"}:
        if not stat.S_ISREG(initial.st_mode):
            raise ValueError(
                f"Firefox source tree differs from ordered patches: type {shown}"
            )
        if bool(initial.st_mode & 0o111) != (mode == b"100755"):
            raise ValueError(
                f"Firefox source tree differs from ordered patches: mode {shown}"
            )
        descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
        try:
            with os.fdopen(descriptor, "rb", closefd=False) as stream:
                opened = os.fstat(descriptor)
                if (opened.st_dev, opened.st_ino) != (initial.st_dev, initial.st_ino):
                    raise ValueError(
                        f"Firefox source tree changed during verification: {shown}"
                    )
                observed_oid = hash_git_blob(stream, opened.st_size, algorithm)
                final_descriptor = os.fstat(descriptor)
        finally:
            os.close(descriptor)
        final_path = path.lstat()
        initial_identity = (
            initial.st_dev,
            initial.st_ino,
            initial.st_mode,
            initial.st_size,
            initial.st_mtime_ns,
        )
        if initial_identity != (
            final_descriptor.st_dev,
            final_descriptor.st_ino,
            final_descriptor.st_mode,
            final_descriptor.st_size,
            final_descriptor.st_mtime_ns,
        ) or initial_identity != (
            final_path.st_dev,
            final_path.st_ino,
            final_path.st_mode,
            final_path.st_size,
            final_path.st_mtime_ns,
        ):
            raise ValueError(f"Firefox source tree changed during verification: {shown}")
    elif mode == b"120000":
        if not stat.S_ISLNK(initial.st_mode):
            raise ValueError(
                f"Firefox source tree differs from ordered patches: type {shown}"
            )
        target = os.fsencode(os.readlink(path))
        observed_oid = hash_memory_git_blob(target, algorithm)
        final_path = path.lstat()
        if (
            initial.st_dev,
            initial.st_ino,
            initial.st_mode,
            initial.st_mtime_ns,
        ) != (
            final_path.st_dev,
            final_path.st_ino,
            final_path.st_mode,
            final_path.st_mtime_ns,
        ) or target != os.fsencode(os.readlink(path)):
            raise ValueError(f"Firefox source tree changed during verification: {shown}")
    else:
        raise ValueError(
            f"Firefox expected patched tree mode unsupported: {mode.decode()} {shown}"
        )
    if observed_oid.encode("ascii") != expected_oid:
        raise ValueError(
            f"Firefox source tree differs from ordered patches: content {shown}"
        )


def verify_expected_patched_tree(
    source_dir: pathlib.Path, root: pathlib.Path = ROOT
) -> str:
    """Reconstruct and compare the exact tracked tree without touching its index."""
    expected = expected_identity(root)
    with tempfile.TemporaryDirectory(prefix="makos-firefox-source-index-") as name:
        temporary = pathlib.Path(name)
        index = temporary / "expected.index"
        objects = temporary / "objects"
        objects.mkdir()
        real_objects = pathlib.Path(
            os.fsdecode(
                git_output(source_dir, "rev-parse", "--git-path", "objects")
            ).strip()
        )
        if not real_objects.is_absolute():
            real_objects = (source_dir / real_objects).resolve()
        environment = os.environ.copy()
        environment["GIT_INDEX_FILE"] = str(index)
        # Applying into a temporary index normally writes reconstructed blobs
        # into the real repository. Keep both index and newly generated objects
        # isolated while reading the pinned commit through an alternate.
        environment["GIT_OBJECT_DIRECTORY"] = str(objects)
        environment["GIT_ALTERNATE_OBJECT_DIRECTORIES"] = str(real_objects)
        git_output(source_dir, "read-tree", expected["source_commit"], environment=environment)
        for patch in sorted((root / "ports/firefox/patches").glob("*.patch")):
            git_output(
                source_dir,
                "apply",
                "--cached",
                "--whitespace=nowarn",
                str(patch),
                environment=environment,
            )
        expected_tree = git_output(
            source_dir, "write-tree", environment=environment
        ).decode("ascii").strip()
        object_format = git_output(
            source_dir, "rev-parse", "--show-object-format"
        ).decode("ascii").strip()
        if object_format not in {"sha1", "sha256"}:
            raise ValueError(
                f"Firefox source Git object format unsupported: {object_format}"
            )
        unexpected = [
            path
            for path in git_output(
                source_dir,
                "ls-files",
                "--others",
                "--exclude-standard",
                "-z",
                environment=environment,
            ).split(b"\0")
            if path
        ]
        if unexpected:
            raise ValueError(
                "Firefox source tree has unexpected untracked path: "
                + display_git_path(unexpected[0])
            )

        verified_directories: set[pathlib.PurePosixPath] = set()
        entries = git_output(
            source_dir, "ls-files", "--stage", "-z", environment=environment
        )
        for record in entries.split(b"\0"):
            if not record:
                continue
            metadata, separator, raw_path = record.partition(b"\t")
            fields = metadata.split()
            if not separator or len(fields) != 3 or fields[2] != b"0":
                raise ValueError("Firefox expected patched tree index is malformed")
            verify_expected_entry(
                source_dir,
                raw_path,
                fields[0],
                fields[1],
                object_format,
                verified_directories,
            )
        return expected_tree


def verify_source_state(source_dir: pathlib.Path, root: pathlib.Path = ROOT) -> str:
    expected = expected_identity(root)
    commit = subprocess.check_output(
        ["git", "-C", str(source_dir), "rev-parse", "HEAD"], text=True
    ).strip()
    if commit != expected["source_commit"]:
        raise ValueError(
            f"Firefox source commit mismatch: expected={expected['source_commit']} observed={commit}"
        )
    marker = source_git_dir(source_dir) / "makos-patches.sha256"
    applied = marker.read_text().splitlines()
    if applied != [expected["patch_series_sha256"]]:
        raise ValueError(
            "Firefox applied-patch marker mismatch: "
            f"expected={expected['patch_series_sha256']} observed={applied}"
        )
    return verify_expected_patched_tree(source_dir, root)


def main() -> int:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    create = subparsers.add_parser("create-build-stamp")
    create.add_argument("--source-dir", type=pathlib.Path, required=True)
    create.add_argument("--bin-dir", type=pathlib.Path, required=True)
    create.add_argument("--output", type=pathlib.Path, required=True)
    verify = subparsers.add_parser("verify-build-stamp")
    verify.add_argument("--source-dir", type=pathlib.Path, required=True)
    verify.add_argument("--bin-dir", type=pathlib.Path, required=True)
    verify.add_argument("--stamp", type=pathlib.Path, required=True)
    runtime = subparsers.add_parser("create-runtime-record")
    runtime.add_argument("--source-dir", type=pathlib.Path, required=True)
    runtime.add_argument("--bin-dir", type=pathlib.Path, required=True)
    runtime.add_argument("--stamp", type=pathlib.Path, required=True)
    runtime.add_argument("--package-dir", type=pathlib.Path, required=True)
    runtime.add_argument("--stripped-libxul", type=pathlib.Path, required=True)
    runtime.add_argument("--output", type=pathlib.Path, required=True)
    source = subparsers.add_parser("verify-source")
    source.add_argument("--source-dir", type=pathlib.Path, required=True)
    args = parser.parse_args()
    try:
        source_tree = verify_source_state(args.source_dir)
        if args.command == "verify-source":
            identity = expected_identity()
            print(
                "MAKOS_FIREFOX_SOURCE_OK "
                f"source={identity['source_version']}@{identity['source_commit']} "
                f"patches={identity['patch_count']} "
                f"patch_series_sha256={identity['patch_series_sha256']} "
                f"tree={source_tree} tracked=exact untracked=nonignored-denied"
            )
            return 0
        elif args.command == "create-build-stamp":
            record = build_record(args.bin_dir, source_tree)
            verify_record_source_tree(record, source_tree)
            write_record(args.output, record)
        elif args.command == "verify-build-stamp":
            record = verify_build_stamp(args.stamp, args.bin_dir)
        else:
            record = runtime_record(
                args.stamp,
                args.bin_dir,
                args.package_dir,
                args.stripped_libxul,
            )
            verify_record_source_tree(record, source_tree)
            write_record(args.output, record)
        verify_record_source_tree(record, source_tree)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"firefox_provenance: {error}", file=sys.stderr)
        return 1
    print(
        "MAKOS_FIREFOX_PROVENANCE_OK "
        f"source={record['source_version']}@{record['source_commit']} "
        f"patches={record['patch_count']} "
        f"patch_series_sha256={record['patch_series_sha256']} "
        f"source_tree={source_tree} "
        f"build_artifacts={len(record['build_artifacts'])} "
        f"runtime_artifacts={len(record.get('runtime_artifacts', {}))} "
        f"target={record['target']}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
