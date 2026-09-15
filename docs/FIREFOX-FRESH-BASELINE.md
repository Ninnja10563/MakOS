# Authorized fresh Firefox test baseline

The user reports the qualified integrated image is missing on the Mac; only
its manifest remains, and preserved account/private data images are absent.
The user explicitly authorizes a fresh test image/account/profile if no backup
can be recovered. This is a new test baseline, not restoration of old data.
The manifest contains hashes and metadata, not disk contents. Generated
`build/` artifacts are not stored in Git.

This procedure is for the Apple Silicon macOS/QEMU/HVF testing agent. The
Pi workspace cannot recover or create files on that Mac. Preserve the old
manifest, logs, release build outputs, provenance and any surviving images.
Do not delete caches or overwrite a canonical data image to start fresh.

## Recovery and prerequisites

First record the result of read-only checks of known image/session locations
and available backups. Do not run invasive recovery tools or modify backups.
If the original integrated image is recovered, require its recorded SHA-256
`c23395ff4644b183991f2508bdd475ad2120110019f134ebd2b5af0c550a12dc`
and the unchanged provenance preflight, then use the normal reuse path.
If a recovered candidate fails verification, preserve it and stop; do not
quietly replace a failed candidate with a fresh disk. If no usable original
image or account-data backup is available, the authorized fresh path follows.

Follow [the Mac qualification protocol](MACOS-HVF-TEST-AGENT-PROMPT.md) at the
exact pushed commit supplied in chat. The early unit/image/EL0/self-host/SMP/
cursor gates and their order are unchanged. Stop at the first failed command.
Never run concurrent QEMU; do not build while a runtime is active.

At its integration step, use the documented supported release build, with
developer mode unset, preserving the existing source/sysroot/object caches.
Only `ports/firefox/build-makos.sh` may generate release-build provenance.
The supported packager must authorize all five binaries from that stamp and
stage `plugin-container` and `xpcshell` itself. No manual copying, restamping,
synthetic executables or preflight exemptions are permitted. The integration
path also requires real GNU nano/ncurses and CPython 3.14.7; use the documented
host tools and build/staging wrappers. Missing prerequisites are blockers,
not permission to publish a reduced package set.

Preserve the previous package auxiliary outputs/provenance before supported
packaging refreshes them; a private image output directory does not redirect
those canonical build/package artifacts. Keep complete build/packaging logs.

## Create a new source disk without replacing anything

Run from the repository root, after the preceding gates and release build
pass. These commands must share one shell so their variables are retained:

```sh
set -eu
FRESH_DATA_DIR=$(mktemp -d "$PWD/build/makos-fresh-firefox-XXXXXX")
SOURCE_DATA_IMAGE="$FRESH_DATA_DIR/blank-source.img"
test ! -e "$SOURCE_DATA_IMAGE"
test ! -L "$SOURCE_DATA_IMAGE"
python3 scripts/mkdata.py "$SOURCE_DATA_IMAGE"
shasum -a 256 "$SOURCE_DATA_IMAGE" > "$FRESH_DATA_DIR/source-before.sha256"
make integrated-data-aarch64 \
  SOURCE_DATA_IMAGE="$SOURCE_DATA_IMAGE" \
  INTEGRATED_OUTPUT_DIR="$FRESH_DATA_DIR"
shasum -a 256 -c "$FRESH_DATA_DIR/source-before.sha256"
```

`mkdata.py` creates a sparse, zero-initialized 1 GiB source disk. It can
truncate an existing path, which is why it is used only inside the newly
created private directory with both absence checks. Never point it at an old
image. Keep the source, its hash record and the published image/manifest.

Require `MAKOS_FIREFOX_PACKAGE_OK` and `MAKOS_INTEGRATED_DATA_OK`. Set
`INTEGRATED_IMAGE` to the exact newly published path printed by the latter
marker; do not select an unrelated image by wildcard or rename it to the old
`c23395ff4644b183` filename. Record its full SHA-256, manifest and provenance.
The new image is judged against its own builder-generated identity, not the
missing old image's SHA. This changes test data, not verification semantics.

```sh
test -n "${INTEGRATED_IMAGE:-}"
test -f "$INTEGRATED_IMAGE"
shasum -a 256 "$INTEGRATED_IMAGE"
python3 scripts/verify_firefox_runtime_image.py "$INTEGRATED_IMAGE"
```

Confirm that hash equals the new manifest's `image_sha256` and that the
unchanged pinned-source/60-patch/all-five-ELF runtime preflight passes.

## Fresh guest state and unchanged runtime gates

Do not manually inject account files or Firefox preferences into the disk on
the host; only the supported packager supplies its normal package contents.
The normal guest paths initialize blank MakFS4 state, persist the compiled
default test account through `aarch64_accounts::initialize`, and create
`/home/user/firefox-profile` through `aarch64_process::spawn_firefox`.
Authentication remains required. Record the actual account `created` and
profile `state=created` markers if execution reaches those stages; do not
invent evidence for a fatal that occurs earlier.

The Firefox harness boots a private clone, leaving the integrated master
unchanged. Account/profile changes happen in that clone; normal temporary
cleanup may remove it. This does not prove recovery or cross-run retention
of the old profile. Record any removed paths honestly. After a passing gate,
the final visible-login procedure uses another private clone, which has fresh
state again. Keep that visible session's private data for subsequent testing.

Only on an idle Mac with normal memory pressure and no QEMU running, run:

```sh
AARCH64_FIREFOX_PACKAGE_IMAGE="$INTEGRATED_IMAGE" make test-aarch64-firefox-runtime
```

The 600-second probe, first-character <500 ms, Ctrl-A <10,000 ms, and every
existing assertion/deadline/provenance check remain unchanged. Preserve final
raw serial (including any `MAKOS_FAILURE_DETAIL:` and `MAKOS_FATAL:`), complete
harness output, screenshots, hashes, host state and PID/session/QMP details.
Stop at the first failure without retry or final visible login. Only after
all gates pass follow the normal visible-login instructions and record its
PID, session, private boot/data/vars, QMP socket and screenshot.

Report this as a **fresh-account/profile baseline**. The guest Firefox fatal
is still undiagnosed until new evidence identifies it; this authorization
does not fix it. No new Mac runtime result or full OS completion is claimed.

## Handoff validation

This increment changes documentation only. On Pi/Debian,
`make test-integrated-data` passes with preservation, CRC, ELF and provenance
negative controls intact; the shell examples pass `sh -n`. Log:
`build/logs/fresh-baseline-20260915-integrated-host.log`. No fresh integrated
image/account/profile was created locally, and no QEMU was launched. The Mac
testing agent must perform the authorized build and qualification above.
