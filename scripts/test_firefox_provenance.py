#!/usr/bin/env python3
"""Offline regression tests for Firefox source/patch/artifact provenance."""

from __future__ import annotations

import hashlib
import json
import pathlib
import shlex
import subprocess
import tempfile

import firefox_provenance as provenance


EXPECTED_PATCH_COUNT = 59
EXPECTED_PATCH_SHA256 = (
    "c922d619398e64b6a162046efde105bc19152a9d868e9a2254ffa701874cc974"
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
    return root


def fixture_source(base: pathlib.Path, root: pathlib.Path) -> pathlib.Path:
    source = base / "source"
    source.mkdir()
    subprocess.run(["git", "init", "-q", str(source)], check=True)
    (source / "README").write_text("pinned Firefox fixture\n")
    (source / ".gitattributes").write_text("README text filter=tripwire\n")
    subprocess.run(
        ["git", "-C", str(source), "add", "README", ".gitattributes"],
        check=True,
    )
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
    patches = root / "ports/firefox/patches"
    # Creation order intentionally differs from lexical application order.
    (patches / "0002-second.patch").write_text(
        "diff --git a/widget/fixture.cpp b/widget/fixture.cpp\n"
        "new file mode 100644\n"
        "--- /dev/null\n"
        "+++ b/widget/fixture.cpp\n"
        "@@ -0,0 +1 @@\n"
        "+int fixture(void) { return 42; }\n"
        "diff --git a/widget/fixture-link b/widget/fixture-link\n"
        "new file mode 120000\n"
        "--- /dev/null\n"
        "+++ b/widget/fixture-link\n"
        "@@ -0,0 +1 @@\n"
        "+fixture.cpp\n"
        "\\ No newline at end of file\n"
    )
    (patches / "0001-first.patch").write_text(
        "diff --git a/README b/README\n"
        "--- a/README\n"
        "+++ b/README\n"
        "@@ -1 +1 @@\n"
        "-pinned Firefox fixture\n"
        "+patched Firefox fixture\n"
    )
    for patch in sorted(patches.glob("*.patch")):
        subprocess.run(
            ["git", "-C", str(source), "apply", str(patch)], check=True
        )
    _, patch_identity = provenance.patch_series_identity(root)
    git_dir = provenance.source_git_dir(source)
    (git_dir / "makos-patches.sha256").write_text(patch_identity + "\n")
    filter_sentinel = base / "filter-invoked"
    filter_script = base / "tripwire-filter.sh"
    filter_script.write_text(
        "#!/bin/sh\n"
        "printf invoked > \"$1\"\n"
        "printf 'patched Firefox fixture\\n'\n"
    )
    filter_script.chmod(0o755)
    filter_command = (
        f"{shlex.quote(str(filter_script))} {shlex.quote(str(filter_sentinel))}"
    )
    subprocess.run(
        ["git", "-C", str(source), "config", "filter.tripwire.clean", filter_command],
        check=True,
    )
    subprocess.run(
        ["git", "-C", str(source), "config", "core.autocrlf", "true"], check=True
    )
    subprocess.run(
        ["git", "-C", str(source), "config", "core.fileMode", "false"], check=True
    )
    index_before = (git_dir / "index").read_bytes()
    objects_dir = git_dir / "objects"
    objects_before = {
        path.relative_to(objects_dir): hashlib.sha256(path.read_bytes()).hexdigest()
        for path in objects_dir.rglob("*")
        if path.is_file()
    }
    provenance.verify_source_state(source, root)
    assert not filter_sentinel.exists()
    assert (git_dir / "index").read_bytes() == index_before
    assert {
        path.relative_to(objects_dir): hashlib.sha256(path.read_bytes()).hexdigest()
        for path in objects_dir.rglob("*")
        if path.is_file()
    } == objects_before

    readme = source / "README"
    original_readme = readme.read_bytes()
    readme.write_bytes(original_readme + b"dirty tracked mutation\n")
    expect_failure(
        lambda: provenance.verify_source_state(source, root),
        "differs from ordered patches",
    )
    assert not filter_sentinel.exists()
    readme.write_bytes(original_readme)
    provenance.verify_source_state(source, root)
    assert not filter_sentinel.exists()

    readme.write_bytes(b"patched Firefox fixture\r\n")
    expect_failure(
        lambda: provenance.verify_source_state(source, root),
        "differs from ordered patches: content README",
    )
    assert not filter_sentinel.exists()
    readme.write_bytes(original_readme)
    provenance.verify_source_state(source, root)

    fixture = source / "widget/fixture.cpp"
    fixture.chmod(0o755)
    expect_failure(
        lambda: provenance.verify_source_state(source, root),
        "differs from ordered patches: mode widget/fixture.cpp",
    )
    assert not filter_sentinel.exists()
    fixture.chmod(0o644)
    provenance.verify_source_state(source, root)

    fixture_link = source / "widget/fixture-link"
    fixture_link.unlink()
    fixture_link.symlink_to("other.cpp")
    expect_failure(
        lambda: provenance.verify_source_state(source, root),
        "differs from ordered patches: content widget/fixture-link",
    )
    assert not filter_sentinel.exists()
    fixture_link.unlink()
    fixture_link.write_text("fixture.cpp")
    expect_failure(
        lambda: provenance.verify_source_state(source, root),
        "differs from ordered patches: type widget/fixture-link",
    )
    assert not filter_sentinel.exists()
    fixture_link.unlink()
    fixture_link.symlink_to("fixture.cpp")
    provenance.verify_source_state(source, root)

    untracked = source / "unexpected-build-source.cpp"
    untracked.write_text("int unexpected(void) { return 1; }\n")
    expect_failure(
        lambda: provenance.verify_source_state(source, root),
        "unexpected untracked path: unexpected-build-source.cpp",
    )
    untracked.unlink()
    provenance.verify_source_state(source, root)
    assert not filter_sentinel.exists()

    (git_dir / "makos-patches.sha256").write_text("0" * 64 + "\n")
    expect_failure(
        lambda: provenance.verify_source_state(source, root),
        "applied-patch marker mismatch",
    )
    (git_dir / "makos-patches.sha256").write_text(patch_identity + "\n")

    linked = base / "linked-source"
    subprocess.run(
        [
            "git",
            "-C",
            str(source),
            "-c",
            "core.autocrlf=false",
            "worktree",
            "add",
            "-q",
            "--detach",
            str(linked),
            commit,
        ],
        check=True,
    )
    assert (linked / ".git").is_file()
    # Shared core.autocrlf/filter config must remain active for verification,
    # but the fixture patch itself is deliberately LF-exact.
    (linked / "README").write_bytes(b"pinned Firefox fixture\n")
    for patch in sorted(patches.glob("*.patch")):
        subprocess.run(
            [
                "git",
                "-C",
                str(linked),
                "-c",
                "core.autocrlf=false",
                "-c",
                "filter.tripwire.clean=cat",
                "apply",
                str(patch),
            ],
            check=True,
        )
    linked_git_dir = provenance.source_git_dir(linked)
    (linked_git_dir / "makos-patches.sha256").write_text(patch_identity + "\n")
    provenance.verify_source_state(linked, root)
    assert not filter_sentinel.exists()
    return source


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="makos-firefox-provenance-") as name:
        base = pathlib.Path(name)
        root = fixture_root(base)
        source = fixture_source(base, root)
        ordered = hashlib.sha256()
        for patch in sorted((root / "ports/firefox/patches").glob("*.patch")):
            payload = patch.read_bytes()
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
        source_tree = provenance.verify_source_state(source, root)
        record = provenance.build_record(bin_dir, source_tree, root)
        provenance.write_record(stamp, record)
        assert provenance.verify_build_stamp(stamp, bin_dir, root) == record
        provenance.verify_record_source_tree(record, source_tree)
        expect_failure(
            lambda: provenance.verify_record_source_tree(record, "0" * len(source_tree)),
            "source tree differs from verified patched tree",
        )

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
            "dist/firefox/makos-build-provenance.json",
        ),
        "ports/firefox/apply-patches.sh": ("verify-source",),
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
    handoff_prompt = (
        provenance.ROOT / "docs/MACOS-HVF-TEST-AGENT-PROMPT.md"
    ).read_text()
    assert (
        f"report {current_count} patches with exact ordered" in handoff_prompt
    ), "macOS/HVF handoff prompt has a stale Firefox patch count"
    assert (
        f"pinned source, {current_count}-patch series" in handoff_prompt
    ), "macOS/HVF preflight requirement has a stale Firefox patch count"
    assert (
        current_identity in handoff_prompt
    ), "macOS/HVF handoff prompt has a stale Firefox patch identity"
    print(
        "MAKOS_FIREFOX_PROVENANCE_TEST_OK "
        f"patches={current_count} patch_series_sha256={current_identity} "
        "source=pinned build_hashes=5 runtime_hashes=5 stale=denied fields=exact "
        "source_tree=ordered-patches-exact raw_bytes=exact crlf=denied "
        "clean_filter=not-invoked executable_mode=exact symlink=target-and-type-exact "
        "tracked_dirty=denied untracked=denied "
        "real_index=unchanged real_objects=unchanged git_worktree=supported "
        "pipeline=apply,build,package,integrate,pre-qemu"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
