#!/bin/sh
# Offline adversarial tests for release build-to-package byte derivation.
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
real_python=$(command -v python3)
real_mv=$(command -v mv)
tmp=$(mktemp -d "${TMPDIR:-/tmp}/makos-firefox-package-test.XXXXXX")
trap 'rm -rf "$tmp"' EXIT HUP INT TERM
fixture=$tmp/repo
tools=$fixture/tools
release_obj=$fixture/build/ports/firefox/obj-aarch64-makos
bin=$release_obj/dist/bin
dist=$release_obj/dist/firefox
stamp=$release_obj/makos-build-provenance.json
stripped=$fixture/output/libxul.so
image=$fixture/output/makos.img
expected=$fixture/expected
artifacts='firefox plugin-container xpcshell libxul.so libnspr4.so'

mkdir -p "$fixture/ports/firefox" "$fixture/ports/cpython" \
    "$fixture/scripts" "$tools" "$bin" "$dist" \
    "$fixture/build/ports/firefox/source/layout/reftests/fonts/mplus" \
    "$fixture/output" "$fixture/tmp" "$expected"
cp "$port_dir/package-makos.sh" "$fixture/ports/firefox/package-makos.sh"
chmod +x "$fixture/ports/firefox/package-makos.sh"
cp "$repo_dir/scripts/mkpackage.py" "$fixture/scripts/mkpackage.py"
cp "$repo_dir/scripts/verify_package.py" "$fixture/scripts/verify_package.py"
cp "$repo_dir/scripts/package_layout.py" "$fixture/scripts/package_layout.py"

cat > "$fixture/ports/cpython/stage-makos.sh" <<'EOF'
#!/bin/sh
set -eu
exit 0
EOF
chmod +x "$fixture/ports/cpython/stage-makos.sh"

cat > "$tools/llvm-strip" <<'EOF'
#!/bin/sh
set -eu
test "$1" = --strip-sections
printf '%s' '|deterministically-stripped' >> "$2"
EOF
chmod +x "$tools/llvm-strip"

cat > "$tools/make" <<'EOF'
#!/bin/sh
set -eu
root=$TEST_FIXTURE_ROOT
bin=$root/build/ports/firefox/obj-aarch64-makos/dist/bin
dist=$root/build/ports/firefox/obj-aarch64-makos/dist/firefox
artifacts='firefox plugin-container xpcshell libxul.so libnspr4.so'
rm -rf "$dist"
mkdir -p "$dist"
for artifact in $artifacts; do
    case "${TEST_STAGE_MODE:-coherent}:$artifact" in
        unrelated:*|stale-developer:libxul.so)
            printf 'unrelated-self-consistent-runtime:%s\n' "$artifact" > "$dist/$artifact"
            ;;
        *)
            cp "$bin/$artifact" "$dist/$artifact"
            ;;
    esac
done
chmod +x "$dist/firefox"
printf 'omni\n' > "$dist/omni.ja"
printf 'application\n' > "$dist/application.ini"
printf '{"runtime":"previous-complete"}\n' > "$dist/makos-build-provenance.json"
if test "${TEST_STAGE_MODE:-coherent}" = stage-symlink; then
    ln -s "$bin/firefox" "$dist/host-fallback"
fi
if test "${TEST_STAGE_MODE:-coherent}" = mutate-bin; then
    printf 'concurrent mutation\n' >> "$bin/xpcshell"
fi
if test "${TEST_STAGE_MODE:-coherent}" = signal; then
    kill -TERM "$PPID"
fi
EOF
chmod +x "$tools/make"

cat > "$tools/mv" <<'EOF'
#!/bin/sh
set -eu
source=$1
destination=$2
mode=${TEST_STAGE_MODE:-coherent}
case "$mode:$destination" in
    publish-stop-*:$TEST_FIXTURE_ROOT/build/ports/firefox/obj-aarch64-makos/dist/firefox/*|\
    publish-stop-*:$TEST_FIXTURE_ROOT/output/libxul.so)
        count=0
        if test -f "$TEST_FIXTURE_ROOT/publish-count"; then
            count=$(cat "$TEST_FIXTURE_ROOT/publish-count")
        fi
        count=$((count + 1))
        printf '%s\n' "$count" > "$TEST_FIXTURE_ROOT/publish-count"
        "$TEST_REAL_MV" "$source" "$destination"
        wanted=${mode#publish-stop-}
        if test "$count" -eq "$wanted"; then
            echo "injected auxiliary publish failure after move $count" >&2
            exit 97
        fi
        ;;
    *)
        exec "$TEST_REAL_MV" "$@"
        ;;
esac
EOF
chmod +x "$tools/mv"

# This mock validates stamped BIN hashes but intentionally accepts any runtime
# record. Thus the unrelated-runtime case proves that direct byte comparison,
# not two independently self-hashed sets, is the rejecting gate.
cat > "$tools/python3" <<'EOF'
#!/bin/sh
set -eu
script=$1
shift
case "$script" in
    -c)
        exec "$TEST_REAL_PYTHON" -c "$@"
        ;;
    */firefox_provenance.py)
        command=$1
        shift
        bin_dir=
        stamp=
        output=
        while test "$#" -gt 0; do
            case "$1" in
                --bin-dir) bin_dir=$2; shift 2 ;;
                --stamp) stamp=$2; shift 2 ;;
                --output) output=$2; shift 2 ;;
                *) shift ;;
            esac
        done
        case "$command" in
            verify-build-stamp)
                for artifact in firefox plugin-container xpcshell libxul.so libnspr4.so; do
                    digest=$(cksum < "$bin_dir/$artifact")
                    grep -Fxq "$artifact=$digest" "$stamp" || {
                        echo "mock provenance: build artifacts differ from audited build stamp" >&2
                        exit 1
                    }
                done
                case "$bin_dir" in
                    */makos-firefox-package.*/stamped-bin)
                        mode=$(
                            "$TEST_REAL_PYTHON" -c \
                                'import os, stat, sys; print(stat.S_IMODE(os.stat(sys.argv[1]).st_mode))' \
                                "$(dirname "$bin_dir")"
                        )
                        test "$mode" -eq 448 || {
                            echo "mock provenance: snapshot directory is not mode 0700" >&2
                            exit 1
                        }
                        printf 'private-mode:0700\n' >> "$TEST_FIXTURE_ROOT/verify.log"
                        ;;
                esac
                printf 'verify:%s\n' "$bin_dir" >> "$TEST_FIXTURE_ROOT/verify.log"
                if test "${TEST_STAGE_MODE:-coherent}" = final-stamp-fail && \
                   test "$(grep -Fc 'verify:' "$TEST_FIXTURE_ROOT/verify.log")" -eq 4; then
                    echo "injected final post-publication stamp failure" >&2
                    exit 1
                fi
                ;;
            create-runtime-record)
                printf '{"runtime":"mock-self-consistent"}\n' > "$output"
                if test "${TEST_STAGE_MODE:-coherent}" = mutate-dist-after-compare; then
                    printf 'late mutable DIST bytes\n' > \
                        "$TEST_FIXTURE_ROOT/build/ports/firefox/obj-aarch64-makos/dist/firefox/firefox"
                fi
                ;;
            *) exit 2 ;;
        esac
        ;;
    */mkpackage.py)
        image=$1
        package_root=$2
        release_dist=$TEST_FIXTURE_ROOT/build/ports/firefox/obj-aarch64-makos/dist/firefox
        test "$package_root" != "$release_dist"
        case "$package_root" in
            "$TEST_FIXTURE_ROOT/tmp"/makos-firefox-package.*/package-root) ;;
            *)
                echo "mock package check: package root is not invocation-private" >&2
                exit 1
                ;;
        esac
        printf '%s\n' "$package_root" > "$TEST_FIXTURE_ROOT/package-root.log"
        "$TEST_REAL_PYTHON" "$script" "$@"
        if test "${TEST_STAGE_MODE:-coherent}" = mutate-during-image; then
            printf 'image-time mutation\n' >> \
                "$TEST_FIXTURE_ROOT/build/ports/firefox/obj-aarch64-makos/dist/bin/plugin-container"
        fi
        ;;
    */verify_package.py)
        "$TEST_REAL_PYTHON" "$script" "$@"
        if test "${TEST_STAGE_MODE:-coherent}" = verify-package-fail; then
            echo "injected verify-package failure" >&2
            exit 1
        fi
        ;;
    */verify_firefox_runtime_image.py)
        if test "${TEST_STAGE_MODE:-coherent}" = preflight-fail; then
            echo "injected Firefox preflight failure" >&2
            exit 1
        fi
        exec "$TEST_REAL_PYTHON" "$TEST_FIXTURE_ROOT/check-image.py" \
            "$1" "$TEST_FIXTURE_ROOT/expected"
        ;;
    *)
        echo "unexpected mock python invocation: $script" >&2
        exit 2
        ;;
esac
EOF
chmod +x "$tools/python3"

cat > "$fixture/check-image.py" <<'PY'
#!/usr/bin/env python3
import pathlib
import struct
import sys

fixture = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(fixture / "scripts"))
import mkpackage

image = pathlib.Path(sys.argv[1])
expected = pathlib.Path(sys.argv[2])
wanted = {f"/usr/lib/firefox/{name}": expected / name for name in (
    "firefox", "plugin-container", "xpcshell", "libxul.so", "libnspr4.so"
)}
observed = {}
with image.open("rb") as source:
    source.seek(mkpackage.HEADER_LBA * mkpackage.SECTOR)
    header = source.read(mkpackage.SECTOR)
    assert header[:8] == mkpackage.HEADER_MAGIC
    count = struct.unpack_from("<I", header, 12)[0]
    for index in range(count):
        source.seek((mkpackage.ENTRY_LBA + index) * mkpackage.SECTOR)
        entry = source.read(mkpackage.SECTOR)
        path_length = struct.unpack_from("<H", entry, 8)[0]
        size, first_lba = struct.unpack_from("<QQ", entry, 16)
        path = entry[64:64 + path_length].decode()
        source.seek(first_lba * mkpackage.SECTOR)
        observed[path] = source.read(size)
for guest, path in wanted.items():
    assert observed[guest] == path.read_bytes(), guest
assert b'"runtime":"mock-self-consistent"' in observed[
    "/usr/lib/firefox/makos-build-provenance.json"
]
print("MAKOS_FIREFOX_TEST_IMAGE_PREFLIGHT_OK payloads=5 provenance=present")
PY

printf 'font\n' > "$fixture/build/ports/firefox/source/layout/reftests/fonts/mplus/mplus-1p-regular.ttf"
printf 'font license\n' > "$fixture/build/ports/firefox/source/layout/reftests/fonts/mplus/mplus-license.txt"
printf 'firefox license\n' > "$fixture/build/ports/firefox/source/LICENSE"
mkdir -p "$fixture/build/ports/firefox/source/toolkit/content"
printf 'firefox license html\n' > "$fixture/build/ports/firefox/source/toolkit/content/license.html"
printf 'pref("fixture", true);\n' > "$fixture/ports/firefox/makos-prefs.js"

reset_release() {
    rm -rf "$bin" "$dist" "$expected"
    mkdir -p "$bin" "$dist" "$expected"
    for artifact in $artifacts; do
        printf 'audited-release-bin:%s\n' "$artifact" > "$bin/$artifact"
    done
    chmod +x "$bin/firefox"
    : > "$stamp"
    for artifact in $artifacts; do
        digest=$(cksum < "$bin/$artifact")
        printf '%s=%s\n' "$artifact" "$digest" >> "$stamp"
        cp "$bin/$artifact" "$expected/$artifact"
        "$tools/llvm-strip" --strip-sections "$expected/$artifact"
    done
    printf 'previous-published-libxul\n' > "$stripped"
    printf 'prior-image\n' > "$image"
    rm -f "$fixture/verify.log" "$fixture/package-root.log" \
        "$fixture/publish-count"
    rm -rf "$fixture/tmp"/*
}

run_package_paths() {
    TEST_FIXTURE_ROOT=$fixture \
    TEST_REAL_PYTHON=$real_python \
    TEST_REAL_MV=$real_mv \
    TEST_STAGE_MODE=$1 \
    TMPDIR=$fixture/tmp \
    PATH=$tools:/usr/bin:/bin \
    MAKOS_LLVM_STRIP=$tools/llvm-strip \
    MAKOS_FIREFOX_LIBXUL=$3 \
        "$fixture/ports/firefox/package-makos.sh" "$2"
}

run_package() {
    run_package_paths "$1" "$image" "$stripped"
}

assert_no_package_temps() {
    test "$(find "$fixture/tmp" -mindepth 1 -print -quit)" = ""
    test "$(find "$dist" -maxdepth 1 -type f -name '.*' -print -quit)" = ""
    test "$(find "$fixture/output" -maxdepth 1 -type f \
        \( -name '.makos-firefox-image.*' -o -name 'libxul.so.*' \) \
        -print -quit)" = ""
}

expect_failure() {
    mode=$1
    fragment=$2
    output=$tmp/output-$mode.log
    if run_package "$mode" > "$output" 2>&1; then
        echo "Firefox package coherence test unexpectedly passed: $mode" >&2
        exit 1
    fi
    grep -Fq "$fragment" "$output"
    assert_no_package_temps
    grep -Fxq 'prior-image' "$image"
}

reset_release
run_package coherent > "$tmp/coherent.log"
grep -Fq 'MAKOS_FIREFOX_PACKAGE_OK' "$tmp/coherent.log"
test -f "$image"
test -f "$dist/makos-build-provenance.json"
cmp -s "$expected/libxul.so" "$stripped"
for artifact in firefox plugin-container xpcshell libnspr4.so; do
    cmp -s "$expected/$artifact" "$dist/$artifact"
done
test -x "$dist/firefox"
test "$(grep -Fc 'verify:' "$fixture/verify.log")" -eq 4
grep -Fxq 'private-mode:0700' "$fixture/verify.log"
grep -Fq '/makos-firefox-package.' "$fixture/package-root.log"
assert_no_package_temps

reset_release
expect_failure unrelated \
    'Firefox packaged artifact is not derived from stamped BIN: firefox'
grep -Fxq 'previous-published-libxul' "$stripped"

reset_release
expect_failure stale-developer \
    'Firefox packaged artifact is not derived from stamped BIN: libxul.so'
grep -Fxq 'previous-published-libxul' "$stripped"

reset_release
expect_failure stage-symlink \
    'Mozilla stage-package output contains a symlink'

reset_release
expect_failure mutate-bin \
    'mock provenance: build artifacts differ from audited build stamp'
grep -Fxq 'previous-published-libxul' "$stripped"

reset_release
expect_failure mutate-during-image \
    'mock provenance: build artifacts differ from audited build stamp'
grep -Fxq 'previous-published-libxul' "$stripped"

reset_release
expect_failure verify-package-fail 'injected verify-package failure'
grep -Fxq 'previous-published-libxul' "$stripped"

reset_release
expect_failure preflight-fail 'injected Firefox preflight failure'
grep -Fxq 'previous-published-libxul' "$stripped"

reset_release
run_package mutate-dist-after-compare > "$tmp/dist-mutation.log"
grep -Fq 'MAKOS_FIREFOX_PACKAGE_OK' "$tmp/dist-mutation.log"
for artifact in $artifacts; do
    if test "$artifact" = libxul.so; then
        cmp -s "$expected/$artifact" "$stripped"
    else
        cmp -s "$expected/$artifact" "$dist/$artifact"
    fi
done
assert_no_package_temps

assert_auxiliary_outputs_complete() {
    for artifact in firefox plugin-container xpcshell libnspr4.so; do
        cmp -s "$dist/$artifact" "$bin/$artifact" || \
            cmp -s "$dist/$artifact" "$expected/$artifact"
    done
    grep -Eq '^\{"runtime":"(previous-complete|mock-self-consistent)"\}$' \
        "$dist/makos-build-provenance.json"
    grep -Fxq 'previous-published-libxul' "$stripped" || \
        cmp -s "$stripped" "$expected/libxul.so"
    assert_no_package_temps
}

for stop_after in 1 2 3 4 5 6; do
    reset_release
    publish_log=$tmp/publish-stop-$stop_after.log
    if run_package "publish-stop-$stop_after" > "$publish_log" 2>&1; then
        echo "Firefox auxiliary publication injection unexpectedly passed: $stop_after" >&2
        exit 1
    fi
    grep -Fq "injected auxiliary publish failure after move $stop_after" \
        "$publish_log"
    grep -Fxq 'prior-image' "$image"
    assert_auxiliary_outputs_complete
    rm -f "$fixture/publish-count"
    run_package coherent > "$tmp/publish-recovery-$stop_after.log"
    grep -Fq 'MAKOS_FIREFOX_PACKAGE_OK' "$tmp/publish-recovery-$stop_after.log"
    cmp -s "$expected/libxul.so" "$stripped"
done

reset_release
if run_package final-stamp-fail > "$tmp/final-stamp-fail.log" 2>&1; then
    echo "Firefox final post-publication stamp injection unexpectedly passed" >&2
    exit 1
fi
grep -Fq 'injected final post-publication stamp failure' \
    "$tmp/final-stamp-fail.log"
grep -Fxq 'prior-image' "$image"
assert_auxiliary_outputs_complete
run_package coherent > "$tmp/final-stamp-recovery.log"
grep -Fq 'MAKOS_FIREFOX_PACKAGE_OK' "$tmp/final-stamp-recovery.log"
cmp -s "$expected/libxul.so" "$stripped"

expect_symlink_rejection() {
    target=$1
    backup=$target.real
    mv "$target" "$backup"
    ln -s "$(basename "$backup")" "$target"
    expect_failure coherent 'rejects noncanonical path'
    rm "$target"
    mv "$backup" "$target"
}

reset_release
expect_symlink_rejection "$release_obj"
expect_symlink_rejection "$dist"
expect_symlink_rejection "$bin"
expect_symlink_rejection "$stamp"
mv "$bin/firefox" "$bin/firefox.real"
ln -s firefox.real "$bin/firefox"
expect_failure coherent 'release BIN artifact is absent or symlinked: firefox'
rm "$bin/firefox"
mv "$bin/firefox.real" "$bin/firefox"

protected_digest() {
    cksum "$stamp" "$bin/firefox" "$bin/plugin-container" "$bin/xpcshell" \
        "$bin/libxul.so" "$bin/libnspr4.so" \
        "$fixture/build/ports/firefox/source/LICENSE"
}

expect_output_rejection() {
    image_path=$1
    stripped_path=$2
    fragment=$3
    output=$tmp/output-alias.log
    before=$(protected_digest)
    if run_package_paths coherent "$image_path" "$stripped_path" \
        > "$output" 2>&1; then
        echo "Firefox package accepted aliased output: image=$image_path stripped=$stripped_path" >&2
        exit 1
    fi
    grep -Fq "$fragment" "$output"
    test "$before" = "$(protected_digest)"
    grep -Fxq 'prior-image' "$image"
    grep -Fxq 'previous-published-libxul' "$stripped"
    assert_no_package_temps
}

expect_output_rejection "$bin/firefox" "$stripped" \
    'IMAGE destination aliases protected Firefox tree'
expect_output_rejection "$stamp" "$stripped" \
    'IMAGE destination aliases protected Firefox tree'
expect_output_rejection "$image" "$image" \
    'IMAGE destination aliases protected input'
expect_output_rejection "$image" "$bin/firefox" \
    'STRIPPED destination aliases protected Firefox tree'
expect_output_rejection "$image" "$stamp" \
    'STRIPPED destination aliases protected Firefox tree'
expect_output_rejection "$image" "$dist" \
    'stripped libxul destination is symlinked or non-regular'
expect_output_rejection "$bin/../bin/firefox" "$stripped" \
    'IMAGE destination aliases protected Firefox tree'
ln -s "$bin" "$fixture/output/bin-alias"
expect_output_rejection "$fixture/output/bin-alias/firefox" "$stripped" \
    'IMAGE destination aliases protected Firefox tree'
rm "$fixture/output/bin-alias"
printf 'safe output target\n' > "$fixture/output/safe-target"
ln -s safe-target "$fixture/output/stripped-link"
expect_output_rejection "$image" "$fixture/output/stripped-link" \
    'stripped libxul destination is symlinked or non-regular'
rm "$fixture/output/stripped-link"
mkdir "$fixture/output/stripped-directory"
expect_output_rejection "$image" "$fixture/output/stripped-directory" \
    'stripped libxul destination is symlinked or non-regular'
rmdir "$fixture/output/stripped-directory"
expect_output_rejection \
    "$fixture/build/ports/firefox/source/layout/reftests/fonts/mplus/mplus-1p-regular.ttf" \
    "$stripped" 'IMAGE destination aliases protected Firefox tree'

reset_release
signal_log=$tmp/signal.log
if run_package signal > "$signal_log" 2>&1; then
    echo "Firefox package coherence signal test unexpectedly passed" >&2
    exit 1
fi
assert_no_package_temps
grep -Fxq 'prior-image' "$image"
grep -Fxq 'previous-published-libxul' "$stripped"

reset_release
redirected=$tmp/redirected.log
for override in \
    "MAKOS_FIREFOX_OBJ=$fixture/developer-obj" \
    "MAKOS_FIREFOX_DIST=$fixture/developer-dist" \
    "MAKOS_FIREFOX_BIN_DIR=$fixture/developer-bin" \
    "MAKOS_FIREFOX_BUILD_PROVENANCE=$fixture/developer-stamp.json"
do
    if env TEST_FIXTURE_ROOT=$fixture TEST_REAL_PYTHON=$real_python \
        PATH=$tools:/usr/bin:/bin "$override" \
        "$fixture/ports/firefox/package-makos.sh" "$image" > "$redirected" 2>&1; then
        echo "Firefox release package accepted redirected input: $override" >&2
        exit 1
    fi
    grep -Fq 'rejects developer or redirected build inputs' "$redirected"
    grep -Fxq 'prior-image' "$image"
done

developer=$tmp/developer.log
if TEST_FIXTURE_ROOT=$fixture TEST_REAL_PYTHON=$real_python PATH=$tools:/usr/bin:/bin \
    MAKOS_FIREFOX_DEVELOPER_BUILD=1 \
    "$fixture/ports/firefox/package-makos.sh" "$image" > "$developer" 2>&1; then
    echo "Firefox release package accepted developer mode" >&2
    exit 1
fi
grep -Fq 'rejects developer or redirected build inputs' "$developer"
grep -Fxq 'prior-image' "$image"

echo "MAKOS_FIREFOX_PACKAGE_COHERENCE_TEST_OK stamped_snapshot=5 private_root=0700 direct_bytes=5 actual_image=verified unrelated_runtime=denied stale_developer=denied symlinks=denied output_aliases=denied stage_mutation=denied dist_mutation=isolated image_mutation=denied verify_failures=image-atomic auxiliary_publication=old-or-candidate,recoverable redirected_inputs=denied signal_cleanup=passed prior_image=authoritative"
