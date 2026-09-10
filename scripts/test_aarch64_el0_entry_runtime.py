#!/usr/bin/env python3
"""Check the focused dynamic-musl runtime evidence parser, not guest execution."""

from __future__ import annotations

import unittest

import boot_test_aarch64_el0_entry as probe


GOOD = "\n".join((
    "MAKOS_AARCH64_THREAD_AFFINITY_OK tid=11 operation=get mask=0x2 cpu=1",
    "MAKOS_AARCH64_THREAD_AFFINITY_OK tid=12 operation=get mask=0x4 cpu=2",
    "MAKOS_AARCH64_THREAD_AFFINITY_OK tid=13 operation=get mask=0x8 cpu=3",
    "MAKOS_AARCH64_HIGH_EL0_ENTRY_OK cpu=1 tid=11 root=0x4012e000 pc=0x80001000 proof=validated-before-eret",
    "MAKOS_AARCH64_HIGH_EL0_ENTRY_OK cpu=2 tid=12 root=0x4012e000 pc=0x80001000 proof=validated-before-eret",
    "MAKOS_AARCH64_HIGH_EL0_ENTRY_OK cpu=3 tid=13 root=0x4012e000 pc=0x80001000 proof=validated-before-eret",
    "MAKOS_MUSL_EL0_ENTRY_OK loader=musl threads=3 singleton=0x2,0x4,0x8 "
    "tids=11,12,13 pthread_create=0x280b0750 rx=0x80000000 "
    "resume=0x80001000 calls=96 statuses=42,42,42 block=sleep-until",
    probe.REAP_MARKER.decode(),
))


class RuntimeEvidenceTests(unittest.TestCase):
    def test_complete_evidence(self):
        self.assertEqual(probe.validate_output(GOOD), ((11, 12, 13), 0x280B0750, 0x80000000))

    def test_missing_kernel_observation(self):
        with self.assertRaisesRegex(AssertionError, "kernel did not observe"):
            probe.validate_output(GOOD.replace("tid=12 operation=get", "tid=99 operation=get"))

    def test_wrong_cpu_or_affinity(self):
        for bad in ("mask=0x4 cpu=1", "mask=0xe cpu=2"):
            with self.subTest(bad=bad), self.assertRaisesRegex(AssertionError, "kernel did not observe"):
                probe.validate_output(GOOD.replace("mask=0x4 cpu=2", bad))

    def test_duplicate_tid(self):
        with self.assertRaisesRegex(AssertionError, "distinct live TIDs"):
            probe.validate_output(GOOD.replace("tids=11,12,13", "tids=11,11,13"))

    def test_static_linkage(self):
        with self.assertRaisesRegex(AssertionError, "inside musl loader"):
            probe.validate_output(GOOD.replace("pthread_create=0x280b0750", "pthread_create=0x10000750"))

    def test_low_or_invalid_rx(self):
        for bad in ("0x10000000", "0x80000004", "0x3c0000000"):
            with self.subTest(bad=bad), self.assertRaisesRegex(AssertionError, "next high mmap page"):
                probe.validate_output(GOOD.replace("rx=0x80000000", f"rx={bad}"))

    def test_same_page_resume(self):
        with self.assertRaisesRegex(AssertionError, "next high mmap page"):
            probe.validate_output(GOOD.replace("resume=0x80001000", "resume=0x80000004"))

    def test_incomplete_work_or_join(self):
        for original, bad in (("calls=96", "calls=95"), ("statuses=42,42,42", "statuses=42,42,0")):
            with self.subTest(bad=bad), self.assertRaisesRegex(AssertionError, "one complete"):
                probe.validate_output(GOOD.replace(original, bad))

    def test_missing_reap(self):
        with self.assertRaisesRegex(AssertionError, "not reaped"):
            probe.validate_output(GOOD.replace(probe.REAP_MARKER.decode(), ""))

    def test_missing_outer_entry(self):
        with self.assertRaisesRegex(AssertionError, "outer EL0 resume"):
            probe.validate_output("\n".join(line for line in GOOD.splitlines()
                if "HIGH_EL0_ENTRY_OK cpu=2" not in line))

    def test_wrong_outer_cpu_or_tid(self):
        for bad in ("cpu=0 tid=12", "cpu=2 tid=99"):
            with self.subTest(bad=bad), self.assertRaisesRegex(AssertionError, "outer EL0 resume"):
                probe.validate_output(GOOD.replace("HIGH_EL0_ENTRY_OK cpu=2 tid=12",
                    "HIGH_EL0_ENTRY_OK " + bad))

    def test_wrong_outer_pc(self):
        for bad in ("0x10000000", "0x80000ff4", "0x80001004"):
            with self.subTest(bad=bad), self.assertRaisesRegex(AssertionError, "outer EL0 resume"):
                probe.validate_output(GOOD.replace("pc=0x80001000", "pc=" + bad))

    def test_zero_or_unaligned_root(self):
        for bad in ("0x0", "0x4012e004"):
            with self.subTest(bad=bad), self.assertRaisesRegex(AssertionError, "invalid address-space root"):
                probe.validate_output(GOOD.replace("root=0x4012e000", "root=" + bad))

    def test_different_roots(self):
        with self.assertRaisesRegex(AssertionError, "different address-space roots"):
            probe.validate_output(GOOD.replace("root=0x4012e000", "root=0x4012f000", 1))

    def test_missing_blocking_proof(self):
        with self.assertRaisesRegex(AssertionError, "one complete"):
            probe.validate_output(GOOD.replace(" block=sleep-until", ""))

    def test_duplicate_result(self):
        with self.assertRaisesRegex(AssertionError, "one complete"):
            probe.validate_output(GOOD + "\n" + GOOD)

    def test_fatal_always_fails(self):
        for fatal in ("MAKOS_FATAL: AArch64 EL0 entry precondition failed", "EL0 entry rejected cpu=1"):
            with self.subTest(fatal=fatal), self.assertRaisesRegex(AssertionError, "fatal/rejection"):
                probe.validate_output(GOOD + "\n" + fatal)


if __name__ == "__main__":
    unittest.main()
