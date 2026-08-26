#!/usr/bin/env python3
"""Offline regression tests for Firefox source/patch/artifact provenance."""

from __future__ import annotations

import hashlib
import json
import pathlib
import subprocess
import tempfile

import firefox_provenance as provenance


EXPECTED_PATCH_COUNT = 56
EXPECTED_PATCH_SHA256 = (
    "9cd45fc60a13102f7a52cf6f31b2c33b3f66c501a8d64b3e567a97e6e34aae9c"
)


def expect_failure(action, fragment: str) -> None:
    try:
        action()
    except ValueError as error:
        assert fragment in str(error), error
    else:
        raise AssertionError(f"expected provenance failure: {fragment}")


def fixture_root(base: pathlib.Path) -> pathlib.Path:
    root = base / "fixture-root"
    patches = root / "ports/firefox/patches"
    patches.mkdir(parents=True)
    (root / "ports/firefox/source.lock").write_text(
        "FIREFOX_VERSION=1.2.3esr\nFIREFOX_COMMIT=" + "a" * 40 + "\n"
    )
    # Creation order intentionally differs from lexical series order.
    (patches / "0002-second.patch").write_bytes(b"second\n")
    (patches / "0001-first.patch").write_bytes(b"first\n")
    return root


def fixture_source(base: pathlib.Path, root: pathlib.Path) -> pathlib.Path:
    source = base / "source"
    source.mkdir()
    subprocess.run(["git", "init", "-q", str(source)], check=True)
    (source / "README").write_text("pinned Firefox fixture\n")
    subprocess.run(["git", "-C", str(source), "add", "README"], check=True)
    subprocess.run(
        [
            "git",
            "-C",
            str(source),
            "-c",
            "user.name=MakOS Test",
            "-c",
            "user.email=makos-test@localhost",
            "commit",
            "-q",
            "-m",
            "fixture",
        ],
        check=True,
    )
    commit = subprocess.check_output(
        ["git", "-C", str(source), "rev-parse", "HEAD"], text=True
    ).strip()
    (root / "ports/firefox/source.lock").write_text(
        f"FIREFOX_VERSION=1.2.3esr\nFIREFOX_COMMIT={commit}\n"
    )
    _, patch_identity = provenance.patch_series_identity(root)
    git_dir = provenance.source_git_dir(source)
    (git_dir / "makos-patches.sha256").write_text(patch_identity + "\n")
    provenance.verify_source_state(source, root)
    (git_dir / "makos-patches.sha256").write_text("0" * 64 + "\n")
    expect_failure(
        lambda: provenance.verify_source_state(source, root),
        "applied-patch marker mismatch",
    )
    (git_dir / "makos-patches.sha256").write_text(patch_identity + "\n")
    return source


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="makos-firefox-provenance-") as name:
        base = pathlib.Path(name)
        root = fixture_root(base)
        source = fixture_source(base, root)
        ordered = hashlib.sha256()
        for payload in (b"first\n", b"second\n"):
            ordered.update(hashlib.sha256(payload).hexdigest().encode())
            ordered.update(b"\n")
        count, identity = provenance.patch_series_identity(root)
        assert count == 2 and identity == ordered.hexdigest()

        bin_dir = base / "bin"
        bin_dir.mkdir()
        for index, artifact in enumerate(provenance.BUILD_ARTIFACTS):
            (bin_dir / artifact).write_bytes(
                b"\x7fELF" + artifact.encode() + bytes([index])
            )
        stamp = base / "stamp.json"
        record = provenance.build_record(bin_dir, root)
        provenance.write_record(stamp, record)
        assert provenance.verify_build_stamp(stamp, bin_dir, root) == record

        package_dir = base / "package"
        package_dir.mkdir()
        for artifact in provenance.RUNTIME_ARTIFACTS:
            (package_dir / artifact).write_bytes(b"runtime-" + artifact.encode())
        stripped_libxul = base / "stripped-libxul.so"
        stripped_libxul.write_bytes(b"stripped-runtime-libxul")
        runtime_record = provenance.runtime_record(
            stamp, bin_dir, package_dir, stripped_libxul, root
        )
        runtime_stamp = base / "runtime.json"
        provenance.write_record(runtime_stamp, runtime_record)
        assert provenance.read_runtime_record(runtime_stamp, root) == runtime_record
        assert runtime_record["runtime_artifacts"]["libxul.so"] == provenance.sha256_path(
            stripped_libxul
        )

        (bin_dir / "libxul.so").write_bytes(b"stale")
        expect_failure(
            lambda: provenance.verify_build_stamp(stamp, bin_dir, root),
            "differ from audited build stamp",
        )

        wrong = dict(record)
        wrong["patch_series_sha256"] = "0" * 64
        provenance.write_record(stamp, wrong)
        expect_failure(
            lambda: provenance.read_build_record(stamp, root), "identity mismatch"
        )

        extra = {**record, "unbounded": True}
        provenance.write_record(stamp, extra)
        expect_failure(
            lambda: provenance.read_build_record(stamp, root), "fields invalid"
        )

        noncanonical = json.dumps(record, indent=2).encode() + b"\n"
        expect_failure(
            lambda: provenance.parse_record(noncanonical), "not canonical"
        )

        bad_runtime = dict(runtime_record)
        bad_runtime["runtime_artifacts"] = dict(runtime_record["runtime_artifacts"])
        bad_runtime["runtime_artifacts"]["firefox"] = "x" * 64
        provenance.write_record(runtime_stamp, bad_runtime)
        expect_failure(
            lambda: provenance.read_runtime_record(runtime_stamp, root),
            "artifact digest invalid",
        )

        # Source verification is a required part of both record-creation CLIs.
        provenance.verify_source_state(source, root)

    pipeline_guards = {
        "ports/firefox/build-makos.sh": (
            "create-build-stamp",
            "audit-binary.sh",
        ),
        "ports/firefox/package-makos.sh": (
            "verify-build-stamp",
            "create-runtime-record",
            "--stripped-libxul",
        ),
        "scripts/integrate_data_image.py": (
            "verify_firefox_provenance",
            '"firefox_build_provenance"',
        ),
        "Makefile": (
            "scripts/verify_firefox_runtime_image.py",
            "test-aarch64-firefox-runtime",
        ),
    }
    for relative, required in pipeline_guards.items():
        contents = (provenance.ROOT / relative).read_text()
        for token in required:
            assert token in contents, f"Firefox provenance pipeline guard absent: {token}"

    current_count, current_identity = provenance.patch_series_identity()
    assert (current_count, current_identity) == (
        EXPECTED_PATCH_COUNT,
        EXPECTED_PATCH_SHA256,
    )
    print(
        "MAKOS_FIREFOX_PROVENANCE_TEST_OK "
        f"patches={current_count} patch_series_sha256={current_identity} "
        "source=pinned build_hashes=5 runtime_hashes=5 stale=denied fields=exact "
        "pipeline=build,package,integrate,pre-qemu"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
