#!/bin/sh
set -eu

IMAGE=${1:-build/makos-x86_64-gpt.img}
TARGET=${2:-}
QEMU=${QEMU_SYSTEM_X86_64:-qemu-system-x86_64}

if [ ! -f "$IMAGE" ]; then
    echo "x86_64 GPT image not found: $IMAGE" >&2
    exit 1
fi
if [ -n "$TARGET" ] && [ ! -f "$TARGET" ]; then
    echo "x86_64 install target not found: $TARGET" >&2
    exit 1
fi

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

if [ -n "$TARGET" ]; then
    set -- \
        -drive id=target,if=none,format=raw,file="$TARGET" \
        -device ide-hd,drive=target,bus=ide.1,unit=1,bootindex=1
else
    set --
fi

exec "$QEMU" \
    -machine pc,accel=tcg \
    -cpu qemu64 \
    -smp 4 \
    -m 256M \
    -drive if=pflash,format=raw,readonly=on,file="$OVMF" \
    -drive id=system,if=none,format=raw,file="$IMAGE" \
    -device ide-hd,drive=system,bus=ide.1,unit=0,bootindex=0 \
    "$@" \
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
