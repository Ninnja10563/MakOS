#!/usr/bin/env python3
"""Real private-copy independence and bounded platform-dispatch regression.

No release artifact, image, guest or source-tree mutation. APFS block sharing
is not inferred from inode/size counters; clone fallback may save no space.
"""
from __future__ import annotations

import hashlib
import os
from pathlib import Path
import platform
import stat
import subprocess
import tempfile

PORT = Path(__file__).resolve().parent
HELPER = PORT / "copy-private.sh"
CP = "/bin/cp"


def run_copy(helper: Path, *args: str | Path, env: dict | None = None,
             trace: bool = False) -> subprocess.CompletedProcess:
    command = ["sh"] + (["-x"] if trace else []) + [str(helper), *map(str, args)]
    return subprocess.run(command, env=env, capture_output=True, text=True, timeout=15)


def require_ok(result: subprocess.CompletedProcess) -> None:
    assert result.returncode == 0, result.stdout + result.stderr


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def mode(path: Path) -> int:
    return stat.S_IMODE(path.stat().st_mode)


def independent_file(helper: Path, directory: Path, flag: str | None = None) -> None:
    directory.mkdir()
    source, output = directory / "source", directory / "output"
    initial = bytes(range(256)) * 1024
    source.write_bytes(initial)
    source.chmod(0o750)
    os.utime(source, (1600000000, 1600000000))
    require_ok(run_copy(helper, *([flag] if flag else []), source, output))
    assert sha(source) == sha(output), "copy bytes differ"
    assert not source.samefile(output), "private copy must not hardlink source"
    if flag == "-p":
        assert mode(source) == mode(output)
        assert source.stat().st_mtime_ns == output.stat().st_mtime_ns
    output.write_bytes(b"destination mutation")
    assert source.read_bytes() == initial, "destination mutation changed source"
    source.write_bytes(b"source mutation")
    assert output.read_bytes() == b"destination mutation", "source mutation changed destination"
    output.write_bytes(b"")
    assert source.read_bytes() == b"source mutation", "destination truncation changed source"
    output.write_bytes(b"destination survives source truncate")
    source.write_bytes(b"")
    assert output.read_bytes() == b"destination survives source truncate", "source truncation changed destination"
    link = directory / "source-link"
    link.symlink_to("output")
    link_copy = directory / "link-copy"
    require_ok(run_copy(helper, *([flag] if flag else []), link, link_copy))
    assert not link_copy.is_symlink(), "file copy follows source symlink like normal cp"
    assert link_copy.read_bytes() == output.read_bytes()
    assert not link_copy.samefile(output)


def tree_state(root: Path) -> dict:
    result = {}
    for parent, directories, files in os.walk(root, followlinks=False):
        for name in directories + files:
            path = Path(parent) / name
            key = str(path.relative_to(root))
            if path.is_symlink():
                result[key] = ("link", os.readlink(path))
            elif path.is_dir():
                result[key] = ("dir", mode(path))
            else:
                result[key] = ("file", mode(path), sha(path))
    return result


def recursive_copy(helper: Path, directory: Path, flag: str = "-R") -> None:
    directory.mkdir()
    source, output, normal = [directory / name for name in ("source", "output", "normal")]
    (source / "nested").mkdir(parents=True)
    (source / "nested/file").write_bytes(b"nested source bytes")
    (source / "nested/file").chmod(0o640)
    os.utime(source / "nested/file", (1600000000, 1600000000))
    (source / "nested").chmod(0o750)
    (source / "link").symlink_to("nested/file")
    (source / "dir-link").symlink_to("nested", target_is_directory=True)
    (source / "broken-link").symlink_to("absent")
    output.mkdir()
    normal.mkdir()
    require_ok(run_copy(helper, flag, str(source) + "/.", output))
    subprocess.run([CP, flag, "--", str(source) + "/.", str(normal)], check=True)
    assert tree_state(output) == tree_state(normal), "recursive copy changed cp semantics"
    assert mode(output) == mode(normal), "recursive root mode differs from normal cp"
    if flag == "-Rp":
        assert (output / "nested/file").stat().st_mtime_ns == (source / "nested/file").stat().st_mtime_ns
    before = tree_state(source)
    assert not (source / "nested/file").samefile(output / "nested/file")
    (output / "nested/file").write_bytes(b"changed private nested file")
    assert tree_state(source) == before, "recursive destination mutation changed source"
    (source / "nested/file").write_bytes(b"")
    assert (output / "nested/file").read_bytes() == b"changed private nested file"
    # Links are preserved as links, not expanded, exactly as ordinary -R.
    assert os.readlink(output / "link") == "nested/file"
    assert os.readlink(output / "broken-link") == "absent"
    # Existing files/directories take the same preserve-or-retain metadata
    # path as cp; verify a second recursive copy rather than only empty roots.
    for tree in (output, normal):
        (tree / "nested/file").chmod(0o600)
        (tree / "nested").chmod(0o700)
    require_ok(run_copy(helper, flag, str(source) + "/.", output))
    subprocess.run([CP, flag, "--", str(source) + "/.", str(normal)], check=True)
    assert tree_state(output) == tree_state(normal), "existing recursive destination differs from normal cp"
    assert not (source / "nested/file").samefile(output / "nested/file")


def compare_modes(helper: Path, directory: Path) -> None:
    directory.mkdir()
    source = directory / "source"
    source.write_bytes(b"mode fixture")
    source.chmod(0o751)
    for preserve in (False, True):
        for existing in (False, True):
            name = f"{preserve}-{existing}"
            output, normal = directory / (name + "-output"), directory / (name + "-normal")
            if existing:
                for path in (output, normal):
                    path.write_bytes(b"old")
                    path.chmod(0o600)
            flag = ["-p"] if preserve else []
            # Match normal cp under a restrictive umask, including an existing
            # destination whose mode ordinary non-preserving cp keeps intact.
            subprocess.run(["sh", "-c", 'umask 027; exec "$@"', "copy-test",
                            CP, *flag, "--", str(source), str(normal)], check=True)
            result = subprocess.run(["sh", "-c", 'umask 027; exec sh "$@"', "copy-test",
                                     str(helper), *flag, str(source), str(output)],
                                    capture_output=True, text=True, timeout=15)
            require_ok(result)
            assert mode(output) == mode(normal), (
                "copy mode differs from normal cp", preserve, existing,
                oct(mode(output)), oct(mode(normal)))
            assert output.read_bytes() == normal.read_bytes()
    # Existing destination symlinks must behave like ordinary cp too.
    for preserve in (False, True):
        output, normal = directory / f"symlink-{preserve}", directory / f"normal-symlink-{preserve}"
        for target in (output, normal):
            referent = target.with_suffix(".referent")
            referent.write_bytes(b"prior symlink referent")
            target.symlink_to(referent.name)
        flag = ["-p"] if preserve else []
        require_ok(run_copy(helper, *flag, source, output))
        subprocess.run([CP,*flag,"--",str(source),str(normal)],check=True)
        assert output.is_symlink() == normal.is_symlink(), "destination symlink behavior differs from cp"
        assert output.read_bytes() == normal.read_bytes()
        assert output.with_suffix(".referent").read_bytes() == normal.with_suffix(".referent").read_bytes()


def selection(helper: Path, directory: Path) -> None:
    directory.mkdir()
    tools = directory / "tools"
    tools.mkdir()
    source = directory / "source"
    source.write_text("dispatch")
    uname = tools / "uname"
    copy = tools / "cp"
    log = directory / "cp-args"
    env = os.environ | {"PATH": str(tools) + ":" + os.environ["PATH"], "COPY_TEST_LOG": str(log)}
    # Non-Darwin path: record argv, then perform an actual ordinary copy.
    uname.write_text("#!/bin/sh\nprintf '%s\\n' Linux\n")
    copy.write_text("#!/bin/sh\nprintf '%s\\n' \"$@\" >\"$COPY_TEST_LOG\"\nexec /bin/cp \"$@\"\n")
    uname.chmod(0o755)
    copy.chmod(0o755)
    require_ok(run_copy(helper, source, directory / "linux-copy", env=env))
    assert log.read_text().splitlines() == ["--", str(source), str(directory / "linux-copy")]
    # Real Darwin cp must be absolute and receive -c; do not pretend a foreign
    # cp supports this flag when the regression runs on a Linux build host.
    if platform.system() == "Darwin":
        uname.write_text("#!/bin/sh\nprintf '%s\\n' Darwin\n")
        copy.write_text("#!/bin/sh\nexit 97\n")
        result = run_copy(helper, "-p", source, directory / "darwin-copy", env=env, trace=True)
        require_ok(result)
        assert "exec /bin/cp -c -p --" in result.stderr, "Darwin must request clone copy"


def failure_checks(helper: Path, directory: Path) -> None:
    directory.mkdir()
    missing, target = directory / "missing", directory / "target"
    result = run_copy(helper, missing, target)
    assert result.returncode != 0, "copy failure must propagate"
    assert not target.exists()
    for args in [(), ("-L", "source", "target"), ("-R", "-p", "source", "target")]:
        assert run_copy(helper, *args).returncode == 2, "unsupported form must fail"


def main() -> None:
    package = (PORT / "package-makos.sh").read_text()
    assert package.count('"$ROOT/ports/firefox/copy-private.sh"') == 2
    assert '"$ROOT/ports/firefox/copy-private.sh" -p "$BIN/$artifact" "$artifact_snapshot/$artifact"' in package
    assert '"$ROOT/ports/firefox/copy-private.sh" -Rp "$DIST/." "$package_root/"' in package
    assert 'snapshot_dir=$(mktemp -d "${TMPDIR:-/tmp}/makos-firefox-package.XXXXXX")' in package
    assert 'package_root=$snapshot_dir/package-root' in package
    assert package.index('find "$package_root" -type l') < package.index('"$LLVM_STRIP" --strip-sections "$package_root/$artifact"')
    assert 'cmp -s "$artifact_snapshot/$artifact" "$package_root/$artifact"' in package
    with tempfile.TemporaryDirectory(prefix="makos-private-copy-") as temporary:
        root = Path(temporary)
        independent_file(HELPER, root / "file")
        independent_file(HELPER, root / "preserved-file", "-p")
        recursive_copy(HELPER, root / "tree")
        recursive_copy(HELPER, root / "preserved-tree", "-Rp")
        compare_modes(HELPER, root / "modes")
        selection(HELPER, root / "selection")
        failure_checks(HELPER, root / "failure")
        # A hardlink "optimization" must fail before mutating its source.
        hardlink = root / "hardlink.sh"
        hardlink.write_text('#!/bin/sh\nexec ln -- "$1" "$2"\n')
        try:
            independent_file(hardlink, root / "negative-hardlink")
        except AssertionError as error:
            assert "must not hardlink source" in str(error)
        else:
            raise AssertionError("hardlink negative control escaped")
        swallowed = root / "swallowed.sh"
        swallowed.write_text(HELPER.read_text().replace(
            'exec /bin/cp -c "$@"', '/bin/cp -c "$@" || true; exit 0').replace(
            'exec cp "$@"', 'cp "$@" || true; exit 0'))
        try:
            failure_checks(swallowed, root / "negative-failure")
        except AssertionError as error:
            assert "copy failure must propagate" in str(error)
        else:
            raise AssertionError("failure-status negative control escaped")
        if platform.system() == "Darwin":
            unpreserved = root / "unpreserved-clone.sh"
            unpreserved.write_text(HELPER.read_text().replace(
                'Darwin:-p|Darwin:-Rp)', 'Darwin:*)'))
            try:
                compare_modes(unpreserved, root / "negative-clone-umask")
            except AssertionError as error:
                assert "copy mode differs from normal cp" in str(error)
            else:
                raise AssertionError("non-preserving clone negative control escaped")
            overwrite = root / "clone-overwrite.sh"
            overwrite.write_text(HELPER.read_text().replace(
                'if clone_destination_available "$copy_destination"; then', 'if true; then'))
            try:
                compare_modes(overwrite, root / "negative-clone-overwrite")
            except AssertionError as error:
                assert "destination symlink behavior differs from cp" in str(error)
            else:
                raise AssertionError("clone-overwrite negative control escaped")
    print("MAKOS_FIREFOX_PRIVATE_COPY_HOST_OK forms=file,preserved-file,recursive-tree,preserved-tree "
          "independence=bidirectional-write,truncate modes=normal-cp symlinks=normal-cp "
          "negative_controls=hardlink,swallowed-failure "
          f"darwin_umask_negative={'passed' if platform.system() == 'Darwin' else 'not-applicable'} "
          f"darwin_overwrite_negative={'passed' if platform.system() == 'Darwin' else 'not-applicable'} "
          f"host={platform.system()} clone_savings=not-guaranteed browser_qualification=not-run")


if __name__ == "__main__":
    main()
