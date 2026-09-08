#!/usr/bin/env python3
"""Check the MakOS manifest patch and, with source, real Mozilla staging."""
from __future__ import annotations

import argparse
from pathlib import Path
import shlex
import stat
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / "ports/firefox"
PATCH = PORT / "patches/0060-makos-stage-process-artifacts.patch"
MANIFEST = "browser/installer/package-manifest.in"
CHILDREN = ("plugin-container", "xpcshell")


def check_wiring() -> None:
    patch = PATCH.read_text()
    additions = [line[1:] for line in patch.splitlines()
                 if line.startswith("+") and not line.startswith("+++")]
    assert additions == ["#ifdef XP_MAKOS", "@BINPATH@/@MOZ_CHILD_PROCESS_NAME@",
                         "@BINPATH@/xpcshell", "#endif"]
    assert subprocess.check_output(["git", "apply", "--numstat", str(PATCH)], text=True).strip() == (
        f"4\t0\t{MANIFEST}"
    )
    target = (PORT / "patches/0001-makos-target-recognition.patch").read_text()
    assert 'set_define("XP_MAKOS", target_is_makos)' in target
    package = (PORT / "package-makos.sh").read_text()
    assert 'make -C "$OBJ/browser/installer" stage-package' in package
    assert 'Mozilla stage-package artifact absent: $DIST/$artifact' in package
    assert 'cmp -s "$artifact_snapshot/$artifact" "$package_root/$artifact"' in package


def source_test(source: Path) -> None:
    lock = dict(line.split("=", 1) for line in (PORT / "source.lock").read_text().splitlines())
    revision = lock["FIREFOX_COMMIT"]
    # Read the original pinned manifest, not a hand-authored stage listing or
    # an already altered working-tree file. Never patch the user's source.
    original = subprocess.check_output(["git", "-C", str(source), "show", f"{revision}:{MANIFEST}"])
    sys.dont_write_bytecode = True
    sys.path[:0] = [str(source / "python/mozbuild"), str(source / "third_party/python/jsmin"),
                   str(source / "third_party/python/packaging")]
    from mozbuild.backend.configenvironment import ConfigEnvironment
    from mozpack.copier import FileCopier
    from mozpack.errors import ErrorMessage
    from mozpack.files import FileFinder
    from mozpack.packager import SimpleManifestSink, preprocess_manifest
    from mozpack.packager.formats import FlatFormatter, OmniJarFormatter

    installer = (source / "toolkit/mozapps/installer/packager.mk").read_text()
    assert 'packager.py $(DEFINES) $(ACDEFINES)' in installer
    upload = (source / "toolkit/mozapps/installer/upload-files.mk").read_text()
    exclusions = upload.split("NO_PKG_FILES", 1)[1].split("export", 1)[0]
    assert not any(name in exclusions for name in CHILDREN)

    class Listing:
        def __init__(self):
            self.records = []

        def add(self, component, path):
            self.records.append(("add", component.name, path))

        def remove(self, component, path):
            self.records.append(("remove", component.name, path))

    def listing(manifest, defines):
        sink = Listing()
        preprocess_manifest(sink, str(manifest), defines)
        return sink.records

    with tempfile.TemporaryDirectory(prefix="makos-mozilla-manifest-") as tmp:
        base = Path(tmp)
        before = base / "original.in"
        before.write_bytes(original)
        patched = base / MANIFEST
        patched.parent.mkdir(parents=True)
        patched.write_bytes(original)
        subprocess.run(["git", "apply", "--check", str(PATCH)], cwd=base, check=True)
        subprocess.run(["git", "apply", str(PATCH)], cwd=base, check=True)
        locale = base / "locale-manifest.in"
        locale.write_text("")
        common = dict(BINPATH="bin", RESPATH="bin", AB_CD="en-US", PREF_DIR="defaults/pref",
                      MOZ_APP_NAME="firefox", MOZ_CHILD_PROCESS_NAME="plugin-container",
                      MOZ_EME_PROCESS_NAME="media-plugin-helper", MOZ_STATIC_JS=1,
                      DLL_PREFIX="lib", DLL_SUFFIX=".so", BIN_SUFFIX="", JAREXT="",
                      PKG_LOCALE_MANIFEST=str(locale), APPNAME="Firefox.app", LPROJ_ROOT="en")
        # Exercise Mozilla's real config -> ACDEFINES serialization as well as
        # the preprocessor. XP_MAKOS comes from configure, not the macOS host.
        environment = ConfigEnvironment(str(source), str(base),
                                        defines={"XP_UNIX": 1, "XP_MAKOS": 1})
        generated = {}
        for flag in shlex.split(environment.substs["ACDEFINES"]):
            name, value = flag.removeprefix("-D").split("=", 1)
            generated[name] = int(value)
        assert generated == {"XP_MAKOS": 1, "XP_UNIX": 1}
        makos = {**common, **generated}
        old = listing(before, makos)
        new = listing(patched, makos)
        gained = [("add", "xpcom", f"bin/{name}") for name in CHILDREN]
        assert all(record not in old and new.count(record) == 1 for record in gained)
        assert [record for record in new if record not in gained] == old
        for platform in ({"XP_UNIX": 1}, {"XP_UNIX": 1, "XP_MACOSX": 1}, {"XP_WIN": 1}):
            defines = {**common, **platform}
            assert listing(before, defines) == listing(patched, defines), platform

        # The full pinned manifest is preprocessed, but only its complete
        # [xpcom] component is staged. This does not qualify a full Gecko package.
        paths = [path for op, component, path in new if component == "xpcom" and op == "add"]
        assert paths and all(path.startswith("bin/") and "*" not in path for path in paths)
        inputs = base / "dist"
        for path in paths:
            file = inputs / path
            file.parent.mkdir(parents=True, exist_ok=True)
            file.write_bytes(f"stage fixture: {path}\n".encode())
            file.chmod(0o755 if file.name in CHILDREN else 0o644)

        def stage(manifest, output, omni=False):
            copier = FileCopier()
            formatter = OmniJarFormatter(copier, "omni.ja") if omni else FlatFormatter(copier)
            sink = SimpleManifestSink(FileFinder(str(inputs)), formatter)

            class Xpcom:
                def add(self, component, path):
                    if component.name == "xpcom":
                        sink.add(component, path)

                def remove(self, component, path):
                    if component.name == "xpcom":
                        sink.remove(component, path)

            preprocess_manifest(Xpcom(), str(manifest), makos)
            sink.close(auto_root_manifest=False)
            copier.copy(str(output))

        for omni in (False, True):
            destination = base / f"staged-{omni}"
            stage(patched, destination, omni)
            for name in CHILDREN:
                file = destination / name
                assert file.is_file() and not file.is_symlink()
                assert file.read_bytes() == (inputs / "bin" / name).read_bytes()
                assert stat.S_IMODE(file.stat().st_mode) == 0o755
            assert not (destination / "bin").exists(), "executables must be flat, not nested"
        # The old manifest must genuinely omit both even with inputs present.
        stage(before, base / "old-stage")
        assert all(not (base / "old-stage" / name).exists() for name in CHILDREN)
        for name in CHILDREN:
            path = inputs / "bin" / name
            data = path.read_bytes()
            path.unlink()
            try:
                stage(patched, base / f"missing-{name}")
            except ErrorMessage as error:
                assert f"Missing file(s): bin/{name}" in str(error), error
            else:
                raise AssertionError(f"Mozilla accepted missing {name}")
            path.write_bytes(data)
            path.chmod(0o755)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--source-dir", type=Path, help="require real Mozilla preprocessing/staging")
    args = parser.parse_args()
    check_wiring()
    source = args.source_dir or ROOT / "build/ports/firefox/source"
    if args.source_dir or (source / "python/mozbuild/mozpack").is_dir():
        source_test(source.resolve(strict=True))
        print("MAKOS_FIREFOX_PACKAGE_MANIFEST_OK source=pinned-upstream preprocessor=mozilla "
              "configure=ACDEFINES stage=mozpack-xpcom-flat,omni artifacts=plugin-container,xpcshell "
              "bytes=exact mode=755 missing=denied old_manifest=omits other_platforms=unchanged")
    else:
        print("MAKOS_FIREFOX_PACKAGE_MANIFEST_STRUCTURAL_OK source=absent "
              "stage=not-run require=--source-dir")


if __name__ == "__main__":
    main()
