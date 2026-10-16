#!/usr/bin/env python3
"""Exercise the real errno staging script using private temporary fixtures only.

No canonical source/stage, Firefox build, package provenance, or guest mutation.
Preparation failure is transactional; publication failure is reported, not
claimed rollback-safe. Temporary fixtures are removed on exit.
"""
from __future__ import annotations

import os
from pathlib import Path
import shutil
import stat
import subprocess
import tempfile

PORT = Path(__file__).resolve().parent
SCRIPT = PORT / "prepare-rust-errno.sh"
PATCH = PORT / "rust-patches/errno-0.3.8-makos.patch"
MARKER = "MAKOS_FIREFOX_ERRNO_OK version=0.3.8 accessor=__errno_location tls=musl checksum-safe=1"


def snapshot(root: Path) -> dict:
    result = {}
    for parent, directories, files in os.walk(root, followlinks=False):
        for name in directories + files:
            path = Path(parent) / name
            info = path.lstat()
            content = (os.readlink(path) if path.is_symlink() else
                       path.read_bytes() if path.is_file() else None)
            result[str(path.relative_to(root))] = (
                info.st_mode, info.st_ino, info.st_mtime_ns, content)
    return result


def files(snapshot_value: dict) -> dict:
    return {key: value for key, value in snapshot_value.items()
            if not stat.S_ISDIR(value[0])}


def fixture(root: Path) -> tuple[Path, Path]:
    source, stage = root / "source", root / "stage"
    (source / "src").mkdir(parents=True)
    (source / "Cargo.toml").write_text('[package]\nname = "errno"\nversion = "0.3.8"\n')
    (source / "README.md").write_text("upstream-v1\n")
    (source / "src/unix.rs").write_text(
        'extern "C" {\n    #[cfg_attr(\n        any(\n'
        '            target_os = "linux",\n'
        '            target_os = "hurd",\n'
        '            target_os = "redox",\n'
        '            target_os = "dragonfly"\n'
        '        ),\n        link_name = "__errno_location"\n'
        '    )]\n    #[cfg_attr(target_os = "aix", link_name = "_Errno")]\n'
        '    fn errno_location() -> *mut i32;\n}\n')
    (source / "src/unix.rs").chmod(0o640)
    (source / "readme-link").symlink_to("README.md")
    (source / "broken-link").symlink_to("missing")
    (source / "directory-link").symlink_to("src", target_is_directory=True)
    return source, stage


def prepare(source: Path, stage: Path, temporary: Path, *, script: Path = SCRIPT,
            patch: Path = PATCH, env_extra: dict | None = None,
            success: bool = True) -> subprocess.CompletedProcess:
    before = snapshot(source)
    result = subprocess.run(["sh", str(script)], capture_output=True, text=True,
                            timeout=20, env={**os.environ,
        "TMPDIR": str(temporary), "MAKOS_FIREFOX_ERRNO_SOURCE_DIR": str(source),
        "MAKOS_FIREFOX_ERRNO_STAGE_DIR": str(stage),
        "MAKOS_FIREFOX_ERRNO_PATCH_FILE": str(patch), **(env_extra or {})})
    assert snapshot(source) == before, "staging mutated source"
    assert (result.returncode == 0) == success, (result.returncode, result.stdout, result.stderr)
    if success:
        assert result.stdout.rstrip().endswith(MARKER), result.stdout
    else:
        assert MARKER not in result.stdout, "failure emitted success marker"
    assert not list(temporary.glob("makos-firefox-errno-stage.*")), "candidate leaked"
    return result


def exercise(root: Path) -> None:
    source, stage = fixture(root)
    temporary = root / "temporary"
    temporary.mkdir()
    prepare(source, stage, temporary)
    assert 'target_os = "makos"' in (stage / "src/unix.rs").read_text()
    assert stat.S_IMODE((stage / "src/unix.rs").stat().st_mode) == 0o640
    for name in ("readme-link", "broken-link", "directory-link"):
        assert os.readlink(stage / name) == os.readlink(source / name)
    initial = files(snapshot(stage))
    prepare(source, stage, temporary)
    assert files(snapshot(stage)) == initial, "identical preparation rewrote files"
    for path in source.rglob("*"):
        if path.is_file() and not path.is_symlink():
            os.utime(path, ns=(1500000000000000000, 1500000000000000000))
    prepare(source, stage, temporary)
    assert files(snapshot(stage)) == initial, "source timestamp-only update rewrote files"

    # Equal-size source changes with identical timestamps must not evade checksum.
    source_readme = source / "README.md"
    old = source_readme.stat()
    source_readme.write_text("upstream-v2\n")
    os.utime(source_readme, ns=(old.st_atime_ns, old.st_mtime_ns))
    prepare(source, stage, temporary)
    assert (stage / "README.md").read_text() == "upstream-v2\n"
    assert files(snapshot(stage))["src/unix.rs"] == initial["src/unix.rs"]

    # Destination corruption, including patched code, must be reconstructed.
    for path in (stage / "README.md", stage / "src/unix.rs"):
        old = path.stat()
        path.write_bytes(b"x" * old.st_size)
        os.utime(path, ns=(old.st_atime_ns, old.st_mtime_ns))
    (stage / "stale/subdir").mkdir(parents=True)
    (stage / "stale/subdir/file").write_text("stale")
    (stage / "src/unix.rs.orig").write_text("stale patch backup")
    (stage / "readme-link").unlink()
    (stage / "readme-link").symlink_to("wrong-target")
    prepare(source, stage, temporary)
    assert (stage / "README.md").read_bytes() == source_readme.read_bytes()
    assert (stage / "src/unix.rs").read_bytes() == initial["src/unix.rs"][3]
    assert not (stage / "stale").exists() and not (stage / "src/unix.rs.orig").exists()
    assert os.readlink(stage / "readme-link") == "README.md"

    # Existing directory symlinks are replaced, never traversed into source.
    shutil.rmtree(stage / "src")
    (stage / "src").symlink_to(source / "src", target_is_directory=True)
    prepare(source, stage, temporary)
    assert not (stage / "src").is_symlink()
    assert (stage / "src/unix.rs").read_bytes() == initial["src/unix.rs"][3]

    # Real make dependency, not a simulated freshness predicate. Age the tiny
    # fixture explicitly instead of adding sleeps or touching production paths.
    dependency = stage / "README.md"
    makefile = root / "Makefile"
    makefile.write_text("output: stage/README.md\n\tcp stage/README.md output\n")
    os.utime(dependency, (1500000000, 1500000000))
    subprocess.run(["make", "-f", str(makefile)], cwd=root, check=True, capture_output=True)
    os.utime(root / "output", (1500000010, 1500000010))
    prepare(source, stage, temporary)
    query = lambda: subprocess.run(["make", "-q", "-f", str(makefile)], cwd=root,
                                   capture_output=True).returncode
    assert query() == 0, "unchanged dependency triggered make rebuild"
    source_readme.write_text("upstream-v3\n")
    prepare(source, stage, temporary)
    assert query() == 1, "changed bytes did not trigger make rebuild"
    subprocess.run(["make", "-f", str(makefile)], cwd=root, check=True, capture_output=True)
    assert (root / "output").read_text() == "upstream-v3\n"
    assert query() == 0

    # A malformed patch must not publish any portion of its candidate.
    bad_patch = root / "bad.patch"
    bad_patch.write_text("not a patch\n")
    before = snapshot(stage)
    prepare(source, stage, temporary, patch=bad_patch, success=False)
    assert snapshot(stage) == before, "failed preparation changed published stage"
    absent = root / "absent-stage"
    prepare(source, absent, temporary, patch=bad_patch, success=False)
    assert not absent.exists(), "failed preparation created destination"

    # Even a patch command which edits its candidate before failing must leave
    # the previous stage unchanged. Its outer directory must remain private.
    failing_patch = root / "failing-patch"
    failing_patch.mkdir()
    (failing_patch / "patch").write_text(
        '#!/bin/sh\n'
        'case "$(uname -s)" in\n'
        'Darwin) mode=$(stat -f %Lp "$(dirname "$3")") ;;\n'
        '*) mode=$(stat -c %a "$(dirname "$3")") ;;\n'
        'esac\ntest "$mode" = 700 || exit 75\n'
        'printf partial >> "$3/src/unix.rs"\nexit 74\n')
    (failing_patch / "patch").chmod(0o755)
    result = prepare(source, stage, temporary, success=False,
                     env_extra={"PATH": str(failing_patch) + os.pathsep + os.environ["PATH"]})
    assert result.returncode == 74, (result.returncode, result.stderr)
    assert snapshot(stage) == before, "partially failed patch published candidate"

    # Publication failures must propagate and must not claim atomic rollback.
    shim = root / "shim"
    shim.mkdir()
    real_rsync = shutil.which("rsync")
    assert real_rsync
    (shim / "rsync").write_text('#!/bin/sh\nif test "$1" = -rlpgoD; then exit 73; fi\n'
                                f'exec "{real_rsync}" "$@"\n')
    (shim / "rsync").chmod(0o755)
    result = prepare(source, stage, temporary, success=False,
                     env_extra={"PATH": str(shim) + os.pathsep + os.environ["PATH"]})
    assert result.returncode == 73


def negative_controls(root: Path) -> None:
    original = SCRIPT.read_text()
    variants = {
        "timestamp-publication": original.replace(
            'rsync -rlpgoD --delete --checksum "$candidate/"',
            'rsync -a --delete --checksum "$candidate/"'),
        "size-only-publication": original.replace(
            'rsync -rlpgoD --delete --checksum "$candidate/"',
            'rsync -rlpgoD --delete --size-only "$candidate/"'),
    }
    for name, mutant in variants.items():
        assert mutant != original
        directory = root / name
        directory.mkdir()
        script = directory / "prepare.sh"
        script.write_text(mutant)
        source, stage = fixture(directory)
        temporary = directory / "temporary"
        temporary.mkdir()
        prepare(source, stage, temporary, script=script)
        if name == "timestamp-publication":
            before = files(snapshot(stage))
            os.utime(source / "README.md", (1400000000, 1400000000))
            prepare(source, stage, temporary, script=script)
            assert files(snapshot(stage)) != before, "timestamp negative control survived"
        else:
            (stage / "README.md").write_text("corrupted!!\n")
            assert (stage / "README.md").stat().st_size == (source / "README.md").stat().st_size
            prepare(source, stage, temporary, script=script)
            assert (stage / "README.md").read_bytes() != (source / "README.md").read_bytes(), \
                "size-only negative control survived"


def main() -> None:
    with tempfile.TemporaryDirectory(prefix="makos-errno-staging-test-") as value:
        root = Path(value)
        exercise(root / "positive")
        negative_controls(root)
    print("MAKOS_FIREFOX_ERRNO_STAGING_HOST_OK "
          "unchanged=inode,mtime source_timestamp_only=preserved "
          "checksum=equal-size-source,destination-drift "
          "source=unchanged symlinks=preserved,drift-repaired "
          "stale=removed prepare_failure=unpublished publish_failure=propagated "
          "make=unchanged-skipped,changed-rebuilt negative_controls=2 guest=not-run")


if __name__ == "__main__":
    main()
