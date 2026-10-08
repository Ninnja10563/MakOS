#!/usr/bin/env python3
"""Execute the production AArch64 VM fault transaction on host threads.

The complete kernel VM implementation and RegionTable are compiled unchanged.
Only physical allocation, architecture page tables, package I/O and shared
objects are host adapters. This is a deterministic concurrency regression, not
execution of AArch64 instructions or Firefox.
"""

from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def item(source: str, start: str) -> str:
    offset = source.index(start)
    opening = source.index("{", offset)
    depth = 1
    end = opening + 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[offset:end]


with tempfile.TemporaryDirectory(prefix="makos-vm-fault-commit-") as temporary:
    directory = Path(temporary)
    library = directory / "libmakos_vm_space.rlib"
    subprocess.run(
        ["rustc", "--edition=2024", "--crate-name", "makos_vm_space", "--crate-type", "rlib",
         str(ROOT / "crates/vm-space/src/lib.rs"), "-o", str(library)],
        check=True,
    )
    production = (ROOT / "kernel/src/aarch64_vm.rs").read_text()
    state_lock = item(production, "fn with_state<")
    assert state_lock.index("LocalInterruptMask::acquire()") < state_lock.index("compare_exchange_weak")
    (directory / "test.rs").write_text((ROOT / "scripts/test_aarch64_vm_fault_commit.rs").read_text())

    def compile_fixture(name: str, source: str) -> Path:
        (directory / "vm_production.rs").write_text(source)
        binary = directory / name
        subprocess.run(
            ["rustc", "--edition=2024", "--test", str(directory / "test.rs"),
             "--extern", f"makos_vm_space={library}", "-o", str(binary)],
            check=True,
        )
        return binary

    subprocess.run([str(compile_fixture("vm-fault-tests", production)), "--test-threads=1"], check=True, timeout=30)

    def reject_control(name: str, source: str, test: str, diagnostic: str) -> None:
        result = subprocess.run(
            [str(compile_fixture(name, source)), "--exact", test],
            capture_output=True, text=True, timeout=15,
        )
        output = result.stdout + result.stderr
        if result.returncode == 0 or diagnostic not in output:
            raise SystemExit(f"{name} did not reject the restored defect:\n{output}")

    # Restore the old strict map-after-load operation in the generated copy
    # only. Two real host threads have already passed their absent snapshot
    # before the package-read barrier releases either one to commit.
    handler = item(production, "pub fn handle_page_fault(")
    install = "crate::arch::map_user_page_permissions_if_absent_in("
    assert handler.count(install) == 1
    legacy_install = """
fn legacy_fault_install(root: u64, page: u64, frame: u64,
    readable: bool, writable: bool, executable: bool) -> Result<(), u64> {
    crate::arch::map_user_page_permissions_in(root, page, frame, readable, writable, executable);
    Ok(())
}
"""
    reject_control(
        "old-check-then-map", production.replace(handler, handler.replace(install, "legacy_fault_install(")) + legacy_install,
        "simultaneous_same_page_faults_commit_one_private_frame", "duplicate AArch64 user-page mapping",
    )

    # A same-range/same-permission replacement must still reload the new file:
    # matching addresses and protection are insufficient VMA identity.
    generation = "&& process.generation == snapshot.generation"
    assert handler.count(generation) == 1
    reject_control(
        "missing-generation-check", production.replace(handler, handler.replace(generation, "")),
        "same_range_same_permissions_file_remap_requires_new_backing", "VMA ABA must not admit original file frame",
    )

    # Merely observing a resident PTE cannot authorize a protection fault.
    access = "crate::arch::user_page_access_permitted_in("
    assert production.count(access) >= 2
    presence_only = """
fn legacy_presence_is_access(root: u64, page: u64, _write: bool, _execute: bool) -> bool {
    crate::arch::user_page_physical_in(root, page).is_some()
}
"""
    reject_control(
        "presence-without-permission", production.replace(access, "legacy_presence_is_access(") + presence_only,
        "incompatible_resident_permissions_do_not_count_as_resolved", "incompatible resident mapping must remain a protection fault",
    )

    load = "let loaded = load_fault_page(snapshot.backing, page, output);"
    assert handler.count(load) == 1
    reject_control(
        "io-under-vm-lock", production.replace(load, "let loaded = with_state(|_| load_fault_page(snapshot.backing, page, output));"),
        "package_io_failure_reclaims_private_frame", "VM interrupt mask/lock must not span package/block I/O",
    )

    failed_read = item(handler, "if !loaded {")
    reject_control(
        "failed-read-ignores-winner", production.replace(failed_read, "if !loaded { return FaultCommit::Resolved(false); }"),
        "failed_private_io_accepts_only_authorized_concurrent_winner", "failed candidate I/O must resolve an authorized winner but not a protection fault",
    )

print("MAKOS_AARCH64_VM_FAULT_COMMIT_HOST_OK vm=production-code regions=production-code "
      "concurrency=host-threads adapters=allocator,page-tables,io "
      "negative_controls=duplicate-map,vma-aba,permission,io-lock,io-winner")
