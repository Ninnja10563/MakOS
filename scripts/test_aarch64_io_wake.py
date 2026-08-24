#!/usr/bin/env python3
"""Structural guard for bounded, source-targeted AArch64 I/O wakes."""

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PROCESS = (ROOT / "kernel/src/aarch64_process.rs").read_text()
ARCH = (ROOT / "kernel/src/arch/aarch64.rs").read_text()
SOCKET = (ROOT / "kernel/src/aarch64_socket.rs").read_text()
VIRTIO_NET = (ROOT / "kernel/src/aarch64_virtio_net.rs").read_text()
VFS = (ROOT / "kernel/src/vfs.rs").read_text()
READINESS = (ROOT / "crates/readiness/src/lib.rs").read_text()


def function_body(source: str, signature: str) -> str:
    start = source.index(signature)
    brace = source.index("{", start)
    depth = 0
    for index in range(brace, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return source[brace + 1 : index]
    raise AssertionError(f"unterminated function: {signature}")


wait_match = function_body(READINESS, "pub fn woken_by")
block_any = function_body(PROCESS, "pub(crate) fn block_current_for_io(")
block_target = function_body(PROCESS, "pub(crate) fn block_current_for_io_on(")
wake_target = function_body(PROCESS, "pub(crate) fn wake_io_source")
wake_any = function_body(PROCESS, "pub(crate) fn wake_io_waiters")
vfs_key = function_body(VFS, "pub fn io_wait_key")
pump = function_body(SOCKET, "pub fn pump()")

assert "(Self::Any, _) | (_, Self::Any) => true" in wait_match
assert "WaitSource::Any" in block_any and "block_current_for_io_on" in block_any
assert "io_source = source" in block_target
assert "io_source.woken_by(source)" in wake_target
assert "wake_io_source(makos_readiness::WaitSource::Any)" in wake_any
assert "DESCRIPTION_PIPE_READ | DESCRIPTION_PIPE_WRITE" in vfs_key
assert "DESCRIPTION_SOCKETPAIR" in vfs_key
assert "description.pipe.min(description.peer_pipe)" in vfs_key
assert ARCH.count("wake_vfs_source(frame.registers[0])") == 6
assert ARCH.count("vfs_wait_source(frame.registers[0])") == 7
assert ARCH.count("network_wait_source(frame.registers[0])") == 2
assert ARCH.count("block_current_for_io(timeout, frame)") == 3
assert "progressed_handles" in pump
assert "WaitSource::Network(handle)" in pump
assert "wake_io_waiters" not in SOCKET
assert "const TCP_RECEIVE_CAPACITY: usize = 32_768;" in SOCKET
assert "tcp_responses: [[u8; TCP_RECEIVE_CAPACITY]; MAX_SOCKETS]" in SOCKET
assert "tcp_response: [u8; TCP_RECEIVE_CAPACITY]" not in SOCKET
assert "receive_window: u16" in VIRTIO_NET
assert "pub fn tcp_update_receive_window" in VIRTIO_NET
assert "pub fn tcp6_update_receive_window" in VIRTIO_NET
assert VIRTIO_NET.count("segment.payload.len() > output.len()") >= 2
assert VIRTIO_NET.count("connection.receive_window = 0;") >= 2
assert VIRTIO_NET.count("receive_window <= connection.receive_window") == 2
assert "tcp_update_receive_window(" in SOCKET
assert "tcp6_update_receive_window(" in SOCKET

print(
    "MAKOS_AARCH64_IO_WAKE_TEST_OK "
    "direct=descriptor,network wildcard=poll,epoll,pselect,record-lock "
    "matching=exact bounded=process-table,pump-frames tcp_rx=32k-pooled,dynamic-window"
)
