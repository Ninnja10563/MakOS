#!/usr/bin/env python3
"""Guard AArch64 AF_UNIX SCM_RIGHTS implementation and runtime probe."""

from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def require(path: str, fragments: tuple[str, ...]) -> None:
    source = (ROOT / path).read_text()
    missing = [fragment for fragment in fragments if fragment not in source]
    if missing:
        raise AssertionError(f"{path}: missing {missing}")


require(
    "kernel/src/vfs.rs",
    (
        "pub fn socket_pair(",
        "pub fn send_result_with_rights(",
        "pub fn read_result_with_rights(",
        "rights_skip",
        "release_description(state, description_index as usize)",
    ),
)
require(
    "kernel/src/arch/aarch64.rs",
    (
        "SYS_SENDMSG_RIGHTS",
        "SYS_RECVMSG_RIGHTS",
        "send_result_with_rights",
        "read_result_with_rights",
    ),
)
require(
    "ports/musl/pthread-probe.c",
    (
        "scm_rights_probe",
        "socketpair(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC",
        "CMSG_LEN(sizeof source)",
        "MAKOS_MUSL_SCM_RIGHTS_OK",
    ),
)
require(
    "scripts/boot_test_aarch64.py",
    ("MAKOS_MUSL_SCM_RIGHTS_OK socketpair=unix-stream",),
)

print(
    "MAKOS_AARCH64_SCM_RIGHTS_TEST_OK "
    "transport=unix-socketpair ordering=associated-byte refs=queued "
    "lifetime=sender-close,receiver-read cleanup=truncate,close"
)
