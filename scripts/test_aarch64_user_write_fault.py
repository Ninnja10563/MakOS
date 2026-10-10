#!/usr/bin/env python3
"""Exercise production lazy-read validation for AArch64 write buffers.

The production VM, region table, read preparation helper and resident-PTE
validation are compiled unchanged. Existing VM-test adapters provide physical
pages and immutable package I/O; the final copy/TTY sink is a host adapter.
This is host functional evidence, not guest execution or Firefox qualification.
"""

from pathlib import Path
import re
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


architecture = (ROOT / "kernel/src/arch/aarch64.rs").read_text()
production_vm = (ROOT / "kernel/src/aarch64_vm.rs").read_text()
helper = item(architecture, "fn fault_in_user_read_buffer(")
readable = item(architecture, "pub(crate) fn user_range_readable(")
assert "crate::aarch64_vm::fault_in_range(" in helper
assert "crate::aarch64_process::current_pid()" in helper

# Check every production write-dispatch route, not only the standalone helper.
# Authorization still precedes constructing any slice or invoking a sink.
branches = [
    "SYS_FILE_WRITE if crate::vfs::is_pipe_owned(frame.registers[0]) =>",
    "SYS_FILE_WRITE if crate::aarch64_socket::is_owned(frame.registers[0]) =>",
    "SYS_FILE_WRITE =>",
    "fn tty_write(",
]
for branch in branches:
    body = item(architecture, branch)
    preparation = body.index("!fault_in_user_read_buffer(address, length)")
    permission = body.index("!user_range_readable(address, length)")
    copy = body.index("core::slice::from_raw_parts(")
    assert preparation < permission < copy, branch
    assert body[preparation:permission].rstrip().endswith("||"), branch
    assert body.count("fault_in_user_read_buffer(") == 1, branch

normal_write = item(architecture, "SYS_FILE_WRITE =>")
guard = re.search(
    r"\bif\s+(!fault_in_user_read_buffer\(address, length\)\s*\|\|\s*"
    r"!user_range_readable\(address, length\))\s*\{", normal_write
)
assert guard is not None, "normal write must reject either failed validation"
read_check = """
use crate::user_read_adapters::*;
pub fn validated_read_buffer(address: u64, length: usize) -> bool {
    !(REJECTION_CONDITION)
}
""".replace("REJECTION_CONDITION", guard.group(1))

constants = "\n".join(
    re.search(rf"^const {name}:.*;$", architecture, re.MULTILINE).group(0)
    for name in ("PAGE_SIZE", "TABLE_DESCRIPTOR", "AP_USER_RW", "AP_USER_RO",
                 "USER_ADDRESS_LIMIT", "MAX_USER_COPY")
)

# Reuse the existing page/frame/package adapters rather than maintaining a
# second independent model of the VM. Its unrelated concurrency tests are not
# included or changed by this runner.
adapters = (ROOT / "scripts/test_aarch64_vm_fault_commit.rs").read_text().split(
    "\n#[test]\n", 1
)[0]
for name in ("USER_ADDRESS_BASE", "USER_HEAP_BASE", "USER_HEAP_LIMIT",
             "USER_MMAP_BASE", "USER_MMAP_LIMIT", "USER_STACK_TOP"):
    declared = re.search(rf"pub const {name}:.*;$", architecture, re.MULTILINE).group(0)
    assert declared in adapters, f"VM adapter address constant drift: {name}"
adapters = adapters.replace(
    "mod aarch64_process {",
    "mod aarch64_process {\n"
    "    pub fn current_pid() -> u64 {\n"
    "        crate::CURRENT_PID.load(std::sync::atomic::Ordering::SeqCst)\n"
    "    }",
    1,
)
adapters = adapters.replace(
    "mod arch {", 'mod arch {\n    include!("read_production.rs");', 1
)
test_source = adapters + (ROOT / "scripts/test_aarch64_user_write_fault.rs").read_text()

with tempfile.TemporaryDirectory(prefix="makos-user-write-fault-") as temporary:
    directory = Path(temporary)
    library = directory / "libmakos_vm_space.rlib"
    subprocess.run(
        ["rustc", "--edition=2024", "--crate-name", "makos_vm_space", "--crate-type", "rlib",
         str(ROOT / "crates/vm-space/src/lib.rs"), "-o", str(library)],
        check=True,
    )
    (directory / "test.rs").write_text(test_source)

    def compile_fixture(name: str, prepare: str, permissions: str,
                        vm: str = production_vm) -> Path:
        (directory / "read_production.rs").write_text(
            constants + "\n" + prepare + "\n" + permissions + "\n" + read_check
        )
        (directory / "vm_production.rs").write_text(vm)
        binary = directory / name
        subprocess.run(
            ["rustc", "--edition=2024", "--test", str(directory / "test.rs"),
             "--extern", f"makos_vm_space={library}", "-o", str(binary)],
            check=True,
        )
        return binary

    subprocess.run(
        [str(compile_fixture("user-write-tests", helper, readable)), "--test-threads=1"],
        check=True, timeout=30,
    )

    def reject_control(name: str, prepare: str, permissions: str,
                       test: str, diagnostic: str, vm: str = production_vm) -> None:
        binary = compile_fixture(name, prepare, permissions, vm)
        result = subprocess.run(
            [str(binary), "--exact", test], capture_output=True, text=True, timeout=15
        )
        output = result.stdout + result.stderr
        if result.returncode == 0 or diagnostic not in output:
            raise SystemExit(f"{name} did not reject the restored defect:\n{output}")

    # Generated copies only: restore the pre-repair resident-only validation.
    old_preparation = """
fn fault_in_user_read_buffer(_address: u64, _length: usize) -> bool {
    cached_active_root() != 0
}
"""
    reject_control(
        "resident-only", old_preparation, readable,
        "cold_immutable_package_buffer_reaches_one_complete_write",
        "cold authorized package buffer must be populated before copying",
    )
    # fault_in_range may find an already-resident page. It is not a replacement
    # for checking that page's live EL0 read permissions before copying.
    no_permission = """
pub(crate) fn user_range_readable(_address: u64, _length: usize) -> bool { true }
"""
    reject_control(
        "missing-resident-permission", helper, no_permission,
        "resident_prot_none_is_rejected_after_population_preparation",
        "resident PROT_NONE must not reach the copy adapter",
    )
    # The source helper must retain the existing fault-in length bound. An
    # invalid large request must not perform even its first package read.
    fault_range = item(production_vm, "pub fn fault_in_range(")
    cap = "|| length > 16 * 1024 * 1024"
    assert fault_range.count(cap) == 1
    no_cap = production_vm.replace(fault_range, fault_range.replace(cap, ""))
    reject_control(
        "missing-fault-length-bound", helper, readable,
        "oversized_buffer_does_not_allocate_or_read_its_first_page",
        "oversized syscall buffer must not allocate or perform I/O",
        no_cap,
    )

print("MAKOS_AARCH64_USER_WRITE_FAULT_HOST_OK vm=production-code "
      "read_validation=production-code package=host-adapter copy=host-adapter "
      "dispatch=3-file-write,tty-write "
      "negative_controls=resident-only,missing-permission,missing-length-bound")
