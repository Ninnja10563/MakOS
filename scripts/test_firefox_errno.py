#!/usr/bin/env python3
"""Behavioral regression tests for Firefox's MakOS Rust errno ABI staging."""

from __future__ import annotations

import hashlib
import os
import re
import shutil
import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PORT = ROOT / "ports" / "firefox"
PREPARE = PORT / "prepare-rust-errno.sh"
RUST_PATCH = PORT / "rust-patches" / "errno-0.3.8-makos.patch"
GECKO_PATCH = PORT / "patches" / "0059-rust-errno-makos-accessor.patch"


def require(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)


def tree_identity(root: Path) -> tuple[tuple[str, str], ...]:
    return tuple(
        (str(path.relative_to(root)), hashlib.sha256(path.read_bytes()).hexdigest())
        for path in sorted(root.rglob("*"))
        if path.is_file()
    )


def errno_source(root: Path, version: str = "0.3.8") -> None:
    (root / "src").mkdir(parents=True)
    (root / "Cargo.toml").write_text(
        "[package]\n"
        'name = "errno"\n'
        f'version = "{version}"\n'
    )
    (root / "README.md").write_text("upstream-v1\n")
    (root / "src" / "unix.rs").write_text(
        'extern "C" {\n'
        "    #[cfg_attr(\n"
        "        any(\n"
        '            target_os = "linux",\n'
        '            target_os = "hurd",\n'
        '            target_os = "redox",\n'
        '            target_os = "dragonfly"\n'
        "        ),\n"
        '        link_name = "__errno_location"\n'
        "    )]\n"
        '    #[cfg_attr(target_os = "aix", link_name = "_Errno")]\n'
        "    fn errno_location() -> *mut i32;\n"
        "}\n"
    )


def run_prepare(source: Path, stage: Path, *, success: bool = True) -> subprocess.CompletedProcess[str]:
    environment = {
        **os.environ,
        "MAKOS_FIREFOX_ERRNO_SOURCE_DIR": str(source),
        "MAKOS_FIREFOX_ERRNO_STAGE_DIR": str(stage),
        "MAKOS_FIREFOX_ERRNO_PATCH_FILE": str(RUST_PATCH),
    }
    result = subprocess.run(
        [str(PREPARE)],
        env=environment,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    require(
        (result.returncode == 0) == success,
        f"prepare-rust-errno exit {result.returncode}: {result.stderr}",
    )
    return result


def portable_nm() -> str | None:
    for name in ("llvm-nm", "/opt/homebrew/opt/llvm/bin/llvm-nm", "nm"):
        candidate = shutil.which(name)
        if not candidate:
            continue
        version = subprocess.run(
            [candidate, "--version"],
            check=False,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
        ).stdout
        if "LLVM" in version or "GNU" in version:
            return candidate
    return None


rust_patch = RUST_PATCH.read_text()
require(
    rust_patch.count('+            target_os = "makos",') == 1,
    "errno patch must add MakOS to exactly one accessor selection",
)
makos = rust_patch.index('+            target_os = "makos",')
accessor = rust_patch.find('link_name = "__errno_location"', makos)
require(
    accessor != -1 and accessor - makos < 240,
    "MakOS errno cfg must select musl __errno_location",
)
require(
    "rsync -a --delete --checksum" in PREPARE.read_text()
    and "patch -s -R --dry-run" not in PREPARE.read_text(),
    "errno staging must reconstruct instead of trusting reverse patch applicability",
)

with tempfile.TemporaryDirectory(prefix="makos-firefox-errno-test-") as directory:
    fixture = Path(directory)
    source = fixture / "source"
    stage = fixture / "stage"
    errno_source(source)

    first = run_prepare(source, stage)
    require(
        first.stdout.strip()
        == "MAKOS_FIREFOX_ERRNO_OK version=0.3.8 accessor=__errno_location tls=musl checksum-safe=1",
        "errno stage marker changed",
    )
    patched = (stage / "src" / "unix.rs").read_text()
    require(
        re.search(
            r'target_os = "makos"[\s\S]{0,240}link_name = "__errno_location"',
            patched,
        )
        is not None,
        "staged errno source does not select __errno_location",
    )

    (stage / "stale-extra").write_text("must disappear\n")
    (stage / "src" / "unix.rs.orig").write_text("must disappear\n")
    (source / "README.md").write_text("upstream-v2\n")
    second = run_prepare(source, stage)
    require(not (stage / "stale-extra").exists(), "stale stage file survived")
    require(not (stage / "src" / "unix.rs.orig").exists(), ".orig file survived")
    require((stage / "README.md").read_text() == "upstream-v2\n", "upstream refresh lost")
    second_identity = tree_identity(stage)
    run_prepare(source, stage)
    require(tree_identity(stage) == second_identity, "identical restage is not deterministic")

    (source / "Cargo.toml").write_text(
        "[package]\nname = \"errno\"\nversion = \"0.3.9\"\n"
    )
    refused = run_prepare(source, stage, success=False)
    require("expected errno 0.3.8" in refused.stderr, "wrong version did not fail closed")

    gecko = fixture / "gecko"
    gecko.mkdir()
    (gecko / "Cargo.lock").write_text(
        '[[package]]\nname = "prior"\nversion = "1.0.0"\n\n'
        '[[package]]\nname = "errno"\nversion = "0.3.8"\n'
        'source = "registry+https://github.com/rust-lang/crates.io-index"\n'
        'checksum = "a258e46cdc063eb8519c00b9fc845fc47bcfca4130e2f08e88665ceda8474245"\n'
        'dependencies = [\n "libc",\n "windows-sys",\n]\n'
    )
    (gecko / "Cargo.toml").write_text(
        '[patch.crates-io]\nlibc = { path = "../libc-makos" }\n'
        "# code and will fail the build in unwanted cases.\n"
        'cmake = { path = "build/rust/cmake" }\n'
        'vcpkg = { path = "build/rust/vcpkg" }\n'
        'getrandom_0_3 = { package = "getrandom", path = "../getrandom-makos" }\n'
    )
    subprocess.run(["git", "apply", str(GECKO_PATCH)], cwd=gecko, check=True)
    lock = (gecko / "Cargo.lock").read_text()
    errno_tail = lock.split('name = "errno"', 1)[1]
    errno_entry = errno_tail.split("[[package]]", 1)[0]
    require('version = "0.3.8"' in errno_entry, "errno lock entry version changed")
    require("source =" not in errno_entry, "errno lock entry retained registry source")
    require("checksum =" not in errno_entry, "errno lock entry retained checksum")
    cargo = (gecko / "Cargo.toml").read_text()
    require(
        cargo.count('errno = { path = "../errno-makos" }') == 1,
        "Gecko errno path override missing or duplicated",
    )

source_evidence = "fixture"
real_source_value = os.environ.get("MAKOS_FIREFOX_ERRNO_SOURCE_DIR")
if real_source_value:
    real_source = Path(real_source_value)
    require((real_source / "Cargo.toml").is_file(), "required errno source missing")
    require('version = "0.3.8"' in (real_source / "Cargo.toml").read_text(), "required errno source version wrong")
    with tempfile.TemporaryDirectory(prefix="makos-firefox-errno-source-") as directory:
        checked_source = Path(directory) / "errno"
        shutil.copytree(real_source, checked_source)
        subprocess.run(
            ["patch", "-s", "-d", str(checked_source), "-p1", "-i", str(RUST_PATCH)],
            check=True,
        )
        checked_unix = (checked_source / "src" / "unix.rs").read_text()
        require(
            re.search(
                r'target_os = "makos"[\s\S]{0,240}link_name = "__errno_location"',
                checked_unix,
            )
            is not None,
            "required errno source does not accept MakOS accessor patch",
        )
    source_evidence = "required"

nm = portable_nm()
libc_evidence = "skipped"
libc_value = os.environ.get("MAKOS_FIREFOX_ERRNO_LIBC")
default_libc = ROOT / "build/ports/firefox/sysroot-runtime/usr/lib/libc.so"
libc = Path(libc_value) if libc_value else default_libc
if libc_value or libc.is_file():
    require(libc.is_file(), "required MakOS libc fixture missing")
    require(nm is not None, "GNU/LLVM nm required for MakOS libc fixture")
    symbols = subprocess.run(
        [nm, "--dynamic", "--defined-only", str(libc)],
        check=True,
        text=True,
        stdout=subprocess.PIPE,
    ).stdout
    require(re.search(r"\b[TW] __errno_location$", symbols, re.MULTILINE) is not None, "libc lacks __errno_location")
    require(re.search(r"\b[TW] errno_location$", symbols, re.MULTILINE) is None, "libc exports misleading errno_location")
    libc_evidence = "performed"

object_evidence = "skipped"
object_value = os.environ.get("MAKOS_FIREFOX_ERRNO_OBJECT")
if object_value:
    object_path = Path(object_value)
    require(object_path.is_file(), "required patched errno object missing")
    require(nm is not None, "GNU/LLVM nm required for errno object fixture")
    undefined = subprocess.run(
        [nm, "-A", "-u", str(object_path)],
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
    ).stdout
    require(re.search(r"\b__errno_location$", undefined, re.MULTILINE) is not None, "patched errno object lacks __errno_location import")
    require(re.search(r"(^|\s)errno_location$", undefined, re.MULTILINE) is None, "patched errno object still imports errno_location")
    object_evidence = "performed"

if os.environ.get("MAKOS_FIREFOX_ERRNO_REQUIRE_FIXTURES") == "1":
    require(source_evidence == "required", "required real errno source check skipped")
    require(libc_evidence == "performed", "required libc ABI check skipped")
    require(object_evidence == "performed", "required object ABI check skipped")

print(
    "MAKOS_FIREFOX_ERRNO_TEST_OK "
    "stage=behavioral-twice,stale-deleted,upstream-refreshed,wrong-version-denied "
    "cargo_patch=applied-lock-source-less,path-override "
    f"source={source_evidence} libc={libc_evidence} object={object_evidence}"
)
