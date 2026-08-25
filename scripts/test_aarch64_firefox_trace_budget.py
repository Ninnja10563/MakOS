#!/usr/bin/env python3
"""Guard Firefox hot-path serial traces against unbounded synchronous output."""

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PROCESS = (ROOT / "kernel/src/aarch64_process.rs").read_text()
SOCKET = (ROOT / "kernel/src/aarch64_socket.rs").read_text()
ARCH = (ROOT / "kernel/src/arch/aarch64.rs").read_text()
VM = (ROOT / "kernel/src/aarch64_vm.rs").read_text()
VFS = (ROOT / "kernel/src/vfs.rs").read_text()
SHMEM = (ROOT / "kernel/src/aarch64_shmem.rs").read_text()
EPOLL = (ROOT / "kernel/src/aarch64_epoll.rs").read_text()
POOL_PATCH = (
    ROOT / "ports/firefox/patches/0053-makos-bsp-thread-pool-limits.patch"
).read_text()

assert "const THREAD_TRACE_LIMIT: u64 = 8;" in PROCESS
assert PROCESS.count("< THREAD_TRACE_LIMIT") == 2
assert "const FIREFOX_SOCKET_TRACE_LIMIT: u64 = 8;" in SOCKET
assert SOCKET.count("FIREFOX_SOCKET_TRACE_LIMIT") == 4
assert "MAKOS_FIREFOX_SOCKET_CREATE_FAIL" in SOCKET
assert "const FIREFOX_FILE_TRACE_LIMIT: u64 = 8;" in ARCH
assert "const FIREFOX_MUTATION_TRACE_LIMIT: u64 = 16;" in ARCH
assert ARCH.count("< FIREFOX_FILE_TRACE_LIMIT") == 4
assert ARCH.count("< FIREFOX_MUTATION_TRACE_LIMIT") == 3
assert "const VM_TRACE_LIMIT: u64 = 8;" in VM
assert VM.count("< VM_TRACE_LIMIT") == 2
assert "const SHMEM_TRACE_LIMIT: u64 = 8;" in VFS
assert "MAKOS_SHMEM_OPEN_FAIL" in VFS
assert VFS.count("result.is_err()") >= 2
assert "const SHMEM_TRUNCATE_TRACE_LIMIT: u64 = 8;" in SHMEM
assert "result.is_err()" in SHMEM
assert "const FIREFOX_EPOLL_TRACE_LIMIT: u64 = 8;" in EPOLL
assert EPOLL.count("< FIREFOX_EPOLL_TRACE_LIMIT") == 3
assert 'b"MOZ_TASKCONTROLLER_THREADCOUNT=3".as_slice()' in PROCESS
assert "kMaxConnectionThreadCount = 2" in POOL_PATCH
assert "kMaxConnectionThreadCount = 20" in POOL_PATCH
assert "mPool->SetThreadLimit(4)" in POOL_PATCH
assert "mPool->SetIdleThreadLimit(1)" in POOL_PATCH
assert "mPool->SetThreadLimit(25)" in POOL_PATCH

# Gate3's observed high-frequency classes. Caps project 1,374 fewer serial
# lines per cold start; this model documents the measured input to the change.
observed_and_caps = (
    (117, 8), (57, 8), (128, 8), (128, 8), (26, 8), (86, 8),
    (194, 8), (64, 8), (360, 16), (56, 8), (78, 8), (69, 8),
    (42, 8), (42, 8), (42, 8),
    (21, 8),
)
saved = sum(max(0, observed - cap) for observed, cap in observed_and_caps)
assert saved == 1374

print(
    "MAKOS_AARCH64_FIREFOX_TRACE_BUDGET_TEST_OK "
    f"projected_gate3_lines_removed={saved} proof_markers=first-bounded errors=unbounded"
)
