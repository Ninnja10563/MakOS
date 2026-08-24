#!/usr/bin/env python3
"""Structural gate for package-store production geometry and kernel routing."""

from __future__ import annotations

import pathlib
import re

import package_layout


ROOT = pathlib.Path(__file__).resolve().parent.parent


def rust_constant(source: str, name: str) -> int:
    match = re.search(rf"pub const {name}: u64 = ([0-9_]+);", source)
    if not match:
        raise AssertionError(f"Rust constant absent: {name}")
    return int(match.group(1).replace("_", ""))


def main() -> int:
    store = (ROOT / "crates/package-store/src/lib.rs").read_text()
    package = (ROOT / "kernel/src/package.rs").read_text()
    filesystem = (ROOT / "kernel/src/fs.rs").read_text()
    makfs4 = (ROOT / "kernel/src/makfs4_volume.rs").read_text()
    vfs = (ROOT / "kernel/src/vfs.rs").read_text()
    graphics = (ROOT / "kernel/src/graphics.rs").read_text()
    aarch64 = (ROOT / "kernel/src/arch/aarch64.rs").read_text()
    sdk_header = (ROOT / "sdk/include/makos.h").read_text()
    sdk_libc = (ROOT / "sdk/libc/makos.c").read_text()

    assert rust_constant(store, "PRODUCTION_BASE_SECTOR") == package_layout.TRANSACTION_BASE_LBA
    assert rust_constant(store, "PRODUCTION_SLOT_SECTORS") == package_layout.TRANSACTION_SLOT_SECTORS
    assert package_layout.TRANSACTION_END_LBA == package_layout.PROFILE_DATA_LBA
    assert "end_lba > PACKAGE_TRANSACTION_BASE_LBA" in filesystem
    assert "end_lba > PACKAGE_TRANSACTION_BASE_LBA" in makfs4
    assert "impl makos_package_store::SectorDevice for DataDisk" in filesystem

    verified = package.index("!verify_manifest")
    routed = package.index("package_transaction_store()")
    assert verified < routed
    for operation in ("persistent.install", "persistent.replace", "persistent.rollback", "persistent.remove"):
        assert operation in package
    assert "decode_dependencies(dependency" in package
    assert 'b"/packages/"' in filesystem and 'b"/payload"' in filesystem
    assert "file.first_lba = info.payload_first_sector" in filesystem
    assert "pub fn read_payload_at" in store
    assert "replace_transaction_packages" in vfs
    assert "runtime_status_text" in graphics
    for syscall in ("SYS_PACKAGE_INSTALL", "SYS_PACKAGE_QUERY", "SYS_PACKAGE_ROLLBACK", "SYS_PACKAGE_REMOVE"):
        assert f"const {syscall}" in aarch64
        assert f"{syscall} =>" in aarch64
    assert "| ABI_FEATURE_PACKAGE_TRANSACTIONS" in aarch64
    for api in ("makos_package_install", "makos_package_query", "makos_package_rollback", "makos_package_remove"):
        assert api in sdk_header and api in sdk_libc
    assert "STATIC_PACKAGE_FILE_LIMIT" in filesystem

    print(
        "MAKOS_PACKAGE_STORE_INTEGRATION_OK layout=1 base_lba="
        f"{package_layout.TRANSACTION_BASE_LBA} slots=2x{package_layout.TRANSACTION_SLOT_SECTORS} "
        "legacy_overlap=denied auth_before_persist=1 dependency_wire=MAKDEP1 "
        "vfs=/packages/NAME/payload settings=status syscalls=install,replace,query,rollback,remove"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
