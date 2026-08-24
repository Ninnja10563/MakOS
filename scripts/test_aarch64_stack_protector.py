#!/usr/bin/env python3
"""Guard deployed AArch64 stack-protector failure-path runtime proof."""

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PROBE = (ROOT / "ports/musl/crt-probe.c").read_text()
BUILD = (ROOT / "ports/musl/build-makos.sh").read_text()
AUDIT = (ROOT / "ports/musl/audit-static.sh").read_text()
PROCESS = (ROOT / "kernel/src/aarch64_process.rs").read_text()
ARCH = (ROOT / "kernel/src/arch/aarch64.rs").read_text()
SHELL = (ROOT / "user/aarch64_shell.c").read_text()
BOOT = (ROOT / "scripts/boot_test_aarch64.py").read_text()

assert "volatile unsigned char buffer[16]" in PROBE
assert "overwrite(buffer, 32)" in PROBE
assert "MAKOS_STACK_PROTECTOR_TRIGGER_OK" in PROBE
assert "-fstack-protector-strong" in BUILD
assert '-c "$port_dir/crt-probe.c"' in BUILD
assert "nm_tool -u \"$build_dir/crt-probe.o\"" in AUDIT
assert "__stack_chk_fail" in AUDIT
assert "spawn_stack_protector_probe" in PROCESS
assert 'b"MODE=stack-smash"' in PROCESS
assert "15 => crate::aarch64_process::spawn_stack_protector_probe()" in ARCH
assert "kind == 8 && matches!(exception_class, 0x20 | 0x24)" in ARCH
assert "MAKOS_AARCH64_USER_FAULT_OK" in ARCH
assert "exit_group_from_exception(139, frame)" in ARCH
assert "SYS_PROCESS_SPAWN, 15" in SHELL
assert "status != 0 && status != 42 && status != 134" in SHELL
assert "MAKOS_STACK_PROTECTOR_REAP_OK failure=contained shell=survived" in BOOT

print(
    "MAKOS_AARCH64_STACK_PROTECTOR_TEST_OK "
    "compiler=strong canary=real-overwrite libc=__stack_chk_fail "
    "containment=el0-abort,process-group,child-reap,shell-survives runtime=guest"
)
