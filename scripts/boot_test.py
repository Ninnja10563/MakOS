#!/usr/bin/env python3
"""Boot MakOS under QEMU and require kernel serial success marker."""

from __future__ import annotations

import os
import pathlib
import selectors
import shutil
import socket
import struct
import subprocess
import sys
import tempfile
import time
import zlib


def write_stdout(data: bytes) -> None:
    """Write captured serial output even when parent PTY is nonblocking."""
    fd = sys.stdout.fileno()
    offset = 0
    deadline = time.monotonic() + 10
    while offset < len(data):
        try:
            written = os.write(fd, data[offset : offset + 4096])
        except BlockingIOError:
            if time.monotonic() >= deadline:
                return
            time.sleep(0.01)
            continue
        if written <= 0:
            return
        offset += written


def firmware() -> str:
    candidates = [
        os.environ.get("OVMF_CODE", ""),
        "/opt/homebrew/share/qemu/edk2-x86_64-code.fd",
        "/usr/local/share/qemu/edk2-x86_64-code.fd",
        "/usr/share/OVMF/OVMF_CODE.fd",
        "/usr/share/edk2/x64/OVMF_CODE.fd",
    ]
    for candidate in candidates:
        if candidate and pathlib.Path(candidate).is_file():
            return candidate
    raise RuntimeError("OVMF x86_64 firmware not found; set OVMF_CODE")


def main() -> int:
    gpt = len(sys.argv) == 3 and sys.argv[1] == "--gpt"
    if len(sys.argv) != 3:
        print(
            f"usage: {sys.argv[0]} BOOT_IMAGE DATA_IMAGE | --gpt SYSTEM_IMAGE",
            file=sys.stderr,
        )
        return 2
    qemu = os.environ.get("QEMU_SYSTEM_X86_64") or shutil.which("qemu-system-x86_64")
    if not qemu:
        raise RuntimeError("qemu-system-x86_64 not found")

    with tempfile.TemporaryDirectory(prefix="makos-boot-test-", dir="build") as temporary:
        work = pathlib.Path(temporary)
        boot_image = work / "boot.img"
        data_image = boot_image if gpt else work / "data.img"
        monitor = work / "monitor.sock"
        screenshot = pathlib.Path("build/makos-desktop.ppm").resolve()
        login_screenshot = pathlib.Path("build/makos-login.ppm").resolve()
        screenshot.unlink(missing_ok=True)
        login_screenshot.unlink(missing_ok=True)
        if gpt:
            shutil.copy2(sys.argv[2], boot_image)
            data_offset = gpt_data_offset(boot_image)
        else:
            shutil.copy2(sys.argv[1], boot_image)
            shutil.copy2(sys.argv[2], data_image)
            data_offset = 0
        seed_legacy_dynamic_file(data_image, data_offset)

        def command() -> list[str]:
            monitor.unlink(missing_ok=True)
            result = [
                qemu,
                "-machine", "pc,accel=tcg",
                "-cpu", "qemu64",
                "-smp", "4",
                "-m", "256M",
                "-drive", f"if=pflash,format=raw,readonly=on,file={firmware()}",
            ]
            result += [
                "-drive", f"id=boot,if=none,format=raw,file={boot_image}",
                "-device", f"ide-hd,drive=boot,bus={'ide.1' if gpt else 'ide.0'},unit=0",
            ]
            if not gpt:
                result += [
                    "-drive", f"id=data,if=none,format=raw,file={data_image}",
                    "-device", "ide-hd,drive=data,bus=ide.1,unit=0",
                ]
            result += [
                "-display", "none",
                "-serial", "stdio",
                "-monitor", f"unix:{monitor},server=on,wait=off",
                "-netdev", "user,id=net0",
                "-device", "rtl8139,netdev=net0",
                "-audiodev", "driver=none,id=audio0",
                "-device", "AC97,audiodev=audio0",
                "-device", "piix3-usb-uhci,id=usb",
                "-device", "usb-kbd,bus=usb.0",
                "-no-reboot",
                "-no-shutdown",
            ]
            if capture := os.environ.get("MAKOS_PCAP"):
                result.extend(
                    ["-object", f"filter-dump,id=netdump,netdev=net0,file={capture}"]
                )
            return result

        first = run_boot(
            command(), expected_boot_count=1, print_output=False, monitor=monitor,
            screenshot=None, login_screenshot=None, require_gpt=gpt,
        )
        if first != 0:
            return first
        corrupt_primary_superblock(data_image, data_offset)
        second = run_boot(
            command(), expected_boot_count=2, print_output=True, monitor=monitor,
            screenshot=screenshot, login_screenshot=login_screenshot, require_gpt=gpt,
        )
        if second == 0 and (
            not screenshot.is_file() or screenshot.read_bytes()[:2] != b"P6"
        ):
            print("boot test failed: framebuffer screenshot absent", file=sys.stderr)
            return 1
        if second == 0 and (
            not login_screenshot.is_file()
            or login_screenshot.read_bytes()[:2] != b"P6"
        ):
            print("boot test failed: login screenshot absent", file=sys.stderr)
            return 1
        return second

def gpt_data_offset(image: pathlib.Path) -> int:
    makos_data_type = bytes.fromhex("748f6a8d333e444da2e70f5a4b4f5301")
    with image.open("rb") as disk:
        disk.seek(512)
        header = disk.read(512)
        if header[:8] != b"EFI PART":
            raise RuntimeError("system image lacks primary GPT")
        entries_lba, entry_count, entry_bytes = struct.unpack_from("<QII", header, 72)
        if entry_count == 0 or entry_count > 128 or entry_bytes != 128:
            raise RuntimeError("system image has unsupported GPT entries")
        disk.seek(entries_lba * 512)
        for _ in range(entry_count):
            entry = disk.read(entry_bytes)
            if entry[:16] == makos_data_type:
                first, last = struct.unpack_from("<QQ", entry, 32)
                if first <= last:
                    return first * 512
    raise RuntimeError("system image lacks MakOS data partition")


def corrupt_primary_superblock(image: pathlib.Path, data_offset: int = 0) -> None:
    checksum_bytes = (512 + 508, 7 * 512 + 508)
    with image.open("r+b") as disk:
        for checksum_byte in checksum_bytes:
            disk.seek(data_offset + checksum_byte)
            value = disk.read(1)
            if len(value) != 1:
                raise RuntimeError("data image lacks MakFS recovery metadata")
            disk.seek(data_offset + checksum_byte)
            disk.write(bytes((value[0] ^ 0x80,)))


def seed_legacy_dynamic_file(image: pathlib.Path, data_offset: int = 0) -> None:
    """Seed one v1 fixed-slot record; kernel must migrate it to allocator v2."""
    name = b"legacy.txt"
    data = b"legacy-migration"
    record = bytearray(512)
    record[0:8] = b"MAKDYN02"
    record[8] = 1
    record[9] = 0
    record[10] = len(name)
    record[12:16] = struct.pack("<I", 0o100600)
    record[16:20] = struct.pack("<I", 1000)
    record[20:24] = struct.pack("<I", 1000)
    record[24:28] = struct.pack("<I", len(data))
    record[32 : 32 + len(name)] = name
    record[64 : 64 + len(data)] = data
    record[508:512] = struct.pack("<I", zlib.crc32(record[:508]))
    with image.open("r+b") as disk:
        disk.seek(data_offset + 7 * 512)
        disk.write(record)


def run_boot(
    command: list[str],
    expected_boot_count: int,
    print_output: bool,
    monitor: pathlib.Path,
    screenshot: pathlib.Path | None,
    login_screenshot: pathlib.Path | None,
    require_gpt: bool = False,
) -> int:
    process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    assert process.stdout is not None
    selector = selectors.DefaultSelector()
    selector.register(process.stdout, selectors.EVENT_READ)
    deadline = time.monotonic() + 150
    output = bytearray()
    required = [
        b"MAKOS_CONFIG_OK source=fat bytes=40 root=ata1 log=serial makfs_recover=auto",
        b"pmm managed_mib=",
        b"self_test=ok",
        b"heap bytes=1048576 box_vec=ok",
        b"MAKOS_CREDENTIAL_POLICY_OK unknown_pid=denied kernel_context=denied explicit_pid1=1 ambient_caps=0",
        b"cpu gdt=owned",
        b"paging cr3=",
        b"page_fault=ok",
        b"acpi cpus=4",
        b"apic id=",
        b"smp online=4 init_sipi=ok",
        b"process init elf=ok",
        b"MAKOS_PROCESS_OK parent=1 child=2 concurrent=1 isolated_cr3=1 wait=1 exit_code=42",
        b"MAKOS_PROCESS_REAP pid=2 frames=",
        b"MAKOS_MAKFS_ALLOCATOR_OK inodes=16 blocks=80 max_file=2048 bitmap=1 reconcile=1",
        b"MAKOS_MAKFS4_READY state=",
        b"generation=1 block_bytes=4096 volume_blocks=262144 data_start=131072 max_inodes=512 extents=14 cow=inode,bitmap,catalog root=redundant flush=metadata,root",
        b"MAKOS_LINUX_APP_OK write=1 getpid=1 uname=1 clock_gettime=1 exit=1",
        b"MAKOS_LINUX_OK personality=linux-x86_64 apis=write,getpid,uname,clock_gettime,exit tests=5 trap=int80-adapter",
        b"MAKOS_COMPAT_SPAWN personality=win32-x86_64 pid=4 loader=pe32+",
        b"MAKOS_WINDOWS_APP_OK write=1 pid=1 time=1 event=1 wait=1 close=1 exit=1 ms_abi=1 pe32=1",
        b"MAKOS_WINDOWS_OK personality=win32-x86_64 loader=pe32+ apis=WriteFile,GetCurrentProcessId,GetTickCount64,CreateEventA,SetEvent,WaitForSingleObject,CloseHandle,ExitProcess tests=8 thunk=int80-adapter",
        b"MAKOS_SERVICE_RUN unit=demo generation=1 outcome=failure",
        b"MAKOS_PROCESS_FAULT pid=5 vector=14",
        b"MAKOS_SERVICE_RUN unit=demo generation=2 outcome=success",
        b"MAKOS_SERVICE_OK unit=demo starts=2 restart=1 policy=on-failure first_exit=142 fault_contained=1 final_exit=0 state=completed isolated=1",
        b"MAKOS_POSIX_OK app=c-worker libc=static write=1 open=1 read=1 close=1",
        b"MAKOS_SDK_SPAWN_ARGS_OK version=1 argc=2 envc=1 packed=1 sandbox_denied=1",
        b"MAKOS_UDP_SOCKET_OK family=inet type=dgram create=1 connect=1 send=1 recv=1 close=1 dns=1",
        b"MAKOS_IPV6_OK ethernet=1 ipv6=1 icmpv6=1 ndp=1 echo=1 checksum=1 source=fec0::15 gateway=fec0::2 ring3=1",
        b"MAKOS_TCP_OK connect=1 syn=1 synack=1 ack=1 send=1 recv=1 checksum=1 http=1 socket_object=1 close=1",
        b"MAKOS_SOCKET_OK family=inet objects=2 udp=1 tcp=1 create=2 connect=2 send=2 recv=2 close=2 stale_denied=1 dns=1 http=1",
        b"MAKOS_TCP_OK connect=1 syn=1 synack=1 ack=1 send=1 recv=1 checksum=1 http=1",
        b"MAKOS_SHELL_READY",
        b"MAKOS_M7_OK graphics_abi=1",
        b"windows=2 z_order=1 clipping=1",
        b"MAKOS_MOUSE_OK x=",
        b"cursor=rendered redraw=raw-save-restore no_trails=1 input=irq coalesce=latest",
        b"MAKOS_CURSOR_FOCUS_OK cursor=software buttons=left hit_test=1 focused_surface=2 z_order=raised",
        b"MAKOS_WINDOW_DRAG_OK surface=2 outline=fast commit=release",
        b"MAKOS_TASKBAR_APP_OK surface=2 activate=1 minimize_toggle=1",
        b"MAKOS_WINDOW_CLOSE_OK surface=1 close_button=1 taskbar_removed=1",
        b"MAKOS_START_MENU_OK launcher=1 open=1 apps=2",
        b"MAKOS_APP_REOPEN_OK app=system-monitor source=start-menu surface=1 reopened=1",
        b"MAKOS_APP_REOPEN_OK app=terminal source=start-menu surface=2 reopened=1",
        b"MAKOS_AUDIO_OK",
        b"MAKOS_AUDIO_STREAM_OK pid=1 frames=96 rate=48000 channels=2 userspace=1 dma=1",
        b"MAKOS_AUDIO_API_OK format=s16le rate=48000 channels=2 frames=96 ring3=1",
        b"MAKOS_USB_OK controller=uhci device=keyboard control_transfer=1 descriptor=1 hid=1",
        b"input ps2_keyboard=set2 ps2_mouse=3byte irq=1,12 coalesce=latest edge_queue=16 accel=adaptive key_queue=64 polling=0",
        b"MAKOS_LOGIN_UI_OK framebuffer=1280x800 prompt=visible console=live cursor=rendered",
        b"MAKOS_LOGIN_CLICK_OK form=retained blank_screen=0 active_field=username cursor=rendered",
        b"MAKOS_SECURITY_OK uid=1000 gid=1000",
        b"MAKOS_LOGIN_OK user=marcus uid=1000 gid=1000 session=1 password_hash=pbkdf2-hmac-sha256 iterations=100000 bad_password_denied=1",
        b"MAKOS_ABI_OK version=1.0 arch=x86_64 calling=int80 feature_query=1 max_syscall=57",
        b"MAKOS_TOOLCHAIN_SPAWN pid=6 format=elf64",
        b"MAKOS_TOOLCHAIN_APP_OK compiler=expr assembler=x86_64 source=20+22 emitted=6 result=42 wx_denied=1 rx_exec=1",
        b"MAKOS_SELFHOST_OK stage=seed compiler=expr assembler=x86_64 generated=native-code result=42 wx=1 isolated=1",
        b"MAKOS_TOOLCHAIN_ELF_OK path=/home/user/generated.elf format=elf64 persisted=1 emitted_by=toolchain result=42 segments=2 data_nx=1",
        b"MAKOS_TOOLCHAIN_ELF_REJECT_FIXTURE_OK path=/home/user/overlap.elf layout=overlapping-load-segments",
        b"MAKOS_EXEC_SPAWN pid=7 slot=0 source=makfs format=elf64",
        b"MAKOS_EXEC_SPAWN pid=8 slot=1 source=makfs format=elf64",
        b"MAKOS_GENERATED_APP_OK source=makfs loader=elf64 emitted_by=toolchain result=42 isolated=1 segments=2 data_nx=1 startup_abi=1",
        b"MAKOS_PROCESS_REAP pid=7 frames=10",
        b"MAKOS_PROCESS_REAP pid=8 frames=10",
        b"MAKOS_EXEC_STARTUP_OK abi=1 argc=3 argv=3 envp=1 auxv=pagesz,entry register_args=1 stack_contract=1 malformed_denied=3 legacy_abi56=1",
        b"MAKOS_EXEC_PATH_OK path=/home/user/generated.elf source=makfs format=elf64 segments=2 code_rx=1 data_nx=1 validate=1 map=1 ring3=1 children=3 pids=7,8,7 concurrent=2 exits=42,42,42 reaped=3 invalid_denied=1 overlap_denied=1",
        b"MAKOS_SANDBOX_OK pid=2 uid=1001 caps=console,graphics,sync ipc_denied=1 network_denied=1 package_denied=1",
        b"MAKOS_VM_OK pid=2 mmap=1 writable=1 nx=1 munmap=1 reclaimed=1",
        b"MAKOS_VM_REGION_OK pid=2 regions=4 max_region_pages=16 mapped_pages=11 zero_fill=1 partial_protect=1 wx_denied=1 copyin_rx=1 copyout_rx_denied=1 unmapped_denied=1 unmap=3 exact_length=1 hole_reuse=1 leaked_for_reaper=2",
        b"MAKOS_VM_REAP_OK pid=2 regions=1 pages=2 address_space_destroy=1",
        b"MAKOS_VM_REAP_OK pid=6 regions=1 pages=1 address_space_destroy=1",
        b"MAKOS_THREAD_OK pid=2 tid=3 shared_cr3=1 separate_stacks=1 join=1 exit=7",
        b"MAKOS_SYNC_OK object=event signal=1 blocking_wait=1 wake=1 thread_argument=1 handle_close=1",
        b"MAKOS_UPDATE_OK package=makos-core",
        b"MAKOS_PACKAGE_OK install=1 dependency=libc dependency_resolved=1 upgrade=2.0 rollback=1.0 remove=1 removal_rollback=1 content_hash=sha256 transactional=1 signature=rsa2048-sha256 tamper_denied=1",
        b"MAKOS_LOG_OK structured=1 ring=32 pid=1 severity=5 monotonic=1 readback=1",
        b"MAKOS_VFS_OK mount=/ file=/boot-count.txt fd=1 read=1 close=1 write_denied=1",
        b"MAKOS_DIR_OK stat=1 readdir=1 root_entries=2 nested=1 metadata=mode,uid,gid,size,mtime,inode",
        f"MAKOS_FILE_RW_OK path=/home/user/note.txt write=1 readback=1 previous={expected_boot_count - 1} mode=0600".encode(),
        b"MAKOS_SHELL_CMD help",
        b"MAKOS_SHELL_CMD status",
        b"MAKOS_SHELL_CMD mem free_frames=",
        b"MAKOS_SHELL_CMD ps occupied=",
        b"MAKOS_SHELL_CMD pwd",
        b"MAKOS_SHELL_CMD ls entries=",
        b"MAKOS_SHELL_CMD echo bytes=12",
        b"lower_case!?",
        b"MAKOS_SHELL_CMD touch persisted=1",
        b"MAKOS_SHELL_CMD write bytes=12 persisted=1",
        b"MAKOS_SHELL_CMD cat bytes=12",
        b"MAKOS_SHELL_CMD stat size=12",
        b"MAKOS_SHELL_CMD rm persisted=1",
        b"MAKOS_SHELL_CMD uptime",
        b"MAKOS_SHELL_CMD exit close=1 pid1_alive=1 reopen=start-menu retained=1",
        b"MAKOS_TERMINAL_READY grid=58x22 retained=1 commands=real",
        b"MAKOS_M3_OK ring3=1 int80=1 ipc=1",
        f"MAKOS_M4_OK ata_sectors=2097152 makfs_generation={expected_boot_count} boot_count={expected_boot_count}".encode(),
        b"MAKOS_M5_OK",
        b"tcp_synack=1",
        b"MAKOS_M2_OK",
    ]
    if require_gpt:
        required.append(
            b"MAKOS_GPT_DATA_OK start_lba=133120 sectors=2097152 legacy_raw=0"
        )
    if expected_boot_count == 2:
        required.append(
            b"MAKOS_MAKFS_RECOVERY_OK degraded_copy=primary recovered_from=backup repaired=1 generation=2"
        )
        required.append(
            b"MAKOS_MAKFS_BITMAP_RECOVERY_OK source=crc-inodes repaired=1"
        )
        required.append(
            b"MAKOS_FILE_UNLINK_OK path=/home/user/* persisted=10 unlink=10 absent=10 inodes=10 capacity=16 arbitrary_names=1 multiblock=1 bytes=1024 blocks=2 bitmap_reuse=1"
        )
        required.append(
            b"MAKOS_TOOLCHAIN_ELF_PERSIST_OK path=/home/user/generated.elf existing=1 magic=elf64 remount=1"
        )
    else:
        required.append(
            b"MAKOS_MAKFS_MIGRATE_OK from=dynamic-v1 to=dynamic-v2 files=1"
        )
        required.append(
            b"MAKOS_FILE_CREATE_OK path=/home/user/* create=10 write=10 read=1 inodes=10 capacity=16 arbitrary_names=1 multiblock=1 bytes=1024 blocks=2 persisted_pending=1"
        )
    login_sent = False
    commands_sent = False
    generated = b"MAKOS_GENERATED_APP_OK source=makfs loader=elf64 emitted_by=toolchain result=42 isolated=1 segments=2 data_nx=1 startup_abi=1"
    try:
        while time.monotonic() < deadline:
            for key, _ in selector.select(timeout=0.5):
                chunk = os.read(key.fileobj.fileno(), 4096)
                if not chunk:
                    break
                output.extend(chunk)
                if os.environ.get("MAKOS_BOOT_TRACE"):
                    print(chunk.decode("utf-8", errors="replace"), end="", flush=True)
                if not login_sent and b"MAKOS_LOGIN_READY" in output:
                    click_login(monitor)
                    if login_screenshot is not None:
                        time.sleep(0.2)
                        capture_screenshot(monitor, login_screenshot)
                    send_keyboard_lines(
                        monitor,
                        ("marcus", "makos"),
                    )
                    login_sent = True
                if not commands_sent and b"MAKOS_SHELL_READY" in output:
                    send_shell_commands(monitor, screenshot)
                    commands_sent = True
                marker_index = output.find(required[-1])
                complete_marker_line = marker_index >= 0 and b"\n" in output[marker_index:]
                if (
                    all(marker in output for marker in required)
                    and output.count(generated) == 3
                    and complete_marker_line
                ):
                    if print_output:
                        write_stdout(bytes(output))
                        write_stdout(b"two-boot persistence test passed\n")
                    return 0
            if process.poll() is not None:
                break
        write_stdout(bytes(output))
        missing = [marker.decode() for marker in required if marker not in output]
        if output.count(generated) != 3:
            missing.append(f"generated app count=3 (actual={output.count(generated)})")
        print(f"boot test failed: missing markers: {missing}", file=sys.stderr)
        return 1
    finally:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()


def send_shell_commands(
    monitor: pathlib.Path, screenshot: pathlib.Path | None
) -> None:
    send_keyboard_lines(
        monitor,
        (
            "help",
            "status",
            "mem",
            "ps",
            "pwd",
            "ls",
            "echo lower_case!?",
            "touch shell.txt",
            "write shell.txt lower_case!?",
            "cat shell.txt",
            "stat shell.txt",
            "rm shell.txt",
            "uptime",
            "exit",
        ),
        mouse=True,
        screenshot=screenshot,
    )


def capture_screenshot(monitor: pathlib.Path, screenshot: pathlib.Path) -> None:
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline and not monitor.exists():
        time.sleep(0.05)
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
        client.connect(str(monitor))
        client.settimeout(1)
        try:
            client.recv(4096)
        except TimeoutError:
            pass
        client.sendall(f"screendump {screenshot}\n".encode())
        time.sleep(0.5)


def click_login(monitor: pathlib.Path) -> None:
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline and not monitor.exists():
        time.sleep(0.05)
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
        client.connect(str(monitor))
        client.settimeout(1)
        try:
            client.recv(4096)
        except TimeoutError:
            pass
        client.sendall(b"mouse_button 1\n")
        time.sleep(0.15)
        client.sendall(b"mouse_button 0\n")
        time.sleep(0.25)


def send_keyboard_lines(
    monitor: pathlib.Path,
    lines: tuple[str, ...],
    mouse: bool = False,
    screenshot: pathlib.Path | None = None,
) -> None:
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline and not monitor.exists():
        time.sleep(0.05)
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
        client.connect(str(monitor))
        client.settimeout(1)
        try:
            client.recv(4096)
        except TimeoutError:
            pass
        if mouse:
            for movement in (b"40 -25", b"-28 18", b"35 -12", b"-7 19"):
                client.sendall(b"mouse_move " + movement + b"\n")
                time.sleep(0.15)
            client.sendall(b"mouse_button 1\n")
            time.sleep(0.2)
            client.sendall(b"mouse_button 0\n")
            time.sleep(0.2)
        if mouse:
            exercise_window_management(client)
        if screenshot is not None:
            client.sendall(f"screendump {screenshot}\n".encode())
            time.sleep(0.5)
        for index, command in enumerate(lines):
            for key in command:
                key_name = qemu_key_name(key)
                client.sendall(f"sendkey {key_name} 40\n".encode())
                time.sleep(0.09)
            client.sendall(b"sendkey ret 40\n")
            time.sleep(0.6)


def qemu_key_name(key: str) -> str:
    names = {
        " ": "spc",
        ".": "dot",
        ",": "comma",
        "-": "minus",
        "_": "shift-minus",
        "!": "shift-1",
        "?": "shift-slash",
        "/": "slash",
        "=": "equal",
        "+": "shift-equal",
        "@": "shift-2",
        ":": "shift-semicolon",
        ";": "semicolon",
    }
    return names.get(key, key)


def move_mouse(client: socket.socket, dx: int, dy: int, step: int = 1) -> None:
    if dx and dy:
        raise ValueError("test mouse moves one axis at a time")
    distance = abs(dx or dy)
    direction = 1 if (dx or dy) > 0 else -1
    if distance % step:
        raise ValueError("mouse distance must be divisible by step")
    for _ in range(distance // step):
        move_x = direction * step if dx else 0
        move_y = direction * step if dy else 0
        client.sendall(f"mouse_move {move_x} {move_y}\n".encode())
        time.sleep(0.008)
    time.sleep(0.1)


def click_mouse(client: socket.socket) -> None:
    client.sendall(b"mouse_button 1\n")
    time.sleep(0.15)
    client.sendall(b"mouse_button 0\n")
    time.sleep(0.2)


def exercise_window_management(client: socket.socket) -> None:
    # Adaptive acceleration maps magnitude 1 -> 1 and magnitude 2 -> 4.
    # Initial burst settles at (760, 400). Reach terminal title bar exactly.
    move_mouse(client, 0, -225)
    client.sendall(b"mouse_button 1\n")
    time.sleep(0.15)
    move_mouse(client, 40, 0)
    move_mouse(client, 0, 30)
    client.sendall(b"mouse_button 0\n")
    time.sleep(0.3)

    # Terminal taskbar button: minimize, then restore. Unit deltas bypass
    # acceleration so hit coordinates remain exact under emulation.
    move_mouse(client, -350, 0)
    move_mouse(client, 0, 565)
    click_mouse(client)
    click_mouse(client)

    # Close System Monitor through title-bar X.
    move_mouse(client, -130, 0)
    move_mouse(client, 0, -675)
    click_mouse(client)

    # Return to moved terminal content before keyboard commands.
    move_mouse(client, 180, 0)
    move_mouse(client, 0, 205)
    click_mouse(client)

    # Reopen closed System Monitor through Start menu.
    move_mouse(client, -440, 0)
    move_mouse(client, 0, 470)
    click_mouse(client)
    move_mouse(client, 70, 0)
    move_mouse(client, 0, -94)
    click_mouse(client)

    # Close Terminal, then reopen it through Start menu too.
    move_mouse(client, 320, 0)
    move_mouse(client, 0, 94)
    click_mouse(client)
    move_mouse(client, 578, 0)
    move_mouse(client, 0, -565)
    click_mouse(client)
    move_mouse(client, -968, 0)
    move_mouse(client, 0, 565)
    click_mouse(client)
    move_mouse(client, 70, 0)
    move_mouse(client, 0, -54)
    click_mouse(client)


if __name__ == "__main__":
    raise SystemExit(main())
