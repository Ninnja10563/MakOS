#!/usr/bin/env python3
"""Offline replay/recovery tests, optionally repeated on the pinned archive."""
import argparse
import hashlib
import itertools
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile

PORT = Path(__file__).resolve().parent
NAMES = ("config.sub", "configure.ac", "configure")
# Independent excerpts from the pinned 3.14.7 files, not parsed from the patch.
# Keep the newly added upstream hiux/windows line: the old patch rejects it.
CONFIG_SUB = """#!/bin/sh
case ${1##*-} in
\tgnu* | android* | bsd* | mach* | minix* | genix* | ultrix* | irix* \\
\t     | *vms* | esix* | aix* | cnk* | sunos | sunos[34]* \\
\t     | hpux* | unos* | osf* | luna* | dgux* | auroraux* | solaris* \\
\t     | sym* |  plan9* | psp* | sim* | xray* | os68k* | v88r* \\
\t     | hiux* | abug | nacl* | netware* | windows* \\
\t     | os9* | macos* | osx* | ios* | tvos* | watchos* \\
\t     | mpw* | magic* | mmixware* | mon960* | lnews* \\
\t     | amigaos* | amigados* | msdos* | newsos* | unicos* | aof* \\
\t     | aos* | aros* | cloudabi* | sortix* | twizzler* \\
\t     | nindy* )
    echo "$1";;
*) exit 1;;
esac
"""
CONFIGURE = """case $host in
\t*-*-darwin*)
\t\tac_sys_system=Darwin
\t\t;;
\t*-*-vxworks*)
\t    ac_sys_system=VxWorks
\t    ;;
esac
case $host in
\t*-*-darwin*)
\t\tcase "$host_cpu" in
\t\tarm*)
\t\t\t_host_ident=arm
\t\t\t;;
\t\t*)
\t\t\t_host_ident=$host_cpu
\t\tesac
\t\t;;
\t*-*-vxworks*)
\t\t_host_ident=$host_cpu
\t\t;;
esac
"""


def invoke(root, ok=True):
    result = subprocess.run(
        [str(PORT / "apply-patches.sh")],
        env={**os.environ, "CPYTHON_SOURCE_DIR": str(root)},
        text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
    )
    assert (result.returncode == 0) == ok, result.stdout
    if ok:
        assert "MAKOS_CPYTHON_PATCHES_OK target=aarch64-unknown-makos" in result.stdout
    return result


def check(originals):
    with tempfile.TemporaryDirectory(prefix="makos-cpython-patches-") as tmp:
        root = Path(tmp)
        for name, content in originals.items():
            (root / name).write_bytes(content)
            (root / name).chmod(0o755 if name != "configure.ac" else 0o644)
        invoke(root)
        patched = {name: (root / name).read_bytes() for name in NAMES}
        assert patched["config.sub"].count(b"macos* | makos*") == 1
        for name in ("configure", "configure.ac"):
            assert patched[name].count(b"*-*-makos*)") == 2
            assert b"ac_sys_system=MakOS" in patched[name]
            assert b"*-*-makos*)\n\t\t_host_ident=$host_cpu\n" in patched[name]
        # Every per-file interrupted state, including the exact old failure:
        # configure/configure.ac patched, config.sub still upstream.
        for states in itertools.product((False, True), repeat=3):
            for name, state in zip(NAMES, states):
                (root / name).write_bytes((patched if state else originals)[name])
            (root / "config.sub.rej").write_text("preserved old failure\n")
            invoke(root)
            invoke(root)
            assert {n: (root / n).read_bytes() for n in NAMES} == patched
            assert (root / "config.sub.rej").read_text() == "preserved old failure\n"
            assert (root / "configure").stat().st_mode & 0o777 == 0o755
            assert (root / "configure.ac").stat().st_mode & 0o777 == 0o644
        # Unsupported drift in ANY file cannot partially publish other files.
        for corrupt in NAMES:
            for name in NAMES:
                (root / name).write_bytes(originals[name])
            (root / corrupt).write_bytes(originals[corrupt].replace(
                b"macos*", b"localos*"
            ).replace(b"ac_sys_system=Darwin", b"ac_sys_system=Local"))
            before = {n: (root / n).read_bytes() for n in NAMES}
            invoke(root, ok=False)
            assert {n: (root / n).read_bytes() for n in NAMES} == before
        for name in NAMES:
            (root / name).write_bytes(originals[name])
        (root / "config.sub").rename(root / "external")
        (root / "config.sub").symlink_to(root / "external")
        invoke(root, ok=False)
        assert (root / "external").read_bytes() == originals["config.sub"]


parser = argparse.ArgumentParser()
parser.add_argument("--archive", type=Path, help="require SHA-verified full upstream replay")
args = parser.parse_args()
check({"config.sub": CONFIG_SUB.encode(), "configure.ac": CONFIGURE.encode(),
       "configure": CONFIGURE.encode()})
if args.archive:
    lock = dict(line.split("=", 1) for line in (PORT / "source.lock").read_text().splitlines())
    assert hashlib.sha256(args.archive.read_bytes()).hexdigest() == lock["CPYTHON_SHA256"]
    with tarfile.open(args.archive) as archive:
        check({name: archive.extractfile(f"Python-{lock['CPYTHON_VERSION']}/{name}").read()
               for name in NAMES})
print("MAKOS_CPYTHON_PATCH_REPLAY_OK fresh=1 interrupted_states=8 rerun=1 "
      "drift=fail-closed symlink=denied archive=" + ("sha256-verified" if args.archive else "not-requested"))
