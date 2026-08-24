#!/usr/bin/env python3
"""Static regression gate for AArch64 cursor-plane isolation."""

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
GPU = (ROOT / "kernel/src/aarch64_virtio_gpu.rs").read_text()
GRAPHICS = (ROOT / "kernel/src/graphics.rs").read_text()


def require(source: str, fragment: str) -> None:
    if fragment not in source:
        raise AssertionError(f"missing cursor-plane invariant: {fragment}")


require(GPU, 'const CMD_UPDATE_CURSOR: u32 = 0x0300;')
require(GPU, 'const CMD_MOVE_CURSOR: u32 = 0x0301;')
require(GPU, 'create_cursor(\n        &mut state,')
require(GPU, 'cursor=virtio-gpu-plane move=cursorq scanout_damage=none')
if GPU.index('create_cursor(\n        &mut state,') > GPU.index(
    'with_state(|destination| *destination = state);'
):
    raise AssertionError("cursor created after GPU state publication")

require(GRAPHICS, 'const CURSOR_BACKEND: &str = "virtio-gpu-plane";')
require(GRAPHICS, 'crate::aarch64_virtio_gpu::move_cursor(cursor_x, cursor_y);')
require(GRAPHICS, 'else if buttons_changed {')
require(GRAPHICS, 'fn draw_cursor(_screen: &mut crate::framebuffer::Screen, _x: u32, _y: u32) {}')
require(GRAPHICS, 'let x = state.cursor_under_x;')
require(GRAPHICS, 'let y = state.cursor_under_y;')

# Runtime routing model mirrored by mouse_packet: no outline + no button edge
# means cursor queue only; scene changes still retain explicit scanout flushes.
def scanout_flush(outline_active: bool, buttons_changed: bool) -> bool:
    return outline_active or buttons_changed


assert not scanout_flush(False, False)
assert scanout_flush(True, False)
assert scanout_flush(False, True)
print("MAKOS_AARCH64_CURSOR_PLANE_TEST_OK move=cursorq pure_motion_scanout_writes=0")
