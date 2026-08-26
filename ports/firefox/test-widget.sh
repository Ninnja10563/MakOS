#!/bin/sh
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. https://mozilla.org/MPL/2.0/
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
source_dir="$repo_dir/build/ports/firefox/source"
out_dir="$repo_dir/build/ports/firefox/widget-probe"

test -d "$source_dir/.git" || {
    echo "Firefox source missing; run ports/firefox/clone.sh" >&2
    exit 1
}
"$port_dir/apply-patches.sh" >/dev/null

test "$("$source_dir/build/autoconf/config.sub" aarch64-unknown-makos)" = \
    aarch64-unknown-makos
grep -Fq '"MakOS",' \
    "$source_dir/python/mozbuild/mozbuild/configure/constants.py"
grep -Fq '"MakOS": "__makos__"' \
    "$source_dir/python/mozbuild/mozbuild/configure/constants.py"
grep -Fq 'return ("cairo-makos",)' "$source_dir/toolkit/moz.configure"
grep -Fq 'elif toolkit == "makos":' "$source_dir/widget/moz.build"
mkdir -p "$out_dir"
PYTHONPYCACHEPREFIX="$out_dir/pycache" python3 -m py_compile \
    "$source_dir/python/mozbuild/mozbuild/configure/constants.py" \
    "$source_dir/toolkit/moz.configure" \
    "$source_dir/widget/moz.build" \
    "$source_dir/widget/makos/moz.build"

clang++ --target=aarch64-unknown-makos -D__makos__ -DMOZ_WIDGET_MAKOS \
    -std=c++17 -ffreestanding -fno-exceptions -fno-rtti \
    -fno-stack-protector -I"$source_dir/widget/makos" \
    -c "$source_dir/widget/makos/MakOSSurface.cpp" \
    -o "$out_dir/MakOSSurface.o"
file "$out_dir/MakOSSurface.o" | grep -q 'ELF 64-bit.*ARM aarch64'

# Native input wake is a blocking watcher transport, not a polling loop.
# Keep syscall number/order/backpressure/teardown semantics structurally tied
# across the kernel ABI and Gecko widget bridge.
grep -Fq 'const SYS_SURFACE_WAIT_EVENT: u64 = 140;' \
    "$repo_dir/kernel/src/arch/aarch64.rs"
grep -Fq 'const SYS_SURFACE_MAIN_HANDOFF_READY: u64 = 149;' \
    "$repo_dir/kernel/src/arch/aarch64.rs"
grep -Fq 'SYS_SURFACE_WAIT_EVENT => {' \
    "$repo_dir/kernel/src/arch/aarch64.rs"
grep -Fq 'block_current_for_input(frame)' \
    "$repo_dir/kernel/src/arch/aarch64.rs"
grep -Fq 'wake_input_waiters();' "$repo_dir/kernel/src/graphics.rs"
grep -Fq 'SurfaceWaitEvent = 140' \
    "$source_dir/widget/makos/MakOSSurface.cpp"
grep -Fq 'makos::WaitSurfaceEvent(surface, event)' \
    "$source_dir/widget/makos/MakOSWindow.cpp"
grep -Fq 'makos::NotifySurfaceMainHandoffReady()' \
    "$source_dir/widget/makos/MakOSWindow.cpp"
grep -Fq 'mainRunnableReady' \
    "$source_dir/widget/makos/MakOSWindow.cpp"
grep -Fq 'MAKOS_WIDGET_MAIN_HANDOFF_' \
    "$source_dir/widget/makos/MakOSWindow.cpp"
grep -Fq 'kEventQueueCapacity = 256' \
    "$source_dir/widget/makos/MakOSWindow.cpp"
grep -Fq 'pthread_cond_wait(&mEventQueueNotFull' \
    "$source_dir/widget/makos/MakOSWindow.cpp"
grep -Fq 'NS_DispatchToMainThread(NS_NewRunnableFunction' \
    "$source_dir/widget/makos/MakOSWindow.cpp"
grep -Fq 'pthread_join(mEventWatcher' \
    "$source_dir/widget/makos/MakOSWindow.cpp"
git -C "$source_dir" diff --check

echo "MAKOS_FIREFOX_WIDGET_ABI_OK toolkit=makos arch=aarch64 svc=surface,event,yield,main-handoff nsIWidget=blocked input_wake=blocking-watcher queue=ordered,bounded main_dispatch=gecko-runnable,post-enqueue-ack teardown=wake,join"
