#!/bin/sh
set -eu

IMAGE=${1:-build/makos-x86_64.img}
DATA_IMAGE=${2:-build/makos-data.img}
QEMU=${QEMU_SYSTEM_X86_64:-qemu-system-x86_64}

find_ovmf() {
    for candidate in \
        "${OVMF_CODE:-}" \
        /opt/homebrew/share/qemu/edk2-x86_64-code.fd \
        /usr/local/share/qemu/edk2-x86_64-code.fd \
        /usr/share/OVMF/OVMF_CODE.fd \
        /usr/share/edk2/x64/OVMF_CODE.fd
    do
        if [ -n "$candidate" ] && [ -f "$candidate" ]; then
            printf '%s\n' "$candidate"
            return 0
        fi
    done
    return 1
}

OVMF=$(find_ovmf) || {
    echo "OVMF x86_64 firmware not found. Set OVMF_CODE." >&2
    exit 1
}

python3 scripts/ensure_data_size.py "$DATA_IMAGE"

exec "$QEMU" \
    -machine pc,accel=tcg \
    -cpu qemu64 \
    -smp 4 \
    -m 256M \
    -drive if=pflash,format=raw,readonly=on,file="$OVMF" \
    -drive id=boot,if=none,format=raw,file="$IMAGE" \
    -device ide-hd,drive=boot,bus=ide.0,unit=0 \
    -drive id=data,if=none,format=raw,file="$DATA_IMAGE" \
    -device ide-hd,drive=data,bus=ide.1,unit=0 \
    -display cocoa,zoom-to-fit=on \
    -serial stdio \
    -monitor none \
    -netdev user,id=net0 \
    -device rtl8139,netdev=net0 \
    -audiodev driver=coreaudio,id=audio0 \
    -device AC97,audiodev=audio0 \
    -device piix3-usb-uhci,id=usb \
    -device usb-kbd,bus=usb.0 \
    -no-reboot \
    -no-shutdown
