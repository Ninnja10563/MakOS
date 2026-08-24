#!/bin/sh
set -eu

SYSTEM_IMAGE=${1:-build/makos-aarch64-gpt.img}
QEMU=${QEMU_SYSTEM_AARCH64:-qemu-system-aarch64}
BUILD_DIR=${MAKOS_AARCH64_BUILD_DIR:-build}
QMP_SOCKET=${MAKOS_AARCH64_QMP_SOCKET:-$BUILD_DIR/makos-live-gpt-qmp.sock}

find_file() {
    for candidate in "$@"; do
        if [ -n "$candidate" ] && [ -f "$candidate" ]; then
            printf '%s\n' "$candidate"
            return 0
        fi
    done
    return 1
}

CODE=$(find_file "${AAVMF_CODE:-}" \
    /opt/homebrew/share/qemu/edk2-aarch64-code.fd \
    /usr/local/share/qemu/edk2-aarch64-code.fd \
    /usr/share/AAVMF/AAVMF_CODE.fd) || {
    echo "AArch64 UEFI firmware not found. Set AAVMF_CODE." >&2
    exit 1
}
VARS_TEMPLATE=$(find_file "${AAVMF_VARS:-}" \
    /opt/homebrew/share/qemu/edk2-arm-vars.fd \
    /usr/local/share/qemu/edk2-arm-vars.fd \
    /usr/share/AAVMF/AAVMF_VARS.fd) || {
    echo "AArch64 UEFI variable template not found. Set AAVMF_VARS." >&2
    exit 1
}
test -f "$SYSTEM_IMAGE" || {
    echo "GPT system image absent: $SYSTEM_IMAGE" >&2
    exit 1
}
mkdir -p "$BUILD_DIR"
rm -f -- "$QMP_SOCKET"
VARS="$BUILD_DIR/edk2-arm-vars-makos-gpt.fd"
cp "$VARS_TEMPLATE" "$VARS"

exec "$QEMU" \
    -machine virt,accel=hvf,highmem=off,gic-version=2 \
    -cpu host \
    -global virtio-mmio.force-legacy=false \
    -smp 4 \
    -m 1G \
    -drive if=pflash,format=raw,readonly=on,file="$CODE" \
    -drive if=pflash,format=raw,file="$VARS" \
    -drive id=system,if=none,format=raw,file="$SYSTEM_IMAGE" \
    -device virtio-blk-device,drive=system,bootindex=0 \
    -device virtio-keyboard-device \
    -device virtio-tablet-device \
    -netdev user,id=makosnet \
    -device virtio-net-device,netdev=makosnet,mac=52:54:00:12:34:56 \
    -device virtio-gpu-device,xres=800,yres=600 \
    -object rng-random,id=makosrng,filename=/dev/urandom \
    -device virtio-rng-device,rng=makosrng \
    -display cocoa,zoom-to-fit=on,show-cursor=off \
    -name "MakOS GPT" \
    -serial stdio \
    -monitor none \
    -qmp unix:"$QMP_SOCKET",server=on,wait=off \
    -no-reboot \
    -no-shutdown
