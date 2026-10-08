#!/usr/bin/env python3
"""Host-only strict BTYPE evidence and immutable fixture wiring regressions."""

from pathlib import Path
import re
import subprocess
import tempfile
import unittest

import boot_test_aarch64_btype as probe
from test_aarch64_el0_entry_runtime import GOOD as EL0_GOOD


ROOT = Path(__file__).resolve().parents[1]
STATE = "tid=1 root=0x4012e000 pc=0x10000080 sp=0x3fffffff0 tls=0x11223344 spsr=0x80000400 btype=1 "
SOURCE = probe.PREFIX + "SOURCE_OK cpu=1 " + STATE + "capture=hardware-timer\n"
ENTRY = probe.PREFIX + "ENTRY_OK cpu=2 " + STATE + "proof=validated-before-eret\n"
TARGET = probe.PREFIX + "TARGET_OK cpu=2 " + STATE.replace("pc=0x10000080", "pc=0x10000084") + "proof=hardware-timer-after-eret\n"
REAP = ("process-reap arch=aarch64 pid=1 status=42 closed_fds=0 closed_tty_fds=3 "
        "closed_surfaces=0 closed_sockets=0 closed_epolls=0 closed_ipc_handles=0 "
        "closed_futex_waiters=0 vm_regions=0 vm_pages=0 reclaimed_frames=73\n")
RESULT = (probe.PREFIX + "OK tid=1 source_cpu=1 target_cpu=2 root=0x4012e000 "
          "spsr=0x80000400 migrations=1 capture=hardware-timer entry=validated-before-eret "
          "resumed=hardware-timer status=42 termination=kernel-after-irq-proof "
          "cleanup=reaped free_balance=1 scope=immutable-boot-probe\n")
GOOD = SOURCE + ENTRY + TARGET + REAP + RESULT + EL0_GOOD


def item(source, declaration):
    start = source.index(declaration)
    end = source.index("{", start) + 1
    depth = 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[start:end]


class EvidenceTests(unittest.TestCase):
    def test_complete_ordered_records(self):
        self.assertEqual(probe.validate_output(GOOD), (1, 0x4012E000))

    def test_crlf(self):
        self.assertEqual(probe.validate_output(GOOD.replace("\n", "\r\r\n")), (1, 0x4012E000))

    def test_both_actual_loop_pcs(self):
        self.assertEqual(probe.validate_output(GOOD.replace("pc=0x10000080", "pc=0x10000084")), (1, 0x4012E000))

    def test_missing_or_duplicate_record(self):
        for record in (SOURCE, ENTRY, TARGET, RESULT):
            for altered in (GOOD.replace(record, ""), GOOD + record, GOOD + record[:30]):
                with self.subTest(record=record), self.assertRaisesRegex(AssertionError, "one complete"):
                    probe.validate_output(altered)

    def test_record_must_be_contiguous_and_line_delimited(self):
        for record in (SOURCE, ENTRY, TARGET, RESULT):
            for replacement in ("partial-prefix" + record, record.rstrip(),
                                record.replace(" root=", "kernel-record\n root=")):
                with self.subTest(record=record), self.assertRaises(AssertionError):
                    probe.validate_output(GOOD.replace(record, replacement))

    def test_order(self):
        for records in ((ENTRY, SOURCE, TARGET, REAP, RESULT),
                        (SOURCE, TARGET, ENTRY, REAP, RESULT),
                        (SOURCE, ENTRY, RESULT, TARGET, REAP)):
            with self.subTest(records=records), self.assertRaisesRegex(AssertionError, "out of order"):
                probe.validate_output("".join(records) + EL0_GOOD)

    def test_no_spsr_sanitizing_or_privilege_bits(self):
        for spsr in ("0x80000000", "0x400", "0x80000800", "0x80000480", "0x80000405"):
            with self.subTest(spsr=spsr), self.assertRaisesRegex(AssertionError, "one complete"):
                probe.validate_output(GOOD.replace("0x80000400", spsr))

    def test_identity_and_saved_state(self):
        for original, replacement in (("tid=1", "tid=2"), ("0x4012e000", "0x4013e000"),
                                      ("0x3fffffff0", "0x3ffffffe0"), ("0x11223344", "0x11223345")):
            for record in (ENTRY, TARGET, RESULT if original in RESULT else ENTRY):
                with self.subTest(record=record, original=original), self.assertRaises(AssertionError):
                    probe.validate_output(GOOD.replace(record, record.replace(original, replacement)))

    def test_source_identity_validity(self):
        for original, replacement in (("tid=1", "tid=0"), ("0x4012e000", "0x0"),
                                      ("0x4012e000", "0x4012e001"), ("0x10000080", "0x10000088"),
                                      ("0x3fffffff0", "0x0"), ("0x11223344", "0x0")):
            with self.subTest(original=original), self.assertRaises(AssertionError):
                probe.validate_output(GOOD.replace(original, replacement))

    def test_reap_is_actual_and_after_target(self):
        for altered in (GOOD.replace(REAP, ""), GOOD.replace(REAP, REAP * 2),
                        REAP + GOOD.replace(REAP, ""), GOOD.replace(REAP, REAP.replace("status=42", "status=0")),
                        GOOD.replace(REAP, REAP.replace("reclaimed_frames=73", "reclaimed_frames=0"))):
            with self.subTest(altered=altered), self.assertRaisesRegex(AssertionError, "reaped"):
                probe.validate_output(altered)

    def test_no_unsupported_skip_fatal_or_missing_original_gate(self):
        for altered in (GOOD + probe.PREFIX + "UNSUPPORTED", GOOD + "MAKOS_FATAL:",
                        GOOD + "MAKOS_PANIC:", GOOD.replace("MAKOS_MUSL_EL0_ENTRY_OK", "REMOVED")):
            with self.subTest(altered=altered), self.assertRaises(AssertionError):
                probe.validate_output(altered)

    def test_probe_wiring_and_no_spsr_injection(self):
        process = (ROOT / "kernel/src/aarch64_process.rs").read_text()
        timer = item(process, "pub(crate) fn btype_probe_from_timer(")
        entry = item(process, "pub(crate) fn observe_btype_probe_entry(")
        run = item(process, "pub fn run_smp_btype_self_test(")
        self.assertIn("crate::arch::UserContext::capture(frame)", timer)
        self.assertIn("migrate_smp_probe_from_exception(2, frame)", timer)
        self.assertIn("exit_from_exception(42, frame)", timer)
        for body in (timer, entry):
            self.assertIn("BTYPE_PROBE_TID.load(Ordering::Acquire)", body)
            self.assertIn("ProcessRole::SmpProbe", body)
            self.assertNotRegex(body, r"\.spsr\s*=(?!=)")
        self.assertIn("counter_deadline_millis(20_000)", run)
        self.assertIn("cleanup_reaped(pid, resource, status)", run)
        self.assertIn("crate::mm::free_frames() != free_before", run)
        assembly = (ROOT / "user/aarch64_btype_probe.S").read_text()
        self.assertIn("msr nzcv, x9", assembly)
        self.assertIn("mov x9, #0x80000000", assembly)
        loop = assembly.split(".org 128", 1)[1].split(".size", 1)[0]
        self.assertEqual(re.findall(r"^\s+(br\s+x\d+)\s*$", loop, re.M), ["br x16", "br x17"])
        self.assertNotRegex(assembly, r"\bmsr\s+(?:spsr|elr)")

    def test_exact_production_saved_state_comparison(self):
        process = (ROOT / "kernel/src/aarch64_process.rs").read_text()
        comparison = item(process, "fn btype_probe_saved_state_equal(")
        adapter = """
mod arch {
    #[derive(Clone, Copy)]
    pub struct UserContext {
        pub registers: [u64; 31], pub spsr: u64, pub sp_el0: u64,
        pub ttbr0: u64, pub tpidr_el0: u64,
    }
}
fn main() {
    let source = arch::UserContext { registers: [0; 31], spsr: 0x80000400,
        sp_el0: 0x3fffffff0, ttbr0: 0x4012e000, tpidr_el0: 0x11223344 };
    assert!(btype_probe_saved_state_equal(&source, &source));
    for register in 0..31 {
        let mut changed = source; changed.registers[register] ^= 1;
        assert!(!btype_probe_saved_state_equal(&source, &changed));
    }
    for bit in 0..64 {
        let mut changed = source; changed.spsr ^= 1 << bit;
        assert!(!btype_probe_saved_state_equal(&source, &changed));
    }
    let mut changed = source; changed.sp_el0 -= 16;
    assert!(!btype_probe_saved_state_equal(&source, &changed));
    let mut changed = source; changed.ttbr0 += 4096;
    assert!(!btype_probe_saved_state_equal(&source, &changed));
    let mut changed = source; changed.tpidr_el0 ^= 1;
    assert!(!btype_probe_saved_state_equal(&source, &changed));
}
"""
        with tempfile.TemporaryDirectory(prefix="makos-btype-state-") as temporary:
            path = Path(temporary)
            source = path / "compare.rs"
            binary = path / "compare"
            source.write_text(comparison + adapter)
            subprocess.run(["rustc", "--edition=2024", str(source), "-o", str(binary)], check=True)
            subprocess.run([str(binary)], check=True, timeout=15)
            # Negative control: a comparator that silently ignores BTYPE must
            # be caught, even though the guest loop will recreate it later.
            source.write_text(comparison.replace("source.spsr == observed.spsr",
                "source.spsr & !0xc00 == observed.spsr & !0xc00") + adapter)
            subprocess.run(["rustc", "--edition=2024", str(source), "-o", str(binary)], check=True)
            result = subprocess.run([str(binary)], capture_output=True, timeout=15)
            self.assertNotEqual(result.returncode, 0, "BTYPE-clearing comparison negative control passed")

    def test_hardware_capture_and_entry_call_sites(self):
        arch = (ROOT / "kernel/src/arch/aarch64.rs").read_text()
        entry = item(arch, "pub(crate) fn enter_user_context(")
        guard = item(entry, "if !user_context_entry_valid(context, active_root)")
        observer = "crate::aarch64_process::observe_btype_probe_entry(context);"
        self.assertIn('crate::fatal("AArch64 EL0 entry precondition failed")', guard)
        self.assertEqual(arch.count(observer), 1)
        self.assertLess(entry.index(guard) + len(guard), entry.index(observer))
        self.assertLess(entry.index(observer), entry.index("aarch64_enter_user_context(context)"))

        irq = item(arch, "fn handle_irq(")
        hook = "crate::aarch64_process::btype_probe_from_timer(frame)"
        self.assertEqual(arch.count(hook), 1)
        timer_blocks = [item(irq[match.start():], "if timer {")
                        for match in re.finditer(r"(?m)^    if timer \{", irq)]
        blocks_with_hook = [block for block in timer_blocks if hook in block]
        self.assertEqual(len(blocks_with_hook), 1, "BTYPE hook must be in the timer-only branch")
        lower_el = item(blocks_with_hook[0], "if kind == 9 {")
        self.assertIn(hook, lower_el, "BTYPE hook must consume only a lower-EL timer IRQ")
        consume = item(lower_el, "if " + hook)
        self.assertIn("return;", consume)
        self.assertLess(lower_el.index(hook), lower_el.index("preempt_from_timer(frame)"))

        main = (ROOT / "kernel/src/main.rs").read_text()
        start = "aarch64_process::run_smp_btype_self_test();"
        self.assertEqual(main.count(start), 1)
        self.assertLess(main.index("aarch64_process::run_smp_forced_migration_self_test();"), main.index(start))
        self.assertLess(main.index(start), main.index("aarch64_process::run_smp_load_balancing_self_test();"))


if __name__ == "__main__":
    unittest.main()
