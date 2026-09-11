#!/usr/bin/env python3
"""Exercise the real bounded C emitter; never repair fragmented guest output.

Host write/format adapters inject failures and a kernel record at each native
write boundary. They do not execute the guest scheduler or qualify Firefox.
"""

from pathlib import Path
import os
import re
import shutil
import subprocess
import tempfile

import boot_test_aarch64_el0_entry as runtime
from test_aarch64_el0_entry_runtime import GOOD

ROOT = Path(__file__).resolve().parents[1]
source = (ROOT / "ports/musl/dynamic-probe.c").read_text()


def item(text: str, declaration: str) -> str:
    start = text.index(declaration)
    end = text.index("{", start) + 1
    depth = 1
    while depth:
        depth += (text[end] == "{") - (text[end] == "}")
        end += 1
    return text[start:end]


emitter = item(source, "static int emit_el0_result(")
main = item(source, "int main(")
assert main.index("pthread_join(") < main.index("emit_el0_result(")
assert main.index("emit_el0_result(") < main.index("write(1, marker,")
assert "fflush(" not in main and "printf(" not in main
assert len(re.findall(r"\bwrite\(", emitter)) == 1
assert not re.search(r"\b(?:printf|fflush|writev|syscall)\(", emitter)
assert "char record[512];" in emitter
# Adapt only this extracted call, not the SDK's snprintf macro. Darwin's
# fortified <stdio.h> owns that macro; defining or undefining it here would
# collide with or discard the host header's fortification policy.
adapted_emitter, format_calls = re.subn(
    r"\bsnprintf(?=\s*\()", "checked_snprintf", emitter)
assert format_calls == 1
declarations = re.search(r"enum \{ WORKERS = .*?;", source).group()
declarations += "\n" + item(source, "struct worker_result {") + ";\n"

ADAPTER = r'''
#include <assert.h>
#include <limits.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/types.h>
#include <unistd.h>

static char captured[2048];
static size_t used;
static unsigned writes;
static int format_fault = INT_MIN;
static int write_fault;
static const char diagnostic[] =
    "MAKOS_AARCH64_THREAD_EXIT_OK pid=4 tid=7 status=0 "
    "reap=task-only shared_root=retained\n";

static int checked_snprintf(char *buffer, size_t capacity, const char *format, ...)
{
    assert(capacity == 512);
    if (format_fault != INT_MIN) return format_fault;
    va_list arguments;
    va_start(arguments, format);
    int length = vsnprintf(buffer, capacity, format, arguments);
    va_end(arguments);
    return length;
}

static ssize_t checked_write(int fd, const void *buffer, size_t length)
{
    assert(fd == STDOUT_FILENO && length > 0 && length < 512);
    writes++;
    assert(used + length + sizeof diagnostic < sizeof captured);
    memcpy(captured + used, buffer, length);
    used += length;
    /* A kernel diagnostic may run between any two native writes. */
    memcpy(captured + used, diagnostic, sizeof diagnostic - 1);
    used += sizeof diagnostic - 1;
    captured[used] = 0;
    switch (write_fault) {
    case 1: return -1;
    case 2: return 0;
    case 3: return (ssize_t)length - 1;
    case 4: return (ssize_t)length + 1;
    default: return (ssize_t)length;
    }
}

/* Negative control for the two native writes made by the old stdio path. */
#ifdef FRAGMENT_WRITES
static ssize_t fragmented_write(int fd, const void *buffer, size_t length)
{
    const char *suffix = strstr(buffer, " calls=96");
    assert(suffix);
    size_t prefix = (size_t)(suffix - (const char *)buffer);
    ssize_t first = checked_write(fd, buffer, prefix);
    ssize_t second = checked_write(fd, suffix, length - prefix);
    return first + second;
}
#define write fragmented_write
#else
#define write checked_write
#endif
'''

DRIVER = r'''
#undef write
int main(int argc, char **argv)
{
    struct worker_result results[WORKERS] = {{0}};
    for (unsigned i = 0; i < WORKERS; i++) results[i].tid = 11 + i;
    void *code = (void *)(uintptr_t)0x80000000;
    uintptr_t loader = 0x280b0750;
    if (argc == 2 && !strcmp(argv[1], "--emit")) {
        assert(emit_el0_result(results, loader, code) == 0);
        assert(fwrite(captured, 1, used, stdout) == used);
        return 0;
    }
    assert(argc == 1);
    assert(emit_el0_result(results, loader, code) == 0);
    assert(writes == 1);
    assert(strstr(captured, "block=sleep-until\nMAKOS_AARCH64_THREAD_EXIT_OK"));
    const int bad_formats[] = {-1, 0, 512, 513, INT_MAX};
    for (unsigned i = 0; i < sizeof bad_formats / sizeof *bad_formats; i++) {
        writes = 0; used = 0; format_fault = bad_formats[i];
        assert(emit_el0_result(results, loader, code) == -1);
        assert(writes == 0);
    }
    format_fault = INT_MIN;
    for (write_fault = 1; write_fault <= 4; write_fault++) {
        writes = 0; used = 0;
        assert(emit_el0_result(results, loader, code) == -1);
        assert(writes == 1); /* No suffix retry, including EINTR/error. */
    }
    writes = 0; used = 0; write_fault = 0;
    for (unsigned i = 0; i < WORKERS; i++) results[i].tid = LONG_MAX - i;
    assert(emit_el0_result(results, loader, (void *)(uintptr_t)0x3bff00000) == 0);
    assert(writes == 1 && used < sizeof captured);
    puts("C emission cases=11 passed");
    return 0;
}
'''

# This models the reported Darwin header collision on Linux; it is not an
# implementation of Darwin fortification. Keep an actual SDK macro when one
# exists. The model is never called: checked_snprintf uses the host vsnprintf.
SDK_SNPRINTF_MODEL = r'''
#ifndef snprintf
#define snprintf(str, len, ...) __snprintf_chk_func (str, len, 0, __VA_ARGS__)
#endif
'''
SDK_SNPRINTF_RETAINED = r'''
#ifndef snprintf
#error "SDK snprintf macro was removed by the test adapter"
#endif
'''
assert not re.search(r"^\s*#\s*(?:define|undef)\s+snprintf\b",
                     ADAPTER + DRIVER, re.MULTILINE)


def with_record(record: str) -> str:
    return "\n".join(line for line in GOOD.splitlines()
                     if not line.startswith("MAKOS_MUSL_EL0_ENTRY_OK")) + "\n" + record


def fragmented_must_fail(record: str) -> None:
    try:
        runtime.validate_output(with_record(record))
    except AssertionError as error:
        assert "one complete" in str(error), error
    else:
        raise AssertionError("fragmented evidence unexpectedly accepted")


with tempfile.TemporaryDirectory(prefix="makos-el0-emission-") as name:
    directory = Path(name)
    fixture = directory / "emission.c"
    compiler = os.environ.get("HOST_CC") or shutil.which("cc")
    if not compiler:
        raise SystemExit("host C compiler unavailable")
    flags = ["-std=c11", "-Wall", "-Wextra", "-Werror", "-O2"]
    # Insert the model after all real host headers, never before <stdio.h>.
    sdk_adapter = ADAPTER.replace("\nstatic char captured",
                                 SDK_SNPRINTF_MODEL + "\nstatic char captured", 1)
    assert sdk_adapter != ADAPTER
    for label, adapter, extra_flags, suffix in (
        ("host", ADAPTER, [], ""),
        ("sdk-macro", sdk_adapter, ["-D_FORTIFY_SOURCE=2"], SDK_SNPRINTF_RETAINED),
    ):
        fixture.write_text(adapter + declarations + adapted_emitter + DRIVER + suffix)
        binaries = [directory / f"{label}-one-write", directory / f"{label}-fragmented-write"]
        for index, binary in enumerate(binaries):
            subprocess.run([compiler, *flags, *extra_flags,
                            *(["-DFRAGMENT_WRITES"] if index else []),
                            str(fixture), "-o", str(binary)], check=True)
        subprocess.run([str(binaries[0])], check=True, timeout=10)
        good = subprocess.check_output([str(binaries[0]), "--emit"], text=True, timeout=10)
        assert runtime.validate_output(with_record(good)) == ((11, 12, 13), 0x280B0750, 0x80000000)
        fragmented_must_fail(subprocess.check_output(
            [str(binaries[1]), "--emit"], text=True, timeout=10))

    # The old global redirection must reproduce a macro-redefinition failure
    # under the same warnings-as-errors policy, not merely fail at runtime.
    fixture.write_text(sdk_adapter + "\n#define snprintf checked_snprintf\n"
                       + declarations + emitter + DRIVER)
    collision = subprocess.run(
        [compiler, *flags, "-D_FORTIFY_SOURCE=2", str(fixture),
         "-o", str(directory / "old-macro-collision")], capture_output=True, text=True)
    assert collision.returncode != 0, "old SDK macro collision unexpectedly compiled"
    assert "snprintf" in collision.stderr and "redefined" in collision.stderr, collision.stderr

# Preserve the reported Mac failure as a negative fixture, not a repaired line.
fragmented_must_fail(
    "MAKOS_MUSL_EL0_ENTRY_OK loader=musl threads=3 singleton=0x2,0x4,0x8 "
    "tids=5,6,7 pthread_create=0x280afcd4 rx=0x80000000 resume=0x80001000"
    "MAKOS_AARCH64_THREAD_EXIT_OK pid=4 tid=7 status=0 reap=task-only shared_root=retained\n"
    "MAKOS_CLEAR_CHILD_TID_OK pid=6 address=0x280e03e8 zeroed=1 wake=0\n"
    "MAKOS_AARCH64_THREAD_EXIT_OK pid=4 tid=5 status=0 reap=task-only shared_root=retained\n"
    "MAKOS_AARCH64_THREAD_EXIT_OK pid=4 tid=6 status=0 reap=task-only shared_root=retained\n"
    " calls=96 statuses=42,42,42 block=sleep-until\n")

print("MAKOS_AARCH64_EL0_EMISSION_HOST_OK emitter=production-c cases=11 "
      "buffer=512 write=one-full short,error,overflow=fail-closed "
      "negative_controls=split-write,reported-mac-fragment parser=unchanged "
      "header_macro=preserved fortify=host-default,2 macro_collision=negative-control")
