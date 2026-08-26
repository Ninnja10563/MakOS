#!/usr/bin/env python3
"""Guard strict Firefox clipboard and real-link interaction coverage."""

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BOOT = (ROOT / "scripts/boot_test_aarch64.py").read_text()
MAKEFILE = (ROOT / "Makefile").read_text()
INPUT = (ROOT / "kernel/src/aarch64_virtio_input.rs").read_text()
MOUSE_BUTTONS_PATCH = (
    ROOT / "ports/firefox/patches/0054-headless-synthesized-mouse-buttons.patch"
).read_text()
LOCATION_PATCH = (
    ROOT / "ports/firefox/patches/0055-makos-location-shortcut.patch"
).read_text()
WHEEL_PATCH = (
    ROOT / "ports/firefox/patches/0039-makos-wheel-events.patch"
).read_text()
WHEEL_DIRECTION_PATCH = (
    ROOT / "ports/firefox/patches/0056-makos-native-wheel-direction.patch"
).read_text()
PROCESS = (ROOT / "kernel/src/aarch64_process.rs").read_text()
ARCH = (ROOT / "kernel/src/arch/aarch64.rs").read_text()
MAIN_HANDOFF_PATCH = (
    ROOT / "ports/firefox/patches/0057-makos-post-enqueue-main-handoff.patch"
).read_text()

for fragment in (
    'os.environ.get("MAKOS_AARCH64_FIREFOX_CLIPBOARD") == "1"',
    '"MAKOS_FIREFOX_CLIPBOARD_OK "',
    'os.environ.get("MAKOS_AARCH64_FIREFOX_LINK_CLICK") == "1"',
    '"https://www.iana.org/help/example-domains"',
    'shutil.copyfile(',
    'click_pointer(',
    'rb"MAKOS_CURSOR_FOCUS_OK "',
    'rb"hit_test=1 focused_surface=5 "',
    'link_loaded and link_tls',
    'link_loaded\n                                and link_tls\n                                and link_changed',
    '"MAKOS_FIREFOX_MOUSE_LINK_OK "',
    '"MAKOS_AARCH64_FIREFOX_DOCUMENT_SELECTION"',
    '"Firefox document selection pointer-down not dispatched"',
    "verify_firefox_selection_changed(",
    'send_key(stream, "ctrl-c")',
    '"ctrl-v",',
    '"MAKOS_FIREFOX_DOCUMENT_SELECTION_OK "',
    '"copy=document paste=makos exact_uri=1 "',
    'send_key(stream, "ctrl-l")',
    'b"MAKOS_WIDGET_KEY raw=136"',
    "verify_firefox_url_selected(",
    '"MAKOS_AARCH64_FIREFOX_SUSTAINED_INTERACTION"',
    'send_wheel(stream, "wheel-down")',
    'send_wheel(stream, "wheel-up")',
    '"https://httpbin.org/forms/post"',
    '"https://example.com/?customer="',
    'repeated_uris = (\n                            "https://www.iana.org/help/example-domains",\n                            "https://example.com/",',
    '"MAKOS_AARCH64_FIREFOX_SUSTAINED_PAINT_SECONDS"',
    '"Firefox repeated-navigation paint timed out "',
    '"MAKOS_FIREFOX_SUSTAINED_INTERACTION_OK "',
    'process_resident_bytes(process.pid)',
    'rb".* resident_pages=([0-9]+) resident_kib=([0-9]+)"',
    'os.environ.get("MAKOS_AARCH64_FIREFOX_SMP_REQUIRED") == "1"',
    'rb"MAKOS_AARCH64_FIREFOX_SMP_OVERLAP_OK "',
    '"Firefox did not overlap distinct worker TIDs on multiple guest CPUs"',
    '"MAKOS_FIREFOX_SMP_OVERLAP_OK "',
    'rb"MAKOS_AARCH64_APPLICATION_PLACEMENT_OK role=firefox "',
    'placed_cpus != {1, 2, 3}',
    'rb"MAKOS_AARCH64_APPLICATION_MIGRATION_OK role=firefox "',
    'loads[source_cpu - 1]\n                                >= loads[target_cpu - 1] + 64',
    '"Firefox did not provide a live kernel-owned load migration"',
    '"MAKOS_FIREFOX_SMP_AUTOBALANCE_OK "',
):
    assert fragment in BOOT, fragment

for fragment in (
    "MAKOS_AARCH64_FIREFOX_CLIPBOARD=1",
    "MAKOS_AARCH64_FIREFOX_LINK_CLICK=1",
    "MAKOS_AARCH64_FIREFOX_LINK_URI=https://www.iana.org/help/example-domains",
    "MAKOS_AARCH64_FIREFOX_SMP_REQUIRED=1",
):
    assert fragment in MAKEFILE, fragment

for fragment in (
    "mSynthesizedMouseButtons",
    "MouseButtonsFlagToChange(aButton)",
    "event.mButtons = mSynthesizedMouseButtons",
):
    assert fragment in MOUSE_BUTTONS_PATCH, fragment

for fragment in ("KEY_FOCUS_LOCATION", "code == KEY_L"):
    assert fragment in INPUT, fragment

for fragment in ("case 0x88:", "NS_VK_L", "MODIFIER_CONTROL"):
    assert fragment in LOCATION_PATCH, fragment

for fragment in ("SurfaceEventKind::Scroll", "SynthesizeNativeMouseScrollEvent"):
    assert fragment in WHEEL_PATCH, fragment

for fragment in (
    "#elif defined(MOZ_WIDGET_MAKOS)",
    "mozilla::dom::WheelEvent_Binding::DOM_DELTA_LINE",
):
    assert fragment in WHEEL_DIRECTION_PATCH, fragment

for fragment in ("user_resident_pages", "resident_pages={}", "resident_kib={}"):
    assert fragment in PROCESS or fragment in ARCH, fragment

for fragment in (
    "SurfaceMainHandoffReady = 149",
    "NotifySurfaceMainHandoffReady",
    "mainRunnableReady = !scheduleDrain",
    "if (NS_SUCCEEDED(result))",
    "MAKOS_WIDGET_MAIN_HANDOFF_%s source=post-enqueue",
):
    assert fragment in MAIN_HANDOFF_PATCH, fragment

for fragment in (
    "SYS_SURFACE_MAIN_HANDOFF_READY",
    "complete_firefox_process_leader_handoff",
):
    assert fragment in ARCH, fragment

print(
    "MAKOS_AARCH64_FIREFOX_INTERACTION_TEST_OK "
    "clipboard=round-trip mouse=left-link selection=document-drag-copy-paste "
    "sustained=wheel,form,repeated-navigation,resources tls=builtin-root "
    "handoff=post-enqueue-syscall:149 pixels=changed"
)
