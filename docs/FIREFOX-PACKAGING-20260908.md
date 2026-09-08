# Firefox MakOS stage-package repair — 2026-09-08

## Qualification received

The user reports testing `6de93f55c121737e1b23168dec543dc68bc872a5` on Apple
M3/macOS 26.6.2 with QEMU 11.0.3/HVF. HEAD, origin/main, and remote main
matched; the worktree stayed clean, no concurrent QEMU ran, and no test,
timeout, assertion, or threshold changed. These Mac logs are user-reported,
not available for local inspection on the Raspberry Pi.

The report passes `make unit check`, CPython fetch/test/build/package,
self-host runtime twice, Native/Python SMP runtime twice, Firefox-role SMP,
cursor, Firefox binary audit, and Firefox build-provenance verification.
This qualifies the four repairs documented in
[the September 6 report](HVF-BLOCKERS-20260906.md) on that Mac:

- Self-host: 20 CLI builds, 21 processes, graph sequence `4,3,2,2,3,2,2,3`,
  three parallel status-42 children; locked snapshot PIDs `14,13,15` on CPUs
  `1,2,3` with distinct groups, TTBR0 roots, and address spaces.
- Exact generated-header readback: inline 164 bytes/FNV-1a
  `bbbc9068d3d73e49`; leaf 1,215 bytes/FNV-1a `ccf73fc02c2f9ceb`.
- Native/Python/Firefox-role migration evidence drops: zero. Cursor: seven
  positions, zero changed scanout pixels, GPU timeouts/errors zero.

Integration failed at the supported command:

```sh
make integrated-data-aarch64 SOURCE_DATA_IMAGE=build/makos-data-aarch64.img
```

It exited 2 with `Mozilla stage-package artifact absent: .../dist/firefox/plugin-container`.
Both process executables existed and passed provenance in `dist/bin`, but the
upstream manifest omitted them for the MakOS target. No manual copying was
performed. No fresh integrated image, strict Firefox runtime, or visible login
was qualified; existing outputs, accounts, profiles, images, and logs remain
preserved.

## Repair and release boundary

Commit `f409f4925ed4fe4e1dd319cd7f33e7d0d0067d0f` appends
`ports/firefox/patches/0060-makos-stage-process-artifacts.patch`. Its only
upstream change is four manifest lines under `[xpcom]`, adding
`@BINPATH@/@MOZ_CHILD_PROCESS_NAME@` and `@BINPATH@/xpcshell` under `XP_MAKOS`.
The existing distinct MakOS configure define reaches Mozilla's packager through
`ACDEFINES`. This preserves the flat Unix layout without selecting the macOS
bundle or Windows packaging branches. Linux, macOS, and Windows manifest
outputs are unchanged.

There is no host-side copy workaround. `package-makos.sh` still requires all
five staged executables/libraries, directly compares independently stripped
staged bytes with the stamp-authorized private snapshots, performs the actual
package/image/ELF preflights, and publishes the image last. No kernel, runtime
harness, deadline, or performance threshold changes in this increment.

The source remains Firefox 140.13.0esr at
`90ad18aabeaa9cbd63a1f749a57f266e758e50da`. The new ordered **60-patch** series
SHA-256 is
`4f6a84b2ec7c198b5e15b0273fe6931286c2836404ec231056e951d76d46d8fe`.
All 59 earlier patches retain their bytes and order. A new negative regression
requires rejection of the previously valid 59-patch release identity; a
packaging-only source change does not exempt the provenance boundary.

Run the supported release wrapper again with developer mode unset. It may
reuse the existing build cache, but must apply the appended patch, complete
the release build and audit, and write current provenance itself. Do not
manually restamp old outputs or copy the two files into `dist/firefox`.

## Local verification

Raspberry Pi Debian/AArch64, Python 3.13.5, repository-staged LLVM 19;
no QEMU was launched for this manifest-only repair. The following passed:

```sh
python3 ports/firefox/test-package-manifest.py --source-dir build/ports/firefox/source
ports/firefox/test-package-coherence.sh
make unit check
```

The new test reads the full manifest from the pinned upstream Git commit and
patches a temporary copy only. Using the locally available Mozilla Python
modules, it exercises configure's `ACDEFINES` serialization and full-manifest
preprocessing, then stages the complete `[xpcom]` component through Mozilla's
flat and omni packagers with byte fixtures. Both executables retain exact
fixture bytes and mode 0755; either missing input fails; the original manifest
actually omits both; other platforms' preprocessed outputs are unchanged.
This is real Mozilla component-staging regression evidence, **not** a complete
Gecko release package or guest executable test. Without a source checkout the
default test explicitly reports structural-only coverage; `--source-dir`
requires Mozilla staging and fails if unavailable.

The existing adversarial package-coherence test also passes: five authorized
snapshots, byte comparisons, stale/developer/symlink/alias rejection, stage
mutation rejection, isolated tree/image mutation, actual image verification,
image-atomic failure, recoverable auxiliary publication, and signal cleanup.
Full unit/check passes both architectures with existing compiler warnings.

Local log identities (exit 0 for each):

| Log under `build/logs/` | Bytes | SHA-256 |
| --- | ---: | --- |
| `firefox-package-manifest-20260908.log` | 245 | `851001932cb393fc99f691151e8f72b5b3634b94d3961c0f2a6188576b8c3539` |
| `firefox-package-coherence-20260908.log` | 420 | `40e7f62836b737bd65fbc00c4c9bb295c208d1043690863bd9dc27bccac204a2` |
| `full-unit-check-firefox-package-manifest-20260908.log` | 83,851 | `459138f6bfa48e4fb7ebc2a2d4296b1ed6630091b6653d4cdf4aa50b5ec99808` |

The shared cached Firefox manifest was not modified. This Pi has no canonical
release `dist/bin`, no staged nano/ncurses integration prerequisites, and no
matching host Python 3.14 for a new CPython build. Consequently no fresh full
release build, integration, strict Firefox runtime, or visible login was
attempted locally. Preserved developer outputs cannot substitute for these.

## Required Mac retest

Use the exact final handoff commit from chat and the self-contained protocol
in [the test-agent prompt](MACOS-HVF-TEST-AGENT-PROMPT.md). Refresh the supported
release build/provenance, rerun integration, then run the unchanged strict
Firefox target against the newly published image on an idle, low-pressure
Mac. Stop on any failed prerequisite; never fall back to an old image. After
Firefox passes and its QEMU exits, qualify visible login from private boot/data
clones as the sole QEMU and record PID/session/data/QMP and capture identities.

Firefox, scheduler breadth, self-hosting, and other affected original-spec
rows remain Partial. This repair is ready for target qualification, not a
claim of a successful current browser runtime or full OS completion.
