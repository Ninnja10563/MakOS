#!/usr/bin/env python3
"""Boot MakOS AArch64 through UEFI/HVF and verify serial plus framebuffer output."""

from __future__ import annotations

import json
import os
import pathlib
import platform
import re
import selectors
import shutil
import socket
import subprocess
import tempfile
import time
from urllib.parse import urlsplit, urlunsplit

ROOT = pathlib.Path(__file__).resolve().parent.parent
IMAGE = pathlib.Path(
    os.environ.get("MAKOS_AARCH64_IMAGE", ROOT / "build/makos-aarch64.img")
)
SCREENSHOT = ROOT / "build/makos-aarch64.ppm"
LOGIN_BACKSPACE_SCREENSHOT = ROOT / "build/makos-login-backspace.ppm"
MODE_SCREENSHOT = ROOT / "build/makos-aarch64-1024x768.ppm"
START_MENU_SCREENSHOT = ROOT / "build/makos-start-menu.ppm"
SYSTEM_MONITOR_SCREENSHOT = ROOT / "build/makos-system-monitor.ppm"
SETTINGS_USERS_SCREENSHOT = ROOT / "build/makos-settings-users.ppm"
BUTTON_PRESSED_SCREENSHOT = ROOT / "build/makos-button-pressed.ppm"
FIREFOX_SCREENSHOT = ROOT / "build/makos-firefox-live.ppm"
FIREFOX_CHROME_SCREENSHOT = ROOT / "build/makos-firefox-chrome.ppm"
FIREFOX_PAGE_SCREENSHOT = ROOT / "build/makos-firefox-page.ppm"
FIREFOX_LINK_SCREENSHOT = ROOT / "build/makos-firefox-link.ppm"
FIREFOX_SELECTION_BASE_SCREENSHOT = ROOT / "build/makos-firefox-selection-base.ppm"
FIREFOX_SELECTION_SCREENSHOT = ROOT / "build/makos-firefox-selection.ppm"
FIREFOX_URL_SELECTION_SCREENSHOT = ROOT / "build/makos-firefox-url-selection.ppm"
FIREFOX_SELECTION_ENTRY_SCREENSHOT = ROOT / "build/makos-firefox-selection-entry.ppm"
FIREFOX_SCROLL_BASE_SCREENSHOT = ROOT / "build/makos-firefox-scroll-base.ppm"
FIREFOX_SCROLL_SCREENSHOT = ROOT / "build/makos-firefox-scroll.ppm"
FIREFOX_SCROLL_RESTORED_SCREENSHOT = ROOT / "build/makos-firefox-scroll-restored.ppm"
FIREFOX_FORM_BASE_SCREENSHOT = ROOT / "build/makos-firefox-form-base.ppm"
FIREFOX_FORM_SCREENSHOT = ROOT / "build/makos-firefox-form.ppm"
CURSOR_BASELINE_SCREENSHOT = ROOT / "build/makos-cursor-baseline.ppm"
CURSOR_AFTER_SCREENSHOT = ROOT / "build/makos-cursor-after.ppm"
CURSOR_MOVE_SCREENSHOTS = tuple(
    ROOT / f"build/makos-cursor-move-{index}.ppm" for index in range(6)
) + (CURSOR_AFTER_SCREENSHOT,)
CURSOR_MOVE_POSITIONS = (
    (40, 80),
    (200, 110),
    (400, 200),
    (700, 400),
    (130, 575),
    (500, 575),
    (760, 540),
)
TIMEOUT_SECONDS = 50


def first_file(environment: str, candidates: tuple[str, ...]) -> pathlib.Path:
    configured = os.environ.get(environment)
    choices = ((configured,) if configured else ()) + candidates
    for choice in choices:
        path = pathlib.Path(choice)
        if path.is_file():
            return path
    raise FileNotFoundError(f"{environment} firmware not found")


def copy_sparse(source: pathlib.Path, destination: pathlib.Path) -> None:
    with source.open("rb") as input_file, destination.open("wb") as output_file:
        while block := input_file.read(1024 * 1024):
            if block.count(0) == len(block):
                output_file.seek(len(block), 1)
            else:
                output_file.write(block)
        output_file.truncate(source.stat().st_size)


def qmp_command(stream, execute: str, arguments: dict[str, object] | None = None) -> dict:
    request: dict[str, object] = {"execute": execute}
    if arguments:
        request["arguments"] = arguments
    stream.write(json.dumps(request).encode("ascii") + b"\n")
    stream.flush()
    while True:
        response = json.loads(stream.readline())
        if "return" in response or "error" in response:
            return response


def wait_for_socket(path: pathlib.Path, process: subprocess.Popen[bytes]) -> None:
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        if path.exists():
            return
        if process.poll() is not None:
            raise RuntimeError("QEMU exited before QMP became ready")
        time.sleep(0.05)
    raise TimeoutError("QMP socket timeout")


def wait_for_output(
    selector: selectors.BaseSelector,
    process: subprocess.Popen[bytes],
    output: bytearray,
    marker: bytes,
    timeout: float = 10,
) -> None:
    deadline = time.monotonic() + timeout
    while marker not in output and time.monotonic() < deadline:
        for key, _ in selector.select(timeout=0.1):
            chunk = os.read(key.fileobj.fileno(), 4096)
            if chunk:
                output.extend(chunk)
        if process.poll() is not None:
            break
    if marker not in output:
        raise AssertionError(
            f"missing interactive marker {marker!r}\n{output.decode(errors='replace')}"
        )


def wait_for_new_output(
    selector: selectors.BaseSelector,
    process: subprocess.Popen[bytes],
    output: bytearray,
    marker: bytes,
    timeout: float = 10,
) -> None:
    previous = output.count(marker)
    deadline = time.monotonic() + timeout
    while output.count(marker) == previous and time.monotonic() < deadline:
        for key, _ in selector.select(timeout=0.1):
            chunk = os.read(key.fileobj.fileno(), 4096)
            if chunk:
                output.extend(chunk)
        if process.poll() is not None:
            break
    if output.count(marker) == previous:
        raise AssertionError(
            f"missing new interactive marker {marker!r}\n"
            f"{output.decode(errors='replace')}"
        )


def wait_for_output_count(
    selector: selectors.BaseSelector,
    process: subprocess.Popen[bytes],
    output: bytearray,
    marker: bytes,
    expected: int,
    timeout: float = 10,
) -> None:
    deadline = time.monotonic() + timeout
    while output.count(marker) < expected and time.monotonic() < deadline:
        for key, _ in selector.select(timeout=0.1):
            chunk = os.read(key.fileobj.fileno(), 4096)
            if chunk:
                output.extend(chunk)
        if process.poll() is not None:
            break
    if output.count(marker) < expected:
        raise AssertionError(
            f"missing marker occurrence {expected} for {marker!r}\n"
            f"{output.decode(errors='replace')}"
        )


def drain_output(
    selector: selectors.BaseSelector,
    process: subprocess.Popen[bytes],
    output: bytearray,
    duration: float,
) -> None:
    deadline = time.monotonic() + duration
    while time.monotonic() < deadline and process.poll() is None:
        for key, _ in selector.select(timeout=min(0.05, deadline - time.monotonic())):
            chunk = os.read(key.fileobj.fileno(), 4096)
            if chunk:
                output.extend(chunk)


def process_cpu_seconds(pid: int) -> float:
    """Read cumulative CPU time without sampling or disturbing guest."""
    value = subprocess.check_output(
        ["ps", "-o", "time=", "-p", str(pid)], text=True
    ).strip()
    days = 0
    if "-" in value:
        day_text, value = value.split("-", 1)
        days = int(day_text)
    fields = value.split(":")
    if len(fields) == 2:
        hours = 0
        minutes, seconds = fields
    elif len(fields) == 3:
        hours, minutes, seconds = fields
    else:
        raise AssertionError(f"unexpected ps CPU time {value!r}")
    return days * 86400 + int(hours) * 3600 + int(minutes) * 60 + float(seconds)


def process_resident_bytes(pid: int) -> int:
    value = subprocess.check_output(
        ["ps", "-o", "rss=", "-p", str(pid)], text=True
    ).strip()
    if not value:
        raise AssertionError(f"host process {pid} has no RSS sample")
    return int(value) * 1024


def send_key(stream, key: str) -> None:
    response = qmp_command(
        stream,
        "human-monitor-command",
        {"command-line": f"sendkey {key} 40"},
    )
    if "error" in response:
        raise AssertionError(f"sendkey {key} failed: {response}")
    # QEMU emits press, SYN, release, SYN for each key. Leave enough time for
    # the 32-entry virtio-input queue to recycle all four descriptors under
    # HVF host scheduling load; a 20 ms post-release gap was intermittently
    # dropping the tail of login credentials.
    time.sleep(0.10)


def send_command(stream, command: str) -> None:
    key_names = {
        " ": "spc",
        ".": "dot",
        "-": "minus",
        "/": "slash",
        "(": "shift-9",
        ")": "shift-0",
        "[": "bracket_left",
        "]": "bracket_right",
        "*": "shift-8",
        "+": "shift-equal",
        ";": "semicolon",
        ":": "shift-semicolon",
        "=": "equal",
        "_": "shift-minus",
        ",": "comma",
        "<": "shift-comma",
        ">": "shift-dot",
        "?": "shift-slash",
        "@": "shift-2",
        "'": "apostrophe",
        '"': "shift-apostrophe",
        "\\": "backslash",
    }
    for character in command:
        send_key(stream, key_names.get(character, character))
    send_key(stream, "ret")


def send_pointer(
    stream,
    x: int,
    y: int,
    button: bool | None = None,
    screen_width: int = 800,
    screen_height: int = 600,
) -> None:
    events: list[dict[str, object]] = [
        {
            "type": "abs",
            "data": {"axis": "x", "value": round(x * 32767 / (screen_width - 1))},
        },
        {
            "type": "abs",
            "data": {"axis": "y", "value": round(y * 32767 / (screen_height - 1))},
        },
    ]
    if button is not None:
        events.append(
            {"type": "btn", "data": {"down": button, "button": "left"}}
        )
    response = qmp_command(stream, "input-send-event", {"events": events})
    if "error" in response:
        raise AssertionError(f"pointer injection failed: {response}")
    time.sleep(0.06)


def click_pointer(stream, x: int, y: int) -> None:
    send_pointer(stream, x, y)
    send_pointer(stream, x, y, True)
    send_pointer(stream, x, y, False)


def send_wheel(stream, direction: str) -> None:
    if direction not in ("wheel-up", "wheel-down"):
        raise ValueError(f"invalid wheel direction {direction!r}")
    events = [
        {"type": "btn", "data": {"down": True, "button": direction}},
        {"type": "btn", "data": {"down": False, "button": direction}},
    ]
    response = qmp_command(stream, "input-send-event", {"events": events})
    if "error" in response:
        raise AssertionError(f"wheel injection failed: {response}")
    time.sleep(0.10)


def send_text(stream, value: str) -> None:
    key_names = {
        ":": "shift-semicolon",
        "/": "slash",
        ".": "dot",
        "?": "shift-slash",
        "=": "equal",
        "-": "minus",
        "_": "shift-minus",
        "&": "shift-7",
    }
    for character in value:
        key = key_names.get(character, character)
        if "A" <= character <= "Z":
            key = f"shift-{character.lower()}"
        send_key(stream, key)


def verify_ppm(path: pathlib.Path, *, require_browser_page: bool = True) -> None:
    header, dimensions, maximum, pixels = path.read_bytes().split(b"\n", 3)
    if header != b"P6" or maximum != b"255":
        raise AssertionError("AArch64 screenshot is not P6 PPM")
    width, height = (int(value) for value in dimensions.split())
    if (width, height) != (800, 600) or len(pixels) != width * height * 3:
        raise AssertionError("unexpected AArch64 framebuffer dimensions")
    colors = [pixels[index : index + 3] for index in range(0, len(pixels), 3)]
    minimums = {
        b"\x00\x80\x80": 80_000,
        b"\xc0\xc0\xc0": 40_000,
        b"\x00\x00\x00": 20_000,
    }
    if require_browser_page:
        minimums[b"\xff\xff\xff"] = 30_000
    for color, minimum in minimums.items():
        if colors.count(color) < minimum:
            raise AssertionError(f"missing expected theme color {color.hex()}")
    # At 800px, app labels must not spill across taskbar button gaps.
    for left, right in ((210, 214), (298, 302), (386, 390), (474, 478)):
        for y in range(568, 586):
            for x in range(left, right):
                offset = (y * width + x) * 3
                if pixels[offset : offset + 3] == b"\x00\x00\x00":
                    raise AssertionError("taskbar label escaped button bounds")


def verify_start_menu_ppm(path: pathlib.Path) -> None:
    header, dimensions, maximum, pixels = path.read_bytes().split(b"\n", 3)
    if header != b"P6" or dimensions != b"800 600" or maximum != b"255":
        raise AssertionError("unexpected Start menu screenshot format")
    # Created apps must occupy consecutive 40px rows. Text Edit has a reserved
    # stable slot but is not created yet; it must not create a visual hole.
    expected_icons = (
        (30, 362, b"\x00\x80\x80"),  # Monitor
        (30, 402, b"\x00\x00\x00"),  # Terminal
        (30, 442, b"\x00\x80\x80"),  # Settings
        (30, 482, b"\x00\x80\x80"),  # Browser
        (30, 522, b"\x00\x80\x80"),  # Files
    )
    for x, y, color in expected_icons:
        offset = (y * 800 + x) * 3
        if pixels[offset : offset + 3] != color:
            raise AssertionError("Start menu contains reserved-slot gap")


def verify_login_backspace_ppm(path: pathlib.Path) -> None:
    header, dimensions, maximum, pixels = path.read_bytes().split(b"\n", 3)
    if header != b"P6" or dimensions != b"800 600" or maximum != b"255":
        raise AssertionError("unexpected login artifact screenshot format")
    # Password field interior after seven masked characters then seven deletes.
    # Only caret at x=252 may remain; former cells x>=264 must be white.
    stale_black = 0
    for y in range(286, 309):
        for x in range(264, 360):
            offset = (y * 800 + x) * 3
            if pixels[offset : offset + 3] == b"\x00\x00\x00":
                stale_black += 1
    if stale_black:
        raise AssertionError(
            f"login backspace left {stale_black} black caret artifact pixels"
        )


def verify_live_ui_ppm(path: pathlib.Path, label: str) -> None:
    header, dimensions, maximum, pixels = path.read_bytes().split(b"\n", 3)
    if header != b"P6" or dimensions != b"800 600" or maximum != b"255":
        raise AssertionError(f"unexpected {label} screenshot format")
    black = pixels.count(b"\x00\x00\x00")
    white = pixels.count(b"\xff\xff\xff")
    if black < 500 or white < 5_000:
        raise AssertionError(f"{label} lacks rendered controls/text")


def verify_pressed_button_ppm(path: pathlib.Path) -> None:
    header, dimensions, maximum, pixels = path.read_bytes().split(b"\n", 3)
    if header != b"P6" or dimensions != b"800 600" or maximum != b"255":
        raise AssertionError("unexpected pressed-button screenshot format")
    # Settings Add User button starts at (538,430). Sunken bevel's top/left
    # edge is system shadow; raised bevel would be white.
    for x, y in ((538, 430), (550, 430), (538, 440)):
        offset = (y * 800 + x) * 3
        if pixels[offset : offset + 3] != b"\x80\x80\x80":
            raise AssertionError("button did not render sunken during mouse-down")


def verify_cursor_scene_stable(before: pathlib.Path, after: pathlib.Path) -> None:
    baseline = before.read_bytes()
    moved = after.read_bytes()
    if baseline == moved:
        return
    changed = sum(first != second for first, second in zip(baseline, moved))
    changed += abs(len(baseline) - len(moved))
    raise AssertionError(f"cursor movement changed scene bytes={changed}")


def ppm_pixels(path: pathlib.Path) -> tuple[int, int, bytes]:
    header, dimensions, maximum, pixels = path.read_bytes().split(b"\n", 3)
    width, height = (int(value) for value in dimensions.split())
    if header != b"P6" or maximum != b"255" or len(pixels) != width * height * 3:
        raise AssertionError(f"invalid cursor regression PPM: {path}")
    return width, height, pixels


def verify_firefox_probe_markers(output: bytearray, start: int) -> None:
    firefox_output = output[start:]
    missing = [
        marker
        for marker in (b"MAKOS_JIT_POOL_OK", b"MAKOS_FIREFOX_BLIT")
        if marker not in firefox_output
    ]
    if missing:
        raise AssertionError(f"missing Firefox probe markers: {missing!r}")


def verify_firefox_client_ppm(path: pathlib.Path) -> None:
    width, height, pixels = ppm_pixels(path)
    if (width, height) != (800, 600):
        raise AssertionError("unexpected Firefox screenshot dimensions")
    # Exclude MakOS frame/title/resize grip. A single Gecko clear-to-gray blit
    # is not a usable browser UI; require real variation inside client area.
    colors: dict[bytes, int] = {}
    total = 0
    for y in range(115, 480):
        row_start = (y * width + 70) * 3
        row_end = (y * width + 730) * 3
        for offset in range(row_start, row_end, 3):
            pixel = pixels[offset : offset + 3]
            colors[pixel] = colors.get(pixel, 0) + 1
            total += 1
    dominant = max(colors.values(), default=total)
    if len(colors) >= 8 and dominant * 200 < total * 199:
        return
    raise AssertionError(
        "Firefox browser client remained blank/uniform "
        f"colors={len(colors)} dominant={dominant}/{total}: {path}"
    )


def firefox_document_changed_pixels(before: pathlib.Path, after: pathlib.Path) -> int:
    before_width, before_height, before_pixels = ppm_pixels(before)
    after_width, after_height, after_pixels = ppm_pixels(after)
    if (before_width, before_height) != (after_width, after_height):
        raise AssertionError("Firefox page screenshot dimensions changed")
    changed = 0
    colors: dict[bytes, int] = {}
    total = 0
    # Central document body only. Exclude chrome plus Firefox's bottom status
    # strip: a URL/status update is not evidence that document content painted.
    for y in range(220, 450):
        start = (y * before_width + 100) * 3
        end = (y * before_width + 700) * 3
        for offset in range(start, end, 3):
            pixel = after_pixels[offset : offset + 3]
            colors[pixel] = colors.get(pixel, 0) + 1
            total += 1
            if before_pixels[offset : offset + 3] != pixel:
                changed += 1
    dominant = max(colors.values(), default=total)
    if changed < 1000 or len(colors) < 8 or dominant * 1000 >= total * 999:
        raise AssertionError(
            "Firefox document body did not materially paint "
            f"pixels={changed} colors={len(colors)} dominant={dominant}/{total}"
        )
    return changed


def verify_firefox_page_changed(before: pathlib.Path, after: pathlib.Path) -> None:
    firefox_document_changed_pixels(before, after)


def firefox_document_difference_pixels(before: pathlib.Path, after: pathlib.Path) -> int:
    width, height, before_pixels = ppm_pixels(before)
    after_width, after_height, after_pixels = ppm_pixels(after)
    if (width, height) != (after_width, after_height):
        raise AssertionError("Firefox screenshot dimensions changed")
    changed = 0
    for y in range(220, 450):
        start = (y * width + 100) * 3
        end = (y * width + 700) * 3
        for offset in range(start, end, 3):
            changed += before_pixels[offset : offset + 3] != after_pixels[offset : offset + 3]
    return changed


def verify_firefox_page_restored(
    reference: pathlib.Path, scrolled: pathlib.Path, after: pathlib.Path
) -> tuple[int, int]:
    scrolled_changed = firefox_document_changed_pixels(reference, scrolled)
    restored_changed = firefox_document_difference_pixels(reference, after)
    # Network pages can settle by several pixels after first load. Require the
    # reverse wheel to recover at least two thirds of the document-body delta.
    if restored_changed * 3 >= scrolled_changed:
        raise AssertionError(
            "Firefox reverse wheel did not recover document position "
            f"scrolled_pixels={scrolled_changed} restored_pixels={restored_changed}"
        )
    return scrolled_changed, restored_changed


def verify_firefox_selection_changed(
    before: pathlib.Path,
    after: pathlib.Path,
    bounds: tuple[int, int, int, int],
) -> None:
    before_width, before_height, before_pixels = ppm_pixels(before)
    after_width, after_height, after_pixels = ppm_pixels(after)
    if (before_width, before_height) != (after_width, after_height):
        raise AssertionError("Firefox selection screenshot dimensions changed")
    left, top, right, bottom = bounds
    if not (0 <= left < right <= before_width and 0 <= top < bottom <= before_height):
        raise AssertionError("Firefox selection bounds invalid")
    changed = 0
    for y in range(top, bottom):
        for x in range(left, right):
            offset = (y * before_width + x) * 3
            if before_pixels[offset : offset + 3] != after_pixels[offset : offset + 3]:
                changed += 1
    if changed < 80:
        raise AssertionError(f"Firefox document selection highlight too small pixels={changed}")


def verify_firefox_url_selected(path: pathlib.Path) -> None:
    width, height, pixels = ppm_pixels(path)
    if (width, height) != (800, 600):
        raise AssertionError("unexpected Firefox URL-selection dimensions")
    selected = 0
    for y in range(150, 178):
        for x in range(250, 575):
            offset = (y * width + x) * 3
            red, green, blue = pixels[offset : offset + 3]
            if blue > red + 60 and blue > green + 30:
                selected += 1
    if selected < 1000:
        raise AssertionError(
            f"Firefox URL selection not visually active pixels={selected}"
        )


def verify_cursor_plane_scene_stable(
    baseline: pathlib.Path,
    snapshots: tuple[pathlib.Path, ...],
) -> None:
    """Require cursor motion to leave every scanout pixel unchanged.

    QMP screendump captures scanout, not virtio-GPU cursor-plane composition.
    Cursor visibility is proved by the accepted UPDATE_CURSOR/MOVE_CURSOR
    runtime markers; these frames prove that moving it never paints, restores,
    or damages the underlying desktop scene.
    """
    width, height, baseline_pixels = ppm_pixels(baseline)
    for index, path in enumerate(snapshots):
        frame_width, frame_height, pixels = ppm_pixels(path)
        if (frame_width, frame_height) != (width, height):
            raise AssertionError("cursor move changed screenshot dimensions")
        if pixels != baseline_pixels:
            changed = sum(
                pixels[offset : offset + 3]
                != baseline_pixels[offset : offset + 3]
                for offset in range(0, len(pixels), 3)
            )
            raise AssertionError(
                f"cursor-plane move {index} modified scanout pixels={changed}"
            )


def verify_mode_ppm(path: pathlib.Path, expected: tuple[int, int]) -> None:
    header, dimensions, maximum, pixels = path.read_bytes().split(b"\n", 3)
    width, height = (int(value) for value in dimensions.split())
    if header != b"P6" or maximum != b"255" or (width, height) != expected:
        raise AssertionError(
            f"virtio-gpu mode mismatch: {(width, height)} != {expected}"
        )
    if len(pixels) != width * height * 3:
        raise AssertionError("virtio-gpu screenshot byte count mismatch")


def main() -> int:
    qemu = os.environ.get("QEMU_SYSTEM_AARCH64", "qemu-system-aarch64")
    code = first_file(
        "AAVMF_CODE",
        (
            "/opt/homebrew/share/qemu/edk2-aarch64-code.fd",
            "/usr/local/share/qemu/edk2-aarch64-code.fd",
            "/usr/share/AAVMF/AAVMF_CODE.fd",
        ),
    )
    vars_template = first_file(
        "AAVMF_VARS",
        (
            "/opt/homebrew/share/qemu/edk2-arm-vars.fd",
            "/usr/local/share/qemu/edk2-arm-vars.fd",
            "/usr/share/AAVMF/AAVMF_VARS.fd",
        ),
    )
    default_accel = "hvf" if platform.system() == "Darwin" and platform.machine() == "arm64" else "tcg"
    accel = os.environ.get("MAKOS_AARCH64_ACCEL", default_accel)
    temporary_root = pathlib.Path(
        os.environ.get("MAKOS_AARCH64_TEMP_ROOT", str(ROOT / "build"))
    )
    with tempfile.TemporaryDirectory(
        prefix="makos-aarch64-test-", dir=temporary_root
    ) as temporary:
        temp = pathlib.Path(temporary)
        vars_copy = temp / "vars.fd"
        boot_image = temp / "boot.img"
        qmp_path = temp / "qmp.sock"
        data_image = temp / "data.img"
        system_image = temp / "system.img"
        configured_system_image = os.environ.get("MAKOS_AARCH64_GPT_IMAGE")
        package_image = os.environ.get("MAKOS_AARCH64_PACKAGE_IMAGE")
        if configured_system_image:
            if package_image:
                raise ValueError("GPT system image and package data image are mutually exclusive")
            copy_sparse(pathlib.Path(configured_system_image), system_image)
        else:
            if package_image:
                copy_sparse(pathlib.Path(package_image), data_image)
            else:
                with data_image.open("wb") as output_file:
                    output_file.truncate(1024 * 1024 * 1024)
            shutil.copyfile(IMAGE, boot_image)
        shutil.copyfile(vars_template, vars_copy)
        qmp_parent = None
        qmp_child = None
        if os.environ.get("MAKOS_QMP_SOCKETPAIR") == "1":
            qmp_parent, qmp_child = socket.socketpair()
            qmp_arguments = [
                "-chardev",
                f"socket,id=makosqmp,fd={qmp_child.fileno()}",
                "-qmp",
                "chardev:makosqmp",
            ]
        else:
            qmp_arguments = ["-qmp", f"unix:{qmp_path},server=on,wait=off"]
        pcap_path = os.environ.get("MAKOS_AARCH64_PCAP")
        pcap_arguments = (
            ["-object", f"filter-dump,id=makosnetdump,netdev=makosnet,file={pcap_path}"]
            if pcap_path
            else []
        )
        disk_arguments = (
            [
                "-drive",
                f"id=system,if=none,format=raw,file={system_image}",
                "-device",
                "virtio-blk-device,drive=system,bootindex=0",
            ]
            if configured_system_image
            else [
                "-drive",
                f"id=boot,if=none,format=raw,readonly=on,file={boot_image}",
                "-device",
                "virtio-blk-pci,drive=boot",
                "-drive",
                f"id=data,if=none,format=raw,file={data_image}",
                "-device",
                "virtio-blk-device,drive=data",
            ]
        )
        command = [
            qemu,
            "-machine",
            f"virt,accel={accel},highmem=off,gic-version=2",
            "-cpu",
            "host" if accel == "hvf" else "max",
            "-global",
            "virtio-mmio.force-legacy=false",
            "-smp",
            "4",
            "-m",
            "1G",
            "-drive",
            f"if=pflash,format=raw,readonly=on,file={code}",
            "-drive",
            f"if=pflash,format=raw,file={vars_copy}",
            *disk_arguments,
            "-device",
            "virtio-keyboard-device",
            "-device",
            "virtio-tablet-device",
            "-netdev",
            "user,id=makosnet",
            "-device",
            "virtio-net-device,netdev=makosnet,mac=52:54:00:12:34:56",
            *pcap_arguments,
            "-device",
            "virtio-gpu-device,xres=800,yres=600",
            "-object",
            "rng-random,id=makosrng,filename=/dev/urandom",
            "-device",
            "virtio-rng-device,rng=makosrng",
            "-display",
            "none",
            "-serial",
            "stdio",
            "-monitor",
            "none",
            *qmp_arguments,
            "-no-reboot",
            "-no-shutdown",
        ]
        process = subprocess.Popen(
            command,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            pass_fds=(() if qmp_child is None else (qmp_child.fileno(),)),
        )
        if qmp_child is not None:
            qmp_child.close()
        assert process.stdout is not None
        output = bytearray()
        selector = selectors.DefaultSelector()
        selector.register(process.stdout, selectors.EVENT_READ)
        deadline = time.monotonic() + TIMEOUT_SECONDS
        marker = (
            b"MAKOS_AARCH64_BOOT_OK uefi=1 hvf_ready=1 native_isa=1 "
            b"framebuffer=800x600 gpu=virtio pmm=1 heap=1 mmu=1 exceptions=1 gic=2 timer=1 userspace=1 svc=1 input=virtio desktop=login\r\n"
        )
        try:
            while marker not in output and time.monotonic() < deadline:
                for key, _ in selector.select(timeout=0.25):
                    chunk = os.read(key.fileobj.fileno(), 4096)
                    if chunk:
                        output.extend(chunk)
                if process.poll() is not None:
                    break
                if b"MAKOS_FATAL:" in output:
                    break
            required = (
                b"MakOS loader v0.1",
                b"MakOS AArch64 kernel: entry",
                b"arch=aarch64",
                b"pmm managed_mib=",
                b"heap bytes=1048576 box_vec=ok",
                b"MAKOS_CREDENTIAL_POLICY_OK unknown_pid=denied kernel_context=denied explicit_pid1=1 ambient_caps=0",
                b"paging arch=aarch64 ttbr0=",
                b"identity_gib=2 owned=1",
                b"acpi arch=aarch64 cpus=4 gic_version=2",
                b"virtual_timer_gsiv=27",
                b"MAKOS_AARCH64_IRQ_TRACE stage=ack",
                b"intid=27 group1_alias=0",
                b"MAKOS_AARCH64_SMP_OK discovered=4 online=4",
                b"MAKOS_AARCH64_SMP_USER_OK cpus=4",
                b"MAKOS_AARCH64_SMP_IPC_OK waiter_cpu=1 signaler_cpu=0",
                b"MAKOS_AARCH64_SMP_EXIT_GROUP_OK caller_cpu=0 stopped_cpu_mask=0x2 ack_mask=0x2",
                b"MAKOS_AARCH64_SMP_EXIT_GROUP_EL1_OK caller_cpu=0 stopped_cpu_mask=0x2 ack_mask=0x2 entered_el1_mask=0x2 deferred_ack_mask=0x2",
                b"MAKOS_AARCH64_SMP_CONCURRENT_EXIT_GROUP_OK callers=2 cpu_mask=0x3 rendezvous_mask=0x3 serialized_acquire_mask=0x3 statuses=57,58",
                b"MAKOS_AARCH64_SMP_SAME_GROUP_EXIT_OK callers=2 cpu_mask=0x3",
                b"aps=3 psci=",
                b"conduit=hvc cpu_on=64 per_cpu_stacks=1 stack_bytes=1048576 per_cpu_vbar=1 coherent_parallel_el1=1",
                b"userspace_scheduler_cpus=1 aps_after_test=idle scheduler_gate=closed ap_idle=wfi",
                b"MAKOS_AARCH64_CORE_OK el=1 stack=owned mmu=owned vectors=1 brk_return=1 gic=2 timer_hz=100",
                b"timer_freq=",
                b"acpi=1",
                b"MakOS AArch64 EL0 init running",
                b"MAKOS_AARCH64_SCHED_CHILD_OK pid=2 register_restore=x0-x30,sp_el0,q0,q8,q16,q31,fpcr,fpsr preemptions=multiple pattern=child exit=42",
                b"MAKOS_AARCH64_SCHED_USER_OK parent=1 child=2 spawn=1 timer_only=1 concurrent=1 patterns=distinct wait=1 exit=42 register_restore=x0-x30,sp_el0,q0,q8,q16,q31,fpcr,fpsr preemptions=multiple",
                b"MAKOS_AARCH64_SCHEDULER_OK processes=2 timer_preemptions=",
                b"context=x0-x30,elr,spsr,sp_el0,ttbr0,tpidr_el0,q0-q31,fpcr,fpsr isolated_ttbr0=1 spawn=1 concurrent=1 patterns=distinct exit=1 wait=1 reap=1 free_balance=1",
                b"MAKOS_AARCH64_USER_OK pid=1 el=0 elf=1 svc=1 write=1 abi=1 clock=1 isolation=ttbr0",
                b"MAKOS_AARCH64_ABI_OK version=1.0 normative_max=57 target_extension_max=147 features=ipc,process,vm,vfs,network,graphics,auth,log,sync,ipv6,selfhost-seed,sockets,packages,vm-regions,exec-path,startup-vectors,tty-signals,typed-ipc truthful=1",
                b"MAKOS_AARCH64_LOG_OK structured=1 ring=32 pid=1 severity=5 monotonic=1 readback=1",
                b"MAKOS_STRUCTURED_LOG_PERSIST_OK path=/.makos-system-log format=MAKLOG01 records=",
                b"MAKOS_AARCH64_PROCESS_OK pid=1 exit=42 scheduler=saved-context address_space=isolated lifecycle=ready,running,zombie,reaped",
                b"free_balance=1",
                b"MAKOS_AARCH64_BLOCK_OK transport=virtio-mmio",
                b"read=1 write=1 flush=request",
                b"MAKOS_AARCH64_NET_OK transport=virtio-net-mmio",
                b"ethernet=1 dhcp=1 arp=1 ipv4_stack=1 udp=1 tcp=1 host_proxy=0",
                b"MAKOS_AARCH64_IPV6_READY address=",
                b"slaac=ra,eui64 ndp=ns,na udp6=1 tcp6=1 fake_mapping=0",
                b"MAKOS_AARCH64_RNG_OK transport=virtio-rng-mmio",
                b"source=host-urandom queue=1 bytes=32 zeroized=1",
                b"MAKOS_AARCH64_RTC_OK transport=pl031 clock=realtime",
                b"tls_validation=ready",
                b"MAKOS_AARCH64_GPU_OK transport=virtio-mmio",
                b"mode=800x600",
                b"transfer=2d flush=dirty",
                b"cursor=virtio-gpu-plane move=cursorq scanout_damage=none host-cursor=hidden",
                b"MAKOS_M4_OK ata_sectors=",
                b"makfs_generation=",
                b"boot_count=",
                b"MAKOS_AARCH64_INPUT_OK transport=virtio-mmio devices=2 eventq=32 polling=single-consumer event_drain=1 notify=batched pointer_motion=coalesced pointer_edges=preserved keyboard_syn=ignored absolute_pointer=1 keyboard=1",
                b"MAKOS_AARCH64_TTY_OK fds=0,1,2 controlling=1 canonical=1 raw=1 termios=1 ioctl_winsize=1 pgrp=1 signals=INT,QUIT,TSTP,WINCH sigreturn=kernel-saved",
                b"MAKOS_LOGIN_UI_OK framebuffer=800x600 prompt=visible console=live cursor=virtio-gpu-plane theme=95css-native",
                b"MAKOS_AARCH64_BOOT_OK uefi=1 hvf_ready=1 native_isa=1 framebuffer=800x600 gpu=virtio",
            )
            if package_image:
                required += (
                    b"MAKOS_PACKAGE_FS_OK files=",
                    b"disk_backed=1",
                )
            else:
                required += (
                    b"MAKOS_MAKFS4_READY state=",
                    b"block_bytes=4096 volume_blocks=262144 data_start=131072 max_inodes=512 extents=14 cow=inode,bitmap,catalog root=redundant flush=metadata,root",
                )
            if configured_system_image:
                required += (
                    b"MAKOS_GPT_DATA_OK start_lba=133120 sectors=2097152 legacy_raw=0",
                )
            missing = [value.decode("ascii") for value in required if value not in output]
            if package_image and not (
                b"MAKOS_MAKFS4_READY state=" in output
                or b"MAKOS_MAKFS4_DEFERRED reason=disk-below-1GiB legacy_v3=active"
                in output
            ):
                missing.append("MakFS4 ready-or-package-compatible deferred state")
            if missing:
                raise AssertionError(f"missing AArch64 markers: {missing}\n{output.decode(errors='replace')}")
            persisted = re.findall(
                rb"MAKOS_STRUCTURED_LOG_PERSIST_OK .* records=(\d+) next_sequence=(\d+) cow=makfs4",
                output,
            )
            if not persisted:
                raise AssertionError("structured-log marker malformed")
            persisted_records, persisted_next = map(int, persisted[-1])
            if not 1 <= persisted_records <= 32 or persisted_next <= persisted_records:
                raise AssertionError(
                    "structured-log state invalid "
                    f"records={persisted_records} next_sequence={persisted_next}"
                )
            if qmp_parent is None:
                wait_for_socket(qmp_path, process)
                client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                client.connect(str(qmp_path))
            else:
                client = qmp_parent
            with client:
                stream = client.makefile("rwb", buffering=0)
                json.loads(stream.readline())
                if "error" in qmp_command(stream, "qmp_capabilities"):
                    raise AssertionError("QMP capability negotiation failed")
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_AARCH64_SHELL_PROCESS_OK pid=1 elf=1 el=0",
                )
                click_pointer(stream, 390, 220)
                wait_for_output(selector, process, output, b"MAKOS_LOGIN_CLICK_OK")
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"cursor=virtio-gpu-plane redraw=cursorq-no-scanout no_trails=1",
                )
                send_pointer(stream, 750, 550)
                for key in ("m", "a", "r", "c", "u", "s", "tab"):
                    send_key(stream, key)
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_LOGIN_TAB_OK from=username to=password key=Tab focus=visible",
                )
                for key in ("x",) * 7 + ("backspace",) * 7:
                    send_key(stream, key)
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_LOGIN_BACKSPACE_OK field=password column=0 stale_caret=0",
                )
                response = qmp_command(
                    stream,
                    "screendump",
                    {"filename": str(LOGIN_BACKSPACE_SCREENSHOT)},
                )
                if "error" in response:
                    raise AssertionError(f"login screendump failed: {response}")
                verify_login_backspace_ppm(LOGIN_BACKSPACE_SCREENSHOT)
                for key in ("m", "a", "k", "o", "s"):
                    send_key(stream, key)
                click_pointer(stream, 575, 378)
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_LOGIN_BUTTON_OK source=mouse action=next-or-submit feedback=sunken-on-press",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_AARCH64_DESKTOP_OK login=1 apps=5 terminal=interactive taskbar=1 start_menu=1 drag=1 close=1 reopen=1 cursor=virtio-gpu-plane shell=el0 settings=1 browser=1 files=1 clock=utc network=dhcp wifi=unavailable-no-device",
                    TIMEOUT_SECONDS,
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_TERMINAL_ANSI_OK parser=bounded backend=cells cursor=1 erase=1 sgr=16-color alternate=1 winsize=58x22",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_TERMINAL_INPUT_OK discipline=raw bounded=256 lowercase=1 punctuation=1",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_AARCH64_BROWSER_PROCESS_OK pid=2 parent=1 elf=1 el=0",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_AARCH64_BROWSER_OK elf=1 surface=owned event_loop=1 native_transport=virtio-net",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_AARCH64_LOG_ACCESS_OK reader=browser cap_console=0 read=denied buffers=untouched",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_AARCH64_BROWSER_BACKGROUND_OK startup_fetch=0 reopen=start-menu state=retained",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_AARCH64_FILES_OK surface=owned vfs=real list=1 scroll=1 select=1 double_click=text-edit mutations=verified delete_ui=confirmation-required resize=1 reopen=start-menu",
                )
                # All desktop clients now block on input and accepted presents
                # require changed retained pixels. Prove quiescence after app
                # startup, including bounded host CPU while guest waits in WFI.
                idle_presents = output.count(b"MAKOS_M7_OK graphics_abi=1")
                idle_cpu_before = process_cpu_seconds(process.pid)
                idle_started = time.monotonic()
                drain_output(selector, process, output, 0.75)
                idle_elapsed = time.monotonic() - idle_started
                idle_cpu = process_cpu_seconds(process.pid) - idle_cpu_before
                if output.count(b"MAKOS_M7_OK graphics_abi=1") != idle_presents:
                    raise AssertionError("desktop accepted presents while completely idle")
                # TCG still gets present-quiescence coverage without a host-
                # dependent CPU ceiling.
                if accel == "hvf" and idle_cpu > idle_elapsed * 0.35:
                    raise AssertionError(
                        f"desktop idle CPU unbounded cpu={idle_cpu:.2f}s wall={idle_elapsed:.2f}s"
                    )
                print(
                    "MAKOS_AARCH64_DESKTOP_IDLE_OK accepted_presents=0 "
                    f"cpu_ratio={idle_cpu / idle_elapsed:.3f} wait=wfi dirty_present=1"
                )
                if os.environ.get("MAKOS_AARCH64_FIREFOX_PROBE") == "1":
                    click_pointer(stream, 250, 580)
                    wait_for_output(
                        selector,
                        process,
                        output,
                        b"MAKOS_TASKBAR_APP_OK surface=2 activate=1 minimize_toggle=1",
                    )
                    firefox_output_start = len(output)
                    send_command(stream, "firefox")
                    wait_for_output(
                        selector, process, output, b"MAKOS_FIREFOX_PROCESS_OK", 30
                    )
                    wait_for_output(
                        selector,
                        process,
                        output,
                        b"MAKOS_FIREFOX_LAUNCH_OK source=official-esr package=disk process=isolated",
                        30,
                    )
                    # Hide launcher terminal while Firefox initializes. This
                    # exposes its real surface and avoids repainting diagnostic
                    # output over every browser startup event.
                    click_pointer(stream, 250, 580)
                    probe_seconds = int(
                        os.environ.get("MAKOS_AARCH64_FIREFOX_PROBE_SECONDS", "10")
                    )
                    probe_started = time.monotonic()
                    probe_deadline = time.monotonic() + probe_seconds
                    next_visual_check = probe_started + 10
                    while time.monotonic() < probe_deadline and process.poll() is None:
                        for key, _ in selector.select(timeout=0.25):
                            chunk = os.read(key.fileobj.fileno(), 4096)
                            if chunk:
                                output.extend(chunk)
                        if b"MAKOS_FATAL:" in output:
                            serial_log = os.environ.get("MAKOS_AARCH64_FIREFOX_SERIAL_LOG")
                            if serial_log:
                                pathlib.Path(serial_log).write_bytes(output)
                            raise AssertionError(
                                "Firefox probe reached kernel fatal:\n"
                                + output[-12000:].decode(errors="replace")
                            )
                        firefox_probe_output = output[firefox_output_start:]
                        if (
                            time.monotonic() >= next_visual_check
                            and b"MAKOS_JIT_POOL_OK" in firefox_probe_output
                            and b"MAKOS_FIREFOX_BLIT" in firefox_probe_output
                        ):
                            next_visual_check = time.monotonic() + 5
                            response = qmp_command(
                                stream,
                                "screendump",
                                {"filename": str(FIREFOX_SCREENSHOT)},
                            )
                            if "error" in response:
                                raise AssertionError(
                                    f"Firefox screendump failed: {response}"
                                )
                            try:
                                verify_firefox_client_ppm(FIREFOX_SCREENSHOT)
                            except AssertionError:
                                pass
                            else:
                                break
                    firefox_probe_output = output[firefox_output_start:]
                    if (
                        os.environ.get("MAKOS_AARCH64_FIREFOX_DIAGNOSTIC_PS") == "1"
                        and b"MAKOS_PRES_PAINT uri=chrome://browser/content/browser.xhtml"
                        not in firefox_probe_output
                    ):
                        # Keep this opt-in: task-state snapshots make startup
                        # stalls actionable without weakening render proof.
                        try:
                            terminal_focused = False
                            for _ in range(2):
                                click_pointer(stream, 250, 580)
                                wait_for_new_output(
                                    selector,
                                    process,
                                    output,
                                    b"MAKOS_TASKBAR_APP_OK surface=2 activate=1 minimize_toggle=1",
                                    10,
                                )
                                click_pointer(stream, 100, 160)
                                try:
                                    wait_for_new_output(
                                        selector,
                                        process,
                                        output,
                                        b"MAKOS_CURSOR_FOCUS_OK cursor=virtio-gpu-plane buttons=left hit_test=1 focused_surface=2",
                                        10,
                                    )
                                except AssertionError:
                                    continue
                                terminal_focused = True
                                break
                            if not terminal_focused:
                                raise AssertionError("could not focus Terminal for Firefox diagnostics")
                            send_command(stream, "ps")
                            wait_for_new_output(
                                selector,
                                process,
                                output,
                                b"MAKOS_AARCH64_SHELL_CMD ps",
                                90,
                            )
                            time.sleep(5)
                            send_command(stream, "ps")
                            wait_for_new_output(
                                selector,
                                process,
                                output,
                                b"MAKOS_AARCH64_SHELL_CMD ps",
                                30,
                            )
                        except AssertionError:
                            # Preserve primary paint failure plus any partial
                            # snapshot instead of replacing it with diagnostics.
                            pass
                    serial_log = os.environ.get("MAKOS_AARCH64_FIREFOX_SERIAL_LOG")
                    if serial_log:
                        pathlib.Path(serial_log).write_bytes(output)
                    visible_surface = (
                        b"MAKOS_FIREFOX_SURFACE_OK handle=5 width=700 height=400 slot=5"
                    )
                    if visible_surface not in output:
                        raise AssertionError(
                            "Firefox survived but never created its visible 700x400 surface"
                        )
                    if (
                        b"MAKOS_FIREFOX_PROFILE_READY path=/home/user/firefox-profile "
                        b"state=created mode=0700 owner=session"
                        not in firefox_probe_output
                        and b"MAKOS_FIREFOX_PROFILE_READY path=/home/user/firefox-profile "
                        b"state=existing mode=0700 owner=session"
                        not in firefox_probe_output
                    ):
                        raise AssertionError("Firefox persistent profile was not ready")
                    if (
                        b"MAKOS_PRES_PAINT uri=chrome://browser/content/browser.xhtml"
                        not in firefox_probe_output
                    ):
                        if (
                            b"MAKOS_PRES_PAINT uri=chrome://global/content/commonDialog.xhtml"
                            in firefox_probe_output
                        ):
                            raise AssertionError(
                                "Firefox paint came from a dialog, not browser chrome"
                            )
                        raise AssertionError(
                            "Firefox did not paint browser chrome within probe window"
                        )
                    verify_firefox_probe_markers(output, firefox_output_start)
                    response = qmp_command(
                        stream,
                        "screendump",
                        {"filename": str(FIREFOX_SCREENSHOT)},
                    )
                    if "error" in response:
                        raise AssertionError(f"Firefox screendump failed: {response}")
                    verify_firefox_client_ppm(FIREFOX_SCREENSHOT)
                    print(
                        "MAKOS_FIREFOX_FIRST_PAINT_OK "
                        f"process_start_to_verified_paint_ms="
                        f"{int((time.monotonic() - probe_started) * 1000)}"
                    )
                    if os.environ.get("MAKOS_AARCH64_FIREFOX_NAVIGATE") == "1":
                        shutil.copyfile(FIREFOX_SCREENSHOT, FIREFOX_CHROME_SCREENSHOT)
                        target = os.environ.get(
                            "MAKOS_AARCH64_FIREFOX_NAVIGATION_TARGET",
                            "https://example.com",
                        )
                        target_parts = urlsplit(target)
                        if target_parts.scheme != "https" or not target_parts.hostname:
                            raise AssertionError(
                                "Firefox navigation proof currently requires an absolute HTTPS URL"
                            )
                        try:
                            expected_key_sequence = [*target.encode("ascii"), 10]
                        except UnicodeEncodeError as error:
                            raise AssertionError(
                                "Firefox QEMU keyboard proof currently requires an ASCII URL"
                            ) from error
                        expected_uri = os.environ.get(
                            "MAKOS_AARCH64_FIREFOX_EXPECTED_URI"
                        ) or urlunsplit(
                            (
                                target_parts.scheme,
                                target_parts.netloc,
                                target_parts.path or "/",
                                target_parts.query,
                                target_parts.fragment,
                            )
                        )
                        # Current input ABI has dedicated Ctrl-A/C/X/V/S
                        # sentinels; select location text through real pointer
                        # focus until generic modifier propagation lands.
                        click_pointer(stream, 400, 164)
                        selection_started = time.monotonic()
                        send_key(stream, "ctrl-a")
                        wait_for_new_output(
                            selector,
                            process,
                            output,
                            b"MAKOS_WIDGET_KEY raw=132",
                            180,
                        )
                        selection_latency_ms = int(
                            (time.monotonic() - selection_started) * 1000
                        )
                        selection_latency_limit_ms = int(
                            os.environ.get(
                                "MAKOS_AARCH64_FIREFOX_SELECTION_LIMIT_MS",
                                "10000",
                            )
                        )
                        if selection_latency_ms >= selection_latency_limit_ms:
                            if serial_log:
                                pathlib.Path(serial_log).write_bytes(output)
                            raise AssertionError(
                                "Firefox Ctrl-A QMP-to-widget latency exceeded bound "
                                f"observed_ms={selection_latency_ms} "
                                f"limit_ms={selection_latency_limit_ms}"
                            )
                        print(
                            "MAKOS_FIREFOX_SELECTION_LATENCY_OK raw=132 "
                            f"enqueue_to_widget_ms={selection_latency_ms} "
                            f"limit_ms={selection_latency_limit_ms}"
                        )
                        navigation_output_start = len(output)
                        # Prove focus accepted the first character before
                        # sending the rest. This distinguishes a real Urlbar
                        # input path from queued keystrokes lost during focus.
                        first_character_started = time.monotonic()
                        send_key(stream, target[0])
                        wait_for_new_output(
                            selector,
                            process,
                            output,
                            f"MAKOS_WIDGET_KEY raw={ord(target[0])}".encode(),
                            180,
                        )
                        first_character_latency_ms = int(
                            (time.monotonic() - first_character_started) * 1000
                        )
                        input_latency_limit_ms = int(
                            os.environ.get(
                                "MAKOS_AARCH64_FIREFOX_INPUT_LIMIT_MS", "500"
                            )
                        )
                        if first_character_latency_ms >= input_latency_limit_ms:
                            raise AssertionError(
                                "Firefox QMP-to-widget input latency exceeded bound "
                                f"observed_ms={first_character_latency_ms} "
                                f"limit_ms={input_latency_limit_ms}"
                            )
                        print(
                            "MAKOS_FIREFOX_INPUT_LATENCY_OK "
                            f"raw={ord(target[0])} enqueue_to_widget_ms="
                            f"{first_character_latency_ms} "
                            f"limit_ms={input_latency_limit_ms}"
                        )
                        send_command(stream, target[1:])
                        expected_host = target_parts.hostname
                        page_pattern = re.compile(
                            rb"^MAKOS_FIREFOX_PAGE_STOP status=0x00000000 "
                            rb"http_status=([0-9]{3}) uri=(.*) end=1$"
                        )
                        tls_pattern = re.compile(
                            rb"^MAKOS_FIREFOX_TLS_OK host="
                            + re.escape(expected_host.encode("utf-8"))
                            + rb" builtin_root=1 ocsp_requests=[01]$"
                        )
                        navigation_deadline = time.monotonic() + int(
                            os.environ.get(
                                "MAKOS_AARCH64_FIREFOX_NAVIGATION_SECONDS", "90"
                            )
                        )
                        page_loaded = False
                        tls_verified = False
                        page_changed = False
                        page_visual_error = "not checked"
                        next_page_visual_check = 0.0
                        while (
                            time.monotonic() < navigation_deadline
                            and process.poll() is None
                        ):
                            for key, _ in selector.select(timeout=0.25):
                                chunk = os.read(key.fileobj.fileno(), 4096)
                                if chunk:
                                    output.extend(chunk)
                            if b"MAKOS_FATAL:" in output[firefox_output_start:]:
                                raise AssertionError(
                                    "Firefox navigation reached kernel fatal:\n"
                                    + output[-12000:].decode(errors="replace")
                                )
                            navigation_lines = output[navigation_output_start:].splitlines()
                            for line in navigation_lines:
                                match = page_pattern.fullmatch(line.rstrip(b"\r"))
                                if match and 200 <= int(match.group(1)) <= 399:
                                    page_loaded = (
                                        match.group(2).decode("utf-8", errors="strict")
                                        == expected_uri
                                    )
                                if tls_pattern.fullmatch(line.rstrip(b"\r")):
                                    tls_verified = True
                            now = time.monotonic()
                            if (
                                page_loaded
                                and tls_verified
                                and now >= next_page_visual_check
                            ):
                                next_page_visual_check = now + 0.5
                                response = qmp_command(
                                    stream,
                                    "screendump",
                                    {"filename": str(FIREFOX_PAGE_SCREENSHOT)},
                                )
                                if "error" in response:
                                    raise AssertionError(
                                        f"Firefox page screendump failed: {response}"
                                    )
                                try:
                                    verify_firefox_page_changed(
                                        FIREFOX_CHROME_SCREENSHOT,
                                        FIREFOX_PAGE_SCREENSHOT,
                                    )
                                except AssertionError as error:
                                    page_visual_error = str(error)
                                else:
                                    page_changed = True
                            if page_loaded and tls_verified and page_changed:
                                break
                        if page_loaded and tls_verified and page_changed:
                            observed_key_sequence = []
                            for line in output[navigation_output_start:].splitlines():
                                key_match = re.fullmatch(
                                    rb"MAKOS_WIDGET_KEY raw=([0-9]+)",
                                    line.rstrip(b"\r"),
                                )
                                if key_match:
                                    observed_key_sequence.append(int(key_match.group(1)))
                            if observed_key_sequence != expected_key_sequence:
                                raise AssertionError(
                                    "Firefox widget key sequence mismatch "
                                    f"expected={expected_key_sequence} "
                                    f"observed={observed_key_sequence}"
                                )
                            print(
                                "MAKOS_FIREFOX_TYPED_URL_OK selection=ctrl-a "
                                "raw="
                                + ",".join(str(value) for value in expected_key_sequence)
                            )
                        if (
                            not page_loaded
                            or not page_changed
                            or not tls_verified
                        ):
                            if serial_log:
                                pathlib.Path(serial_log).write_bytes(output)
                            raise AssertionError(
                                "Firefox navigation did not complete with real page pixels "
                                f"loaded={page_loaded} changed={page_changed} "
                                f"tls_verified={tls_verified} "
                                f"visual={page_visual_error}\n"
                                + output[-12000:].decode(errors="replace")
                            )
                        if serial_log:
                            pathlib.Path(serial_log).write_bytes(output)
                        print(
                            "MAKOS_FIREFOX_NAVIGATION_OK "
                            f"target={target} channel_status=success "
                            "tls=builtin-root page_pixels=changed"
                        )
                        if os.environ.get("MAKOS_AARCH64_FIREFOX_CLIPBOARD") == "1":
                            click_pointer(stream, 400, 164)
                            clipboard_output_start = len(output)
                            for key in (
                                "ctrl-a",
                                "ctrl-c",
                                "backspace",
                                "ctrl-v",
                                "ret",
                            ):
                                send_key(stream, key)
                            clipboard_deadline = time.monotonic() + int(
                                os.environ.get(
                                    "MAKOS_AARCH64_FIREFOX_CLIPBOARD_SECONDS", "120"
                                )
                            )
                            clipboard_loaded = False
                            while (
                                time.monotonic() < clipboard_deadline
                                and process.poll() is None
                            ):
                                for key, _ in selector.select(timeout=0.25):
                                    chunk = os.read(key.fileobj.fileno(), 4096)
                                    if chunk:
                                        output.extend(chunk)
                                if b"MAKOS_FATAL:" in output[firefox_output_start:]:
                                    raise AssertionError(
                                        "Firefox clipboard reload reached kernel fatal:\n"
                                        + output[-12000:].decode(errors="replace")
                                    )
                                for line in output[clipboard_output_start:].splitlines():
                                    match = page_pattern.fullmatch(line.rstrip(b"\r"))
                                    if match and 200 <= int(match.group(1)) <= 399:
                                        clipboard_loaded = (
                                            match.group(2).decode(
                                                "utf-8", errors="strict"
                                            )
                                            == expected_uri
                                        )
                                if clipboard_loaded:
                                    break
                            observed_clipboard_keys = []
                            for line in output[clipboard_output_start:].splitlines():
                                key_match = re.fullmatch(
                                    rb"MAKOS_WIDGET_KEY raw=([0-9]+)",
                                    line.rstrip(b"\r"),
                                )
                                if key_match:
                                    observed_clipboard_keys.append(
                                        int(key_match.group(1))
                                    )
                            expected_clipboard_keys = [132, 133, 8, 135, 10]
                            if (
                                not clipboard_loaded
                                or observed_clipboard_keys != expected_clipboard_keys
                            ):
                                if serial_log:
                                    pathlib.Path(serial_log).write_bytes(output)
                                raise AssertionError(
                                    "Firefox clipboard URL round trip failed "
                                    f"loaded={clipboard_loaded} "
                                    f"expected_keys={expected_clipboard_keys} "
                                    f"observed_keys={observed_clipboard_keys}\n"
                                    + output[-12000:].decode(errors="replace")
                                )
                            print(
                                "MAKOS_FIREFOX_CLIPBOARD_OK "
                                "selection=ctrl-a copy=ctrl-c clear=backspace "
                                "paste=ctrl-v reload=exact-uri storage=makos"
                            )
                        if os.environ.get("MAKOS_AARCH64_FIREFOX_LINK_CLICK") == "1":
                            link_uri = os.environ.get(
                                "MAKOS_AARCH64_FIREFOX_LINK_URI",
                                "https://www.iana.org/help/example-domains",
                            )
                            link_parts = urlsplit(link_uri)
                            if link_parts.scheme != "https" or not link_parts.hostname:
                                raise AssertionError(
                                    "Firefox link-click proof requires absolute HTTPS URI"
                                )
                            shutil.copyfile(
                                FIREFOX_PAGE_SCREENSHOT, FIREFOX_LINK_SCREENSHOT
                            )
                            link_output_start = len(output)
                            link_started = time.monotonic()
                            click_pointer(
                                stream,
                                int(
                                    os.environ.get(
                                        "MAKOS_AARCH64_FIREFOX_LINK_X", "230"
                                    )
                                ),
                                int(
                                    os.environ.get(
                                        "MAKOS_AARCH64_FIREFOX_LINK_Y", "360"
                                    )
                                ),
                            )
                            link_deadline = time.monotonic() + int(
                                os.environ.get(
                                    "MAKOS_AARCH64_FIREFOX_LINK_SECONDS", "120"
                                )
                            )
                            link_loaded = False
                            link_tls = False
                            link_changed = False
                            link_visual_error = "not checked"
                            link_tls_pattern = re.compile(
                                rb"^MAKOS_FIREFOX_TLS_OK host="
                                + re.escape(link_parts.hostname.encode("utf-8"))
                                + rb" builtin_root=1 ocsp_requests=[01]$"
                            )
                            while (
                                time.monotonic() < link_deadline
                                and process.poll() is None
                            ):
                                for key, _ in selector.select(timeout=0.25):
                                    chunk = os.read(key.fileobj.fileno(), 4096)
                                    if chunk:
                                        output.extend(chunk)
                                if b"MAKOS_FATAL:" in output[firefox_output_start:]:
                                    raise AssertionError(
                                        "Firefox link click reached kernel fatal:\n"
                                        + output[-12000:].decode(errors="replace")
                                    )
                                for line in output[link_output_start:].splitlines():
                                    stripped = line.rstrip(b"\r")
                                    match = page_pattern.fullmatch(stripped)
                                    if match and 200 <= int(match.group(1)) <= 399:
                                        link_loaded = (
                                            match.group(2).decode(
                                                "utf-8", errors="strict"
                                            )
                                            == link_uri
                                        )
                                    if link_tls_pattern.fullmatch(stripped):
                                        link_tls = True
                                if link_loaded and link_tls:
                                    response = qmp_command(
                                        stream,
                                        "screendump",
                                        {"filename": str(FIREFOX_PAGE_SCREENSHOT)},
                                    )
                                    if "error" in response:
                                        raise AssertionError(
                                            f"Firefox link screendump failed: {response}"
                                        )
                                    try:
                                        verify_firefox_page_changed(
                                            FIREFOX_LINK_SCREENSHOT,
                                            FIREFOX_PAGE_SCREENSHOT,
                                        )
                                    except AssertionError as error:
                                        link_visual_error = str(error)
                                    else:
                                        link_changed = True
                                        break
                            pointer_routed = re.search(
                                rb"MAKOS_CURSOR_FOCUS_OK "
                                rb"cursor=virtio-gpu-plane buttons=left "
                                rb"hit_test=1 focused_surface=5 ",
                                output[link_output_start:],
                            ) is not None
                            if not (
                                pointer_routed
                                and link_loaded
                                and link_tls
                                and link_changed
                            ):
                                if serial_log:
                                    pathlib.Path(serial_log).write_bytes(output)
                                raise AssertionError(
                                    "Firefox real-link mouse navigation failed "
                                    f"pointer={pointer_routed} loaded={link_loaded} "
                                    f"tls={link_tls} changed={link_changed} "
                                    f"visual={link_visual_error}\n"
                                    + output[-12000:].decode(errors="replace")
                                )
                            print(
                                "MAKOS_FIREFOX_MOUSE_LINK_OK "
                                f"source={expected_uri} target={link_uri} "
                                "button=left surface=firefox tls=builtin-root "
                                f"page_pixels=changed elapsed_ms="
                                f"{int((time.monotonic() - link_started) * 1000)}"
                            )
                        if (
                            os.environ.get(
                                "MAKOS_AARCH64_FIREFOX_SUSTAINED_INTERACTION"
                            )
                            == "1"
                        ):
                            if os.environ.get("MAKOS_AARCH64_FIREFOX_LINK_CLICK") != "1":
                                raise AssertionError(
                                    "Firefox sustained interaction requires link-click phase"
                                )
                            shutil.copyfile(
                                FIREFOX_PAGE_SCREENSHOT,
                                FIREFOX_SCROLL_BASE_SCREENSHOT,
                            )
                            scroll_output_start = len(output)
                            send_pointer(stream, 400, 430)
                            send_wheel(stream, "wheel-down")
                            wait_for_new_output(
                                selector,
                                process,
                                output,
                                b"kind=5",
                                15,
                            )
                            time.sleep(2.0)
                            response = qmp_command(
                                stream,
                                "screendump",
                                {"filename": str(FIREFOX_SCROLL_SCREENSHOT)},
                            )
                            if "error" in response:
                                raise AssertionError(
                                    f"Firefox wheel screendump failed: {response}"
                                )
                            verify_firefox_page_changed(
                                FIREFOX_SCROLL_BASE_SCREENSHOT,
                                FIREFOX_SCROLL_SCREENSHOT,
                            )
                            send_wheel(stream, "wheel-up")
                            wait_for_new_output(
                                selector,
                                process,
                                output,
                                b"kind=5",
                                15,
                            )
                            time.sleep(2.0)
                            response = qmp_command(
                                stream,
                                "screendump",
                                {"filename": str(FIREFOX_SCROLL_RESTORED_SCREENSHOT)},
                            )
                            if "error" in response:
                                raise AssertionError(
                                    "Firefox reverse-wheel screendump failed: "
                                    f"{response}"
                                )
                            scrolled_pixels, restored_pixels = verify_firefox_page_restored(
                                FIREFOX_SCROLL_BASE_SCREENSHOT,
                                FIREFOX_SCROLL_SCREENSHOT,
                                FIREFOX_SCROLL_RESTORED_SCREENSHOT,
                            )
                            scroll_dispatches = len(
                                re.findall(
                                    rb"MAKOS_WIDGET_EVENT_DISPATCH .* kind=5 ",
                                    output[scroll_output_start:],
                                )
                            )
                            if scroll_dispatches != 2:
                                raise AssertionError(
                                    "Firefox wheel dispatch count invalid "
                                    f"count={scroll_dispatches}"
                                )
                            print(
                                "MAKOS_FIREFOX_WHEEL_OK direction=down,up "
                                "surface=firefox dispatches=2 "
                                f"scrolled_pixels={scrolled_pixels} "
                                f"restored_pixels={restored_pixels}"
                            )
                        if (
                            os.environ.get(
                                "MAKOS_AARCH64_FIREFOX_DOCUMENT_SELECTION"
                            )
                            == "1"
                        ):
                            if (
                                os.environ.get("MAKOS_AARCH64_FIREFOX_LINK_CLICK")
                                != "1"
                            ):
                                raise AssertionError(
                                    "Firefox document selection requires link-click phase"
                                )
                            selection_uri = os.environ.get(
                                "MAKOS_AARCH64_FIREFOX_SELECTION_URI",
                                "https://example.com/",
                            )
                            selection_parts = urlsplit(selection_uri)
                            if (
                                selection_parts.scheme != "https"
                                or not selection_parts.hostname
                            ):
                                raise AssertionError(
                                    "Firefox selection proof requires absolute HTTPS URI"
                                )
                            selection_started = time.monotonic()

                            # The mandatory clipboard phase leaves the exact
                            # page URI in the clipboard. That value cannot
                            # satisfy the prefixed document-selection target,
                            # so it is a deterministic poison without touching
                            # location-bar focus again.

                            focus_up_pattern = re.compile(
                                rb"MAKOS_WIDGET_EVENT_DISPATCH .* "
                                rb"kind=2 key=0 "
                            )
                            focus_up_count = len(focus_up_pattern.findall(output))
                            click_pointer(stream, 400, 360)
                            focus_deadline = time.monotonic() + 15
                            while (
                                len(focus_up_pattern.findall(output))
                                == focus_up_count
                                and time.monotonic() < focus_deadline
                                and process.poll() is None
                            ):
                                for key, _ in selector.select(timeout=0.1):
                                    chunk = os.read(key.fileobj.fileno(), 4096)
                                    if chunk:
                                        output.extend(chunk)
                            if (
                                len(focus_up_pattern.findall(output))
                                == focus_up_count
                            ):
                                raise AssertionError(
                                    "Firefox document focus click not dispatched"
                                )
                            time.sleep(0.5)
                            send_pointer(stream, 700, 430)
                            time.sleep(1.0)
                            response = qmp_command(
                                stream,
                                "screendump",
                                {
                                    "filename": str(
                                        FIREFOX_SELECTION_BASE_SCREENSHOT
                                    )
                                },
                            )
                            if "error" in response:
                                raise AssertionError(
                                    f"Firefox selection baseline failed: {response}"
                                )

                            selection_start_x = int(
                                os.environ.get(
                                    "MAKOS_AARCH64_FIREFOX_SELECTION_START_X", "605"
                                )
                            )
                            selection_end_x = int(
                                os.environ.get(
                                    "MAKOS_AARCH64_FIREFOX_SELECTION_END_X", "703"
                                )
                            )
                            selection_y = int(
                                os.environ.get(
                                    "MAKOS_AARCH64_FIREFOX_SELECTION_Y", "449"
                                )
                            )
                            selection_output_start = len(output)
                            down_pattern = re.compile(
                                rb"MAKOS_WIDGET_EVENT_DISPATCH .* "
                                rb"kind=2 key=1 "
                            )
                            down_count = len(down_pattern.findall(output))
                            send_pointer(stream, selection_start_x, selection_y)
                            send_pointer(
                                stream,
                                selection_start_x,
                                selection_y,
                                True,
                            )
                            pointer_deadline = time.monotonic() + 15
                            while (
                                len(down_pattern.findall(output)) == down_count
                                and time.monotonic() < pointer_deadline
                                and process.poll() is None
                            ):
                                for key, _ in selector.select(timeout=0.1):
                                    chunk = os.read(key.fileobj.fileno(), 4096)
                                    if chunk:
                                        output.extend(chunk)
                            if len(down_pattern.findall(output)) == down_count:
                                raise AssertionError(
                                    "Firefox document selection pointer-down not dispatched"
                                )
                            time.sleep(0.5)
                            distance = selection_end_x - selection_start_x
                            for numerator in (1, 2, 3):
                                held_count = len(down_pattern.findall(output))
                                send_pointer(
                                    stream,
                                    selection_start_x + distance * numerator // 3,
                                    selection_y,
                                    True,
                                )
                                pointer_deadline = time.monotonic() + 15
                                while (
                                    len(down_pattern.findall(output)) == held_count
                                    and time.monotonic() < pointer_deadline
                                    and process.poll() is None
                                ):
                                    for key, _ in selector.select(timeout=0.1):
                                        chunk = os.read(key.fileobj.fileno(), 4096)
                                        if chunk:
                                            output.extend(chunk)
                                if len(down_pattern.findall(output)) == held_count:
                                    raise AssertionError(
                                        "Firefox document selection held-button move "
                                        f"{numerator} not dispatched"
                                    )
                            up_pattern = re.compile(
                                rb"MAKOS_WIDGET_EVENT_DISPATCH .* "
                                rb"kind=2 key=0 "
                            )
                            up_count = len(up_pattern.findall(output))
                            send_pointer(
                                stream,
                                selection_end_x,
                                selection_y,
                                False,
                            )
                            pointer_deadline = time.monotonic() + 15
                            while (
                                len(up_pattern.findall(output)) == up_count
                                and time.monotonic() < pointer_deadline
                                and process.poll() is None
                            ):
                                for key, _ in selector.select(timeout=0.1):
                                    chunk = os.read(key.fileobj.fileno(), 4096)
                                    if chunk:
                                        output.extend(chunk)
                            pointer_keys = [
                                int(match.group(1))
                                for match in re.finditer(
                                    rb"MAKOS_WIDGET_EVENT_DISPATCH .* "
                                    rb"kind=2 key=([01]) ",
                                    output[selection_output_start:],
                                )
                            ]
                            if (
                                pointer_keys.count(1) < 4
                                or 0 not in pointer_keys[pointer_keys.index(1) + 1 :]
                            ):
                                raise AssertionError(
                                    "Firefox document selection pointer sequence invalid "
                                    f"keys={pointer_keys}"
                                )
                            time.sleep(0.5)
                            response = qmp_command(
                                stream,
                                "screendump",
                                {"filename": str(FIREFOX_SELECTION_SCREENSHOT)},
                            )
                            if "error" in response:
                                raise AssertionError(
                                    f"Firefox selection screendump failed: {response}"
                                )
                            try:
                                verify_firefox_selection_changed(
                                    FIREFOX_SELECTION_BASE_SCREENSHOT,
                                    FIREFOX_SELECTION_SCREENSHOT,
                                    (
                                        min(selection_start_x, selection_end_x) - 8,
                                        selection_y - 20,
                                        max(selection_start_x, selection_end_x) + 8,
                                        selection_y + 10,
                                    ),
                                )
                            except AssertionError:
                                if serial_log:
                                    pathlib.Path(serial_log).write_bytes(output)
                                raise

                            copy_count = output.count(b"MAKOS_WIDGET_KEY raw=133")
                            send_key(stream, "ctrl-c")
                            copy_deadline = time.monotonic() + 15
                            while (
                                output.count(b"MAKOS_WIDGET_KEY raw=133")
                                == copy_count
                                and time.monotonic() < copy_deadline
                                and process.poll() is None
                            ):
                                for key, _ in selector.select(timeout=0.1):
                                    chunk = os.read(key.fileobj.fileno(), 4096)
                                    if chunk:
                                        output.extend(chunk)
                            if output.count(b"MAKOS_WIDGET_KEY raw=133") == copy_count:
                                raise AssertionError(
                                    "Firefox document selection copy not dispatched"
                                )
                            time.sleep(0.5)
                            selection_navigation_start = len(output)
                            send_key(stream, "ctrl-l")
                            wait_for_new_output(
                                selector,
                                process,
                                output,
                                b"MAKOS_WIDGET_KEY raw=136",
                                15,
                            )
                            url_selection_deadline = time.monotonic() + 60
                            url_selection_error = "not checked"
                            while time.monotonic() < url_selection_deadline:
                                for ready, _ in selector.select(timeout=0.25):
                                    chunk = os.read(ready.fileobj.fileno(), 4096)
                                    if chunk:
                                        output.extend(chunk)
                                response = qmp_command(
                                    stream,
                                    "screendump",
                                    {"filename": str(FIREFOX_URL_SELECTION_SCREENSHOT)},
                                )
                                if "error" in response:
                                    raise AssertionError(
                                        "Firefox URL selection screendump failed: "
                                        f"{response}"
                                    )
                                try:
                                    verify_firefox_url_selected(
                                        FIREFOX_URL_SELECTION_SCREENSHOT
                                    )
                                except AssertionError as error:
                                    url_selection_error = str(error)
                                else:
                                    break
                            else:
                                raise AssertionError(url_selection_error)
                            send_key(stream, "h")
                            wait_for_new_output(
                                selector,
                                process,
                                output,
                                b"MAKOS_WIDGET_KEY raw=104",
                                15,
                            )
                            for key in (
                                "t", "t", "p", "s", "shift-semicolon",
                                "slash", "slash",
                            ):
                                send_key(stream, key)
                            send_key(stream, "ctrl-v")
                            wait_for_new_output(
                                selector,
                                process,
                                output,
                                b"MAKOS_WIDGET_KEY raw=135",
                                15,
                            )
                            time.sleep(8.0)
                            response = qmp_command(
                                stream,
                                "screendump",
                                {"filename": str(FIREFOX_SELECTION_ENTRY_SCREENSHOT)},
                            )
                            if "error" in response:
                                raise AssertionError(
                                    "Firefox selection entry screendump failed: "
                                    f"{response}"
                                )
                            send_key(stream, "ret")
                            selection_deadline = time.monotonic() + int(
                                os.environ.get(
                                    "MAKOS_AARCH64_FIREFOX_SELECTION_SECONDS",
                                    "120",
                                )
                            )
                            selection_loaded = False
                            selection_changed = False
                            selection_visual_error = "not checked"
                            while (
                                time.monotonic() < selection_deadline
                                and process.poll() is None
                            ):
                                for key, _ in selector.select(timeout=0.25):
                                    chunk = os.read(key.fileobj.fileno(), 4096)
                                    if chunk:
                                        output.extend(chunk)
                                for line in output[
                                    selection_navigation_start:
                                ].splitlines():
                                    match = page_pattern.fullmatch(line.rstrip(b"\r"))
                                    if match and 200 <= int(match.group(1)) <= 399:
                                        selection_loaded = (
                                            match.group(2).decode(
                                                "utf-8", errors="strict"
                                            )
                                            == selection_uri
                                        )
                                if selection_loaded:
                                    response = qmp_command(
                                        stream,
                                        "screendump",
                                        {"filename": str(FIREFOX_PAGE_SCREENSHOT)},
                                    )
                                    if "error" in response:
                                        raise AssertionError(
                                            f"Firefox selection reload screendump failed: {response}"
                                        )
                                    try:
                                        verify_firefox_page_changed(
                                            FIREFOX_SELECTION_BASE_SCREENSHOT,
                                            FIREFOX_PAGE_SCREENSHOT,
                                        )
                                    except AssertionError as error:
                                        selection_visual_error = str(error)
                                    else:
                                        selection_changed = True
                                        break
                            observed_selection_keys = []
                            for line in output[
                                selection_navigation_start:
                            ].splitlines():
                                key_match = re.fullmatch(
                                    rb"MAKOS_WIDGET_KEY raw=([0-9]+)",
                                    line.rstrip(b"\r"),
                                )
                                if key_match:
                                    observed_selection_keys.append(
                                        int(key_match.group(1))
                                    )
                            expected_selection_keys = [
                                136,
                                104,
                                116,
                                116,
                                112,
                                115,
                                58,
                                47,
                                47,
                                135,
                                10,
                            ]
                            selection_tls = any(
                                re.fullmatch(
                                    rb"MAKOS_FIREFOX_TLS_OK host="
                                    + re.escape(
                                        selection_parts.hostname.encode("utf-8")
                                    )
                                    + rb" builtin_root=1 ocsp_requests=[01]",
                                    line.rstrip(b"\r"),
                                )
                                for line in output[firefox_output_start:].splitlines()
                            )
                            if not (
                                selection_loaded
                                and selection_changed
                                and selection_tls
                                and observed_selection_keys
                                == expected_selection_keys
                            ):
                                if serial_log:
                                    pathlib.Path(serial_log).write_bytes(output)
                                raise AssertionError(
                                    "Firefox document selection copy/paste failed "
                                    f"loaded={selection_loaded} changed={selection_changed} "
                                    f"tls={selection_tls} "
                                    f"expected_keys={expected_selection_keys} "
                                    f"observed_keys={observed_selection_keys} "
                                    f"visual={selection_visual_error}\n"
                                    + output[-12000:].decode(errors="replace")
                                )
                            print(
                                "MAKOS_FIREFOX_DOCUMENT_SELECTION_OK "
                                "drag=pointer-down,moves,up highlight=changed "
                                "copy=document paste=makos exact_uri=1 "
                                "tls=builtin-root elapsed_ms="
                                f"{int((time.monotonic() - selection_started) * 1000)}"
                            )
                    if (
                        os.environ.get(
                            "MAKOS_AARCH64_FIREFOX_SUSTAINED_INTERACTION"
                        )
                        == "1"
                    ):
                        if (
                            os.environ.get(
                                "MAKOS_AARCH64_FIREFOX_DOCUMENT_SELECTION"
                            )
                            != "1"
                        ):
                            raise AssertionError(
                                "Firefox sustained interaction requires document selection"
                            )

                        def navigate_sustained(uri: str) -> None:
                            parts = urlsplit(uri)
                            if parts.scheme != "https" or not parts.hostname:
                                raise AssertionError(
                                    "Firefox sustained navigation requires HTTPS URI"
                                )
                            phase_start = len(output)
                            send_key(stream, "ctrl-l")
                            time.sleep(0.5)
                            send_text(stream, uri)
                            send_key(stream, "ret")
                            deadline = time.monotonic() + int(
                                os.environ.get(
                                    "MAKOS_AARCH64_FIREFOX_SUSTAINED_NAVIGATION_SECONDS",
                                    "120",
                                )
                            )
                            loaded = False
                            tls = False
                            tls_pattern = re.compile(
                                rb"^MAKOS_FIREFOX_TLS_OK host="
                                + re.escape(parts.hostname.encode("utf-8"))
                                + rb" builtin_root=1 ocsp_requests=[01]$"
                            )
                            tls = any(
                                tls_pattern.fullmatch(line.rstrip(b"\r"))
                                for line in output[firefox_output_start:].splitlines()
                            )
                            while deadline > time.monotonic() and process.poll() is None:
                                for ready, _ in selector.select(timeout=0.25):
                                    chunk = os.read(ready.fileobj.fileno(), 4096)
                                    if chunk:
                                        output.extend(chunk)
                                for line in output[phase_start:].splitlines():
                                    stripped = line.rstrip(b"\r")
                                    match = page_pattern.fullmatch(stripped)
                                    if match and 200 <= int(match.group(1)) <= 399:
                                        loaded = (
                                            match.group(2).decode(
                                                "utf-8", errors="strict"
                                            )
                                            == uri
                                        )
                                    if tls_pattern.fullmatch(stripped):
                                        tls = True
                                if loaded and tls:
                                    return
                            raise AssertionError(
                                "Firefox sustained navigation failed "
                                f"uri={uri} loaded={loaded} tls={tls}"
                            )

                        sustained_started = time.monotonic()
                        sustained_cpu_before = process_cpu_seconds(process.pid)
                        sustained_rss_samples = [process_resident_bytes(process.pid)]
                        form_uri = os.environ.get(
                            "MAKOS_AARCH64_FIREFOX_FORM_URI",
                            "https://httpbin.org/forms/post",
                        )
                        navigate_sustained(form_uri)
                        response = qmp_command(
                            stream,
                            "screendump",
                            {"filename": str(FIREFOX_FORM_BASE_SCREENSHOT)},
                        )
                        if "error" in response:
                            raise AssertionError(
                                f"Firefox form baseline screendump failed: {response}"
                            )
                        form_x = int(
                            os.environ.get("MAKOS_AARCH64_FIREFOX_FORM_X", "235")
                        )
                        form_y = int(
                            os.environ.get("MAKOS_AARCH64_FIREFOX_FORM_Y", "205")
                        )
                        form_token = os.environ.get(
                            "MAKOS_AARCH64_FIREFOX_FORM_TOKEN", "makos42"
                        )
                        if not re.fullmatch(r"[a-z0-9]{4,24}", form_token):
                            raise AssertionError(
                                "Firefox form token must be 4..24 lowercase ASCII chars"
                            )
                        click_pointer(stream, form_x, form_y)
                        send_text(stream, form_token)
                        time.sleep(3.0)
                        response = qmp_command(
                            stream,
                            "screendump",
                            {"filename": str(FIREFOX_FORM_SCREENSHOT)},
                        )
                        if "error" in response:
                            raise AssertionError(
                                f"Firefox form-entry screendump failed: {response}"
                            )
                        verify_firefox_selection_changed(
                            FIREFOX_FORM_BASE_SCREENSHOT,
                            FIREFOX_FORM_SCREENSHOT,
                            (
                                max(0, form_x - 15),
                                max(0, form_y - 18),
                                min(800, form_x + 220),
                                min(600, form_y + 18),
                            ),
                        )
                        # Ctrl-A is intentionally a global location shortcut on
                        # MakOS. Select field text with a reverse pointer drag.
                        send_pointer(stream, form_x + 25, form_y)
                        send_pointer(stream, form_x + 25, form_y, True)
                        send_pointer(stream, form_x - 50, form_y, True)
                        send_pointer(stream, form_x - 50, form_y, False)
                        time.sleep(0.5)
                        send_key(stream, "ctrl-c")
                        time.sleep(0.5)
                        query_prefix = os.environ.get(
                            "MAKOS_AARCH64_FIREFOX_FORM_QUERY_PREFIX",
                            "https://example.com/?customer=",
                        )
                        query_uri = query_prefix + form_token
                        query_start = len(output)
                        send_key(stream, "ctrl-l")
                        time.sleep(0.5)
                        send_text(stream, query_prefix)
                        send_key(stream, "ctrl-v")
                        send_key(stream, "ret")
                        query_parts = urlsplit(query_uri)
                        query_deadline = time.monotonic() + int(
                            os.environ.get(
                                "MAKOS_AARCH64_FIREFOX_SUSTAINED_NAVIGATION_SECONDS",
                                "120",
                            )
                        )
                        query_loaded = False
                        query_tls_pattern = re.compile(
                            rb"MAKOS_FIREFOX_TLS_OK host="
                            + re.escape(query_parts.hostname.encode("utf-8"))
                            + rb" builtin_root=1 ocsp_requests=[01]"
                        )
                        query_tls = any(
                            query_tls_pattern.fullmatch(line.rstrip(b"\r"))
                            for line in output[firefox_output_start:].splitlines()
                        )
                        while query_deadline > time.monotonic() and process.poll() is None:
                            for ready, _ in selector.select(timeout=0.25):
                                chunk = os.read(ready.fileobj.fileno(), 4096)
                                if chunk:
                                    output.extend(chunk)
                            for line in output[query_start:].splitlines():
                                stripped = line.rstrip(b"\r")
                                match = page_pattern.fullmatch(stripped)
                                if match and 200 <= int(match.group(1)) <= 399:
                                    query_loaded = (
                                        match.group(2).decode("utf-8", errors="strict")
                                        == query_uri
                                    )
                                if query_tls_pattern.fullmatch(stripped):
                                    query_tls = True
                            if query_loaded and query_tls:
                                break
                        if not query_loaded or not query_tls:
                            if serial_log:
                                pathlib.Path(serial_log).write_bytes(output)
                            observed_pages = [
                                line.rstrip(b"\r").decode("utf-8", errors="replace")
                                for line in output[query_start:].splitlines()
                                if line.startswith(b"MAKOS_FIREFOX_PAGE_STOP ")
                            ]
                            raise AssertionError(
                                "Firefox form clipboard query failed "
                                f"loaded={query_loaded} tls={query_tls} uri={query_uri} "
                                f"pages={observed_pages[-4:]}"
                            )
                        response = qmp_command(
                            stream,
                            "screendump",
                            {"filename": str(FIREFOX_PAGE_SCREENSHOT)},
                        )
                        if "error" in response:
                            raise AssertionError(
                                f"Firefox form-query screendump failed: {response}"
                            )
                        sustained_rss_samples.append(
                            process_resident_bytes(process.pid)
                        )

                        repeated_uris = (
                            "https://www.iana.org/help/example-domains",
                            "https://example.com/",
                        )
                        cycles = int(
                            os.environ.get(
                                "MAKOS_AARCH64_FIREFOX_SUSTAINED_CYCLES", "2"
                            )
                        )
                        if not 1 <= cycles <= 4:
                            raise AssertionError(
                                "Firefox sustained cycles must be between 1 and 4"
                            )
                        repeated_pages = 0
                        for _ in range(cycles):
                            for uri in repeated_uris:
                                shutil.copyfile(
                                    FIREFOX_PAGE_SCREENSHOT,
                                    FIREFOX_FORM_BASE_SCREENSHOT,
                                )
                                navigate_sustained(uri)
                                visual_deadline = time.monotonic() + int(
                                    os.environ.get(
                                        "MAKOS_AARCH64_FIREFOX_SUSTAINED_PAINT_SECONDS",
                                        "120",
                                    )
                                )
                                visual_error = None
                                while process.poll() is None:
                                    response = qmp_command(
                                        stream,
                                        "screendump",
                                        {"filename": str(FIREFOX_PAGE_SCREENSHOT)},
                                    )
                                    if "error" in response:
                                        raise AssertionError(
                                            "Firefox repeated-navigation screendump failed: "
                                            f"{response}"
                                        )
                                    try:
                                        verify_firefox_page_changed(
                                            FIREFOX_FORM_BASE_SCREENSHOT,
                                            FIREFOX_PAGE_SCREENSHOT,
                                        )
                                        break
                                    except AssertionError as error:
                                        visual_error = error
                                    if time.monotonic() >= visual_deadline:
                                        raise AssertionError(
                                            "Firefox repeated-navigation paint timed out "
                                            f"uri={uri}: {visual_error}"
                                        )
                                    for ready, _ in selector.select(timeout=0.5):
                                        chunk = os.read(ready.fileobj.fileno(), 4096)
                                        if chunk:
                                            output.extend(chunk)
                                else:
                                    raise AssertionError(
                                        "Firefox exited before repeated-navigation paint"
                                    )
                                repeated_pages += 1
                                sustained_rss_samples.append(
                                    process_resident_bytes(process.pid)
                                )

                        click_pointer(stream, 250, 580)
                        wait_for_new_output(
                            selector,
                            process,
                            output,
                            b"MAKOS_TASKBAR_APP_OK surface=2 activate=1 minimize_toggle=1",
                            15,
                        )
                        click_pointer(stream, 100, 160)
                        task_report_start = len(output)
                        send_command(stream, "ps")
                        wait_for_new_output(
                            selector,
                            process,
                            output,
                            b"MAKOS_AARCH64_SHELL_CMD ps",
                            30,
                        )
                        firefox_resource = re.search(
                            rb"MAKOS_TASK tid=([0-9]+) pid=([0-9]+) role=firefox "
                            rb".* resident_pages=([0-9]+) resident_kib=([0-9]+)",
                            output[task_report_start:],
                        )
                        if not firefox_resource:
                            raise AssertionError(
                                "Firefox guest resident-page report absent"
                            )
                        resident_pages = int(firefox_resource.group(3))
                        resident_limit = int(
                            os.environ.get(
                                "MAKOS_AARCH64_FIREFOX_RESIDENT_PAGE_LIMIT",
                                "196608",
                            )
                        )
                        if resident_pages > resident_limit:
                            raise AssertionError(
                                "Firefox guest resident pages unbounded "
                                f"pages={resident_pages} limit={resident_limit}"
                            )
                        click_pointer(stream, 250, 580)
                        wait_for_new_output(
                            selector,
                            process,
                            output,
                            b"MAKOS_TASKBAR_APP_OK surface=2 activate=1 minimize_toggle=1",
                            15,
                        )

                        sustained_elapsed = time.monotonic() - sustained_started
                        sustained_cpu = (
                            process_cpu_seconds(process.pid) - sustained_cpu_before
                        )
                        cpu_ratio = sustained_cpu / sustained_elapsed
                        cpu_limit = float(
                            os.environ.get(
                                "MAKOS_AARCH64_FIREFOX_SUSTAINED_CPU_RATIO", "2.5"
                            )
                        )
                        max_rss = max(sustained_rss_samples)
                        rss_limit = int(
                            os.environ.get(
                                "MAKOS_AARCH64_FIREFOX_HOST_RSS_LIMIT_BYTES",
                                str(1536 * 1024 * 1024),
                            )
                        )
                        if accel == "hvf" and cpu_ratio > cpu_limit:
                            raise AssertionError(
                                "Firefox sustained host CPU unbounded "
                                f"ratio={cpu_ratio:.3f} limit={cpu_limit:.3f}"
                            )
                        if max_rss > rss_limit:
                            raise AssertionError(
                                "Firefox sustained host RSS unbounded "
                                f"bytes={max_rss} limit={rss_limit}"
                            )
                        print(
                            "MAKOS_FIREFOX_SUSTAINED_INTERACTION_OK "
                            "wheel=down,up,changed,restored "
                            f"form=typed,copy,exact-query token_bytes={len(form_token)} "
                            f"repeated_top_level_pages={repeated_pages} "
                            f"host_cpu_ratio={cpu_ratio:.3f} host_rss_bytes={max_rss} "
                            f"guest_firefox_resident_pages={resident_pages}"
                        )
                    firefox_process_match = re.search(
                        rb"MAKOS_FIREFOX_PROCESS_OK pid=([0-9]+)",
                        output[firefox_output_start:],
                    )
                    if not firefox_process_match:
                        raise AssertionError("Firefox PID marker missing")
                    firefox_pid = firefox_process_match.group(1)
                    firefox_output = output[firefox_output_start:]
                    if os.environ.get("MAKOS_AARCH64_FIREFOX_SMP_REQUIRED") == "1":
                        overlap_matches = re.findall(
                            rb"MAKOS_AARCH64_FIREFOX_SMP_OVERLAP_OK "
                            rb"group_pid=([0-9]+) cpu_mask=(0x[0-9a-f]+) "
                            rb"tids=([0-9]+),([0-9]+),([0-9]+)",
                            firefox_output,
                        )
                        valid_overlap = None
                        for group_pid, mask_text, tid1, tid2, tid3 in overlap_matches:
                            cpu_mask = int(mask_text, 16) & 0xE
                            tids = (int(tid1), int(tid2), int(tid3))
                            active_tids = [
                                tids[cpu - 1]
                                for cpu in range(1, 4)
                                if cpu_mask & (1 << cpu)
                            ]
                            if (
                                group_pid == firefox_pid
                                and cpu_mask.bit_count() >= 2
                                and all(tid != 0 for tid in active_tids)
                                and len(active_tids) == len(set(active_tids))
                            ):
                                valid_overlap = (cpu_mask, active_tids)
                                break
                        if valid_overlap is None:
                            raise AssertionError(
                                "Firefox did not overlap distinct worker TIDs on multiple guest CPUs"
                            )
                        print(
                            "MAKOS_FIREFOX_SMP_OVERLAP_OK "
                            f"group_pid={firefox_pid.decode()} "
                            f"cpu_mask={valid_overlap[0]:#x} "
                            f"worker_cpus={valid_overlap[0].bit_count()} "
                            "tids=distinct concurrent=1 ownership=exclusive"
                        )
                    if (
                        b"process-exit arch=aarch64 pid=" + firefox_pid + b" "
                        in firefox_output
                        or b"MAKOS_AARCH64_EXIT_GROUP_OK pid=" + firefox_pid + b" "
                        in firefox_output
                    ):
                        raise AssertionError("Firefox exited during guest probe")
                    survived_seconds = int(time.monotonic() - probe_started)
                    print(
                        "MAKOS_FIREFOX_GUEST_PROBE_OK package=mounted exec=1 visible=700x400 "
                        f"pt_interp=1 survived_seconds={survived_seconds}"
                    )
                    return 0
                # Concurrent process startup may leave another app above Files.
                # Activate its existing surface through Start before testing X.
                pointer_started = time.monotonic()
                click_pointer(stream, 50, 580)
                wait_for_new_output(
                    selector, process, output,
                    b"MAKOS_START_MENU_OK launcher=1 open=1 apps=5",
                )
                pointer_latency = time.monotonic() - pointer_started
                pointer_limit = 0.5 if accel == "hvf" else 2.0
                if pointer_latency > pointer_limit:
                    raise AssertionError(
                        f"pointer click latency {pointer_latency:.3f}s exceeds {pointer_limit:.3f}s"
                    )
                print(
                    "MAKOS_AARCH64_POINTER_LATENCY_OK path=qmp-to-guest-click "
                    f"milliseconds={pointer_latency * 1000:.1f} limit={pointer_limit * 1000:.0f}"
                )
                click_pointer(stream, 100, 524)
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_APP_REOPEN_OK app=files source=start-menu surface=6 reopened=0",
                )
                hover_presents = output.count(b"MAKOS_M7_OK graphics_abi=1")
                for x, y in ((180, 220), (360, 300), (560, 410)):
                    send_pointer(stream, x, y)
                drain_output(selector, process, output, 0.30)
                if output.count(b"MAKOS_M7_OK graphics_abi=1") != hover_presents:
                    raise AssertionError("Files accepted presents for hover-only pointer motion")
                # Close it, reopen from Start, then close again so terminal
                # input remains focus for shell tests.
                click_pointer(stream, 725, 102)
                wait_for_output(
                    selector, process, output, b"MAKOS_WINDOW_CLOSE_OK surface=6"
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_AARCH64_FILES_CLOSE_OK background=1 reopen=start-menu state=retained",
                )
                click_pointer(stream, 50, 580)
                wait_for_output(
                    selector, process, output,
                    b"MAKOS_START_MENU_OK launcher=1 open=1 apps=5",
                )
                response = qmp_command(
                    stream,
                    "screendump",
                    {"filename": str(START_MENU_SCREENSHOT)},
                )
                if "error" in response:
                    raise AssertionError(f"Start menu screendump failed: {response}")
                click_pointer(stream, 100, 524)
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_APP_REOPEN_OK app=files source=start-menu surface=6 reopened=1",
                )
                click_pointer(stream, 725, 102)
                wait_for_new_output(
                    selector, process, output, b"MAKOS_WINDOW_CLOSE_OK surface=6"
                )
                # Monitor is kernel-rendered: click Refresh, then prove live PMM
                # and scheduler values replaced the old static colored fixture.
                click_pointer(stream, 165, 580)
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_SYSTEM_MONITOR_LIVE_OK cadence_ms=1000 source=svc-yield damage=content-only focused-only=1",
                )
                click_pointer(stream, 390, 323)
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_SYSTEM_MONITOR_REFRESH_OK source=button uptime_ms=",
                )
                response = qmp_command(
                    stream,
                    "screendump",
                    {"filename": str(SYSTEM_MONITOR_SCREENSHOT)},
                )
                if "error" in response:
                    raise AssertionError(f"Monitor screendump failed: {response}")
                click_pointer(stream, 250, 580)
                for key in (
                    "e", "c", "h", "o", "spc",
                    "l", "o", "w", "e", "r", "shift-minus", "c", "a", "s", "e",
                    "shift-1", "shift-slash", "ret",
                ):
                    send_key(stream, key)
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_AARCH64_SHELL_INPUT_OK exact=lower_case!? lowercase=1 punctuation=1",
                )
                send_key(stream, "up")
                wait_for_output(
                    selector, process, output,
                    b"MAKOS_AARCH64_SHELL_HISTORY_OK direction=up offset=1",
                )
                send_key(stream, "down")
                wait_for_output(
                    selector, process, output,
                    b"MAKOS_AARCH64_SHELL_HISTORY_OK direction=down offset=0",
                )
                for key in ("w", "h", "o", "tab", "ret"):
                    send_key(stream, key)
                wait_for_output(
                    selector, process, output,
                    b"MAKOS_AARCH64_SHELL_EDIT_OK completion=1 history_slots=8",
                )
                wait_for_output(
                    selector, process, output, b"MAKOS_AARCH64_SHELL_CMD whoami"
                )
                for key in (
                    "w", "r", "i", "t", "e", "spc",
                    "t", "e", "s", "t", "dot", "t", "x", "t", "spc",
                    "l", "o", "w", "e", "r", "shift-minus", "c", "a", "s", "e",
                    "shift-1", "shift-slash", "ret",
                ):
                    send_key(stream, key)
                wait_for_output(
                    selector, process, output,
                    b"MAKOS_AARCH64_SHELL_CMD write bytes=12 persisted=1",
                )
                for key in (
                    "c", "a", "t", "spc", "t", "e", "s", "t", "dot", "t", "x", "t", "ret",
                ):
                    send_key(stream, key)
                wait_for_output(
                    selector, process, output, b"MAKOS_AARCH64_SHELL_CMD cat bytes=12"
                )
                for key in (
                    "s", "t", "a", "t", "spc", "t", "e", "s", "t", "dot", "t", "x", "t", "ret",
                ):
                    send_key(stream, key)
                wait_for_output(
                    selector, process, output,
                    b"MAKOS_AARCH64_SHELL_CMD stat size=12",
                )
                send_command(stream, "selfhost-aarch64")
                wait_for_output(
                    selector, process, output,
                    b"MAKOS_AARCH64_LINKER_OK sources=3 languages=aarch64-asm,c-subset-v1 compiler=guest-native assembler=guest-native objects=3 format=elf64-et-rel linker=guest-native relocations=R_AARCH64_CALL26:2 symbols=_start,answer,adjust output=/home/user/generated-aarch64.elf c_source=/home/user/generated-answer.c c_abi=aapcs64-int32-pointer64 c_features=parameter,pointer-parameter,local,array,array-decay,index,assignment,pointer,address-of,address-expression,dereference,if,equality,inequality,while,call,return nonleaf_frame=96 c_operators=mul,sub,add branch_results=42,86 loop_results=42,2 memory_results=42,2 pointer_call=answer-to-adjust pointee_results=42,2 array_results=41:42,1:2 code_bytes=76,128,132 object_bytes=688,736,688 linked_bytes=336 output_bytes=815 persisted_reopened=1 malformed_c_denied=8 malformed_relocation_denied=1 unresolved_symbol_denied=1 duplicate_definition_denied=1",
                )
                wait_for_output(
                    selector, process, output, b"MAKOS_AARCH64_EXEC_SPAWN",
                )
                wait_for_output(
                    selector, process, output,
                    b"MAKOS_AARCH64_SELFHOST_LINK_OK source=guest-makfs sources=3 languages=aarch64-asm,c-subset-v1 compiler=guest-native assembler=guest-native linker=guest-native objects=3 object_format=elf64-et-rel relocations=R_AARCH64_CALL26:2 symbols=_start,answer,adjust c_source=/home/user/generated-answer.c c_abi=aapcs64-int32-pointer64 c_features=parameter,pointer-parameter,local,array,array-decay,index,assignment,pointer,address-of,address-expression,dereference,if,equality,inequality,while,call,return nonleaf_frame=96 c_operators=mul,sub,add branch_results=42,86 loop_results=42,2 memory_results=42,2 pointer_call=answer-to-adjust pointee_results=42,2 array_results=41:42,1:2 code_bytes=76,128,132 object_bytes=688,736,688 linked_bytes=336 output_bytes=815 persisted_reopened=1 malformed_c_denied=8 malformed_relocation_denied=1 unresolved_symbol_denied=1 duplicate_definition_denied=1 output=elf64-aarch64 kernel_loader=validated abi56=1 abi57=1 argv=3 env=1 malformed_startup_denied=3 executed=2 status=42",
                )
                send_command(stream, "abi-startup")
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_AARCH64_SYSV_STARTUP_OK argc=3 argv=1 envp=1 auxv=pagesz,entry,uid,gid,random,execfn stack_align=16 registers=x0,x1,x2",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_CLEAR_CHILD_TID_OK",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_AARCH64_SYSV_REAP_OK status=42 lifecycle=spawn,run,exit,wait,reap",
                )
                send_command(stream, "musl-probe")
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_MUSL_PROCESS_OK",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_MUSL_RUNTIME_OK version=1.2.6 libc=upstream-static syscalls=open,read,close,write,exit custom_entry=1 crt=upstream",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_MUSL_REAP_OK status=42 lifecycle=spawn,run,exit,wait,reap",
                )
                send_command(stream, "musl-crt")
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_MUSL_CRT_PROCESS_OK",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_MUSL_CRT_OK version=1.2.6 crt1=upstream libc_start_main=1 argc=2 envp=1 tls=1 tid=real fd=dup,dup3,fcntl,shared-offset,lseek,cloexec",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_MUSL_CRT_REAP_OK status=42 lifecycle=spawn,run,exit,wait,reap",
                )
                send_command(stream, "stack-protector")
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_STACK_PROTECTOR_PROCESS_OK",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_STACK_PROTECTOR_TRIGGER_OK instrumentation=strong canary=corrupt",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_AARCH64_USER_FAULT_OK",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_STACK_PROTECTOR_REAP_OK failure=contained shell=survived",
                )
                send_command(stream, "musl-pthread")
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_MUSL_PTHREAD_PROCESS_OK",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_AARCH64_THREAD_CREATE_OK",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_MUSL_SCM_RIGHTS_OK socketpair=unix-stream ordering=associated-byte lifetime=queued-open-description payload=read-after-sender-close",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_MUSL_TYPED_IPC_OK service=org.makos.echo client=child typed=1 fifo=1 transfer=channel rights=attenuated spoof_denied=1 stale_denied=1 cleanup=1",
                )
                print(
                    "MAKOS_AARCH64_TYPED_IPC_RUNTIME_OK "
                    "service=same-domain fifo=1 transfer=attenuated "
                    "cleanup=process-exit-before-reap"
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_AARCH64_SLEEP_BLOCK_OK",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_AARCH64_SLEEP_WAKE_OK",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_AARCH64_IO_BLOCK_OK",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_AARCH64_IO_WAKE_OK",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_AARCH64_FUTEX_REQUEUE_OK libc=pthread_cond_broadcast waiters=3 wake=relay requeue=mutex fifo=1 joins=bounded",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_AARCH64_EPOLL_OK instances=4 watches=16 level=1 edge=1 oneshot=1 wait=scheduler-blocked targets=pipe,socket",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_AARCH64_TCP_ASYNC_OK source=timer-rx buffer=32768 wake=poll,epoll bounded_frames=16",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_MAKFS_DIRECTORY_PERSIST_OK phase=create format=makfs4 path=/home/user/persist/sub/value.txt scalable=64-siblings,name255,indexed-lookup,cursor",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_MAKFS_DIRECTORY_SCALE_OK siblings=64 name_bytes=255 lookup=hash-index cursor=resumable remount=verified",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_MUSL_PTHREAD_OK version=1.2.6 clone=shared-vm tls=distinct futex=wait,wake,requeue,timed-timeout join=1 clear_child_tid=1 robust=owner-death,wake-one ipc=channel,event blocking=1 cleanup=handles getrandom=virtio-rng bytes=64 zeroized=1 clock_realtime=pl031 tls_validation=ready sleep=nanosleep,clock_nanosleep,blocked,timer-wake resolution=10ms process_identity=pid,uid,gid session_bound=1 parent=thread-consistent pipe=blocking,nonblock,cloexec,bounded,atomic poll=timed,block,wake,timeout,write,read,eagain,eof,hup signals=task-mask,inherit,pselect-atomic,ppoll-atomic,epoll-pwait-atomic,eintr,restore,kill,tkill,tgkill select=fdsets,timeout,pipe-wake,ebadf epoll=create,ctl,level,edge,oneshot,pipe,udp,tcp-async,timed resolver=getaddrinfo,addrconfig,ipv4,ipv6,aaaa,udp4 udp6=tx,rx-backend-dependent metadata=stat,fstat,regular,fifo,tty,timestamps-unix symlink=create,readlink,lstat,follow,readdir,unlink directory=opendir,getdents64,readdir,rewind,dot,dotdot cwd=getcwd,chdir,relative,dotdot,create,rename,replace,unlink file_access=rdwr,preserve record_lock=setlkw,getlk-own,unlock profile_storage=makfs4,8k,long-name,readdir,reopen file_size=ftruncate,grow-zero,shrink,offset-unchanged durability=virtio-flush,fsync positional=pread,pwrite,offset-preserved,sparse-zero madvise=dontneed,free,decommit,zero-refault shmem=posix,named,excl,truncate,unlink-lifetime,shared-coherent,readonly,reclaim directories=mkdir,nested,stat,readdir,rmdir,notempty,busy exit_group=all-threads",
                )
                if os.environ.get("MAKOS_AARCH64_IPV6_PROBE") == "1":
                    for ipv6_marker in (
                        b"MAKOS_AARCH64_AF_INET6_SOCKET_OK sockaddr_in6=28 v6only=1 fake_mapping=0",
                        b"MAKOS_AARCH64_UDP6_SEND_OK checksum=pseudoheader ndp=resolved fake_mapping=0",
                    ):
                        wait_for_output(
                            selector,
                            process,
                            output,
                            ipv6_marker,
                            30,
                        )
                    print(
                        "MAKOS_AARCH64_IPV6_RUNTIME_OK "
                        "slaac=ra,eui64 socket=af_inet6,sockaddr28 "
                        "ndp=resolved udp6=checksum,tx fake_mapping=0"
                    )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_AARCH64_THREAD_EXIT_OK",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_AARCH64_EXIT_GROUP_OK",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_MUSL_PTHREAD_REAP_OK status=42 lifecycle=spawn,threads,join,exit,wait,reap",
                )
                send_command(stream, "musl-dynamic")
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_MUSL_INTERP_PROCESS_OK",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_MUSL_INTERP_OK loader=upstream-musl pt_interp=1 relative_relocation=1 entry=dynamic",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_MUSL_INTERP_REAP_OK status=42 lifecycle=spawn,interp,relocate,entry,exit,wait,reap",
                )
                send_command(stream, "musl-shared")
                wait_for_output(selector, process, output, b"MAKOS_MUSL_DYNAMIC_PROCESS_OK")
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_MUSL_DYNAMIC_OK loader=musl relocations=executed",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_MUSL_DYNAMIC_REAP_OK status=42 lifecycle=spawn,interp,needed-libc,relocate,main,exit,wait,reap",
                )
                send_command(stream, "musl-dso")
                wait_for_output(selector, process, output, b"MAKOS_MUSL_DSO_PROCESS_OK")
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_MUSL_DSO_OK loader=upstream-musl needed=libmakosdemo.so symbol=makos_shared_add result=42 file=vfs",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_MUSL_DSO_REAP_OK status=42 lifecycle=spawn,interp,open,fstat,mmap,relocate,symbol,main,exit,wait,reap",
                )
                send_command(stream, "musl-dlopen")
                wait_for_output(selector, process, output, b"MAKOS_MUSL_DLOPEN_PROCESS_OK")
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_MUSL_DLOPEN_OK loader=upstream-musl path=/usr/lib/libmakosdemo.so mode=RTLD_NOW dlsym=makos_shared_add result=42 dlclose=1 large_file=libc.so,multi-page",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_MUSL_DLOPEN_REAP_OK status=42 lifecycle=spawn,interp,main,dlopen,dlsym,call,dlclose,exit,wait,reap",
                )
                send_command(stream, "musl-exec")
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_MUSL_EXEC_CALL_OK syscall=execve target=/usr/bin/makos-exec-target",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_AARCH64_EXEC_OK",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_MUSL_EXEC_TARGET_OK argc=3 argv=alpha,two-words env=ready pid=preserved dynamic=1",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_MUSL_EXEC_REAP_OK status=42 lifecycle=spawn,execve,same-pid,interp,libc,main,exit,wait,reap",
                )
                # Real upstream MicroPython: VFS source -> parser -> bytecode
                # compiler -> VM. Comprehension and builtin sum distinguish it
                # from command-specific expression spoofing.
                send_command(
                    stream,
                    "write run.py print([x*x for x in range(6)]);print(sum(range(10)))",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_AARCH64_SHELL_CMD write bytes=52 persisted=1",
                )
                send_command(stream, "python run.py")
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_AARCH64_PYTHON_PROCESS_OK",
                    20,
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"[0, 1, 4, 9, 16, 25]",
                    20,
                )
                wait_for_output(selector, process, output, b"45\r\n", 20)
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_PYTHON_OK implementation=micropython version=1.28.0 parser=1 compiler=bytecode vm=1 gc=tracing source=vfs",
                    20,
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_AARCH64_PYTHON_LAUNCH_OK selector=4 process=isolated wait=1",
                    20,
                )
                send_pointer(stream, 400, 110)
                send_pointer(stream, 400, 110, True)
                send_pointer(stream, 350, 80, True)
                send_pointer(stream, 350, 80, False)
                wait_for_output(
                    selector, process, output, b"MAKOS_WINDOW_DRAG_OK surface=2"
                )
                # Resize live terminal: concrete cell backend updates dimensions;
                # compositor publishes winsize into POSIX TTY/SIGWINCH path.
                send_pointer(stream, 738, 512)
                send_pointer(stream, 738, 512, True)
                send_pointer(stream, 650, 450, True)
                send_pointer(stream, 650, 450, False)
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_WINDOW_RESIZE_OK surface=2 outline=fast commit=release width=633 height=357",
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_TTY_RESIZE_OK rows=18 columns=51 pixels=613x337",
                )
                click_pointer(stream, 644, 80)
                wait_for_output(
                    selector, process, output, b"MAKOS_WINDOW_CLOSE_OK surface=2"
                )
                click_pointer(stream, 50, 580)
                wait_for_output(
                    selector, process, output, b"MAKOS_START_MENU_OK launcher=1 open=1 apps=5"
                )
                click_pointer(stream, 100, 404)
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_APP_REOPEN_OK app=terminal source=start-menu surface=2 reopened=1",
                )
                click_pointer(stream, 50, 580)
                wait_for_output(
                    selector, process, output,
                    b"MAKOS_START_MENU_OK launcher=1 open=1 apps=5",
                )
                click_pointer(stream, 100, 444)
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_APP_REOPEN_OK app=settings source=start-menu surface=3 reopened=1",
                )
                # Create persistent account through Settings. Tab traverses all
                # fields; passwords render as stars and use same PBKDF2 DB path.
                send_pointer(stream, 590, 442)
                send_pointer(stream, 590, 442, True)
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_BUTTON_PRESS_OK control=2 phase=mouse-down bevel=sunken action=on-release cancel=pointer-leave",
                )
                response = qmp_command(
                    stream,
                    "screendump",
                    {"filename": str(BUTTON_PRESSED_SCREENSHOT)},
                )
                if "error" in response:
                    raise AssertionError(f"pressed-button screendump failed: {response}")
                send_pointer(stream, 590, 442, False)
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_SETTINGS_USERS_OPEN_OK source=button fields=username,password,confirm password=hidden tab=next feedback=sunken-on-press",
                )
                for key in ("g", "u", "i", "u", "s", "e", "r", "tab"):
                    send_key(stream, key)
                for key in (
                    "g", "u", "i", "minus", "p", "a", "s", "s", "1", "tab",
                    "g", "u", "i", "minus", "p", "a", "s", "s", "1",
                ):
                    send_key(stream, key)
                click_pointer(stream, 523, 448)
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_SETTINGS_ADDUSER_OK user=guiuser uid=2000 gid=2000 persisted=makfs-vfs password=pbkdf2-hmac-sha256 plaintext=never-stored",
                    20,
                )
                response = qmp_command(
                    stream,
                    "screendump",
                    {"filename": str(SETTINGS_USERS_SCREENSHOT)},
                )
                if "error" in response:
                    raise AssertionError(f"Settings Users screendump failed: {response}")
                click_pointer(stream, 620, 448)
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_SETTINGS_USERS_CANCEL_OK secrets=zeroed return=settings",
                )
                # Apply real guest scanout mode, verify QEMU display geometry,
                # then restore 800x600 before existing resize regression.
                click_pointer(stream, 397, 187)
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_SETTINGS_DISPLAY_OK requested=1024x768 applied=1 backend=virtio-gpu live=1",
                )
                response = qmp_command(
                    stream, "screendump", {"filename": str(MODE_SCREENSHOT)}
                )
                if "error" in response:
                    raise AssertionError(f"mode screendump failed: {response}")
                verify_mode_ppm(MODE_SCREENSHOT, (1024, 768))
                send_pointer(stream, 205, 187, screen_width=1024, screen_height=768)
                send_pointer(
                    stream, 205, 187, True, screen_width=1024, screen_height=768
                )
                send_pointer(
                    stream, 205, 187, False, screen_width=1024, screen_height=768
                )
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_SETTINGS_DISPLAY_OK requested=800x600 applied=1 backend=virtio-gpu live=1",
                )
                # Settings starts at (118,118), 560x360. Drag its bottom-right
                # grip to a deterministic 450x290 logical client size.
                send_pointer(stream, 670, 470)
                send_pointer(stream, 670, 470, True)
                send_pointer(stream, 560, 400, True)
                send_pointer(stream, 560, 400, False)
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_WINDOW_RESIZE_OK surface=3 outline=fast commit=release width=450 height=290 backing=bounded cursor_safe=1",
                )
                # Browser starts hidden and idle so networking cannot stall login.
                # Reopen from Start; normal regression fetches on explicit Enter.
                click_pointer(stream, 50, 580)
                wait_for_output(
                    selector, process, output,
                    b"MAKOS_START_MENU_OK launcher=1 open=1 apps=5",
                )
                click_pointer(stream, 100, 484)
                wait_for_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_APP_REOPEN_OK app=browser source=start-menu surface=5 reopened=1",
                )
                if (
                    os.environ.get("MAKOS_AARCH64_FIREFOX_PROBE") != "1"
                    and os.environ.get("MAKOS_AARCH64_SKIP_BROWSER_FETCH") != "1"
                ):
                    send_key(stream, "ret")
                    wait_for_output(
                        selector,
                        process,
                        output,
                        b"MAKOS_AARCH64_BROWSER_HTTP_OK dns=1 tcp=1 http=1 parser=1 render=1",
                        20,
                    )
                    wait_for_output(
                        selector,
                        process,
                        output,
                        b"MAKOS_AARCH64_IDLE_SLEEP_OK scheduler=wfi last_runnable=1 wake=deadline-or-runnable busy_spin=0",
                    )
                click_pointer(stream, 735, 85)
                wait_for_output(
                    selector, process, output, b"MAKOS_WINDOW_CLOSE_OK surface=5"
                )
                for key in (
                    "e", "d", "i", "t", "spc", "t", "e", "x", "t", "e", "d", "i", "t",
                    "dot", "t", "x", "t", "ret",
                ):
                    send_key(stream, key)
                wait_for_output(
                    selector, process, output,
                    b"MAKOS_TEXT_EDIT_OPEN_OK app=native-el0 vfs=real modal=1",
                )
                idle_presents = output.count(b"MAKOS_M7_OK graphics_abi=1 surface=680x420")
                drain_output(selector, process, output, 0.30)
                if output.count(b"MAKOS_M7_OK graphics_abi=1 surface=680x420") != idle_presents:
                    raise AssertionError("Text Edit presented frames while input was idle")
                send_key(stream, "t")
                click_pointer(stream, 785, 100)
                wait_for_output(
                    selector, process, output,
                    b"MAKOS_TEXT_EDIT_CLOSE_REQUEST_OK source=titlebar key=Escape surface=4",
                )
                wait_for_output(
                    selector, process, output,
                    b"MAKOS_TEXT_EDIT_CLOSE_REFUSED dirty=1 save=Ctrl-S-or-F2",
                )
                for key in (
                    "e", "x", "t", "minus", "e", "d", "i", "t", "minus",
                    "o", "x", "k", "left", "left", "delete", "home", "end",
                ):
                    send_key(stream, key)
                send_key(stream, "ctrl-s")
                wait_for_output(
                    selector, process, output,
                    b"MAKOS_TEXT_EDIT_SAVE_OK key=Ctrl-S write=complete dirty=0",
                )
                # Selection + clipboard must preserve the exact fixture:
                # select all, copy, cut, paste, then save restored bytes.
                send_key(stream, "ctrl-a")
                send_key(stream, "ctrl-c")
                wait_for_output(
                    selector, process, output,
                    b"MAKOS_CLIPBOARD_OK action=copy source=text-edit highlight=visible",
                )
                send_key(stream, "ctrl-x")
                wait_for_output(
                    selector, process, output,
                    b"MAKOS_CLIPBOARD_OK action=cut source=text-edit selection=deleted",
                )
                send_key(stream, "ctrl-v")
                wait_for_output(
                    selector, process, output,
                    b"MAKOS_CLIPBOARD_OK action=paste target=text-edit selection=replaced",
                )
                send_key(stream, "ctrl-s")
                wait_for_output_count(
                    selector,
                    process,
                    output,
                    b"MAKOS_TEXT_EDIT_SAVE_OK key=Ctrl-S write=complete dirty=0",
                    2,
                )
                # Save button must perform real VFS save too. Temporarily add
                # one byte, click client button, then restore exact fixture.
                send_key(stream, "x")
                button_marker = b"MAKOS_TEXT_EDIT_SAVE_REQUEST_OK source=button key=Ctrl-S surface=4"
                expected_button_saves = output.count(button_marker) + 1
                shortcut_marker = b"MAKOS_TEXT_EDIT_SAVE_OK key=Ctrl-S write=complete dirty=0"
                expected_shortcut_saves = output.count(shortcut_marker) + 1
                click_pointer(stream, 750, 133)
                wait_for_output_count(
                    selector, process, output, button_marker, expected_button_saves,
                )
                wait_for_output_count(
                    selector, process, output, shortcut_marker, expected_shortcut_saves,
                )
                send_key(stream, "backspace")
                expected_shortcut_saves = output.count(shortcut_marker) + 1
                send_key(stream, "ctrl-s")
                wait_for_output_count(
                    selector, process, output, shortcut_marker, expected_shortcut_saves,
                )
                send_key(stream, "esc")
                wait_for_output(
                    selector, process, output,
                    b"MAKOS_TEXT_EDIT_CLOSE_OK key=Escape return=shell",
                )
                for key in (
                    "c", "a", "t", "spc", "t", "e", "x", "t", "e", "d", "i", "t",
                    "dot", "t", "x", "t", "ret",
                ):
                    send_key(stream, key)
                wait_for_new_output(
                    selector, process, output,
                    b"MAKOS_AARCH64_SHELL_CMD cat bytes=12",
                )
                open_existing_marker = (
                    b"MAKOS_TEXT_EDIT_OPEN_OK app=native-el0 vfs=real "
                    b"modal=1 loaded=1"
                )
                expected_existing_opens = output.count(open_existing_marker) + 1
                for key in (
                    "e", "d", "i", "t", "spc", "t", "e", "x", "t", "e", "d", "i", "t",
                    "dot", "t", "x", "t", "ret",
                ):
                    send_key(stream, key)
                wait_for_output_count(
                    selector, process, output, open_existing_marker,
                    expected_existing_opens,
                )
                close_marker = b"MAKOS_TEXT_EDIT_CLOSE_OK key=Escape return=shell"
                expected_closes = output.count(close_marker) + 1
                close_request_marker = (
                    b"MAKOS_TEXT_EDIT_CLOSE_REQUEST_OK source=titlebar key=Escape surface=4"
                )
                expected_close_requests = output.count(close_request_marker) + 1
                click_pointer(stream, 785, 100)
                wait_for_output_count(
                    selector, process, output, close_request_marker,
                    expected_close_requests,
                )
                wait_for_output_count(
                    selector, process, output, close_marker, expected_closes,
                )
                send_command(stream, "cp textedit.txt copy.txt")
                wait_for_new_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_AARCH64_SHELL_CMD cp bytes=12 vfs=real persisted=1",
                )
                send_command(stream, "wc copy.txt")
                wait_for_new_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_AARCH64_SHELL_CMD wc lines=0 words=1 bytes=12 vfs=real",
                )
                send_command(stream, "mv copy.txt moved.txt")
                wait_for_new_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_AARCH64_SHELL_CMD mv vfs=real rename=atomic persisted=1",
                )
                send_command(stream, "rm moved.txt")
                wait_for_new_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_AARCH64_SHELL_CMD rm persisted=1",
                )
                # Terminal owns real mouse selection and the session clipboard.
                # Select exactly three visible cells, copy, then paste through
                # the bounded raw TTY queue. This also proves Ctrl-C is no
                # longer misrouted as Text Edit's private shortcut token.
                send_pointer(stream, 38, 115)
                send_pointer(stream, 38, 115, True)
                send_pointer(stream, 62, 115, True)
                send_pointer(stream, 62, 115, False)
                send_key(stream, "ctrl-c")
                wait_for_new_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_TERMINAL_CLIPBOARD_OK action=copy bytes=3 highlight=visible",
                )
                send_key(stream, "ctrl-v")
                wait_for_new_output(
                    selector,
                    process,
                    output,
                    b"MAKOS_TERMINAL_CLIPBOARD_OK action=paste bytes=3 bracketed=0",
                )
                # Serial command completion can precede compositor's bounded
                # 33 ms terminal repaint. Establish cursor baseline only after
                # that legitimate scene update, otherwise final cursor move is
                # falsely blamed for thousands of changed terminal glyphs.
                drain_output(selector, process, output, 0.10)
                send_pointer(stream, 760, 540)
                response = qmp_command(
                    stream,
                    "screendump",
                    {"filename": str(CURSOR_BASELINE_SCREENSHOT)},
                )
                if "error" in response:
                    raise AssertionError(f"cursor baseline screendump failed: {response}")
                for path, (x, y) in zip(
                    CURSOR_MOVE_SCREENSHOTS, CURSOR_MOVE_POSITIONS
                ):
                    send_pointer(stream, x, y)
                    response = qmp_command(
                        stream,
                        "screendump",
                        {"filename": str(path)},
                    )
                    if "error" in response:
                        raise AssertionError(
                            f"cursor move screendump failed at {(x, y)}: {response}"
                        )
                response = qmp_command(stream, "screendump", {"filename": str(SCREENSHOT)})
                if "error" in response:
                    raise AssertionError(f"QMP screendump failed: {response}")
                qmp_command(stream, "quit")
            process.wait(timeout=5)
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=5)
        missing_controls = [
            control
            for control in range(1, 12)
            if f"MAKOS_BUTTON_PRESS_OK control={control} ".encode() not in output
        ]
        if missing_controls:
            raise AssertionError(
                f"missing pressed-button coverage: {missing_controls}\n"
                f"{output.decode(errors='replace')}"
            )
        verify_ppm(
            SCREENSHOT,
            require_browser_page=(
                os.environ.get("MAKOS_AARCH64_SKIP_BROWSER_FETCH") != "1"
            ),
        )
        verify_start_menu_ppm(START_MENU_SCREENSHOT)
        verify_live_ui_ppm(SYSTEM_MONITOR_SCREENSHOT, "System Monitor")
        verify_live_ui_ppm(SETTINGS_USERS_SCREENSHOT, "Settings Users")
        verify_pressed_button_ppm(BUTTON_PRESSED_SCREENSHOT)
        verify_cursor_plane_scene_stable(
            CURSOR_BASELINE_SCREENSHOT,
            CURSOR_MOVE_SCREENSHOTS,
        )
        verify_cursor_scene_stable(CURSOR_BASELINE_SCREENSHOT, CURSOR_AFTER_SCREENSHOT)

        qmp_path_second = temp / "qmp-second.sock"
        second_command = [
            value.replace(str(qmp_path), str(qmp_path_second)) for value in command
        ]
        second = subprocess.Popen(
            second_command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT
        )
        assert second.stdout is not None
        second_output = bytearray()
        second_selector = selectors.DefaultSelector()
        second_selector.register(second.stdout, selectors.EVENT_READ)
        try:
            wait_for_output(
                second_selector,
                second,
                second_output,
                b"makfs_generation=2 boot_count=2",
                TIMEOUT_SECONDS,
            )
            wait_for_output(
                second_selector,
                second,
                second_output,
                b"MAKOS_AARCH64_BOOT_OK uefi=1 hvf_ready=1 native_isa=1",
                TIMEOUT_SECONDS,
            )
            wait_for_output(
                second_selector,
                second,
                second_output,
                b"MAKOS_STRUCTURED_LOG_PERSIST_OK path=/.makos-system-log format=MAKLOG01 records=",
                TIMEOUT_SECONDS,
            )
            persisted = re.findall(
                rb"MAKOS_STRUCTURED_LOG_PERSIST_OK .* records=(\d+) next_sequence=(\d+) cow=makfs4",
                second_output,
            )
            if not persisted:
                raise AssertionError("second-boot structured-log marker malformed")
            persisted_records, persisted_next = map(int, persisted[-1])
            if not 2 <= persisted_records <= 32 or persisted_next < 3:
                raise AssertionError(
                    "second-boot structured-log merge invalid "
                    f"records={persisted_records} next_sequence={persisted_next}"
                )
            audit_match = re.search(
                rb"MAKOS_SECURITY_AUDIT_PERSIST_OK source=prior-boot severity=4 "
                rb"records=(\d+) auth_accepted=(\d+) auth_denied=(\d+) "
                rb"account=(\d+) session=(\d+) package=(\d+) pid_attributed=1",
                second_output,
            )
            if not audit_match:
                raise AssertionError("second-boot persisted security-audit marker missing")
            (
                audit_records,
                audit_auth_accepted,
                _audit_auth_denied,
                audit_account,
                _audit_session,
                _audit_package,
            ) = map(int, audit_match.groups())
            if audit_records < 2 or audit_auth_accepted < 1 or audit_account < 1:
                raise AssertionError(
                    "second-boot persisted security-audit coverage incomplete "
                    f"records={audit_records} auth_accepted={audit_auth_accepted} "
                    f"account={audit_account}"
                )
            print(
                "MAKOS_SECURITY_AUDIT_TWO_BOOT_OK "
                f"records={audit_records} auth_accepted={audit_auth_accepted} "
                f"account={audit_account} pid_attributed=1"
            )
            wait_for_socket(qmp_path_second, second)
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
                client.connect(str(qmp_path_second))
                stream = client.makefile("rwb", buffering=0)
                json.loads(stream.readline())
                if "error" in qmp_command(stream, "qmp_capabilities"):
                    raise AssertionError("second-boot QMP negotiation failed")
                for key in (
                    "m", "a", "r", "c", "u", "s", "ret",
                    "m", "a", "k", "o", "s", "ret",
                ):
                    send_key(stream, key)
                wait_for_output(
                    second_selector,
                    second,
                    second_output,
                    b"MAKOS_AARCH64_DESKTOP_OK",
                )
                wait_for_output(
                    second_selector,
                    second,
                    second_output,
                    b"MAKOS_AARCH64_BROWSER_BACKGROUND_OK startup_fetch=0 reopen=start-menu state=retained",
                    20,
                )
                # Browser and Files start concurrently, so z-order is not a
                # stable way to select Files. Activate its retained surface
                # through Start before clicking its close button.
                click_pointer(stream, 50, 580)
                wait_for_output(
                    second_selector,
                    second,
                    second_output,
                    b"MAKOS_START_MENU_OK launcher=1 open=1 apps=5",
                )
                click_pointer(stream, 100, 524)
                wait_for_output(
                    second_selector,
                    second,
                    second_output,
                    b"MAKOS_APP_REOPEN_OK app=files source=start-menu surface=6 reopened=0",
                )
                click_pointer(stream, 725, 102)
                wait_for_output(
                    second_selector,
                    second,
                    second_output,
                    b"MAKOS_AARCH64_FILES_CLOSE_OK background=1 reopen=start-menu state=retained",
                )
                for key in (
                    "c", "a", "t", "spc", "t", "e", "s", "t", "dot", "t", "x", "t", "ret",
                ):
                    send_key(stream, key)
                wait_for_output(
                    second_selector,
                    second,
                    second_output,
                    b"MAKOS_AARCH64_SHELL_CMD cat bytes=12",
                )
                send_command(stream, "musl-pthread")
                wait_for_output(
                    second_selector,
                    second,
                    second_output,
                    b"MAKOS_MAKFS_DIRECTORY_PERSIST_OK phase=remount-read-cleanup format=makfs4 path=/home/user/persist/sub/value.txt scalable=64-siblings,name255,indexed-lookup,cursor",
                    20,
                )
                wait_for_output(
                    second_selector,
                    second,
                    second_output,
                    b"MAKOS_MAKFS_DIRECTORY_SCALE_OK siblings=64 name_bytes=255 lookup=hash-index cursor=resumable remount=verified",
                    20,
                )
                wait_for_output(
                    second_selector,
                    second,
                    second_output,
                    b"MAKOS_MUSL_PTHREAD_REAP_OK status=42 lifecycle=spawn,threads,join,exit,wait,reap",
                    20,
                )
                click_pointer(stream, 50, 580)
                wait_for_output(
                    second_selector,
                    second,
                    second_output,
                    b"MAKOS_START_MENU_OK launcher=1 open=1 apps=5",
                )
                send_pointer(stream, 100, 324)
                send_pointer(stream, 100, 324, True)
                wait_for_output(
                    second_selector,
                    second,
                    second_output,
                    b"MAKOS_BUTTON_PRESS_OK control=12 phase=mouse-down bevel=sunken action=on-release cancel=pointer-leave",
                )
                send_pointer(stream, 100, 324, False)
                wait_for_output(
                    second_selector,
                    second,
                    second_output,
                    b"MAKOS_SIGNOUT_REQUEST_OK source=start-menu feedback=sunken-on-press deferred=svc-yield",
                )
                wait_for_output(
                    second_selector,
                    second,
                    second_output,
                    b"MAKOS_SIGNOUT_DEFERRED_OK source=start-menu context=svc-yield blocking-in-irq=0",
                )
                wait_for_output(
                    second_selector,
                    second,
                    second_output,
                    b"MAKOS_SIGNOUT_OK generation=",
                )
                wait_for_output(
                    second_selector,
                    second,
                    second_output,
                    b"MAKOS_AARCH64_GUI_SIGNOUT_SYNC_OK shell=login-loop app-pids=cleared",
                )
                wait_for_output_count(
                    second_selector,
                    second,
                    second_output,
                    b"MAKOS_LOGIN_UI_OK",
                    2,
                )
                for key in (
                    "g", "u", "i", "u", "s", "e", "r", "tab",
                    "g", "u", "i", "minus", "p", "a", "s", "s", "1", "ret",
                ):
                    send_key(stream, key)
                wait_for_output(
                    second_selector,
                    second,
                    second_output,
                    b"MAKOS_LOGIN_OK user=guiuser uid=2000 gid=2000 session=1 credential=per-process password_hash=pbkdf2-hmac-sha256 iterations=100000 bad_password_denied=1",
                    20,
                )
                wait_for_new_output(
                    second_selector,
                    second,
                    second_output,
                    b"MAKOS_AARCH64_DESKTOP_OK",
                    20,
                )
                qmp_command(stream, "quit")
            second.wait(timeout=5)
        finally:
            if second.poll() is None:
                second.terminate()
                second.wait(timeout=5)
        preserve_data_image = os.environ.get("MAKOS_AARCH64_PRESERVE_DATA_IMAGE")
        if preserve_data_image:
            if configured_system_image:
                raise ValueError("cannot export a separate data image from GPT-system mode")
            destination = pathlib.Path(preserve_data_image)
            destination.parent.mkdir(parents=True, exist_ok=True)
            copy_sparse(data_image, destination)
            print(f"MAKOS_AARCH64_DATA_IMAGE_EXPORTED path={destination}")
    print(f"AArch64 {accel} UEFI boot and framebuffer test passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
