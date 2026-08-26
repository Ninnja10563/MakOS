#!/usr/bin/env python3
"""Bind staged Firefox artifacts to the pinned source and ordered patch set."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
import subprocess
import sys
from typing import Any


ROOT = pathlib.Path(__file__).resolve().parent.parent
SCHEMA = 1
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


def build_record(bin_dir: pathlib.Path, root: pathlib.Path = ROOT) -> dict[str, Any]:
    return {**expected_identity(root), "build_artifacts": artifact_hashes(bin_dir)}


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
    if set(record) != {*expected, "build_artifacts"}:
        raise ValueError("Firefox provenance record fields invalid")
    observed_identity = {key: record.get(key) for key in expected}
    if observed_identity != expected:
        raise ValueError(
            "Firefox provenance identity mismatch: "
            f"expected={expected} observed={observed_identity}"
        )
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
    build_record_fields = set(expected_identity(root)) | {"build_artifacts"}
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


def source_git_dir(source_dir: pathlib.Path) -> pathlib.Path:
    output = subprocess.check_output(
        ["git", "-C", str(source_dir), "rev-parse", "--absolute-git-dir"],
        text=True,
    ).strip()
    return pathlib.Path(output)


def verify_source_state(source_dir: pathlib.Path, root: pathlib.Path = ROOT) -> None:
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
    args = parser.parse_args()
    try:
        verify_source_state(args.source_dir)
        if args.command == "create-build-stamp":
            record = build_record(args.bin_dir)
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
            write_record(args.output, record)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"firefox_provenance: {error}", file=sys.stderr)
        return 1
    print(
        "MAKOS_FIREFOX_PROVENANCE_OK "
        f"source={record['source_version']}@{record['source_commit']} "
        f"patches={record['patch_count']} "
        f"patch_series_sha256={record['patch_series_sha256']} "
        f"build_artifacts={len(record['build_artifacts'])} "
        f"runtime_artifacts={len(record.get('runtime_artifacts', {}))} "
        f"target={record['target']}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
