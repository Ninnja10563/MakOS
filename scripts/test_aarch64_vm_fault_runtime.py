#!/usr/bin/env python3
"""Host regression for strict VM-fault evidence and the bounded guest emitter.

Compiler/parser checks here are not guest execution or Mac/HVF qualification.
"""

from __future__ import annotations

import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest

import boot_test_aarch64_vm_fault as probe
from test_aarch64_el0_entry_runtime import GOOD as EL0_GOOD


ROOT = Path(__file__).resolve().parents[1]
RECORD = (
    "MAKOS_MUSL_VM_FAULT_OK loader=musl threads=3 singleton=0x2,0x4,0x8 "
    "tids=11,12,13 rounds=16 same_page_rounds=16 distinct_pages=48 "
    "table_stride=2097152 base=0x80200000 coherent_checks=288 "
    "first_touch=barrier-released statuses=42,42,42 cleanup=unmapped\n"
)
GOOD = EL0_GOOD + "\n" + RECORD


def item(text: str, declaration: str) -> str:
    start = text.index(declaration)
    end = text.index("{", start) + 1
    depth = 1
    while depth:
        depth += (text[end] == "{") - (text[end] == "}")
        end += 1
    return text[start:end]


class RuntimeEvidenceTests(unittest.TestCase):
    def test_complete_evidence(self):
        self.assertEqual(probe.validate_output(GOOD), ((11, 12, 13), 0x80200000))

    def test_crlf_serial(self):
        self.assertEqual(probe.validate_output(GOOD.replace("\n", "\r\r\n")),
                         ((11, 12, 13), 0x80200000))

    def test_missing_result(self):
        with self.assertRaisesRegex(AssertionError, "one complete"):
            probe.validate_output(EL0_GOOD)

    def test_duplicate_result(self):
        for duplicate in (RECORD, RECORD[:-15]):
            with self.subTest(duplicate=duplicate), self.assertRaisesRegex(AssertionError, "one complete"):
                probe.validate_output(GOOD + duplicate)

    def test_incomplete_work(self):
        changes = (("rounds=16", "rounds=15"),
                   ("same_page_rounds=16", "same_page_rounds=15"),
                   ("distinct_pages=48", "distinct_pages=47"),
                   ("coherent_checks=288", "coherent_checks=287"),
                   ("statuses=42,42,42", "statuses=42,42,0"),
                   ("cleanup=unmapped", "cleanup=leaked"),
                   ("first_touch=barrier-released", "first_touch=serial"),
                   ("table_stride=2097152", "table_stride=4096"))
        for original, replacement in changes:
            with self.subTest(original=original), self.assertRaisesRegex(AssertionError, "one complete"):
                probe.validate_output(EL0_GOOD + "\n" + RECORD.replace(original, replacement))

    def test_different_workers(self):
        for replacement in ("11,12,14", "11,11,13", "13,12,11"):
            with self.subTest(replacement=replacement), self.assertRaisesRegex(AssertionError, "workers differ"):
                probe.validate_output(EL0_GOOD + "\n" + RECORD.replace("11,12,13", replacement))

    def test_invalid_reservation(self):
        for replacement in ("0x10000000", "0x80201000", "0x3c0000000", "0x3bfe00000"):
            with self.subTest(replacement=replacement), self.assertRaisesRegex(AssertionError, "fresh-table span"):
                probe.validate_output(GOOD.replace("base=0x80200000", "base=" + replacement))

    def test_missing_kernel_affinity(self):
        with self.assertRaisesRegex(AssertionError, "kernel did not observe"):
            probe.validate_output(GOOD.replace("tid=12 operation=get", "tid=99 operation=get"))

    def test_missing_original_el0_proof(self):
        for token in ("MAKOS_MUSL_EL0_ENTRY_OK", "MAKOS_MUSL_DYNAMIC_REAP_OK",
                      "MAKOS_AARCH64_HIGH_EL0_ENTRY_OK cpu=2"):
            with self.subTest(token=token), self.assertRaises(AssertionError):
                probe.validate_output(GOOD.replace(token, "REMOVED"))

    def test_fragmented_records_rejected(self):
        for split in (" tids=", " base=", " first_touch=", " cleanup="):
            fragment = RECORD.replace(split,
                "MAKOS_AARCH64_THREAD_EXIT_OK pid=4 tid=13 status=0\n" + split)
            with self.subTest(split=split), self.assertRaisesRegex(AssertionError, "one complete"):
                probe.validate_output(EL0_GOOD + "\n" + fragment)

    def test_missing_newline_or_prefix_boundary(self):
        for record in (RECORD.rstrip(), "partial-record" + RECORD):
            with self.subTest(record=record), self.assertRaisesRegex(AssertionError, "one complete"):
                probe.validate_output(EL0_GOOD + "\n" + record)

    def test_fatal_and_panic_fail(self):
        for marker in ("MAKOS_FATAL:", "EL0 entry rejected", "MAKOS_PANIC:"):
            with self.subTest(marker=marker), self.assertRaises(AssertionError):
                probe.validate_output(GOOD + marker)

    def test_guest_source_and_emitter(self):
        source = (ROOT / "ports/musl/dynamic-probe.c").read_text()
        emitter = item(source, "static int emit_fault_result(")
        main = item(source, "int main(")
        self.assertLess(main.index("pthread_join("), main.index("emit_fault_result("))
        self.assertLess(main.index("munmap(fault_region, FAULT_BYTES)"), main.index("emit_fault_result("))
        self.assertIn("results[index].fault_rounds != FAULT_ROUNDS", main)
        self.assertEqual(len(re.findall(r"\bwrite\(", emitter)), 1)
        self.assertNotRegex(emitter, r"\b(?:printf|fflush|writev|syscall)\(")
        self.assertIn("char record[512];", emitter)
        touches = item(source, "static int concurrent_first_touches(")
        self.assertEqual(touches.count("fault_barrier()"), 4)
        self.assertIn("shared[worker] = fault_value(round, worker)", touches)
        self.assertIn("distinct[worker * PAGE_BYTES / sizeof *distinct] =", touches)
        compiler = os.environ.get("HOST_CC") or shutil.which("cc")
        self.assertTrue(compiler, "host C compiler unavailable")
        flags = ["-std=c11", "-Wall", "-Wextra", "-Werror", "-O2", "-D_FORTIFY_SOURCE=2"]
        # Adapt just this call site, retaining any SDK snprintf macro and
        # its fortification. Never #define/undefine a host stdio API macro.
        adapted, calls = re.subn(r"\bsnprintf(?=\s*\()", "checked_snprintf", emitter)
        self.assertEqual(calls, 1)
        declarations = re.search(r"enum \{ WORKERS = .*?;", source).group()
        declarations += "\n" + item(source, "struct worker_result {") + ";\n"
        adapter = r'''
#include <assert.h>
#include <limits.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/types.h>
#include <unistd.h>
static char captured[512];
static unsigned writes;
static int format_fault = INT_MIN;
static int write_fault;
static int checked_snprintf(char *buffer, size_t capacity, const char *format, ...)
{
    assert(capacity == sizeof captured);
    if (format_fault != INT_MIN) return format_fault;
    va_list arguments;
    va_start(arguments, format);
    int length = vsnprintf(buffer, capacity, format, arguments);
    va_end(arguments);
    return length;
}
static ssize_t checked_write(int fd, const void *buffer, size_t length)
{
    assert(fd == STDOUT_FILENO && length > 0 && length < sizeof captured);
    writes++;
    memcpy(captured, buffer, length);
    captured[length] = 0;
    switch (write_fault) {
    case 1: return -1;
    case 2: return 0;
    case 3: return (ssize_t)length - 1;
    case 4: return (ssize_t)length + 1;
    default: return (ssize_t)length;
    }
}
#define write checked_write
'''
        driver = r'''
#undef write
int main(void)
{
    struct worker_result results[WORKERS] = {{0}};
    for (unsigned i = 0; i < WORKERS; i++) {
        results[i].tid = 11 + i;
        results[i].fault_base = 0x80200000;
    }
    assert(emit_fault_result(results) == 0 && writes == 1);
    fputs(captured, stdout);
    const int bad_formats[] = {-1, 0, 512, 513, INT_MAX};
    for (unsigned i = 0; i < sizeof bad_formats / sizeof *bad_formats; i++) {
        writes = 0; format_fault = bad_formats[i];
        assert(emit_fault_result(results) == -1 && writes == 0);
    }
    format_fault = INT_MIN;
    for (write_fault = 1; write_fault <= 4; write_fault++) {
        writes = 0;
        assert(emit_fault_result(results) == -1 && writes == 1);
    }
    return 0;
}
'''
        with tempfile.TemporaryDirectory(prefix="makos-vm-fault-emission-") as name:
            directory = Path(name)
            fixture, binary = directory / "emission.c", directory / "emission"
            fixture.write_text(adapter + declarations + adapted + driver)
            subprocess.run([compiler, *flags, str(fixture), "-o", str(binary)], check=True)
            emitted = subprocess.check_output([str(binary)], text=True, timeout=10)
            self.assertEqual(emitted, RECORD)
            self.assertEqual(probe.validate_output(EL0_GOOD + "\n" + emitted),
                             ((11, 12, 13), 0x80200000))


if __name__ == "__main__":
    unittest.main()
