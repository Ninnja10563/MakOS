#!/usr/bin/env python3
"""Execute production AArch64 EL0-entry policy with real host page tables.

Only physical-memory allocation and VM-state storage are host adapters. The
page-table walk, context predicate, and executable-reservation predicate are
extracted from the kernel; the reservation table is the production Rust crate.
This is a policy regression, not execution of AArch64 instructions or Firefox.
"""

from pathlib import Path
import re
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
ARCH = (ROOT / "kernel/src/arch/aarch64.rs").read_text()
VM = (ROOT / "kernel/src/aarch64_vm.rs").read_text()


def item(source: str, start: str) -> str:
    offset = source.index(start)
    opening = source.index("{", offset)
    depth = 1
    end = opening + 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[offset:end]


constants = "\n".join(
    re.findall(r"^(?:pub )?const [A-Z_0-9]+: u64 = .*;$", ARCH.split("const GICD_CTLR", 1)[0], re.M)
)
context_guard = item(ARCH, "fn user_context_entry_valid(")
functions = "\n".join(
    item(ARCH, declaration)
    for declaration in (
        "fn user_instruction_mapping_in(",
        "fn user_address_executable_in(",
        "pub fn user_address_executable(",
        "fn user_instruction_pointer_valid_in(",
        "fn user_stack_pointer_valid_in(",
        "fn user_page_slot(",
        "fn user_page_slot_from_low(",
        "fn table_child(",
    )
)
mapping = "#[derive(Clone, Copy, Debug, Eq, PartialEq)]\n" + item(ARCH, "enum UserInstructionMapping")
context = "#[repr(C)]\n#[derive(Clone, Copy)]\n" + item(ARCH, "pub(crate) struct UserContext")

# Protect the actual entry wiring as well as the extracted predicate. Hardware
# ERET and IRQ instructions remain in the kernel and are not host-simulated.
entry = item(ARCH, "pub(crate) fn enter_user_context(")
assert "user_context_entry_valid(context, active_root)" in entry
assert 'crate::fatal("AArch64 EL0 entry precondition failed")' in entry
assert "start_scheduler_timer();" in entry
assert "aarch64_enter_user_context(context)" in entry
assert "disable_interrupts();" in entry
assert "stop_scheduler_timer();" in entry
assert entry.index('crate::fatal("AArch64 EL0 entry precondition failed")') < entry.index(
    "MAKOS_AARCH64_HIGH_EL0_ENTRY_OK"
) < entry.index("aarch64_enter_user_context(context)")
assert "HIGH_USER_ENTRY_REPORTED_MASK.fetch_or" in entry
assert "context.elr >= USER_MMAP_BASE" in entry
assert "context.elr & (PAGE_SIZE - 1) == 0" in entry
assert "crate::aarch64_process::current_tid()" in entry
assert "USER_IMAGE_LIMIT" not in re.sub(r"//[^\n]*", "", context_guard)
signal_predicate = item(ARCH, "pub fn user_address_executable(")
assert "user_address_executable_in(" in signal_predicate
assert "handle_page_fault" not in signal_predicate
assert "executable_region_in" not in signal_predicate
assert "handle_page_fault" not in functions

vm_excerpt = "\n".join(
    re.findall(r"^pub const (?:PAGE_SIZE|PROT_READ|PROT_WRITE|PROT_EXEC):.*;$", VM, re.M)
) + "\n" + "\n".join(
    item(VM, declaration)
    for declaration in (
        "struct ProcessVm",
        "impl ProcessVm",
        "pub(crate) fn executable_region_in(",
    )
)
assert "handle_page_fault" not in vm_excerpt

shared_build = (ROOT / "ports/musl/build-shared-makos.sh").read_text()
assert '-fno-stack-protector -fPIC \\\n\t-c "$port_dir/dynamic-probe.c"' in shared_build

with tempfile.TemporaryDirectory(prefix="makos-el0-entry-") as temporary:
    directory = Path(temporary)
    library = directory / "libmakos_vm_space.rlib"
    subprocess.run(
        ["rustc", "--edition=2024", "--crate-name", "makos_vm_space", "--crate-type", "rlib",
         str(ROOT / "crates/vm-space/src/lib.rs"), "-o", str(library)],
        check=True,
    )
    production = constants + "\n" + mapping + "\n" + context + "\n" + functions
    (directory / "arch_policy.rs").write_text(production + "\n" + context_guard)
    (directory / "vm_policy.rs").write_text(vm_excerpt)
    (directory / "test.rs").write_text((ROOT / "scripts/test_aarch64_el0_entry.rs").read_text())

    def compile_fixture(name: str) -> Path:
        binary = directory / name
        subprocess.run(
            ["rustc", "--edition=2024", "--test", str(directory / "test.rs"),
             "--extern", f"makos_vm_space={library}", "-o", str(binary)],
            check=True,
        )
        return binary

    subprocess.run([str(compile_fixture("el0-entry-tests")), "--test-threads=1"], check=True, timeout=30)

    # Restore the precise old image-range policy in a generated copy only.
    # A valid loader/thread context must fail this same behavioral regression.
    old_guard = """
fn user_context_entry_valid(context: &UserContext, active_root: u64) -> bool {
    active_root != 0 && context.ttbr0 == active_root
        && (USER_ADDRESS_BASE..USER_IMAGE_LIMIT).contains(&context.elr)
        && (matches!(context.sp_el0, LEGACY_USER_STACK_TOP | USER_STACK_TOP)
            || user_stack_pointer_valid_in(context.ttbr0, context.sp_el0))
        && context.spsr & !0xf000_0000 == 0
}
"""
    (directory / "arch_policy.rs").write_text(production + "\n" + old_guard)
    negative = subprocess.run(
        [str(compile_fixture("el0-entry-old-image-guard")), "--exact", "loader_thread_entry_uses_executable_mapping"],
        capture_output=True, text=True, timeout=15,
    )
    if negative.returncode == 0 or "legitimate dynamic-loader thread entry rejected" not in negative.stdout + negative.stderr:
        raise SystemExit(f"EL0 old-guard negative control did not reproduce loader rejection:\n{negative.stdout}{negative.stderr}")

print("MAKOS_AARCH64_EL0_ENTRY_HOST_OK policy=production-code page_tables=four-level "
      "loader_pc=0x280adc14 permissions=rx,pxn,af isolation=root-bound "
      "lazy=rx-reservation-only stack,spsr=preserved negative_control=old-guard-rejected")
