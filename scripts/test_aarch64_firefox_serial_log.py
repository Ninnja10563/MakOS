#!/usr/bin/env python3
"""Prove a Firefox Ctrl-A timeout preserves current serial evidence."""

from __future__ import annotations

import pathlib
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
