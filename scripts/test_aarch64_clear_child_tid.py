#!/usr/bin/env python3
"""Run production clear-child-TID cleanup with host task/futex/IPI adapters."""

from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
PROCESS = (ROOT / "kernel/src/aarch64_process.rs").read_text()


def item(source: str, start: str) -> str:
    offset = source.index(start)
    opening = source.index("{", offset)
    depth = 1
    end = opening + 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[offset:end]


cleanup = item(PROCESS, "fn clear_child_tid_on_exit()")
assert cleanup.count("notify_idle_cpus();") == 1
# Observe the real scheduler publication/IPI wiring in addition to the
# controlled host wake adapter. No device or architecture behavior is mocked
# into the production cleanup routine itself.
notify = item(PROCESS, "fn notify_idle_cpus()")
assert 'asm!("dsb ish"' in notify
assert "crate::arch::send_scheduler_ipi();" in notify
assert "wake_futex_in_state(state, FutexKey::new(root, address), usize::MAX)" in cleanup

with tempfile.TemporaryDirectory(prefix="makos-clear-child-tid-") as temporary:
    directory = Path(temporary)
    source = directory / "cleanup.rs"
    source.write_text(cleanup)
    (directory / "test.rs").write_text((ROOT / "scripts/test_aarch64_clear_child_tid.rs").read_text())

    def compile_fixture(name: str) -> Path:
        binary = directory / name
        subprocess.run(["rustc", "--edition=2024", "--test", str(directory / "test.rs"),
                        "-o", str(binary)], check=True)
        return binary

    subprocess.run([str(compile_fixture("clear-child-tid")), "--test-threads=1"],
                   check=True, timeout=20)
    source.write_text(cleanup.replace("notify_idle_cpus();", ""))
    negative = subprocess.run([str(compile_fixture("clear-child-tid-without-ipi")),
                               "--exact", "positive_wake_notifies_idle_cpus_after_unlock"],
                              capture_output=True, text=True, timeout=10)
    if negative.returncode == 0 or "Ready futex waiters were not notified" not in negative.stdout + negative.stderr:
        raise SystemExit(f"missing-IPI negative control did not expose sleeping waiters:\n{negative.stdout}{negative.stderr}")

print("MAKOS_AARCH64_CLEAR_CHILD_TID_HOST_OK cleanup=production-code "
      "order=zero,wake,unlock,ipi notification=positive-wakes-only "
      "zero,unmapped,missing=denied repeat=idempotent negative_control=missing-ipi-rejected")
