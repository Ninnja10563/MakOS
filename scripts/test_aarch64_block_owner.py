#!/usr/bin/env python3
"""Run production block queue/lock code with host counter/event/MMIO adapters."""

from __future__ import annotations

import pathlib
import re
import subprocess
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
BLOCK = (ROOT / "kernel/src/aarch64_virtio_blk.rs").read_text()
VFS = (ROOT / "kernel/src/vfs.rs").read_text()
VOLUME = (ROOT / "kernel/src/makfs4_volume.rs").read_text()


def item(source: str, start: str) -> str:
    offset = source.index(start)
    opening = source.index("{", offset)
    depth = 1
    end = opening + 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[offset:end]


def lock_item(source: str, start: str) -> str:
    result = item(source, start)
    # Select the actual AArch64 branch for this host-side execution, including
    # on Darwin/x86. No lock logic or request-service call is replaced.
    return result.replace('#[cfg(target_arch = "aarch64")]\n', "")


constants = "\n".join(
    re.findall(r"^const (?:MAX_DEVICES|REQUEST_(?:READ|WRITE|FLUSH)|SERVICE_\w+|SLOT_\w+):.*;$", BLOCK, re.M)
)
counts = "\n".join(
    re.findall(r"^static (?:NONOWNER_REQUESTS|OWNER_\w+|TIMER_SERVICE_COMPLETIONS):.*;$", BLOCK, re.M)
)
queue_types = BLOCK[
    BLOCK.index("#[derive(Clone, Copy)]\nstruct ServiceRequest") :
    BLOCK.index("pub struct SourceWriteFreeze")
]
functions = "\n".join(
    item(BLOCK, declaration)
    for declaration in (
        "fn queue_request(",
        "pub fn service_requests_from_timer()",
        "pub fn service_requests_while_waiting()",
        "fn service_requests(timer_service:",
        "pub fn reset_service_affinity_evidence()",
        "pub fn service_affinity_evidence()",
    )
)
assert "counter_deadline_millis(5_000)" in functions
assert "wait_for_service_event();" in functions
assert "notify_service_waiters();" in functions
assert '"dsb ish", "sev"' in item(BLOCK, "fn notify_service_waiters()")
assert '"wfe"' in item(BLOCK, "fn wait_for_service_event()")

lock_functions = {
    "vfs_lock.rs": lock_item(VFS, "fn with_state<R>"),
    "inode_lock.rs": lock_item(VOLUME, "fn with_inode_cache<R>"),
    "mutation_lock.rs": "struct MutationGuard;\n"
    + lock_item(VOLUME, "impl MutationGuard")
    + "\n"
    + item(VOLUME, "impl Drop for MutationGuard"),
}
for name, body in lock_functions.items():
    assert "crate::aarch64_virtio_blk::service_requests_while_waiting();" in body, name
    assert "compare_exchange_weak" in body and "Ordering::Acquire" in body, name

with tempfile.TemporaryDirectory(prefix="makos-block-owner-") as directory:
    output = pathlib.Path(directory)
    (output / "block_queue.rs").write_text(
        "use core::cell::UnsafeCell;\n"
        "use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};\n"
        + constants + "\n" + counts + "\n" + queue_types + "\n" + functions
    )
    for name, body in lock_functions.items():
        (output / name).write_text(body)
    # include! resolves the unmodified behavioral fixture beside these exact
    # source excerpts. Hardware access alone is supplied by the host fixture.
    (output / "test.rs").write_text((ROOT / "scripts/test_aarch64_block_owner.rs").read_text())
    binary = output / "block-owner-test"
    subprocess.run(
        ["rustc", "--edition=2024", "--test", str(output / "test.rs"), "-o", str(binary)],
        check=True,
    )
    subprocess.run([str(binary), "--test-threads=1"], check=True, timeout=30)

    # Remove only the new VFS progress call in a generated negative control.
    # The exact same behavioral regression must then observe the circular
    # wait, demonstrating that ordinary host scheduling cannot hide the bug.
    (output / "vfs_lock.rs").write_text(
        lock_functions["vfs_lock.rs"].replace(
            "crate::aarch64_virtio_blk::service_requests_while_waiting();", ""
        )
    )
    negative = output / "block-owner-without-vfs-progress"
    subprocess.run(
        ["rustc", "--edition=2024", "--test", str(output / "test.rs"), "-o", str(negative)],
        check=True,
    )
    failed = subprocess.run(
        [str(negative), "--exact", "vfs_contention_completes_ap_io_without_timer"],
        capture_output=True, text=True, timeout=15,
    )
    if failed.returncode == 0 or "CPU0/AP filesystem-block circular wait" not in failed.stdout + failed.stderr:
        raise SystemExit(f"block owner negative control did not reproduce circular wait:\n{failed.stdout}{failed.stderr}")

print("MAKOS_AARCH64_BLOCK_OWNER_HOST_OK locks=vfs,inode-cache,mutation "
      "service=production-queue timer=absent-in-contention requests=read,write,flush "
      "owner=cpu0 timeout_ms=5000 negative_control=deadlock-reproduced")
