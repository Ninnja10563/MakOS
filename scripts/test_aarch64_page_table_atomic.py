#!/usr/bin/env python3
"""Host-execute production shared-root page-table mutation and access policy.

Only frame storage, DAIF masking, serial/fatal output and architectural TLB
instructions are adapted. The real spin guard, hierarchy walk/allocation,
map-if-absent, strict map, unmap, protect and access policy are extracted. This
is a Linux/Darwin host regression, not execution of AArch64 MMU instructions.
"""

from pathlib import Path
import re
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[1]
ARCH = (ROOT / "kernel/src/arch/aarch64.rs").read_text()
EXPORTS = (ROOT / "kernel/src/arch/mod.rs").read_text()
for name in ("LocalInterruptMask", "map_user_page_permissions_if_absent_in",
             "user_page_access_permitted_in"):
    assert name in EXPORTS, f"missing architecture facade export: {name}"


def item(source: str, declaration: str) -> str:
    start = source.index(declaration)
    opening = source.index("{", start)
    depth = 1
    end = opening + 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[start:end]


mask = item(ARCH, "impl LocalInterruptMask")
unmask = item(ARCH, "impl Drop for LocalInterruptMask")
guard = item(ARCH, "impl UserPageTableGuard")
release = item(ARCH, "impl Drop for UserPageTableGuard")
assert '"mrs {saved}, daif"' in mask
assert '"msr daifset, #0xf"' in mask
assert '"msr daif, {saved}"' in unmask
assert "self.saved_daif" in unmask and '"isb"' in unmask
assert guard.index("LocalInterruptMask::acquire()") < guard.index("compare_exchange_weak")
assert "Ordering::Acquire, Ordering::Relaxed" in guard
assert "_interrupts: LocalInterruptMask" in item(ARCH, "struct UserPageTableGuard")
assert "USER_PAGE_TABLE_LOCK.store(false, Ordering::Release)" in release

commit = item(ARCH, "pub fn map_user_page_permissions_if_absent_in(")
assert commit.index("UserPageTableGuard::acquire()") < commit.index("table_child(root, 0)")
assert commit.count("UserPageTableGuard::acquire()") == 1
assert "return Err(entry);" in commit and "Ok(())" in commit
assert "crate::aarch64_process" not in commit and "crate::fs" not in commit
assert "Ordering::Acquire" in item(ARCH, "fn read_table_entry(")
assert "Ordering::Release" in item(ARCH, "unsafe fn write_table_entry(")
for declaration in ("pub fn unmap_user_page_in(", "pub fn protect_user_page_permissions_in("):
    body = item(ARCH, declaration)
    assert body.index("UserPageTableGuard::acquire()") < body.index("user_page_slot(")
    assert "write_table_entry(" in body and "invalidate_user_page_if_active(" in body
strict = item(ARCH, "pub fn map_user_page_permissions_in(")
assert 'crate::fatal("duplicate AArch64 user-page mapping")' in strict
assert "MAKOS_AARCH64_DUPLICATE_MAPPING" in strict
assert "root={:#x} va={:#x} candidate_pa={:#x} existing_entry={:#x}" in strict
assert "crate::aarch64_process" not in strict

# Preserve the actual broadcast TLB maintenance, not just the host hook.
invalidate = item(ARCH, "fn invalidate_user_page_if_active(")
assert '"dsb ishst"' in invalidate and '"tlbi vae1is, {page}"' in invalidate
assert invalidate.index('"dsb ishst"') < invalidate.index('"tlbi vae1is, {page}"')
assert invalidate.index('"tlbi vae1is, {page}"') < invalidate.index('"dsb ish"')

constants = "\n".join(re.findall(
    r"^(?:pub )?const [A-Z_0-9]+: u64 = .*;$",
    ARCH.split("const GICD_CTLR", 1)[0], re.M,
))
declarations = (
    "struct UserPageTableGuard",
    "impl UserPageTableGuard",
    "impl Drop for UserPageTableGuard",
    "pub fn map_user_page_in(",
    "pub fn map_user_page_permissions_in(",
    "pub fn map_user_page_permissions_if_absent_in(",
    "pub fn unmap_user_page_in(",
    "pub fn protect_user_page_in(",
    "pub fn protect_user_page_permissions_in(",
    "pub fn user_page_physical_in(",
    "pub fn user_page_access_permitted_in(",
    "fn user_instruction_mapping_in(",
    "fn user_page_slot(",
    "fn user_page_slot_from_low(",
    "fn table_child(",
    "fn read_table_entry(",
    "fn allocate_table(",
    "unsafe fn write_table_entry(",
)
production = constants + "\n#[derive(Clone, Copy, Debug, Eq, PartialEq)]\n"
production += item(ARCH, "enum UserInstructionMapping") + "\n"
production += "\n\n".join(item(ARCH, declaration) for declaration in declarations)
assert "asm!(" not in production

with tempfile.TemporaryDirectory(prefix="makos-page-table-atomic-") as temporary:
    directory = Path(temporary)
    source = directory / "page_tables.rs"
    fixture = directory / "test.rs"
    template = (ROOT / "scripts/test_aarch64_page_table_atomic.rs").read_text()

    def compile_fixture(name: str, implementation: str) -> Path:
        source.write_text(implementation)
        fixture.write_text(template)
        binary = directory / name
        subprocess.run(
            ["rustc", "--edition=2024", "--test", str(fixture), "-o", str(binary)], check=True,
        )
        return binary

    subprocess.run(
        [str(compile_fixture("page-table-tests", production)), "--test-threads=1"],
        check=True, timeout=60,
    )

    # A lenient strict map is not an acceptable repair. The original API must
    # still reject the duplicate and retain the first physical page intact.
    lenient = production.replace(
        'crate::fatal("duplicate AArch64 user-page mapping");',
        'let _ignored_duplicate = entry;',
    )
    assert lenient != production
    negative = subprocess.run(
        [str(compile_fixture("duplicate-guard-removed", lenient)), "--exact", "strict_duplicate_remains_fatal"],
        capture_output=True, text=True, timeout=15,
    )
    if negative.returncode == 0 or "strict duplicate guard missing" not in negative.stdout + negative.stderr:
        raise SystemExit(f"Strict duplicate negative control was not rejected:\n{negative.stdout}{negative.stderr}")

    # Removing only the commit guard must deterministically fail even when a
    # host scheduler happens not to interleave concurrent mapper threads.
    unlocked_commit = commit.replace("    let _guard = UserPageTableGuard::acquire();\n", "")
    assert unlocked_commit != commit
    unlocked = production.replace(commit, unlocked_commit)
    negative = subprocess.run(
        [str(compile_fixture("commit-guard-removed", unlocked)), "--exact", "mutation_span_holds_lock"],
        capture_output=True, text=True, timeout=15,
    )
    if negative.returncode == 0 or "page-table mutation outside IRQ-safe guard" not in negative.stdout + negative.stderr:
        raise SystemExit(f"Unlocked commit negative control was not rejected:\n{negative.stdout}{negative.stderr}")

print("MAKOS_AARCH64_PAGE_TABLE_ATOMIC_HOST_OK implementation=production-code "
      "same_page=one-winner distinct_pages=shared-subtree-isolated "
      "mutations=map,unmap,protect descriptors=atomic-acquire-release "
      "access=el0,wx,upper-table strict_duplicate=fatal "
      "negative_controls=strict-guard,commit-lock")
