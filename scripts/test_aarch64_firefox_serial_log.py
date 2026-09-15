#!/usr/bin/env python3
"""Prove Firefox failures preserve raw serial without changing rejection."""

from __future__ import annotations

import pathlib
import contextlib
import io
import os
import sys
import tempfile


ROOT = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

import boot_test_aarch64 as boot  # noqa: E402


source = (ROOT / "scripts/boot_test_aarch64.py").read_text()
assert 'send_key(stream, "ctrl-a")' in source
assert 'b"MAKOS_WIDGET_KEY raw=132"' in source
assert '"MAKOS_AARCH64_FIREFOX_SELECTION_LIMIT_MS",\n                                "10000",' in source

original_wait = boot.wait_for_new_output
expected = AssertionError("synthetic Ctrl-A timeout")
output = bytearray(b"serial-before-wait\n")
observed: tuple[bytes, int] | None = None


def timeout_wait(selector, process, current, marker, timeout):
    global observed
    assert selector is selection_selector
    assert process is qemu_process
    assert current is output
    observed = (marker, timeout)
    current.extend(b"serial-during-timeout\n")
    raise expected


selection_selector = object()
qemu_process = object()

try:
    with tempfile.TemporaryDirectory(prefix="makos-firefox-serial-test-") as name:
        serial_log = pathlib.Path(name) / "firefox-runtime-serial.log"
        boot.wait_for_new_output = timeout_wait
        caught = None
        try:
            boot.wait_for_firefox_selection_output(
                selection_selector,
                qemu_process,
                output,
                str(serial_log),
            )
        except AssertionError as error:
            caught = error
        assert caught is expected
        assert observed == (b"MAKOS_WIDGET_KEY raw=132", 180)
        assert serial_log.read_bytes() == bytes(output)
finally:
    boot.wait_for_new_output = original_wait

print(
    "MAKOS_AARCH64_FIREFOX_SERIAL_TIMEOUT_TEST_OK "
    "marker=raw-132 timeout=180 serial=current exception=re-raised"
)

# The runtime's fatal checks, including startup's immediate prefix rejection,
# must stay intact. Cleanup only preserves bytes; it cannot turn failure into
# success or introduce a wait for a complete fatal line.
assert source.count('if b"MAKOS_FATAL:" in output:') == 2
assert source.count('if b"MAKOS_FATAL:" in output[firefox_output_start:]:') == 3
assert 'raise AssertionError(\n                                "Firefox probe reached kernel fatal:\\n"' in source
assert 'process.wait(timeout=5)\n            finally:\n                preserve_final_firefox_serial_output(' in source


class PipeProcess:
    def __init__(self, stdout, returncode):
        self.stdout = stdout
        self.returncode = returncode

    def poll(self):
        return self.returncode


def capture_case(directory, tail, *, writer_open=False, running=False, preserve=None):
    if preserve is None:
        preserve = boot.preserve_final_firefox_serial_output
    read_fd, write_fd = os.pipe()
    try:
        os.write(write_fd, tail)
        if not writer_open:
            os.close(write_fd)
            write_fd = None
        with os.fdopen(read_fd, "rb", buffering=0) as pipe:
            process = PipeProcess(pipe, None if running else 0)
            current = bytearray(b"MAKOS_FATAL:")
            log = directory / "fatal-serial.log"
            failure = AssertionError("Firefox probe reached kernel fatal:\nMAKOS_FATAL:")
            caught = None
            try:
                try:
                    # This is already a failure at the colon, not a wait for
                    # the reason or an attempt to recognise later successes.
                    raise failure
                finally:
                    preserve(process, current, str(log))
            except AssertionError as error:
                caught = error
            assert caught is failure
            assert os.get_blocking(pipe.fileno())
            expected = b"MAKOS_FATAL:" + (b"" if running else tail)
            assert bytes(current) == expected, "queued fatal bytes lost"
            assert log.read_bytes() == expected, "raw fatal log differs"
    finally:
        if write_fd is not None:
            os.close(write_fd)


with tempfile.TemporaryDirectory(prefix="makos-firefox-fatal-serial-") as name:
    directory = pathlib.Path(name)
    for tail in (
        b" synthetic reason\r\n",
        b"",  # EOF at the colon must remain a failure, with no invented reason.
        b" partial reason",  # No newline is manufactured.
        b" synthetic reason\r\nMAKOS_FIREFOX_RUNTIME_OK\r\n\x00\xff",
    ):
        capture_case(directory, tail)
        capture_case(directory, tail, writer_open=True)
    capture_case(directory, b"must not read a live process", writer_open=True, running=True)

    # Reproduce the old cleanup behaviour: saving only the accumulated prefix
    # must fail the same raw-byte assertion even though fatal rejection holds.
    def old_capture(process, current, serial_log):
        pathlib.Path(serial_log).write_bytes(current)

    try:
        capture_case(directory, b" synthetic reason\r\n", preserve=old_capture)
    except AssertionError as error:
        assert str(error) == "queued fatal bytes lost"
    else:
        raise AssertionError("old no-drain cleanup negative control passed")

    # A diagnostic file failure must not mask the original guest assertion.
    failure = AssertionError("original guest failure")
    diagnostics = io.StringIO()
    with contextlib.redirect_stderr(diagnostics):
        try:
            try:
                raise failure
            finally:
                boot.preserve_final_firefox_serial_output(
                    PipeProcess(None, 0), bytearray(b"MAKOS_FATAL:"), str(directory)
                )
        except AssertionError as error:
            assert error is failure
        else:
            raise AssertionError("fatal exception suppressed by diagnostic I/O")
    assert "Could not preserve final Firefox serial log:" in diagnostics.getvalue()
    with (directory / "closed-pipe").open("wb") as closed_pipe:
        pass
    diagnostics = io.StringIO()
    log = directory / "closed-pipe-serial.log"
    with contextlib.redirect_stderr(diagnostics):
        try:
            try:
                raise failure
            finally:
                boot.preserve_final_firefox_serial_output(
                    PipeProcess(closed_pipe, 0), bytearray(b"MAKOS_FATAL:"), str(log)
                )
        except AssertionError as error:
            assert error is failure
        else:
            raise AssertionError("fatal exception suppressed by closed pipe")
    assert "Could not drain final Firefox serial output:" in diagnostics.getvalue()
    assert log.read_bytes() == b"MAKOS_FATAL:"
    boot.preserve_final_firefox_serial_output(PipeProcess(None, 0), bytearray(), None)

print(
    "MAKOS_AARCH64_FIREFOX_FATAL_CAPTURE_HOST_OK "
    "pipe=real raw_bytes=exact drain=stopped-nonblocking "
    "fatal=immediate exception=unchanged negative_control=old-no-drain"
)
