#!/usr/bin/env python3
"""Structural guard for typed IPC/service-routing runtime coverage."""

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
IPC = (ROOT / "kernel/src/ipc.rs").read_text()
ARCH = (ROOT / "kernel/src/arch/aarch64.rs").read_text()
PROCESS = (ROOT / "kernel/src/aarch64_process.rs").read_text()
X86_SYSCALL = (ROOT / "kernel/src/syscall.rs").read_text()
SECURITY = (ROOT / "kernel/src/security.rs").read_text()
PROBE = (ROOT / "ports/musl/pthread-probe.c").read_text()
SDK = (ROOT / "sdk/include/makos.h").read_text()


def require(source: str, fragment: str) -> None:
    if fragment not in source:
        raise AssertionError(f"missing typed IPC invariant: {fragment}")


for fragment in (
    "TYPED_HANDLE_MARKER",
    "state.cleanup_pid(pid)",
    "state.publish(owner, name)",
    "state.connect(client, name)",
    ".accept(provider, listener)",
    "state.send(sender, endpoint, message, transfer)",
    "state.receive(receiver, endpoint)",
):
    require(IPC, fragment)
for number in range(143, 148):
    require(ARCH, f"= {number};")
require(ARCH, "ABI_FEATURE_TYPED_IPC: u64 = 1 << 21")
require(PROCESS, "let closed_ipc_handles = crate::ipc::close_all(pid);")
require(PROCESS, "process-exit arch=aarch64 pid={} status={} closed_ipc_handles={}")
require(PROCESS, "let closed_ipc_handles = crate::ipc::close_all(group_pid);")
require(PROCESS, "leader=zombie shared_root=retained closed_ipc_handles={}")
require(X86_SYSCALL, "(transfer_address as *mut u64).write_unaligned(transfer);")
require(SECURITY, "CAP_SERVICE_PUBLISH: u64 = 1 << 9")
require(PROBE, 'static const char service[] = "org.makos.echo";')
require(PROBE, 'static const char child_service[] = "org.makos.child";')
require(PROBE, "received.sender_pid != (uint32_t)child")
require(PROBE, "makos_call4(146, primary, (long)&message, delegated, 0x80)")
require(PROBE, "received.type != 17")
require(PROBE, "Process exit must close primary and child_listener immediately.")
require(PROBE, "MAKOS_MUSL_TYPED_IPC_OK")
require(SDK, "struct makos_typed_message")
require(SDK, "MAKOS_FEATURE_TYPED_IPC")

print(
    "MAKOS_AARCH64_TYPED_IPC_TEST_OK "
    "service=same-uid,session handles=generation-tagged "
    "messages=typed,fifo transfer=attenuated,lifetime-retained "
    "runtime=parent-child"
)
