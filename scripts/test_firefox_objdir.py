#!/usr/bin/env python3
"""Behavioral fixtures for moved Firefox object-directory detection."""

import json
import os
import pathlib
import subprocess
import sys
import tempfile

import firefox_objdir as objdir_module

SCRIPT = pathlib.Path(__file__).with_name("firefox_objdir.py")
BUILD = SCRIPT.parents[1] / "ports/firefox/build-makos.sh"


def invoke(mode: str, obj: pathlib.Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(SCRIPT), mode, str(obj)],
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )


def run(mode: str, obj: pathlib.Path) -> int:
    return invoke(mode, obj).returncode


def write_moved_identity(obj: pathlib.Path, old: pathlib.Path, source: pathlib.Path) -> None:
    obj.mkdir(parents=True, exist_ok=True)
    (obj / "config.status").write_text(
        f"topobjdir = {str(old)!r}\ntopsrcdir = {str(source)!r}\n"
    )
    (obj / ".mozconfig.json").write_text(
        json.dumps(
            {
                "topobjdir": str(old),
                "topsrcdir": str(source),
                "mozconfig": {"topobjdir": str(old)},
            }
        )
    )


def expected_journal(obj: pathlib.Path, old: pathlib.Path) -> dict[str, object]:
    quarantine = obj / "makos-moved-cargo-quarantine"
    return {
        "version": 1,
        "selected_objdir": str(obj.resolve()),
        "old_objdir": str(old.resolve()),
        "mappings": [
            {
                "source": str(obj / "release"),
                "destination": str(quarantine / "host-release"),
            },
            {
                "source": str(obj / "aarch64-unknown-makos" / "release"),
                "destination": str(quarantine / "target-release"),
            },
        ],
    }


build_text = BUILD.read_text()
quarantine_call = '"$build_python" "$obj_check" quarantine-moved-cargo "$obj"'
configure_call = '"$build_python" "$source_dir/mach" configure'
assert build_text.count(quarantine_call) == 1
assert build_text.index(quarantine_call) < build_text.index(configure_call)


with tempfile.TemporaryDirectory() as directory:
    root = pathlib.Path(directory) / "repo ;$[] with spaces"
    root.mkdir()
    obj = root / "obj-aarch64-makos-developer"
    obj.mkdir()
    stale = root / "obj-aarch64-makos"
    source = root / "source"
    write_moved_identity(obj, stale, source)
    assert run("needs-configure", obj) == 10
    assert run("verify", obj) == 1
    host_cache = obj / "release"
    target_cache = obj / "aarch64-unknown-makos" / "release"
    cxx_cache = obj / "widget" / "Unified_cpp_widget0.o"
    host_cache.mkdir()
    target_cache.mkdir(parents=True)
    cxx_cache.parent.mkdir()
    (host_cache / "host.rlib").write_text("host cargo cache\n")
    (target_cache / "target.rlib").write_text("target cargo cache\n")
    cxx_cache.write_text("preserved C++ cache\n")
    migrated = invoke("quarantine-moved-cargo", obj)
    assert migrated.returncode == 0, migrated.stderr
    assert "MAKOS_FIREFOX_CARGO_QUARANTINE_OK moved=2" in migrated.stdout
    quarantine = obj / "makos-moved-cargo-quarantine"
    journal = json.loads((quarantine / "migration.json").read_text())
    assert journal == expected_journal(obj, stale)
    assert (quarantine / "host-release" / "host.rlib").read_text() == "host cargo cache\n"
    assert (quarantine / "target-release" / "target.rlib").read_text() == "target cargo cache\n"
    assert not host_cache.exists() and not target_cache.exists()
    assert cxx_cache.read_text() == "preserved C++ cache\n"
    (quarantine / "target-release").rename(target_cache)
    partial = invoke("quarantine-moved-cargo", obj)
    assert partial.returncode == 0 and "moved=1" in partial.stdout
    assert (quarantine / "host-release" / "host.rlib").is_file()
    assert (quarantine / "target-release" / "target.rlib").is_file()
    repeated = invoke("quarantine-moved-cargo", obj)
    assert repeated.returncode == 0 and "moved=0" in repeated.stdout
    (obj / "config.status").write_text(
        f"topobjdir = {str(obj)!r}\ntopsrcdir = {str(source)!r}\n"
    )
    (obj / ".mozconfig.json").write_text(
        json.dumps(
            {
                "topobjdir": str(obj),
                "topsrcdir": str(source),
                "mozconfig": {"topobjdir": str(obj)},
            }
        )
    )
    assert run("needs-configure", obj) == 10
    (obj / "config").mkdir()
    (obj / "config" / "autoconf.mk").write_text(f"DIST = {obj}/dist\n")
    (obj / "widget" / "makos").mkdir(parents=True)
    (obj / "widget" / "makos" / "backend.mk").write_text(
        "CPPSRCS += $(srcdir)/nsPrintSettingsMakOS.cpp\n"
    )
    (obj / "backend.RecursiveMakeBackend.in").write_text(
        str((root / "source/widget/makos/moz.build").resolve()) + "\n"
    )
    (obj / "backend.RecursiveMakeBackend").write_text(
        "widget/makos/Makefile\nwidget/makos/backend.mk\n"
    )
    configured = invoke("needs-configure", obj)
    assert configured.returncode == 0, (configured.stdout, configured.stderr)
    assert run("verify", obj) == 0
    current_cache = obj / "release"
    current_cache.mkdir()
    (current_cache / "current.rlib").write_text("current\n")
    current = invoke("quarantine-moved-cargo", obj)
    assert current.returncode == 0 and "reason=current-object-directory" in current.stdout
    assert (current_cache / "current.rlib").read_text() == "current\n"
    (obj / "config.status").unlink()
    assert run("needs-configure", obj) == 10
    assert run("verify", obj) == 1
    (obj / "config.status").write_text(
        f"topobjdir = {str(obj)!r}\ntopsrcdir = {str(source)!r}\n"
    )
    (obj / ".mozconfig.json").unlink()
    assert run("needs-configure", obj) == 10
    assert run("verify", obj) == 1
    (obj / ".mozconfig.json").write_text(
        json.dumps(
            {
                "topobjdir": str(obj),
                "topsrcdir": str(source),
                "mozconfig": {"topobjdir": str(obj)},
            }
        )
    )
    (obj / "config.status").write_text(
        f"topobjdir = {str(obj)!r}\ntopobjdir = {str(obj)!r}\n"
        f"topsrcdir = {str(source)!r}\n"
    )
    assert run("needs-configure", obj) == 2
    (obj / "config.status").write_text(
        f"topobjdir = {str(obj)!r}\ntopsrcdir = {str(source)!r}\n"
    )
    (obj / ".mozconfig.json").write_text("{malformed")
    assert run("needs-configure", obj) == 2
    link = root / "linked"
    link.symlink_to(obj, target_is_directory=True)
    assert run("needs-configure", link) == 2
    hostile_parent = root / "hostile ;[] parent"
    hostile = hostile_parent / "obj-aarch64-makos-developer"
    hostile_stale = hostile_parent / "obj-aarch64-makos"
    hostile_source = hostile_parent / "source"
    write_moved_identity(hostile, hostile_stale, hostile_source)
    outside = root / "outside cargo"
    outside.mkdir()
    (outside / "keep").write_text("outside\n")
    (hostile / "release").symlink_to(outside, target_is_directory=True)
    rejected = invoke("quarantine-moved-cargo", hostile)
    assert rejected.returncode == 2
    assert "cache path is a symlink" in rejected.stderr
    assert (outside / "keep").read_text() == "outside\n"

    collision_parent = root / "collision parent"
    collision = collision_parent / "obj-aarch64-makos-developer"
    write_moved_identity(
        collision, collision_parent / "obj-aarch64-makos", collision_parent / "source"
    )
    collision_quarantine = collision / "makos-moved-cargo-quarantine"
    (collision_quarantine / "host-release").mkdir(parents=True)
    arbitrary = invoke("quarantine-moved-cargo", collision)
    assert arbitrary.returncode == 2
    assert "destination lacks valid journal" in arbitrary.stderr, arbitrary.stderr

    destination_parent = root / "destination symlink parent"
    destination_obj = destination_parent / "obj-aarch64-makos-developer"
    write_moved_identity(
        destination_obj,
        destination_parent / "obj-aarch64-makos",
        destination_parent / "source",
    )
    destination_quarantine = destination_obj / "makos-moved-cargo-quarantine"
    destination_quarantine.mkdir()
    (destination_quarantine / "host-release").symlink_to(
        outside, target_is_directory=True
    )
    destination_rejected = invoke("quarantine-moved-cargo", destination_obj)
    assert destination_rejected.returncode == 2
    assert "destination is a symlink" in destination_rejected.stderr

    journal_parent = root / "journal symlink parent"
    journal_obj = journal_parent / "obj-aarch64-makos-developer"
    write_moved_identity(
        journal_obj, journal_parent / "obj-aarch64-makos", journal_parent / "source"
    )
    journal_quarantine = journal_obj / "makos-moved-cargo-quarantine"
    journal_quarantine.mkdir()
    (journal_quarantine / "migration.json").symlink_to(outside / "keep")
    journal_rejected = invoke("quarantine-moved-cargo", journal_obj)
    assert journal_rejected.returncode == 2
    assert "journal is a symlink" in journal_rejected.stderr

    temporary_parent = root / "temporary collision parent"
    temporary_obj = temporary_parent / "obj-aarch64-makos-developer"
    write_moved_identity(
        temporary_obj,
        temporary_parent / "obj-aarch64-makos",
        temporary_parent / "source",
    )
    temporary_source = temporary_obj / "release"
    temporary_source.mkdir()
    temporary_quarantine = temporary_obj / "makos-moved-cargo-quarantine"
    temporary_quarantine.mkdir()
    (temporary_quarantine / ".migration.json.tmp").write_text("collision\n")
    temporary_rejected = invoke("quarantine-moved-cargo", temporary_obj)
    assert temporary_rejected.returncode == 2
    assert temporary_source.is_dir()
    assert not (temporary_quarantine / "migration.json").exists()

    temp_link_parent = root / "temporary symlink parent"
    temp_link_obj = temp_link_parent / "obj-aarch64-makos-developer"
    temp_link_old = temp_link_parent / "obj-aarch64-makos"
    write_moved_identity(temp_link_obj, temp_link_old, temp_link_parent / "source")
    temp_link_source = temp_link_obj / "release"
    temp_link_source.mkdir()
    temp_link_quarantine = temp_link_obj / "makos-moved-cargo-quarantine"
    temp_link_quarantine.mkdir()
    (temp_link_quarantine / ".migration.json.tmp").symlink_to(outside / "keep")
    temp_link_rejected = invoke("quarantine-moved-cargo", temp_link_obj)
    assert temp_link_rejected.returncode == 2
    assert "temporary is a symlink" in temp_link_rejected.stderr
    assert temp_link_source.is_dir()

    crash_parent = root / "exact temporary crash parent"
    crash_obj = crash_parent / "obj-aarch64-makos-developer"
    crash_old = crash_parent / "obj-aarch64-makos"
    write_moved_identity(crash_obj, crash_old, crash_parent / "source")
    (crash_obj / "release").mkdir()
    (crash_obj / "aarch64-unknown-makos" / "release").mkdir(parents=True)
    crash_quarantine = crash_obj / "makos-moved-cargo-quarantine"
    crash_quarantine.mkdir(mode=0o700)
    crash_payload = (
        json.dumps(expected_journal(crash_obj, crash_old), sort_keys=True, separators=(",", ":"))
        + "\n"
    ).encode()
    crash_temporary = crash_quarantine / ".migration.json.tmp"
    crash_temporary.write_bytes(crash_payload)
    crash_temporary.chmod(0o600)
    crash_recovered = invoke("quarantine-moved-cargo", crash_obj)
    assert crash_recovered.returncode == 0, crash_recovered.stderr
    assert "moved=2" in crash_recovered.stdout
    assert not crash_temporary.exists()
    assert json.loads((crash_quarantine / "migration.json").read_text()) == expected_journal(
        crash_obj, crash_old
    )
    assert (crash_quarantine / "host-release").is_dir()
    assert (crash_quarantine / "target-release").is_dir()

    linked_parent = root / "post-link crash parent"
    linked_obj = linked_parent / "obj-aarch64-makos-developer"
    linked_old = linked_parent / "obj-aarch64-makos"
    write_moved_identity(linked_obj, linked_old, linked_parent / "source")
    (linked_obj / "release").mkdir()
    (linked_obj / "aarch64-unknown-makos" / "release").mkdir(parents=True)
    linked_quarantine = linked_obj / "makos-moved-cargo-quarantine"
    linked_quarantine.mkdir(mode=0o700)
    linked_payload = (
        json.dumps(expected_journal(linked_obj, linked_old), sort_keys=True, separators=(",", ":"))
        + "\n"
    ).encode()
    linked_temporary = linked_quarantine / ".migration.json.tmp"
    linked_journal = linked_quarantine / "migration.json"
    linked_temporary.write_bytes(linked_payload)
    linked_temporary.chmod(0o600)
    os.link(linked_temporary, linked_journal)
    assert linked_temporary.stat().st_nlink == 2
    linked_recovered = invoke("quarantine-moved-cargo", linked_obj)
    assert linked_recovered.returncode == 0, linked_recovered.stderr
    assert not linked_temporary.exists()
    assert linked_journal.stat().st_nlink == 1
    assert linked_journal.read_bytes() == linked_payload

    for label, mutate in (
        ("wrong mode", lambda path: path.chmod(0o644)),
        ("noncanonical whitespace", lambda path: path.write_bytes(b" " + path.read_bytes())),
    ):
        invalid_parent = root / f"journal {label} parent"
        invalid_obj = invalid_parent / "obj-aarch64-makos-developer"
        invalid_old = invalid_parent / "obj-aarch64-makos"
        write_moved_identity(invalid_obj, invalid_old, invalid_parent / "source")
        invalid_quarantine = invalid_obj / "makos-moved-cargo-quarantine"
        invalid_quarantine.mkdir(mode=0o700)
        invalid_journal = invalid_quarantine / "migration.json"
        invalid_journal.write_bytes(
            (json.dumps(expected_journal(invalid_obj, invalid_old), sort_keys=True, separators=(",", ":")) + "\n").encode()
        )
        invalid_journal.chmod(0o600)
        mutate(invalid_journal)
        invalid = invoke("quarantine-moved-cargo", invalid_obj)
        assert invalid.returncode == 2, (label, invalid.stdout, invalid.stderr)

    nlink_parent = root / "journal nlink parent"
    nlink_obj = nlink_parent / "obj-aarch64-makos-developer"
    nlink_old = nlink_parent / "obj-aarch64-makos"
    write_moved_identity(nlink_obj, nlink_old, nlink_parent / "source")
    nlink_quarantine = nlink_obj / "makos-moved-cargo-quarantine"
    nlink_quarantine.mkdir(mode=0o700)
    nlink_journal = nlink_quarantine / "migration.json"
    nlink_journal.write_bytes(
        (json.dumps(expected_journal(nlink_obj, nlink_old), sort_keys=True, separators=(",", ":")) + "\n").encode()
    )
    nlink_journal.chmod(0o600)
    os.link(nlink_journal, nlink_quarantine / "unexpected-link")
    assert invoke("quarantine-moved-cargo", nlink_obj).returncode == 2

    parent_barrier_parent = root / "parent barrier crash parent"
    parent_barrier_obj = parent_barrier_parent / "obj-aarch64-makos-developer"
    parent_barrier_old = parent_barrier_parent / "obj-aarch64-makos"
    parent_barrier_source = parent_barrier_parent / "source"
    write_moved_identity(
        parent_barrier_obj, parent_barrier_old, parent_barrier_source
    )
    parent_barrier_host = parent_barrier_obj / "release"
    parent_barrier_target = (
        parent_barrier_obj / "aarch64-unknown-makos" / "release"
    )
    parent_barrier_host.mkdir()
    parent_barrier_target.mkdir(parents=True)
    (parent_barrier_host / "host.rlib").write_text("host before barrier\n")
    (parent_barrier_target / "target.rlib").write_text("target before barrier\n")
    parent_barrier_quarantine = (
        parent_barrier_obj / "makos-moved-cargo-quarantine"
    )
    real_fsync_directory = objdir_module.fsync_directory
    parent_barrier_calls: list[pathlib.Path] = []

    def fail_parent_barrier(path: pathlib.Path) -> None:
        parent_barrier_calls.append(path)
        if path == parent_barrier_obj.resolve():
            raise OSError("injected objdir parent barrier failure")
        real_fsync_directory(path)

    objdir_module.fsync_directory = fail_parent_barrier
    try:
        assert (
            objdir_module.quarantine_moved_cargo(
                parent_barrier_obj.resolve(), parent_barrier_source.resolve()
            )
            == 2
        )
    finally:
        objdir_module.fsync_directory = real_fsync_directory
    assert parent_barrier_calls == [
        parent_barrier_quarantine,
        parent_barrier_obj.resolve(),
    ]
    assert (parent_barrier_quarantine / "migration.json").is_file()
    assert (
        parent_barrier_host / "host.rlib"
    ).read_text() == "host before barrier\n"
    assert (
        parent_barrier_target / "target.rlib"
    ).read_text() == "target before barrier\n"
    assert not (parent_barrier_quarantine / "host-release").exists()
    assert not (parent_barrier_quarantine / "target-release").exists()
    parent_barrier_retry = invoke("quarantine-moved-cargo", parent_barrier_obj)
    assert parent_barrier_retry.returncode == 0, parent_barrier_retry.stderr
    assert "moved=2" in parent_barrier_retry.stdout
    assert (
        parent_barrier_quarantine / "host-release" / "host.rlib"
    ).read_text() == "host before barrier\n"
    assert (
        parent_barrier_quarantine / "target-release" / "target.rlib"
    ).read_text() == "target before barrier\n"

    sync_parent = root / "destination-only fsync parent"
    sync_obj = sync_parent / "obj-aarch64-makos-developer"
    sync_old = sync_parent / "obj-aarch64-makos"
    sync_source = sync_parent / "source"
    write_moved_identity(sync_obj, sync_old, sync_source)
    sync_quarantine = sync_obj / "makos-moved-cargo-quarantine"
    (sync_quarantine / "host-release").mkdir(parents=True)
    (sync_quarantine / "target-release").mkdir()
    (sync_obj / "aarch64-unknown-makos").mkdir()
    sync_journal = sync_quarantine / "migration.json"
    sync_journal.write_bytes(
        (json.dumps(expected_journal(sync_obj, sync_old), sort_keys=True, separators=(",", ":")) + "\n").encode()
    )
    sync_journal.chmod(0o600)
    real_fsync_directory = objdir_module.fsync_directory
    failed_calls: list[pathlib.Path] = []

    def fail_first_fsync(path: pathlib.Path) -> None:
        failed_calls.append(path)
        raise OSError("injected first directory fsync failure")

    objdir_module.fsync_directory = fail_first_fsync
    try:
        assert objdir_module.quarantine_moved_cargo(sync_obj.resolve(), sync_source.resolve()) == 2
    finally:
        objdir_module.fsync_directory = real_fsync_directory
    assert len(failed_calls) == 1
    retry_calls: list[pathlib.Path] = []

    def record_fsync(path: pathlib.Path) -> None:
        retry_calls.append(path)
        real_fsync_directory(path)

    objdir_module.fsync_directory = record_fsync
    try:
        assert objdir_module.quarantine_moved_cargo(sync_obj.resolve(), sync_source.resolve()) == 0
    finally:
        objdir_module.fsync_directory = real_fsync_directory
    assert set(retry_calls) == {
        sync_obj,
        sync_obj / "aarch64-unknown-makos",
        sync_quarantine,
    }

    host_only_parent = root / "host-only cargo parent"
    host_only_obj = host_only_parent / "obj-aarch64-makos-developer"
    host_only_old = host_only_parent / "obj-aarch64-makos"
    write_moved_identity(host_only_obj, host_only_old, host_only_parent / "source")
    (host_only_obj / "release").mkdir()
    host_only_cxx = host_only_obj / "widget" / "host-only.o"
    host_only_cxx.parent.mkdir()
    host_only_cxx.write_text("preserved host-only C++\n")
    host_only = invoke("quarantine-moved-cargo", host_only_obj)
    assert host_only.returncode == 0, host_only.stderr
    assert "moved=1" in host_only.stdout
    host_only_quarantine = host_only_obj / "makos-moved-cargo-quarantine"
    assert (host_only_quarantine / "host-release").is_dir()
    assert not (host_only_quarantine / "target-release").exists()
    assert not (host_only_obj / "aarch64-unknown-makos").exists()
    assert host_only_cxx.read_text() == "preserved host-only C++\n"

    zero_parent = root / "zero cargo parent"
    zero_obj = zero_parent / "obj-aarch64-makos-developer"
    zero_old = zero_parent / "obj-aarch64-makos"
    write_moved_identity(zero_obj, zero_old, zero_parent / "source")
    zero_cxx = zero_obj / "widget" / "zero.o"
    zero_cxx.parent.mkdir()
    zero_cxx.write_text("preserved zero C++\n")
    zero = invoke("quarantine-moved-cargo", zero_obj)
    assert zero.returncode == 0, zero.stderr
    assert "moved=0" in zero.stdout
    zero_quarantine = zero_obj / "makos-moved-cargo-quarantine"
    assert (zero_quarantine / "migration.json").is_file()
    assert not (zero_quarantine / "host-release").exists()
    assert not (zero_quarantine / "target-release").exists()
    assert not (zero_obj / "aarch64-unknown-makos").exists()
    assert zero_cxx.read_text() == "preserved zero C++\n"
    assert not (root / "SENTINEL").exists()
    empty = root / "empty-obj"
    empty.mkdir()
    assert run("needs-configure", empty) == 0
    assert run("verify", empty) == 1
print("MAKOS_FIREFOX_OBJDIR_TEST_OK moved=configure-required,cargo-journaled,host-only current=accepted,no-op,zero-cargo partial,temp-crash,post-link-crash=completed journal=canonical-fd-only,parent-before-rename,retry-fsync collision,symlink=denied cxx=preserved")
