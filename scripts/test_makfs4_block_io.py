#!/usr/bin/env python3
"""Structural guard for AArch64 MakFS4 whole-block virtio transfers."""

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BLOCK = (ROOT / "kernel/src/block.rs").read_text()
VOLUME = (ROOT / "kernel/src/makfs4_volume.rs").read_text()


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


data_read = function_body(BLOCK, "pub fn read_sectors_8")
data_write = function_body(BLOCK, "pub fn write_sectors_8")
read_block = function_body(VOLUME, "fn read_block")
write_block = function_body(VOLUME, "fn write_block")

assert "physical_lba(u64::from(lba), 8)" in data_read
assert "aarch64_virtio_blk::read_sectors_8" in data_read
assert "physical_lba(u64::from(lba), 8)" in data_write
assert "aarch64_virtio_blk::write_sectors_8_on" in data_write
assert "makos_makfs4::block_first_sector(block, SECTOR_BYTES)" in read_block
assert '#[cfg(target_arch = "aarch64")]' in read_block
assert "return disk.read_sectors_8(first_lba, output);" in read_block
assert "makos_makfs4::block_first_sector(block, SECTOR_BYTES)" in write_block
assert '#[cfg(target_arch = "aarch64")]' in write_block
assert "return disk.write_sectors_8(first_lba, input);" in write_block
assert "for sector in 0..SECTORS_PER_BLOCK as usize" in read_block
assert "for sector in 0..SECTORS_PER_BLOCK as usize" in write_block

print(
    "MAKOS_MAKFS4_BLOCK_IO_TEST_OK "
    "aarch64=request-per-4k read=1 write=1 sectors=8 bounds=partition-aware "
    "other-arches=sector-fallback"
)
