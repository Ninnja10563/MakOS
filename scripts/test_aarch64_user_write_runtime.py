#!/usr/bin/env python3
"""Host checks for strict cold-user-write evidence, not guest qualification."""

from __future__ import annotations

import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest

import boot_test_aarch64_user_write as probe
from test_aarch64_vm_fault_runtime import GOOD as VM_GOOD


ROOT = Path(__file__).resolve().parents[1]
BEGIN = "MAKOS_MUSL_USER_WRITE_BEGIN source=immutable-mmap tty_records=2\n"
RESULT = (
    "MAKOS_MUSL_USER_WRITE_OK source=immutable-mmap first_touch=kernel-copy "
    "tty=musl-17,native-63 tty_records=2 file_offset=4093 file_bytes=67 "
    "readback=exact negatives=10 errors=17:-1,63:-22 cleanup=unmapped,unlinked\n"
)
LINES = "#include <stdint.h>\n#include <stdint.h>\n"
GOOD = VM_GOOD + BEGIN + LINES + RESULT


def item(text: str, declaration: str) -> str:
    start = text.index(declaration)
    end = text.index("{", start) + 1
    depth = 1
    while depth:
        depth += (text[end] == "{") - (text[end] == "}")
        end += 1
    return text[start:end]


def validate_probe_capabilities(security: str, processes: str) -> None:
    """Keep file-create authority scoped to the immutable dynamic fixture."""
    registration = item(security, "pub fn register_session_process(")
    assert re.search(
        r"SessionProcessRole::Python\s*\|\s*SessionProcessRole::Native"
        r"\s*=>\s*CAP_CONSOLE\s*,", registration
    ), "ordinary Native and Python roles must remain console-only"
    fixture_arm = re.search(
        r"SessionProcessRole::MuslDynamicProbe\s*=>\s*([^,]+),", registration
    )
    assert fixture_arm is not None, "dedicated dynamic-fixture capability arm absent"
    mask = fixture_arm.group(1).strip()
    assert re.fullmatch(r"CAP_[A-Z_]+(?:\s*\|\s*CAP_[A-Z_]+)*", mask), mask
    assert sorted(re.findall(r"CAP_[A-Z_]+", mask)) == ["CAP_CONSOLE", "CAP_FILE_WRITE"], (
        "dynamic fixture must receive exactly console and file-write capabilities"
    )
    spawn = item(processes, "pub fn spawn_musl_dynamic_probe(")
    role = "SessionProcessRole::MuslDynamicProbe"
    assert spawn.count(role) == 1 and processes.count(role) == 1, (
        "dedicated authority must be bound only by the dynamic-fixture spawn"
    )
    assert re.search(
        r"install_loaded_process\(parent_pid,\s*process,\s*ProcessRole::Native,\s*context\)",
        spawn,
    ), "dynamic fixture must retain the Native scheduling/application role"
    assert "load_dynamic_process(MUSL_DYNAMIC_PROBE_ELF, MUSL_DYNAMIC_LOADER_ELF)" in spawn, (
        "dedicated authority must load the fixed embedded dynamic fixture"
    )
    assert re.search(
        r"if\s+!crate::security::register_session_process\(\s*pid,\s*"
        r"crate::security::SessionProcessRole::MuslDynamicProbe,?\s*\)"
        r"\s*\{\s*discard_spawned\(pid\);\s*return None;\s*\}", spawn
    ), "failed credential binding must discard the fixture process"


class RuntimeEvidenceTests(unittest.TestCase):
    def test_complete_evidence(self):
        self.assertEqual(probe.validate_output(GOOD), ((11, 12, 13), 0x80200000))

    def test_crlf_and_intervening_complete_diagnostics(self):
        serial = GOOD.replace(LINES,
            "#include <stdint.h>\nMAKOS_OTHER_DIAGNOSTIC valid=1\n#include <stdint.h>\n")
        self.assertEqual(probe.validate_output(serial.replace("\n", "\r\r\n")),
                         ((11, 12, 13), 0x80200000))

    def test_missing_or_duplicate_results(self):
        for bad in (GOOD.replace(BEGIN, ""), GOOD.replace(RESULT, ""),
                    GOOD + BEGIN, GOOD + RESULT, GOOD + RESULT[:40]):
            with self.subTest(bad=bad), self.assertRaisesRegex(AssertionError, "one complete"):
                probe.validate_output(bad)

    def test_record_boundaries_are_mandatory(self):
        for bad in (GOOD.rstrip(), GOOD.replace(BEGIN, "prefix" + BEGIN),
                    GOOD.replace(RESULT, "prefix" + RESULT),
                    GOOD.replace("tty=musl-17", "tty=musl\n-17")):
            with self.subTest(bad=bad), self.assertRaisesRegex(AssertionError, "one complete"):
                probe.validate_output(bad)

    def test_all_result_fields_are_required(self):
        changes = (("first_touch=kernel-copy", "first_touch=prewarmed"),
                   ("tty=musl-17,native-63", "tty=musl-17"),
                   ("tty_records=2", "tty_records=1"),
                   ("file_offset=4093", "file_offset=0"),
                   ("file_bytes=67", "file_bytes=66"),
                   ("readback=exact", "readback=unchecked"),
                   ("negatives=10", "negatives=9"),
                   ("errors=17:-1,63:-22", "errors=17:-14,63:-14"),
                   ("cleanup=unmapped,unlinked", "cleanup=unmapped"))
        for before, after in changes:
            with self.subTest(before=before), self.assertRaisesRegex(AssertionError, "one complete"):
                probe.validate_output(GOOD.replace(RESULT, RESULT.replace(before, after)))

    def test_actual_tty_bytes_are_required(self):
        for lines in ("", "#include <stdint.h>\n", LINES * 2,
                      LINES.replace("stdint", "stddef", 1),
                      LINES.replace("stdint", "std\nMAKOS_OTHER\nint", 1),
                      "prefix" + LINES):
            with self.subTest(lines=lines), self.assertRaisesRegex(AssertionError, "intact"):
                probe.validate_output(GOOD.replace(LINES, lines))

    def test_tty_bytes_cannot_be_borrowed_from_elsewhere(self):
        for bad in (VM_GOOD + LINES + BEGIN + RESULT,
                    VM_GOOD + BEGIN + RESULT + LINES):
            with self.subTest(bad=bad), self.assertRaisesRegex(AssertionError, "between"):
                probe.validate_output(bad)

    def test_original_vm_el0_and_reap_proofs_remain_required(self):
        for marker in ("MAKOS_MUSL_EL0_ENTRY_OK", "MAKOS_MUSL_VM_FAULT_OK",
                       "MAKOS_AARCH64_HIGH_EL0_ENTRY_OK cpu=2", "MAKOS_MUSL_DYNAMIC_REAP_OK"):
            with self.subTest(marker=marker), self.assertRaises(AssertionError):
                probe.validate_output(GOOD.replace(marker, "REMOVED"))

    def test_failures_and_leaked_prefix_rejected(self):
        for marker in ("MAKOS_FATAL:", "MAKOS_PANIC:", "EL0 entry rejected",
                       "MAKOS_MUSL_USER_WRITE_FAIL phase=4 observed=-1", "UNSAFE!!"):
            with self.subTest(marker=marker), self.assertRaises(AssertionError):
                probe.validate_output(GOOD + marker)

    def test_file_write_authority_is_exclusive_to_the_fixed_dynamic_fixture(self):
        security_path = ROOT / "kernel/src/security.rs"
        processes_path = ROOT / "kernel/src/aarch64_process.rs"
        validate_probe_capabilities(security_path.read_text(), processes_path.read_text())
        # No alternative kernel spawn path may reuse the fixture's grant.
        for path in (ROOT / "kernel/src").rglob("*.rs"):
            if path not in (security_path, processes_path):
                self.assertNotIn("SessionProcessRole::MuslDynamicProbe", path.read_text(),
                                 f"fixture-only capability reused by {path}")

    def test_fixture_authority_scope_negative_controls(self):
        security = (ROOT / "kernel/src/security.rs").read_text()
        processes = (ROOT / "kernel/src/aarch64_process.rs").read_text()
        validate_probe_capabilities(security, processes)
        controls = (
            (security.replace("SessionProcessRole::Native => CAP_CONSOLE,",
                              "SessionProcessRole::Native => CAP_CONSOLE | CAP_FILE_WRITE,"),
             processes),
            (security.replace("SessionProcessRole::MuslDynamicProbe => CAP_CONSOLE | CAP_FILE_WRITE,",
                              "SessionProcessRole::MuslDynamicProbe => CAP_CONSOLE | CAP_FILE_WRITE | CAP_NETWORK,"),
             processes),
            (security, processes + "\nfn unrelated_spawn() {\n"
             "    let _ = crate::security::SessionProcessRole::MuslDynamicProbe;\n}\n"),
            (security, processes.replace(
                item(processes, "pub fn spawn_musl_dynamic_probe("),
                item(processes, "pub fn spawn_musl_dynamic_probe(").replace(
                    "process, ProcessRole::Native, context", "process, ProcessRole::Firefox, context"))),
            (security, processes.replace("load_dynamic_process(MUSL_DYNAMIC_PROBE_ELF,",
                                         "load_dynamic_process(USER_SELECTED_ELF,")),
        )
        for modified_security, modified_processes in controls:
            with self.subTest(security=modified_security != security,
                              processes=modified_processes != processes):
                self.assertNotEqual((modified_security, modified_processes),
                                    (security, processes), "negative control must mutate source")
                with self.assertRaises(AssertionError):
                    validate_probe_capabilities(modified_security, modified_processes)

    def test_guest_source_and_actual_bounded_emitter(self):
        source = (ROOT / "ports/musl/dynamic-probe.c").read_text()
        workload = item(source, "static int probe_user_write(")
        emitter = item(source, "static int emit_user_write_result(")
        main = item(source, "int main(")
        self.assertLess(main.index("emit_fault_result("), main.index("probe_user_write()"))
        self.assertLess(main.index("probe_user_write()"), main.index("write(1, marker,"))
        self.assertIn("write(STDERR_FILENO, cold, text_bytes)", workload)
        self.assertIn("native_user_write(63, cold, text_bytes)", workload)
        cold_prefix = workload[:workload.index("long written = path == 0")]
        self.assertNotRegex(cold_prefix, r"\b(?:memcpy|memcmp|memset|strlen|pread|read)\s*\(")
        self.assertEqual((ROOT / "ports/musl/shared-demo.c").read_text().splitlines()[0],
                         probe.TTY_LINE)
        self.assertIn("write(target, (const char *)cold + PAGE_BYTES - 3, 67)", workload)
        self.assertIn("O_CREAT | O_EXCL | O_RDWR", workload)
        self.assertLess(workload.index("write(target,"), workload.index("pread(source,"))
        self.assertLess(workload.index("open(path,"), workload.index("unlink(path)"))
        self.assertEqual(workload.count("reject_user_write("), 5)
        self.assertIn("mprotect(resident, PAGE_BYTES, PROT_NONE)", workload)
        self.assertIn("(const void *)(UINTPTR_MAX - 7), 16", workload)
        self.assertNotIn("MAKOS_JIT_POOL_OK", workload + emitter)
        self.assertEqual(len(re.findall(r"\bwrite\(", emitter)), 1)
        self.assertNotRegex(emitter, r"\b(?:printf|fflush|writev|syscall)\(")
        compiler = os.environ.get("HOST_CC") or shutil.which("cc")
        self.assertTrue(compiler, "host C compiler unavailable")
        # Adapt call sites only. Darwin's fortified SDK may already own a
        # snprintf macro; do not redefine, undefine or disable fortification.
        adapted, calls = re.subn(r"\bsnprintf(?=\s*\()", "checked_snprintf", emitter)
        self.assertEqual(calls, 1)
        adapted, calls = re.subn(r"\bwrite(?=\s*\()", "checked_write", adapted)
        self.assertEqual(calls, 1)
        adapter = r'''
#include <assert.h>
#include <limits.h>
#include <stdarg.h>
#include <stdio.h>
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
    if (write_fault == 1) return -1;
    if (write_fault == 2) return 0;
    if (write_fault == 3) return (ssize_t)length - 1;
    if (write_fault == 4) return (ssize_t)length + 1;
    return (ssize_t)length;
}
'''
        driver = r'''
int main(void)
{
    assert(emit_user_write_result() == 0 && writes == 1);
    fputs(captured, stdout);
    const int bad_formats[] = {-1, 0, 512, 513, INT_MAX};
    for (unsigned i = 0; i < sizeof bad_formats / sizeof *bad_formats; i++) {
        writes = 0; format_fault = bad_formats[i];
        assert(emit_user_write_result() == -1 && writes == 0);
    }
    format_fault = INT_MIN;
    for (write_fault = 1; write_fault <= 4; write_fault++) {
        writes = 0;
        assert(emit_user_write_result() == -1 && writes == 1);
    }
    return 0;
}
'''
        with tempfile.TemporaryDirectory(prefix="makos-user-write-host-") as directory:
            unit = Path(directory) / "emission.c"
            binary = Path(directory) / "emission"
            unit.write_text(adapter + adapted + driver)
            subprocess.run([compiler, "-std=c11", "-Wall", "-Wextra", "-Werror", "-O2",
                            "-D_FORTIFY_SOURCE=2", str(unit), "-o", str(binary)], check=True)
            output = subprocess.check_output([str(binary)], text=True)
            self.assertEqual(output, RESULT)
            self.assertEqual(probe.validate_output(VM_GOOD + BEGIN + LINES + output),
                             ((11, 12, 13), 0x80200000))
            # A mutated compiled emitter must fail: no host fixture can
            # manufacture actual file readback or claim fewer negative cases.
            for old, new in (("readback=exact", "readback=unchecked"),
                             ("negatives=10", "negatives=9")):
                unit.write_text(adapter + adapted.replace(old, new) + driver)
                subprocess.run([compiler, "-std=c11", "-Wall", "-Wextra", "-Werror", "-O2",
                                "-D_FORTIFY_SOURCE=2", str(unit), "-o", str(binary)], check=True)
                rejected = subprocess.check_output([str(binary)], text=True)
                with self.assertRaisesRegex(AssertionError, "one complete"):
                    probe.validate_output(VM_GOOD + BEGIN + LINES + rejected)


if __name__ == "__main__":
    unittest.main()
