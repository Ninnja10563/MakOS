#!/usr/bin/env python3
"""Guard executable UEFI memory typing for the direct kernel handoff."""

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
LOADER = (ROOT / "boot/uefi/src/main.rs").read_text()

load_kernel = LOADER.split("fn load_kernel", 1)[1].split(
    "const fn kernel_machine", 1
)[0]
allocation = load_kernel.split("boot::allocate_pages(", 1)[1].split(
    ".map_err", 1
)[0]

assert "MemoryType::LOADER_CODE" in allocation, (
    "kernel handoff allocation must remain executable under UEFI memory protection"
)
assert "MemoryType::LOADER_DATA" not in allocation, (
    "LOADER_DATA may be execute-never under current AAVMF firmware"
)

print(
    "MAKOS_UEFI_KERNEL_HANDOFF_TEST_OK allocation=loader-code "
    "direct-entry=post-exit-boot-services firmware-xp=compatible"
)
